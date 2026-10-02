mod camera;
mod color;
mod cube;
mod framebuffer;
mod light;
mod ray_intersect;
mod sky;
mod texture;

use minifb::{Key, Window, WindowOptions};
use nalgebra_glm::{dot, normalize, Vec3};
use std::f32::consts::PI;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use crate::camera::Camera;
use crate::color::Color;
use crate::cube::Cube;
use crate::framebuffer::Framebuffer;
use crate::light::Light;
use crate::ray_intersect::{
    Intersect, Material, RayIntersect, DIFFUSE, REFLECTIVITY, SPECULAR, TRANSPARENCY,
};
use crate::sky::sample_skybox;
use crate::texture::Texture;

const WIDTH: usize = 800;
const HEIGHT: usize = 600;

const FOV: f32 = 50.0 * PI / 180.0;

const ROTATION_SPEED: f32 = PI / 60.0;
const ZOOM_SPEED: f32 = 0.15;

const SHADOW_BIAS: f32 = 1e-3;
// Usado para separar el origen de un rayo reflejado/refractado de la superficie
// que lo generó, para que no se vuelva a auto-intersectar por error de redondeo.
const BIAS: f32 = 1e-3;

const MAX_DEPTH: u32 = 3;
const PREVIEW_DEPTH: u32 = 1;
const PREVIEW_SCALE: usize = 4;
const FINAL_RENDER_DELAY: Duration = Duration::from_millis(180);

// Luz de cielo rebotada, tenue y con un tinte ligeramente azulado, para que las
// zonas en sombra no queden completamente negras.
const AMBIENT_INTENSITY: f32 = 0.18;

pub fn reflect(incident: &Vec3, normal: &Vec3) -> Vec3 {
    incident - normal * (2.0 * dot(incident, normal))
}

/// Ley de Snell: dado un rayo incidente y la normal de la superficie, calcula la
/// dirección refractada. Devuelve `None` en reflexión interna total (el rayo no
/// puede salir del material más denso, así que toda la luz rebota).
pub fn refract(incident: &Vec3, normal: &Vec3, refractive_index: f32) -> Option<Vec3> {
    let mut cosi = dot(incident, normal).clamp(-1.0, 1.0);
    let mut eta_i = 1.0;
    let mut eta_t = refractive_index;
    let mut n = *normal;

    if cosi < 0.0 {
        // El rayo viene de afuera hacia el material.
        cosi = -cosi;
    } else {
        // El rayo viene de adentro del material hacia afuera: invertimos la
        // normal y los índices de refracción.
        std::mem::swap(&mut eta_i, &mut eta_t);
        n = normal * -1.0;
    }

    let eta = eta_i / eta_t;
    let k = 1.0 - eta * eta * (1.0 - cosi * cosi);

    if k < 0.0 {
        None
    } else {
        Some(incident * eta + n * (eta * cosi - k.sqrt()))
    }
}

/// Aproximación de Schlick al Fresnel: qué tan reflectivo se ve un material
/// transparente según el ángulo de vista. Es lo que hace que el agua se vea casi
/// como espejo en ángulos rasantes, pero transparente al mirarla de frente.
fn fresnel_reflectance(incident: &Vec3, normal: &Vec3, refractive_index: f32) -> f32 {
    let cosi = dot(incident, normal).abs();
    let mut r0 = (1.0 - refractive_index) / (1.0 + refractive_index);
    r0 *= r0;
    r0 + (1.0 - r0) * (1.0 - cosi).powi(5)
}

/// Desplaza el punto de intersección un poco a lo largo de la normal, hacia el
/// lado al que apunta `direction` — mismo lado para reflexión, lado opuesto para
/// refracción, ya que un rayo refractado cruza al otro lado de la superficie.
fn offset_origin(intersect: &Intersect, direction: &Vec3) -> Vec3 {
    if dot(direction, &intersect.normal) < 0.0 {
        intersect.point - intersect.normal * BIAS
    } else {
        intersect.point + intersect.normal * BIAS
    }
}

