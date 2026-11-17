//! Uso de GPU y memoria de vídeo por proceso, desde `/proc/<pid>/fdinfo`.
//!
//! Los controladores DRM publican, por cada descriptor abierto sobre
//! `/dev/dri/*`, claves `drm-engine-<motor>: <ns> ns` (tiempo acumulado de
//! GPU) y de memoria (`drm-memory-*`, `drm-total-*`, `drm-resident-*`). Un
//! mismo cliente DRM puede aparecer en varios descriptores y procesos: se
//! deduplica por dispositivo + `drm-client-id`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Datos de un cliente DRM leídos de un `fdinfo`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrmClient {
    pub driver: String,
    /// Dispositivo PCI (`drm-pdev`), si el controlador lo informa.
    pub device: String,
    pub client_id: u64,
    /// Motor → (nanosegundos acumulados, capacidad).
    pub engines: HashMap<String, (u64, u32)>,
    /// Memoria por región, en bytes, según cada familia de claves.
    pub memory: HashMap<String, u64>,
    pub total: HashMap<String, u64>,
    pub resident: HashMap<String, u64>,
}

/// Interpreta un valor de memoria: `344720 KiB`, `12 MiB` o bytes sin unidad.
fn parse_size(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let number: u64 = parts.next()?.parse().ok()?;
    let factor = match parts.next() {
        None => 1,
        Some("KiB") => 1 << 10,
        Some("MiB") => 1 << 20,
        Some("GiB") => 1 << 30,
        Some(_) => return None,
    };
    Some(number.saturating_mul(factor))
}

/// Interpreta un `fdinfo`. Devuelve `None` si no es de un cliente DRM.
pub fn parse_fdinfo(text: &str) -> Option<DrmClient> {
    let mut client = DrmClient::default();
    let mut has_id = false;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if key == "drm-driver" {
            client.driver = value.to_string();
        } else if key == "drm-pdev" {
            client.device = value.to_string();
        } else if key == "drm-client-id" {
            client.client_id = value.parse().ok()?;
            has_id = true;
        } else if let Some(engine) = key.strip_prefix("drm-engine-capacity-") {
            if let Ok(capacity) = value.parse::<u32>() {
                client.engines.entry(engine.to_string()).or_insert((0, 1)).1 = capacity.max(1);
            }
        } else if let Some(engine) = key.strip_prefix("drm-engine-") {
            if let Some(ns) = value
                .strip_suffix("ns")
                .and_then(|v| v.trim().parse::<u64>().ok())
            {
                client.engines.entry(engine.to_string()).or_insert((0, 1)).0 = ns;
            }
        } else if let Some(region) = key.strip_prefix("drm-memory-") {
            if let Some(bytes) = parse_size(value) {
                client.memory.insert(region.to_string(), bytes);
            }
        } else if let Some(region) = key.strip_prefix("drm-total-") {
            if let Some(bytes) = parse_size(value) {
                client.total.insert(region.to_string(), bytes);
            }
        } else if let Some(region) = key.strip_prefix("drm-resident-") {
            if let Some(bytes) = parse_size(value) {
                client.resident.insert(region.to_string(), bytes);
            }
        }
    }
    has_id.then_some(client)
}

impl DrmClient {
    /// Memoria de vídeo del cliente y si es VRAM dedicada.
    ///
    /// Con región `vram` (GPU dedicada, p. ej. amdgpu) se informa esa. Sin
    /// ella (gráficos integrados) se suman las regiones residentes en RAM
    /// del sistema.
    pub fn video_memory(&self) -> (u64, bool) {
        for family in [&self.resident, &self.memory, &self.total] {
            if let Some(vram) = family.get("vram") {
                return (*vram, true);
            }
        }
        for family in [&self.resident, &self.total, &self.memory] {
            if !family.is_empty() {
                return (family.values().sum(), false);
            }
        }
        (0, false)
    }
}

