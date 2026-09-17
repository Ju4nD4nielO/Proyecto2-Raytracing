mod camera;
mod color;
mod cube;
mod framebuffer;
mod light;
mod ray_intersect;
mod texture;

use minifb::{Key, Window, WindowOptions};
use nalgebra_glm::{dot, normalize, Vec3};
use std::f32::consts::PI;
use std::time::Duration;

use crate::camera::Camera;
use crate::color::Color;
use crate::cube::Cube;
use crate::framebuffer::Framebuffer;
use crate::light::Light;
use crate::ray_intersect::{
    Intersect, Material, RayIntersect, DIFFUSE, REFLECTIVITY, SPECULAR, TRANSPARENCY,
};
use crate::texture::Texture;

const WIDTH: usize = 800;
const HEIGHT: usize = 600;
const BACKGROUND_COLOR: u32 = 0x040C24;

const FOV: f32 = PI / 3.0;

const ROTATION_SPEED: f32 = PI / 60.0;

const SHADOW_BIAS: f32 = 1e-3;
// Usado para separar el origen de un rayo reflejado/refractado de la superficie
// que lo generó, para que no se vuelva a auto-intersectar por error de redondeo.
const BIAS: f32 = 1e-3;

const MAX_DEPTH: u32 = 3;

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
    light: &Light,
    objects: &[Box<dyn RayIntersect>],
) -> Color {
    let light_direction = (light.position - intersect.point).normalize();
    let view_direction = (ray_origin - intersect.point).normalize();

    let light_intensity = if cast_shadow(intersect, &light_direction, light, objects) {
        0.0
    } else {
        light.intensity
    };

    let diffuse_color = intersect.material.sample_diffuse(intersect.uv);

    let diffuse_intensity = dot(&intersect.normal, &light_direction).max(0.0);
    let diffuse =
        diffuse_color * (diffuse_intensity * intersect.material.albedo[DIFFUSE] * light_intensity);

    let reflect_direction = reflect(&-light_direction, &intersect.normal);
    let specular_intensity = dot(&view_direction, &reflect_direction)
        .max(0.0)
        .powf(intersect.material.specular);

    let specular = light.color
        * (specular_intensity * intersect.material.albedo[SPECULAR] * light_intensity);

    diffuse + specular
}

pub fn cast_ray(
    ray_origin: &Vec3,
    ray_direction: &Vec3,
    objects: &[Box<dyn RayIntersect>],
    light: &Light,
    depth: u32,
) -> Color {
    if depth > MAX_DEPTH {
        return Color::from_hex(BACKGROUND_COLOR);
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
        return Color::from_hex(BACKGROUND_COLOR);
    };

    let local_color = shade(&intersect, ray_origin, light, objects);

    let reflectivity = intersect.material.albedo[REFLECTIVITY];
    let transparency = intersect.material.albedo[TRANSPARENCY];

    if reflectivity <= 0.0 && transparency <= 0.0 {
        return local_color;
    }

    let fresnel = if transparency > 0.0 {
        fresnel_reflectance(ray_direction, &intersect.normal, intersect.material.refractive_index)
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
        cast_ray(&reflect_origin, &reflect_direction, objects, light, depth + 1)
    } else {
        Color::from_hex(0)
    };

    let effective_transparency = transparency * (1.0 - fresnel);

    let refracted = if effective_transparency > 0.0 {
        match refract(ray_direction, &intersect.normal, intersect.material.refractive_index) {
            Some(refract_direction) => {
                let refract_direction = refract_direction.normalize();
                let refract_origin = offset_origin(&intersect, &refract_direction);
                cast_ray(&refract_origin, &refract_direction, objects, light, depth + 1)
            }
            // Reflexión interna total: no hay rayo refractado, toda la energía
            // que no se manejó como reflexión "base" también rebota.
            None => reflected,
        }
    } else {
        Color::from_hex(0)
    };

    let local_weight = (1.0 - effective_reflectivity - effective_transparency).max(0.0);

    local_color * local_weight + reflected * effective_reflectivity + refracted * effective_transparency
}

pub fn render(
    framebuffer: &mut Framebuffer,
    objects: &[Box<dyn RayIntersect>],
    camera: &Camera,
    light: &Light,
) {
    let width = framebuffer.width as f32;
    let height = framebuffer.height as f32;
    let aspect_ratio = width / height;

    let perspective_scale = (FOV / 2.0).tan();

    for y in 0..framebuffer.height {
        for x in 0..framebuffer.width {
            let screen_x = (2.0 * x as f32) / width - 1.0;
            let screen_y = -(2.0 * y as f32) / height + 1.0;

            let screen_x = screen_x * aspect_ratio * perspective_scale;
            let screen_y = screen_y * perspective_scale;

            let ray_direction = normalize(&Vec3::new(screen_x, screen_y, -1.0));
            let ray_direction = camera.basis_change(&ray_direction);

            framebuffer.set_current_color(
                cast_ray(&camera.eye, &ray_direction, objects, light, 0).to_hex(),
            );
            framebuffer.point(x, y);
        }
    }
}

