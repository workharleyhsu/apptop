//! Agrupa procesos en aplicaciones y servicios.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use super::cgroup::{classify, Unit};
use super::desktop::{DesktopIndex, DesktopInfo};
use crate::collect::ProcInfo;
use crate::i18n::Lang;

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
    pub fn label(self, lang: Lang) -> &'static str {
        let t = lang.t();
        match self {
            Origin::Scope => t.origin_app,
            Origin::Flatpak => t.origin_flatpak,
            Origin::UserService => t.origin_user_service,
            Origin::SystemService => t.origin_system_service,
            Origin::Container => t.origin_container,
            Origin::Fallback => t.origin_process,
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
    /// `.desktop` asociado: de ahí salen el nombre visible y el icono.
    pub desktop: Option<Arc<DesktopInfo>>,
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
    desktop: Option<Arc<DesktopInfo>>,
}

impl Key {
    /// Clave de una app identificada por su `.desktop`. Es la misma venga del
    /// scope propio o de un subárbol separado de otro grupo.
    fn for_desktop(desktop: Arc<DesktopInfo>, origin: Origin, app_id: Option<String>) -> Key {
        Key {
            id: format!("app:{}", desktop.id.to_lowercase()),
            name: desktop.name.clone(),
            kind: AppKind::App,
            origin,
            app_id,
            desktop: Some(desktop),
        }
    }
}

fn key_for_unit(unit: Unit, index: Option<&DesktopIndex>) -> Option<Key> {
    match unit {
        Unit::AppScope { app_id, flatpak } => {
            let origin = if flatpak {
                Origin::Flatpak
            } else {
                Origin::Scope
            };
            match index.and_then(|i| i.by_app_id(&app_id)) {
                Some(desktop) => Some(Key::for_desktop(desktop, origin, Some(app_id))),
                None => Some(Key {
                    id: format!("app:{}", app_id.to_lowercase()),
                    name: pretty_app_id(&app_id),
                    kind: AppKind::App,
                    origin,
                    app_id: Some(app_id),
                    desktop: None,
                }),
            }
        }
        Unit::Service { name, user, app_id } => {
            let desktop = index.and_then(|i| match &app_id {
                Some(id) => i.by_app_id(id),
                None => i.by_exact_id(&name),
            });
            // Un servicio corriente conserva el nombre de su unidad; el
            // `.desktop` solo le presta el icono.
            let shown = match (&app_id, &desktop) {
                (Some(_), Some(d)) => d.name.clone(),
                (Some(id), None) => pretty_app_id(id),
                (None, _) => name.clone(),
            };
            Some(Key {
                id: format!("{}:{name}", if user { "usvc" } else { "svc" }),
                name: shown,
                kind: AppKind::Service,
                origin: if user {
                    Origin::UserService
                } else {
                    Origin::SystemService
                },
                app_id,
                desktop,
            })
        }
        Unit::Container { runtime, id } => {
            let short: String = id.chars().take(12).collect();
            Some(Key {
                id: format!("ctr:{id}"),
                name: format!("{runtime} {short}"),
                kind: AppKind::Service,
                origin: Origin::Container,
                app_id: None,
                desktop: None,
            })
        }
        Unit::None => None,
    }
}

/// Agrupa una muestra de procesos sin consultar archivos `.desktop`.
pub fn group_apps(procs: Vec<ProcInfo>) -> Vec<App> {
    group_apps_with(procs, None)
}

