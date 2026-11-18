//! Textos de la interfaz en los idiomas soportados.
//!
//! El idioma por defecto es inglés; el español se elige con
//! `language = "es"` en la configuración o con `--lang es`.
//!
//! El idioma se pasa siempre de forma explícita (vive en `State`), nunca en
//! una variable global: los tests corren en paralelo y se pisarían.
//!
//! Los textos fijos son campos de [`Texts`], así el compilador garantiza que
//! ningún idioma se olvide de uno. Los que llevan números o plurales son
//! métodos de [`Lang`], porque el plural y el orden de las palabras cambian
//! de un idioma a otro.

use nix::sys::signal::Signal;

/// Idioma de la interfaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Lang {
    #[default]
    En,
    Es,
}

/// Textos fijos de la interfaz.
#[derive(Debug)]
pub struct Texts {
    // Valores no disponibles y estados de medición.
    pub na: &'static str,
    pub no_permission: &'static str,
    pub measuring: &'static str,
    // Barra superior.
    pub no_swap: &'static str,
    pub services_word: &'static str,
    // Atajos de la barra inferior.
    pub hint_move: &'static str,
    pub hint_open: &'static str,
    pub hint_filter: &'static str,
    pub hint_sort: &'static str,
    pub hint_view: &'static str,
    pub hint_close: &'static str,
    pub hint_kill: &'static str,
    pub hint_pause: &'static str,
    pub hint_resume: &'static str,
    pub hint_priority: &'static str,
    pub hint_signal: &'static str,
    pub hint_refresh: &'static str,
    pub hint_clear_filter: &'static str,
    pub hint_quit: &'static str,
    pub hint_apply: &'static str,
    pub hint_confirm: &'static str,
    pub hint_cancel: &'static str,
    pub hint_change: &'static str,
    pub hint_choose: &'static str,
    pub hint_send: &'static str,
    pub hint_select_process: &'static str,
    pub hint_back: &'static str,
    pub hint_theme: &'static str,
    // Selector de temas
    pub title_theme: &'static str,
    pub theme_filter: &'static str,
    pub theme_placeholder: &'static str,
    pub theme_hint: &'static str,
    pub theme_no_match: &'static str,
    pub theme_invalid: &'static str,
    pub unknown_theme: &'static str,
    pub theme_files_failed: &'static str,
    /// Letra que confirma un diálogo (también se acepta la del otro idioma).
    pub yes_key: &'static str,
    // Orden y secciones.
    pub sort_name: &'static str,
    pub sections_all: &'static str,
    pub sections_apps: &'static str,
    pub sections_services: &'static str,
    // Grilla.
    pub applications: &'static str,
    pub services: &'static str,
    pub no_apps: &'static str,
    pub no_match: &'static str,
    pub paused: &'static str,
    // Origen de un grupo.
    pub origin_app: &'static str,
    pub origin_flatpak: &'static str,
    pub origin_user_service: &'static str,
    pub origin_system_service: &'static str,
    pub origin_container: &'static str,
    pub origin_process: &'static str,
    // Detalle.
    pub kind_app: &'static str,
    pub kind_service: &'static str,
    pub label_user: &'static str,
    pub label_uptime: &'static str,
    pub label_processes: &'static str,
    pub label_disk: &'static str,
    pub label_net: &'static str,
    pub threads: &'static str,
    /// Letras de lectura y escritura a disco.
    pub disk_read: &'static str,
    pub disk_write: &'static str,
    pub gpu_not_used: &'static str,
    pub no_tcp: &'static str,
    /// Tope del eje de un gráfico.
    pub axis_top: &'static str,
    pub app_gone: &'static str,
    // Árbol de procesos.
    pub col_process: &'static str,
    pub col_threads: &'static str,
    pub col_state: &'static str,
    pub state_running: &'static str,
    pub state_sleeping: &'static str,
    pub state_io: &'static str,
    pub state_idle: &'static str,
    pub state_stopped: &'static str,
    pub state_traced: &'static str,
    pub state_zombie: &'static str,
    pub state_dead: &'static str,
    pub state_unknown: &'static str,
    // Diálogos.
    pub title_close_app: &'static str,
    pub title_close_process: &'static str,
    pub title_kill_app: &'static str,
    pub title_kill_process: &'static str,
    pub title_signal_app: &'static str,
    pub title_signal_process: &'static str,
    pub title_send_signal: &'static str,
    pub title_priority_app: &'static str,
    pub title_priority_process: &'static str,
    pub yes_no: &'static str,
    pub menu_keys: &'static str,
    pub nice_keys: &'static str,
    pub nice_new: &'static str,
    /// Leyenda de la regla de prioridad; mide 40 columnas, como la regla.
    pub nice_ruler: &'static str,
    pub nice_only_under_load: &'static str,
    pub nice_raise_needs_root: &'static str,
    pub nice_one_way_1: &'static str,
    pub nice_one_way_2: &'static str,
    // Avisos.
    pub cancelled: &'static str,
    // Reintento con sudo.
    pub title_elevate: &'static str,
    pub elevate_retry: &'static str,
    pub elevate_threads: &'static str,
    pub elevate_banner: &'static str,
    pub elevate_cancel_hint: &'static str,
    pub sudo_not_authorized: &'static str,
    pub sudo_all_vanished: &'static str,
    pub would_affect_apptop: &'static str,
    // Terminal chica.
    pub too_small: &'static str,
    pub too_small_hint: &'static str,
    // Línea de comandos.
    pub not_a_terminal: &'static str,
    pub list_applications: &'static str,
    pub list_services: &'static str,
    pub list_name: &'static str,
    pub list_type: &'static str,
    pub list_icon_theme: &'static str,
    pub list_icon: &'static str,
    pub icon_cached: &'static str,
    pub icon_converted: &'static str,
}

