use crate::color::Color;
use std::f32::consts::PI;
use std::fs;
use std::io;

/// Textura 2D simple: un buffer de píxeles en memoria muestreable con coordenadas
/// UV en [0, 1]. Solo soporta PPM binario (P6) porque no podemos depender de un
/// crate externo para decodificar PNG/JPEG — el enunciado prohíbe librerías fuera
/// del lenguaje. Puedes generar PPMs exportando desde GIMP/Photoshop, o con
/// ImageMagick: `magick textura.png -compress none textura.ppm`.
#[derive(Debug)]
pub struct Texture {
    width: usize,
    height: usize,
    pixels: Vec<Color>,
}

impl Texture {
    /// Textura de un solo color — útil como placeholder mientras no hay arte final.
    pub fn solid(color: Color) -> Self {
        Texture {
            width: 1,
            height: 1,
            pixels: vec![color],
        }
    }

    /// Tablero de ajedrez procedural de `cells x cells` casillas. Sirve para
    /// verificar visualmente que el mapeo UV de una cara está bien orientado
    /// (sin estirones ni espejeos raros) antes de tener texturas reales.
    pub fn checkerboard(cells: usize, a: Color, b: Color) -> Self {
        let cells = cells.max(1);
        let resolution = cells * 32;
        let mut pixels = Vec::with_capacity(resolution * resolution);

        for y in 0..resolution {
            for x in 0..resolution {
                let cell_x = (x * cells) / resolution;
                let cell_y = (y * cells) / resolution;
                pixels.push(if (cell_x + cell_y) % 2 == 0 { a } else { b });
            }
        }

        Texture {
            width: resolution,
            height: resolution,
            pixels,
        }
    }

    /// Pasto de campo: verde base con ruido por pixel para simular briznas, más
    /// algunas franjas verticales un poco más oscuras. Se usa tanto para el piso
    /// como para el follaje del árbol.
    pub fn grass() -> Self {
        let resolution = 64;
        let mut pixels = Vec::with_capacity(resolution * resolution);

        for y in 0..resolution {
            for x in 0..resolution {
                let n = value_at(x, y, 0xA17F);
                let blade = if (x * 7 + y * 3) % 11 == 0 { 0.15 } else { 0.0 };
                let shade = 0.85 + n * 0.3 - blade;

                pixels.push(shade_color(70.0, 150.0, 60.0, shade));
            }
        }

        Texture { width: resolution, height: resolution, pixels }
    }

    /// Piedra de santuario: patrón de ladrillos con ruido de superficie y un
    /// acento celeste al centro de cada ladrillo (guiño a la tecnología Sheikah).
    pub fn stone_bricks() -> Self {
        let resolution = 64;
        let brick_w = 16;
        let brick_h = 8;
        let mut pixels = Vec::with_capacity(resolution * resolution);

        for y in 0..resolution {
            for x in 0..resolution {
                let row = y / brick_h;
                let offset = if row % 2 == 0 { 0 } else { brick_w / 2 };
                let bx = (x + offset) % brick_w;
                let by = y % brick_h;
                let is_mortar = bx == 0 || by == 0;
                let is_accent = !is_mortar && bx == brick_w / 2 && by == brick_h / 2;

                let n = value_at(x, y, 0x57E11);
                let color = if is_accent {
                    Color::new(90, 210, 225)
                } else if is_mortar {
                    Color::new(55, 55, 62)
                } else {
                    let shade = 0.85 + n * 0.3;
                    shade_color(140.0, 140.0, 148.0, shade)
                };

                pixels.push(color);
            }
        }

        Texture { width: resolution, height: resolution, pixels }
    }

    /// Madera en tablones verticales, con vetas onduladas y una línea oscura en
    /// cada unión de tablón. Se usa para el tronco del árbol y el cofre.
    pub fn wood_planks() -> Self {
        let resolution = 64;
        let plank_w = 16;
        let mut pixels = Vec::with_capacity(resolution * resolution);

        for y in 0..resolution {
            for x in 0..resolution {
                let plank_edge = x % plank_w == 0;
                let n = value_at(x, y, 0x900D);
                let grain = ((y as f32 * 0.4 + n * 6.0).sin() * 0.5 + 0.5) * 0.15;
                let shade = 0.9 + n * 0.2 - grain;

                let color = if plank_edge {
                    Color::new(55, 38, 24)
                } else {
                    shade_color(150.0, 100.0, 55.0, shade)
                };

                pixels.push(color);
            }
        }

        Texture { width: resolution, height: resolution, pixels }
    }

