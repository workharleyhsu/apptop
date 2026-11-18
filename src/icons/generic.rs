//! Iconos genéricos para servicios y apps sin icono propio.
//!
//! Se describen como SVG y pasan por el mismo pipeline que los iconos
//! reales, así comparten exactamente el mismo estilo de líneas.

use std::f32::consts::PI;

use super::braille::BrailleIcon;
use super::{convert_image, raster};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenericKind {
    /// Engranaje: servicios.
    Service,
    /// Ventana: apps sin icono.
    App,
}

impl GenericKind {
    pub fn color(self) -> (u8, u8, u8) {
        match self {
            GenericKind::Service => crate::themes::color::icon::GENERIC_SERVICE,
            GenericKind::App => crate::themes::color::icon::GENERIC_APP,
        }
    }

    /// Nombre estable, usado como clave de caché y en `--dump-icon`.
    pub fn name(self) -> &'static str {
        match self {
            GenericKind::Service => "generic:service",
            GenericKind::App => "generic:app",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [GenericKind::Service, GenericKind::App]
            .into_iter()
            .find(|k| k.name() == name)
    }
}

/// Engranaje de ocho dientes con un agujero central.
fn gear_svg() -> String {
    const TEETH: usize = 8;
    const OUTER: f32 = 46.0;
    const ROOT: f32 = 35.0;
    let step = 2.0 * PI / TEETH as f32;
    let mut path = String::new();
    for tooth in 0..TEETH {
        let base = tooth as f32 * step;
        // Cada diente: flanco de subida, cresta, flanco de bajada, valle.
        let corners = [(ROOT, -0.30), (OUTER, -0.19), (OUTER, 0.19), (ROOT, 0.30)];
        for (radius, offset) in corners {
            let angle = base + offset * step;
            let (x, y) = (50.0 + radius * angle.cos(), 50.0 + radius * angle.sin());
            let command = if path.is_empty() { 'M' } else { 'L' };
            path.push_str(&format!("{command}{x:.2} {y:.2} "));
        }
    }
    path.push('Z');
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<path fill-rule="evenodd" fill="#8896aa" d="{path} M66 50 A16 16 0 1 0 34 50 A16 16 0 1 0 66 50 Z"/>
</svg>"##
    )
}

/// Ventana con barra de título y tres botones.
fn window_svg() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<rect x="4" y="12" width="92" height="76" rx="9" fill="#78aad2"/>
<path d="M4 34 H96 V79 A9 9 0 0 1 87 88 H13 A9 9 0 0 1 4 79 Z" fill="#1c2733"/>
<circle cx="18" cy="23" r="4.5" fill="#1c2733"/>
<circle cx="33" cy="23" r="4.5" fill="#1c2733"/>
<circle cx="48" cy="23" r="4.5" fill="#1c2733"/>
</svg>"##
}

/// Dibuja el icono genérico al tamaño pedido (en celdas).
pub fn generic_icon(kind: GenericKind, width: u16, height: u16) -> BrailleIcon {
    let svg = match kind {
        GenericKind::Service => gear_svg(),
        GenericKind::App => window_svg().to_string(),
    };
    let mut icon = match raster::load_svg(svg.as_bytes()) {
        Some(img) => convert_image(&img, width, height),
        None => BrailleIcon::from_dots(&[], width, height, kind.color()),
    };
    icon.color = kind.color();
    icon
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::{DETAIL_ICON, GRID_ICON};

    #[test]
    fn generic_icons_are_outlines_at_both_sizes() {
        for kind in [GenericKind::Service, GenericKind::App] {
            for (w, h) in [GRID_ICON, DETAIL_ICON] {
                let icon = generic_icon(kind, w, h);
                assert_eq!((icon.width, icon.height), (w, h));
                assert_eq!(icon.color, kind.color());
                let d = icon.density();
                assert!(
                    d > 0.06 && d < 0.30,
                    "{kind:?} {w}x{h}: {d}\n{}",
                    icon.to_plain()
                );
                assert!(!icon.rows().iter().any(|r| r.contains("⣿⣿")));
            }
        }
    }

    #[test]
    fn the_two_kinds_differ_and_names_round_trip() {
        let (w, h) = GRID_ICON;
        assert_ne!(
            generic_icon(GenericKind::Service, w, h).cells,
            generic_icon(GenericKind::App, w, h).cells
        );
        for kind in [GenericKind::Service, GenericKind::App] {
            assert_eq!(GenericKind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(GenericKind::from_name("otro"), None);
    }

    #[test]
    fn gear_is_roughly_symmetric() {
        let icon = generic_icon(GenericKind::Service, 16, 8);
        let rows = icon.rows();
        let lit = |r: &str| r.chars().filter(|c| *c != ' ').count() as i32;
        // Mitad superior e inferior con una cantidad parecida de celdas.
        let top: i32 = rows[..4].iter().map(|r| lit(r)).sum();
        let bottom: i32 = rows[4..].iter().map(|r| lit(r)).sum();
        assert!(
            (top - bottom).abs() <= 6,
            "{top} vs {bottom}\n{}",
            icon.to_plain()
        );
    }
}
