use crate::color::Color;
use std::fs;
use std::io;

/// Textura 2D simple: un buffer de píxeles en memoria muestreable con coordenadas
/// UV en [0, 1]. Solo soporta PPM binario (P6) porque no podemos depender de un
/// crate externo para decodificar PNG/JPEG — el enunciado prohíbe librerías fuera
/// del lenguaje. Puedes generar PPMs exportando desde GIMP/Photoshop, o con
/// ImageMagick: `magick textura.png -compress none textura.ppm`.
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