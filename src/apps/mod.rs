//! Agrupación de procesos en aplicaciones y asociación con archivos `.desktop`.

pub mod cgroup;
pub mod desktop;
pub mod group;

pub use cgroup::{classify, Unit};
pub use desktop::{DesktopIndex, DesktopInfo};
pub use group::{group_apps, group_apps_with, App, AppKind, Origin};

use std::time::Instant;

use crate::collect::{
    Collector, GpuReading, GpuTracker, IoReading, IoTracker, SystemSampler, SystemStats,
};

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
}

/// Toma muestras de procesos y las entrega ya agrupadas en apps.
pub struct Sampler {
    collector: Collector,
    system: SystemSampler,
    index: DesktopIndex,
    io: IoTracker,
    gpu: GpuTracker,
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
            self.detail_id = Some(app.id.clone());
        }
        let ids: Vec<(i32, u64)> = app.procs.iter().map(|p| (p.pid, p.starttime)).collect();
        // Ajeno o ya muerto → ese proceso no suma; todos ilegibles → n/d.
        let pss: Vec<u64> = app
            .procs
            .iter()
            .filter_map(|p| self.collector.pss(p.pid))
            .collect();
        DetailData {
            app_id: app.id.clone(),
            pss: (!pss.is_empty()).then(|| pss.iter().sum()),
            io: self.io.sample(&ids, Instant::now()),
            gpu: self.gpu.sample(&app.pids(), Instant::now()),
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
