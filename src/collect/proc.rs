//! Muestreo de procesos desde `/proc/<pid>/`.
//!
//! Los procesos pueden morir entre dos lecturas: cualquier error al leer un
//! proceso hace que se descarte de la muestra, nunca un panic.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use procfs::process::{all_processes_with_root, Process};

/// Bit `PF_KTHREAD` del campo `flags` de `/proc/<pid>/stat`.
const PF_KTHREAD: u32 = 0x0020_0000;

/// Cuántas muestras seguidas se releen los datos "estáticos" de un proceso
/// nuevo. systemd mueve el proceso a su scope justo después del fork, así que
/// la primera lectura del cgroup puede ser la del lanzador.
const SETTLE_READS: u8 = 3;

/// Estado de un proceso en una muestra.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcInfo {
    pub pid: i32,
    pub ppid: i32,
    /// Nombre corto (`comm`).
    pub name: String,
    /// Ejecutable; `None` si no hay permiso para leer el enlace.
    pub exe: Option<PathBuf>,
    pub cmdline: Vec<String>,
    /// Ruta del cgroup (v2, o la jerarquía de systemd en v1).
    pub cgroup: Option<String>,
    /// Letra de estado del kernel (`R`, `S`, `T`, `Z`, ...).
    pub state: char,
    /// Porcentaje de la CPU total de la máquina (100 = todos los núcleos
    /// ocupados) desde la muestra anterior; 0 en la primera.
    pub cpu_pct: f32,
    /// Memoria residente en bytes.
    pub rss: u64,
    pub threads: i64,
    pub nice: i64,
    pub uid: Option<u32>,
    /// Inicio del proceso en ticks desde el arranque del sistema.
    pub starttime: u64,
    /// Ticks de CPU acumulados (usuario + sistema).
    pub cpu_ticks: u64,
    pub kthread: bool,
}

struct Cached {
    starttime: u64,
    comm: String,
    exe: Option<PathBuf>,
    cmdline: Vec<String>,
    cgroup: Option<String>,
    uid: Option<u32>,
    reads: u8,
    ticks: u64,
    generation: u64,
}

/// Toma muestras sucesivas de todos los procesos y calcula el uso de CPU
/// como diferencia entre muestras.
pub struct Collector {
    root: PathBuf,
    cache: HashMap<i32, Cached>,
    last: Option<Instant>,
    generation: u64,
    ticks_per_second: f64,
    /// Núcleos de la máquina: el uso se expresa sobre el total, igual que
    /// la barra de CPU del sistema.
    cores: f64,
    page_size: u64,
}

impl Default for Collector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector {
    pub fn new() -> Self {
        Self::with_root("/proc")
    }

