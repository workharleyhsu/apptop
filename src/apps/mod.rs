//! Agrupación de procesos en aplicaciones y asociación con archivos `.desktop`.

pub mod cgroup;
pub mod desktop;
pub mod group;

pub use cgroup::{classify, Unit};
pub use desktop::{DesktopIndex, DesktopInfo};
pub use group::{group_apps, group_apps_with, App, AppKind, Origin};

use crate::collect::Collector;

/// Toma muestras de procesos y las entrega ya agrupadas en apps.
pub struct Sampler {
    collector: Collector,
    index: DesktopIndex,
}

impl Sampler {
    /// Carga el índice de `.desktop` y toma una primera muestra, para que la
    /// siguiente ya tenga porcentajes de CPU.
    pub fn new() -> Self {
        let mut collector = Collector::new();
        collector.sample();
        Self {
            collector,
            index: DesktopIndex::load(),
        }
    }

    pub fn sample(&mut self) -> Vec<App> {
        group_apps_with(self.collector.sample(), Some(&self.index))
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
