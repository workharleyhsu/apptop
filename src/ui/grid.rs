//! Pantalla principal: grilla de iconos de apps y servicios.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::apps::{App, AppKind};
use crate::i18n::Lang;
use crate::icons::{BrailleIcon, GenericKind, IconStore};

use super::format::{fit_center, human_bytes};
use super::state::{app_is_paused, State};

/// Columna libre a cada lado del icono (ahí va la marca de selección).
const SIDE_PAD: u16 = 1;
/// Líneas de texto bajo el icono: nombre y resumen CPU/RAM.
const TEXT_ROWS: u16 = 2;
/// Línea en blanco entre filas de iconos.
const ROW_GAP: u16 = 1;

/// Posición de una app dentro de la grilla, en coordenadas virtuales
/// (antes de aplicar el scroll).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    /// Índice dentro de las apps visibles.
    pub index: usize,
    pub x: u16,
    pub y: u16,
}

/// Disposición de la grilla para un ancho dado.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GridLayout {
    pub columns: u16,
    pub tile_width: u16,
    pub tile_height: u16,
    pub tiles: Vec<Tile>,
    /// Encabezados de sección: (y virtual, tipo, cantidad).
    pub headers: Vec<(u16, AppKind, usize)>,
    /// Alto total del contenido, en líneas.
    pub height: u16,
    /// Área de pantalla donde se dibujó por última vez.
    pub area: Rect,
}

impl GridLayout {
    /// Calcula la disposición para `kinds` (el tipo de cada app visible, en
    /// orden) en un área de `width` columnas.
    pub fn compute(kinds: &[AppKind], width: u16, icon: (u16, u16)) -> Self {
        let tile_width = icon.0 + 2 * SIDE_PAD;
        let tile_height = icon.1 + TEXT_ROWS;
        let columns = (width / tile_width).max(1);
        let left = width.saturating_sub(columns * tile_width) / 2;

        let mut layout = GridLayout {
            columns,
            tile_width,
            tile_height,
            ..Default::default()
        };
        let mut y = 0u16;
        let mut index = 0usize;
        for kind in [AppKind::App, AppKind::Service] {
            let count = kinds.iter().filter(|k| **k == kind).count();
            if count == 0 {
                continue;
            }
            layout.headers.push((y, kind, count));
            y = y.saturating_add(1);
            for n in 0..count {
                let col = (n % columns as usize) as u16;
                if n > 0 && col == 0 {
                    y = y.saturating_add(tile_height + ROW_GAP);
                }
                layout.tiles.push(Tile {
                    index,
                    x: left + col * tile_width,
                    y,
                });
                index += 1;
            }
            y = y.saturating_add(tile_height + ROW_GAP);
        }
        layout.height = y.saturating_sub(ROW_GAP);
        layout
    }

    /// Scroll necesario para que la app `index` quede visible en una ventana
    /// de `view_height` líneas, moviendo lo menos posible desde `scroll`.
    pub fn scroll_to_show(&self, index: usize, scroll: u16, view_height: u16) -> u16 {
        let max_scroll = self.height.saturating_sub(view_height);
        let Some(tile) = self.tiles.iter().find(|t| t.index == index) else {
            return scroll.min(max_scroll);
        };
        // Al subir se muestra también el encabezado de la sección.
        let top = tile.y.saturating_sub(1);
        let bottom = tile.y + self.tile_height;
        let scroll = if top < scroll {
            top
        } else if bottom > scroll.saturating_add(view_height) {
            bottom.saturating_sub(view_height)
        } else {
            scroll
        };
        scroll.min(max_scroll)
    }

    /// App bajo la celda de pantalla `(column, row)`, si hay alguna.
    pub fn hit(&self, column: u16, row: u16, scroll: u16) -> Option<usize> {
        let a = self.area;
        if column < a.x || row < a.y || column >= a.x + a.width || row >= a.y + a.height {
            return None;
        }
        let (vx, vy) = (column - a.x, row - a.y + scroll);
        self.tiles
            .iter()
            .find(|t| {
                vx >= t.x && vx < t.x + self.tile_width && vy >= t.y && vy < t.y + self.tile_height
            })
            .map(|t| t.index)
    }
}

/// Escribe `text` en la línea virtual `vy` si cae dentro del área visible.
fn put(buf: &mut Buffer, area: Rect, scroll: u16, x: u16, vy: u16, text: &str, style: Style) {
    if vy < scroll || vy - scroll >= area.height || x >= area.width {
        return;
    }
    let width = (area.width - x) as usize;
    buf.set_stringn(area.x + x, area.y + vy - scroll, text, width, style);
}

fn section_title(kind: AppKind, lang: Lang) -> &'static str {
    match kind {
        AppKind::App => lang.t().applications,
        AppKind::Service => lang.t().services,
    }
}

