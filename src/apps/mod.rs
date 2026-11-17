//! Agrupación de procesos en aplicaciones y asociación con archivos `.desktop`.

pub mod cgroup;
pub mod desktop;
pub mod group;

pub use cgroup::{classify, Unit};
pub use desktop::{DesktopIndex, DesktopInfo};
pub use group::{group_apps, group_apps_with, App, AppKind, Origin};
