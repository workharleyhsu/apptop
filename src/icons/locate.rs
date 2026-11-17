//! Localiza el archivo de un icono según el tema activo.
//!
//! El orden de búsqueda es: ruta absoluta → tema activo y sus padres →
//! `hicolor` → `pixmaps`. El tema activo se lee de los archivos de
//! configuración (sin ejecutar `gsettings`).

use std::path::{Path, PathBuf};

/// Tamaño pedido al tema. Es mayor que el lienzo de trabajo para que la
/// detección de bordes parta de una imagen reducida, no ampliada.
const LOOKUP_SIZE: u16 = 128;

const FALLBACK_THEME: &str = "hicolor";

pub struct IconLocator {
    theme: String,
}

/// Lee `gtk-icon-theme-name=` de un `settings.ini` de GTK.
fn theme_from_gtk_ini(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "gtk-icon-theme-name").then(|| clean(value))
    })
}

/// Lee `Theme=` de la sección `[Icons]` de `kdeglobals`.
fn theme_from_kdeglobals(text: &str) -> Option<String> {
    let mut in_icons = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_icons = line == "[Icons]";
        } else if in_icons {
            if let Some(value) = line.strip_prefix("Theme=") {
                return Some(clean(value));
            }
        }
    }
    None
}

/// Lee `Net/IconThemeName "x"` de `xsettingsd.conf`.
fn theme_from_xsettingsd(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("Net/IconThemeName").map(clean))
}

fn clean(value: &str) -> String {
    value.trim().trim_matches(['"', '\'']).to_string()
}

/// Tema de iconos configurado por el usuario, si se puede averiguar.
pub fn detect_theme() -> Option<String> {
    if let Some(theme) = std::env::var("APPTOP_ICON_THEME")
        .ok()
        .filter(|t| !t.is_empty())
    {
        return Some(theme);
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    type Parser = fn(&str) -> Option<String>;
    let sources: [(&str, Parser); 4] = [
        ("gtk-4.0/settings.ini", theme_from_gtk_ini),
        ("gtk-3.0/settings.ini", theme_from_gtk_ini),
        ("kdeglobals", theme_from_kdeglobals),
        ("xsettingsd/xsettingsd.conf", theme_from_xsettingsd),
    ];
    sources.iter().find_map(|(file, parse)| {
        let text = std::fs::read_to_string(config.join(file)).ok()?;
        parse(&text).filter(|t| !t.is_empty())
    })
}

fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "svg" | "png"))
}

impl IconLocator {
    /// Usa el tema activo del usuario (o `hicolor` si no se puede detectar).
    pub fn detect() -> Self {
        Self::with_theme(detect_theme().unwrap_or_else(|| FALLBACK_THEME.to_string()))
    }

    pub fn with_theme(theme: impl Into<String>) -> Self {
        Self {
            theme: theme.into(),
        }
    }

    pub fn theme(&self) -> &str {
        &self.theme
    }

    /// Devuelve la ruta de un SVG o PNG para el valor de `Icon=` dado.
    pub fn locate(&self, icon: &str) -> Option<PathBuf> {
        if icon.is_empty() {
            return None;
        }
        let as_path = Path::new(icon);
        if as_path.is_absolute() {
            return (supported(as_path) && as_path.is_file()).then(|| as_path.to_path_buf());
        }
        // Algunos `.desktop` ponen `Icon=foo.png`: el tema espera solo `foo`.
        let name = [".svg", ".png", ".xpm"]
            .iter()
            .find_map(|ext| icon.strip_suffix(ext))
            .unwrap_or(icon);
        let find = |theme: &str| {
            freedesktop_icons::lookup(name)
                .with_size(LOOKUP_SIZE)
                .with_theme(theme)
                .find()
                .filter(|p| supported(p))
        };
        find(&self.theme).or_else(|| {
            (self.theme != FALLBACK_THEME)
                .then(|| find(FALLBACK_THEME))
                .flatten()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_theme_from_config_files() {
        let gtk = "[Settings]\ngtk-theme-name=Breeze\ngtk-icon-theme-name=breeze\n";
        assert_eq!(theme_from_gtk_ini(gtk).as_deref(), Some("breeze"));
        assert_eq!(theme_from_gtk_ini("[Settings]\n"), None);

        let kde = "[General]\nTheme=wrong\n[Icons]\nTheme=breeze-dark\n[KDE]\nTheme=no\n";
        assert_eq!(theme_from_kdeglobals(kde).as_deref(), Some("breeze-dark"));
        assert_eq!(theme_from_kdeglobals("[General]\nTheme=x\n"), None);

        let xs = "Net/ThemeName \"Breeze\"\nNet/IconThemeName \"Papirus\"\n";
        assert_eq!(theme_from_xsettingsd(xs).as_deref(), Some("Papirus"));
    }

    #[test]
    fn absolute_paths_must_exist_and_be_svg_or_png() {
        let tmp = tempfile::tempdir().unwrap();
        let png = tmp.path().join("a.png");
        let xpm = tmp.path().join("a.xpm");
        std::fs::write(&png, b"x").unwrap();
        std::fs::write(&xpm, b"x").unwrap();
        let locator = IconLocator::with_theme("hicolor");
        assert_eq!(locator.locate(png.to_str().unwrap()), Some(png));
        assert_eq!(locator.locate(xpm.to_str().unwrap()), None);
        assert_eq!(locator.locate("/no/existe.svg"), None);
        assert_eq!(locator.locate(""), None);
    }

    #[test]
    fn unknown_icons_are_none() {
        let locator = IconLocator::detect();
        assert_eq!(locator.locate("apptop-icono-que-no-existe-xyz"), None);
    }
}
