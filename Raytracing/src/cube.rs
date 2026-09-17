use crate::ray_intersect::{Intersect, Material, RayIntersect};
use nalgebra_glm::Vec3;

const EPSILON: f32 = 1e-4;

/// Cubo alineado a los ejes (AABB), definido por sus esquinas mínima y máxima.
/// Cada una de las 6 caras se mapea a su propio espacio UV en [0, 1] para poder
/// texturizar el cubo como un bloque tipo Minecraft (cada cara "desenrollada" por
/// separado, no una textura envolvente continua).
pub struct Cube {
    pub min: Vec3,
    pub max: Vec3,
    pub material: Material,
}

impl Cube {
    /// Crea un cubo de lado `size` centrado en `center`.
    pub fn new(center: Vec3, size: f32, material: Material) -> Self {
        let half = size / 2.0;
        Cube {
            min: center - Vec3::new(half, half, half),
            max: center + Vec3::new(half, half, half),
            material,
        }
    }

    /// Crea un cubo (o prisma rectangular) a partir de sus esquinas directamente,
    /// útil cuando el bloque no es perfectamente cúbico (p. ej. un piso delgado).
    pub fn from_bounds(min: Vec3, max: Vec3, material: Material) -> Self {
        Cube { min, max, material }
    }

    /// Dado un punto sobre la superficie y la normal de la cara que se golpeó,
    /// calcula coordenadas UV locales a esa cara, normalizadas a [0, 1].
    fn face_uv(&self, point: &Vec3, normal: &Vec3) -> (f32, f32) {
        let size = self.max - self.min;
        let local = point - self.min;

        if normal.x.abs() > 0.5 {
            (local.z / size.z, local.y / size.y)
        } else if normal.y.abs() > 0.5 {
            (local.x / size.x, local.z / size.z)
        } else {
            (local.x / size.x, local.y / size.y)
        }
    }
}

impl RayIntersect for Cube {
    fn ray_intersect(&self, ray_origin: &Vec3, ray_direction: &Vec3) -> Option<Intersect> {
        // Método de slabs: para cada eje (x, y, z) el rayo entra y sale de la franja
        // definida por [min[axis], max[axis]] en algún t. La intersección con el cubo
        // es donde esos tres intervalos se solapan. Guardamos tanto el t de entrada
        // (t_near) como el de salida (t_far) junto con el eje/signo que los produjo,
        // porque un rayo que nace dentro del cubo (p. ej. saliendo de un material
        // refractivo) debe usar la cara de salida, no la de entrada.
        let mut t_near = f32::NEG_INFINITY;
        let mut t_far = f32::INFINITY;

        let mut near_axis = 0usize;
        let mut near_sign = -1.0f32;
        let mut far_axis = 0usize;
        let mut far_sign = 1.0f32;

        for axis in 0..3 {
            let origin = ray_origin[axis];
            let direction = ray_direction[axis];

            if direction.abs() < EPSILON {
                // Rayo paralelo a esta pareja de planos: si el origen está fuera de la
                // franja, nunca puede tocar el cubo.
                if origin < self.min[axis] || origin > self.max[axis] {
                    return None;
                }
                continue;
            }

            let inv_d = 1.0 / direction;
            let (t0, t1, sign0, sign1) = if inv_d >= 0.0 {
                (
                    (self.min[axis] - origin) * inv_d,
                    (self.max[axis] - origin) * inv_d,
                    -1.0,
                    1.0,
                )
            } else {
                (
                    (self.max[axis] - origin) * inv_d,
                    (self.min[axis] - origin) * inv_d,
                    1.0,
                    -1.0,
                )
            };

            if t0 > t_near {
                t_near = t0;
                near_axis = axis;
                near_sign = sign0;
            }

            if t1 < t_far {
                t_far = t1;
                far_axis = axis;
                far_sign = sign1;
            }

            if t_near > t_far {
                return None;
            }
        }

        let (distance, axis, sign) = if t_near > EPSILON {
            (t_near, near_axis, near_sign)
        } else if t_far > EPSILON {
            (t_far, far_axis, far_sign)
        } else {
            return None;
        };

        let point = ray_origin + ray_direction * distance;

        let mut normal = Vec3::new(0.0, 0.0, 0.0);
        normal[axis] = sign;

        let uv = self.face_uv(&point, &normal);

        Some(Intersect {
            point,
            normal,
            distance,
            material: self.material,
            uv,
        })
    }
}