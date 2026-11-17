//! Vista de detalle de una app.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::apps::{App, AppKind};
use crate::icons::{IconStore, DETAIL_ICON};

use super::bars::write_spans;
use super::format::{human_bytes, human_duration};
use super::grid::icon_for;
use super::state::State;

/// Separación entre el icono y el bloque de datos.
const GAP: u16 = 2;
/// Ancho de la columna de etiquetas del bloque de datos.
const LABEL_WIDTH: usize = 10;

/// Segundos que lleva en ejecución el proceso más antiguo de la app.
pub fn app_uptime(app: &App, system_uptime: Option<f64>, ticks_per_second: f64) -> Option<f64> {
    let oldest = app.procs.iter().map(|p| p.starttime).min()?;
    let uptime = system_uptime?;
    (ticks_per_second > 0.0).then(|| (uptime - oldest as f64 / ticks_per_second).max(0.0))
}

/// Lista de PIDs que entra en `width` columnas, con `+N` si sobran.
pub fn pid_list(pids: &[i32], width: usize) -> String {
    let mut out = String::new();
    for (i, pid) in pids.iter().enumerate() {
        let piece = if i == 0 {
            pid.to_string()
        } else {
            format!(" {pid}")
        };
        let rest = pids.len() - i - 1;
        // Reserva lugar para el " +N" final si todavía quedan más.
        let reserve = if rest > 0 {
            format!(" +{rest}").len()
        } else {
            0
        };
        if out.len() + piece.len() + reserve > width {
            return if out.is_empty() {
                format!("{} en total", pids.len())
            } else {
                format!("{out} +{}", rest + 1)
            };
        }
        out.push_str(&piece);
    }
    out
}

/// Usuarios dueños de los procesos de la app, sin repetir.
fn owners(app: &App, state: &mut State) -> String {
    let mut uids: Vec<u32> = app.procs.iter().filter_map(|p| p.uid).collect();
    uids.sort_unstable();
    uids.dedup();
    if uids.is_empty() {
        return "n/d".to_string();
    }
    let names: Vec<String> = uids.into_iter().map(|uid| state.user_name(uid)).collect();
    names.join(", ")
}

fn kind_label(app: &App) -> String {
    let kind = match app.kind {
        AppKind::App => "aplicación",
        AppKind::Service => "servicio",
    };
    let id = app
        .desktop
        .as_ref()
        .map(|d| d.id.clone())
        .or_else(|| app.app_id.clone());
    match id {
        Some(id) => format!("{kind} · {} · {id}", app.origin.label()),
        None => format!("{kind} · {}", app.origin.label()),
    }
}

/// Alto de la cabecera: el del icono grande.
pub const HEADER_HEIGHT: u16 = DETAIL_ICON.1;

/// Dibuja la cabecera: icono grande a la izquierda y datos a la derecha.
fn draw_header(buf: &mut Buffer, area: Rect, app: &App, state: &mut State, store: &mut IconStore) {
    let theme = state.theme;
    let icon = icon_for(app, store, DETAIL_ICON);
    let icon_style = Style::default().fg(theme.rgb(icon.color));
    for (row, line) in icon.rows().iter().enumerate().take(area.height as usize) {
        buf.set_stringn(
            area.x,
            area.y + row as u16,
            line,
            area.width as usize,
            icon_style,
        );
    }

    let info_x = DETAIL_ICON.0 + GAP;
    if area.width <= info_x {
        return;
    }
    let info = Rect::new(area.x + info_x, area.y, area.width - info_x, area.height);
    let value_width = (info.width as usize).saturating_sub(LABEL_WIDTH);

    let threads: i64 = app.procs.iter().map(|p| p.threads).sum();
    let uptime = app_uptime(app, state.system.uptime, state.ticks_per_second)
        .map(human_duration)
        .unwrap_or_else(|| "n/d".to_string());
    let ram = match state.detail_of_selected().and_then(|d| d.pss) {
        Some(pss) => format!("{}  (PSS {})", human_bytes(app.rss), human_bytes(pss)),
        None => format!("{}  (PSS n/d)", human_bytes(app.rss)),
    };
    let rows: Vec<(&str, String, Style)> = vec![
        ("Usuario", owners(app, state), theme.text()),
        ("Activa", uptime, theme.text()),
        (
            "Procesos",
            format!("{} · {threads} hilos", app.procs.len()),
            theme.text(),
        ),
        ("PIDs", pid_list(&app.pids(), value_width), theme.text()),
        (
            "CPU",
            format!("{:.1}%", app.cpu_pct),
            Style::default().fg(theme.load(app.cpu_pct)),
        ),
        ("RAM", ram, theme.text()),
    ];

    let line = |y: u16| Rect::new(info.x, info.y + y, info.width, 1);
    let name_style = Style::default()
        .fg(theme.rgb(icon.color))
        .add_modifier(Modifier::BOLD);
    if info.height > 0 {
        write_spans(buf, line(0), &[(app.name.clone(), name_style)]);
    }
    if info.height > 1 {
        write_spans(buf, line(1), &[(kind_label(app), theme.dim())]);
    }
    for (i, (label, value, style)) in rows.into_iter().enumerate() {
        let y = 3 + i as u16;
        if y >= info.height {
            break;
        }
        let label = format!("{label:<LABEL_WIDTH$}");
        write_spans(buf, line(y), &[(label, theme.dim()), (value, style)]);
    }
}

/// Dibuja el detalle de la app seleccionada.
pub fn draw(buf: &mut Buffer, area: Rect, state: &mut State, store: &mut IconStore) {
    let Some(app) = state.selected_app().cloned() else {
        let message = "La app ya no está en ejecución";
        buf.set_stringn(
            area.x,
            area.y,
            message,
            area.width as usize,
            state.theme.dim(),
        );
        return;
    };
    let header = Rect::new(area.x, area.y, area.width, area.height.min(HEADER_HEIGHT));
    draw_header(buf, header, &app, state, store);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::group::tests::proc;
    use crate::ui::state::tests::app;

    #[test]
    fn uptime_uses_the_oldest_process() {
        let mut a = app("x", AppKind::App, 0.0, 1);
        assert_eq!(app_uptime(&a, Some(100.0), 100.0), None, "sin procesos");
        a.procs = vec![proc(1, 0, "/bin/a", "/"), proc(2, 0, "/bin/b", "/")];
        a.procs[0].starttime = 5000; // arrancó a los 50 s del boot
        a.procs[1].starttime = 9000;
        assert_eq!(app_uptime(&a, Some(170.0), 100.0), Some(120.0));
        assert_eq!(app_uptime(&a, None, 100.0), None);
        assert_eq!(
            app_uptime(&a, Some(10.0), 100.0),
            Some(0.0),
            "nunca negativo"
        );
    }

    #[test]
    fn pid_list_truncates_with_a_count() {
        assert_eq!(pid_list(&[1, 22, 333], 40), "1 22 333");
        assert_eq!(pid_list(&[1000, 2000, 3000, 4000], 14), "1000 2000 +2");
        assert_eq!(pid_list(&[1000, 2000], 9), "1000 2000");
        assert_eq!(pid_list(&[123456, 2], 5), "2 en total");
        assert_eq!(pid_list(&[], 10), "");
    }
}