pub const EN: Texts = Texts {
    na: "n/a",
    no_permission: "n/a (no permission)",
    measuring: "measuring…",
    no_swap: "no swap",
    services_word: "services",
    hint_move: "move",
    hint_open: "open",
    hint_filter: "filter",
    hint_sort: "sort",
    hint_view: "view",
    hint_close: "close",
    hint_kill: "kill",
    hint_pause: "pause",
    hint_resume: "resume",
    hint_priority: "priority",
    hint_signal: "signal",
    hint_refresh: "refresh",
    hint_clear_filter: "clear filter",
    hint_quit: "quit",
    hint_apply: "apply",
    hint_confirm: "confirm",
    hint_cancel: "cancel",
    hint_change: "change",
    hint_choose: "choose",
    hint_send: "send",
    hint_select_process: "select process",
    hint_back: "back",
    hint_theme: "theme",
    title_theme: "Theme",
    theme_filter: "Filter",
    theme_placeholder: "type to filter",
    theme_hint: "↑↓  Enter save  Esc cancel",
    theme_no_match: "No theme matches",
    theme_invalid: "Could not load",
    unknown_theme: "Unknown theme",
    theme_files_failed: "theme file(s) could not be loaded: press Shift+T to see why",
    yes_key: "y",
    sort_name: "name",
    sections_all: "all",
    sections_apps: "apps",
    sections_services: "services",
    applications: "Applications",
    services: "Services",
    no_apps: "No applications to show",
    no_match: "No app matches the filter",
    paused: "‖ paused",
    origin_app: "app",
    origin_flatpak: "flatpak",
    origin_user_service: "user-svc",
    origin_system_service: "system-svc",
    origin_container: "container",
    origin_process: "process",
    kind_app: "application",
    kind_service: "service",
    label_user: "User",
    label_uptime: "Uptime",
    label_processes: "Processes",
    label_disk: "Disk",
    label_net: "Net est.",
    threads: "threads",
    disk_read: "R",
    disk_write: "W",
    gpu_not_used: "not in use",
    no_tcp: "no TCP connections",
    axis_top: "max",
    app_gone: "The app is no longer running",
    col_process: "PROCESS",
    col_threads: "THR",
    col_state: "STATE",
    state_running: "R running",
    state_sleeping: "S asleep",
    state_io: "D in I/O",
    state_idle: "I idle",
    state_stopped: "T stopped",
    state_traced: "t traced",
    state_zombie: "Z zombie",
    state_dead: "X dead",
    state_unknown: "? n/a",
    title_close_app: "Close application",
    title_close_process: "Close process",
    title_kill_app: "Kill application",
    title_kill_process: "Kill process",
    title_signal_app: "Send signal to application",
    title_signal_process: "Send signal to process",
    title_send_signal: "Send signal",
    title_priority_app: "Application priority",
    title_priority_process: "Process priority",
    yes_no: "[y] yes     [n] no",
    menu_keys: "[↑/↓] choose   [Enter] send   [Esc] cancel",
    nice_keys: "[←/→] change   [Enter] apply   [Esc] cancel",
    nice_new: "New:",
    nice_ruler: "-20 higher priority             lower 19",
    nice_only_under_load: "Only matters when the CPU is saturated",
    nice_raise_needs_root: "Raising priority requires root or CAP_SYS_NICE",
    nice_one_way_1: "Without root you cannot go back to a lower value:",
    nice_one_way_2: "restart the app to regain its priority",
    cancelled: "Cancelled",
    title_elevate: "Permission denied",
    elevate_retry: "Retry as root with sudo? It will run:",
    elevate_threads: "(on every thread of those processes)",
    elevate_banner: "apptop needs root to run:",
    elevate_cancel_hint: "Nothing runs until sudo accepts the password. Ctrl+C cancels.",
    sudo_not_authorized: "sudo: password not accepted or cancelled; nothing was changed",
    sudo_all_vanished: "sudo: nothing to do, the processes had already exited",
    would_affect_apptop: "Not possible: it would affect apptop itself or its terminal",
    too_small: "Terminal too small",
    too_small_hint: "Enlarge the window (q to quit)",
    not_a_terminal: "output is not a terminal; use --list, --snapshot or --dump-icon",
    list_applications: "APPLICATIONS",
    list_services: "SERVICES",
    list_name: "NAME",
    list_type: "TYPE",
    list_icon_theme: "Icon theme",
    list_icon: "icon",
    icon_cached: "cached",
    icon_converted: "converted",
};