    /// Usa otra raíz en lugar de `/proc` (para tests con fixtures).
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            cache: HashMap::new(),
            last: None,
            generation: 0,
            ticks_per_second: procfs::ticks_per_second() as f64,
            cores: 1.0,
            page_size: procfs::page_size(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Cantidad de núcleos según `<raíz>/stat`; 1 si no se puede leer.
    fn read_cores(&self) -> f64 {
        fs::read_to_string(self.root.join("stat"))
            .ok()
            .and_then(|text| super::system::parse_stat(&text))
            .map(|(_, cores)| cores)
            .filter(|cores| *cores > 0)
            .unwrap_or(1) as f64
    }

    pub fn ticks_per_second(&self) -> f64 {
        self.ticks_per_second
    }

    pub fn sample(&mut self) -> Vec<ProcInfo> {
        self.sample_at(Instant::now())
    }

    /// Igual que [`Collector::sample`] pero con el reloj inyectado.
    pub fn sample_at(&mut self, now: Instant) -> Vec<ProcInfo> {
        let elapsed = self
            .last
            .map(|t| now.saturating_duration_since(t).as_secs_f64());
        self.last = Some(now);
        self.generation += 1;
        // Puede cambiar en caliente (hotplug): se relee en cada muestra.
        self.cores = self.read_cores();

        let mut out = Vec::with_capacity(self.cache.len().max(64));
        if let Ok(iter) = all_processes_with_root(&self.root) {
            for proc in iter.flatten() {
                if let Some(info) = self.read_one(&proc, elapsed) {
                    out.push(info);
                }
            }
        }
        let generation = self.generation;
        self.cache.retain(|_, c| c.generation == generation);
        out.sort_by_key(|p| p.pid);
        out
    }

    fn read_one(&mut self, proc: &Process, elapsed: Option<f64>) -> Option<ProcInfo> {
        let stat = proc.stat().ok()?;
        let pid = proc.pid;
        let ticks = stat.utime.saturating_add(stat.stime);

        let reusable = self
            .cache
            .get(&pid)
            .is_some_and(|c| c.starttime == stat.starttime);
        let mut cpu_pct = 0.0;
        if reusable {
            if let (Some(c), Some(dt)) = (self.cache.get(&pid), elapsed) {
                if dt > 0.0 {
                    let delta = ticks.saturating_sub(c.ticks) as f64;
                    cpu_pct = (delta / self.ticks_per_second / dt / self.cores * 100.0) as f32;
                }
            }
        }

        let stale = match self.cache.get(&pid) {
            Some(c) => !reusable || c.comm != stat.comm || c.reads < SETTLE_READS,
            None => true,
        };
        if stale {
            let reads = match self.cache.get(&pid) {
                Some(c) if reusable => c.reads.saturating_add(1),
                _ => 1,
            };
            let dir = self.root.join(pid.to_string());
            self.cache.insert(
                pid,
                Cached {
                    starttime: stat.starttime,
                    comm: stat.comm.clone(),
                    exe: proc.exe().ok(),
                    cmdline: proc.cmdline().unwrap_or_default(),
                    cgroup: read_cgroup(&dir),
                    uid: proc.uid().ok(),
                    reads,
                    ticks,
                    generation: self.generation,
                },
            );
        }
        let cached = self.cache.get_mut(&pid)?;
        cached.ticks = ticks;
        cached.generation = self.generation;

        Some(ProcInfo {
            pid,
            ppid: stat.ppid,
            name: stat.comm,
            exe: cached.exe.clone(),
            cmdline: cached.cmdline.clone(),
            cgroup: cached.cgroup.clone(),
            state: stat.state,
            cpu_pct,
            rss: stat.rss.saturating_mul(self.page_size),
            threads: stat.num_threads,
            nice: stat.nice,
            uid: cached.uid,
            starttime: stat.starttime,
            cpu_ticks: ticks,
            kthread: stat.flags & PF_KTHREAD != 0,
        })
    }

    /// PSS en bytes desde `smaps_rollup`. Es una lectura cara: usarla solo
    /// para los procesos que se están mostrando en detalle.
    pub fn pss(&self, pid: i32) -> Option<u64> {
        let text = fs::read_to_string(self.root.join(pid.to_string()).join("smaps_rollup")).ok()?;
        parse_pss(&text)
    }
}

/// Extrae el campo `Pss:` (en kB) de `smaps_rollup` y lo devuelve en bytes.
pub fn parse_pss(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("Pss:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb.saturating_mul(1024))
}

fn read_cgroup(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("cgroup")).ok()?;
    parse_cgroup(&text)
}

/// Devuelve la ruta del cgroup: la línea `0::` en cgroup v2, o la jerarquía
/// `name=systemd` en v1.
pub fn parse_cgroup(text: &str) -> Option<String> {
    let mut v1 = None;
    for line in text.lines() {
        let mut parts = line.splitn(3, ':');
        let (Some(id), Some(controllers), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if id == "0" && controllers.is_empty() {
            return Some(path.to_string());
        }
        if controllers == "name=systemd" {
            v1 = Some(path.to_string());
        }
    }
    v1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Escribe un proceso falso en `root/<pid>/`.
    fn fake_proc(root: &Path, pid: i32, comm: &str, ppid: i32, ticks: (u64, u64), cgroup: &str) {
        let dir = root.join(pid.to_string());
        fs::create_dir_all(&dir).unwrap();
        let stat = format!(
            "{pid} ({comm}) S {ppid} {pid} {pid} 0 -1 4194304 100 0 0 0 {} {} 0 0 20 0 3 0 5000 \
             1000000 250 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 1 0 0 0 0 0 0 0 0 0 0 0 0 0",
            ticks.0, ticks.1
        );
        fs::write(dir.join("stat"), stat).unwrap();
        fs::write(dir.join("cmdline"), format!("/usr/bin/{comm}\0--flag\0")).unwrap();
        fs::write(dir.join("cgroup"), format!("0::{cgroup}\n")).unwrap();
    }

    #[test]
    fn reads_fields_from_fixture() {
        let tmp = tempfile::tempdir().unwrap();
        fake_proc(tmp.path(), 42, "my app", 1, (10, 5), "/user.slice/x.scope");
        let mut c = Collector::with_root(tmp.path());
        let procs = c.sample();
        assert_eq!(procs.len(), 1);
        let p = &procs[0];
        assert_eq!((p.pid, p.ppid, p.name.as_str()), (42, 1, "my app"));
        assert_eq!(p.cmdline, vec!["/usr/bin/my app", "--flag"]);
        assert_eq!(p.cgroup.as_deref(), Some("/user.slice/x.scope"));
        assert_eq!(p.state, 'S');
        assert_eq!(p.threads, 3);
        assert_eq!(p.rss, 250 * procfs::page_size());
        assert_eq!(p.cpu_pct, 0.0);
        assert!(!p.kthread);
    }

    #[test]
    fn cpu_percent_is_delta_over_wall_time() {
        let tmp = tempfile::tempdir().unwrap();
        fake_proc(tmp.path(), 7, "burn", 1, (100, 0), "/");
        let mut c = Collector::with_root(tmp.path());
        let t0 = Instant::now();
        c.sample_at(t0);
        let half_second = (c.ticks_per_second() / 2.0) as u64;
        fake_proc(tmp.path(), 7, "burn", 1, (100 + half_second, 0), "/");
        let procs = c.sample_at(t0 + Duration::from_secs(1));
        assert!(
            (procs[0].cpu_pct - 50.0).abs() < 1.5,
            "{}",
            procs[0].cpu_pct
        );
    }

    #[test]
    fn cpu_percent_is_relative_to_the_whole_machine() {
        let tmp = tempfile::tempdir().unwrap();
        // Máquina de 4 núcleos: un proceso que ocupa un núcleo entero es 25 %.
        let stat = "cpu  1 0 1 1 0 0 0 0\ncpu0 0 0 0 0\ncpu1 0 0 0 0\ncpu2 0 0 0 0\ncpu3 0 0 0 0\n";
        fs::write(tmp.path().join("stat"), stat).unwrap();
        fake_proc(tmp.path(), 7, "burn", 1, (100, 0), "/");
        let mut c = Collector::with_root(tmp.path());
        let t0 = Instant::now();
        c.sample_at(t0);
        let one_second = c.ticks_per_second() as u64;
        fake_proc(tmp.path(), 7, "burn", 1, (100 + one_second, 0), "/");
        let procs = c.sample_at(t0 + Duration::from_secs(1));
        assert!(
            (procs[0].cpu_pct - 25.0).abs() < 0.5,
            "{}",
            procs[0].cpu_pct
        );
    }

    #[test]
    fn pid_reuse_does_not_produce_bogus_cpu() {
        let tmp = tempfile::tempdir().unwrap();
        fake_proc(tmp.path(), 7, "old", 1, (5000, 0), "/");
        let mut c = Collector::with_root(tmp.path());
        let t0 = Instant::now();
        c.sample_at(t0);
        // Mismo pid, otro proceso (starttime distinto).
        let dir = tmp.path().join("7");
        let stat = fs::read_to_string(dir.join("stat")).unwrap();
        fs::write(
            dir.join("stat"),
            stat.replace(" 5000 1000000 ", " 9000 1000000 "),
        )
        .unwrap();
        let procs = c.sample_at(t0 + Duration::from_secs(1));
        assert_eq!(procs[0].cpu_pct, 0.0);
        assert_eq!(procs[0].starttime, 9000);
    }

    #[test]
    fn vanished_and_broken_processes_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        fake_proc(tmp.path(), 1, "ok", 0, (1, 1), "/");
        fs::create_dir_all(tmp.path().join("2")).unwrap(); // sin stat
        fs::create_dir_all(tmp.path().join("3")).unwrap();
        fs::write(tmp.path().join("3/stat"), "basura").unwrap();
        let mut c = Collector::with_root(tmp.path());
        assert_eq!(c.sample().len(), 1);
        fs::remove_dir_all(tmp.path().join("1")).unwrap();
        assert!(c.sample().is_empty());
    }

    #[test]
    fn parses_cgroup_v2_and_v1() {
        assert_eq!(
            parse_cgroup("0::/a/b.scope\n").as_deref(),
            Some("/a/b.scope")
        );
        let v1 = "12:pids:/x\n1:name=systemd:/user.slice/foo.service\n";
        assert_eq!(parse_cgroup(v1).as_deref(), Some("/user.slice/foo.service"));
        assert_eq!(parse_cgroup(""), None);
    }

    #[test]
    fn parses_pss() {
        let text = "55d0-7ffd ---p 00000000 00:00 0 [rollup]\nRss:  2048 kB\nPss:   512 kB\nPss_Dirty: 1 kB\n";
        assert_eq!(parse_pss(text), Some(512 * 1024));
        assert_eq!(parse_pss("nada"), None);
    }

    #[test]
    fn samples_the_real_proc_without_panicking() {
        let mut c = Collector::new();
        let procs = c.sample();
        let me = std::process::id() as i32;
        let mine = procs.iter().find(|p| p.pid == me).expect("proceso propio");
        assert!(mine.rss > 0);
        assert!(mine.exe.is_some());
        assert!(c.pss(me).is_some());
    }
}
