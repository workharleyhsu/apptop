//! Barra superior (totales del sistema) e inferior (atajos).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::format::human_bytes;
use super::state::{Mode, State, View};

/// Ancho de cada medidor, en celdas.
const METER_WIDTH: usize = 10;

/// Escribe segmentos de texto con estilo uno tras otro, recortando al ancho.
/// Devuelve la columna donde quedó el cursor.
pub fn write_spans(buf: &mut Buffer, area: Rect, spans: &[(String, Style)]) -> u16 {
    let mut x = area.x;
    let end = area.x + area.width;
    for (text, style) in spans {
        if x >= end {
            break;
        }
        let (next, _) = buf.set_stringn(x, area.y, text, (end - x) as usize, *style);
        x = next;
    }
    x
}

/// Medidor de `METER_WIDTH` celdas para un valor entre 0 y 1.
fn meter(fraction: f32) -> (String, String) {
    let filled = (fraction.clamp(0.0, 1.0) * METER_WIDTH as f32).round() as usize;
    ("━".repeat(filled), "─".repeat(METER_WIDTH - filled))
}

/// Barra superior: nombre, CPU, RAM, swap y cantidad de apps.
pub fn draw_top(buf: &mut Buffer, area: Rect, state: &State) {
    if area.height == 0 {
        return;
    }
    let theme = state.theme;
    let label = theme.header();
    let dim = theme.dim();
    let mut spans: Vec<(String, Style)> = vec![(
        "apptop".to_string(),
        Style::default()
            .fg(theme.accent())
            .add_modifier(Modifier::BOLD),
    )];

    let mut gauge = |name: &str, fraction: Option<f32>, value: String| {
        spans.push((format!("  {name} "), label));
        match fraction {
            Some(fraction) => {
                let (on, off) = meter(fraction);
                spans.push((on, Style::default().fg(theme.load(fraction * 100.0))));
                spans.push((off, dim));
                spans.push((format!(" {value}"), theme.text()));
            }
            None => spans.push(("n/d".to_string(), dim)),
        }
    };

    let system = &state.system;
    gauge(
        "CPU",
        system.cpu_pct.map(|p| p / 100.0),
        format!("{:.0}%", system.cpu_pct.unwrap_or(0.0)),
    );
    let ratio = |pair: Option<(u64, u64)>| {
        pair.filter(|(total, _)| *total > 0)
            .map(|(total, used)| used as f32 / total as f32)
    };
    let amounts = |pair: Option<(u64, u64)>| {
        pair.map(|(total, used)| format!("{}/{}", human_bytes(used), human_bytes(total)))
            .unwrap_or_default()
    };
    gauge("RAM", ratio(system.mem), amounts(system.mem));
    // Sin swap configurada no es un error: se muestra en cero.
    match system.swap {
        Some((0, _)) => gauge("Swap", Some(0.0), "sin swap".to_string()),
        swap => gauge("Swap", ratio(swap), amounts(swap)),
    }

    let end = write_spans(buf, area, &spans);
    // A la derecha, si queda lugar: cuántas apps y servicios hay.
    let apps = state
        .apps
        .iter()
        .filter(|a| a.kind == crate::apps::AppKind::App)
        .count();
    let summary = format!(
        "⟳ {} · {apps} apps · {} servicios",
        super::format::human_interval(state.refresh),
        state.apps.len() - apps
    );
    let width = summary.chars().count() as u16;
    if area.x + area.width >= end + width + 2 {
        buf.set_string(area.x + area.width - width, area.y, summary, dim);
    }
}

/// Un atajo de la barra inferior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub key: String,
    pub description: String,
    /// Los de menor prioridad se descartan primero si no entran.
    pub priority: u8,
}

impl Hint {
    fn new(key: &str, description: impl Into<String>, priority: u8) -> Self {
        Self {
            key: key.to_string(),
            description: description.into(),
            priority,
        }
    }

    fn width(&self) -> usize {
        self.key.chars().count() + 1 + self.description.chars().count()
    }
}

/// Atajos disponibles según la vista y el modo.
pub fn hints(state: &State) -> Vec<Hint> {
    match state.mode {
        Mode::Filter => {
            return vec![
                Hint::new("Enter", "aplicar", 9),
                Hint::new("Esc", "borrar filtro", 8),
                Hint::new("←↑↓→", "mover", 1),
            ]
        }
        Mode::Confirm(_) => {
            return vec![
                Hint::new("s", "confirmar", 9),
                Hint::new("n/Esc", "cancelar", 9),
            ]
        }
        Mode::Normal => {}
    }
    match state.view {
        View::Grid => {
            let mut hints = vec![
                Hint::new("←↑↓→/hjkl", "mover", 1),
                Hint::new("Enter", "abrir", 6),
                Hint::new("/", "filtrar", 5),
                Hint::new("s", format!("orden: {}", state.sort.label()), 4),
                Hint::new("Tab", format!("ver: {}", state.sections.label()), 3),
                Hint::new("x", "cerrar", 7),
                Hint::new("K", "matar", 6),
                Hint::new("p", pause_label(state), 5),
                Hint::new("+/-", "refresco", 2),
            ];
            if !state.filter.is_empty() {
                hints.push(Hint::new("Esc", "quitar filtro", 8));
            }
            hints.push(Hint::new("q", "salir", 9));
            hints
        }
        View::Detail => vec![
            Hint::new("↑↓/jk", "elegir proceso", 3),
            Hint::new("x", "cerrar", 6),
            Hint::new("K", "matar", 5),
            Hint::new("p", pause_label(state), 4),
            Hint::new("Esc", "volver", 8),
            Hint::new("q", "salir", 9),
        ],
    }
}