pub fn cast_shadow(
    intersect: &Intersect,
    light_direction: &Vec3,
    light: &Light,
    objects: &[Box<dyn RayIntersect>],
) -> bool {
    let shadow_ray_origin = intersect.point + intersect.normal * SHADOW_BIAS;
    let light_distance = (light.position - intersect.point).magnitude();

    objects.iter().any(|object| {
        object
            .ray_intersect(&shadow_ray_origin, light_direction)
            .is_some_and(|blocker| blocker.distance < light_distance)
    })
}

pub fn shade(
    intersect: &Intersect,
    ray_origin: &Vec3,
    lights: &[Light],
    objects: &[Box<dyn RayIntersect>],
) -> Color {
    let view_direction = (ray_origin - intersect.point).normalize();
    let diffuse_color = intersect.material.sample_diffuse(intersect.uv);

    // Contribución ambiental: no depende de sombras ni de ninguna luz puntual,
    // así que se calcula una sola vez, afuera del loop de luces.
    let ambient_tint = Color::new(150, 180, 215);
    let mut color =
        diffuse_color * ambient_tint * (AMBIENT_INTENSITY * intersect.material.albedo[DIFFUSE]);

    for light in lights {
        let light_direction = (light.position - intersect.point).normalize();

        let light_intensity = if cast_shadow(intersect, &light_direction, light, objects) {
            0.0
        } else {
            light.intensity
        };

        if light_intensity <= 0.0 {
            continue;
        }

        let diffuse_intensity = dot(&intersect.normal, &light_direction).max(0.0);
        let diffuse = diffuse_color
            * light.color
            * (diffuse_intensity * intersect.material.albedo[DIFFUSE] * light_intensity);

        let reflect_direction = reflect(&-light_direction, &intersect.normal);
        let specular_intensity = dot(&view_direction, &reflect_direction)
            .max(0.0)
            .powf(intersect.material.specular);

        let specular = light.color
            * (specular_intensity * intersect.material.albedo[SPECULAR] * light_intensity);

        color = color + diffuse + specular;
    }

    color
}

pub fn cast_ray(
    ray_origin: &Vec3,
    ray_direction: &Vec3,
    objects: &[Box<dyn RayIntersect>],
    lights: &[Light],
    depth: u32,
    max_depth: u32,
) -> Color {
    // La primera luz es la "key light" (el sol) — es la que orienta el halo del
    // skybox cuando un rayo no golpea nada.
    let sun_position = &lights[0].position;

    if depth >= max_depth {
        return sample_skybox(ray_direction, sun_position);
    }

    let mut closest: Option<Intersect> = None;

    for object in objects {
        if let Some(intersect) = object.ray_intersect(ray_origin, ray_direction) {
            if closest.is_none_or(|current| intersect.distance < current.distance) {
                closest = Some(intersect);
            }
        }
    }

    let Some(intersect) = closest else {
        return sample_skybox(ray_direction, sun_position);
    };

    let local_color = shade(&intersect, ray_origin, lights, objects);

    let reflectivity = intersect.material.albedo[REFLECTIVITY];
    let transparency = intersect.material.albedo[TRANSPARENCY];

    if reflectivity <= 0.0 && transparency <= 0.0 {
        return local_color;
    }

    let fresnel = if transparency > 0.0 {
        fresnel_reflectance(
            ray_direction,
            &intersect.normal,
            intersect.material.refractive_index,
        )
    } else {
        0.0
    };

    // El Fresnel sube la reflectividad efectiva de un material transparente en
    // ángulos rasantes, por encima de su reflectividad "base".
    let effective_reflectivity =
        (reflectivity + (1.0 - reflectivity) * fresnel * transparency).min(1.0);

    let reflected = if effective_reflectivity > 0.0 {
        let reflect_direction = reflect(ray_direction, &intersect.normal).normalize();
        let reflect_origin = offset_origin(&intersect, &reflect_direction);
        cast_ray(
            &reflect_origin,
            &reflect_direction,
            objects,
            lights,
            depth + 1,
            max_depth,
        )
    } else {
        Color::from_hex(0)
    };

    let effective_transparency = transparency * (1.0 - fresnel);

    let refracted = if effective_transparency > 0.0 {
        match refract(
            ray_direction,
            &intersect.normal,
            intersect.material.refractive_index,
        ) {
            Some(refract_direction) => {
                let refract_direction = refract_direction.normalize();
                let refract_origin = offset_origin(&intersect, &refract_direction);
                cast_ray(
                    &refract_origin,
                    &refract_direction,
                    objects,
                    lights,
                    depth + 1,
                    max_depth,
                )
            }
            // Reflexión interna total: no hay rayo refractado, toda la energía
            // que no se manejó como reflexión "base" también rebota.
            None => reflected,
        }
    } else {
        Color::from_hex(0)
    };

    let local_weight = (1.0 - effective_reflectivity - effective_transparency).max(0.0);

    local_color * local_weight
        + reflected * effective_reflectivity
        + refracted * effective_transparency
}

