//! Vista de detalle de una app.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::icons::IconStore;

use super::format::human_bytes;
use super::state::State;

/// Dibuja el detalle de la app seleccionada.
pub fn draw(buf: &mut Buffer, area: Rect, state: &mut State, _store: &mut IconStore) {
    let theme = state.theme;
    let Some(app) = state.selected_app() else {
        buf.set_stringn(
            area.x,
            area.y,
            "La app ya no está en ejecución",
            area.width as usize,
            theme.dim(),
        );
        return;
    };
    let title = format!(
        "{} — {} procesos — CPU {:.1}% — RAM {}",
        app.name,
        app.procs.len(),
        app.cpu_pct,
        human_bytes(app.rss)
    );
    buf.set_stringn(area.x, area.y, title, area.width as usize, theme.header());
}
