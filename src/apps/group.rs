//! Agrupa procesos en aplicaciones y servicios.

use std::collections::{BTreeMap, HashMap};

use super::cgroup::{classify, Unit};
use crate::collect::ProcInfo;

/// Sección de la grilla en la que aparece un grupo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppKind {
    App,
    Service,
}

/// De dónde salió la agrupación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Origin {
    Scope,
    Flatpak,
    UserService,
    SystemService,
    Container,
    /// Sin cgroup útil: agrupado por ejecutable y árbol de padres.
    Fallback,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Origin::Scope => "app",
            Origin::Flatpak => "flatpak",
            Origin::UserService => "serv-usuario",
            Origin::SystemService => "serv-sistema",
            Origin::Container => "contenedor",
            Origin::Fallback => "proceso",
        }
    }
}

/// Una aplicación o servicio: un conjunto de procesos relacionados.
#[derive(Debug, Clone, PartialEq)]
pub struct App {
    /// Clave estable entre muestras.
    pub id: String,
    pub name: String,
    pub kind: AppKind,
    pub origin: Origin,
    /// Identificador para buscar el `.desktop`, si el cgroup lo aporta.
    pub app_id: Option<String>,
    /// Procesos ordenados por pid.
    pub procs: Vec<ProcInfo>,
    pub cpu_pct: f32,
    pub rss: u64,
}

impl App {
    pub fn pids(&self) -> Vec<i32> {
        self.procs.iter().map(|p| p.pid).collect()
    }
}

/// Nombre a mostrar derivado de un AppID (`org.gnome.Nautilus` → `Nautilus`).
pub fn pretty_app_id(id: &str) -> String {
    let dots = id.matches('.').count();
    if dots >= 2 {
        let last = id.rsplit('.').next().unwrap_or(id);
        if !last.is_empty() {
            return last.to_string();
        }
    }
    id.to_string()
}

/// Nombre del ejecutable de un proceso, o su `comm` si no es legible.
pub fn exe_name(p: &ProcInfo) -> String {
    p.exe
        .as_ref()
        .and_then(|e| e.file_name())
        .map(|n| {
            let n = n.to_string_lossy();
            n.strip_suffix(" (deleted)").unwrap_or(&n).to_string()
        })
        .unwrap_or_else(|| p.name.clone())
}

struct Key {
    id: String,
    name: String,
    kind: AppKind,
    origin: Origin,
    app_id: Option<String>,
}

fn key_for_unit(unit: Unit) -> Option<Key> {
    match unit {
        Unit::AppScope { app_id, flatpak } => Some(Key {
            id: format!("app:{app_id}"),
            name: pretty_app_id(&app_id),
            kind: AppKind::App,
            origin: if flatpak {
                Origin::Flatpak
            } else {
                Origin::Scope
            },
            app_id: Some(app_id),
        }),
        Unit::Service { name, user, app_id } => Some(Key {
            id: format!("{}:{name}", if user { "usvc" } else { "svc" }),
            name: app_id.as_deref().map(pretty_app_id).unwrap_or(name),
            kind: AppKind::Service,
            origin: if user {
                Origin::UserService
            } else {
                Origin::SystemService
            },
            app_id,
        }),
        Unit::Container { runtime, id } => {
            let short: String = id.chars().take(12).collect();
            Some(Key {
                id: format!("ctr:{id}"),
                name: format!("{runtime} {short}"),
                kind: AppKind::Service,
                origin: Origin::Container,
                app_id: None,
            })
        }
        Unit::None => None,
    }
}

/// Agrupa una muestra de procesos. Los hilos del kernel se descartan.
///
/// Los procesos sin cgroup útil se agrupan por árbol de padres: cada árbol
/// toma el nombre del ejecutable de su raíz, y los árboles con el mismo
/// ejecutable se fusionan.
pub fn group_apps(procs: Vec<ProcInfo>) -> Vec<App> {
    let mut groups: BTreeMap<String, App> = BTreeMap::new();
    let mut orphans: Vec<ProcInfo> = Vec::new();

    for p in procs {
        if p.kthread {
            continue;
        }
        let unit = p.cgroup.as_deref().map(classify).unwrap_or(Unit::None);
        match key_for_unit(unit) {
            Some(key) => push(&mut groups, key, p),
            None => orphans.push(p),
        }
    }

    // Fallback: sube por los padres mientras sigan siendo huérfanos.
    let parent_of: HashMap<i32, i32> = orphans.iter().map(|p| (p.pid, p.ppid)).collect();
    let names: HashMap<i32, String> = orphans.iter().map(|p| (p.pid, exe_name(p))).collect();
    for p in orphans {
        let mut root = p.pid;
        // El límite evita ciclos si /proc cambió a mitad de la lectura.
        for _ in 0..64 {
            match parent_of.get(&root) {
                Some(parent) if parent_of.contains_key(parent) && *parent != root => root = *parent,
                _ => break,
            }
        }
        let name = names.get(&root).cloned().unwrap_or_else(|| p.name.clone());
        let key = Key {
            id: format!("exe:{name}"),
            name,
            kind: AppKind::Service,
            origin: Origin::Fallback,
            app_id: None,
        };
        push(&mut groups, key, p);
    }

    let mut apps: Vec<App> = groups.into_values().collect();
    for app in &mut apps {
        app.procs.sort_by_key(|p| p.pid);
        app.cpu_pct = app.procs.iter().map(|p| p.cpu_pct).sum();
        app.rss = app.procs.iter().map(|p| p.rss).sum();
    }
    apps.sort_by(|a, b| {
        (a.kind, a.name.to_lowercase(), &a.id).cmp(&(b.kind, b.name.to_lowercase(), &b.id))
    });
    apps
}

