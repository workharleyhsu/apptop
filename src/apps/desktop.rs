//! Índice de archivos `.desktop` para dar nombre visible e icono a las apps.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use freedesktop_desktop_entry::{DesktopEntry, Iter};

/// Datos de un `.desktop` que le interesan a apptop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopInfo {
    /// Identificador del archivo (`org.mozilla.firefox`).
    pub id: String,
    /// Nombre visible, localizado.
    pub name: String,
    /// Valor de `Icon=`: nombre en el tema o ruta absoluta.
    pub icon: Option<String>,
    /// Ruta del archivo `.desktop`.
    pub path: PathBuf,
    /// Aparece en el menú de aplicaciones (no es `NoDisplay`): es algo que
    /// el usuario reconoce como una app y no un agente de fondo.
    pub visible: bool,
}

/// Ejecutables genéricos que no identifican a ninguna aplicación.
const GENERIC_EXECS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "env",
    "python",
    "python3",
    "perl",
    "ruby",
    "node",
    "java",
    "flatpak",
    "gtk-launch",
    "xdg-open",
    "wine",
    "sudo",
    "pkexec",
    "dbus-launch",
    "systemd-run",
    "gapplication",
    "kioclient",
    "electron",
];

/// Cómo se asoció un ejecutable con un `.desktop`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecMatch {
    /// El programa de la línea `Exec=`.
    ExecLine,
    /// El nombre del ejecutable es el id del `.desktop`.
    DesktopId,
}

#[derive(Default)]
pub struct DesktopIndex {
    entries: Vec<Arc<DesktopInfo>>,
    by_id: HashMap<String, usize>,
    by_tail: HashMap<String, usize>,
    by_wm_class: HashMap<String, usize>,
    /// Nombre del ejecutable → (entrada, puntaje de la coincidencia).
    by_exec: HashMap<String, (usize, u8)>,
    /// Id del `.desktop` (y su último segmento), en minúsculas → entrada.
    /// Solo apps gráficas visibles. Respaldo de `by_exec` para cuando el
    /// proceso no se llama como el lanzador de `Exec=`.
    by_program_id: HashMap<String, usize>,
}

/// Directorios `applications` según XDG, en orden de prioridad.
pub fn application_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut bases: Vec<PathBuf> = Vec::new();
    match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => bases.push(PathBuf::from(dir)),
        None => bases.extend(home.as_ref().map(|h| h.join(".local/share"))),
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    bases.extend(
        data_dirs
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from),
    );
    bases.extend(
        home.as_ref()
            .map(|h| h.join(".local/share/flatpak/exports/share")),
    );
    bases.push(PathBuf::from("/var/lib/flatpak/exports/share"));

    let mut dirs: Vec<PathBuf> = Vec::new();
    for base in bases {
        let dir = base.join("applications");
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// Idiomas preferidos tomados del entorno (`es_AR`, `es`, ...).
fn locales_from_env() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            let locale = value.split(['.', '@']).next().unwrap_or("").to_string();
            if !locale.is_empty() && locale != "C" && locale != "POSIX" && !out.contains(&locale) {
                out.push(locale);
            }
        }
    }
    out
}

