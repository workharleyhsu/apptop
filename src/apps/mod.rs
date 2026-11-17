//! Agrupación de procesos en aplicaciones y asociación con archivos `.desktop`.

pub mod cgroup;
pub mod desktop;
pub mod group;

pub use cgroup::{classify, Unit};
pub use desktop::{DesktopIndex, DesktopInfo, ExecMatch};
pub use group::{group_apps, group_apps_with, App, AppKind, Origin};

use std::time::{Duration, Instant};

use crate::collect::fds::{self, FdScan};
use crate::collect::{
    Collector, GpuReading, GpuTracker, IoReading, IoTracker, NetReading, NetTracker, NetlinkSource,
    SystemSampler, SystemStats,
};

/// Cada cuánto se vuelve a medir el PSS de la app abierta en detalle.
/// `smaps_rollup` hace que el kernel recorra todas las páginas del proceso:
/// para un navegador de varios GB es, por lejos, la lectura más cara.
pub const PSS_INTERVAL: Duration = Duration::from_secs(5);

/// Resultado de una muestra: apps agrupadas y totales del sistema.
#[derive(Debug, Clone, Default)]
pub struct Sample {
    pub apps: Vec<App>,
    pub system: SystemStats,
    /// Ticks de reloj por segundo (para convertir `starttime` a tiempo).
    pub ticks_per_second: f64,
    /// Datos caros, solo de la app abierta en detalle.
    pub detail: Option<DetailData>,
}

/// Mediciones que solo se toman para la app abierta en la vista de detalle.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DetailData {
    /// Id de la app a la que pertenecen.
    pub app_id: String,
    /// PSS total en bytes; `None` si no se pudo leer de ningún proceso.
    pub pss: Option<u64>,
    /// Lectura y escritura a disco por segundo.
    pub io: IoReading,
    /// Uso de GPU y memoria de vídeo.
    pub gpu: GpuReading,
    /// Tráfico TCP aproximado.
    pub net: NetReading,
}

/// Toma muestras de procesos y las entrega ya agrupadas en apps.
pub struct Sampler {
    collector: Collector,
    system: SystemSampler,
    index: DesktopIndex,
    io: IoTracker,
    gpu: GpuTracker,
    net: NetTracker,
    /// Último PSS medido y cuándo: es la lectura más cara, no se repite en
    /// cada refresco.
    pss: Option<(Instant, Option<u64>)>,
    /// App medida en detalle la vez anterior.
    detail_id: Option<String>,
}

impl Sampler {
    /// Carga el índice de `.desktop` y toma una primera muestra, para que la
    /// siguiente ya tenga porcentajes de CPU.
    pub fn new() -> Self {
        let mut collector = Collector::new();
        let mut system = SystemSampler::new();
        collector.sample();
        system.sample();
        Self {
            collector,
            system,
            index: DesktopIndex::load(),
            io: IoTracker::new(),
            gpu: GpuTracker::new(),
            net: NetTracker::new(),
            pss: None,
            detail_id: None,
        }
    }

    pub fn sample(&mut self) -> Sample {
        self.sample_with_detail(None)
    }

    /// Toma una muestra y, si `detail` nombra una app en ejecución, agrega
    /// sus mediciones caras.
    pub fn sample_with_detail(&mut self, detail: Option<&str>) -> Sample {
        let apps = group_apps_with(self.collector.sample(), Some(&self.index));
        let detail = detail
            .and_then(|id| apps.iter().find(|a| a.id == id))
            .map(|app| self.detail_for(app));
        Sample {
            apps,
            system: self.system.sample(),
            ticks_per_second: self.collector.ticks_per_second(),
            detail,
        }
    }

    fn detail_for(&mut self, app: &App) -> DetailData {
        // Al cambiar de app las referencias anteriores no sirven.
        if self.detail_id.as_deref() != Some(app.id.as_str()) {
            self.io = IoTracker::new();
            self.gpu = GpuTracker::new();
            self.net = NetTracker::new();
            self.pss = None;
            self.detail_id = Some(app.id.clone());
        }
        let ids: Vec<(i32, u64)> = app.procs.iter().map(|p| (p.pid, p.starttime)).collect();
        let now = Instant::now();
        let pss = match self.pss {
            Some((at, value)) if now.saturating_duration_since(at) < PSS_INTERVAL => value,
            _ => {
                // Ajeno o ya muerto → ese proceso no suma; todos ilegibles → n/d.
                let values: Vec<u64> = app
                    .procs
                    .iter()
                    .filter_map(|p| self.collector.pss(p.pid))
                    .collect();
                let value = (!values.is_empty()).then(|| values.iter().sum());
                self.pss = Some((now, value));
                value
            }
        };
        // Un solo recorrido de descriptores por proceso, compartido por la
        // medición de GPU y la de red.
        let root = self.collector.root().to_path_buf();
        let scans: Vec<(i32, Option<FdScan>)> = app
            .procs
            .iter()
            .map(|p| (p.pid, fds::scan(&root, p.pid)))
            .collect();
        DetailData {
            app_id: app.id.clone(),
            pss,
            io: self.io.sample(&ids, now),
            gpu: self.gpu.sample_scanned(&scans, now),
            net: self.net.sample_scanned(&scans, now, &mut NetlinkSource),
        }
    }

    pub fn collector(&self) -> &Collector {
        &self.collector
    }

    pub fn index(&self) -> &DesktopIndex {
        &self.index
    }
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}
