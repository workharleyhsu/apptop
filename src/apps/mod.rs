//! Agrupación de procesos en aplicaciones y asociación con archivos `.desktop`.

pub mod cgroup;
pub mod desktop;
pub mod group;

pub use cgroup::{classify, Unit};
pub use desktop::{DesktopIndex, DesktopInfo};
pub use group::{group_apps, group_apps_with, App, AppKind, Origin};

use crate::collect::{Collector, SystemSampler, SystemStats};

/// Resultado de una muestra: apps agrupadas y totales del sistema.
#[derive(Debug, Clone, Default)]
pub struct Sample {
    pub apps: Vec<App>,
    pub system: SystemStats,
}

/// Toma muestras de procesos y las entrega ya agrupadas en apps.
pub struct Sampler {
    collector: Collector,
    system: SystemSampler,
    index: DesktopIndex,
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
        }
    }

    pub fn sample(&mut self) -> Sample {
        Sample {
            apps: group_apps_with(self.collector.sample(), Some(&self.index)),
            system: self.system.sample(),
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