/// Uso de GPU de un conjunto de procesos.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GpuStats {
    /// Porcentaje del motor más ocupado (100 = el motor entero).
    pub busy_pct: f32,
    /// Nombre de ese motor (`gfx`, `render`, `video`...).
    pub engine: String,
    /// Memoria de vídeo en bytes y si es VRAM dedicada.
    pub memory: u64,
    pub dedicated: bool,
    pub driver: String,
    pub clients: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum GpuReading {
    /// No se pudo leer ningún proceso (falta permiso).
    #[default]
    Unavailable,
    /// Los procesos son legibles y ninguno tiene la GPU abierta.
    NotUsed,
    /// Hay clientes DRM pero falta una segunda lectura para el porcentaje.
    Warming(GpuStats),
    Stats(GpuStats),
}

type ClientKey = (String, u64);

pub struct GpuTracker {
    root: PathBuf,
    /// Cliente → nanosegundos acumulados por motor en la lectura anterior.
    prev: HashMap<ClientKey, HashMap<String, u64>>,
    last: Option<Instant>,
}

/// Clientes DRM de un proceso. `None` si su tabla de descriptores no es
/// legible.
fn clients_of(root: &Path, pid: i32) -> Option<Vec<DrmClient>> {
    let dir = root.join(pid.to_string());
    let entries = fs::read_dir(dir.join("fd")).ok()?;
    let mut clients = Vec::new();
    for entry in entries.flatten() {
        // Solo interesan los descriptores sobre /dev/dri: evita leer el
        // fdinfo de cada archivo y socket abierto.
        let Ok(target) = fs::read_link(entry.path()) else {
            continue;
        };
        if !target.starts_with("/dev/dri") {
            continue;
        }
        let info = dir.join("fdinfo").join(entry.file_name());
        if let Some(client) = fs::read_to_string(info).ok().and_then(|t| parse_fdinfo(&t)) {
            clients.push(client);
        }
    }
    Some(clients)
}

impl GpuTracker {
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

    pub fn sample(&mut self, pids: &[i32], now: Instant) -> GpuReading {
        let elapsed = self
            .last
            .map(|t| now.saturating_duration_since(t).as_secs_f64());
        self.last = Some(now);

        let mut readable = 0usize;
        let mut unique: HashMap<ClientKey, DrmClient> = HashMap::new();
        for &pid in pids {
            let Some(clients) = clients_of(&self.root, pid) else {
                continue;
            };
            readable += 1;
            for client in clients {
                let device = if client.device.is_empty() {
                    client.driver.clone()
                } else {
                    client.device.clone()
                };
                unique.entry((device, client.client_id)).or_insert(client);
            }
        }
        if readable == 0 {
            self.prev.clear();
            return GpuReading::Unavailable;
        }
        if unique.is_empty() {
            self.prev.clear();
            return GpuReading::NotUsed;
        }

        let mut stats = GpuStats {
            clients: unique.len(),
            ..Default::default()
        };
        // Motor → fracción ocupada, sumada entre clientes.
        let mut busy: HashMap<String, f64> = HashMap::new();
        let mut compared = false;
        let mut current: HashMap<ClientKey, HashMap<String, u64>> = HashMap::new();
        for (key, client) in &unique {
            let (memory, dedicated) = client.video_memory();
            stats.memory += memory;
            stats.dedicated |= dedicated;
            if stats.driver.is_empty() {
                stats.driver = client.driver.clone();
            }
            let previous = self.prev.get(key);
            for (engine, (ns, capacity)) in &client.engines {
                if let (Some(old), Some(dt)) = (previous.and_then(|p| p.get(engine)), elapsed) {
                    if dt > 0.0 {
                        compared = true;
                        let used = ns.saturating_sub(*old) as f64 / 1e9 / dt / *capacity as f64;
                        *busy.entry(engine.clone()).or_default() += used;
                    }
                }
            }
            let totals = client
                .engines
                .iter()
                .map(|(e, (ns, _))| (e.clone(), *ns))
                .collect();
            current.insert(key.clone(), totals);
        }
        self.prev = current;

        if !compared {
            return GpuReading::Warming(stats);
        }
        if let Some((engine, fraction)) = busy.into_iter().max_by(|a, b| a.1.total_cmp(&b.1)) {
            stats.busy_pct = (fraction * 100.0).clamp(0.0, 100.0) as f32;
            stats.engine = engine;
        }
        GpuReading::Stats(stats)
    }
}

