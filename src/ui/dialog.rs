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

/// Texto "N procesos afectados".
fn affected(count: usize) -> String {
    match count {
        1 => "1 proceso afectado".to_string(),
        n => format!("{n} procesos afectados"),
    }
}

/// Diálogo de confirmación de una señal: muestra el nombre y cuántos
/// procesos se ven afectados.
pub fn draw_confirm(buf: &mut Buffer, area: Rect, state: &State, request: &ActionRequest) {
    let theme = state.theme;
    let mut lines = vec![
        (
            format!("¿Enviar {} a {}?", request.signal.as_str(), request.label),
            theme.text().add_modifier(Modifier::BOLD),
        ),
        (affected(request.targets.len()), theme.text()),
    ];
    if request.excluded > 0 {
        let note = match request.excluded {
            1 => "(se excluye 1 proceso del que depende apptop)".to_string(),
            n => format!("(se excluyen {n} procesos de los que depende apptop)"),
        };
        lines.push((note, theme.dim()));
    }
    lines.push((String::new(), theme.text()));
    lines.push(("[s] sí     [n] no".to_string(), theme.selected()));
    draw_box(buf, area, theme, request.title(), &lines);
}

/// Menú de señales: una por fila, con el cursor marcado.
pub fn draw_signal_menu(buf: &mut Buffer, area: Rect, state: &State, cursor: usize) {
    let theme = state.theme;
    let target = match state.action_scope() {
        Some((targets, label, _)) => format!("{label} · {}", affected(targets.len())),
        None => String::new(),
    };
    let mut lines = vec![
        (target, theme.text().add_modifier(Modifier::BOLD)),
        (String::new(), theme.text()),
    ];
    for (row, (signal, description)) in SIGNAL_MENU.iter().enumerate() {
        let (marker, style) = if row == cursor {
            ("▶", theme.selected())
        } else {
            (" ", theme.text())
        };
        lines.push((
            format!("{marker} {:<8} {description}", signal.as_str()),
            style,
        ));
    }
    lines.push((String::new(), theme.text()));
    lines.push((
        "[↑/↓] elegir   [Enter] enviar   [Esc] cancelar".to_string(),
        theme.dim(),
    ));
    draw_box(buf, area, theme, "Enviar señal", &lines);
}

/// Selector de prioridad: valor actual, valor nuevo y una regla de -20 a 19.
pub fn draw_nice(buf: &mut Buffer, area: Rect, state: &State, request: &NiceRequest) {
    let theme = state.theme;
    let current = match request.current {
        (low, high) if low == high => format!("Prioridad actual: {low}"),
        (low, high) => format!("Prioridad actual: entre {low} y {high}"),
    };
    // Regla de 40 posiciones (-20..=19) con una marca en el valor elegido.
    let position = (request.value + 20).clamp(0, 39) as usize;
    let ruler: String = (0..40)
        .map(|i| if i == position { '●' } else { '─' })
        .collect();
    let raising = (request.value as i64) < request.current.0;
    let mut lines = vec![
        (
            format!("{} · {}", request.label, affected(request.targets.len())),
            theme.text().add_modifier(Modifier::BOLD),
        ),
        (current, theme.text()),
        (String::new(), theme.text()),
        (format!("Nueva: {:>3}", request.value), theme.selected()),
        (ruler, Style::default().fg(theme.accent())),
        (
            "-20 más prioridad               menos 19".to_string(),
            theme.dim(),
        ),
    ];
    // Bajar la prioridad es un camino de ida para un usuario común: el
    // kernel no deja volver a un valor menor sin privilegios.
    let lowering = (request.value as i64) > request.current.0;
    let warn = Style::default().fg(theme.load(50.0));
    lines.push((
        "Solo se nota cuando la CPU está saturada".to_string(),
        theme.dim(),
    ));
    if raising {
        lines.push((
            "Subir la prioridad requiere root o CAP_SYS_NICE".to_string(),
            warn,
        ));
        lines.push((String::new(), theme.text()));
    } else if lowering {
        lines.push((
            "Sin root no se puede volver a un valor menor:".to_string(),
            warn,
        ));
        lines.push((
            "para recuperar la prioridad hay que reiniciar la app".to_string(),
            warn,
        ));
    } else {
        lines.push((String::new(), theme.text()));
        lines.push((String::new(), theme.text()));
    }
    lines.push((
        "[←/→] cambiar   [Enter] aplicar   [Esc] cancelar".to_string(),
        theme.dim(),
    ));
    let title = if request.single_process {
        "Prioridad del proceso"
    } else {
        "Prioridad de la aplicación"
    };
    draw_box(buf, area, theme, title, &lines);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_agree_in_number() {
        assert_eq!(affected(1), "1 proceso afectado");
        assert_eq!(affected(22), "22 procesos afectados");
    }

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