    /// Superficie de agua: gradiente azul con ondas superpuestas (senos cruzados)
    /// para dar sensación de ondulación sin necesitar animación.
    pub fn water_ripples() -> Self {
        let resolution = 64;
        let mut pixels = Vec::with_capacity(resolution * resolution);

        for y in 0..resolution {
            for x in 0..resolution {
                let fx = x as f32 / resolution as f32;
                let fy = y as f32 / resolution as f32;
                let wave = ((fx * 20.0).sin() + (fy * 20.0 + fx * 8.0).sin()) * 0.5;
                let shade = 0.8 + wave * 0.15;

                pixels.push(shade_color(40.0, 110.0, 200.0, shade));
            }
        }

        Texture { width: resolution, height: resolution, pixels }
    }

    /// Metal plateado-azulado con una banda de brillo diagonal, como el filo de
    /// una espada legendaria.
    pub fn metal_shine() -> Self {
        let resolution = 64;
        let mut pixels = Vec::with_capacity(resolution * resolution);

        for y in 0..resolution {
            for x in 0..resolution {
                let fx = x as f32 / resolution as f32;
                let fy = y as f32 / resolution as f32;
                let diagonal = ((fx + fy) * PI * 2.0).sin() * 0.5 + 0.5;
                let n = value_at(x, y, 0x5157);
                let base = 150.0 + diagonal * 90.0 + n * 10.0;

                pixels.push(Color::new(
                    base.clamp(0.0, 255.0) as u8,
                    (base + 10.0).clamp(0.0, 255.0) as u8,
                    (base + 35.0).clamp(0.0, 255.0) as u8,
                ));
            }
        }

        Texture { width: resolution, height: resolution, pixels }
    }

    /// Carga una imagen PPM binaria (P6) desde disco.
    pub fn from_ppm(path: &str) -> io::Result<Self> {
        let data = fs::read(path)?;
        let mut pos = 0usize;

        let magic = read_token(&data, &mut pos);
        if magic != "P6" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{path}: solo se soporta PPM binario (P6), encontrado '{magic}'"),
            ));
        }

        let width: usize = read_token(&data, &mut pos)
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "ancho de PPM inválido"))?;
        let height: usize = read_token(&data, &mut pos)
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "alto de PPM inválido"))?;
        let _maxval: usize = read_token(&data, &mut pos)
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "maxval de PPM inválido"))?;

        // Justo después del token de maxval hay un solo byte de espacio en blanco,
        // y a partir de ahí arranca la data binaria cruda (sin más separadores).
        pos += 1;

        let expected_bytes = width * height * 3;
        if data.len() < pos + expected_bytes {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("{path}: el archivo PPM está truncado"),
            ));
        }

        let mut pixels = Vec::with_capacity(width * height);
        for i in 0..(width * height) {
            let offset = pos + i * 3;
            pixels.push(Color::new(data[offset], data[offset + 1], data[offset + 2]));
        }

        Ok(Texture {
            width,
            height,
            pixels,
        })
    }

    /// Muestrea la textura en coordenadas UV (0.0–1.0). Las coordenadas fuera de
    /// rango se envuelven (wrap) en vez de recortarse. Usa vecino más cercano
    /// (sin interpolación) a propósito, para mantener el look "de bloque".
    pub fn sample(&self, u: f32, v: f32) -> Color {
        let u = u.rem_euclid(1.0);
        let v = v.rem_euclid(1.0);

        let x = ((u * self.width as f32) as usize).min(self.width - 1);
        let y = (((1.0 - v) * self.height as f32) as usize).min(self.height - 1);

        self.pixels[y * self.width + x]
    }
}

/// Hash entero simple (variante de un mix de 32 bits) — determinista, sin estado,
/// sin necesitar ningún crate de números aleatorios.
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Valor pseudoaleatorio determinista en [0, 1] para el pixel (x, y), usado como
/// ruido de superficie en las texturas procedurales.
fn value_at(x: usize, y: usize, seed: u32) -> f32 {
    let h = hash((x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263) ^ seed);
    (h % 1000) as f32 / 1000.0
}

/// Escala un color base RGB por un factor de sombreado y lo recorta a [0, 255].
fn shade_color(r: f32, g: f32, b: f32, shade: f32) -> Color {
    Color::new(
        (r * shade).clamp(0.0, 255.0) as u8,
        (g * shade).clamp(0.0, 255.0) as u8,
        (b * shade).clamp(0.0, 255.0) as u8,
    )
}

/// Lee el siguiente token no-blanco de un header PPM, saltando comentarios `# ...`.
fn read_token(data: &[u8], pos: &mut usize) -> String {
    loop {
        while *pos < data.len() && data[*pos].is_ascii_whitespace() {
            *pos += 1;
        }

        if *pos < data.len() && data[*pos] == b'#' {
            while *pos < data.len() && data[*pos] != b'\n' {
                *pos += 1;
            }
            continue;
        }

        break;
    }

    let start = *pos;
    while *pos < data.len() && !data[*pos].is_ascii_whitespace() {
        *pos += 1;
    }

    String::from_utf8_lossy(&data[start..*pos]).into_owned()
}