pub fn render(
    framebuffer: &mut Framebuffer,
    objects: &[Box<dyn RayIntersect>],
    camera: &Camera,
    lights: &[Light],
    sample_scale: usize,
    max_depth: u32,
) {
    let framebuffer_width = framebuffer.width;
    let framebuffer_height = framebuffer.height;
    let width = framebuffer_width as f32;
    let height = framebuffer_height as f32;
    let aspect_ratio = width / height;
    let perspective_scale = (FOV / 2.0).tan();
    let scale = sample_scale.max(1);
    let sample_width = framebuffer_width.div_ceil(scale);
    let sample_height = framebuffer_height.div_ceil(scale);
    let mut samples = vec![0u32; sample_width * sample_height];
    let (forward, right, up) = camera.basis();
    let thread_count = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(sample_height);
    let rows_per_thread = sample_height.div_ceil(thread_count);

    thread::scope(|scope| {
        for (chunk_index, rows) in samples
            .chunks_mut(rows_per_thread * sample_width)
            .enumerate()
        {
            let first_y = chunk_index * rows_per_thread;
            scope.spawn(move || {
                for (local_y, row) in rows.chunks_mut(sample_width).enumerate() {
                    let y = first_y + local_y;
                    let pixel_y = (y * scale + scale / 2).min(framebuffer_height - 1);
                    let screen_y = (1.0 - (2.0 * pixel_y as f32) / height) * perspective_scale;

                    for (x, pixel) in row.iter_mut().enumerate() {
                        let pixel_x = (x * scale + scale / 2).min(framebuffer_width - 1);
                        let screen_x = ((2.0 * pixel_x as f32) / width - 1.0)
                            * aspect_ratio
                            * perspective_scale;
                        let ray_direction =
                            normalize(&(screen_x * right + screen_y * up + forward));

                        *pixel =
                            cast_ray(&camera.eye, &ray_direction, objects, lights, 0, max_depth)
                                .to_hex();
                    }
                }
            });
        }
    });

    // Amplia cada muestra sobre un bloque de la ventana. En calidad final
    // `scale` vale 1 y la copia es pixel por pixel.
    for y in 0..framebuffer_height {
        let source_row = (y / scale) * sample_width;
        let target_row = y * framebuffer_width;
        for x in 0..framebuffer_width {
            framebuffer.buffer[target_row + x] = samples[source_row + x / scale];
        }
    }
}

/// Busca una textura creada por el artista dentro de `assets`. Si todavia no
/// existe o no es un PPM P6 valido, conserva la alternativa procedural para que
/// el proyecto siempre pueda ejecutarse.
fn load_texture(asset_name: &str, procedural_fallback: fn() -> Texture) -> &'static Texture {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join(asset_name);

    let texture = match Texture::from_ppm(&path.to_string_lossy()) {
        Ok(texture) => texture,
        Err(error) => {
            eprintln!(
                "No se cargo '{}': {error}. Se usara la textura procedural.",
                path.display()
            );
            procedural_fallback()
        }
    };

    Box::leak(Box::new(texture))
}

fn add_cube(objects: &mut Vec<Box<dyn RayIntersect>>, center: Vec3, size: f32, material: Material) {
    objects.push(Box::new(Cube::new(center, size, material)));
}