fn push(groups: &mut BTreeMap<String, App>, key: Key, p: ProcInfo) {
    groups
        .entry(key.id.clone())
        .or_insert_with(|| App {
            id: key.id,
            name: key.name,
            kind: key.kind,
            origin: key.origin,
            app_id: key.app_id,
            procs: Vec::new(),
            cpu_pct: 0.0,
            rss: 0,
        })
        .procs
        .push(p);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    pub(crate) fn proc(pid: i32, ppid: i32, exe: &str, cgroup: &str) -> ProcInfo {
        ProcInfo {
            pid,
            ppid,
            name: exe.rsplit('/').next().unwrap_or(exe).to_string(),
            exe: Some(PathBuf::from(exe)),
            cmdline: vec![exe.to_string()],
            cgroup: Some(cgroup.to_string()),
            state: 'S',
            cpu_pct: 1.5,
            rss: 1000,
            threads: 1,
            nice: 0,
            uid: Some(1000),
            starttime: pid as u64,
            cpu_ticks: 0,
            kthread: false,
        }
    }

    const APP: &str = "/user.slice/user-1000.slice/user@1000.service/app.slice";

    #[test]
    fn merges_instances_of_the_same_app() {
        let apps = group_apps(vec![
            proc(
                10,
                1,
                "/usr/bin/alacritty",
                &format!("{APP}/app-niri-alacritty-10.scope"),
            ),
            proc(
                11,
                10,
                "/usr/bin/fish",
                &format!("{APP}/app-niri-alacritty-10.scope"),
            ),
            proc(
                20,
                1,
                "/usr/bin/alacritty",
                &format!("{APP}/app-niri-alacritty-20.scope"),
            ),
        ]);
        assert_eq!(apps.len(), 1);
        let a = &apps[0];
        assert_eq!(
            (a.name.as_str(), a.kind, a.origin),
            ("alacritty", AppKind::App, Origin::Scope)
        );
        assert_eq!(a.pids(), vec![10, 11, 20]);
        assert_eq!(a.rss, 3000);
        assert!((a.cpu_pct - 4.5).abs() < 1e-4);
    }

    #[test]
    fn classifies_flatpak_services_and_containers() {
        let apps = group_apps(vec![
            proc(
                1,
                0,
                "/app/bin/moonlight",
                &format!("{APP}/app-flatpak-com.moonlight_stream.Moonlight-1.scope"),
            ),
            proc(
                2,
                0,
                "/usr/bin/pipewire",
                "/user.slice/user-1000.slice/user@1000.service/session.slice/pipewire.service",
            ),
            proc(3, 0, "/usr/bin/dockerd", "/system.slice/docker.service"),
            proc(
                4,
                0,
                "/usr/bin/postgres",
                "/system.slice/docker-7ef05203aac0c961d1f8.scope",
            ),
        ]);
        let by_name: HashMap<&str, &App> = apps.iter().map(|a| (a.name.as_str(), a)).collect();
        assert_eq!(by_name["Moonlight"].origin, Origin::Flatpak);
        assert_eq!(by_name["Moonlight"].kind, AppKind::App);
        assert_eq!(by_name["pipewire"].origin, Origin::UserService);
        assert_eq!(by_name["docker"].origin, Origin::SystemService);
        assert_eq!(by_name["docker 7ef05203aac0"].origin, Origin::Container);
        // Las apps van antes que los servicios.
        assert_eq!(apps[0].name, "Moonlight");
    }

    #[test]
    fn fallback_groups_by_parent_tree_and_executable() {
        let s = "/user.slice/user-1000.slice/session-2.scope";
        let apps = group_apps(vec![
            proc(100, 1, "/usr/libexec/gdm-session-worker", s),
            proc(101, 100, "/usr/bin/niri-session", s),
            proc(102, 101, "/usr/bin/fish", s),
            proc(200, 1, "/usr/bin/gnome-keyring-daemon", s),
            proc(
                300,
                1,
                "/usr/bin/gnome-keyring-daemon",
                "/user.slice/user-1000.slice/session-5.scope",
            ),
        ]);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "gdm-session-worker");
        assert_eq!(apps[0].pids(), vec![100, 101, 102]);
        assert_eq!(apps[0].origin, Origin::Fallback);
        assert_eq!(apps[1].pids(), vec![200, 300]);
    }

    #[test]
    fn skips_kernel_threads_and_survives_parent_cycles() {
        let mut k = proc(2, 0, "", "/");
        k.kthread = true;
        k.exe = None;
        let apps = group_apps(vec![
            k,
            proc(5, 6, "/bin/a", "/"),
            proc(6, 5, "/bin/b", "/"),
        ]);
        assert_eq!(apps.iter().map(|a| a.procs.len()).sum::<usize>(), 2);
    }

    #[test]
    fn exe_name_handles_deleted_and_missing() {
        let mut p = proc(1, 0, "/usr/bin/foo (deleted)", "/");
        assert_eq!(exe_name(&p), "foo");
        p.exe = None;
        p.name = "comm".into();
        assert_eq!(exe_name(&p), "comm");
    }

    #[test]
    fn pretty_ids() {
        assert_eq!(pretty_app_id("org.gnome.Nautilus"), "Nautilus");
        assert_eq!(pretty_app_id("alacritty"), "alacritty");
        assert_eq!(pretty_app_id("foo.bar"), "foo.bar");
    }
}