pub const ES: Texts = Texts {
    na: "n/d",
    no_permission: "n/d (sin permiso)",
    measuring: "midiendo…",
    no_swap: "sin swap",
    services_word: "servicios",
    hint_move: "mover",
    hint_open: "abrir",
    hint_filter: "filtrar",
    hint_sort: "orden",
    hint_view: "ver",
    hint_close: "cerrar",
    hint_kill: "matar",
    hint_pause: "pausar",
    hint_resume: "reanudar",
    hint_priority: "prioridad",
    hint_signal: "señal",
    hint_refresh: "refresco",
    hint_clear_filter: "quitar filtro",
    hint_quit: "salir",
    hint_apply: "aplicar",
    hint_confirm: "confirmar",
    hint_cancel: "cancelar",
    hint_change: "cambiar",
    hint_choose: "elegir",
    hint_send: "enviar",
    hint_select_process: "elegir proceso",
    hint_back: "volver",
    hint_theme: "tema",
    title_theme: "Tema",
    theme_filter: "Filtro",
    theme_placeholder: "escribe para filtrar",
    theme_hint: "↑↓  Enter guardar  Esc cancelar",
    theme_no_match: "Ningún tema coincide",
    theme_invalid: "No se pudo cargar",
    unknown_theme: "Tema desconocido",
    theme_files_failed: "archivo(s) de tema no se pudieron cargar: Shift+T muestra por qué",
    yes_key: "s",
    sort_name: "nombre",
    sections_all: "todo",
    sections_apps: "apps",
    sections_services: "servicios",
    applications: "Aplicaciones",
    services: "Servicios",
    no_apps: "No hay aplicaciones para mostrar",
    no_match: "Ninguna app coincide con el filtro",
    paused: "‖ pausada",
    origin_app: "app",
    origin_flatpak: "flatpak",
    origin_user_service: "serv-usuario",
    origin_system_service: "serv-sistema",
    origin_container: "contenedor",
    origin_process: "proceso",
    kind_app: "aplicación",
    kind_service: "servicio",
    label_user: "Usuario",
    label_uptime: "Activa",
    label_processes: "Procesos",
    label_disk: "Disco",
    label_net: "Red aprox.",
    threads: "hilos",
    disk_read: "L",
    disk_write: "E",
    gpu_not_used: "sin uso",
    no_tcp: "sin conexiones TCP",
    axis_top: "tope",
    app_gone: "La app ya no está en ejecución",
    col_process: "PROCESO",
    col_threads: "HILOS",
    col_state: "ESTADO",
    state_running: "R activo",
    state_sleeping: "S duerme",
    state_io: "D en E/S",
    state_idle: "I ocioso",
    state_stopped: "T pausado",
    state_traced: "t trazado",
    state_zombie: "Z zombi",
    state_dead: "X muerto",
    state_unknown: "? n/d",
    title_close_app: "Cerrar aplicación",
    title_close_process: "Cerrar proceso",
    title_kill_app: "Matar aplicación",
    title_kill_process: "Matar proceso",
    title_signal_app: "Enviar señal a la aplicación",
    title_signal_process: "Enviar señal al proceso",
    title_send_signal: "Enviar señal",
    title_priority_app: "Prioridad de la aplicación",
    title_priority_process: "Prioridad del proceso",
    yes_no: "[s] sí     [n] no",
    menu_keys: "[↑/↓] elegir   [Enter] enviar   [Esc] cancelar",
    nice_keys: "[←/→] cambiar   [Enter] aplicar   [Esc] cancelar",
    nice_new: "Nueva:",
    nice_ruler: "-20 más prioridad               menos 19",
    nice_only_under_load: "Solo se nota cuando la CPU está saturada",
    nice_raise_needs_root: "Subir la prioridad requiere root o CAP_SYS_NICE",
    nice_one_way_1: "Sin root no se puede volver a un valor menor:",
    nice_one_way_2: "para recuperar la prioridad hay que reiniciar la app",
    cancelled: "Cancelado",
    title_elevate: "Sin permiso",
    elevate_retry: "¿Reintentar como root con sudo? Se ejecutará:",
    elevate_threads: "(en todos los hilos de esos procesos)",
    elevate_banner: "apptop necesita root para ejecutar:",
    elevate_cancel_hint: "No se ejecuta nada hasta que sudo acepte la contraseña. Ctrl+C cancela.",
    sudo_not_authorized: "sudo: contraseña no aceptada o cancelada; no se cambió nada",
    sudo_all_vanished: "sudo: nada que hacer, los procesos ya habían terminado",
    would_affect_apptop: "No se puede: afectaría al propio apptop o a su terminal",
    too_small: "Terminal demasiado chica",
    too_small_hint: "Agrandá la ventana (q para salir)",
    not_a_terminal: "la salida no es una terminal; usá --list, --snapshot o --dump-icon",
    list_applications: "APLICACIONES",
    list_services: "SERVICIOS",
    list_name: "NOMBRE",
    list_type: "TIPO",
    list_icon_theme: "Tema de iconos",
    list_icon: "icono",
    icon_cached: "caché",
    icon_converted: "convertido",
};

