//! Bucle interactivo: terminal real, eventos y refresco periódico.

use std::io::{self, stdout};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
use ratatui::crossterm::cursor::{Hide, Show};
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{enable_raw_mode, EnterAlternateScreen};
use ratatui::DefaultTerminal;
use tui_themes::{Catalog, ColorDepth};

use crate::actions::{ancestry, elevate, send_signal, set_nice, ElevateOutcome, Elevated};
use crate::apps::Sampler;
use crate::config::{ColorMode, Config};
use crate::i18n::Lang;
use crate::icons::cache::default_cache_dir;
use crate::icons::{IconLocator, IconStore};

use super::input::{handle_event, Outcome};
use super::state::{clamp_refresh, SortKey, State};
use super::theme::{Theme, ThemeSet, DEFAULT_THEME};

/// Opciones de la sesión interactiva (configuración + línea de comandos).
#[derive(Debug, Clone, PartialEq)]
pub struct RunOptions {
    /// Intervalo entre muestras.
    pub refresh: Duration,
    pub sort: SortKey,
    /// Tamaño de los iconos de la grilla, en celdas.
    pub icon_size: (u16, u16),
    pub theme: Theme,
    /// Catálogo y profundidad de color, para el selector de temas.
    pub themes: ThemeSet,
    /// Tema de iconos; `None` = detectar.
    pub icon_theme: Option<String>,
    /// Idioma de la interfaz.
    pub lang: Lang,
    /// Provoca un panic tras el primer cuadro (para probar la restauración).
    pub panic_test: bool,
    /// Archivo donde se guarda el tema elegido con `T`.
    pub config_path: Option<PathBuf>,
    /// Aviso sobre los temas para mostrar al arrancar (tema desconocido o
    /// archivos de tema que no se pudieron leer).
    pub theme_notice: Option<String>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self::from_config(&Config::default())
    }
}

impl RunOptions {
    /// Opciones a partir de la configuración, con los temas embebidos y
    /// los del usuario (`~/.config/tui-themes/themes`).
    pub fn from_config(config: &Config) -> Self {
        Self::from_config_with(config, Catalog::load())
    }

    /// Igual que [`RunOptions::from_config`], con el catálogo dado.
    pub fn from_config_with(config: &Config, catalog: Catalog) -> Self {
        // Forzar una profundidad en la configuración ignora el entorno
        // (también `NO_COLOR`); en `auto` decide `tui-themes`.
        let depth = match config.color {
            ColorMode::Auto => ColorDepth::detect(),
            ColorMode::Truecolor => ColorDepth::TrueColor,
            ColorMode::Palette256 => ColorDepth::Ansi256,
            ColorMode::Palette16 => ColorDepth::Ansi16,
        };
        let themes = ThemeSet {
            catalog: Arc::new(catalog),
            depth,
            accent: config.accent,
            icon_mode: config.icon_mode,
        };
        let t = config.language.t();
        let wanted = config.theme.as_deref().unwrap_or(DEFAULT_THEME);
        let mut theme_notice = match themes.catalog.errors().len() {
            0 => None,
            n => Some(format!("{n} {}", t.theme_files_failed)),
        };
        let theme = themes.theme(wanted).unwrap_or_else(|| {
            theme_notice = Some(format!("{} \"{wanted}\"", t.unknown_theme));
            themes
                .theme(DEFAULT_THEME)
                .unwrap_or_else(|| Theme::builtin(depth))
        });
        Self {
            refresh: config.refresh,
            sort: config.sort,
            icon_size: config.icon_size.cells(),
            theme,
            themes,
            icon_theme: config.icon_theme.clone(),
            lang: config.language,
            panic_test: false,
            config_path: None,
            theme_notice,
        }
    }

    /// Estado inicial de la interfaz con estas opciones.
    pub fn new_state(&self) -> State {
        let mut state = State::new(self.theme.clone());
        state.refresh = clamp_refresh(self.refresh);
        state.sort = self.sort;
        state.icon_size = self.icon_size;
        state.lang = self.lang;
        state.themes = self.themes.clone();
        if let Some(notice) = &self.theme_notice {
            state.notify(notice.clone(), true);
        }
        state
    }

    /// Almacén de iconos con el tema elegido.
    pub fn new_store(&self) -> IconStore {
        match &self.icon_theme {
            Some(theme) => {
                IconStore::with(IconLocator::with_theme(theme.clone()), default_cache_dir())
            }
            None => IconStore::new(),
        }
    }
}

/// Se pone en `true` cuando llega SIGTERM, SIGHUP o SIGINT: el bucle sale
/// ordenadamente y la terminal queda restaurada.
static TERMINATE: AtomicBool = AtomicBool::new(false);

/// `true` mientras la interfaz está suspendida para que sudo pida la
/// contraseña: un Ctrl+C ahí cancela sudo, no cierra apptop.
static SUSPENDED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_terminate(signal: libc::c_int) {
    // Solo lecturas y escrituras atómicas: es lo único seguro dentro de un
    // manejador.
    if signal == libc::SIGINT && SUSPENDED.load(Ordering::Relaxed) {
        return;
    }
    TERMINATE.store(true, Ordering::Relaxed);
}

