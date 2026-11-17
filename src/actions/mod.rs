//! Acciones sobre procesos: señales y prioridad.

pub mod signal;

pub use nix::sys::signal::Signal;
pub use signal::{ancestry, filter_targets, send_signal, SignalReport, Target};
