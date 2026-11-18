//! Color dominante de un icono.

use super::raster::{Rgba, ALPHA_INSIDE};

/// Color usado cuando el icono no tiene ningún color propio (monocromo).
pub const NEUTRAL: (u8, u8, u8) = tui_themes::color::icon::NEUTRAL;

/// Luminancia mínima del color devuelto, para que el contorno se lea sobre
/// una terminal oscura.
const MIN_LUMA: f32 = 110.0;

fn luma(r: f32, g: f32, b: f32) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

/// Número de sectores del histograma de tonos.
const HUE_BINS: usize = 36;

/// Tono (0..360), saturación y valor de un color RGB.
fn hsv(p: [u8; 4]) -> (f32, f32, f32) {
    let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let delta = max - min;
    if delta <= 0.0 {
        return (0.0, 0.0, max / 255.0);
    }
    let hue = if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    (hue.rem_euclid(360.0), delta / max, max / 255.0)
}

/// Color dominante: el tono con más peso entre los píxeles visibles,
/// ignorando transparentes y los casi blancos, casi negros y grises.
///
/// Los píxeles se reparten en un histograma de tonos y cada uno pesa según
/// su saturación y brillo: así un detalle de color vivo le gana a un fondo
/// gris apenas teñido, y un degradado no se dispersa entre muchos colores.
/// El resultado es el promedio de los píxeles del tono ganador.
pub fn dominant(img: &Rgba) -> (u8, u8, u8) {
    let mut bins = [(0.0f32, [0.0f32; 3]); HUE_BINS];
    for p in &img.pixels {
        if p[3] < ALPHA_INSIDE {
            continue;
        }
        let (max, min) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2]));
        if min >= 232 || max <= 28 {
            continue;
        }
        let (hue, saturation, value) = hsv(*p);
        if saturation < 0.12 {
            continue;
        }
        let weight = saturation * saturation * (0.3 + 0.7 * value);
        let entry = &mut bins[(hue / 360.0 * HUE_BINS as f32) as usize % HUE_BINS];
        entry.0 += weight;
        for (sum, channel) in entry.1.iter_mut().zip(p) {
            *sum += *channel as f32 * weight;
        }
    }
    // Suaviza con los sectores vecinos para no partir un tono en dos.
    let score = |i: usize| {
        bins[i].0 + 0.5 * (bins[(i + 1) % HUE_BINS].0 + bins[(i + HUE_BINS - 1) % HUE_BINS].0)
    };
    let best = (0..HUE_BINS)
        .filter(|i| bins[*i].0 > 0.0)
        .max_by(|a, b| score(*a).total_cmp(&score(*b)));
    let Some(best) = best else {
        return NEUTRAL;
    };
    let (weight, sum) = bins[best];
    legible(sum[0] / weight, sum[1] / weight, sum[2] / weight)
}

/// Aclara un color demasiado oscuro conservando el tono.
fn legible(r: f32, g: f32, b: f32) -> (u8, u8, u8) {
    let l = luma(r, g, b);
    let (r, g, b) = if l < MIN_LUMA {
        // Mezcla con blanco hasta alcanzar la luminancia mínima.
        let t = (MIN_LUMA - l) / (255.0 - l).max(1.0);
        (
            r + (255.0 - r) * t,
            g + (255.0 - g) * t,
            b + (255.0 - b) * t,
        )
    } else {
        (r, g, b)
    };
    (r.round() as u8, g.round() as u8, b.round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(parts: &[([u8; 4], usize)]) -> Rgba {
        let pixels: Vec<[u8; 4]> = parts
            .iter()
            .flat_map(|(c, n)| std::iter::repeat_n(*c, *n))
            .collect();
        Rgba {
            width: pixels.len(),
            height: 1,
            pixels,
        }
    }

    #[test]
    fn picks_the_most_common_colour() {
        let img = image(&[([230, 96, 0, 255], 60), ([40, 90, 220, 255], 30)]);
        assert_eq!(dominant(&img), (230, 96, 0));
    }

    #[test]
    fn ignores_transparent_white_black_and_grey() {
        let img = image(&[
            ([255, 255, 255, 255], 500),
            ([0, 0, 0, 255], 500),
            ([128, 128, 128, 255], 500),
            ([10, 200, 10, 0], 500),
            ([230, 80, 180, 255], 20),
        ]);
        assert_eq!(dominant(&img), (230, 80, 180));
    }

    #[test]
    fn a_vivid_detail_beats_a_dull_background() {
        let img = image(&[([103, 112, 117, 255], 900), ([240, 120, 20, 255], 60)]);
        assert_eq!(dominant(&img), (240, 120, 20));
    }

    #[test]
    fn monochrome_icons_get_the_neutral_colour() {
        let img = image(&[([0, 0, 0, 255], 50), ([255, 255, 255, 255], 50)]);
        assert_eq!(dominant(&img), NEUTRAL);
        assert_eq!(dominant(&Rgba::new(4, 4)), NEUTRAL);
    }

    #[test]
    fn dark_colours_are_lightened_keeping_the_hue() {
        let (r, g, b) = dominant(&image(&[([10, 20, 90, 255], 10)]));
        assert!(luma(r as f32, g as f32, b as f32) >= MIN_LUMA - 1.5);
        assert!(b > r && b > g);
    }
}
