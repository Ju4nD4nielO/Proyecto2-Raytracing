use crate::color::Color;
use crate::texture::Texture;
use nalgebra_glm::Vec3;

/// Índices del arreglo `albedo`, para no tener que recordar el orden de memoria
/// en cada lugar donde se lee o escribe.
pub const DIFFUSE: usize = 0;
pub const SPECULAR: usize = 1;
pub const REFLECTIVITY: usize = 2;
pub const TRANSPARENCY: usize = 3;

#[derive(Debug, Clone, Copy)]
pub struct Material {
    pub diffuse: Color,
    pub specular: f32,
    /// [peso difuso, peso especular, reflectividad, transparencia]. Cada material
    /// trae sus propios cuatro pesos, sin importar qué otros materiales existan.
    pub albedo: [f32; 4],
    /// Índice de refracción (ley de Snell). Solo se usa cuando `transparency > 0`.
    pub refractive_index: f32,
    /// Textura propia del material. Es `&'static` porque todas las texturas se
    /// cargan una sola vez al arrancar el programa y viven durante toda su
    /// ejecución (ver `Box::leak` en `main.rs`) — así `Material` se mantiene
    /// `Copy`, igual que antes de agregar texturas.
    pub texture: Option<&'static Texture>,
}

impl Material {
    pub fn new(diffuse: Color, specular: f32, albedo: [f32; 4]) -> Self {
        Material {
            diffuse,
            specular,
            albedo,
            refractive_index: 1.0,
            texture: None,
        }
    }

    pub fn new_textured(
        texture: &'static Texture,
        specular: f32,
        albedo: [f32; 4],
        refractive_index: f32,
    ) -> Self {
        Material {
            diffuse: Color::new(255, 255, 255),
            specular,
            albedo,
            refractive_index,
            texture: Some(texture),
        }
    }

    /// Fija el índice de refracción de un material ya construido (estilo builder,
    /// para no tener que multiplicar constructores por cada combinación posible).
    pub fn with_refractive_index(mut self, refractive_index: f32) -> Self {
        self.refractive_index = refractive_index;
        self
    }

    /// Color difuso base en un punto de la superficie: la textura muestreada en
    /// `uv` si el material tiene una, o el color plano `diffuse` si no.
    pub fn sample_diffuse(&self, uv: (f32, f32)) -> Color {
        match self.texture {
            Some(texture) => texture.sample(uv.0, uv.1),
            None => self.diffuse,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Intersect {
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f32,
    pub material: Material,
    pub uv: (f32, f32),
}

pub trait RayIntersect {
    fn ray_intersect(&self, ray_origin: &Vec3, ray_direction: &Vec3) -> Option<Intersect>;
}