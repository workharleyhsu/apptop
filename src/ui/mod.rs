//! Interfaz de terminal: grilla, vista de detalle y diálogos.

pub mod bars;
pub mod detail;
pub mod format;
pub mod graph;
pub mod grid;
pub mod input;
pub mod run;
pub mod state;
pub mod theme;
pub mod tree;

use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::{Frame, Terminal};

use crate::icons::IconStore;

pub use run::{run, RunOptions};
pub use state::{Mode, Sections, SortKey, State, View};
pub use theme::Theme;

/// Dibuja un cuadro completo: barra superior, vista actual, línea de
/// filtro (si hay) y barra de atajos.
pub fn draw(frame: &mut Frame, state: &mut State, store: &mut IconStore) {
    let full = frame.area();
    let buf = frame.buffer_mut();
    let row = |y: u16| Rect::new(full.x, y, full.width, 1);
    let mut body = full;

    // Con muy poco alto se sacrifican las barras antes que el contenido.
    if full.height >= 3 {
        bars::draw_top(buf, row(full.y), state);
        bars::draw_bottom(buf, row(full.y + full.height - 1), state);
        body = Rect::new(full.x, full.y + 1, full.width, full.height - 2);
    }
    // Línea del filtro: visible mientras se escribe o si hay uno aplicado.
    let filtering = state.mode == Mode::Filter || !state.filter.is_empty();
    if state.view == View::Grid && filtering && body.height > 1 {
        body.height -= 1;
        let cursor = if state.mode == Mode::Filter {
            "▏"
        } else {
            ""
        };
        let line = format!("/{}{cursor}", state.filter);
        buf.set_stringn(
            body.x,
            body.y + body.height,
            line,
            body.width as usize,
            state.theme.selected(),
        );
    }
    match state.view {
        View::Grid => grid::draw(buf, body, state, store),
        View::Detail => detail::draw(buf, body, state, store),
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
        state.system = crate::collect::SystemStats {
            cpu_pct: Some(37.0),
            cores: 8,
            mem: Some((16 << 30, 6 << 30)),
            swap: Some((8 << 30, 1 << 29)),
            uptime: Some(3600.0),
        };
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
    fn filter_line_appears_while_typing_and_when_applied() {
        let mut state = State::new(Theme::default());
        state.set_apps(state::tests::sample_apps());
        state.start_filter();
        state.filter_push('f');
        state.filter_push('i');
        let text = render_to_text(60, 14, &mut state, &mut test_store());
        let filter_line = |text: &str| text.lines().rev().nth(1).map(str::to_string);
        assert_eq!(filter_line(&text).as_deref(), Some("/fi▏"));
        assert!(text.lines().last().unwrap().starts_with("Enter aplicar"));
        assert!(text.contains("Firefox") && !text.contains("Alacritty"));
        state.accept_filter();
        let text = render_to_text(60, 14, &mut state, &mut test_store());
        assert_eq!(filter_line(&text).as_deref(), Some("/fi"));
        assert!(text.lines().last().unwrap().contains("Esc quitar filtro"));
        state.filter = "zzz".into();
        state.reorder();
        let text = render_to_text(60, 14, &mut state, &mut test_store());
        assert!(text.contains("Ninguna app coincide"));
    }

    #[test]
    fn bars_show_totals_and_shortcuts_and_nd_when_unknown() {
        let mut state = State::new(Theme::default());
        state.set_apps(state::tests::sample_apps());
        let text = render_to_text(100, 20, &mut state, &mut test_store());
        let top = text.lines().next().unwrap();
        assert!(
            top.starts_with("apptop  CPU n/d  RAM n/d  Swap n/d"),
            "{top}"
        );
        assert!(top.ends_with("3 apps · 2 servicios"), "{top}");
        assert!(text.lines().last().unwrap().contains("q salir"));

        state.system.cpu_pct = Some(250.0);
        state.system.mem = Some((8 << 30, 2 << 30));
        state.system.swap = Some((0, 0));
        let text = render_to_text(100, 20, &mut state, &mut test_store());
        let top = text.lines().next().unwrap();
        assert!(top.contains("CPU ━━━━━━━━━━ 250%"), "{top}");
        assert!(
            top.contains("2.0G/8.0G") && top.contains("sin swap"),
            "{top}"
        );
    }

    /// Estado en vista de detalle con una app de tres procesos.
    pub(crate) fn detail_state() -> State {
        use crate::apps::group::tests::proc;
        let mut state = State::new(Theme::default());
        let mut apps = state::tests::sample_apps();
        let cg = "/user.slice/app.slice/app-firefox-1.scope";
        let mut procs = vec![
            proc(4100, 1, "/usr/lib64/firefox/firefox", cg),
            proc(4130, 4100, "/usr/lib64/firefox/firefox", cg),
            proc(4222, 4100, "/usr/lib64/firefox/firefox", cg),
        ];
        procs[1].cmdline = vec![
            "firefox".into(),
            "-contentproc".into(),
            "-isForBrowser".into(),
            "tab".into(),
        ];
        procs[2].state = 'T';
        procs[2].cpu_pct = 97.2;
        for (p, (start, threads, rss)) in
            procs
                .iter_mut()
                .zip([(6000, 120, 900), (6400, 30, 400), (9000, 22, 250)])
        {
            p.starttime = start;
            p.threads = threads;
            p.rss = rss << 20;
        }
        apps[1].procs = procs;
        state.set_apps(apps);
        state.system.uptime = Some(7460.0);
        state.users.insert(1000, "ana".into());
        state.selected = Some("App:Firefox".into());
        state.enter_detail();
        state.detail = Some(crate::apps::DetailData {
            app_id: "App:Firefox".into(),
            pss: Some(1200 << 20),
            io: crate::collect::IoReading::Rate(crate::collect::IoRate {
                read_bps: 2_400_000.0,
                write_bps: 350_000.0,
            }),
        });
        // Historial de 90 s: CPU en rampa con un pico, RAM creciendo.
        for t in 0..90 {
            let cpu = if (40..50).contains(&t) {
                180.0
            } else {
                t as f32 / 2.0
            };
            let app = &mut state.apps[1];
            (app.cpu_pct, app.rss) = (cpu, (4000 + t * 15) << 20);
            state.record_history(t as f64);
        }
        let app = &mut state.apps[1];
        (app.cpu_pct, app.rss) = (23.5, 5400 << 20);
        state.record_history(90.0);
        state
    }

    #[test]
    fn detail_snapshot_80x24() {
        let mut state = detail_state();
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        insta::assert_snapshot!(text);
    }

    #[test]
    fn detail_tree_marks_the_selected_process_and_scrolls() {
        let mut state = detail_state();
        state.move_proc_cursor(2);
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        assert!(text.contains("▶   4130  ├─ firefox -contentproc"), "{text}");
        assert!(text.contains("T pausado"), "{text}");
        // Con poco alto el árbol se desplaza para mostrar la fila elegida.
        state.move_proc_cursor(1);
        let text = render_to_text(80, 18, &mut state, &mut test_store());
        assert!(text.contains("▶   4222"), "{text}");
        assert_eq!(state.tree.scroll, 1);
        assert_eq!(state.tree.len, 3);
    }

    #[test]
    fn detail_shows_nd_for_unknown_values_and_survives_app_exit() {
        let mut state = detail_state();
        state.detail = None;
        state.system.uptime = None;
        state.apps[1].procs.iter_mut().for_each(|p| p.uid = None);
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        assert!(text.contains("PSS n/d"), "{text}");
        assert!(
            text.contains("Usuario   n/d") && text.contains("Activa    n/d"),
            "{text}"
        );

        // La app muere mientras se mira su detalle.
        state.apps.retain(|a| a.name != "Firefox");
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        assert!(text.contains("ya no está en ejecución"));
        for (w, h) in [(1, 1), (20, 4), (30, 30)] {
            let _ = render_to_text(w, h, &mut detail_state(), &mut test_store());
        }
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