fn add_box(objects: &mut Vec<Box<dyn RayIntersect>>, min: Vec3, max: Vec3, material: Material) {
    objects.push(Box::new(Cube::from_bounds(min, max, material)));
}

/// Construye el "Santuario perdido de Hyrule" sin introducir primitivas nuevas:
/// todo el detalle nace de cubos, alturas, materiales y una composicion asimetrica.
fn build_scene(
    grass: Material,
    stone: Material,
    wood: Material,
    water: Material,
    metal: Material,
) -> Vec<Box<dyn RayIntersect>> {
    let mut objects: Vec<Box<dyn RayIntersect>> = Vec::new();

    // Terreno de 7x7. El estanque hunde su lecho; la esquina del arbol y algunos
    // bordes se elevan para romper la silueta plana de la plataforma.
    const HALF_TERRAIN: i32 = 3;
    let pond_cells = [(2, 0), (3, 0), (1, 1), (2, 1), (3, 1), (2, 2), (3, 2)];
    let raised_cells = [(-3, -3), (-2, -3), (-3, -2), (-2, -2), (-3, -1)];
    let exposed_stone = [(-1, 2), (1, 0), (-3, 1), (2, -2), (0, -3)];

    for gx in -HALF_TERRAIN..=HALF_TERRAIN {
        for gz in -HALF_TERRAIN..=HALF_TERRAIN {
            let is_pond = pond_cells.contains(&(gx, gz));
            let is_raised = raised_cells.contains(&(gx, gz));
            let top = if is_pond {
                -1.3
            } else if is_raised {
                -0.72
            } else if (gx + gz).rem_euclid(5) == 0 {
                -0.9
            } else {
                -1.0
            };
            let material = if is_pond || exposed_stone.contains(&(gx, gz)) {
                stone
            } else {
                grass
            };

            add_box(
                &mut objects,
                Vec3::new(gx as f32 - 0.5, -2.0, gz as f32 - 0.5),
                Vec3::new(gx as f32 + 0.5, top, gz as f32 + 0.5),
                material,
            );
        }
    }

    // Camino antiguo: losas separadas, de anchos distintos y con una curva suave
    // desde el borde frontal (+Z) hasta el centro del santuario.
    let path_stones = [
        (0.65, 3.15, 0.78, 0.42, -0.84),
        (0.42, 2.62, 0.52, 0.36, -0.86),
        (0.18, 2.12, 0.72, 0.4, -0.83),
        (-0.12, 1.58, 0.5, 0.35, -0.85),
        (0.02, 1.08, 0.68, 0.4, -0.82),
        (-0.28, 0.52, 0.48, 0.34, -0.84),
        (-0.18, 0.02, 0.7, 0.38, -0.81),
        (-0.42, -0.52, 0.54, 0.35, -0.83),
        (-0.35, -0.92, 0.78, 0.42, -0.78),
    ];
    for (x, z, width, depth, top) in path_stones {
        add_box(
            &mut objects,
            Vec3::new(x - width / 2.0, -1.01, z - depth / 2.0),
            Vec3::new(x + width / 2.0, top, z + depth / 2.0),
            stone,
        );
    }

    // Estanque irregular y hundido. Cada celda de agua descansa sobre el lecho
    // de piedra, dejando visibles reflexion y refraccion desde varios angulos.
    for (x, z) in pond_cells {
        add_box(
            &mut objects,
            Vec3::new(x as f32 - 0.49, -1.29, z as f32 - 0.49),
            Vec3::new(x as f32 + 0.49, -0.93, z as f32 + 0.49),
            water,
        );
    }

    // Arbol monumental sobre la elevacion posterior izquierda. El tronco se
    // bifurca y la copa usa tres alturas para obtener una silueta irregular.
    let tree_x = -2.35;
    let tree_z = -2.25;
    add_box(
        &mut objects,
        Vec3::new(tree_x - 0.28, -0.72, tree_z - 0.28),
        Vec3::new(tree_x + 0.28, 0.92, tree_z + 0.28),
        wood,
    );
    add_box(
        &mut objects,
        Vec3::new(tree_x - 0.62, 0.35, tree_z - 0.18),
        Vec3::new(tree_x + 0.58, 0.62, tree_z + 0.18),
        wood,
    );
    add_box(
        &mut objects,
        Vec3::new(tree_x - 0.16, 0.52, tree_z - 0.55),
        Vec3::new(tree_x + 0.16, 0.78, tree_z + 0.42),
        wood,
    );

    let foliage = [
        (-0.95, 0.0, 1.05, 0.9),
        (-0.45, -0.55, 1.18, 0.95),
        (0.15, -0.62, 1.08, 0.88),
        (0.72, -0.28, 1.15, 0.9),
        (-0.78, 0.55, 1.2, 0.92),
        (-0.12, 0.48, 1.35, 1.05),
        (0.6, 0.5, 1.12, 0.88),
        (-0.52, -0.15, 1.78, 0.95),
        (0.2, -0.1, 1.92, 1.0),
        (-0.2, 0.25, 2.42, 0.84),
        (0.42, 0.18, 2.2, 0.78),
    ];
    for (dx, dz, y, size) in foliage {
        add_cube(
            &mut objects,
            Vec3::new(tree_x + dx, y, tree_z + dz),
            size,
            grass,
        );
    }

    // Santuario central posterior: tres gradas, altar elevado y piedras caidas.
    let shrine_x = -0.35;
    let shrine_z = -1.3;
    add_box(
        &mut objects,
        Vec3::new(shrine_x - 1.2, -1.0, shrine_z - 0.95),
        Vec3::new(shrine_x + 1.2, -0.7, shrine_z + 0.95),
        stone,
    );
    add_box(
        &mut objects,
        Vec3::new(shrine_x - 0.88, -0.7, shrine_z - 0.7),
        Vec3::new(shrine_x + 0.88, -0.4, shrine_z + 0.7),
        stone,
    );
    add_box(
        &mut objects,
        Vec3::new(shrine_x - 0.5, -0.4, shrine_z - 0.46),
        Vec3::new(shrine_x + 0.5, -0.08, shrine_z + 0.46),
        stone,
    );

    // Variantes del mismo metal.ppm: conservan su textura, pero separan la
    // paleta y las propiedades opticas de cada parte de la Espada Maestra.
    let sword_blade = Material {
        specular: 70.0,
        albedo: [0.72, 0.25, 0.12, 0.0],
        ..metal.with_tint(Color::new(175, 235, 245))
    };
    let sword_guard = Material {
        specular: 55.0,
        albedo: [0.72, 0.22, 0.16, 0.0],
        ..metal.with_tint(Color::new(65, 75, 155))
    };
    let sword_grip = Material {
        specular: 30.0,
        albedo: [0.82, 0.15, 0.06, 0.0],
        ..metal.with_tint(Color::new(65, 125, 85))
    };
    let sword_gold = Material {
        specular: 75.0,
        albedo: [0.68, 0.25, 0.22, 0.0],
        ..metal.with_tint(Color::new(255, 190, 65))
    };

    // Hoja cian escalonada: ancha cerca de la guarda y terminada en punta hacia
    // el pedestal. La baja reflectividad deja visible la textura metalica.
    for (half_width, min_y, max_y, half_depth) in [
        (0.035, -0.08, 0.04, 0.035),
        (0.075, 0.04, 0.2, 0.045),
        (0.115, 0.2, 0.88, 0.055),
        (0.15, 0.88, 1.08, 0.065),
    ] {
        add_box(
            &mut objects,
            Vec3::new(shrine_x - half_width, min_y, shrine_z - half_depth),
            Vec3::new(shrine_x + half_width, max_y, shrine_z + half_depth),
            sword_blade,
        );
    }

    // Marca vertical sobre la cara frontal de la hoja, inspirada en los grabados
    // de la espada original.
    add_box(
        &mut objects,
        Vec3::new(shrine_x - 0.018, 0.32, shrine_z + 0.056),
        Vec3::new(shrine_x + 0.018, 0.74, shrine_z + 0.072),
        sword_guard,
    );

    // Guarda alada. Los tres escalones de cada lado producen la curva descendente
    // de la referencia usando unicamente prismas rectangulares.
    for side in [-1.0_f32, 1.0] {
        let (inner, outer) = if side < 0.0 {
            (shrine_x - 0.24, shrine_x - 0.08)
        } else {
            (shrine_x + 0.08, shrine_x + 0.24)
        };
        add_box(
            &mut objects,
            Vec3::new(inner.min(outer), 1.04, shrine_z - 0.11),
            Vec3::new(inner.max(outer), 1.18, shrine_z + 0.11),
            sword_guard,
        );

        let (inner, outer) = if side < 0.0 {
            (shrine_x - 0.4, shrine_x - 0.24)
        } else {
            (shrine_x + 0.24, shrine_x + 0.4)
        };
        add_box(
            &mut objects,
            Vec3::new(inner.min(outer), 0.96, shrine_z - 0.1),
            Vec3::new(inner.max(outer), 1.11, shrine_z + 0.1),
            sword_guard,
        );

        let (inner, outer) = if side < 0.0 {
            (shrine_x - 0.54, shrine_x - 0.4)
        } else {
            (shrine_x + 0.4, shrine_x + 0.54)
        };
        add_box(
            &mut objects,
            Vec3::new(inner.min(outer), 0.86, shrine_z - 0.085),
            Vec3::new(inner.max(outer), 1.02, shrine_z + 0.085),
            sword_guard,
        );
    }

    // Empunadura verde con bandas azul oscuro.
    add_box(
        &mut objects,
        Vec3::new(shrine_x - 0.065, 1.13, shrine_z - 0.065),
        Vec3::new(shrine_x + 0.065, 1.72, shrine_z + 0.065),
        sword_grip,
    );
    for y in [1.22, 1.38, 1.54, 1.68] {
        add_box(
            &mut objects,
            Vec3::new(shrine_x - 0.078, y, shrine_z - 0.078),
            Vec3::new(shrine_x + 0.078, y + 0.035, shrine_z + 0.078),
            sword_guard,
        );
    }

    // Gema dorada frontal en el centro de la guarda.
    for (half_width, min_y, max_y) in [(0.035, 0.91, 1.0), (0.075, 1.0, 1.12), (0.04, 1.12, 1.2)] {
        add_box(
            &mut objects,
            Vec3::new(shrine_x - half_width, min_y, shrine_z + 0.112),
            Vec3::new(shrine_x + half_width, max_y, shrine_z + 0.145),
            sword_gold,
        );
    }

    // Pomo facetado por escalones, similar al remate azul de la referencia.
    for (half_width, min_y, max_y) in [(0.09, 1.72, 1.84), (0.14, 1.84, 2.0), (0.075, 2.0, 2.12)] {
        add_box(
            &mut objects,
            Vec3::new(shrine_x - half_width, min_y, shrine_z - 0.09),
            Vec3::new(shrine_x + half_width, max_y, shrine_z + 0.09),
            sword_guard,
        );
    }

    // Cristales de energia antigua. El agua aporta el cian, la transparencia y
    // los reflejos sin agregar un sexto material.
    let energy_crystals = [
        (-0.78, -0.35, 0.24),
        (0.74, -0.28, 0.3),
        (-0.72, 0.42, 0.2),
        (0.66, 0.46, 0.22),
    ];
    for (dx, dz, height) in energy_crystals {
        add_box(
            &mut objects,
            Vec3::new(shrine_x + dx - 0.07, -0.4, shrine_z + dz - 0.07),
            Vec3::new(shrine_x + dx + 0.07, -0.4 + height, shrine_z + dz + 0.07),
            water,
        );
    }

    // Ruinas asimetricas: dos columnas rotas y bloques caidos alrededor del altar.
    for (x, z, width, height) in [
        (-1.75, -1.75, 0.34, 0.95),
        (1.05, -2.05, 0.38, 0.62),
        (1.32, -1.72, 0.28, 0.4),
    ] {
        add_box(
            &mut objects,
            Vec3::new(x - width / 2.0, -1.0, z - width / 2.0),
            Vec3::new(x + width / 2.0, -1.0 + height, z + width / 2.0),
            stone,
        );
    }
    for (x, z, size) in [
        (-1.55, -0.62, 0.34),
        (-1.9, -0.92, 0.3),
        (0.8, -0.38, 0.36),
        (1.22, -0.72, 0.28),
        (-1.05, -2.48, 0.32),
    ] {
        add_cube(
            &mut objects,
            Vec3::new(x, -1.0 + size / 2.0, z),
            size,
            stone,
        );
    }

    // Losa posterior con una Trifuerza en mosaico. Cada triangulo se aproxima
    // con tres teselas metalicas para que el simbolo se reconozca como pixel art.
    add_box(
        &mut objects,
        Vec3::new(0.55, -0.98, -2.72),
        Vec3::new(1.35, 0.28, -2.48),
        stone,
    );
    for (center_x, center_y) in [(0.78, -0.42), (1.12, -0.42), (0.95, -0.08)] {
        for (dx, dy) in [(0.0, 0.07), (-0.07, -0.07), (0.07, -0.07)] {
            add_box(
                &mut objects,
                Vec3::new(center_x + dx - 0.035, center_y + dy - 0.035, -2.47),
                Vec3::new(center_x + dx + 0.035, center_y + dy + 0.035, -2.39),
                metal,
            );
        }
    }

    // Cofre junto al estanque, arropado por piedras y vegetacion.
    let chest_x = 1.05;
    let chest_z = 2.55;
    add_box(
        &mut objects,
        Vec3::new(chest_x - 0.4, -1.0, chest_z - 0.28),
        Vec3::new(chest_x + 0.4, -0.57, chest_z + 0.28),
        wood,
    );
    add_box(
        &mut objects,
        Vec3::new(chest_x - 0.42, -0.57, chest_z - 0.3),
        Vec3::new(chest_x + 0.42, -0.35, chest_z + 0.3),
        wood,
    );
    add_box(
        &mut objects,
        Vec3::new(chest_x - 0.06, -0.68, chest_z + 0.28),
        Vec3::new(chest_x + 0.06, -0.45, chest_z + 0.34),
        metal,
    );

    // Piedras de ribera, arbustos y manojos de pasto. Se agrupan en vez de
    // repartirse uniformemente para mantener una composicion natural.
    for (x, z, size) in [
        (1.12, 0.25, 0.32),
        (0.78, 0.82, 0.27),
        (1.05, 1.9, 0.35),
        (1.72, 2.62, 0.3),
        (2.72, -0.62, 0.34),
        (3.22, 2.65, 0.28),
    ] {
        add_cube(
            &mut objects,
            Vec3::new(x, -1.0 + size / 2.0, z),
            size,
            stone,
        );
    }
    for (x, z, size) in [
        (0.58, 1.92, 0.38),
        (1.62, 2.78, 0.42),
        (0.72, 0.42, 0.32),
        (2.72, -0.5, 0.36),
        (-1.72, 0.82, 0.4),
        (-2.65, 0.25, 0.45),
        (-1.55, 2.5, 0.35),
    ] {
        add_cube(
            &mut objects,
            Vec3::new(x, -1.0 + size / 2.0, z),
            size,
            grass,
        );
    }
    for (x, z, width, height) in [
        (-1.35, 1.65, 0.12, 0.38),
        (-1.58, 1.78, 0.1, 0.3),
        (0.52, 2.35, 0.1, 0.32),
        (0.35, 2.5, 0.09, 0.26),
        (1.02, 0.48, 0.08, 0.3),
        (-2.9, -0.65, 0.12, 0.36),
    ] {
        add_box(
            &mut objects,
            Vec3::new(x - width / 2.0, -1.0, z - width / 2.0),
            Vec3::new(x + width / 2.0, -1.0 + height, z + width / 2.0),
            grass,
        );
    }

    objects
}