/// Textos de CPU y RAM que entran juntos en `width` columnas. Con iconos
/// chicos se van quitando decimales y, si aun así no entra, queda solo la CPU.
pub fn stats_text(cpu_pct: f32, rss: u64, width: usize) -> (String, String) {
    let ram = human_bytes(rss);
    let short_ram = || {
        // `1.5G` → `2G`: sin decimal.
        let unit = ram.chars().last().filter(|c| c.is_alphabetic());
        let number: f64 = ram
            .trim_end_matches(char::is_alphabetic)
            .parse()
            .unwrap_or(0.0);
        match unit {
            Some(unit) => format!("{number:.0}{unit}"),
            None => ram.clone(),
        }
    };
    let candidates = [
        (format!("{cpu_pct:.1}%"), ram.clone()),
        (format!("{cpu_pct:.0}%"), ram.clone()),
        (format!("{cpu_pct:.0}%"), short_ram()),
    ];
    for (cpu, ram) in candidates {
        if cpu.chars().count() + 1 + ram.chars().count() <= width {
            return (cpu, ram);
        }
    }
    let cpu: String = format!("{cpu_pct:.0}%").chars().take(width).collect();
    (cpu, String::new())
}

/// Icono de una app: el de su `.desktop`, o el genérico según el tipo.
pub fn icon_for(app: &App, store: &mut IconStore, size: (u16, u16)) -> std::sync::Arc<BrailleIcon> {
    let fallback = match app.kind {
        AppKind::App => GenericKind::App,
        AppKind::Service => GenericKind::Service,
    };
    let name = app.desktop.as_ref().and_then(|d| d.icon.as_deref());
    store.named_or_generic(name, fallback, size.0, size.1)
}

/// Dibuja la grilla en `area` y deja la disposición en `state.layout`.
pub fn draw(buf: &mut Buffer, area: Rect, state: &mut State, store: &mut IconStore) {
    let kinds: Vec<AppKind> = state.visible().map(|a| a.kind).collect();
    let mut layout = GridLayout::compute(&kinds, area.width, state.icon_size);
    layout.area = area;
    state.scroll = match state.selected_index() {
        Some(index) => layout.scroll_to_show(index, state.scroll, area.height),
        None => 0,
    };
    let scroll = state.scroll;
    let theme = state.theme.clone();

    if layout.tiles.is_empty() {
        let message = if state.filter.is_empty() {
            state.lang.t().no_apps
        } else {
            state.lang.t().no_match
        };
        let x = area.width.saturating_sub(message.chars().count() as u16) / 2;
        put(buf, area, 0, x, area.height / 2, message, theme.dim());
        state.layout = layout;
        return;
    }

    for (y, kind, count) in &layout.headers {
        let title = format!("── {} ({count}) ", section_title(*kind, state.lang));
        let rule = "─".repeat((area.width as usize).saturating_sub(title.chars().count()));
        put(
            buf,
            area,
            scroll,
            0,
            *y,
            &format!("{title}{rule}"),
            theme.header(),
        );
    }

    let selected = state.selected_index();
    let (icon_w, icon_h) = state.icon_size;
    for tile in &layout.tiles {
        // Nada que dibujar si la baldosa entera queda fuera de la ventana.
        if tile.y + layout.tile_height <= scroll || tile.y >= scroll.saturating_add(area.height) {
            continue;
        }
        let Some(app) = state.order.get(tile.index).and_then(|i| state.apps.get(*i)) else {
            continue;
        };
        let is_selected = selected == Some(tile.index);
        let icon = icon_for(app, store, state.icon_size);
        let icon_style = theme.icon(icon.color);
        for (row, line) in icon.rows().iter().enumerate() {
            put(
                buf,
                area,
                scroll,
                tile.x + SIDE_PAD,
                tile.y + row as u16,
                line,
                icon_style,
            );
        }

        let name_style = if is_selected {
            theme.selected()
        } else {
            theme.text().add_modifier(Modifier::BOLD)
        };
        let name = fit_center(&app.name, icon_w as usize);
        put(
            buf,
            area,
            scroll,
            tile.x + SIDE_PAD,
            tile.y + icon_h,
            &name,
            name_style,
        );

        // Una app pausada no consume CPU: se avisa en lugar del resumen.
        if app_is_paused(app) {
            let label = fit_center(state.lang.t().paused, icon_w as usize);
            let style = theme.load(50.0);
            let y = tile.y + icon_h + 1;
            put(buf, area, scroll, tile.x + SIDE_PAD, y, &label, style);
        } else {
            // Resumen: CPU coloreada por carga y RAM atenuada, centrados juntos.
            let (cpu, ram) = stats_text(app.cpu_pct, app.rss, icon_w as usize);
            let used = cpu.chars().count() + 1 + ram.chars().count();
            let start = tile.x + SIDE_PAD + (icon_w as usize).saturating_sub(used) as u16 / 2;
            let stats_y = tile.y + icon_h + 1;
            put(
                buf,
                area,
                scroll,
                start,
                stats_y,
                &cpu,
                theme.load(app.cpu_pct),
            );
            let ram_x = start + cpu.chars().count() as u16 + 1;
            put(buf, area, scroll, ram_x, stats_y, &ram, theme.dim());
        }

        if is_selected {
            // Marca de selección: barras a los lados, sin tocar el fondo.
            let right = tile.x + layout.tile_width - 1;
            for row in 0..layout.tile_height {
                let (l, r) = match row {
                    0 => ("╭", "╮"),
                    r if r == layout.tile_height - 1 => ("╰", "╯"),
                    _ => ("│", "│"),
                };
                put(buf, area, scroll, tile.x, tile.y + row, l, theme.selected());
                put(buf, area, scroll, right, tile.y + row, r, theme.selected());
            }
        }
    }
    state.layout = layout;
}

