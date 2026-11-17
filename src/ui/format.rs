//! Formato de cantidades para mostrar.

/// Bytes en unidades binarias cortas: `512B`, `1.5M`, `12G`.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}B")
    } else if value >= 100.0 {
        format!("{value:.0}{}", UNITS[unit])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

/// Recorta `text` a `width` columnas (con `…`) y lo centra con espacios.
pub fn fit_center(text: &str, width: usize) -> String {
    let len = text.chars().count();
    let shown: String = if len > width {
        let cut: String = text.chars().take(width.saturating_sub(1)).collect();
        if width == 0 {
            String::new()
        } else {
            format!("{}…", cut.trim_end())
        }
    } else {
        text.to_string()
    };
    let pad = width.saturating_sub(shown.chars().count());
    format!(
        "{}{}{}",
        " ".repeat(pad / 2),
        shown,
        " ".repeat(pad - pad / 2)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_and_truncates() {
        assert_eq!(fit_center("abc", 7), "  abc  ");
        assert_eq!(fit_center("abcd", 7), " abcd  ");
        assert_eq!(fit_center("un nombre largo", 8), "un nomb…");
        assert_eq!(fit_center("ñandú", 5), "ñandú");
        assert_eq!(fit_center("abc", 0), "");
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(human_bytes(0), "0B");
        assert_eq!(human_bytes(1023), "1023B");
        assert_eq!(human_bytes(1536), "1.5K");
        assert_eq!(human_bytes(250 * 1024 * 1024), "250M");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0G");
    }
}
