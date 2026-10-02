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

    /// Aplica un tinte a una textura sin reemplazarla. Blanco conserva sus
    /// colores originales; otros tonos permiten reutilizar una misma textura en
    /// distintas piezas de un objeto, como la hoja y la guarda de una espada.
    pub fn with_tint(mut self, tint: Color) -> Self {
        self.diffuse = tint;
        self
    }

    /// Color difuso base en un punto de la superficie: la textura muestreada en
    /// `uv` si el material tiene una, o el color plano `diffuse` si no.
    pub fn sample_diffuse(&self, uv: (f32, f32)) -> Color {
        match self.texture {
            Some(texture) => texture.sample(uv.0, uv.1) * self.diffuse,
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

// `Sync` permite repartir las filas del render entre los hilos de la biblioteca
// estandar. Los objetos solo se leen durante el render.
pub trait RayIntersect: Sync {
    fn ray_intersect(&self, ray_origin: &Vec3, ray_direction: &Vec3) -> Option<Intersect>;
}
