//! Carga de iconos SVG/PNG a un mapa RGBA y preparación para el análisis.

use std::path::Path;

use resvg::{tiny_skia, usvg};

/// Lado en píxeles al que se rasterizan los SVG antes de recortar y reducir.
const SVG_RENDER_SIZE: u32 = 256;

/// Alfa a partir del cual un píxel cuenta como parte del icono.
pub const ALPHA_INSIDE: u8 = 96;

/// Imagen RGBA de 8 bits, alfa no premultiplicado.
#[derive(Debug, Clone, PartialEq)]
pub struct Rgba {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[u8; 4]>,
}

impl Rgba {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![[0; 4]; width * height],
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> [u8; 4] {
        self.pixels[y * self.width + x]
    }

    #[inline]
    pub fn inside(&self, x: usize, y: usize) -> bool {
        self.pixels[y * self.width + x][3] >= ALPHA_INSIDE
    }

    /// Caja que contiene todos los píxeles visibles: `(x0, y0, x1, y1)` con
    /// los extremos incluidos.
    pub fn content_box(&self) -> Option<(usize, usize, usize, usize)> {
        let mut found: Option<(usize, usize, usize, usize)> = None;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.inside(x, y) {
                    found = Some(match found {
                        None => (x, y, x, y),
                        Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                    });
                }
            }
        }
        found
    }
}

/// Carga un icono desde disco. Devuelve `None` si el formato no se soporta
/// o el archivo está dañado.
pub fn load(path: &Path) -> Option<Rgba> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let data = std::fs::read(path).ok()?;
    match ext.as_str() {
        "svg" => load_svg(&data),
        "png" => load_png(&data),
        _ => None,
    }
}

pub fn load_svg(data: &[u8]) -> Option<Rgba> {
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let longest = size.width().max(size.height());
    if !(longest.is_finite() && longest > 0.0) {
        return None;
    }
    let scale = SVG_RENDER_SIZE as f32 / longest;
    let width = ((size.width() * scale).round() as u32).max(1);
    let height = ((size.height() * scale).round() as u32).max(1);
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia entrega alfa premultiplicado.
    let pixels = pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let a = p[3];
            if a == 0 {
                [0; 4]
            } else {
                let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
                [un(p[0]), un(p[1]), un(p[2]), a]
            }
        })
        .collect();
    Some(Rgba {
        width: width as usize,
        height: height as usize,
        pixels,
    })
}

