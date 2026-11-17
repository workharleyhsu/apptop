//! Interfaz de terminal: grilla, vista de detalle y diálogos.

pub mod detail;
pub mod format;
pub mod grid;
pub mod input;
pub mod run;
pub mod state;
pub mod theme;

use ratatui::backend::TestBackend;
use ratatui::{Frame, Terminal};

use crate::icons::IconStore;

pub use run::{run, RunOptions};
pub use state::{State, View};
pub use theme::Theme;

/// Dibuja un cuadro completo de la interfaz.
pub fn draw(frame: &mut Frame, state: &mut State, store: &mut IconStore) {
    let area = frame.area();
    match state.view {
        View::Grid => grid::draw(frame.buffer_mut(), area, state, store),
        View::Detail => detail::draw(frame.buffer_mut(), area, state, store),
    }
}

/// Renderiza un cuadro a texto plano, sin terminal real. Lo usan
/// `--snapshot` y los tests.
pub fn render_to_text(width: u16, height: u16, state: &mut State, store: &mut IconStore) -> String {
    let mut terminal = match Terminal::new(TestBackend::new(width, height)) {
        Ok(terminal) => terminal,
        Err(err) => return format!("error al crear la terminal de prueba: {err}\n"),
    };
    if let Err(err) = terminal.draw(|frame| draw(frame, state, store)) {
        return format!("error al dibujar: {err}\n");
    }
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        let line: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::IconLocator;

    pub(crate) fn test_store() -> IconStore {
        IconStore::with(IconLocator::with_theme("hicolor"), None)
    }

    #[test]
    fn grid_snapshot_80x24() {
        let mut state = State::new(Theme::default());
        state.set_apps(state::tests::sample_apps());
        state.selected = Some("App:Firefox".into());
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        insta::assert_snapshot!(text);
    }

    #[test]
    fn grid_snapshot_narrow_and_scrolled() {
        let mut state = State::new(Theme::default());
        state.set_apps(state::tests::sample_apps());
        state.selected = Some("Service:pipewire".into());
        let text = render_to_text(40, 16, &mut state, &mut test_store());
        insta::assert_snapshot!(text);
        assert!(state.scroll > 0);
    }

    #[test]
    fn empty_state_shows_a_message_and_tiny_sizes_do_not_panic() {
        let mut state = State::new(Theme::default());
        let text = render_to_text(60, 10, &mut state, &mut test_store());
        assert!(text.contains("No hay aplicaciones"));
        state.set_apps(state::tests::sample_apps());
        for (w, h) in [(1, 1), (3, 2), (17, 5), (200, 3)] {
            let _ = render_to_text(w, h, &mut state, &mut test_store());
        }
    }

    #[test]
    fn nothing_paints_a_background() {
        let mut state = State::new(Theme::default());
        state.set_apps(state::tests::sample_apps());
        let mut store = test_store();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| draw(f, &mut state, &mut store)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(buffer
            .content
            .iter()
            .all(|c| c.bg == ratatui::style::Color::Reset));
    }
}