/// Instala los manejadores de señales de terminación.
fn install_signal_handlers() {
    let action = SigAction::new(
        SigHandler::Handler(on_terminate),
        SaFlags::empty(),
        SigSet::empty(),
    );
    // Es un manejador y no SIG_IGN: al ejecutar sudo vuelve al
    // comportamiento por defecto, así Ctrl+C sí le llega a sudo.
    for signal in [Signal::SIGTERM, Signal::SIGHUP, Signal::SIGINT] {
        // SAFETY: el manejador solo usa AtomicBool (async-signal-safe).
        let _ = unsafe { sigaction(signal, &action) };
    }
}

/// Deja la terminal como estaba: sin captura de mouse, fuera de la pantalla
/// alternativa y del modo crudo. Se puede llamar más de una vez.
fn restore_terminal() {
    let _ = execute!(stdout(), DisableMouseCapture, Show);
    ratatui::restore();
}

/// Ejecuta un reintento con sudo. Si hace falta la contraseña, deja la
/// terminal en modo normal, muestra el comando y deja que sudo pregunte;
/// después vuelve a la interfaz.
fn run_elevated(
    terminal: &mut DefaultTerminal,
    request: &elevate::ElevateRequest,
    lang: Lang,
) -> io::Result<ElevateOutcome> {
    let authorized = if elevate::has_cached_credentials() {
        true
    } else {
        SUSPENDED.store(true, Ordering::Relaxed);
        restore_terminal();
        let t = lang.t();
        println!(
            "\n{}\n\n  {}\n\n{}\n",
            t.elevate_banner,
            request.command_line(),
            t.elevate_cancel_hint
        );
        let authorized = elevate::authenticate();
        // La interfaz vuelve igual si sudo falló o se canceló.
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, EnableMouseCapture, Hide)?;
        terminal.clear()?;
        SUSPENDED.store(false, Ordering::Relaxed);
        authorized
    };
    Ok(if authorized {
        elevate::execute(request)
    } else {
        ElevateOutcome::NotAuthorized
    })
}

/// Ante un panic, primero se restaura la terminal y después se imprime el
/// mensaje: si no, quedaría en modo crudo y el error sería ilegible.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous(info);
    }));
}

