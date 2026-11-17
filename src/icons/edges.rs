//! Detección de bordes: silueta por canal alfa más bordes de color internos
//! (Sobel + supresión de no-máximos + histéresis, al estilo Canny).

use super::raster::Rgba;

/// Umbral alto inicial del gradiente de color. Un escalón de Δ en un canal
/// sobre un borde recto da una magnitud de Sobel de 4·Δ.
const HIGH_THRESHOLD: f32 = 70.0;

/// Fracción máxima del lienzo que pueden ocupar los bordes internos. Si se
/// supera, el umbral sube: un icono muy detallado no debe verse relleno.
const MAX_INNER_DENSITY: f32 = 0.085;

/// Devuelve, para cada píxel, si pertenece a un borde.
pub fn detect(img: &Rgba) -> Vec<bool> {
    let (w, h) = (img.width, img.height);
    let mut edges = vec![false; w * h];
    if w < 3 || h < 3 {
        return edges;
    }
    let inside: Vec<bool> = img
        .pixels
        .iter()
        .map(|p| p[3] >= super::raster::ALPHA_INSIDE)
        .collect();
    let at = |x: isize, y: isize| -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < w
            && (y as usize) < h
            && inside[y as usize * w + x as usize]
    };

    // 1. Silueta: píxeles interiores con algún vecino (4-conexo) exterior.
    let mut near_outside = vec![false; w * h];
    for y in 0..h as isize {
        for x in 0..w as isize {
            let i = y as usize * w + x as usize;
            if !inside[i] {
                continue;
            }
            if !(at(x - 1, y) && at(x + 1, y) && at(x, y - 1) && at(x, y + 1)) {
                edges[i] = true;
            }
            // Zona de 2 px junto al exterior: ahí el antialiasing produciría
            // un segundo contorno pegado a la silueta.
            near_outside[i] = (-2..=2).any(|dy| (-2..=2).any(|dx| !at(x + dx, y + dy)));
        }
    }

    // 2. Gradiente de color (Sobel por canal) en el interior.
    let mut mag = vec![0.0f32; w * h];
    let mut dir = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if !inside[i] || near_outside[i] {
                continue;
            }
            let center = img.pixels[i];
            let sample = |dx: isize, dy: isize| -> [f32; 3] {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                let p = if at(nx, ny) {
                    img.pixels[ny as usize * w + nx as usize]
                } else {
                    center
                };
                [p[0] as f32, p[1] as f32, p[2] as f32]
            };
            let n = [
                sample(-1, -1),
                sample(0, -1),
                sample(1, -1),
                sample(-1, 0),
                sample(1, 0),
                sample(-1, 1),
                sample(0, 1),
                sample(1, 1),
            ];
            let (mut best, mut best_gx, mut best_gy, mut total) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            // `c` recorre los canales R, G, B de los ocho vecinos.
            #[allow(clippy::needless_range_loop)]
            for c in 0..3 {
                let v = |k: usize| n[k][c];
                let gx = (v(2) + 2.0 * v(4) + v(7)) - (v(0) + 2.0 * v(3) + v(5));
                let gy = (v(5) + 2.0 * v(6) + v(7)) - (v(0) + 2.0 * v(1) + v(2));
                let m2 = gx * gx + gy * gy;
                total += m2;
                if m2 > best {
                    (best, best_gx, best_gy) = (m2, gx, gy);
                }
            }
            mag[i] = total.sqrt();
            // Dirección del gradiente en 4 sectores: 0 = →, 1 = ↘, 2 = ↓, 3 = ↙.
            let angle = best_gy.atan2(best_gx).to_degrees().rem_euclid(180.0);
            dir[i] = match angle {
                a if !(22.5..157.5).contains(&a) => 0,
                a if a < 67.5 => 1,
                a if a < 112.5 => 2,
                _ => 3,
            };
        }
    }

    // 3. Supresión de no-máximos: deja líneas de un píxel de grosor.
    let mut thin = vec![0.0f32; w * h];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let m = mag[i];
            if m <= 0.0 {
                continue;
            }
            let (a, b) = match dir[i] {
                0 => (i - 1, i + 1),
                1 => (i - w - 1, i + w + 1),
                2 => (i - w, i + w),
                _ => (i - w + 1, i + w - 1),
            };
            if m > mag[a] && m >= mag[b] {
                thin[i] = m;
            }
        }
    }

    // 4. Histéresis con umbral adaptativo a la densidad.
    let budget = (MAX_INNER_DENSITY * (w * h) as f32) as usize;
    let mut high = HIGH_THRESHOLD;
    let mut inner = hysteresis(&thin, w, h, high);
    for _ in 0..8 {
        if inner.iter().filter(|e| **e).count() <= budget {
            break;
        }
        high *= 1.3;
        inner = hysteresis(&thin, w, h, high);
    }
    for (edge, add) in edges.iter_mut().zip(inner) {
        *edge |= add;
    }
    edges
}

