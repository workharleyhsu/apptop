//! Árbol de procesos de una app.

use std::collections::{HashMap, HashSet};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::apps::App;
use crate::collect::ProcInfo;
use crate::i18n::Lang;

use super::bars::write_spans;
use super::format::human_bytes;
use super::state::State;

/// Una fila del árbol: un proceso y el dibujo de ramas que lo precede.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    /// Índice dentro de `app.procs`.
    pub index: usize,
    pub pid: i32,
    /// Ramas (`│  ├─ `) que van antes del nombre.
    pub prefix: String,
}

/// Ordena los procesos de la app como árbol (padres antes que hijos,
/// hermanos por pid). Son raíces los procesos cuyo padre no es de la app.
pub fn tree_rows(app: &App) -> Vec<TreeRow> {
    let by_pid: HashMap<i32, usize> = app
        .procs
        .iter()
        .enumerate()
        .map(|(i, p)| (p.pid, i))
        .collect();
    let mut children: HashMap<i32, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (i, p) in app.procs.iter().enumerate() {
        if p.ppid != p.pid && by_pid.contains_key(&p.ppid) {
            children.entry(p.ppid).or_default().push(i);
        } else {
            roots.push(i);
        }
    }

    let mut rows = Vec::with_capacity(app.procs.len());
    let mut seen: HashSet<usize> = HashSet::new();
    // Pila de (índice, prefijo heredado, es último hermano, es raíz).
    let mut stack: Vec<(usize, String, bool, bool)> = roots
        .iter()
        .rev()
        .map(|i| (*i, String::new(), true, true))
        .collect();
    while let Some((index, inherited, last, root)) = stack.pop() {
        if !seen.insert(index) {
            continue;
        }
        let proc = &app.procs[index];
        let branch = match (root, last) {
            (true, _) => "",
            (false, true) => "└─ ",
            (false, false) => "├─ ",
        };
        rows.push(TreeRow {
            index,
            pid: proc.pid,
            prefix: format!("{inherited}{branch}"),
        });
        let below = match (root, last) {
            (true, _) => inherited,
            (false, true) => format!("{inherited}   "),
            (false, false) => format!("{inherited}│  "),
        };
        if let Some(kids) = children.get(&proc.pid) {
            for (n, kid) in kids.iter().enumerate().rev() {
                stack.push((*kid, below.clone(), n == kids.len() - 1, false));
            }
        }
    }
    // Si /proc cambió a mitad de la lectura puede quedar un ciclo sin raíz:
    // esos procesos se listan igual, sin ramas.
    for (index, proc) in app.procs.iter().enumerate() {
        if !seen.contains(&index) {
            rows.push(TreeRow {
                index,
                pid: proc.pid,
                prefix: String::new(),
            });
        }
    }
    rows
}

/// Descripción corta de la letra de estado del kernel.
pub fn state_label(state: char, lang: Lang) -> &'static str {
    let t = lang.t();
    match state {
        'R' => t.state_running,
        'S' => t.state_sleeping,
        'D' => t.state_io,
        'I' => t.state_idle,
        'T' => t.state_stopped,
        't' => t.state_traced,
        'Z' => t.state_zombie,
        'X' | 'x' => t.state_dead,
        _ => t.state_unknown,
    }
}

/// Argumentos del proceso (sin el ejecutable), para mostrar junto al nombre.
fn arguments(proc: &ProcInfo) -> String {
    proc.cmdline
        .iter()
        .skip(1)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

const PID_WIDTH: usize = 8;
/// Columnas fijas a la derecha: CPU%, RAM, hilos y estado.
const RIGHT_WIDTH: usize = 7 + 8 + 6 + 11;

/// Dónde quedó dibujado el árbol (para el mouse) y cuántas filas tiene.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TreeView {
    /// Área de las filas (sin el encabezado).
    pub area: Rect,
    pub scroll: usize,
    pub len: usize,
}

impl TreeView {
    /// Fila del árbol bajo la celda de pantalla, si hay alguna.
    pub fn hit(&self, column: u16, row: u16) -> Option<usize> {
        let a = self.area;
        if column < a.x || column >= a.x + a.width || row < a.y || row >= a.y + a.height {
            return None;
        }
        let index = self.scroll + (row - a.y) as usize;
        (index < self.len).then_some(index)
    }
}

/// Ajusta el scroll para que la fila `cursor` quede dentro de `height` filas.
pub fn scroll_for(cursor: Option<usize>, scroll: usize, len: usize, height: usize) -> usize {
    let max = len.saturating_sub(height);
    let scroll = match cursor {
        Some(row) if row < scroll => row,
        Some(row) if height > 0 && row >= scroll + height => row + 1 - height,
        _ => scroll,
    };
    scroll.min(max)
}

