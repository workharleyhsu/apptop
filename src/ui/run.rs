//! Bucle interactivo: terminal real, eventos y refresco periódico.

use std::io::{self, stdout};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;
use ratatui::DefaultTerminal;

use crate::apps::Sampler;
use crate::icons::IconStore;

use super::input::{handle_event, Outcome};
use super::state::{clamp_refresh, State};
use super::theme::Theme;

/// Opciones de la sesión interactiva.
#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    /// Intervalo entre muestras.
    pub refresh: Duration,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            refresh: Duration::from_secs(1),
        }
    }
}

/// Ejecuta la interfaz hasta que el usuario sale.
pub fn run(options: RunOptions) -> io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let result =
        execute!(stdout(), EnableMouseCapture).and_then(|()| event_loop(&mut terminal, options));
    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, options: RunOptions) -> io::Result<()> {
    let mut sampler = Sampler::new();
    let mut store = IconStore::new();
    let mut state = State::new(Theme::detect());
    state.refresh = clamp_refresh(options.refresh);

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
            if state.refresh != before {
                // El intervalo nuevo rige desde ya, no desde la próxima muestra.
                next_sample = Instant::now() + state.refresh;
            }
        } else {
            state.apply(sampler.sample());
            next_sample = Instant::now() + state.refresh;
            dirty = true;
        }
    }
}
