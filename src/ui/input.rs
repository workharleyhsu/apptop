//! Traducción de eventos de teclado y mouse a cambios de estado.

use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use super::state::{Move, State, View};

/// Qué debe hacer el bucle principal después de un evento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Nada cambió: no hace falta redibujar.
    Ignored,
    /// Cambió el estado: redibujar.
    Redraw,
    Quit,
}

pub fn handle_event(state: &mut State, event: &Event) -> Outcome {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => handle_key(state, key),
        Event::Mouse(mouse) => handle_mouse(state, mouse),
        Event::Resize(_, _) => Outcome::Redraw,
        _ => Outcome::Ignored,
    }
}

pub fn handle_key(state: &mut State, key: &KeyEvent) -> Outcome {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Outcome::Quit;
    }
    match state.view {
        View::Grid => grid_key(state, key),
        View::Detail => detail_key(state, key),
    }
}

fn grid_key(state: &mut State, key: &KeyEvent) -> Outcome {
    let movement = match key.code {
        KeyCode::Char('q') => return Outcome::Quit,
        KeyCode::Enter => {
            state.enter_detail();
            return Outcome::Redraw;
        }
        KeyCode::Left | KeyCode::Char('h') => Move::Left,
        KeyCode::Right | KeyCode::Char('l') => Move::Right,
        KeyCode::Up | KeyCode::Char('k') => Move::Up,
        KeyCode::Down | KeyCode::Char('j') => Move::Down,
        KeyCode::PageUp => Move::PageUp,
        KeyCode::PageDown => Move::PageDown,
        KeyCode::Home | KeyCode::Char('g') => Move::First,
        KeyCode::End | KeyCode::Char('G') => Move::Last,
        _ => return Outcome::Ignored,
    };
    state.move_selection(movement);
    Outcome::Redraw
}

fn detail_key(state: &mut State, key: &KeyEvent) -> Outcome {
    match key.code {
        KeyCode::Char('q') => Outcome::Quit,
        KeyCode::Esc | KeyCode::Backspace => {
            state.back_to_grid();
            Outcome::Redraw
        }
        _ => Outcome::Ignored,
    }
}

pub fn handle_mouse(state: &mut State, mouse: &MouseEvent) -> Outcome {
    if state.view != View::Grid {
        return Outcome::Ignored;
    }
    match mouse.kind {
        MouseEventKind::ScrollDown => {
            state.move_selection(Move::Down);
            Outcome::Redraw
        }
        MouseEventKind::ScrollUp => {
            state.move_selection(Move::Up);
            Outcome::Redraw
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let Some(index) = state.layout.hit(mouse.column, mouse.row, state.scroll) else {
                return Outcome::Ignored;
            };
            // Clic sobre la app ya seleccionada: entra al detalle.
            if state.selected_index() == Some(index) {
                state.enter_detail();
            } else {
                state.select_index(index);
            }
            Outcome::Redraw
        }
        _ => Outcome::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::state::tests::laid_out;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn arrows_and_hjkl_move_the_selection() {
        let mut s = laid_out(8, 0, 80, 24);
        for (code, expected) in [
            (KeyCode::Right, 1),
            (KeyCode::Char('l'), 2),
            (KeyCode::Down, 6),
            (KeyCode::Char('h'), 5),
            (KeyCode::Char('k'), 1),
            (KeyCode::Left, 0),
            (KeyCode::Char('j'), 4),
            (KeyCode::End, 7),
            (KeyCode::Home, 0),
            (KeyCode::Char('G'), 7),
            (KeyCode::Char('g'), 0),
        ] {
            assert_eq!(handle_event(&mut s, &key(code)), Outcome::Redraw);
            assert_eq!(s.selected_index(), Some(expected), "{code:?}");
        }
    }

    #[test]
    fn enter_opens_detail_and_esc_returns_keeping_selection() {
        let mut s = laid_out(8, 0, 80, 24);
        handle_event(&mut s, &key(KeyCode::Right));
        handle_event(&mut s, &key(KeyCode::Enter));
        assert_eq!(s.view, View::Detail);
        // En detalle las flechas no mueven la selección de la grilla.
        assert_eq!(handle_event(&mut s, &key(KeyCode::Right)), Outcome::Ignored);
        assert_eq!(handle_event(&mut s, &key(KeyCode::Esc)), Outcome::Redraw);
        assert_eq!((s.view, s.selected_index()), (View::Grid, Some(1)));
    }

    #[test]
    fn q_and_ctrl_c_quit_from_any_view() {
        let mut s = laid_out(2, 0, 80, 24);
        assert_eq!(
            handle_event(&mut s, &key(KeyCode::Char('q'))),
            Outcome::Quit
        );
        s.enter_detail();
        assert_eq!(
            handle_event(&mut s, &key(KeyCode::Char('q'))),
            Outcome::Quit
        );
        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(handle_event(&mut s, &ctrl_c), Outcome::Quit);
        assert_eq!(
            handle_event(&mut s, &key(KeyCode::Char('x'))),
            Outcome::Ignored
        );
    }

    #[test]
    fn click_selects_then_enters_and_wheel_moves_rows() {
        let mut s = laid_out(8, 0, 80, 24);
        let click = MouseEventKind::Down(MouseButton::Left);
        // Segunda baldosa de la primera fila: x = 4 + 18.
        assert_eq!(handle_event(&mut s, &mouse(click, 30, 4)), Outcome::Redraw);
        assert_eq!((s.selected_index(), s.view), (Some(1), View::Grid));
        handle_event(&mut s, &mouse(click, 30, 4));
        assert_eq!(s.view, View::Detail);
        s.back_to_grid();

        assert_eq!(handle_event(&mut s, &mouse(click, 0, 0)), Outcome::Ignored);
        handle_event(&mut s, &mouse(MouseEventKind::ScrollDown, 0, 0));
        assert_eq!(s.selected_index(), Some(5));
        handle_event(&mut s, &mouse(MouseEventKind::ScrollUp, 0, 0));
        assert_eq!(s.selected_index(), Some(1));
        assert_eq!(
            handle_event(&mut s, &mouse(MouseEventKind::Moved, 3, 3)),
            Outcome::Ignored
        );
    }

    #[test]
    fn resize_requests_a_redraw() {
        let mut s = laid_out(1, 0, 80, 24);
        assert_eq!(
            handle_event(&mut s, &Event::Resize(100, 40)),
            Outcome::Redraw
        );
    }
}
