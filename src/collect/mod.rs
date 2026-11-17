//! Lectura de procesos y totales del sistema desde `/proc` y `/sys`.

pub mod proc;

pub use proc::{Collector, ProcInfo};
