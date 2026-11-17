//! Configuración en `~/.config/apptop/config.toml`.
//!
//! El archivo es opcional y tolerante: una clave desconocida o un valor
//! inválido generan una advertencia y se usa el valor por defecto, en vez de
//! impedir el arranque.
//!
//! ```toml
//! refresh = 1.0          # segundos entre refrescos (0.25 a 10)
//! sort = "name"          # name | cpu | ram
//! icon_size = "large"    # small (8x4) | medium (12x6) | large (16x8)
//!
//! [theme]
//! color = "auto"         # auto | truecolor | 256
//! accent = "#7aa2f7"     # color de selección y atajos
//! icons = "breeze"       # tema de iconos; por defecto, el del escritorio
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use toml::{Table, Value};

use crate::ui::state::{clamp_refresh, SortKey, DEFAULT_REFRESH};

/// Tamaños de icono de la grilla, en celdas. Siempre 2:1 para que el icono
/// quede cuadrado en pantalla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconSize {
    Small,
    Medium,
    #[default]
    Large,
}

impl IconSize {
    pub fn cells(self) -> (u16, u16) {
        match self {
            IconSize::Small => (8, 4),
            IconSize::Medium => (12, 6),
            IconSize::Large => (16, 8),
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "small" | "chico" | "pequeño" => Some(IconSize::Small),
            "medium" | "mediano" => Some(IconSize::Medium),
            "large" | "grande" => Some(IconSize::Large),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            IconSize::Small => "small",
            IconSize::Medium => "medium",
            IconSize::Large => "large",
        }
    }
}

/// Profundidad de color a usar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// Según `COLORTERM`.
    #[default]
    Auto,
    Truecolor,
    Palette256,
}

impl ColorMode {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "auto" => Some(ColorMode::Auto),
            "truecolor" | "24bit" | "rgb" => Some(ColorMode::Truecolor),
            "256" | "256color" | "ansi256" => Some(ColorMode::Palette256),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ColorMode::Auto => "auto",
            ColorMode::Truecolor => "truecolor",
            ColorMode::Palette256 => "256",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub refresh: Duration,
    pub sort: SortKey,
    pub icon_size: IconSize,
    pub color: ColorMode,
    /// Color de acento; `None` = el de fábrica.
    pub accent: Option<(u8, u8, u8)>,
    /// Tema de iconos; `None` = detectar el del escritorio.
    pub icon_theme: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            refresh: DEFAULT_REFRESH,
            sort: SortKey::default(),
            icon_size: IconSize::default(),
            color: ColorMode::default(),
            accent: None,
            icon_theme: None,
        }
    }
}

/// `#rrggbb` o `rrggbb` → RGB.
pub fn parse_hex_color(text: &str) -> Option<(u8, u8, u8)> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some((channel(0)?, channel(2)?, channel(4)?))
}

/// Ruta por defecto: `$XDG_CONFIG_HOME/apptop/config.toml` o
/// `~/.config/apptop/config.toml`.
pub fn default_path() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|base| base.join("apptop").join("config.toml"))
}