/// Conserva los píxeles fuertes y los débiles conectados a alguno fuerte.
fn hysteresis(thin: &[f32], w: usize, h: usize, high: f32) -> Vec<bool> {
    let low = high * 0.5;
    let mut out = vec![false; w * h];
    let mut stack: Vec<usize> = thin
        .iter()
        .enumerate()
        .filter(|(_, m)| **m >= high)
        .map(|(i, _)| i)
        .collect();
    for &i in &stack {
        out[i] = true;
    }
    while let Some(i) = stack.pop() {
        let (x, y) = ((i % w) as isize, (i / w) as isize);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if !out[j] && thin[j] >= low {
                    out[j] = true;
                    stack.push(j);
                }
            }
        }
    }
    out
}

/// Reduce un mapa de bordes a la mitad: un punto se enciende si alguno de
/// sus 2×2 píxeles es borde. Después elimina puntos sueltos.
pub fn downsample(edges: &[bool], w: usize, h: usize) -> (Vec<bool>, usize, usize) {
    let (dw, dh) = (w / 2, h / 2);
    let mut dots = vec![false; dw * dh];
    for y in 0..dh {
        for x in 0..dw {
            let i = 2 * y * w + 2 * x;
            dots[y * dw + x] = edges[i] || edges[i + 1] || edges[i + w] || edges[i + w + 1];
        }
    }
    let lit = |x: isize, y: isize| -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < dw
            && (y as usize) < dh
            && dots[y as usize * dw + x as usize]
    };
    let mut clean = dots.clone();
    for y in 0..dh as isize {
        for x in 0..dw as isize {
            if lit(x, y) {
                let neighbours = (-1..=1)
                    .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                    .filter(|&(dx, dy)| (dx, dy) != (0, 0) && lit(x + dx, y + dy))
                    .count();
                if neighbours == 0 {
                    clean[y as usize * dw + x as usize] = false;
                }
            }
        }
    }
    (clean, dw, dh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(size: usize, lo: usize, hi: usize, color: [u8; 4]) -> Rgba {
        let mut img = Rgba::new(size, size);
        for y in lo..hi {
            for x in lo..hi {
                img.pixels[y * size + x] = color;
            }
        }
        img
    }

    #[test]
    fn solid_shape_yields_only_its_outline() {
        let img = square(32, 8, 24, [200, 30, 30, 255]);
        let edges = detect(&img);
        let at = |x: usize, y: usize| edges[y * 32 + x];
        assert!(at(8, 8) && at(23, 8) && at(8, 16) && at(16, 23));
        assert!(!at(16, 16), "el interior no se rellena");
        assert!(!at(4, 4), "el fondo queda vacío");
        // Perímetro de un cuadrado de 16 px: 4·16 − 4.
        assert_eq!(edges.iter().filter(|e| **e).count(), 60);
    }

    #[test]
    fn colour_boundaries_inside_the_shape_are_detected() {
        let mut img = square(32, 2, 30, [20, 60, 200, 255]);
        for y in 2..30 {
            for x in 16..30 {
                img.pixels[y * 32 + x] = [240, 220, 40, 255];
            }
        }
        let edges = detect(&img);
        let row: Vec<usize> = (0..32).filter(|x| edges[16 * 32 + x]).collect();
        // Silueta a izquierda y derecha, y una línea fina en el cambio de color.
        assert_eq!(row.first(), Some(&2));
        assert_eq!(row.last(), Some(&29));
        let middle: Vec<&usize> = row.iter().filter(|x| (14..=17).contains(*x)).collect();
        assert_eq!(middle.len(), 1, "{row:?}");
    }

    #[test]
    fn smooth_gradients_do_not_create_edges() {
        let mut img = Rgba::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                img.pixels[y * 32 + x] = [(x * 6) as u8, 80, (y * 6) as u8, 255];
            }
        }
        let edges = detect(&img);
        let inner = (4..28)
            .flat_map(|y| (4..28).map(move |x| (x, y)))
            .filter(|&(x, y)| edges[y * 32 + x])
            .count();
        assert_eq!(inner, 0);
    }

    #[test]
    fn busy_images_are_capped() {
        // Tablero de ajedrez de 2 px: bordes por todas partes.
        let mut img = Rgba::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let v = if (x / 2 + y / 2) % 2 == 0 { 255 } else { 0 };
                img.pixels[y * 64 + x] = [v, v, v, 255];
            }
        }
        let edges = detect(&img);
        let count = edges.iter().filter(|e| **e).count();
        assert!(count < 64 * 64 / 4, "{count}");
    }

    #[test]
    fn downsample_halves_and_drops_specks() {
        let mut edges = vec![false; 8 * 8];
        edges[0] = true; // punto aislado
        edges[4 * 8 + 4] = true;
        edges[4 * 8 + 6] = true;
        let (dots, w, h) = downsample(&edges, 8, 8);
        assert_eq!((w, h), (4, 4));
        assert!(!dots[0]);
        assert!(dots[2 * 4 + 2] && dots[2 * 4 + 3]);
    }

    #[test]
    fn tiny_images_do_not_panic() {
        assert!(detect(&Rgba::new(2, 2)).iter().all(|e| !e));
        assert!(detect(&Rgba::new(0, 0)).is_empty());
    }
}