/// Agrupa una muestra de procesos. Los hilos del kernel se descartan.
///
/// 1. El cgroup decide el grupo base (scope de app, servicio, contenedor).
/// 2. Dentro de un scope de app o entre los procesos sin cgroup útil, un
///    subárbol cuyo ejecutable corresponde a otro `.desktop` se separa como
///    app propia: así Firefox lanzado desde una terminal no cuenta como parte
///    de la terminal.
/// 3. Lo que queda sin cgroup útil se agrupa por árbol de padres: cada árbol
///    toma el nombre del ejecutable de su raíz.
pub fn group_apps_with(procs: Vec<ProcInfo>, index: Option<&DesktopIndex>) -> Vec<App> {
    struct Node {
        proc: ProcInfo,
        /// Grupo base por cgroup; `None` = sin cgroup útil.
        base: Option<Key>,
        /// App a la que apunta el ejecutable del propio proceso.
        own: Option<Arc<DesktopInfo>>,
        splittable: bool,
    }

    let mut unit_keys: HashMap<String, Option<Key>> = HashMap::new();
    let mut exec_cache: HashMap<String, Option<Arc<DesktopInfo>>> = HashMap::new();
    let mut nodes: Vec<Node> = Vec::with_capacity(procs.len());
    for p in procs {
        if p.kthread {
            continue;
        }
        let cgroup = p.cgroup.clone().unwrap_or_default();
        let base = unit_keys
            .entry(cgroup)
            .or_insert_with_key(|path| key_for_unit(classify(path), index))
            .as_ref()
            .map(Key::clone_key);
        let splittable = base.as_ref().is_none_or(|k| k.kind == AppKind::App);
        let own = match index {
            Some(index) if splittable => exec_cache
                .entry(exe_name(&p))
                .or_insert_with_key(|name| index.by_exec(name))
                .clone(),
            _ => None,
        };
        nodes.push(Node {
            proc: p,
            base,
            own,
            splittable,
        });
    }

    let by_pid: HashMap<i32, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.proc.pid, i))
        .collect();
    let same_base = |a: &Node, b: &Node| match (&a.base, &b.base) {
        (Some(x), Some(y)) => x.id == y.id,
        (None, None) => true,
        _ => false,
    };

    // Para cada proceso: ¿pertenece a una app distinta de su grupo base?
    let mut split: Vec<Option<Arc<DesktopInfo>>> = vec![None; nodes.len()];
    // Raíz del árbol dentro de los procesos sin cgroup útil.
    let mut roots: Vec<usize> = (0..nodes.len()).collect();
    for (i, node) in nodes.iter().enumerate() {
        if !node.splittable {
            continue;
        }
        let base_desktop = node.base.as_ref().and_then(|k| k.desktop.as_ref());
        let mut cur = i;
        // El límite evita ciclos si /proc cambió a mitad de la lectura.
        for _ in 0..64 {
            if let Some(own) = &nodes[cur].own {
                if base_desktop.is_none_or(|d| d.id != own.id) {
                    split[i] = Some(own.clone());
                }
                break;
            }
            match by_pid.get(&nodes[cur].proc.ppid) {
                Some(&parent) if parent != cur && same_base(&nodes[parent], node) => cur = parent,
                _ => break,
            }
        }
        roots[i] = cur;
    }

    let root_names: Vec<String> = nodes.iter().map(|n| exe_name(&n.proc)).collect();
    let mut groups: BTreeMap<String, App> = BTreeMap::new();
    for (i, node) in nodes.into_iter().enumerate() {
        let key = match (split[i].take(), node.base) {
            (Some(desktop), base) => {
                let origin = base.map(|k| k.origin).unwrap_or(Origin::Fallback);
                Key::for_desktop(desktop, origin, None)
            }
            (None, Some(base)) => base,
            (None, None) => {
                let name = root_names[roots[i]].clone();
                Key {
                    id: format!("exe:{name}"),
                    name,
                    kind: AppKind::Service,
                    origin: Origin::Fallback,
                    app_id: None,
                    desktop: None,
                }
            }
        };
        push(&mut groups, key, node.proc);
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

impl Key {
    fn clone_key(&self) -> Key {
        Key {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: self.kind,
            origin: self.origin,
            app_id: self.app_id.clone(),
            desktop: self.desktop.clone(),
        }
    }
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
            desktop: key.desktop,
            procs: Vec::new(),
            cpu_pct: 0.0,
            rss: 0,
        })
        .procs
        .push(p);
}

#[cfg(test)]
pub(crate) mod tests {
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
    fn desktop_gives_name_and_splits_foreign_apps_out_of_a_terminal() {
        use crate::apps::desktop::tests::{index_with, SAMPLE};
        let (_tmp, index) = index_with(SAMPLE);
        let term = format!("{APP}/app-niri-alacritty-10.scope");
        let own = format!("{APP}/app-niri-firefox-77.scope");
        let procs = vec![
            proc(10, 1, "/usr/bin/alacritty", &term),
            proc(11, 10, "/usr/bin/fish", &term),
            proc(12, 11, "/usr/bin/cargo", &term),
            // Firefox lanzado desde la terminal y luego reparentado.
            proc(20, 1, "/usr/lib64/firefox/firefox", &term),
            proc(21, 20, "/usr/lib64/firefox/firefox", &term),
            proc(22, 21, "/usr/lib64/firefox/crashhelper", &term),
            // Otra instancia con scope propio: debe fusionarse.
            proc(30, 1, "/usr/lib64/firefox/firefox", &own),
            // Sin cgroup útil pero con `.desktop` por ejecutable.
            proc(
                40,
                1,
                "/usr/bin/sudo",
                "/user.slice/user-1000.slice/session-c7.scope",
            ),
            proc(
                41,
                40,
                "/usr/share/code/code",
                "/user.slice/user-1000.slice/session-c7.scope",
            ),
            // Los servicios no se parten aunque el ejecutable coincida.
            proc(
                50,
                1,
                "/usr/lib64/firefox/firefox",
                "/system.slice/kiosk.service",
            ),
        ];
        let apps = group_apps_with(procs, Some(&index));
        let find = |name: &str| {
            apps.iter()
                .find(|a| a.name == name)
                .unwrap_or_else(|| panic!("{name}"))
        };
        assert_eq!(find("Alacritty").pids(), vec![10, 11, 12]);
        assert_eq!(find("Alacritty").desktop.as_ref().unwrap().id, "Alacritty");
        let fx = find("Zorro");
        assert_eq!(fx.pids(), vec![20, 21, 22, 30]);
        assert_eq!(fx.kind, AppKind::App);
        assert_eq!(
            fx.desktop.as_ref().unwrap().icon.as_deref(),
            Some("firefox")
        );
        assert_eq!(find("Code").pids(), vec![41]);
        assert_eq!(find("Code").kind, AppKind::App);
        assert_eq!(find("sudo").pids(), vec![40]);
        assert_eq!(find("kiosk").pids(), vec![50]);
        assert_eq!(apps.len(), 5);
    }

    #[test]
    fn app_services_take_the_desktop_name() {
        use crate::apps::desktop::tests::{index_with, SAMPLE};
        let (_tmp, index) = index_with(SAMPLE);
        let apps = group_apps_with(
            vec![proc(
                1,
                0,
                "/usr/bin/kdeconnectd",
                &format!("{APP}/app-org.kde.kdeconnect.daemon@autostart.service"),
            )],
            Some(&index),
        );
        assert_eq!(apps[0].name, "KDE Connect");
        assert_eq!(apps[0].kind, AppKind::Service);
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