fn main() {
    let frame_delay = Duration::from_millis(16);

    let mut framebuffer = Framebuffer::new(WIDTH, HEIGHT);

    let mut window = Window::new(
        "Santuario perdido de Hyrule - Raytracing",
        WIDTH,
        HEIGHT,
        WindowOptions::default(),
    )
    .unwrap();

    // --- Texturas del diorama (generadas proceduralmente, ver texture.rs) ---
    // `Box::leak` les da vida `'static`: se generan una sola vez al arrancar y
    // quedan vivas mientras corre el programa, así `Material` se mantiene `Copy`.
    let grass_tex = load_texture("grass.ppm", Texture::grass);
    let stone_tex = load_texture("stone.ppm", Texture::stone_bricks);
    let wood_tex = load_texture("wood.ppm", Texture::wood_planks);
    let water_tex = load_texture("water.ppm", Texture::water_ripples);
    let metal_tex = load_texture("metal.ppm", Texture::metal_shine);

    // --- Los 5 materiales del diorama, cada uno con su propia textura y sus
    // propios pesos de difuso/especular/reflectividad/transparencia ---
    let grass = Material::new_textured(grass_tex, 8.0, [0.9, 0.05, 0.0, 0.0], 1.0);
    let stone = Material::new_textured(stone_tex, 20.0, [0.8, 0.2, 0.05, 0.0], 1.0);
    let wood = Material::new_textured(wood_tex, 12.0, [0.85, 0.15, 0.02, 0.0], 1.0);
    let water = Material::new_textured(water_tex, 70.0, [0.55, 0.25, 0.08, 0.45], 1.33);
    let sword_metal = Material::new_textured(metal_tex, 100.0, [0.65, 0.3, 0.25, 0.0], 1.0);

    // --- Layout: "Santuario perdido de Hyrule" ---
    let objects = build_scene(grass, stone, wood, water, sword_metal);

    // Dos luces: una cálida principal (el "sol", tipo atardecer) que proyecta
    // sombras marcadas, y una fría de relleno bastante más tenue del lado
    // opuesto, para que las sombras no queden completamente planas — el
    // contraste cálido/frío también ayuda a que el agua y la espada resalten.
    let key_light = Light::new(Vec3::new(-6.0, 7.0, 5.0), Color::new(255, 214, 170), 1.6);
    let fill_light = Light::new(Vec3::new(5.0, 3.5, -4.0), Color::new(140, 170, 255), 0.35);
    let lights = [key_light, fill_light];

    let mut camera = Camera::new(
        Vec3::new(5.8, 4.2, 8.2),
        Vec3::new(0.0, -0.15, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    );

    let mut preview_needed = true;
    let mut final_rendered = false;
    let mut last_camera_move = Instant::now();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let orbit = [
            (Key::Left, ROTATION_SPEED, 0.0),
            (Key::Right, -ROTATION_SPEED, 0.0),
            (Key::Up, 0.0, -ROTATION_SPEED),
            (Key::Down, 0.0, ROTATION_SPEED),
        ];

        let mut camera_moved = false;

        for (key, delta_yaw, delta_pitch) in orbit {
            if window.is_key_down(key) {
                camera.orbit(delta_yaw, delta_pitch);
                camera_moved = true;
            }
        }

        if window.is_key_down(Key::Q) {
            camera.zoom(-ZOOM_SPEED);
            camera_moved = true;
        }
        if window.is_key_down(Key::E) {
            camera.zoom(ZOOM_SPEED);
            camera_moved = true;
        }

        if camera_moved || preview_needed {
            // Vista previa rapida mientras se mueve: 1/16 de los rayos primarios,
            // una sola luz y un rebote.
            render(
                &mut framebuffer,
                &objects,
                &camera,
                &lights[..1],
                PREVIEW_SCALE,
                PREVIEW_DEPTH,
            );
            preview_needed = false;
            final_rendered = false;
            last_camera_move = Instant::now();
        } else if !final_rendered && last_camera_move.elapsed() >= FINAL_RENDER_DELAY {
            // Al soltar las teclas se reemplaza la vista previa por el render
            // completo con ambas luces, resolucion nativa y todos los rebotes.
            render(&mut framebuffer, &objects, &camera, &lights, 1, MAX_DEPTH);
            final_rendered = true;
        }

        window
            .update_with_buffer(&framebuffer.buffer, WIDTH, HEIGHT)
            .unwrap();

        std::thread::sleep(frame_delay);
    }
}
