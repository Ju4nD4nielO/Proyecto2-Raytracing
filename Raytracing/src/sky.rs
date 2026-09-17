use crate::color::Color;
use nalgebra_glm::Vec3;

/// Cielo procedural: gradiente de horizonte a cenit, un halo de sol alineado con
/// la luz de la escena, y nubes dispersas generadas con ruido de baja frecuencia.
/// No es una imagen panorámica cargada de archivo — se calcula directamente a
/// partir de la dirección del rayo, así que no depende de ningún asset externo.
pub fn sample_skybox(direction: &Vec3, sun_position: &Vec3) -> Color {
    let d = direction.normalize();

    // Gradiente vertical: celeste claro cerca del horizonte, azul más profundo
    // hacia el cenit. `d.y` va de -1 (abajo) a 1 (arriba); usamos solo la mitad
    // positiva porque casi nada de lo que ve la cámara mira hacia abajo del
    // horizonte (el piso del diorama ya tapa esos rayos).
    let t = d.y.max(0.0).powf(0.5);
    let horizon = Vec3::new(190.0, 222.0, 235.0);
    let zenith = Vec3::new(60.0, 120.0, 200.0);
    let sky = horizon * (1.0 - t) + zenith * t;

    // Halo de sol: qué tan alineada está esta dirección de rayo con la posición
    // de la luz. Un exponente alto da un disco chico y brillante; uno bajo da un
    // halo ancho y tenue alrededor.
    let sun_alignment = d.dot(&sun_position.normalize()).max(0.0);
    let sun_glow = sun_alignment.powf(180.0) * 230.0 + sun_alignment.powf(8.0) * 35.0;

    // Nubes: ruido de valor de baja frecuencia sobre la proyección angular de la
    // dirección del rayo (como envolver la esfera del cielo), recortado a manchas
    // dispersas y atenuado cerca del horizonte.
    let cloud_u = d.x.atan2(d.z) * 3.0;
    let cloud_v = d.y * 6.0;
    let noise = (cloud_u.sin() * 1.7 + cloud_v.cos() * 1.3 + (cloud_u * 2.3).sin() * 0.6) * 0.5
        + 0.5;
    let cloud_mask = ((noise - 0.62).max(0.0) * 3.0).min(1.0) * d.y.max(0.0).sqrt();

    let color = sky + Vec3::new(sun_glow, sun_glow * 0.9, sun_glow * 0.6);
    let color = color * (1.0 - cloud_mask) + Vec3::new(255.0, 255.0, 255.0) * cloud_mask;

    Color::new(
        color.x.clamp(0.0, 255.0) as u8,
        color.y.clamp(0.0, 255.0) as u8,
        color.z.clamp(0.0, 255.0) as u8,
    )
}