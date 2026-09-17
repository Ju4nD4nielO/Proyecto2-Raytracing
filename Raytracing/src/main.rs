mod camera;
mod color;
mod cube;
mod cylinder;
mod framebuffer;
mod light;
mod ray_intersect;
mod sphere;
mod texture;

use minifb::{Key, Window, WindowOptions};
use nalgebra_glm::{dot, normalize, Vec3};
use std::f32::consts::PI;
use std::time::Duration;

use crate::camera::Camera;
use crate::color::Color;
use crate::cube::Cube;
use crate::cylinder::Cylinder;
use crate::framebuffer::Framebuffer;
use crate::light::Light;
use crate::ray_intersect::{
    Intersect, Material, RayIntersect, DIFFUSE, REFLECTIVITY, SPECULAR, TRANSPARENCY,
};
use crate::sphere::Sphere;
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

    let ivory = Material::new(Color::new(100, 100, 80), 50.0, [0.6, 0.3, 0.1, 0.0]);
    let rubber = Material::new(Color::new(80, 0, 0), 10.0, [0.9, 0.1, 0.0, 0.0]);
    let cobalt = Material::new(Color::new(40, 80, 140), 80.0, [0.7, 0.4, 0.15, 0.0]);
    let jade = Material::new(Color::new(60, 130, 100), 30.0, [0.8, 0.25, 0.05, 0.0]);
    let slate = Material::new(Color::new(80, 80, 92), 15.0, [0.85, 0.1, 0.2, 0.0]);
    let mirror = Material::new(Color::new(255, 255, 255), 1425.0, [0.0, 10.0, 0.85, 0.0]);

    // --- Pruebas temporales del sistema de texturas/refracción (sprint 2) ---
    // `Box::leak` reserva la textura en el heap y le da vida `'static`: se carga
    // una vez y queda viva mientras corre el programa, que es exactamente lo que
    // queremos para texturas (igual que en cualquier motor gráfico).
    let checker_texture: &'static Texture = Box::leak(Box::new(Texture::checkerboard(
        4,
        Color::new(235, 235, 235),
        Color::new(35, 35, 40),
    )));
    let checkered = Material::new_textured(checker_texture, 10.0, [0.9, 0.1, 0.0, 0.0], 1.0);

    let glass = Material::new(Color::new(245, 250, 255), 125.0, [0.05, 0.5, 0.05, 0.9])
        .with_refractive_index(1.5);
    // --- Fin de pruebas temporales ---

    let objects: Vec<Box<dyn RayIntersect>> = vec![
        Box::new(Cylinder::new(
            Vec3::new(0.0, -2.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            0.25,
            6.0,
            slate,
        )),
        Box::new(Sphere {
            center: Vec3::new(0.0, -0.75, 0.0),
            radius: 1.0,
            material: ivory,
        }),
        Box::new(Sphere {
            center: Vec3::new(1.9, -1.25, -0.9),
            radius: 0.5,
            material: rubber,
        }),
        Box::new(Sphere {
            center: Vec3::new(-1.5, -1.25, 1.1),
            radius: 0.5,
            material: cobalt,
        }),
        Box::new(Cylinder::new(
            Vec3::new(-2.3, -1.75, -0.6),
            Vec3::new(0.28, 1.0, -0.12),
            2.0,
            0.35,
            jade,
        )),
        Box::new(Sphere {
            center: Vec3::new(2.35, -1.0, 1.45),
            radius: 0.75,
            material: mirror,
        }),
        // Cubo de prueba temporal #1: valida que el mapeo UV por cara se ve bien
        // (el tablero de ajedrez debe verse derecho y sin estirones en las 3 caras
        // visibles).
        Box::new(Cube::new(Vec3::new(-0.5, 1.2, -0.5), 1.0, checkered)),
        // Cubo de prueba temporal #2: valida refracción — debería verse el fondo
        // (y los otros objetos) distorsionado a través de este cubo, con un borde
        // más reflejante en ángulos rasantes por el Fresnel.
        Box::new(Cube::new(Vec3::new(1.0, 1.2, 1.0), 1.0, glass)),
    ];

    let light = Light::new(Vec3::new(-6.0, 6.0, 8.0), Color::new(255, 255, 255), 1.5);

    let mut camera = Camera::new(
        Vec3::new(0.0, 0.4, 6.0),
        Vec3::new(0.0, -0.7, 0.0),
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