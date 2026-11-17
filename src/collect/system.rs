//! Totales del sistema: CPU, memoria y swap.

use std::fs;
use std::path::PathBuf;

/// Par (total, usado) en bytes; `None` si no se pudo leer.
pub type Usage = Option<(u64, u64)>;

/// Totales del sistema en una muestra. Lo que no se pudo leer queda en
/// `None` y la interfaz lo muestra como `n/d`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SystemStats {
    /// Uso de CPU de todos los núcleos, 0–100. `None` en la primera muestra.
    pub cpu_pct: Option<f32>,
    pub cores: usize,
    /// Memoria total y usada (total − disponible), en bytes.
    pub mem: Usage,
    /// Swap total y usada, en bytes.
    pub swap: Usage,
    /// Segundos desde el arranque.
    pub uptime: Option<f64>,
}

pub struct SystemSampler {
    root: PathBuf,
    /// Ticks (total, inactivos) de la muestra anterior.
    prev: Option<(u64, u64)>,
}

/// Ticks totales e inactivos de la línea `cpu` de `/proc/stat`, y cantidad
/// de núcleos (`cpuN`).
pub fn parse_stat(text: &str) -> Option<((u64, u64), usize)> {
    let mut totals = None;
    let mut cores = 0;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("cpu") => {
                let values: Vec<u64> = fields.filter_map(|f| f.parse().ok()).collect();
                if values.len() < 4 {
                    return None;
                }
                // user nice system idle iowait irq softirq steal; guest ya
                // está contado dentro de user.
                let total: u64 = values.iter().take(8).sum();
                let idle = values[3] + values.get(4).copied().unwrap_or(0);
                totals = Some((total, idle));
            }
            Some(name) if name.starts_with("cpu") => cores += 1,
            _ => {}
        }
    }
    totals.map(|t| (t, cores))
}

/// Valor en bytes de un campo de `/proc/meminfo` (que viene en kB).
fn meminfo_field(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?.strip_prefix(':')?;
        let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
        Some(kb.saturating_mul(1024))
    })
}

/// (memoria total, usada) y (swap total, usada).
pub fn parse_meminfo(text: &str) -> (Usage, Usage) {
    let mem = meminfo_field(text, "MemTotal").map(|total| {
        let available = meminfo_field(text, "MemAvailable")
            .or_else(|| meminfo_field(text, "MemFree"))
            .unwrap_or(0);
        (total, total.saturating_sub(available))
    });
    let swap = meminfo_field(text, "SwapTotal").map(|total| {
        let free = meminfo_field(text, "SwapFree").unwrap_or(0);
        (total, total.saturating_sub(free))
    });
    (mem, swap)
}

impl SystemSampler {
    pub fn new() -> Self {
        Self::with_root("/proc")
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            prev: None,
        }
    }

    pub fn sample(&mut self) -> SystemStats {
        let mut stats = SystemStats::default();
        if let Some((now, cores)) = fs::read_to_string(self.root.join("stat"))
            .ok()
            .and_then(|t| parse_stat(&t))
        {
            stats.cores = cores;
            if let Some(prev) = self.prev {
                let total = now.0.saturating_sub(prev.0);
                let idle = now.1.saturating_sub(prev.1);
                if total > 0 {
                    stats.cpu_pct =
                        Some((total.saturating_sub(idle)) as f32 / total as f32 * 100.0);
                }
            }
            self.prev = Some(now);
        }
        if let Ok(text) = fs::read_to_string(self.root.join("meminfo")) {
            (stats.mem, stats.swap) = parse_meminfo(&text);
        }
        stats.uptime = fs::read_to_string(self.root.join("uptime"))
            .ok()
            .and_then(|t| t.split_whitespace().next()?.parse().ok());
        stats
    }
}

impl Default for SystemSampler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT_A: &str = "cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 50 0 50 350 50 0 0 0 0 0\ncpu1 50 0 50 350 50 0 0 0 0 0\nintr 1\n";
    const STAT_B: &str =
        "cpu  160 0 140 780 120 0 0 0 0 0\ncpu0 1 0 0 0 0 0 0 0 0 0\ncpu1 1 0 0 0 0 0 0 0 0 0\n";
    const MEMINFO: &str = "MemTotal:       16000000 kB\nMemFree:         1000000 kB\nMemAvailable:    6000000 kB\nSwapTotal:       8000000 kB\nSwapFree:        7000000 kB\n";

    #[test]
    fn parses_cpu_totals_and_core_count() {
        assert_eq!(parse_stat(STAT_A), Some(((1000, 800), 2)));
        assert_eq!(parse_stat("intr 5\n"), None);
        assert_eq!(parse_stat("cpu 1 2\n"), None);
    }

    #[test]
    fn parses_memory_and_swap() {
        let (mem, swap) = parse_meminfo(MEMINFO);
        assert_eq!(mem, Some((16_000_000 * 1024, 10_000_000 * 1024)));
        assert_eq!(swap, Some((8_000_000 * 1024, 1_000_000 * 1024)));
        assert_eq!(parse_meminfo("nada"), (None, None));
    }

    #[test]
    fn cpu_percent_comes_from_the_delta_between_samples() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("stat"), STAT_A).unwrap();
        fs::write(tmp.path().join("meminfo"), MEMINFO).unwrap();
        fs::write(tmp.path().join("uptime"), "1234.56 9999.00\n").unwrap();
        let mut sampler = SystemSampler::with_root(tmp.path());
        let first = sampler.sample();
        assert_eq!((first.cpu_pct, first.cores), (None, 2));
        assert_eq!(first.uptime, Some(1234.56));
        // Δtotal = 200, Δinactivo = 100 → 50 %.
        fs::write(tmp.path().join("stat"), STAT_B).unwrap();
        let second = sampler.sample();
        assert_eq!(second.cpu_pct, Some(50.0));
        assert!(second.mem.is_some() && second.swap.is_some());
    }

    #[test]
    fn unreadable_files_become_none_instead_of_failing() {
        let tmp = tempfile::tempdir().unwrap();
        let stats = SystemSampler::with_root(tmp.path()).sample();
        assert_eq!(stats, SystemStats::default());
    }

    #[test]
    fn reads_the_real_system() {
        let mut sampler = SystemSampler::new();
        sampler.sample();
        let stats = sampler.sample();
        assert!(stats.cores >= 1 && stats.mem.is_some());
    }
}