impl Lang {
    /// Todos los idiomas soportados.
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Es];

    /// Textos fijos de este idioma.
    pub fn t(self) -> &'static Texts {
        match self {
            Lang::En => &EN,
            Lang::Es => &ES,
        }
    }

    /// Interpreta el valor de configuración o de `--lang`.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "en" | "eng" | "english" | "inglés" | "ingles" => Some(Lang::En),
            "es" | "spa" | "spanish" | "español" | "espanol" => Some(Lang::Es),
            _ => None,
        }
    }

    /// Código del idioma, como se escribe en la configuración.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    /// "1 process" / "3 processes".
    pub fn processes(self, count: usize) -> String {
        match (self, count) {
            (Lang::En, 1) => "1 process".to_string(),
            (Lang::En, n) => format!("{n} processes"),
            (Lang::Es, 1) => "1 proceso".to_string(),
            (Lang::Es, n) => format!("{n} procesos"),
        }
    }

    /// Primera línea del diálogo que ofrece reintentar con sudo.
    pub fn elevate_question(self, count: usize, label: &str) -> String {
        match self {
            Lang::En => format!("No permission for {} of {label}.", self.processes(count)),
            Lang::Es => format!("Sin permiso para {} de {label}.", self.processes(count)),
        }
    }

    /// "3 processes affected".
    pub fn affected(self, count: usize) -> String {
        match (self, count) {
            (Lang::En, _) => format!("{} affected", self.processes(count)),
            (Lang::Es, 1) => "1 proceso afectado".to_string(),
            (Lang::Es, n) => format!("{n} procesos afectados"),
        }
    }

    /// Resumen de la barra superior: intervalo, apps y servicios.
    pub fn top_summary(self, interval: &str, apps: usize, services: usize) -> String {
        format!(
            "⟳ {interval} · {apps} apps · {services} {}",
            self.t().services_word
        )
    }

    /// "12 total", cuando ni un pid entra en el ancho disponible.
    pub fn pids_total(self, count: usize) -> String {
        match self {
            Lang::En => format!("{count} total"),
            Lang::Es => format!("{count} en total"),
        }
    }

    /// "3 · 172 threads".
    pub fn processes_and_threads(self, processes: usize, threads: i64) -> String {
        format!("{processes} · {threads} {}", self.t().threads)
    }

    /// "80x20 — minimum 80x24".
    pub fn size_notice(self, size: (u16, u16), minimum: (u16, u16)) -> String {
        let word = match self {
            Lang::En => "minimum",
            Lang::Es => "mínimo",
        };
        format!("{}x{} — {word} {}x{}", size.0, size.1, minimum.0, minimum.1)
    }

    /// Pregunta del diálogo de confirmación.
    pub fn send_question(self, signal: Signal, label: &str) -> String {
        match self {
            Lang::En => format!("Send {} to {label}?", signal.as_str()),
            Lang::Es => format!("¿Enviar {} a {label}?", signal.as_str()),
        }
    }

    /// Nota sobre los procesos que se dejan afuera para no afectar a apptop.
    pub fn excluded(self, count: usize) -> String {
        match (self, count) {
            (Lang::En, 1) => "(excluding 1 process apptop depends on)".to_string(),
            (Lang::En, n) => format!("(excluding {n} processes apptop depends on)"),
            (Lang::Es, 1) => "(se excluye 1 proceso del que depende apptop)".to_string(),
            (Lang::Es, n) => format!("(se excluyen {n} procesos de los que depende apptop)"),
        }
    }

    /// Prioridad actual del alcance: un valor o un rango.
    pub fn current_priority(self, low: i64, high: i64) -> String {
        match (self, low == high) {
            (Lang::En, true) => format!("Current priority: {low}"),
            (Lang::En, false) => format!("Current priority: between {low} and {high}"),
            (Lang::Es, true) => format!("Prioridad actual: {low}"),
            (Lang::Es, false) => format!("Prioridad actual: entre {low} y {high}"),
        }
    }

    /// Descripción corta de una señal del menú.
    pub fn signal_description(self, signal: Signal) -> &'static str {
        match (self, signal) {
            (Lang::En, Signal::SIGTERM) => "ask it to terminate",
            (Lang::En, Signal::SIGKILL) => "terminate by force",
            (Lang::En, Signal::SIGINT) => "interrupt (like Ctrl+C)",
            (Lang::En, Signal::SIGHUP) => "hang up / reload configuration",
            (Lang::En, Signal::SIGQUIT) => "quit with a core dump",
            (Lang::En, Signal::SIGSTOP) => "pause",
            (Lang::En, Signal::SIGCONT) => "resume",
            (Lang::En, Signal::SIGUSR1) => "defined by the program (1)",
            (Lang::En, Signal::SIGUSR2) => "defined by the program (2)",
            (Lang::Es, Signal::SIGTERM) => "pedir que termine",
            (Lang::Es, Signal::SIGKILL) => "terminar por la fuerza",
            (Lang::Es, Signal::SIGINT) => "interrumpir (como Ctrl+C)",
            (Lang::Es, Signal::SIGHUP) => "colgar / recargar configuración",
            (Lang::Es, Signal::SIGQUIT) => "salir con volcado de memoria",
            (Lang::Es, Signal::SIGSTOP) => "pausar",
            (Lang::Es, Signal::SIGCONT) => "reanudar",
            (Lang::Es, Signal::SIGUSR1) => "definida por el programa (1)",
            (Lang::Es, Signal::SIGUSR2) => "definida por el programa (2)",
            _ => "",
        }
    }

    /// Resultado de enviar una señal: a cuántos llegó y qué falló.
    pub fn signal_summary(
        self,
        signal: Signal,
        label: &str,
        delivered: usize,
        denied: usize,
        vanished: usize,
        failed: usize,
    ) -> String {
        let name = signal.as_str();
        let mut text = match (self, delivered) {
            (Lang::En, 0) => format!("{name} was not sent to any process of {label}"),
            (Lang::En, n) => format!("{name} sent to {} of {label}", self.processes(n)),
            (Lang::Es, 0) => format!("{name} no se envió a ningún proceso de {label}"),
            (Lang::Es, n) => format!("{name} enviada a {} de {label}", self.processes(n)),
        };
        if denied > 0 {
            text.push_str(&match self {
                Lang::En => format!(" · no permission for {denied}"),
                Lang::Es => format!(" · sin permiso para {denied}"),
            });
        }
        if vanished > 0 {
            text.push_str(&match self {
                Lang::En => format!(" · {vanished} had already exited"),
                Lang::Es => format!(" · {vanished} ya habían terminado"),
            });
        }
        if failed > 0 {
            text.push_str(&match self {
                Lang::En => format!(" · {failed} failed"),
                Lang::Es => format!(" · {failed} con error"),
            });
        }
        text
    }

    /// Partes del resultado de cambiar la prioridad, en orden.
    #[allow(clippy::too_many_arguments)]
    pub fn nice_summary(
        self,
        value: i32,
        label: &str,
        changed: usize,
        needs_privilege: usize,
        not_owner: usize,
        vanished: usize,
        failed: usize,
    ) -> String {
        let mut parts: Vec<String> = Vec::new();
        if changed > 0 {
            parts.push(match self {
                Lang::En => format!(
                    "Priority {value} applied to {} of {label}",
                    self.processes(changed)
                ),
                Lang::Es => format!(
                    "Prioridad {value} aplicada a {} de {label}",
                    self.processes(changed)
                ),
            });
        }
        if needs_privilege > 0 {
            parts.push(match self {
                Lang::En => format!(
                    "no permission for {needs_privilege}: raising priority (lowering nice) requires root or CAP_SYS_NICE"
                ),
                Lang::Es => format!(
                    "sin permiso en {needs_privilege}: subir la prioridad (bajar nice) requiere root o CAP_SYS_NICE"
                ),
            });
        }
        if not_owner > 0 {
            parts.push(match self {
                Lang::En => format!("no permission for {not_owner}: owned by another user"),
                Lang::Es => format!("sin permiso en {not_owner}: son de otro usuario"),
            });
        }
        if vanished > 0 {
            parts.push(match self {
                Lang::En => format!("{vanished} had already exited"),
                Lang::Es => format!("{vanished} ya habían terminado"),
            });
        }
        if failed > 0 {
            parts.push(match self {
                Lang::En => format!("{failed} failed"),
                Lang::Es => format!("{failed} con error"),
            });
        }
        if parts.is_empty() {
            return match self {
                Lang::En => format!("No processes of {label} to change the priority of"),
                Lang::Es => format!("No hay procesos de {label} a los que cambiar la prioridad"),
            };
        }
        let mut text = parts.join(" · ");
        if changed == 0 {
            // Empieza en mayúscula cuando el primer tramo es un error.
            let mut chars = text.chars();
            if let Some(first) = chars.next() {
                text = first.to_uppercase().chain(chars).collect();
            }
        }
        text
    }

    /// Errores de la línea de comandos.
    pub fn no_icon_for(self, target: &str) -> String {
        match self {
            Lang::En => format!("no icon found for '{target}'"),
            Lang::Es => format!("no se encontró un icono para '{target}'"),
        }
    }

    pub fn cannot_convert(self, path: &str) -> String {
        match self {
            Lang::En => format!("could not convert {path}"),
            Lang::Es => format!("no se pudo convertir {path}"),
        }
    }

    pub fn no_app_named(self, name: &str) -> String {
        match self {
            Lang::En => format!("no running app is called '{name}'"),
            Lang::Es => format!("no hay ninguna app en ejecución llamada '{name}'"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_default_and_codes_round_trip() {
        assert_eq!(Lang::default(), Lang::En);
        for lang in Lang::ALL {
            assert_eq!(Lang::parse(lang.code()), Some(lang));
        }
        assert_eq!(Lang::parse(" Español "), Some(Lang::Es));
        assert_eq!(Lang::parse("ENGLISH"), Some(Lang::En));
        assert_eq!(Lang::parse("fr"), None);
    }

    /// Recorre los campos de `Texts` a través de su salida `Debug`: ningún
    /// texto puede quedar vacío en ningún idioma.
    #[test]
    fn no_text_is_empty_in_any_language() {
        for lang in Lang::ALL {
            let dump = format!("{:?}", lang.t());
            assert!(
                !dump.contains(": \"\""),
                "{lang:?} tiene un texto vacío: {dump}"
            );
        }
    }

    #[test]
    fn the_two_languages_actually_differ() {
        assert_ne!(EN.applications, ES.applications);
        assert_ne!(EN.yes_key, ES.yes_key);
        assert_eq!(EN.nice_ruler.chars().count(), 40);
        assert_eq!(ES.nice_ruler.chars().count(), 40);
    }

    #[test]
    fn detail_labels_fit_the_label_column() {
        for lang in Lang::ALL {
            let t = lang.t();
            for label in [
                t.label_user,
                t.label_uptime,
                t.label_processes,
                t.label_disk,
                t.label_net,
            ] {
                // La columna mide 11: tiene que sobrar al menos un espacio.
                assert!(label.chars().count() <= 10, "{label}");
            }
            // Los estados del árbol entran en su columna de 9.
            for state in [
                t.state_running,
                t.state_sleeping,
                t.state_io,
                t.state_idle,
                t.state_stopped,
                t.state_traced,
                t.state_zombie,
                t.state_dead,
                t.state_unknown,
            ] {
                assert!(state.chars().count() <= 9, "{state}");
            }
            assert!(t.col_threads.chars().count() <= 5, "{}", t.col_threads);
        }
    }

    #[test]
    fn plurals() {
        assert_eq!(Lang::En.affected(1), "1 process affected");
        assert_eq!(Lang::En.affected(22), "22 processes affected");
        assert_eq!(Lang::Es.affected(1), "1 proceso afectado");
        assert_eq!(Lang::Es.affected(22), "22 procesos afectados");
        assert_eq!(
            Lang::En.excluded(2),
            "(excluding 2 processes apptop depends on)"
        );
        assert_eq!(
            Lang::Es.current_priority(-3, 5),
            "Prioridad actual: entre -3 y 5"
        );
        assert_eq!(Lang::En.current_priority(0, 0), "Current priority: 0");
    }

    #[test]
    fn every_menu_signal_has_a_description() {
        for lang in Lang::ALL {
            for signal in crate::actions::SIGNAL_MENU {
                assert!(
                    !lang.signal_description(*signal).is_empty(),
                    "{lang:?} {signal:?}"
                );
            }
        }
    }

    #[test]
    fn signal_summaries() {
        let en = Lang::En.signal_summary(Signal::SIGTERM, "Firefox", 12, 0, 0, 0);
        assert_eq!(en, "SIGTERM sent to 12 processes of Firefox");
        let es = Lang::Es.signal_summary(Signal::SIGKILL, "x", 1, 3, 2, 0);
        assert_eq!(
            es,
            "SIGKILL enviada a 1 proceso de x · sin permiso para 3 · 2 ya habían terminado"
        );
        let none = Lang::En.signal_summary(Signal::SIGTERM, "docker", 0, 4, 0, 1);
        assert_eq!(
            none,
            "SIGTERM was not sent to any process of docker · no permission for 4 · 1 failed"
        );
    }

    #[test]
    fn nice_summaries() {
        assert_eq!(
            Lang::En.nice_summary(10, "Firefox", 3, 0, 0, 0, 0),
            "Priority 10 applied to 3 processes of Firefox"
        );
        assert_eq!(
            Lang::En.nice_summary(-5, "Firefox", 0, 2, 0, 0, 0),
            "No permission for 2: raising priority (lowering nice) requires root or CAP_SYS_NICE"
        );
        assert_eq!(
            Lang::Es.nice_summary(5, "docker", 1, 0, 4, 1, 0),
            "Prioridad 5 aplicada a 1 proceso de docker · sin permiso en 4: son de otro usuario · 1 ya habían terminado"
        );
        assert_eq!(
            Lang::Es.nice_summary(0, "x", 0, 0, 0, 0, 0),
            "No hay procesos de x a los que cambiar la prioridad"
        );
    }
}