fn main() {
    let frame_delay = Duration::from_millis(16);

    let mut framebuffer = Framebuffer::new(WIDTH, HEIGHT);

    let mut window = Window::new("Lakitu", WIDTH, HEIGHT, WindowOptions::default()).unwrap();

    // --- Texturas del diorama (generadas proceduralmente, ver texture.rs) ---
    // `Box::leak` les da vida `'static`: se generan una sola vez al arrancar y
    // quedan vivas mientras corre el programa, así `Material` se mantiene `Copy`.
    let grass_tex: &'static Texture = Box::leak(Box::new(Texture::grass()));
    let stone_tex: &'static Texture = Box::leak(Box::new(Texture::stone_bricks()));
    let wood_tex: &'static Texture = Box::leak(Box::new(Texture::wood_planks()));
    let water_tex: &'static Texture = Box::leak(Box::new(Texture::water_ripples()));
    let metal_tex: &'static Texture = Box::leak(Box::new(Texture::metal_shine()));

    // --- Los 5 materiales del diorama, cada uno con su propia textura y sus
    // propios pesos de difuso/especular/reflectividad/transparencia ---
    let grass = Material::new_textured(grass_tex, 8.0, [0.9, 0.05, 0.0, 0.0], 1.0);
    let stone = Material::new_textured(stone_tex, 20.0, [0.8, 0.2, 0.05, 0.0], 1.0);
    let wood = Material::new_textured(wood_tex, 12.0, [0.85, 0.15, 0.02, 0.0], 1.0);
    let water = Material::new_textured(water_tex, 90.0, [0.1, 0.4, 0.15, 0.85], 1.33);
    let sword_metal =
        Material::new_textured(metal_tex, 200.0, [0.15, 0.6, 0.55, 0.0], 1.0);

    // --- Layout: "Rincón de Hyrule" ---
    let mut objects: Vec<Box<dyn RayIntersect>> = Vec::new();

    // Piso: cuadrícula de 6x6 cubos de pasto, con un camino de piedra cruzando el
    // centro y un pequeño estanque en una esquina.
    const FLOOR_TILES: i32 = 6;
    const TILE: f32 = 1.0;
    let offset = (FLOOR_TILES as f32 - 1.0) / 2.0;

    for gx in 0..FLOOR_TILES {
        for gz in 0..FLOOR_TILES {
            let x = (gx as f32 - offset) * TILE;
            let z = (gz as f32 - offset) * TILE;
            let center = Vec3::new(x, -1.5, z);

            let is_path = gz == 3 && (1..=4).contains(&gx);
            let is_pond = gx >= 4 && gz <= 1;

            let material = if is_pond {
                water
            } else if is_path {
                stone
            } else {
                grass
            };

            objects.push(Box::new(Cube::new(center, TILE, material)));
        }
    }

    // Árbol Korok: tronco de dos cubos de madera apilados + copa de pasto en cruz.
    let tree_x = (-offset) * TILE;
    let tree_z = (1.0 - offset) * TILE;
    objects.push(Box::new(Cube::new(Vec3::new(tree_x, -0.7, tree_z), 0.6, wood)));
    objects.push(Box::new(Cube::new(Vec3::new(tree_x, -0.1, tree_z), 0.6, wood)));
    for (dx, dz) in [(0.0, 0.0), (0.7, 0.0), (-0.7, 0.0), (0.0, 0.7), (0.0, -0.7)] {
        objects.push(Box::new(Cube::new(
            Vec3::new(tree_x + dx, 0.55, tree_z + dz),
            0.75,
            grass,
        )));
    }

    // Cofre de madera, apoyado sobre el piso.
    let chest_x = (4.0 - offset) * TILE;
    let chest_z = (4.0 - offset) * TILE;
    objects.push(Box::new(Cube::new(
        Vec3::new(chest_x, -0.75, chest_z),
        0.5,
        wood,
    )));

    // Pedestal de piedra con la Espada Maestra clavada encima: la hoja es un cubo
    // delgado (min/max explícitos en vez de un cubo uniforme) para que se vea como
    // una hoja de espada y no como un bloque.
    let sword_x = (2.0 - offset) * TILE;
    let sword_z = (2.0 - offset) * TILE;
    objects.push(Box::new(Cube::new(
        Vec3::new(sword_x, -0.7, sword_z),
        0.6,
        stone,
    )));
    objects.push(Box::new(Cube::from_bounds(
        Vec3::new(sword_x - 0.04, -0.4, sword_z - 0.09),
        Vec3::new(sword_x + 0.04, 0.55, sword_z + 0.09),
        sword_metal,
    )));
    // Guarda de la espada: una lámina más ancha y corta cruzando la hoja.
    objects.push(Box::new(Cube::from_bounds(
        Vec3::new(sword_x - 0.22, -0.45, sword_z - 0.05),
        Vec3::new(sword_x + 0.22, -0.35, sword_z + 0.05),
        sword_metal,
    )));

    let light = Light::new(Vec3::new(-6.0, 6.0, 8.0), Color::new(255, 255, 255), 1.5);

    let mut camera = Camera::new(
        Vec3::new(0.5, 3.2, 7.5),
        Vec3::new(0.0, -1.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    );

    let mut camera_moved = true;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let orbit = [
            (Key::Left, ROTATION_SPEED, 0.0),
            (Key::Right, -ROTATION_SPEED, 0.0),
            (Key::Up, 0.0, -ROTATION_SPEED),
            (Key::Down, 0.0, ROTATION_SPEED),
        ];

        for (key, delta_yaw, delta_pitch) in orbit {
            if window.is_key_down(key) {
                camera.orbit(delta_yaw, delta_pitch);
                camera_moved = true;
            }
        }

        if camera_moved {
            render(&mut framebuffer, &objects, &camera, &light);
            camera_moved = false;
        }

        window
            .update_with_buffer(&framebuffer.buffer, WIDTH, HEIGHT)
            .unwrap();

        std::thread::sleep(frame_delay);
    }
}