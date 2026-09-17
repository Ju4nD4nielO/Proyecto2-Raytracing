use nalgebra_glm::Vec3;
use std::f32::consts::PI;

const PITCH_LIMIT: f32 = PI / 2.0 - 0.1;

// Qué tan cerca/lejos del centro de la escena puede llegar la cámara con zoom.
// El mínimo evita que la cámara termine dentro de un objeto; el máximo evita
// alejarse tanto que el diorama se vea como un punto.
const MIN_ZOOM_DISTANCE: f32 = 2.5;
const MAX_ZOOM_DISTANCE: f32 = 18.0;

pub struct Camera {
    pub eye: Vec3,
    pub center: Vec3,
    pub up: Vec3,
}

impl Camera {
    pub fn new(eye: Vec3, center: Vec3, up: Vec3) -> Self {
        Camera { eye, center, up }
    }

    pub fn basis_change(&self, vector: &Vec3) -> Vec3 {
        let forward = (self.center - self.eye).normalize();
        let right = forward.cross(&self.up).normalize();

        let up = right.cross(&forward).normalize();

        let rotated = vector.x * right + vector.y * up - vector.z * forward;

        rotated.normalize()
    }

    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        let radius_vector = self.eye - self.center;
        let radius = radius_vector.magnitude();

        let current_yaw = radius_vector.z.atan2(radius_vector.x);
        let radius_xz =
            (radius_vector.x * radius_vector.x + radius_vector.z * radius_vector.z).sqrt();
        let current_pitch = (-radius_vector.y).atan2(radius_xz);

        let new_yaw = (current_yaw + delta_yaw) % (2.0 * PI);
        let new_pitch = (current_pitch + delta_pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);

        self.eye = self.center
            + Vec3::new(
                radius * new_yaw.cos() * new_pitch.cos(),
                -radius * new_pitch.sin(),
                radius * new_yaw.sin() * new_pitch.cos(),
            );
    }

    /// Acerca (`delta` negativo) o aleja (`delta` positivo) la cámara del centro
    /// de la escena, manteniendo el mismo ángulo de vista — solo cambia el radio
    /// de la esfera sobre la que orbita `orbit()`.
    pub fn zoom(&mut self, delta: f32) {
        let radius_vector = self.eye - self.center;
        let radius = radius_vector.magnitude();
        let new_radius = (radius + delta).clamp(MIN_ZOOM_DISTANCE, MAX_ZOOM_DISTANCE);

        self.eye = self.center + radius_vector.normalize() * new_radius;
    }
}