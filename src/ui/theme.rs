//! Colores y estilos de la interfaz.

use ratatui::style::{Color, Modifier, Style};

use crate::icons::color::to_ansi256;

/// Paleta de la interfaz. Nunca define colores de fondo: se respeta el de la
/// terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Usar colores de 24 bits; si no, se degradan a la paleta de 256.
    pub truecolor: bool,
    /// Color de selección y atajos.
    pub accent_rgb: (u8, u8, u8),
}

/// Color de acento de fábrica.
pub const DEFAULT_ACCENT: (u8, u8, u8) = (122, 162, 247);

/// ¿La terminal anuncia soporte de 24 bits?
pub fn terminal_has_truecolor() -> bool {
    std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit")
}

impl Theme {
    /// Detecta truecolor por la variable `COLORTERM`.
    pub fn detect() -> Self {
        Self {
            truecolor: terminal_has_truecolor(),
            accent_rgb: DEFAULT_ACCENT,
        }
    }

    /// Convierte un RGB al mejor color que la terminal soporta.
    pub fn rgb(&self, rgb: (u8, u8, u8)) -> Color {
        if self.truecolor {
            Color::Rgb(rgb.0, rgb.1, rgb.2)
        } else {
            Color::Indexed(to_ansi256(rgb))
        }
    }

    pub fn accent(&self) -> Color {
        self.rgb(self.accent_rgb)
    }

    pub fn text(&self) -> Style {
        Style::default()
    }

    pub fn dim(&self) -> Style {
        Style::default().fg(self.rgb((125, 130, 145)))
    }

    pub fn header(&self) -> Style {
        Style::default()
            .fg(self.rgb((150, 160, 185)))
            .add_modifier(Modifier::BOLD)
    }

    pub fn selected(&self) -> Style {
        Style::default()
            .fg(self.accent())
            .add_modifier(Modifier::BOLD)
    }

    /// Color para un porcentaje de uso sobre el total (0–100).
    pub fn load(&self, pct: f32) -> Color {
        match pct {
            p if p >= 90.0 => self.rgb((247, 118, 142)),
            p if p >= 40.0 => self.rgb((224, 175, 104)),
            _ => self.rgb((158, 206, 106)),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            truecolor: true,
            accent_rgb: DEFAULT_ACCENT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degrades_to_256_colours() {
        let full = Theme::default();
        let low = Theme {
            truecolor: false,
            ..Theme::default()
        };
        assert_eq!(full.rgb((255, 0, 0)), Color::Rgb(255, 0, 0));
        assert_eq!(low.rgb((255, 0, 0)), Color::Indexed(196));
        assert!(matches!(low.accent(), Color::Indexed(_)));
    }

    #[test]
    fn accent_is_configurable() {
        let theme = Theme {
            accent_rgb: (255, 136, 0),
            ..Theme::default()
        };
        assert_eq!(theme.accent(), Color::Rgb(255, 136, 0));
        assert_eq!(theme.selected().fg, Some(Color::Rgb(255, 136, 0)));
    }

    #[test]
    fn styles_never_set_a_background() {
        let t = Theme::default();
        for style in [t.text(), t.dim(), t.header(), t.selected()] {
            assert_eq!(style.bg, None);
        }
    }
}
