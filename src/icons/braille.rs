//! Icono convertido a caracteres braille (2×4 puntos por celda).

use tui_themes::color::to_ansi256;

/// Bit de cada punto dentro de una celda braille, indexado por `[fila][columna]`.
const DOT_BITS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// Icono listo para dibujar: una máscara braille por celda y un color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrailleIcon {
    /// Ancho y alto en celdas de terminal.
    pub width: u16,
    pub height: u16,
    /// Máscara de puntos de cada celda, por filas.
    pub cells: Vec<u8>,
    pub color: (u8, u8, u8),
}

impl BrailleIcon {
    /// Construye el icono desde un mapa de puntos de `2·width` × `4·height`.
    pub fn from_dots(dots: &[bool], width: u16, height: u16, color: (u8, u8, u8)) -> Self {
        let (w, h) = (width as usize, height as usize);
        let mut cells = vec![0u8; w * h];
        if dots.len() >= w * 2 * h * 4 {
            for (i, cell) in cells.iter_mut().enumerate() {
                let (cx, cy) = (i % w, i / w);
                for (row, bits) in DOT_BITS.iter().enumerate() {
                    for (col, bit) in bits.iter().enumerate() {
                        if dots[(cy * 4 + row) * (w * 2) + cx * 2 + col] {
                            *cell |= bit;
                        }
                    }
                }
            }
        }
        Self {
            width,
            height,
            cells,
            color,
        }
    }

    /// Carácter de una celda; las celdas vacías son un espacio.
    pub fn glyph(mask: u8) -> char {
        if mask == 0 {
            ' '
        } else {
            char::from_u32(0x2800 + mask as u32).unwrap_or(' ')
        }
    }

    /// Una cadena por fila de celdas.
    pub fn rows(&self) -> Vec<String> {
        self.cells
            .chunks(self.width.max(1) as usize)
            .map(|row| row.iter().map(|m| Self::glyph(*m)).collect())
            .collect()
    }

    /// Fracción de puntos encendidos sobre el total del lienzo.
    pub fn density(&self) -> f32 {
        if self.cells.is_empty() {
            return 0.0;
        }
        let lit: u32 = self.cells.iter().map(|m| m.count_ones()).sum();
        lit as f32 / (self.cells.len() * 8) as f32
    }

    pub fn is_blank(&self) -> bool {
        self.cells.iter().all(|m| *m == 0)
    }

    /// Texto plano, sin color.
    pub fn to_plain(&self) -> String {
        let mut out = String::new();
        for row in self.rows() {
            out.push_str(row.trim_end());
            out.push('\n');
        }
        out
    }

    /// Texto con secuencias ANSI: solo color de primer plano, nunca fondo.
    pub fn to_ansi(&self, truecolor: bool) -> String {
        let (r, g, b) = self.color;
        let start = if truecolor {
            format!("\x1b[38;2;{r};{g};{b}m")
        } else {
            format!("\x1b[38;5;{}m", to_ansi256(self.color))
        };
        let mut out = String::new();
        for row in self.rows() {
            out.push_str(&start);
            out.push_str(row.trim_end());
            out.push_str("\x1b[0m\n");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_dots_to_braille_bits() {
        // Una celda: columna izquierda completa + punto inferior derecho.
        let mut dots = vec![false; 8];
        for row in 0..4 {
            dots[row * 2] = true;
        }
        dots[3 * 2 + 1] = true;
        let icon = BrailleIcon::from_dots(&dots, 1, 1, (1, 2, 3));
        assert_eq!(icon.cells, vec![0x01 | 0x02 | 0x04 | 0x40 | 0x80]);
        assert_eq!(icon.rows(), vec!["⣇".to_string()]);
    }

    #[test]
    fn empty_cells_are_spaces_and_layout_is_row_major() {
        let mut dots = vec![false; 4 * 8];
        dots[4 * 4 + 2] = true; // celda (1, 1), punto superior izquierdo
        let icon = BrailleIcon::from_dots(&dots, 2, 2, (0, 0, 0));
        assert_eq!(icon.rows(), vec!["  ".to_string(), " ⠁".to_string()]);
        assert_eq!(icon.to_plain(), "\n ⠁\n");
        assert!((icon.density() - 1.0 / 32.0).abs() < 1e-6);
    }

    #[test]
    fn ansi_output_never_sets_a_background() {
        let icon = BrailleIcon::from_dots(&[true; 8], 1, 1, (255, 0, 0));
        let true_color = icon.to_ansi(true);
        assert_eq!(true_color, "\x1b[38;2;255;0;0m⣿\x1b[0m\n");
        assert_eq!(icon.to_ansi(false), "\x1b[38;5;196m⣿\x1b[0m\n");
        for text in [true_color, icon.to_ansi(false)] {
            assert!(!text.contains("\x1b[48"), "no debe haber color de fondo");
        }
    }

    #[test]
    fn short_dot_buffers_give_a_blank_icon() {
        let icon = BrailleIcon::from_dots(&[true; 3], 2, 2, (0, 0, 0));
        assert!(icon.is_blank());
    }
}