/// Dibuja el árbol de procesos de `app` en `area`.
pub fn draw(buf: &mut Buffer, area: Rect, app: &App, state: &mut State) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let theme = state.theme;
    let lang = state.lang;
    let t = lang.t();
    let width = area.width as usize;
    let name_width = width.saturating_sub(PID_WIDTH + 2 + RIGHT_WIDTH).max(8);

    let header = format!(
        "{:>PID_WIDTH$}  {:<name_width$}{:>7}{:>8}{:>6}  {:<9}",
        "PID", t.col_process, "CPU%", "RAM", t.col_threads, t.col_state
    );
    buf.set_stringn(area.x, area.y, header, width, theme.header());

    let rows = tree_rows(app);
    let body = Rect::new(area.x, area.y + 1, area.width, area.height - 1);
    let cursor = state
        .proc_selected
        .and_then(|pid| rows.iter().position(|r| r.pid == pid));
    let scroll = scroll_for(cursor, state.tree.scroll, rows.len(), body.height as usize);
    state.tree = TreeView {
        area: body,
        scroll,
        len: rows.len(),
    };

    for (line, row) in rows
        .iter()
        .skip(scroll)
        .take(body.height as usize)
        .enumerate()
    {
        let proc = &app.procs[row.index];
        let selected = state.proc_selected == Some(proc.pid);
        let base = if selected {
            theme.selected()
        } else {
            theme.text()
        };
        let marker = if selected { "▶" } else { " " };
        let pid = format!("{marker}{:>w$}  ", proc.pid, w = PID_WIDTH - 1);

        // Nombre con ramas y, si sobra lugar, los argumentos atenuados.
        let used = row.prefix.chars().count() + proc.name.chars().count();
        let args = arguments(proc);
        let room = name_width.saturating_sub(used + 1);
        let shown_args: String = if args.is_empty() || room < 4 {
            String::new()
        } else if args.chars().count() > room {
            let cut: String = args.chars().take(room - 1).collect();
            format!(" {cut}…")
        } else {
            format!(" {args}")
        };
        let pad = name_width.saturating_sub(used + shown_args.chars().count());
        let name: String = proc
            .name
            .chars()
            .take(name_width.saturating_sub(row.prefix.chars().count()))
            .collect();

        let state_style = match proc.state {
            'T' | 't' => Style::default().fg(theme.load(50.0)),
            'Z' | 'X' | 'x' => Style::default().fg(theme.load(100.0)),
            _ => theme.dim(),
        };
        let spans = [
            (pid, base),
            (row.prefix.clone(), theme.dim()),
            (name, base.add_modifier(Modifier::BOLD)),
            (shown_args, theme.dim()),
            (" ".repeat(pad), theme.text()),
            (
                format!("{:>7.1}", proc.cpu_pct),
                Style::default().fg(theme.load(proc.cpu_pct)),
            ),
            (format!("{:>8}", human_bytes(proc.rss)), base),
            (format!("{:>6}", proc.threads), base),
            (
                format!("  {:<9}", state_label(proc.state, lang)),
                state_style,
            ),
        ];
        write_spans(
            buf,
            Rect::new(body.x, body.y + line as u16, body.width, 1),
            &spans,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::group::tests::proc;
    use crate::apps::AppKind;
    use crate::ui::state::tests::app;

    fn with_procs(pairs: &[(i32, i32)]) -> App {
        let mut a = app("x", AppKind::App, 0.0, 1);
        a.procs = pairs
            .iter()
            .map(|(pid, ppid)| proc(*pid, *ppid, "/bin/p", "/"))
            .collect();
        a
    }

    fn shape(app: &App) -> Vec<String> {
        tree_rows(app)
            .iter()
            .map(|r| format!("{}{}", r.prefix, r.pid))
            .collect()
    }

    #[test]
    fn builds_a_tree_with_branches() {
        let a = with_procs(&[
            (10, 1),
            (11, 10),
            (12, 10),
            (13, 11),
            (14, 11),
            (20, 1),
            (21, 20),
        ]);
        assert_eq!(
            shape(&a),
            vec![
                "10",
                "├─ 11",
                "│  ├─ 13",
                "│  └─ 14",
                "└─ 12",
                "20",
                "└─ 21"
            ]
        );
    }

    #[test]
    fn orphans_are_roots_and_cycles_do_not_loop() {
        // 30 cuelga de un padre que no es de la app.
        assert_eq!(
            shape(&with_procs(&[(30, 999), (31, 30)])),
            vec!["30", "└─ 31"]
        );
        // Ciclo 5 ↔ 6 y proceso que es su propio padre.
        let rows = tree_rows(&with_procs(&[(5, 6), (6, 5), (7, 7)]));
        let mut pids: Vec<i32> = rows.iter().map(|r| r.pid).collect();
        pids.sort_unstable();
        assert_eq!(pids, vec![5, 6, 7]);
        assert!(tree_rows(&with_procs(&[])).is_empty());
    }

    #[test]
    fn scroll_follows_the_cursor() {
        assert_eq!(scroll_for(None, 0, 20, 5), 0);
        assert_eq!(scroll_for(Some(7), 0, 20, 5), 3);
        assert_eq!(scroll_for(Some(2), 6, 20, 5), 2);
        assert_eq!(scroll_for(Some(4), 2, 20, 5), 2, "ya visible: no se mueve");
        assert_eq!(scroll_for(None, 50, 20, 5), 15, "se recorta al final");
        assert_eq!(scroll_for(Some(3), 0, 4, 0), 0);
    }

    #[test]
    fn hit_maps_screen_rows_to_tree_rows() {
        let view = TreeView {
            area: Rect::new(0, 14, 80, 5),
            scroll: 3,
            len: 6,
        };
        assert_eq!(view.hit(10, 14), Some(3));
        assert_eq!(view.hit(10, 16), Some(5));
        assert_eq!(view.hit(10, 17), None, "más allá de la última fila");
        assert_eq!(view.hit(10, 13), None);
    }

    #[test]
    fn labels_states() {
        assert_eq!(state_label('T', Lang::En), "T stopped");
        assert_eq!(state_label('T', Lang::Es), "T pausado");
        assert_eq!(state_label('Z', Lang::En), "Z zombie");
        assert_eq!(state_label('q', Lang::Es), "? n/d");
    }
}
