//! Lectura y escritura a disco por proceso, desde `/proc/<pid>/io`.
//!
//! Ese archivo solo es legible para los procesos del propio usuario (salvo
//! root), así que para apps ajenas el resultado es "no disponible".

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

/// Bytes por segundo leídos y escritos en almacenamiento.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct IoRate {
    pub read_bps: f64,
    pub write_bps: f64,
}

/// Resultado de medir el disco de un conjunto de procesos.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum IoReading {
    /// Ningún proceso fue legible (falta permiso).
    #[default]
    Unavailable,
    /// Primera lectura: todavía no hay con qué comparar.
    Warming,
    Rate(IoRate),
}

/// Extrae `read_bytes` y `write_bytes` de `/proc/<pid>/io`.
pub fn parse_io(text: &str) -> Option<(u64, u64)> {
    let field = |key: &str| -> Option<u64> {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix(':')?.trim().parse().ok())
    };
    Some((field("read_bytes")?, field("write_bytes")?))
}

/// Calcula tasas de disco comparando lecturas sucesivas.
pub struct IoTracker {
    root: PathBuf,
    /// pid → (starttime, bytes leídos, bytes escritos) de la lectura anterior.
    prev: HashMap<i32, (u64, u64, u64)>,
    last: Option<Instant>,
}

impl IoTracker {
    pub fn new() -> Self {
        Self::with_root("/proc")
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            prev: HashMap::new(),
            last: None,
        }
    }

    /// Mide los procesos dados (`(pid, starttime)`). Los que aparecen por
    /// primera vez sirven de referencia para la próxima medición.
    pub fn sample(&mut self, procs: &[(i32, u64)], now: Instant) -> IoReading {
        let elapsed = self
            .last
            .map(|t| now.saturating_duration_since(t).as_secs_f64());
        self.last = Some(now);

        let mut current = HashMap::with_capacity(procs.len());
        let (mut read, mut write) = (0u64, 0u64);
        let mut compared = 0usize;
        for &(pid, starttime) in procs {
            let Some((r, w)) = fs::read_to_string(self.root.join(pid.to_string()).join("io"))
                .ok()
                .and_then(|t| parse_io(&t))
            else {
                continue;
            };
            if let Some(&(old_start, old_r, old_w)) = self.prev.get(&pid) {
                if old_start == starttime {
                    read += r.saturating_sub(old_r);
                    write += w.saturating_sub(old_w);
                    compared += 1;
                }
            }
            current.insert(pid, (starttime, r, w));
        }
        let readable = current.len();
        self.prev = current;

        match (readable, compared, elapsed) {
            (0, _, _) => IoReading::Unavailable,
            (_, 0, _) | (_, _, None) => IoReading::Warming,
            (_, _, Some(dt)) if dt > 0.0 => IoReading::Rate(IoRate {
                read_bps: read as f64 / dt,
                write_bps: write as f64 / dt,
            }),
            _ => IoReading::Warming,
        }
    }
}

impl Default for IoTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::Duration;

    fn write_io(root: &Path, pid: i32, read: u64, write: u64) {
        let dir = root.join(pid.to_string());
        fs::create_dir_all(&dir).unwrap();
        let text = format!(
            "rchar: 9\nwchar: 9\nsyscr: 1\nsyscw: 1\nread_bytes: {read}\nwrite_bytes: {write}\ncancelled_write_bytes: 0\n"
        );
        fs::write(dir.join("io"), text).unwrap();
    }

    #[test]
    fn parses_the_storage_counters() {
        let text = "rchar: 100\nwchar: 200\nread_bytes: 4096\nwrite_bytes: 8192\ncancelled_write_bytes: 0\n";
        assert_eq!(parse_io(text), Some((4096, 8192)));
        assert_eq!(parse_io("rchar: 1\n"), None);
        assert_eq!(parse_io(""), None);
    }

    #[test]
    fn rates_come_from_deltas_summed_over_processes() {
        let tmp = tempfile::tempdir().unwrap();
        write_io(tmp.path(), 1, 1000, 0);
        write_io(tmp.path(), 2, 0, 500);
        let mut tracker = IoTracker::with_root(tmp.path());
        let t0 = Instant::now();
        let procs = [(1, 10), (2, 20)];
        assert_eq!(tracker.sample(&procs, t0), IoReading::Warming);
        write_io(tmp.path(), 1, 5000, 100);
        write_io(tmp.path(), 2, 2000, 1500);
        let reading = tracker.sample(&procs, t0 + Duration::from_secs(2));
        assert_eq!(
            reading,
            IoReading::Rate(IoRate {
                read_bps: 3000.0,
                write_bps: 550.0
            })
        );
    }

    #[test]
    fn unreadable_processes_are_unavailable_not_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let mut tracker = IoTracker::with_root(tmp.path());
        assert_eq!(
            tracker.sample(&[(1, 1)], Instant::now()),
            IoReading::Unavailable
        );
        assert_eq!(tracker.sample(&[], Instant::now()), IoReading::Unavailable);
    }

    #[test]
    fn new_and_recycled_pids_only_set_a_baseline() {
        let tmp = tempfile::tempdir().unwrap();
        write_io(tmp.path(), 1, 1000, 1000);
        let mut tracker = IoTracker::with_root(tmp.path());
        let t0 = Instant::now();
        tracker.sample(&[(1, 10)], t0);
        // Mismo pid, otro proceso (starttime distinto) con contadores menores.
        write_io(tmp.path(), 1, 50, 50);
        write_io(tmp.path(), 2, 9_000_000, 0);
        let reading = tracker.sample(&[(1, 99), (2, 5)], t0 + Duration::from_secs(1));
        assert_eq!(reading, IoReading::Warming);
        // En la siguiente ya hay referencia para ambos.
        write_io(tmp.path(), 1, 150, 50);
        let reading = tracker.sample(&[(1, 99), (2, 5)], t0 + Duration::from_secs(2));
        assert_eq!(
            reading,
            IoReading::Rate(IoRate {
                read_bps: 100.0,
                write_bps: 0.0
            })
        );
    }

    #[test]
    fn reads_own_process_from_the_real_proc() {
        let mut tracker = IoTracker::new();
        let me = std::process::id() as i32;
        assert_eq!(
            tracker.sample(&[(me, 1)], Instant::now()),
            IoReading::Warming
        );
        assert!(matches!(
            tracker.sample(&[(me, 1)], Instant::now() + Duration::from_secs(1)),
            IoReading::Rate(_)
        ));
    }
}
