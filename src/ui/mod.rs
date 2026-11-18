//! Interfaz de terminal: grilla, vista de detalle y diálogos.

pub mod bars;
pub mod detail;
pub mod dialog;
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
pub use state::{ActionRequest, Mode, Sections, SortKey, State, View, MIN_SIZE};
pub use theme::Theme;

/// Dibuja un cuadro completo: barra superior, vista actual, línea de
/// filtro (si hay) y barra de atajos.
pub fn draw(frame: &mut Frame, state: &mut State, store: &mut IconStore) {
    let full = frame.area();
    let buf = frame.buffer_mut();
    // Texto y fondo del tema en todo el cuadro. El tema de fábrica usa los
    // de la terminal y no pinta nada.
    buf.set_style(full, state.theme.text());
    if state.enforce_min_size && (full.width < MIN_SIZE.0 || full.height < MIN_SIZE.1) {
        draw_too_small(buf, full, state);
        return;
    }
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
    // Los diálogos van encima de la vista.
    match &state.mode {
        Mode::Confirm(request) => dialog::draw_confirm(buf, body, state, request),
        Mode::Nice(request) => dialog::draw_nice(buf, body, state, request),
        Mode::SignalMenu(cursor) => dialog::draw_signal_menu(buf, body, state, *cursor),
        Mode::Elevate(request) => dialog::draw_elevate(buf, body, state, request),
        Mode::Normal | Mode::Filter => {}
    }
}

