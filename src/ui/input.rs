//! Traducción de eventos de teclado y mouse a cambios de estado.

use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::actions::Signal;

use super::state::{Mode, Move, State, View};

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
    match state.mode {
        Mode::Filter => return filter_key(state, key),
        Mode::Confirm(_) => {
            // Solo 's' (o 'y') confirma; cualquier otra tecla cancela.
            let yes = matches!(key.code, KeyCode::Char('s' | 'S' | 'y' | 'Y'));
            state.answer_confirm(yes);
            return Outcome::Redraw;
        }
        Mode::Normal => {}
    }
    // Acciones sobre la app seleccionada (o el proceso elegido en el detalle).
    match key.code {
        KeyCode::Char('x') => {
            state.ask_signal(Signal::SIGTERM);
            return Outcome::Redraw;
        }
        KeyCode::Char('K') => {
            state.ask_signal(Signal::SIGKILL);
            return Outcome::Redraw;
        }
        KeyCode::Char('p') => {
            state.toggle_pause();
            return Outcome::Redraw;
        }
        _ => {}
    }
    // El intervalo de refresco se ajusta desde cualquier vista.
    match key.code {
        KeyCode::Char('+') | KeyCode::Char('=') => {
            state.adjust_refresh(false);
            return Outcome::Redraw;
        }
        KeyCode::Char('-') => {
            state.adjust_refresh(true);
            return Outcome::Redraw;
        }
        _ => {}
    }
    match state.view {
        View::Grid => grid_key(state, key),
        View::Detail => detail_key(state, key),
    }
}

/// Teclas mientras se escribe el filtro: todo carácter es texto.
fn filter_key(state: &mut State, key: &KeyEvent) -> Outcome {
    match key.code {
        KeyCode::Esc => state.clear_filter(),
        KeyCode::Enter => state.accept_filter(),
        KeyCode::Backspace => {
            if state.filter.is_empty() {
                state.accept_filter();
            } else {
                state.filter_pop();
            }
        }
        // Las flechas siguen navegando sin salir del filtro.
        KeyCode::Left => state.move_selection(Move::Left),
        KeyCode::Right => state.move_selection(Move::Right),
        KeyCode::Up => state.move_selection(Move::Up),
        KeyCode::Down => state.move_selection(Move::Down),
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            state.filter_push(c)
        }
        _ => return Outcome::Ignored,
    }
    Outcome::Redraw
}

fn grid_key(state: &mut State, key: &KeyEvent) -> Outcome {
    let movement = match key.code {
        KeyCode::Char('q') => return Outcome::Quit,
        KeyCode::Char('/') => {
            state.start_filter();
            return Outcome::Redraw;
        }
        KeyCode::Char('s') => {
            state.cycle_sort();
            return Outcome::Redraw;
        }
        KeyCode::Tab => {
            state.cycle_sections();
            return Outcome::Redraw;
        }
        // Esc en la grilla quita el filtro aplicado.
        KeyCode::Esc if !state.filter.is_empty() => {
            state.clear_filter();
            return Outcome::Redraw;
        }
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
        code => {
            let page = (state.tree.area.height as isize).max(1);
            let delta = match code {
                KeyCode::Up | KeyCode::Char('k') => -1,
                KeyCode::Down | KeyCode::Char('j') => 1,
                KeyCode::PageUp => -page,
                KeyCode::PageDown => page,
                KeyCode::Home | KeyCode::Char('g') => isize::MIN / 2,
                KeyCode::End | KeyCode::Char('G') => isize::MAX / 2,
                _ => return Outcome::Ignored,
            };
            state.move_proc_cursor(delta);
            Outcome::Redraw
        }
    }
}

