//! Lectura de procesos y totales del sistema desde `/proc` y `/sys`.

pub mod proc;
pub mod system;

pub use proc::{Collector, ProcInfo};
pub use system::{SystemSampler, SystemStats};