impl Config {
    /// Carga la configuración. Devuelve también las advertencias a mostrar.
    /// Que el archivo no exista no es un problema.
    pub fn load(path: Option<&Path>) -> (Self, Vec<String>) {
        let explicit = path.is_some();
        let Some(path) = path.map(Path::to_path_buf).or_else(default_path) else {
            return (Self::default(), Vec::new());
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let (config, warnings) = Self::parse(&text);
                let located = warnings
                    .into_iter()
                    .map(|w| format!("{}: {w}", path.display()))
                    .collect();
                (config, located)
            }
            // Solo es un error si el usuario pidió ese archivo explícitamente.
            Err(err) if explicit => (
                Self::default(),
                vec![format!("{}: no se pudo leer ({err})", path.display())],
            ),
            Err(_) => (Self::default(), Vec::new()),
        }
    }

    /// Interpreta el texto TOML campo por campo.
    pub fn parse(text: &str) -> (Self, Vec<String>) {
        let mut config = Self::default();
        let mut warnings = Vec::new();
        let table: Table = match text.parse() {
            Ok(table) => table,
            Err(err) => {
                let reason = err.to_string();
                let first = reason.lines().next().unwrap_or("error de sintaxis");
                warnings.push(format!(
                    "TOML inválido, se usan los valores por defecto ({first})"
                ));
                return (config, warnings);
            }
        };

        for (key, value) in &table {
            match (key.as_str(), value) {
                ("refresh", Value::Float(_) | Value::Integer(_)) => {
                    let seconds = match value {
                        Value::Float(f) => *f,
                        Value::Integer(i) => *i as f64,
                        _ => 0.0,
                    };
                    if seconds.is_finite() && seconds > 0.0 {
                        let wanted = Duration::from_secs_f64(seconds.min(3600.0));
                        config.refresh = clamp_refresh(wanted);
                        if config.refresh != wanted {
                            warnings.push(format!(
                                "refresh = {seconds}: fuera de rango, se usa {} s",
                                config.refresh.as_secs_f64()
                            ));
                        }
                    } else {
                        warnings.push(format!("refresh = {seconds}: debe ser un número positivo"));
                    }
                }
                ("sort", Value::String(text)) => match SortKey::parse(text) {
                    Some(sort) => config.sort = sort,
                    None => {
                        warnings.push(format!("sort = \"{text}\": se esperaba name, cpu o ram"))
                    }
                },
                ("icon_size", Value::String(text)) => match IconSize::parse(text) {
                    Some(size) => config.icon_size = size,
                    None => warnings.push(format!(
                        "icon_size = \"{text}\": se esperaba small, medium o large"
                    )),
                },
                ("theme", Value::Table(theme)) => config.parse_theme(theme, &mut warnings),
                ("refresh" | "sort" | "icon_size" | "theme", other) => {
                    warnings.push(format!("{key}: tipo inesperado ({})", other.type_str()));
                }
                _ => warnings.push(format!("clave desconocida '{key}'")),
            }
        }
        (config, warnings)
    }

    fn parse_theme(&mut self, theme: &Table, warnings: &mut Vec<String>) {
        for (key, value) in theme {
            match (key.as_str(), value) {
                ("color", Value::String(text)) => match ColorMode::parse(text) {
                    Some(mode) => self.color = mode,
                    None => warnings.push(format!(
                        "theme.color = \"{text}\": se esperaba auto, truecolor o 256"
                    )),
                },
                // `color = 256` sin comillas también vale.
                ("color", Value::Integer(256)) => self.color = ColorMode::Palette256,
                ("accent", Value::String(text)) => match parse_hex_color(text) {
                    Some(rgb) => self.accent = Some(rgb),
                    None => {
                        warnings.push(format!("theme.accent = \"{text}\": se esperaba #rrggbb"))
                    }
                },
                ("icons", Value::String(text)) if !text.trim().is_empty() => {
                    self.icon_theme = Some(text.trim().to_string());
                }
                ("color" | "accent" | "icons", other) => {
                    warnings.push(format!(
                        "theme.{key}: valor inesperado ({})",
                        other.type_str()
                    ));
                }
                _ => warnings.push(format!("clave desconocida 'theme.{key}'")),
            }
        }
    }

    /// La configuración efectiva en formato TOML (para `--print-config`).
    pub fn to_toml(&self) -> String {
        let mut out = format!(
            "refresh = {}\nsort = \"{}\"\nicon_size = \"{}\"\n\n[theme]\ncolor = \"{}\"\n",
            self.refresh.as_secs_f64(),
            match self.sort {
                SortKey::Name => "name",
                SortKey::Cpu => "cpu",
                SortKey::Ram => "ram",
            },
            self.icon_size.label(),
            self.color.label(),
        );
        if let Some((r, g, b)) = self.accent {
            out.push_str(&format!("accent = \"#{r:02x}{g:02x}{b:02x}\"\n"));
        }
        if let Some(icons) = &self.icon_theme {
            out.push_str(&format!("icons = \"{icons}\"\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_the_default() {
        let (config, warnings) = Config::parse("");
        assert_eq!(config, Config::default());
        assert!(warnings.is_empty());
        assert_eq!(config.refresh, Duration::from_secs(1));
        assert_eq!(config.icon_size.cells(), (16, 8));
    }

    #[test]
    fn reads_every_documented_key() {
        let text = r##"
            refresh = 2.5
            sort = "cpu"
            icon_size = "medium"

            [theme]
            color = "256"
            accent = "#ff8800"
            icons = "Papirus"
        "##;
        let (config, warnings) = Config::parse(text);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(config.refresh, Duration::from_millis(2500));
        assert_eq!(config.sort, SortKey::Cpu);
        assert_eq!(config.icon_size.cells(), (12, 6));
        assert_eq!(config.color, ColorMode::Palette256);
        assert_eq!(config.accent, Some((255, 136, 0)));
        assert_eq!(config.icon_theme.as_deref(), Some("Papirus"));
    }

    #[test]
    fn bad_values_warn_and_keep_defaults_without_losing_the_good_ones() {
        let text = r#"
            refresh = "rápido"
            sort = "tamaño"
            icon_size = "large"
            volumen = 11

            [theme]
            accent = "azul"
            color = 256
            brillo = true
        "#;
        let (config, warnings) = Config::parse(text);
        assert_eq!(config.refresh, DEFAULT_REFRESH);
        assert_eq!(config.sort, SortKey::Name);
        assert_eq!(config.icon_size, IconSize::Large);
        assert_eq!(config.color, ColorMode::Palette256);
        assert_eq!(config.accent, None);
        assert_eq!(warnings.len(), 5, "{warnings:?}");
        assert!(warnings
            .iter()
            .any(|w| w.contains("clave desconocida 'volumen'")));
        assert!(warnings.iter().any(|w| w.contains("theme.brillo")));
        assert!(warnings.iter().any(|w| w.contains("#rrggbb")));
    }

    #[test]
    fn refresh_is_clamped_with_a_warning() {
        let (config, warnings) = Config::parse("refresh = 0.01");
        assert_eq!(config.refresh, Duration::from_millis(250));
        assert_eq!(warnings.len(), 1);
        let (config, _) = Config::parse("refresh = 3");
        assert_eq!(config.refresh, Duration::from_secs(3));
        let (config, warnings) = Config::parse("refresh = -1");
        assert_eq!(config.refresh, DEFAULT_REFRESH);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn broken_toml_falls_back_to_defaults() {
        let (config, warnings) = Config::parse("refresh = = 1\n[[[");
        assert_eq!(config, Config::default());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].starts_with("TOML inválido"));
    }

    #[test]
    fn loads_from_disk_and_tolerates_a_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "sort = \"ram\"\nextra = 1\n").unwrap();
        let (config, warnings) = Config::load(Some(&path));
        assert_eq!(config.sort, SortKey::Ram);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].starts_with(&path.display().to_string()));

        // Pedido explícitamente y no existe: se avisa.
        let (config, warnings) = Config::load(Some(&tmp.path().join("no-existe.toml")));
        assert_eq!(config, Config::default());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn printed_config_parses_back_to_itself() {
        let config = Config {
            refresh: Duration::from_millis(750),
            sort: SortKey::Ram,
            icon_size: IconSize::Small,
            color: ColorMode::Truecolor,
            accent: Some((1, 2, 3)),
            icon_theme: Some("Adwaita".into()),
        };
        let (parsed, warnings) = Config::parse(&config.to_toml());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(parsed, config);
        let (default_again, warnings) = Config::parse(&Config::default().to_toml());
        assert!(warnings.is_empty());
        assert_eq!(default_again, Config::default());
    }

    #[test]
    fn parses_hex_colours() {
        assert_eq!(parse_hex_color("#7aa2f7"), Some((0x7a, 0xa2, 0xf7)));
        assert_eq!(parse_hex_color("FFFFFF"), Some((255, 255, 255)));
        assert_eq!(parse_hex_color("#fff"), None);
        assert_eq!(parse_hex_color("#gggggg"), None);
        assert_eq!(parse_hex_color("#ñañañ"), None);
    }
}
