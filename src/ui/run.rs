//! Bucle interactivo: terminal real, eventos y refresco periódico.

use std::io::{self, stdout};
use std::path::Path;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;
use ratatui::DefaultTerminal;

use crate::actions::{ancestry, send_signal, set_nice};
use crate::apps::Sampler;
use crate::config::{ColorMode, Config};
use crate::icons::cache::default_cache_dir;
use crate::icons::{IconLocator, IconStore};

use super::input::{handle_event, Outcome};
use super::state::{clamp_refresh, SortKey, State};
use super::theme::{terminal_has_truecolor, Theme, DEFAULT_ACCENT};

/// Opciones de la sesión interactiva (configuración + línea de comandos).
#[derive(Debug, Clone, PartialEq)]
pub struct RunOptions {
    /// Intervalo entre muestras.
    pub refresh: Duration,
    pub sort: SortKey,
    /// Tamaño de los iconos de la grilla, en celdas.
    pub icon_size: (u16, u16),
    pub theme: Theme,
    /// Tema de iconos; `None` = detectar.
    pub icon_theme: Option<String>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self::from_config(&Config::default())
    }
}

impl RunOptions {
    pub fn from_config(config: &Config) -> Self {
        let truecolor = match config.color {
            ColorMode::Auto => terminal_has_truecolor(),
            ColorMode::Truecolor => true,
            ColorMode::Palette256 => false,
        };
        Self {
            refresh: config.refresh,
            sort: config.sort,
            icon_size: config.icon_size.cells(),
            theme: Theme {
                truecolor,
                accent_rgb: config.accent.unwrap_or(DEFAULT_ACCENT),
            },
            icon_theme: config.icon_theme.clone(),
        }
    }

    /// Estado inicial de la interfaz con estas opciones.
    pub fn new_state(&self) -> State {
        let mut state = State::new(self.theme);
        state.refresh = clamp_refresh(self.refresh);
        state.sort = self.sort;
        state.icon_size = self.icon_size;
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

/// Ejecuta la interfaz hasta que el usuario sale.
pub fn run(options: &RunOptions) -> io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let result =
        execute!(stdout(), EnableMouseCapture).and_then(|()| event_loop(&mut terminal, options));
    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, options: &RunOptions) -> io::Result<()> {
    let mut sampler = Sampler::new();
    let mut store = options.new_store();
    let mut state = options.new_state();
    state.lineage = ancestry(Path::new("/proc"), std::process::id() as i32);

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
        // Un solo hilo: se duerme en `poll` hasta el próximo evento o hasta
        // que toque tomar la siguiente muestra.
        let wait = next_sample.saturating_duration_since(Instant::now());
        if event::poll(wait)? {
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
                    report.summary(request.signal, &request.label),
                    report.is_problem(),
                );
                // Refresca pronto para reflejar el efecto de la señal.
                next_sample = Instant::now() + Duration::from_millis(150);
                dirty = true;
            }
            if let Some(request) = state.pending_nice.take() {
                let report = set_nice(&request.targets, request.value);
                state.notify(
                    report.summary(request.value, &request.label),
                    report.is_problem(),
                );
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
            color: ColorMode::Palette256,
            accent: Some((9, 8, 7)),
            icon_theme: Some("Adwaita".into()),
        };
        let options = RunOptions::from_config(&config);
        let state = options.new_state();
        assert_eq!(state.refresh, Duration::from_millis(2500));
        assert_eq!(state.sort, SortKey::Ram);
        assert_eq!(state.icon_size, (8, 4));
        assert!(!state.theme.truecolor);
        assert_eq!(state.theme.accent_rgb, (9, 8, 7));
        assert_eq!(options.new_store().locator().theme(), "Adwaita");
    }

    #[test]
    fn forcing_truecolor_ignores_the_environment() {
        let config = Config {
            color: ColorMode::Truecolor,
            ..Config::default()
        };
        assert!(RunOptions::from_config(&config).theme.truecolor);
        assert_eq!(RunOptions::default().icon_size, (16, 8));
    }
}