pub fn handle_mouse(state: &mut State, mouse: &MouseEvent) -> Outcome {
    // Con un diálogo abierto el mouse no hace nada: evita confirmar o
    // cambiar de app por accidente.
    if state.mode != Mode::Normal && state.mode != Mode::Filter {
        return Outcome::Ignored;
    }
    if state.view == View::Detail {
        return match mouse.kind {
            MouseEventKind::ScrollDown => {
                state.move_proc_cursor(1);
                Outcome::Redraw
            }
            MouseEventKind::ScrollUp => {
                state.move_proc_cursor(-1);
                Outcome::Redraw
            }
            MouseEventKind::Down(MouseButton::Left) => {
                match state.tree.hit(mouse.column, mouse.row) {
                    Some(row) => {
                        state.select_proc_row(row);
                        Outcome::Redraw
                    }
                    None => Outcome::Ignored,
                }
            }
            _ => Outcome::Ignored,
        };
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
        handle_event(&mut s, &key(KeyCode::Down));
        assert_eq!(handle_event(&mut s, &key(KeyCode::Esc)), Outcome::Redraw);
        assert_eq!((s.view, s.selected_index()), (View::Grid, Some(1)));
    }

    #[test]
    fn returning_from_detail_restores_the_grid_exactly() {
        use crate::ui::state::SortKey;
        // 12 apps en 4 columnas y una ventana baja: la última fila obliga a
        // desplazar la grilla.
        let mut s = laid_out(12, 0, 80, 12);
        handle_event(&mut s, &key(KeyCode::Char('s')));
        handle_event(&mut s, &key(KeyCode::End));
        s.scroll = s.layout.scroll_to_show(11, 0, 12);
        assert!(s.scroll > 0);
        let before = (s.selected.clone(), s.scroll, s.sort, s.filter.clone());

        handle_event(&mut s, &key(KeyCode::Enter));
        // Lo que se haga dentro del detalle no toca el estado de la grilla.
        handle_event(&mut s, &key(KeyCode::Down));
        handle_event(&mut s, &key(KeyCode::Down));
        assert_eq!(handle_event(&mut s, &key(KeyCode::Esc)), Outcome::Redraw);

        assert_eq!(s.view, View::Grid);
        assert_eq!(
            (s.selected.clone(), s.scroll, s.sort, s.filter.clone()),
            before
        );
        assert_eq!(s.sort, SortKey::Cpu);
        // Backspace también vuelve.
        handle_event(&mut s, &key(KeyCode::Enter));
        handle_event(&mut s, &key(KeyCode::Backspace));
        assert_eq!((s.view, s.selected_index()), (View::Grid, Some(11)));
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
            handle_event(&mut s, &key(KeyCode::Char('z'))),
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
    fn slash_types_a_filter_where_letters_are_text_not_commands() {
        let mut s = laid_out(3, 2, 80, 24);
        handle_event(&mut s, &key(KeyCode::Char('/')));
        assert_eq!(s.mode, Mode::Filter);
        // 'q', 's' y 'j' son texto mientras se filtra.
        for c in ['s', 'v', 'c', 'q'] {
            assert_eq!(
                handle_event(&mut s, &key(KeyCode::Char(c))),
                Outcome::Redraw
            );
        }
        assert_eq!(s.filter, "svcq");
        handle_event(&mut s, &key(KeyCode::Backspace));
        assert_eq!(s.visible().count(), 2);
        handle_event(&mut s, &key(KeyCode::Right));
        assert_eq!(s.selected_index(), Some(1));
        handle_event(&mut s, &key(KeyCode::Enter));
        assert_eq!((s.mode.clone(), s.filter.as_str()), (Mode::Normal, "svc"));
        // Ya en modo normal, Esc limpia el filtro y 'q' vuelve a salir.
        assert_eq!(handle_event(&mut s, &key(KeyCode::Esc)), Outcome::Redraw);
        assert_eq!(s.visible().count(), 5);
        assert_eq!(
            handle_event(&mut s, &key(KeyCode::Char('q'))),
            Outcome::Quit
        );
    }

    #[test]
    fn esc_while_filtering_clears_and_backspace_on_empty_exits() {
        let mut s = laid_out(3, 2, 80, 24);
        handle_event(&mut s, &key(KeyCode::Char('/')));
        handle_event(&mut s, &key(KeyCode::Char('x')));
        handle_event(&mut s, &key(KeyCode::Esc));
        assert_eq!(
            (s.mode.clone(), s.filter.as_str(), s.visible().count()),
            (Mode::Normal, "", 5)
        );
        handle_event(&mut s, &key(KeyCode::Char('/')));
        handle_event(&mut s, &key(KeyCode::Backspace));
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn s_cycles_sort_and_tab_cycles_sections() {
        use crate::ui::state::{Sections, SortKey};
        let mut s = laid_out(3, 2, 80, 24);
        handle_event(&mut s, &key(KeyCode::Char('s')));
        assert_eq!(s.sort, SortKey::Cpu);
        handle_event(&mut s, &key(KeyCode::Tab));
        assert_eq!((s.sections, s.visible().count()), (Sections::Apps, 3));
    }

    #[test]
    fn plus_and_minus_change_the_refresh_interval() {
        use std::time::Duration;
        let mut s = laid_out(1, 0, 80, 24);
        handle_event(&mut s, &key(KeyCode::Char('+')));
        assert_eq!(s.refresh, Duration::from_millis(750));
        handle_event(&mut s, &key(KeyCode::Char('-')));
        handle_event(&mut s, &key(KeyCode::Char('-')));
        assert_eq!(s.refresh, Duration::from_millis(1250));
        // Mientras se filtra, '+' y '-' son texto.
        handle_event(&mut s, &key(KeyCode::Char('/')));
        handle_event(&mut s, &key(KeyCode::Char('-')));
        assert_eq!(
            (s.refresh, s.filter.as_str()),
            (Duration::from_millis(1250), "-")
        );
    }

    #[test]
    fn x_and_shift_k_ask_before_doing_anything() {
        use crate::apps::group::tests::proc;
        let mut s = laid_out(2, 0, 80, 24);
        s.apps[0].procs = vec![proc(500, 1, "/b/a", "/"), proc(501, 500, "/b/a", "/")];
        handle_event(&mut s, &key(KeyCode::Char('x')));
        assert!(matches!(s.mode, Mode::Confirm(_)));
        // Cualquier tecla que no sea 's' cancela, incluso Enter y 'q'.
        for code in [
            KeyCode::Enter,
            KeyCode::Char('q'),
            KeyCode::Esc,
            KeyCode::Char('n'),
        ] {
            assert_eq!(handle_event(&mut s, &key(code)), Outcome::Redraw);
            assert!(s.mode == Mode::Normal && s.pending.is_none(), "{code:?}");
            handle_event(&mut s, &key(KeyCode::Char('x')));
        }
        // El mouse no confirma ni mueve nada con el diálogo abierto.
        let click = mouse(MouseEventKind::Down(MouseButton::Left), 30, 4);
        assert_eq!(handle_event(&mut s, &click), Outcome::Ignored);
        assert!(matches!(s.mode, Mode::Confirm(_)));

        handle_event(&mut s, &key(KeyCode::Char('s')));
        let pending = s.pending.take().unwrap();
        assert_eq!(
            (pending.signal, pending.targets.len()),
            (Signal::SIGTERM, 2)
        );

        handle_event(&mut s, &key(KeyCode::Char('K')));
        handle_event(&mut s, &key(KeyCode::Char('y')));
        assert_eq!(s.pending.take().unwrap().signal, Signal::SIGKILL);
        // 'k' minúscula sigue siendo "arriba", no matar.
        handle_event(&mut s, &key(KeyCode::Char('k')));
        assert!(s.mode == Mode::Normal && s.pending.is_none());
    }

    #[test]
    fn p_pauses_and_resumes_immediately() {
        use crate::apps::group::tests::proc;
        let mut s = laid_out(2, 0, 80, 24);
        s.apps[0].procs = vec![proc(500, 1, "/b/a", "/")];
        assert_eq!(
            handle_event(&mut s, &key(KeyCode::Char('p'))),
            Outcome::Redraw
        );
        assert_eq!(s.pending.take().unwrap().signal, Signal::SIGSTOP);
        s.apps[0].procs[0].state = 'T';
        handle_event(&mut s, &key(KeyCode::Char('p')));
        assert_eq!(s.pending.take().unwrap().signal, Signal::SIGCONT);
        // Mientras se filtra, 'p' es texto.
        handle_event(&mut s, &key(KeyCode::Char('/')));
        handle_event(&mut s, &key(KeyCode::Char('p')));
        assert!(s.pending.is_none() && s.filter == "p");
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
