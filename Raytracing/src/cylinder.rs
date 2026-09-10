use crate::ray_intersect::{Intersect, Material, RayIntersect};
use nalgebra_glm::{dot, Vec3};

const EPSILON: f32 = 1e-4;

pub struct Cylinder {
    pub base: Vec3,
    pub axis: Vec3,
    pub height: f32,
    pub radius: f32,
    pub material: Material,
}

impl Cylinder {
    pub fn new(base: Vec3, axis: Vec3, height: f32, radius: f32, material: Material) -> Self {
        Cylinder {
            base,
            axis: axis.normalize(),
            height,
            radius,
            material,
        }
    }
}

fn closest(current: Option<(f32, Vec3)>, t: f32, normal: Vec3) -> Option<(f32, Vec3)> {
    if t > EPSILON && current.is_none_or(|(best, _)| t < best) {
        Some((t, normal))
    } else {
        current
    }
}

impl RayIntersect for Cylinder {
    fn ray_intersect(&self, ray_origin: &Vec3, ray_direction: &Vec3) -> Option<Intersect> {
        let oc = ray_origin - self.base;

        let d_axis = dot(ray_direction, &self.axis);
        let oc_axis = dot(&oc, &self.axis);

        let d_perp = ray_direction - d_axis * self.axis;
        let oc_perp = oc - oc_axis * self.axis;

        let mut hit: Option<(f32, Vec3)> = None;

        let a = dot(&d_perp, &d_perp);

        if a > EPSILON {
            let b = 2.0 * dot(&d_perp, &oc_perp);
            let c = dot(&oc_perp, &oc_perp) - self.radius * self.radius;

            let discriminant = b * b - 4.0 * a * c;

            if discriminant > 0.0 {
                let root = discriminant.sqrt();

                for t in [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)] {
                    let height = oc_axis + t * d_axis;

                    if (0.0..=self.height).contains(&height) {
                        let point = oc + ray_direction * t;

                        let normal = (point - height * self.axis) / self.radius;

                        hit = closest(hit, t, normal);
                    }
                }
            }
        }

        if d_axis.abs() > EPSILON {
            for (height, normal) in [(0.0, -self.axis), (self.height, self.axis)] {
                let t = (height - oc_axis) / d_axis;
                let point = oc + ray_direction * t;

                let radial = point - height * self.axis;

                if dot(&radial, &radial) <= self.radius * self.radius {
                    hit = closest(hit, t, normal);
                }
            }
        }

        let (distance, normal) = hit?;

        Some(Intersect {
            point: ray_origin + ray_direction * distance,
            normal,
            distance,
            material: self.material,
        })
    }
}