/// Ejecuta la interfaz hasta que el usuario sale.
pub fn run(options: &RunOptions) -> io::Result<()> {
    install_panic_hook();
    install_signal_handlers();
    let mut terminal = ratatui::try_init()?;
    let result =
        execute!(stdout(), EnableMouseCapture).and_then(|()| event_loop(&mut terminal, options));
    restore_terminal();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, options: &RunOptions) -> io::Result<()> {
    let mut sampler = Sampler::new();
    let mut store = options.new_store();
    let mut state = options.new_state();
    state.lineage = ancestry(Path::new("/proc"), std::process::id() as i32);
    state.can_elevate = elevate::can_elevate();

    // La primera muestra con CPU % necesita un intervalo; se muestra antes
    // una sin porcentajes para no arrancar con la pantalla vacía.
    state.apply(sampler.sample());
    let mut next_sample = Instant::now() + state.refresh.min(Duration::from_millis(500));
    let mut dirty = true;

    loop {
        // Solo se dibuja cuando algo cambió, y ratatui emite únicamente las
        // celdas distintas del cuadro anterior: no hay borrado ni parpadeo.
        if dirty {
            terminal.draw(|frame| super::draw(frame, &mut state, &mut store))?;
            dirty = false;
        }
        if TERMINATE.load(Ordering::Relaxed) {
            return Ok(());
        }
        if options.panic_test {
            panic!("panic de prueba pedido con --panic-test");
        }
        // Un solo hilo: se duerme en `poll` hasta el próximo evento o hasta
        // que toque tomar la siguiente muestra.
        // `poll` puede volver con error si lo interrumpe una señal: se trata
        // como "sin evento" y el bucle vuelve a mirar la bandera de salida.
        let wait = next_sample.saturating_duration_since(Instant::now());
        let ready = match event::poll(wait) {
            Ok(ready) => ready,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        if ready {
            let before = state.refresh;
            match handle_event(&mut state, &event::read()?) {
                Outcome::Quit => return Ok(()),
                Outcome::Redraw => dirty = true,
                Outcome::Ignored => {}
            }
            // Una acción confirmada se ejecuta acá, fuera del manejo de teclas.
            if let Some(request) = state.pending.take() {
                let report = send_signal(&request.targets, request.signal);
                state.notify(
                    report.summary(state.lang, request.signal, &request.label),
                    report.is_problem(),
                );
                state.offer_elevation(
                    Elevated::Signal(request.signal),
                    report.denied_targets,
                    &request.label,
                );
                // Refresca pronto para reflejar el efecto de la señal.
                next_sample = Instant::now() + Duration::from_millis(150);
                dirty = true;
            }
            // El tema confirmado en el selector se guarda en la configuración.
            if let Some(id) = state.theme_to_save.take() {
                if let Some(path) = &options.config_path {
                    if let Err(error) = tui_themes::save_theme(path, &["theme", "name"], &id) {
                        state.notify(error, true);
                    }
                }
                dirty = true;
            }
            if let Some(request) = state.pending_nice.take() {
                let report = set_nice(&request.targets, request.value);
                state.notify(
                    report.summary(state.lang, request.value, &request.label),
                    report.is_problem(),
                );
                state.offer_elevation(
                    Elevated::Nice(request.value),
                    report.denied_targets,
                    &request.label,
                );
                next_sample = Instant::now() + Duration::from_millis(150);
                dirty = true;
            }
            if let Some(request) = state.pending_elevate.take() {
                let outcome = run_elevated(terminal, &request, state.lang)?;
                state.notify(outcome.summary(state.lang, &request), outcome.is_problem());
                next_sample = Instant::now() + Duration::from_millis(150);
                dirty = true;
            }
            // Al entrar al detalle se mide enseguida, sin esperar al intervalo.
            if state.detail_target().is_some() && state.detail_of_selected().is_none() {
                next_sample = Instant::now();
            }
            if state.refresh != before {
                // El intervalo nuevo rige desde ya, no desde la próxima muestra.
                next_sample = Instant::now() + state.refresh;
            }
        } else {
            let sample = sampler.sample_with_detail(state.detail_target());
            state.apply(sample);
            next_sample = Instant::now() + state.refresh;
            dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IconSize;

    #[test]
    fn options_carry_the_config_into_the_initial_state() {
        let config = Config {
            refresh: Duration::from_millis(2500),
            sort: SortKey::Ram,
            icon_size: IconSize::Small,
            language: Lang::Es,
            color: ColorMode::Palette256,
            theme: None,
            icon_mode: None,
            accent: Some((9, 8, 7)),
            icon_theme: Some("Adwaita".into()),
        };
        let options = RunOptions::from_config(&config);
        let state = options.new_state();
        assert_eq!(state.refresh, Duration::from_millis(2500));
        assert_eq!(state.sort, SortKey::Ram);
        assert_eq!(state.icon_size, (8, 4));
        assert_eq!(state.lang, Lang::Es);
        assert_eq!(RunOptions::default().new_state().lang, Lang::En);
        assert!(!state.theme.truecolor());
        assert_eq!(
            state.theme.accent().fg,
            Some(ratatui::style::Color::Indexed(232))
        );
        assert_eq!(options.new_store().locator().theme(), "Adwaita");
    }

    #[test]
    fn forcing_truecolor_ignores_the_environment() {
        let config = Config {
            color: ColorMode::Truecolor,
            ..Config::default()
        };
        assert!(RunOptions::from_config(&config).theme.truecolor());
        assert_eq!(RunOptions::default().icon_size, (16, 8));
    }

    #[test]
    fn the_configured_theme_is_loaded_and_an_unknown_one_falls_back() {
        let named = |name: &str| Config {
            theme: Some(name.into()),
            color: ColorMode::Truecolor,
            ..Config::default()
        };
        let options = RunOptions::from_config_with(&named("gruvbox-dark"), Catalog::builtin());
        assert_eq!(options.theme.id(), "gruvbox-dark");
        assert_eq!(options.theme_notice, None);
        assert_eq!(options.new_state().current_message(), None);

        let options = RunOptions::from_config_with(&named("nope"), Catalog::builtin());
        assert_eq!(options.theme.id(), DEFAULT_THEME);
        let state = options.new_state();
        let message = state.current_message().unwrap();
        assert_eq!(message.text, "Unknown theme \"nope\"");
        assert!(message.is_error);
    }

    #[test]
    fn user_themes_are_offered_and_broken_files_only_warn() {
        let dir = tempfile::tempdir().unwrap();
        let mine = r##"
            name = "Mine"
            [colors]
            bg = "#101010"
            fg = "#f0f0f0"
            dim = "#808080"
            accent = "#00aaff"
            border = "#404040"
            selection_fg = "#ffffff"
            selection_bg = "#303060"
            ok = "#00cc66"
            warn = "#ffcc00"
            danger = "#ff3355"
        "##;
        std::fs::write(dir.path().join("mine.toml"), mine).unwrap();
        std::fs::write(dir.path().join("broken.toml"), "[colors]\n").unwrap();
        let config = Config {
            theme: Some("mine".into()),
            language: Lang::Es,
            ..Config::default()
        };
        let options = RunOptions::from_config_with(&config, Catalog::with_user_dir(dir.path()));
        assert_eq!(options.theme.id(), "mine");
        assert_eq!(options.themes.catalog.errors().len(), 1);
        let notice = options.theme_notice.clone().unwrap();
        assert!(notice.starts_with("1 archivo(s) de tema"), "{notice}");
    }
}
