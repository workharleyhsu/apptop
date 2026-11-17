//! Acciones sobre procesos: señales y prioridad.

pub mod nice;
pub mod signal;

pub use nice::{set_nice, NiceReport, NICE_MAX, NICE_MIN};
pub use nix::sys::signal::Signal;
pub use signal::{
    ancestry, filter_targets, needs_confirmation, send_signal, SignalReport, Target, SIGNAL_MENU,
};
