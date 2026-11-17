//! Localización de iconos y conversión a contornos braille.

pub mod braille;
pub mod color;
pub mod edges;
pub mod generic;
pub mod locate;
pub mod raster;

use std::path::Path;

pub use braille::BrailleIcon;
pub use generic::{generic_icon, GenericKind};
pub use locate::IconLocator;

/// Tamaño del icono en la grilla, en celdas.
pub const GRID_ICON: (u16, u16) = (16, 8);
/// Tamaño del icono grande de la vista de detalle, en celdas.
pub const DETAIL_ICON: (u16, u16) = (24, 12);

/// Convierte una imagen ya cargada en un contorno braille de
/// `width` × `height` celdas.
///
/// La imagen se recorta a su contenido, se reduce al doble de la resolución
/// en puntos, se le detectan los bordes y cada 2×2 píxeles se funden en un
/// punto braille.
pub fn convert_image(img: &raster::Rgba, width: u16, height: u16) -> BrailleIcon {
    let (dots_w, dots_h) = (width as usize * 2, height as usize * 4);
    let work = raster::fit(img, dots_w * 2, dots_h * 2, 2);
    let edges = edges::detect(&work);
    let (dots, _, _) = edges::downsample(&edges, work.width, work.height);
    BrailleIcon::from_dots(&dots, width, height, color::dominant(&work))
}

/// Carga un SVG o PNG y lo convierte. `None` si no se puede leer o si el
/// resultado queda vacío.
pub fn convert_file(path: &Path, width: u16, height: u16) -> Option<BrailleIcon> {
    let img = raster::load(path)?;
    let icon = convert_image(&img, width, height);
    (!icon.is_blank()).then_some(icon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_an_svg_into_an_outline() {
        let img = raster::load_svg(raster::tests::DISC_SVG.as_bytes()).unwrap();
        let icon = convert_image(&img, 16, 8);
        assert_eq!((icon.width, icon.height), (16, 8));
        assert_eq!(icon.color, (0xe6, 0x60, 0x00));
        // Contorno, no relleno: pocos puntos encendidos y el centro del
        // cuadrado blanco interior vacío.
        assert!(
            icon.density() > 0.05 && icon.density() < 0.30,
            "{}",
            icon.density()
        );
        let rows = icon.rows();
        assert_eq!(rows.len(), 8);
        assert!(rows.iter().all(|r| r.chars().count() == 16));
        assert!(!rows.iter().any(|r| r.contains('⣿')), "{}", icon.to_plain());
    }

    #[test]
    fn unreadable_files_give_none() {
        assert!(convert_file(Path::new("/no/existe.svg"), 16, 8).is_none());
    }
}
