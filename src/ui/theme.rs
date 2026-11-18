//! Colores y estilos de la interfaz: envoltorio fino sobre `tui-themes`.
//!
//! Ningún color se define acá. Los roles (texto, acento, aviso, peligro…)
//! salen del tema elegido, y `tui-themes` los adapta a lo que la terminal
//! puede mostrar.

use std::sync::Arc;

use ratatui::style::{Color, Style};
use tui_themes::{Catalog, ColorDepth, Theme as Palette};

/// Tema de fábrica: el aspecto de siempre, sin color de fondo propio.
pub const DEFAULT_THEME: &str = "apptop";

/// Tema activo, ya adaptado a la profundidad de color de la terminal.
/// Clonarlo es barato.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    palette: Arc<Palette>,
}

impl Theme {
    /// Adapta un tema del catálogo (en 24 bits) a la terminal. `accent`
    /// reemplaza el color de acento del tema (opción `theme.accent`).
    pub fn resolve(base: &Palette, accent: Option<(u8, u8, u8)>, depth: ColorDepth) -> Self {
        let mut palette = base.clone();
        if let Some((r, g, b)) = accent {
            let color = Color::Rgb(r, g, b);
            if palette.selection_fg == palette.accent {
                palette.selection_fg = color;
            }
            palette.accent = color;
        }
        Self {
            palette: Arc::new(palette.at(depth)),
        }
    }

    /// El tema de fábrica a una profundidad dada.
    pub fn builtin(depth: ColorDepth) -> Self {
        Self::resolve(&Catalog::builtin().get_or_plain(DEFAULT_THEME), None, depth)
    }

    /// Los roles del tema, para lo que no tiene un método propio.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// ¿Se usan colores de 24 bits?
    pub fn truecolor(&self) -> bool {
        self.palette.depth == ColorDepth::TrueColor
    }

    /// Convierte un RGB (el color de un icono) al mejor color que la
    /// terminal soporta.
    pub fn rgb(&self, rgb: (u8, u8, u8)) -> Color {
        self.palette.rgb(rgb)
    }

    /// Color de selección y atajos.
    pub fn accent(&self) -> Style {
        self.palette.fg(self.palette.accent)
    }

    pub fn text(&self) -> Style {
        self.palette.text()
    }

    pub fn dim(&self) -> Style {
        self.palette.dimmed()
    }

    pub fn header(&self) -> Style {
        self.palette.heading()
    }

    pub fn selected(&self) -> Style {
        self.palette.selected()
    }

    /// Estilo para un porcentaje de uso sobre el total (0–100).
    pub fn load(&self, pct: f32) -> Style {
        let p = &self.palette;
        p.fg(match pct {
            v if v >= 90.0 => p.danger,
            v if v >= 40.0 => p.warn,
            _ => p.ok,
        })
    }

    /// Estilo de un aviso.
    pub fn warn(&self) -> Style {
        self.palette.fg(self.palette.warn)
    }

    /// Estilo de la memoria en el detalle.
    pub fn ram(&self) -> Style {
        self.palette.fg(self.palette.ram)
    }

    /// Estilo del icono de una app a partir de su color dominante.
    pub fn icon(&self, dominant: (u8, u8, u8)) -> Style {
        self.palette.fg(self.rgb(dominant))
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::builtin(ColorDepth::TrueColor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;

    #[test]
    fn degrades_to_256_colours() {
        let full = Theme::default();
        let low = Theme::builtin(ColorDepth::Ansi256);
        assert!(full.truecolor() && !low.truecolor());
        assert_eq!(full.rgb((255, 0, 0)), Color::Rgb(255, 0, 0));
        assert_eq!(low.rgb((255, 0, 0)), Color::Indexed(196));
        assert!(matches!(low.accent().fg, Some(Color::Indexed(_))));
    }

    #[test]
    fn accent_is_configurable() {
        let base = Catalog::builtin().get_or_plain(DEFAULT_THEME);
        let theme = Theme::resolve(&base, Some((255, 136, 0)), ColorDepth::TrueColor);
        assert_eq!(theme.accent().fg, Some(Color::Rgb(255, 136, 0)));
        assert_eq!(theme.selected().fg, Some(Color::Rgb(255, 136, 0)));
    }

    #[test]
    fn default_theme_never_sets_a_background() {
        let t = Theme::default();
        for style in [t.text(), t.dim(), t.header(), t.selected(), t.load(95.0)] {
            assert_eq!(style.bg, None);
        }
        assert_eq!(t.text(), Style::default());
    }

    #[test]
    fn load_follows_ok_warn_danger() {
        let t = Theme::default();
        let p = t.palette();
        assert_eq!(t.load(10.0).fg, Some(p.ok));
        assert_eq!(t.load(40.0).fg, Some(p.warn));
        assert_eq!(t.load(90.0).fg, Some(p.danger));
    }

    #[test]
    fn no_color_keeps_only_modifiers() {
        let t = Theme::builtin(ColorDepth::NoColor);
        assert_eq!(t.accent(), Style::default());
        assert_eq!(t.icon((255, 0, 0)), Style::default());
        assert_eq!(
            t.selected(),
            Style::default().add_modifier(Modifier::REVERSED)
        );
    }
}
