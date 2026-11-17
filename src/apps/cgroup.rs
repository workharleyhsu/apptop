//! Interpretación de rutas de cgroup de systemd.
//!
//! Convención XDG para unidades de aplicaciones (GNOME y varios compositores
//! usan scopes; KDE Plasma usa servicios con instancia):
//! `app[-<lanzador>]-<AppID>-<RANDOM>.scope` y
//! `app[-<lanzador>]-<AppID>[@<RANDOM>].service`. Los guiones propios del
//! AppID van escapados como `\x2d`, así que los `-` literales son separadores.

/// Unidad de systemd a la que pertenece un proceso, según su cgroup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Unit {
    /// `app-*.scope`: aplicación gráfica.
    AppScope { app_id: String, flatpak: bool },
    /// `*.service`. `app_id` está presente en unidades `app-*.service`.
    Service {
        name: String,
        user: bool,
        app_id: Option<String>,
    },
    /// Scope de un contenedor (`docker-<hash>.scope`, `libpod-<hash>.scope`).
    Container { runtime: String, id: String },
    /// Sin unidad útil (sesiones, `init.scope`, unidades transitorias, raíz).
    None,
}

/// Deshace el escapado de systemd (`\x2d` → `-`).
pub fn unescape(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() && bytes[i + 1] == b'x' {
            let hex = s
                .get(i + 2..i + 4)
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(b) = hex {
                out.push(b);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Clasifica una ruta de cgroup.
pub fn classify(path: &str) -> Unit {
    let user = path
        .split('/')
        .any(|c| c.starts_with("user@") && c.ends_with(".service"));
    // La unidad más profunda manda; `user@N.service` solo cuenta si no hay
    // otra. Excepción: una vez dentro de una unidad de app, los scopes que
    // la propia app crea adentro (Konsole hace `main.scope` y uno
    // `tab(<pid>).scope` por pestaña) siguen siendo esa app.
    let mut unit = None;
    let mut manager = None;
    for comp in path.split('/') {
        if comp.ends_with(".scope") || comp.ends_with(".service") {
            if comp.starts_with("user@") && comp.ends_with(".service") {
                manager = Some(comp);
                unit = None;
            } else if !unit.is_some_and(|u: &str| u.starts_with("app-")) {
                unit = Some(comp);
            }
        }
    }
    let (Some(raw), is_manager) = (unit.or(manager), unit.is_none()) else {
        return Unit::None;
    };

    if let Some(stem) = raw.strip_suffix(".scope") {
        if let Some(rest) = stem.strip_prefix("app-") {
            // Quita el sufijo aleatorio y separa el lanzador opcional.
            let parts: Vec<&str> = rest.split('-').collect();
            let body = match parts.len() {
                0 | 1 => &parts[..],
                n => &parts[..n - 1],
            };
            let (launcher, id) = match body {
                [id] => (None, *id),
                [launcher, id @ ..] => (Some(*launcher), id.last().copied().unwrap_or("")),
                [] => (None, ""),
            };
            // Con más de dos segmentos, todo lo que sigue al lanzador es el id.
            let id = if body.len() > 2 {
                body[1..].join("-")
            } else {
                id.to_string()
            };
            if !id.is_empty() {
                return Unit::AppScope {
                    app_id: unescape(&id),
                    flatpak: launcher == Some("flatpak"),
                };
            }
        }
        for runtime in ["docker", "libpod", "crio"] {
            if let Some(id) = stem.strip_prefix(runtime).and_then(|r| r.strip_prefix('-')) {
                return Unit::Container {
                    runtime: runtime.to_string(),
                    id: id.to_string(),
                };
            }
        }
        return Unit::None;
    }

    let stem = raw.strip_suffix(".service").unwrap_or(raw);
    // Unidades transitorias de `systemd-run`: no dicen nada de la app.
    if stem.starts_with("run-") && !is_manager {
        return Unit::None;
    }
    if let Some(rest) = stem.strip_prefix("app-") {
        let (base, instance) = match rest.split_once('@') {
            Some((base, instance)) => (base, Some(instance)),
            None => (rest, None),
        };
        let parts: Vec<&str> = base.split('-').collect();
        let (launcher, id) = match parts.as_slice() {
            [id] => (None, id.to_string()),
            [launcher, id @ ..] => (Some(*launcher), id.join("-")),
            [] => (None, String::new()),
        };
        if !id.is_empty() {
            let id = unescape(&id);
            // KDE Plasma lanza las apps como servicios `app-<id>@<hash>.service`
            // en vez de scopes: una instancia con nombre propio es una app
            // abierta por el usuario. `@autostart` es lo que arranca solo con
            // la sesión (applets, agentes): eso lo decide después su `.desktop`.
            if instance.is_some_and(|i| !i.is_empty() && i != "autostart") {
                return Unit::AppScope {
                    app_id: id,
                    flatpak: launcher == Some("flatpak"),
                };
            }
            return Unit::Service {
                name: id.clone(),
                user,
                app_id: Some(id),
            };
        }
    }
    // Servicios activados por D-Bus: `dbus-:1.2-org.example.Name@0`.
    if let Some(rest) = stem.strip_prefix("dbus-:") {
        if let Some((_, name)) = rest.split_once('-') {
            let name = name.split('@').next().unwrap_or(name);
            return Unit::Service {
                name: unescape(name),
                user,
                app_id: None,
            };
        }
    }
    Unit::Service {
        name: unescape(stem),
        user,
        app_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const U: &str = "/user.slice/user-1000.slice/user@1000.service";

    fn app(id: &str, flatpak: bool) -> Unit {
        Unit::AppScope {
            app_id: id.into(),
            flatpak,
        }
    }

    fn svc(name: &str, user: bool, app_id: Option<&str>) -> Unit {
        Unit::Service {
            name: name.into(),
            user,
            app_id: app_id.map(Into::into),
        }
    }

    #[test]
    fn app_scopes() {
        let c = |s: &str| classify(&format!("{U}/app.slice/{s}"));
        assert_eq!(
            c("app-niri-alacritty-1618554.scope"),
            app("alacritty", false)
        );
        assert_eq!(
            c("app-flatpak-com.moonlight_stream.Moonlight-979803556.scope"),
            app("com.moonlight_stream.Moonlight", true)
        );
        assert_eq!(c("app-firefox-abc123.scope"), app("firefox", false));
        assert_eq!(
            c(r"app-gnome-org.gnome.Nautilus-4242.scope"),
            app("org.gnome.Nautilus", false)
        );
        assert_eq!(
            c(r"app-niri-google\x2dchrome-99.scope"),
            app("google-chrome", false)
        );
    }

    #[test]
    fn app_services_keep_the_app_id() {
        let c = |s: &str| classify(&format!("{U}/app.slice/{s}"));
        assert_eq!(
            c(r"app-nm\x2dapplet@autostart.service"),
            svc("nm-applet", true, Some("nm-applet"))
        );
        assert_eq!(
            c("app-org.kde.kdeconnect.daemon@autostart.service"),
            svc(
                "org.kde.kdeconnect.daemon",
                true,
                Some("org.kde.kdeconnect.daemon")
            )
        );
        // Sin instancia también queda como servicio con su AppID.
        assert_eq!(c("app-foo.service"), svc("foo", true, Some("foo")));
    }

    /// Konsole (visto en CachyOS): dentro de su scope crea `main.scope` y un
    /// `tab(<pid>).scope` por pestaña. Todo eso sigue siendo Konsole.
    #[test]
    fn scopes_nested_inside_an_app_unit_belong_to_that_app() {
        let c = |s: &str| classify(&format!("{U}/app.slice/{s}"));
        let konsole = app("org.kde.konsole", false);
        assert_eq!(c("app-org.kde.konsole-97036.scope/main.scope"), konsole);
        assert_eq!(
            c("app-org.kde.konsole-97036.scope/tab(97044).scope"),
            konsole
        );
        assert_eq!(c("app-org.kde.konsole@77aa.service/tab(5).scope"), konsole);
        // Fuera de una unidad de app sigue mandando la más profunda.
        assert_eq!(
            classify("/system.slice/systemd-udevd.service/udev"),
            svc("systemd-udevd", false, None)
        );
        assert_eq!(c("otra.service/app-x-1.scope"), app("x", false));
    }

    /// KDE Plasma: cada app abierta es `app-<id>@<hash>.service`.
    #[test]
    fn service_instances_launched_by_the_user_are_apps() {
        let c = |s: &str| classify(&format!("{U}/app.slice/{s}"));
        assert_eq!(c("app-steam@a1b2c3d4.service"), app("steam", false));
        assert_eq!(
            c("app-org.kde.konsole@5f0e2c91a7.service"),
            app("org.kde.konsole", false)
        );
        assert_eq!(
            c("app-gnome-org.gnome.Terminal@1a2b.service"),
            app("org.gnome.Terminal", false)
        );
        assert_eq!(
            c(r"app-google\x2dchrome@77.service"),
            app("google-chrome", false)
        );
        // Lo que arranca solo con la sesión no es, de por sí, una app.
        assert_eq!(
            c("app-steam@autostart.service"),
            svc("steam", true, Some("steam"))
        );
    }

    #[test]
    fn plain_services() {
        assert_eq!(
            classify(&format!("{U}/session.slice/pipewire.service")),
            svc("pipewire", true, None)
        );
        assert_eq!(
            classify("/system.slice/docker.service"),
            svc("docker", false, None)
        );
        assert_eq!(
            classify("/system.slice/systemd-udevd.service/udev"),
            svc("systemd-udevd", false, None)
        );
        assert_eq!(
            classify("/system.slice/system-cups.slice/cups.service"),
            svc("cups", false, None)
        );
        assert_eq!(
            classify(&format!(
                "{U}/app.slice/dbus-:1.2-org.freedesktop.secrets@0.service"
            )),
            svc("org.freedesktop.secrets", true, None)
        );
        assert_eq!(
            classify("/user.slice/user-1000.slice/user@1000.service"),
            svc("user@1000", true, None)
        );
    }

    #[test]
    fn containers() {
        assert_eq!(
            classify("/system.slice/docker-7ef05203aac0.scope"),
            Unit::Container {
                runtime: "docker".into(),
                id: "7ef05203aac0".into()
            }
        );
    }

    #[test]
    fn no_useful_unit() {
        for path in [
            "/",
            "/init.scope",
            "/user.slice/user-1000.slice/session-2.scope",
            "/user.slice/user-1000.slice/user@1000.service/init.scope",
            "/user.slice/user-1000.slice/user@1000.service/app.slice/run-p4578-i16777.service",
            "",
        ] {
            assert_eq!(classify(path), Unit::None, "{path}");
        }
    }

    #[test]
    fn unescapes() {
        assert_eq!(unescape(r"nm\x2dapplet"), "nm-applet");
        assert_eq!(unescape(r"a\x2"), r"a\x2");
        assert_eq!(unescape(r"dbus\x2d:1.2\x2dorg"), "dbus-:1.2-org");
    }
}
