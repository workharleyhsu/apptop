//! Lectura de procesos y totales del sistema desde `/proc` y `/sys`.

pub mod io;
pub mod proc;
pub mod system;

pub use io::{IoRate, IoReading, IoTracker};
pub use proc::{Collector, ProcInfo};
pub use system::{SystemSampler, SystemStats};