/// Qué hará la tecla de pausa ahora mismo.
fn pause_label(state: &State) -> &'static str {
    if state.scope_is_paused() {
        "reanudar"
    } else {
        "pausar"
    }
}

/// Separación entre atajos.
const HINT_GAP: usize = 2;

/// Quita los atajos de menor prioridad hasta que el resto entre en `width`.
pub fn fit_hints(mut hints: Vec<Hint>, width: usize) -> Vec<Hint> {
    let total = |hints: &[Hint]| {
        hints.iter().map(Hint::width).sum::<usize>() + HINT_GAP * hints.len().saturating_sub(1)
    };
    while hints.len() > 1 && total(&hints) > width {
        let lowest = hints
            .iter()
            .enumerate()
            .min_by_key(|(_, h)| h.priority)
            .map(|(i, _)| i)
            .unwrap_or(0);
        hints.remove(lowest);
    }
    hints
}

/// Barra inferior de atajos.
pub fn draw_bottom(buf: &mut Buffer, area: Rect, state: &State) {
    if area.height == 0 {
        return;
    }
    let theme = state.theme;
    // Un aviso vigente ocupa el lugar de los atajos.
    if let Some(message) = state.current_message() {
        let color = if message.is_error {
            theme.load(100.0)
        } else {
            theme.load(0.0)
        };
        let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
        buf.set_stringn(area.x, area.y, &message.text, area.width as usize, style);
        return;
    }
    let key_style = Style::default()
        .fg(theme.accent())
        .add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    for (i, hint) in fit_hints(hints(state), area.width as usize)
        .into_iter()
        .enumerate()
    {
        if i > 0 {
            spans.push((" ".repeat(HINT_GAP), theme.dim()));
        }
        spans.push((hint.key, key_style));
        spans.push((format!(" {}", hint.description), theme.dim()));
    }
    write_spans(buf, area, &spans);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_fills_proportionally_and_clamps() {
        assert_eq!(meter(0.0), (String::new(), "─".repeat(10)));
        assert_eq!(meter(0.5).0.chars().count(), 5);
        assert_eq!(meter(1.7), ("━".repeat(10), String::new()));
        assert_eq!(meter(-1.0).0, "");
    }

    #[test]
    fn hints_follow_view_mode_and_state() {
        let mut s = State::new(super::super::Theme::default());
        let keys = |s: &State| -> Vec<String> { hints(s).into_iter().map(|h| h.key).collect() };
        assert!(keys(&s).contains(&"Enter".to_string()));
        assert!(!keys(&s).contains(&"Esc".to_string()));
        assert!(hints(&s).iter().any(|h| h.description == "orden: nombre"));
        s.filter = "x".into();
        assert!(keys(&s).contains(&"Esc".to_string()));
        s.mode = Mode::Filter;
        assert_eq!(keys(&s)[0], "Enter");
        s.mode = Mode::Normal;
        s.view = View::Detail;
        assert_eq!(keys(&s), vec!["↑↓/jk", "x", "K", "p", "Esc", "q"]);
    }

    #[test]
    fn narrow_bars_drop_the_least_important_hints_first() {
        let mut s = State::new(super::super::Theme::default());
        s.filter = "x".into();
        let all = hints(&s);
        fn fitted(s: &State, width: usize) -> Vec<String> {
            fit_hints(hints(s), width)
                .into_iter()
                .map(|h| h.key)
                .collect()
        }
        let keys = |width: usize| fitted(&s, width);
        assert_eq!(keys(200).len(), all.len());
        // Primero se va "mover"; salir y quitar filtro son los últimos.
        assert!(!keys(80).contains(&"←↑↓→/hjkl".to_string()));
        assert!(keys(80).contains(&"q".to_string()));
        assert_eq!(keys(26), vec!["Esc", "q"]);
        s.mode = Mode::Confirm(crate::ui::state::ActionRequest {
            signal: crate::actions::Signal::SIGTERM,
            targets: Vec::new(),
            label: String::new(),
            single_process: false,
            excluded: 0,
        });
        assert_eq!(fitted(&s, 200), vec!["s", "n/Esc"]);
        assert_eq!(fitted(&s, 3).len(), 1, "siempre queda al menos uno");
    }
}
