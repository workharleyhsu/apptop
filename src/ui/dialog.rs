//! Diálogos modales dibujados sobre la vista actual.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::state::{ActionRequest, NiceRequest, State};
use super::theme::Theme;
use crate::actions::SIGNAL_MENU;

/// Dibuja un cuadro centrado con borde, título y líneas de texto. El
/// interior se limpia con espacios (sin color de fondo).
pub fn draw_box(
    buf: &mut Buffer,
    area: Rect,
    theme: Theme,
    title: &str,
    lines: &[(String, Style)],
) {
    let content = lines.iter().map(|l| l.0.chars().count()).max().unwrap_or(0);
    let inner = content.max(title.chars().count() + 2);
    let width = ((inner + 4) as u16).min(area.width);
    let height = ((lines.len() + 2) as u16).min(area.height);
    if width < 4 || height < 2 {
        return;
    }
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    let border = Style::default().fg(theme.accent());
    let span = (width - 2) as usize;

    let label = format!(" {title} ");
    let label: String = label.chars().take(span.saturating_sub(1)).collect();
    let top = format!(
        "╭─{label}{}╮",
        "─".repeat(span.saturating_sub(1 + label.chars().count()))
    );
    buf.set_stringn(x, y, top, width as usize, border);
    buf.set_stringn(x + 2, y, &label, span, border.add_modifier(Modifier::BOLD));
    for row in 1..height - 1 {
        buf.set_stringn(
            x,
            y + row,
            format!("│{}│", " ".repeat(span)),
            width as usize,
            border,
        );
        if let Some((text, style)) = lines.get(row as usize - 1) {
            buf.set_stringn(x + 2, y + row, text, span.saturating_sub(2), *style);
        }
    }
    let bottom = format!("╰{}╯", "─".repeat(span));
    buf.set_stringn(x, y + height - 1, bottom, width as usize, border);
}

/// Diálogo de confirmación de una señal: muestra el nombre y cuántos
/// procesos se ven afectados.
pub fn draw_confirm(buf: &mut Buffer, area: Rect, state: &State, request: &ActionRequest) {
    let theme = state.theme;
    let lang = state.lang;
    let mut lines = vec![
        (
            lang.send_question(request.signal, &request.label),
            theme.text().add_modifier(Modifier::BOLD),
        ),
        (lang.affected(request.targets.len()), theme.text()),
    ];
    if request.excluded > 0 {
        lines.push((lang.excluded(request.excluded), theme.dim()));
    }
    lines.push((String::new(), theme.text()));
    lines.push((lang.t().yes_no.to_string(), theme.selected()));
    draw_box(buf, area, theme, request.title(lang), &lines);
}

/// Menú de señales: una por fila, con el cursor marcado.
pub fn draw_signal_menu(buf: &mut Buffer, area: Rect, state: &State, cursor: usize) {
    let theme = state.theme;
    let lang = state.lang;
    let target = match state.action_scope() {
        Some((targets, label, _)) => format!("{label} · {}", lang.affected(targets.len())),
        None => String::new(),
    };
    let mut lines = vec![
        (target, theme.text().add_modifier(Modifier::BOLD)),
        (String::new(), theme.text()),
    ];
    for (row, signal) in SIGNAL_MENU.iter().enumerate() {
        let (marker, style) = if row == cursor {
            ("▶", theme.selected())
        } else {
            (" ", theme.text())
        };
        let description = lang.signal_description(*signal);
        lines.push((
            format!("{marker} {:<8} {description}", signal.as_str()),
            style,
        ));
    }
    lines.push((String::new(), theme.text()));
    lines.push((lang.t().menu_keys.to_string(), theme.dim()));
    draw_box(buf, area, theme, lang.t().title_send_signal, &lines);
}

/// Selector de prioridad: valor actual, valor nuevo y una regla de -20 a 19.
pub fn draw_nice(buf: &mut Buffer, area: Rect, state: &State, request: &NiceRequest) {
    let theme = state.theme;
    let lang = state.lang;
    let t = lang.t();
    // Regla de 40 posiciones (-20..=19) con una marca en el valor elegido.
    let position = (request.value + 20).clamp(0, 39) as usize;
    let ruler: String = (0..40)
        .map(|i| if i == position { '●' } else { '─' })
        .collect();
    let mut lines = vec![
        (
            format!(
                "{} · {}",
                request.label,
                lang.affected(request.targets.len())
            ),
            theme.text().add_modifier(Modifier::BOLD),
        ),
        (
            lang.current_priority(request.current.0, request.current.1),
            theme.text(),
        ),
        (String::new(), theme.text()),
        (
            format!("{} {:>3}", t.nice_new, request.value),
            theme.selected(),
        ),
        (ruler, Style::default().fg(theme.accent())),
        (t.nice_ruler.to_string(), theme.dim()),
        (t.nice_only_under_load.to_string(), theme.dim()),
    ];
    // Subir la prioridad pide privilegios; bajarla es un camino de ida para
    // un usuario común, porque el kernel no deja volver a un valor menor.
    let raising = (request.value as i64) < request.current.0;
    let lowering = (request.value as i64) > request.current.0;
    let warn = Style::default().fg(theme.load(50.0));
    if raising {
        lines.push((t.nice_raise_needs_root.to_string(), warn));
        lines.push((String::new(), theme.text()));
    } else if lowering {
        lines.push((t.nice_one_way_1.to_string(), warn));
        lines.push((t.nice_one_way_2.to_string(), warn));
    } else {
        lines.push((String::new(), theme.text()));
        lines.push((String::new(), theme.text()));
    }
    lines.push((t.nice_keys.to_string(), theme.dim()));
    let title = if request.single_process {
        t.title_priority_process
    } else {
        t.title_priority_app
    };
    draw_box(buf, area, theme, title, &lines);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_fit_tiny_areas_without_panicking() {
        let theme = Theme::default();
        for (w, h) in [(0, 0), (3, 1), (5, 2), (10, 3), (200, 50)] {
            let area = Rect::new(0, 0, w, h);
            let mut buf = Buffer::empty(area);
            let lines = vec![(
                "una línea bastante larga de prueba".to_string(),
                theme.text(),
            )];
            draw_box(&mut buf, area, theme, "Título", &lines);
        }
    }
}