/// Aviso que reemplaza a la interfaz cuando la terminal es demasiado chica.
fn draw_too_small(buf: &mut ratatui::buffer::Buffer, area: Rect, state: &State) {
    let lang = state.lang;
    let lines = [
        (lang.t().too_small.to_string(), state.theme.selected()),
        (
            lang.size_notice((area.width, area.height), MIN_SIZE),
            state.theme.text(),
        ),
        (lang.t().too_small_hint.to_string(), state.theme.dim()),
    ];
    let top = area.y + area.height.saturating_sub(lines.len() as u16) / 2;
    for (i, (text, style)) in lines.iter().enumerate() {
        let y = top + i as u16;
        if y >= area.y + area.height {
            break;
        }
        let width = text.chars().count() as u16;
        let x = area.x + area.width.saturating_sub(width) / 2;
        buf.set_stringn(x, y, text, area.width as usize, *style);
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
        assert!(text.lines().last().unwrap().starts_with("Enter apply"));
        assert!(text.contains("Firefox") && !text.contains("Alacritty"));
        state.accept_filter();
        let text = render_to_text(60, 14, &mut state, &mut test_store());
        assert_eq!(filter_line(&text).as_deref(), Some("/fi"));
        assert!(text.lines().last().unwrap().contains("Esc clear filter"));
        state.filter = "zzz".into();
        state.reorder();
        let text = render_to_text(60, 14, &mut state, &mut test_store());
        assert!(text.contains("No app matches"));
    }

    #[test]
    fn paused_apps_are_flagged_in_the_grid() {
        use crate::apps::group::tests::proc;
        let mut state = State::new(Theme::default());
        let mut apps = state::tests::sample_apps();
        apps[2].procs = vec![proc(1, 0, "/b/a", "/"), proc(2, 1, "/b/a", "/")];
        apps[2].procs.iter_mut().for_each(|p| p.state = 'T');
        state.set_apps(apps);
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        assert_eq!(text.matches("‖ paused").count(), 1, "{text}");
        assert!(
            text.contains("p resume"),
            "Alacritty (pausada) es la seleccionada"
        );
        state.selected = Some("App:Firefox".into());
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        assert!(text.contains("p pause"));
    }

    #[test]
    fn small_icons_keep_every_tile_inside_its_own_columns() {
        let mut state = State::new(Theme::default());
        state.icon_size = (8, 4);
        state.set_apps(state::tests::sample_apps());
        let text = render_to_text(60, 16, &mut state, &mut test_store());
        insta::assert_snapshot!(text);
        // Baldosas de 10 columnas: 6 por fila en 60 columnas.
        assert_eq!(state.layout.columns, 6);
        assert!(
            text.contains("24% 5.3G") && text.contains("120% 12M"),
            "{text}"
        );
    }

    #[test]
    fn terminals_below_80x24_show_a_message_instead_of_the_ui() {
        let mut state = detail_state();
        state.enforce_min_size = true;
        let mut store = test_store();
        for (w, h) in [(79, 24), (80, 23), (40, 10)] {
            let text = render_to_text(w, h, &mut state, &mut store);
            assert!(text.contains("Terminal too small"), "{w}x{h}\n{text}");
            assert!(text.contains(&format!("{w}x{h} — minimum 80x24")), "{text}");
            assert!(!text.contains("PROCESS"), "no se dibuja la interfaz debajo");
        }
        // Justo en el mínimo, y al volver a agrandar, la interfaz normal.
        let text = render_to_text(80, 24, &mut state, &mut store);
        assert!(!text.contains("too small") && text.contains("PROCESS"));
        // Tamaños absurdos tampoco rompen el aviso.
        for (w, h) in [(0, 0), (1, 1), (5, 2), (200, 3)] {
            let _ = render_to_text(w, h, &mut state, &mut store);
        }
    }

    #[test]
    fn resizing_between_frames_keeps_the_selection_visible() {
        let mut state = State::new(Theme::default());
        let mut apps = Vec::new();
        for i in 0..40 {
            apps.push(state::tests::app(
                &format!("app{i:02}"),
                crate::apps::AppKind::App,
                0.0,
                1,
            ));
        }
        state.set_apps(apps);
        state.select_index(39);
        let mut store = test_store();
        // Grande → chica → ancha y baja → grande: mismo estado, tamaños distintos.
        for (w, h) in [(160, 50), (80, 24), (200, 12), (30, 9), (120, 40)] {
            let text = render_to_text(w, h, &mut state, &mut store);
            assert!(
                text.contains("app39"),
                "{w}x{h}: la selección debe verse\n{text}"
            );
            let view = h.saturating_sub(2);
            assert!(
                state.scroll
                    <= state
                        .layout
                        .height
                        .saturating_sub(view.min(state.layout.height))
            );
            assert_eq!(state.layout.area.width, w);
        }
        // Con un diálogo abierto también.
        state.mode = Mode::SignalMenu(2);
        for (w, h) in [(120, 40), (40, 8), (10, 3)] {
            let _ = render_to_text(w, h, &mut state, &mut store);
        }
    }

    #[test]
    fn bars_show_totals_and_shortcuts_and_nd_when_unknown() {
        let mut state = State::new(Theme::default());
        state.set_apps(state::tests::sample_apps());
        let text = render_to_text(100, 20, &mut state, &mut test_store());
        let top = text.lines().next().unwrap();
        assert!(
            top.starts_with("apptop  CPU n/a  RAM n/a  Swap n/a"),
            "{top}"
        );
        assert!(top.ends_with("3 apps · 2 services"), "{top}");
        assert!(text.lines().last().unwrap().contains("q quit"));

        state.system.cpu_pct = Some(250.0);
        state.system.mem = Some((8 << 30, 2 << 30));
        state.system.swap = Some((0, 0));
        let text = render_to_text(100, 20, &mut state, &mut test_store());
        let top = text.lines().next().unwrap();
        assert!(top.contains("CPU ━━━━━━━━━━ 250%"), "{top}");
        assert!(
            top.contains("2.0G/8.0G") && top.contains("no swap"),
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
            gpu: crate::collect::GpuReading::Stats(crate::collect::GpuStats {
                busy_pct: 18.0,
                engine: "gfx".into(),
                memory: 512 << 20,
                dedicated: true,
                driver: "amdgpu".into(),
                clients: 2,
            }),
            net: crate::collect::NetReading::Rate(crate::collect::NetRate {
                down_bps: 840_000.0,
                up_bps: 52_000.0,
                sockets: 9,
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
        assert!(text.contains("T stopped"), "{text}");
        // Con poco alto el árbol se desplaza para mostrar la fila elegida.
        state.move_proc_cursor(1);
        let text = render_to_text(80, 18, &mut state, &mut test_store());
        assert!(text.contains("▶   4222"), "{text}");
        assert_eq!(state.tree.scroll, 1);
        assert_eq!(state.tree.len, 3);
    }

    #[test]
    fn grid_is_drawn_again_with_the_same_selection_after_leaving_detail() {
        let mut state = State::new(Theme::default());
        state.set_apps(state::tests::sample_apps());
        state.selected = Some("Service:pipewire".into());
        let mut store = test_store();
        let grid_before = render_to_text(40, 16, &mut state, &mut store);
        let scroll = state.scroll;
        state.enter_detail();
        let detail = render_to_text(40, 16, &mut state, &mut store);
        assert_ne!(detail, grid_before);
        state.back_to_grid();
        assert_eq!(render_to_text(40, 16, &mut state, &mut store), grid_before);
        assert_eq!(state.scroll, scroll);
    }

    #[test]
    fn confirm_dialog_snapshot() {
        let mut state = detail_state();
        state.lineage = vec![4222];
        state.ask_signal(crate::actions::Signal::SIGTERM);
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        insta::assert_snapshot!(text);
        assert!(text.contains("Send SIGTERM to Firefox?"));
        assert!(text.contains("2 processes affected"));
        assert!(text.contains("[y] yes     [n] no"));
    }

    #[test]
    fn elevate_dialog_snapshot_shows_the_exact_command() {
        use crate::actions::{Elevated, Signal, Target};
        let mut state = detail_state();
        let denied = vec![
            Target {
                pid: 4100,
                starttime: 1,
            },
            Target {
                pid: 4130,
                starttime: 1,
            },
        ];
        // Sin sudo disponible no se ofrece nada.
        state.offer_elevation(Elevated::Signal(Signal::SIGKILL), denied.clone(), "Firefox");
        assert_eq!(state.mode, Mode::Normal);
        state.can_elevate = true;
        state.offer_elevation(Elevated::Signal(Signal::SIGKILL), denied, "Firefox");
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        insta::assert_snapshot!(text);
        assert!(text.contains("No permission for 2 processes of Firefox."));
        assert!(text.contains("sudo kill -s KILL -- 4100 4130"));
        // Solo 's'/'y' aceptan; cualquier otra tecla cancela.
        let request = match &state.mode {
            Mode::Elevate(request) => request.clone(),
            other => panic!("{other:?}"),
        };
        state.answer_elevate(false);
        assert!(state.pending_elevate.is_none() && state.mode == Mode::Normal);
        state.mode = Mode::Elevate(request.clone());
        state.answer_elevate(true);
        assert_eq!(state.pending_elevate.take(), Some(request));
    }

    #[test]
    fn nice_dialog_snapshot_warns_when_raising_priority() {
        let mut state = detail_state();
        state.ask_nice();
        state.nice_adjust(-4);
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        insta::assert_snapshot!(text);
        assert!(text.contains("New:  -4"));
        assert!(text.contains("requires root or CAP_SYS_NICE"));
        state.nice_adjust(9);
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        assert!(text.contains("New:   5") && !text.contains("requires root"));
        // Al bajar la prioridad se avisa que no hay vuelta atrás sin root.
        assert!(
            text.contains("Without root you cannot go back to a lower value"),
            "{text}"
        );
        assert!(
            text.contains("restart the app to regain its priority"),
            "{text}"
        );
        // Sin cambio respecto del valor actual: ninguna advertencia.
        state.nice_adjust(-5);
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        assert!(text.contains("New:   0") && text.contains("Only matters when the CPU"));
        assert!(!text.contains("Without root") && !text.contains("requires root"));
    }

    #[test]
    fn signal_menu_snapshot_for_a_single_process() {
        let mut state = detail_state();
        state.move_proc_cursor(2);
        state.open_signal_menu();
        state.signal_menu_move(3);
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        insta::assert_snapshot!(text);
        assert!(text.contains("firefox (pid 4130) · 1 process affected"));
        assert!(text.contains("▶ SIGHUP"));
        assert!(text.contains("  SIGUSR2"));
    }

    #[test]
    fn messages_replace_the_shortcut_bar_until_they_expire() {
        let mut state = detail_state();
        state.notify("SIGTERM enviada a 3 procesos de Firefox", false);
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        assert_eq!(
            text.lines().last(),
            Some("SIGTERM enviada a 3 procesos de Firefox")
        );
        state.clock += 60.0;
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        assert!(text.lines().last().unwrap().contains("Esc back"));
    }

    #[test]
    fn detail_shows_nd_for_unknown_values_and_survives_app_exit() {
        let mut state = detail_state();
        state.detail = None;
        state.system.uptime = None;
        state.apps[1].procs.iter_mut().for_each(|p| p.uid = None);
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        assert!(text.contains("PSS n/a"), "{text}");
        assert!(
            text.contains("User       n/a") && text.contains("Uptime     n/a"),
            "{text}"
        );

        // La app muere mientras se mira su detalle.
        state.apps.retain(|a| a.name != "Firefox");
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        assert!(text.contains("no longer running"));
        for (w, h) in [(1, 1), (20, 4), (30, 30)] {
            let _ = render_to_text(w, h, &mut detail_state(), &mut test_store());
        }
    }

    #[test]
    fn empty_state_shows_a_message_and_tiny_sizes_do_not_panic() {
        let mut state = State::new(Theme::default());
        let text = render_to_text(60, 10, &mut state, &mut test_store());
        assert!(text.contains("No applications"));
        state.set_apps(state::tests::sample_apps());
        for (w, h) in [(1, 1), (3, 2), (17, 5), (200, 3)] {
            let _ = render_to_text(w, h, &mut state, &mut test_store());
        }
    }

    /// Vuelca los estilos de un cuadro: por fila, tramos
    /// `color de texto / fondo / modificadores × ancho`.
    fn render_styles(width: u16, height: u16, state: &mut State) -> String {
        let mut store = test_store();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, state, &mut store)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..height {
            let mut runs: Vec<(String, u16)> = Vec::new();
            for x in 0..width {
                let c = &buffer[(x, y)];
                let key = format!("{:?}/{:?}/{:?}", c.fg, c.bg, c.modifier);
                match runs.last_mut() {
                    Some((last, n)) if *last == key => *n += 1,
                    _ => runs.push((key, 1)),
                }
            }
            let line: Vec<String> = runs.iter().map(|(k, n)| format!("{k}×{n}")).collect();
            out.push_str(&line.join(" "));
            out.push('\n');
        }
        out
    }

    /// Con el tema por defecto, los colores de cada celda son los de
    /// siempre: estos snapshots se tomaron antes de pasar a `tui-themes`.
    #[test]
    fn default_theme_styles_are_unchanged() {
        let mut state = State::new(Theme::default());
        state.set_apps(state::tests::sample_apps());
        state.selected = Some("App:Firefox".into());
        insta::assert_snapshot!("styles_grid", render_styles(80, 24, &mut state));
        insta::assert_snapshot!("styles_detail", render_styles(80, 24, &mut detail_state()));
        let mut dialog = detail_state();
        dialog.mode = Mode::SignalMenu(0);
        insta::assert_snapshot!("styles_dialog", render_styles(80, 24, &mut dialog));
    }

    /// Un tema con fondo propio lo pinta en todo el cuadro, también bajo
    /// los diálogos.
    #[test]
    fn a_theme_with_background_paints_every_cell() {
        let base = tui_themes::Catalog::builtin().get_or_plain("ramas-light");
        let bg = base.bg;
        assert_ne!(bg, ratatui::style::Color::Reset);
        let theme = Theme::resolve(&base, None, tui_themes::ColorDepth::TrueColor);
        for dialog in [false, true] {
            let mut state = detail_state();
            state.theme = theme.clone();
            if dialog {
                state.mode = Mode::SignalMenu(0);
            }
            let mut store = test_store();
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
            terminal.draw(|f| draw(f, &mut state, &mut store)).unwrap();
            let buffer = terminal.backend().buffer();
            // Cada celda lleva el fondo del tema o el de la selección.
            let allowed = [bg, base.selection_bg];
            assert!(buffer.content.iter().all(|c| allowed.contains(&c.bg)));
            assert!(buffer.content.iter().any(|c| c.bg == bg));
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

    /// El español no es el idioma por defecto: estos snapshots evitan que se
    /// rompa sin que nadie lo note.
    #[test]
    fn spanish_grid_snapshot() {
        let mut state = State::new(Theme::default());
        state.lang = crate::i18n::Lang::Es;
        state.set_apps(state::tests::sample_apps());
        let text = render_to_text(80, 24, &mut state, &mut test_store());
        insta::assert_snapshot!(text);
        assert!(text.contains("── Aplicaciones (3)") && text.contains("── Servicios (2)"));
        assert!(text.contains("n/d") && text.contains("q salir"));
    }

    #[test]
    fn spanish_detail_and_dialogs_snapshot() {
        let mut state = detail_state();
        state.lang = crate::i18n::Lang::Es;
        let mut store = test_store();
        let text = render_to_text(80, 24, &mut state, &mut store);
        insta::assert_snapshot!("spanish_detail", text);
        for expected in [
            "Usuario    ana",
            "Activa     2h 03m",
            "Red aprox.",
            "PROCESO",
            "T pausado",
        ] {
            assert!(text.contains(expected), "{expected}\n{text}");
        }

        state.ask_signal(crate::actions::Signal::SIGTERM);
        let text = render_to_text(80, 24, &mut state, &mut store);
        insta::assert_snapshot!("spanish_confirm", text);
        assert!(text.contains("¿Enviar SIGTERM a Firefox?") && text.contains("[s] sí     [n] no"));
        state.answer_confirm(false);
        assert_eq!(state.current_message().unwrap().text, "Cancelado");

        state.ask_nice();
        state.nice_adjust(3);
        let text = render_to_text(80, 24, &mut state, &mut store);
        assert!(text.contains("Nueva:   3") && text.contains("hay que reiniciar la app"));
        state.nice_finish(false);

        state.open_signal_menu();
        let text = render_to_text(80, 24, &mut state, &mut store);
        assert!(text.contains("Enviar señal") && text.contains("pedir que termine"));
    }

    /// Mismo estado en los dos idiomas: las columnas del detalle quedan en
    /// la misma posición, cambian solo las palabras.
    #[test]
    fn both_languages_share_the_same_layout() {
        let mut store = test_store();
        let mut column_of = |lang, needle: &str| -> Option<usize> {
            let mut state = detail_state();
            state.lang = lang;
            let text = render_to_text(100, 30, &mut state, &mut store);
            text.lines()
                .find_map(|l| l.find(needle).map(|i| l[..i].chars().count()))
        };
        use crate::i18n::Lang::{En, Es};
        assert_eq!(column_of(En, "ana"), column_of(Es, "ana"));
        assert_eq!(column_of(En, "2h 03m"), column_of(Es, "2h 03m"));
        assert_eq!(column_of(En, "CPU%"), column_of(Es, "CPU%"));
        assert_eq!(column_of(En, "S asleep"), column_of(Es, "S duerme"));
    }
}