/// `firefox (1)` → `firefox`. `None` si no termina en ` (<número>)`.
fn strip_copy_suffix(id: &str) -> Option<&str> {
    let (base, number) = id.strip_suffix(')')?.rsplit_once(" (")?;
    (!base.is_empty() && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
        .then_some(base)
}

/// Nombre del ejecutable al que apunta una línea `Exec=`, si identifica una app.
fn exec_program(exec: &str) -> Option<(String, bool)> {
    let mut tokens = exec.split_whitespace().peekable();
    // Salta `env VAR=valor ...`.
    if tokens
        .peek()
        .is_some_and(|t| t.rsplit('/').next() == Some("env"))
    {
        tokens.next();
        while tokens.peek().is_some_and(|t| t.contains('=')) {
            tokens.next();
        }
    }
    let first = tokens.next()?.trim_matches('"');
    let name = first.rsplit('/').next()?.to_string();
    if name.is_empty() || GENERIC_EXECS.contains(&name.as_str()) {
        return None;
    }
    // Sin argumentos propios (solo códigos `%u`, `%F`...) es el lanzador principal.
    let plain = tokens.all(|t| t.starts_with('%'));
    Some((name, plain))
}

impl DesktopIndex {
    /// Carga todos los `.desktop` del sistema.
    pub fn load() -> Self {
        Self::from_dirs(application_dirs(), &locales_from_env())
    }

    pub fn from_dirs(dirs: Vec<PathBuf>, locales: &[String]) -> Self {
        let mut index = Self::default();
        for entry in Iter::new(dirs.into_iter()).entries(Some(locales)) {
            index.add(&entry, locales);
        }
        index
    }

    fn add(&mut self, entry: &DesktopEntry, locales: &[String]) {
        if entry.type_().is_some_and(|t| t != "Application") || entry.hidden() {
            return;
        }
        let id = entry.id().to_string();
        let key = id.to_lowercase();
        // Los directorios van en orden de prioridad: gana el primero.
        if self.by_id.contains_key(&key) {
            return;
        }
        let name = entry
            .name(locales)
            .map(|n| n.into_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| id.clone());
        let idx = self.entries.len();
        self.entries.push(Arc::new(DesktopInfo {
            id: id.clone(),
            name,
            icon: entry.icon().filter(|i| !i.is_empty()).map(str::to_string),
            path: entry.path.clone(),
            visible: !entry.no_display(),
        }));
        self.by_id.insert(key.clone(), idx);

        let listed = !entry.no_display();
        let tail = key.rsplit('.').next().unwrap_or(&key).to_string();
        if listed || !self.by_tail.contains_key(&tail) {
            self.by_tail.entry(tail.clone()).or_insert(idx);
        }
        if let Some(class) = entry.startup_wm_class() {
            self.by_wm_class.entry(class.to_lowercase()).or_insert(idx);
        }
        // Solo apps gráficas visibles identifican procesos por ejecutable:
        // `fish.desktop` o `htop.desktop` no deben partir una terminal.
        if listed && !entry.terminal() {
            self.by_program_id.entry(key.clone()).or_insert(idx);
            self.by_program_id.entry(tail.clone()).or_insert(idx);
            if let Some((program, plain)) = entry.exec().and_then(exec_program) {
                let program_key = program.to_lowercase();
                let score =
                    u8::from(plain) + 2 * u8::from(tail == program_key || key == program_key);
                let better = self
                    .by_exec
                    .get(&program)
                    .is_none_or(|(_, old)| score > *old);
                if better {
                    self.by_exec.insert(program, (idx, score));
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn get(&self, idx: usize) -> Option<Arc<DesktopInfo>> {
        self.entries.get(idx).cloned()
    }

    /// Busca por AppID: id exacto, último segmento del id, o `StartupWMClass`.
    pub fn by_app_id(&self, app_id: &str) -> Option<Arc<DesktopInfo>> {
        self.lookup_app_id(app_id).or_else(|| {
            // KDE nombra `firefox (1)` al lanzador duplicado de una app: el
            // AppID llega con ese sufijo y no existe ningún `.desktop` así.
            let base = strip_copy_suffix(app_id)?;
            self.lookup_app_id(base)
        })
    }

    fn lookup_app_id(&self, app_id: &str) -> Option<Arc<DesktopInfo>> {
        let key = app_id.to_lowercase();
        self.by_id
            .get(&key)
            .or_else(|| self.by_tail.get(&key))
            .or_else(|| self.by_wm_class.get(&key))
            .or_else(|| key.rsplit('.').next().and_then(|t| self.by_tail.get(t)))
            .and_then(|i| self.get(*i))
    }

    /// Busca solo por id exacto de archivo (para servicios comunes).
    pub fn by_exact_id(&self, id: &str) -> Option<Arc<DesktopInfo>> {
        self.by_id
            .get(&id.to_lowercase())
            .and_then(|i| self.get(*i))
    }

    /// Busca la app a la que pertenece un ejecutable.
    ///
    /// Primero por el programa de la línea `Exec=`. Si no coincide, por el
    /// id del `.desktop`: en Arch `steam.desktop` lanza `steam-runtime`, pero
    /// el proceso que queda corriendo se llama `steam`.
    pub fn by_exec(&self, program: &str) -> Option<Arc<DesktopInfo>> {
        self.match_exec(program).map(|(info, _)| info)
    }

    /// Como [`DesktopIndex::by_exec`], indicando además cómo coincidió.
    pub fn match_exec(&self, program: &str) -> Option<(Arc<DesktopInfo>, ExecMatch)> {
        if let Some((i, _)) = self.by_exec.get(program) {
            return self.get(*i).map(|info| (info, ExecMatch::ExecLine));
        }
        if GENERIC_EXECS.contains(&program) {
            return None;
        }
        let i = self.by_program_id.get(&program.to_lowercase())?;
        self.get(*i).map(|info| (info, ExecMatch::DesktopId))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;

    /// Crea un directorio `applications` con los `.desktop` dados.
    pub(crate) fn index_with(files: &[(&str, &str)]) -> (tempfile::TempDir, DesktopIndex) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("applications");
        fs::create_dir_all(&dir).unwrap();
        for (name, body) in files {
            fs::write(
                dir.join(name),
                format!("[Desktop Entry]\nType=Application\n{body}\n"),
            )
            .unwrap();
        }
        let index = DesktopIndex::from_dirs(vec![dir], &["es_AR".to_string()]);
        (tmp, index)
    }

    pub(crate) const SAMPLE: &[(&str, &str)] = &[
        ("org.mozilla.firefox.desktop", "Name=Firefox\nName[es]=Zorro\nExec=firefox %u\nIcon=firefox\nStartupWMClass=firefox"),
        ("Alacritty.desktop", "Name=Alacritty\nExec=alacritty\nIcon=Alacritty"),
        ("fish.desktop", "Name=fish\nExec=fish\nTerminal=true\nNoDisplay=true\nIcon=fish"),
        ("org.kde.kdeconnect.daemon.desktop", "Name=KDE Connect\nExec=kdeconnectd\nNoDisplay=true\nIcon=kdeconnect"),
        ("kcm_a.desktop", "Name=Audio\nExec=systemsettings kcm_a\nIcon=audio"),
        ("systemsettings.desktop", "Name=System Settings\nExec=systemsettings\nIcon=systemsettings"),
        ("script.desktop", "Name=Script\nExec=python3 /opt/x.py\nIcon=x"),
        ("code.desktop", "Name=Code\nExec=env FOO=1 /usr/share/code/code --unity %F\nIcon=/opt/code.png\nStartupWMClass=Code"),
        ("gone.desktop", "Name=Gone\nExec=gone\nHidden=true"),
    ];

    #[test]
    fn looks_up_by_id_tail_and_wm_class() {
        let (_tmp, index) = index_with(SAMPLE);
        assert_eq!(index.len(), 8);
        let fx = index.by_app_id("firefox").unwrap();
        assert_eq!(
            (fx.id.as_str(), fx.name.as_str()),
            ("org.mozilla.firefox", "Zorro")
        );
        assert_eq!(fx.icon.as_deref(), Some("firefox"));
        assert_eq!(
            index.by_app_id("org.mozilla.firefox").unwrap().id,
            "org.mozilla.firefox"
        );
        assert_eq!(index.by_app_id("alacritty").unwrap().id, "Alacritty");
        assert_eq!(
            index.by_app_id("Code").unwrap().icon.as_deref(),
            Some("/opt/code.png")
        );
        assert_eq!(
            index.by_app_id("org.kde.kdeconnect.daemon").unwrap().name,
            "KDE Connect"
        );
        assert!(index.by_app_id("nope").is_none());
        assert!(index.by_app_id("gone").is_none());
    }

    #[test]
    fn exec_lookup_skips_terminal_hidden_and_generic_programs() {
        let (_tmp, index) = index_with(SAMPLE);
        assert_eq!(index.by_exec("firefox").unwrap().id, "org.mozilla.firefox");
        assert_eq!(index.by_exec("code").unwrap().id, "code");
        assert!(index.by_exec("fish").is_none());
        assert!(index.by_exec("kdeconnectd").is_none());
        assert!(index.by_exec("python3").is_none());
        // Entre varias entradas con el mismo ejecutable gana la principal.
        assert_eq!(
            index.by_exec("systemsettings").unwrap().id,
            "systemsettings"
        );
    }

    /// Caso real de Arch/CachyOS: el `.desktop` lanza un script con otro
    /// nombre que el proceso que queda corriendo.
    #[test]
    fn falls_back_to_the_desktop_id_when_exec_names_a_wrapper() {
        let (_tmp, index) = index_with(&[
            (
                "steam.desktop",
                "Name=Steam\nExec=/usr/bin/steam-runtime %U\nIcon=steam",
            ),
            (
                "org.gnome.Nautilus.desktop",
                "Name=Files\nExec=nautilus-wrapper --new-window\nIcon=nautilus",
            ),
            (
                "htop.desktop",
                "Name=Htop\nExec=htop-launch\nTerminal=true\nIcon=htop",
            ),
            (
                "agent.desktop",
                "Name=Agent\nExec=agent-launch\nNoDisplay=true",
            ),
            (
                "python3.desktop",
                "Name=Python\nExec=python-launch\nIcon=python",
            ),
        ]);
        let (steam, how) = index.match_exec("steam").unwrap();
        assert_eq!((steam.id.as_str(), how), ("steam", ExecMatch::DesktopId));
        assert_eq!(
            index.match_exec("steam-runtime").unwrap().1,
            ExecMatch::ExecLine
        );
        // Último segmento del id, sin distinguir mayúsculas.
        assert_eq!(index.by_exec("nautilus").unwrap().id, "org.gnome.Nautilus");
        // Ni apps de terminal, ni ocultas, ni intérpretes genéricos.
        assert!(index.by_exec("htop").is_none());
        assert!(index.by_exec("agent").is_none());
        assert!(index.by_exec("python3").is_none());
        assert!(steam.visible);
        assert!(!index.by_app_id("agent").unwrap().visible);
    }

    /// Visto en CachyOS/KDE: `app-firefox\x20\x281\x29@<hash>.service`.
    #[test]
    fn app_ids_of_duplicated_launchers_resolve_to_the_original() {
        let (_tmp, index) = index_with(SAMPLE);
        assert_eq!(
            index.by_app_id("firefox (1)").unwrap().id,
            "org.mozilla.firefox"
        );
        assert_eq!(index.by_app_id("Alacritty (12)").unwrap().id, "Alacritty");
        assert!(index.by_app_id("nope (1)").is_none());
        assert_eq!(strip_copy_suffix("a (b)"), None);
        assert_eq!(strip_copy_suffix("a ()"), None);
        assert_eq!(strip_copy_suffix(" (1)"), None);
        assert_eq!(strip_copy_suffix("firefox"), None);
    }

    #[test]
    fn parses_exec_lines() {
        assert_eq!(exec_program("firefox %u"), Some(("firefox".into(), true)));
        assert_eq!(
            exec_program("/usr/bin/steam -silent"),
            Some(("steam".into(), false))
        );
        assert_eq!(
            exec_program("env A=1 B=2 /opt/app/run"),
            Some(("run".into(), true))
        );
        assert_eq!(exec_program("sh -c 'foo'"), None);
        assert_eq!(exec_program(""), None);
    }

    #[test]
    fn loads_the_system_index_without_panicking() {
        let _ = DesktopIndex::load().len();
    }
}
