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

/// Tasa en bytes por segundo: `1.5M/s`.
pub fn human_rate(bytes_per_second: f64) -> String {
    format!(
        "{}/s",
        human_bytes(bytes_per_second.max(0.0).round() as u64)
    )
}

/// Duración legible con dos unidades: `3d 4h`, `2h 05m`, `4m 12s`, `8s`.
pub fn human_duration(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (days, hours, minutes, secs) = (
        total / 86400,
        total / 3600 % 24,
        total / 60 % 60,
        total % 60,
    );
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs:02}s")
    } else {
        format!("{secs}s")
    }
}

/// Intervalo corto: `1s`, `0.75s`, `2.5s`.
pub fn human_interval(interval: std::time::Duration) -> String {
    let text = format!("{:.2}", interval.as_secs_f64());
    format!("{}s", text.trim_end_matches('0').trim_end_matches('.'))
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
    fn formats_rates() {
        assert_eq!(human_rate(0.0), "0B/s");
        assert_eq!(human_rate(1536.0), "1.5K/s");
        assert_eq!(human_rate(-3.0), "0B/s");
    }

    #[test]
    fn formats_durations() {
        assert_eq!(human_duration(8.9), "8s");
        assert_eq!(human_duration(252.0), "4m 12s");
        assert_eq!(human_duration(7500.0), "2h 05m");
        assert_eq!(human_duration(3.0 * 86400.0 + 4.0 * 3600.0 + 59.0), "3d 4h");
        assert_eq!(human_duration(-5.0), "0s");
    }

    #[test]
    fn formats_intervals() {
        use std::time::Duration;
        assert_eq!(human_interval(Duration::from_secs(1)), "1s");
        assert_eq!(human_interval(Duration::from_millis(750)), "0.75s");
        assert_eq!(human_interval(Duration::from_millis(2500)), "2.5s");
        assert_eq!(human_interval(Duration::from_secs(10)), "10s");
    }

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