#[cfg(test)]
mod tests {
    use super::*;
    use AppKind::{App as A, Service as S};

    #[test]
    fn stats_shrink_to_fit_small_icons() {
        let gib = 1u64 << 30;
        assert_eq!(
            stats_text(11.04, 59 * gib / 10, 16),
            ("11.0%".into(), "5.9G".into())
        );
        // 8 columnas: sin decimal en la CPU.
        assert_eq!(
            stats_text(11.04, 59 * gib / 10, 8),
            ("11%".into(), "5.9G".into())
        );
        // "120% 12.0M" no entra en 8: también se redondea la RAM.
        assert_eq!(
            stats_text(120.0, 12 << 20, 8),
            ("120%".into(), "12M".into())
        );
        assert_eq!(
            stats_text(120.0, 12 << 20, 5),
            ("120%".into(), String::new())
        );
        assert_eq!(stats_text(5.0, 0, 16), ("5.0%".into(), "0B".into()));
        for width in 0..20 {
            let (cpu, ram) = stats_text(1234.5, 999 * gib, width);
            let used = cpu.chars().count()
                + if ram.is_empty() {
                    0
                } else {
                    1 + ram.chars().count()
                };
            assert!(used <= width, "{width}: {cpu} {ram}");
        }
    }

    #[test]
    fn adapts_columns_to_the_width() {
        let kinds = [A, A, A, A, A, S, S];
        let wide = GridLayout::compute(&kinds, 120, (16, 8));
        assert_eq!(
            (wide.columns, wide.tile_width, wide.tile_height),
            (6, 18, 10)
        );
        // 120 − 6·18 = 12 → margen izquierdo de 6.
        assert_eq!(
            wide.tiles[0],
            Tile {
                index: 0,
                x: 6,
                y: 1
            }
        );
        assert_eq!(wide.tiles[4].x, 6 + 4 * 18);
        // Servicios: encabezado después de la fila de apps y su separación.
        assert_eq!(wide.headers, vec![(0, A, 5), (12, S, 2)]);
        assert_eq!(
            wide.tiles[5],
            Tile {
                index: 5,
                x: 6,
                y: 13
            }
        );
        assert_eq!(wide.height, 23);

        let narrow = GridLayout::compute(&kinds, 40, (16, 8));
        assert_eq!(narrow.columns, 2);
        assert_eq!(
            narrow.tiles[2],
            Tile {
                index: 2,
                x: 2,
                y: 12
            }
        );
    }

    #[test]
    fn degenerate_sizes_still_lay_out_one_column() {
        let layout = GridLayout::compute(&[A, S], 5, (16, 8));
        assert_eq!(layout.columns, 1);
        assert_eq!(layout.tiles.len(), 2);
        assert!(GridLayout::compute(&[], 80, (16, 8)).tiles.is_empty());
    }

    #[test]
    fn scrolls_just_enough_to_show_the_selection() {
        let kinds = [A; 12];
        let layout = GridLayout::compute(&kinds, 80, (16, 8)); // 4 columnas, 3 filas
        assert_eq!(layout.height, 1 + 3 * 10 + 2);
        assert_eq!(layout.scroll_to_show(0, 0, 22), 0);
        // Fila 2 (y = 12..22): cabe justo en 22 líneas sin mover.
        assert_eq!(layout.scroll_to_show(5, 0, 22), 0);
        // Fila 3 (y = 23..33): hay que bajar 11.
        assert_eq!(layout.scroll_to_show(9, 0, 22), 11);
        // Volver arriba muestra también el encabezado.
        assert_eq!(layout.scroll_to_show(0, 11, 22), 0);
        // Nunca pasa del final del contenido.
        assert_eq!(layout.scroll_to_show(99, 500, 22), 11);
    }

    #[test]
    fn hit_testing_accounts_for_area_and_scroll() {
        let mut layout = GridLayout::compute(&[A; 8], 80, (16, 8));
        layout.area = Rect::new(0, 1, 80, 22);
        assert_eq!(layout.hit(4, 2, 0), Some(0));
        assert_eq!(layout.hit(4 + 18, 5, 0), Some(1));
        assert_eq!(layout.hit(4, 1, 0), None, "encabezado");
        assert_eq!(layout.hit(4, 0, 0), None, "fuera del área");
        assert_eq!(layout.hit(4, 2, 11), Some(4), "segunda fila con scroll");
        assert_eq!(layout.hit(1, 2, 0), None, "margen");
    }
}