pub fn load_png(data: &[u8]) -> Option<Rgba> {
    let img = image::load_from_memory_with_format(data, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let (width, height) = (img.width() as usize, img.height() as usize);
    if width == 0 || height == 0 {
        return None;
    }
    Some(Rgba {
        width,
        height,
        pixels: img.pixels().map(|p| p.0).collect(),
    })
}

/// Recorta la imagen a su contenido visible y la escala para que quepa,
/// centrada y sin deformar, en un lienzo de `width` × `height` con `margin`
/// píxeles libres en cada borde.
pub fn fit(src: &Rgba, width: usize, height: usize, margin: usize) -> Rgba {
    let mut out = Rgba::new(width, height);
    let Some((x0, y0, x1, y1)) = src.content_box() else {
        return out;
    };
    let (cw, ch) = ((x1 - x0 + 1) as f64, (y1 - y0 + 1) as f64);
    let avail_w = width.saturating_sub(2 * margin).max(1) as f64;
    let avail_h = height.saturating_sub(2 * margin).max(1) as f64;
    let scale = (avail_w / cw).min(avail_h / ch);
    let (dw, dh) = (cw * scale, ch * scale);
    let (off_x, off_y) = ((width as f64 - dw) / 2.0, (height as f64 - dh) / 2.0);

    for dy in 0..height {
        for dx in 0..width {
            // Huella del píxel destino en coordenadas de la imagen original.
            let sx0 = (dx as f64 - off_x) / scale + x0 as f64;
            let sy0 = (dy as f64 - off_y) / scale + y0 as f64;
            let (sx1, sy1) = (sx0 + 1.0 / scale, sy0 + 1.0 / scale);
            out.pixels[dy * width + dx] = area_average(src, sx0, sy0, sx1, sy1);
        }
    }
    out
}

/// Promedio ponderado por área (con alfa premultiplicado) de un rectángulo.
fn area_average(src: &Rgba, x0: f64, y0: f64, x1: f64, y1: f64) -> [u8; 4] {
    let (w, h) = (src.width as f64, src.height as f64);
    let total = (x1 - x0) * (y1 - y0);
    let (cx0, cy0, cx1, cy1) = (x0.max(0.0), y0.max(0.0), x1.min(w), y1.min(h));
    if total <= 0.0 || cx1 <= cx0 || cy1 <= cy0 {
        return [0; 4];
    }
    let mut acc = [0.0f64; 4];
    let mut y = cy0.floor() as usize;
    while (y as f64) < cy1 {
        let wy = ((y + 1) as f64).min(cy1) - (y as f64).max(cy0);
        let mut x = cx0.floor() as usize;
        while (x as f64) < cx1 {
            let wx = ((x + 1) as f64).min(cx1) - (x as f64).max(cx0);
            let p = src.get(x, y);
            let weight = wx * wy;
            let alpha = p[3] as f64 / 255.0;
            acc[0] += p[0] as f64 * alpha * weight;
            acc[1] += p[1] as f64 * alpha * weight;
            acc[2] += p[2] as f64 * alpha * weight;
            acc[3] += alpha * weight;
            x += 1;
        }
        y += 1;
    }
    if acc[3] <= 1e-9 {
        return [0; 4];
    }
    let channel = |v: f64| (v / acc[3]).round().clamp(0.0, 255.0) as u8;
    [
        channel(acc[0]),
        channel(acc[1]),
        channel(acc[2]),
        (acc[3] / total * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const DISC_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
        <circle cx="50" cy="50" r="30" fill="#e66000"/>
        <rect x="40" y="40" width="20" height="20" fill="#ffffff"/>
    </svg>"##;

    #[test]
    fn renders_svg_with_transparent_background() {
        let img = load_svg(DISC_SVG.as_bytes()).unwrap();
        assert_eq!((img.width, img.height), (256, 256));
        assert_eq!(img.get(0, 0)[3], 0);
        assert_eq!(img.get(128, 60), [0xe6, 0x60, 0x00, 255]);
        assert_eq!(img.get(128, 128), [255, 255, 255, 255]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(load_svg(b"no es svg").is_none());
        assert!(load_png(b"no es png").is_none());
        assert!(load(Path::new("/no/existe.svg")).is_none());
    }

    #[test]
    fn decodes_png() {
        let mut bytes = Vec::new();
        let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
        image::DynamicImage::ImageRgba8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let out = load_png(&bytes).unwrap();
        assert_eq!((out.width, out.height), (3, 2));
        assert_eq!(out.get(2, 1), [10, 20, 30, 255]);
    }

    #[test]
    fn fit_crops_to_content_and_centers() {
        let img = load_svg(DISC_SVG.as_bytes()).unwrap();
        let out = fit(&img, 64, 64, 2);
        let (x0, y0, x1, y1) = out.content_box().unwrap();
        // El disco ocupaba el 60 % del lienzo; ahora lo llena salvo el margen.
        assert!(
            x0 <= 3 && y0 <= 3 && x1 >= 60 && y1 >= 60,
            "{x0} {y0} {x1} {y1}"
        );
        assert_eq!(out.get(0, 0)[3], 0);
        assert_eq!(out.get(32, 32), [255, 255, 255, 255]);
    }

    #[test]
    fn fit_keeps_aspect_ratio_and_handles_empty_images() {
        let mut wide = Rgba::new(40, 10);
        wide.pixels.fill([200, 0, 0, 255]);
        let out = fit(&wide, 32, 32, 0);
        let (x0, y0, x1, y1) = out.content_box().unwrap();
        assert_eq!((x0, x1), (0, 31));
        assert_eq!((y0, y1), (12, 19));
        assert!(fit(&Rgba::new(8, 8), 16, 16, 1).content_box().is_none());
    }
}
