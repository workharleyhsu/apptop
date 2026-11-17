//! Agrupación de procesos en aplicaciones y asociación con archivos `.desktop`.

pub mod cgroup;
pub mod group;

pub use cgroup::{classify, Unit};
pub use group::{group_apps, App, AppKind, Origin};