impl Default for GpuTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::time::Duration;

    /// fdinfo de amdgpu (GPU dedicada), con el formato de la documentación
    /// del kernel (`Documentation/gpu/amdgpu/driver-misc.rst`).
    fn amdgpu(client: u64, gfx_ns: u64, vram_kib: u64) -> String {
        format!(
            "pos:\t0\nflags:\t02100002\nmnt_id:\t26\nino:\t284\ndrm-driver:\tamdgpu\n\
             drm-client-id:\t{client}\ndrm-pdev:\t0000:c2:00.0\npasid:\t32785\n\
             drm-total-cpu:\t0\ndrm-resident-cpu:\t0\ndrm-total-gtt:\t2048 KiB\n\
             drm-resident-gtt:\t2048 KiB\ndrm-total-vram:\t{vram_kib} KiB\n\
             drm-shared-vram:\t15220 KiB\ndrm-resident-vram:\t{vram_kib} KiB\n\
             drm-memory-vram:\t{vram_kib} KiB\ndrm-memory-gtt: \t2048 KiB\ndrm-memory-cpu: \t0 KiB\n\
             amd-evicted-vram:\t0 KiB\namd-requested-vram:\t{vram_kib} KiB\n\
             drm-engine-gfx:\t{gfx_ns} ns\ndrm-engine-compute:\t0 ns\ndrm-engine-dec:\t1000 ns\n"
        )
    }

    /// fdinfo de i915 (integrada), copiado de esta máquina.
    const I915: &str = "pos:\t0\nflags:\t02100002\nmnt_id:\t42\nino:\t601\ndrm-driver:\ti915\n\
        drm-client-id:\t114\ndrm-pdev:\t0000:00:02.0\ndrm-total-system0:\t34800 KiB\n\
        drm-shared-system0:\t12 MiB\ndrm-active-system0:\t0\ndrm-resident-system0:\t34800 KiB\n\
        drm-purgeable-system0:\t4368 KiB\ndrm-total-stolen-system0:\t0\n\
        drm-resident-stolen-system0:\t0\ndrm-engine-render:\t49107292 ns\ndrm-engine-copy:\t0 ns\n\
        drm-engine-video:\t0 ns\ndrm-engine-capacity-video:\t2\ndrm-engine-video-enhance:\t0 ns\n";

    /// Crea `root/<pid>/fd/<n>` → destino y `fdinfo/<n>` con el texto dado.
    fn add_fd(root: &Path, pid: i32, fd: u32, target: &str, info: &str) {
        let dir = root.join(pid.to_string());
        fs::create_dir_all(dir.join("fd")).unwrap();
        fs::create_dir_all(dir.join("fdinfo")).unwrap();
        let link = dir.join("fd").join(fd.to_string());
        let _ = fs::remove_file(&link);
        symlink(target, link).unwrap();
        fs::write(dir.join("fdinfo").join(fd.to_string()), info).unwrap();
    }

    #[test]
    fn parses_amdgpu_fdinfo() {
        let client = parse_fdinfo(&amdgpu(39, 217_467_930, 344_720)).unwrap();
        assert_eq!((client.driver.as_str(), client.client_id), ("amdgpu", 39));
        assert_eq!(client.device, "0000:c2:00.0");
        assert_eq!(client.engines["gfx"], (217_467_930, 1));
        assert_eq!(client.engines.len(), 3);
        assert_eq!(client.memory["vram"], 344_720 * 1024);
        assert_eq!(client.video_memory(), (344_720 * 1024, true));
    }

    #[test]
    fn parses_i915_fdinfo_with_capacity_and_mixed_units() {
        let client = parse_fdinfo(I915).unwrap();
        assert_eq!(client.engines["render"], (49_107_292, 1));
        assert_eq!(client.engines["video"], (0, 2));
        assert!(!client.engines.contains_key("capacity-video"));
        assert_eq!(client.resident["system0"], 34_800 * 1024);
        // Sin región vram: suma de lo residente, no dedicada.
        assert_eq!(client.video_memory(), (34_800 * 1024, false));
    }

    #[test]
    fn non_drm_fdinfo_is_ignored() {
        assert_eq!(parse_fdinfo("pos:\t0\nflags:\t02\nmnt_id:\t9\n"), None);
        assert_eq!(parse_fdinfo(""), None);
        assert_eq!(parse_size("7 XiB"), None);
        assert_eq!(parse_size("5"), Some(5));
    }

    #[test]
    fn busy_percent_is_the_engine_time_delta_and_clients_are_deduplicated() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // El cliente 39 aparece en dos descriptores del pid 1 y en el pid 2.
        add_fd(
            root,
            1,
            5,
            "/dev/dri/renderD128",
            &amdgpu(39, 1_000_000_000, 300_000),
        );
        add_fd(
            root,
            1,
            6,
            "/dev/dri/renderD128",
            &amdgpu(39, 1_000_000_000, 300_000),
        );
        add_fd(
            root,
            2,
            4,
            "/dev/dri/card1",
            &amdgpu(39, 1_000_000_000, 300_000),
        );
        add_fd(root, 2, 9, "/dev/dri/renderD128", &amdgpu(40, 0, 100_000));
        add_fd(root, 2, 3, "/tmp/archivo", "pos:\t0\n");
        let mut tracker = GpuTracker::with_root(root);
        let t0 = Instant::now();
        let GpuReading::Warming(first) = tracker.sample(&[1, 2], t0) else {
            panic!("la primera lectura no tiene porcentaje");
        };
        assert_eq!(first.clients, 2);
        assert_eq!(first.memory, 400_000 * 1024);
        assert!(first.dedicated);

        // En 2 s el cliente 39 usó 0.5 s de gfx y el 40 otros 0.3 s.
        for (pid, fd) in [(1, 5), (1, 6)] {
            add_fd(
                root,
                pid,
                fd,
                "/dev/dri/renderD128",
                &amdgpu(39, 1_500_000_000, 300_000),
            );
        }
        add_fd(
            root,
            2,
            4,
            "/dev/dri/card1",
            &amdgpu(39, 1_500_000_000, 300_000),
        );
        add_fd(
            root,
            2,
            9,
            "/dev/dri/renderD128",
            &amdgpu(40, 300_000_000, 100_000),
        );
        let GpuReading::Stats(stats) = tracker.sample(&[1, 2], t0 + Duration::from_secs(2)) else {
            panic!("la segunda lectura debe tener porcentaje");
        };
        assert_eq!(stats.engine, "gfx");
        assert!((stats.busy_pct - 40.0).abs() < 0.01, "{}", stats.busy_pct);
        assert_eq!((stats.driver.as_str(), stats.clients), ("amdgpu", 2));
    }

    #[test]
    fn distinguishes_not_used_from_unreadable() {
        let tmp = tempfile::tempdir().unwrap();
        add_fd(tmp.path(), 1, 3, "/tmp/archivo", "pos:\t0\n");
        let mut tracker = GpuTracker::with_root(tmp.path());
        assert_eq!(tracker.sample(&[1], Instant::now()), GpuReading::NotUsed);
        assert_eq!(
            tracker.sample(&[77], Instant::now()),
            GpuReading::Unavailable
        );
        assert_eq!(tracker.sample(&[], Instant::now()), GpuReading::Unavailable);
    }

    #[test]
    fn reads_the_real_proc_without_panicking() {
        let mut tracker = GpuTracker::new();
        let me = std::process::id() as i32;
        assert_eq!(tracker.sample(&[me], Instant::now()), GpuReading::NotUsed);
    }
}
