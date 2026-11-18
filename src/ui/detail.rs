//! Vista de detalle de una app.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::apps::{App, AppKind};
use crate::collect::{GpuReading, GpuStats, IoReading, NetReading};
use crate::i18n::Lang;
use crate::icons::{IconStore, DETAIL_ICON};

use super::bars::write_spans;
use super::format::{human_bytes, human_duration, human_rate};
use super::graph::{cpu_scale, draw_area, History};
use super::grid::icon_for;
use super::state::State;
use super::tree;

/// Ancho mínimo y máximo del bloque de gráficos, y ancho cómodo del bloque
/// de datos que se le reserva antes de darle más lugar a los gráficos.
const GRAPH_MIN: u16 = 20;
const GRAPH_MAX: u16 = 60;
const INFO_COMFORT: u16 = 46;
const INFO_MIN: u16 = 32;

/// Ancho del bloque de gráficos para un ancho total, o 0 si no entra.
pub fn graph_width(total: u16) -> u16 {
    let fixed = DETAIL_ICON.0 + 2 * GAP;
    let spare = total.saturating_sub(fixed + INFO_COMFORT);
    if spare >= GRAPH_MIN {
        spare.min(GRAPH_MAX)
    } else if total >= fixed + INFO_MIN + GRAPH_MIN {
        // Terminal angosta: se le quita lugar a los datos.
        total - fixed - INFO_MIN
    } else {
        0
    }
}

/// Separación entre el icono y el bloque de datos.
const GAP: u16 = 2;
/// Ancho de la columna de etiquetas del bloque de datos.
const LABEL_WIDTH: usize = 11;

/// Segundos que lleva en ejecución el proceso más antiguo de la app.
pub fn app_uptime(app: &App, system_uptime: Option<f64>, ticks_per_second: f64) -> Option<f64> {
    let oldest = app.procs.iter().map(|p| p.starttime).min()?;
    let uptime = system_uptime?;
    (ticks_per_second > 0.0).then(|| (uptime - oldest as f64 / ticks_per_second).max(0.0))
}

/// Lista de PIDs que entra en `width` columnas, con `+N` si sobran.
pub fn pid_list(lang: Lang, pids: &[i32], width: usize) -> String {
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
                lang.pids_total(pids.len())
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
        return state.lang.t().na.to_string();
    }
    let names: Vec<String> = uids.into_iter().map(|uid| state.user_name(uid)).collect();
    names.join(", ")
}

/// Texto de la fila de disco: lectura y escritura por segundo.
fn disk_text(lang: Lang, reading: Option<IoReading>) -> String {
    let t = lang.t();
    match reading {
        Some(IoReading::Rate(rate)) => {
            format!(
                "{} {}  {} {}",
                t.disk_read,
                human_rate(rate.read_bps),
                t.disk_write,
                human_rate(rate.write_bps)
            )
        }
        Some(IoReading::Unavailable) => t.no_permission.to_string(),
        Some(IoReading::Warming) | None => t.measuring.to_string(),
    }
}

/// Texto de la fila de GPU: porcentaje del motor más ocupado y memoria.
fn gpu_text(lang: Lang, reading: Option<&GpuReading>) -> String {
    let t = lang.t();
    let memory = |stats: &GpuStats| {
        // Solo una GPU dedicada tiene VRAM propia; una integrada usa RAM.
        if stats.dedicated {
            format!("VRAM {}", human_bytes(stats.memory))
        } else {
            format!("{} RAM", human_bytes(stats.memory))
        }
    };
    match reading {
        Some(GpuReading::Stats(stats)) if stats.engine.is_empty() => {
            format!("0% · {}", memory(stats))
        }
        Some(GpuReading::Stats(stats)) => {
            format!(
                "{:.0}% {} · {}",
                stats.busy_pct,
                stats.engine,
                memory(stats)
            )
        }
        Some(GpuReading::Warming(stats)) => format!("{} · {}", t.measuring, memory(stats)),
        Some(GpuReading::NotUsed) => t.gpu_not_used.to_string(),
        Some(GpuReading::Unavailable) => t.no_permission.to_string(),
        None => t.measuring.to_string(),
    }
}

/// Texto de la fila de red: bajada y subida TCP.
fn net_text(lang: Lang, reading: Option<NetReading>) -> String {
    let t = lang.t();
    match reading {
        Some(NetReading::Rate(rate)) if rate.sockets == 0 => t.no_tcp.to_string(),
        Some(NetReading::Rate(rate)) => {
            format!(
                "↓{} ↑{}",
                human_rate(rate.down_bps),
                human_rate(rate.up_bps)
            )
        }
        Some(NetReading::Unavailable) => t.no_permission.to_string(),
        Some(NetReading::Warming) | None => t.measuring.to_string(),
    }
}

fn kind_label(app: &App, lang: Lang) -> String {
    let kind = match app.kind {
        AppKind::App => lang.t().kind_app,
        AppKind::Service => lang.t().kind_service,
    };
    let id = app
        .desktop
        .as_ref()
        .map(|d| d.id.clone())
        .or_else(|| app.app_id.clone());
    match id {
        Some(id) => format!("{kind} · {} · {id}", app.origin.label(lang)),
        None => format!("{kind} · {}", app.origin.label(lang)),
    }
}

/// Alto de la cabecera: el del icono grande.
pub const HEADER_HEIGHT: u16 = DETAIL_ICON.1;

/// Dibuja la cabecera: icono grande a la izquierda y datos a la derecha.
fn draw_header(buf: &mut Buffer, area: Rect, app: &App, state: &mut State, store: &mut IconStore) {
    let theme = state.theme.clone();
    let icon = icon_for(app, store, DETAIL_ICON);
    let icon_style = theme.icon(icon.color);
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
    let graphs = graph_width(area.width);
    let info_width = if graphs > 0 {
        area.width - info_x - GAP - graphs
    } else {
        area.width - info_x
    };
    let info = Rect::new(area.x + info_x, area.y, info_width, area.height);
    if graphs > 0 {
        let graph_area = Rect::new(area.x + area.width - graphs, area.y, graphs, area.height);
        draw_graphs(buf, graph_area, app, state);
    }
    let lang = state.lang;
    let t = lang.t();
    let value_width = (info.width as usize).saturating_sub(LABEL_WIDTH);

    let threads: i64 = app.procs.iter().map(|p| p.threads).sum();
    let uptime = app_uptime(app, state.system.uptime, state.ticks_per_second)
        .map(human_duration)
        .unwrap_or_else(|| t.na.to_string());
    let ram = match state.detail_of_selected().and_then(|d| d.pss) {
        Some(pss) => format!("{}  (PSS {})", human_bytes(app.rss), human_bytes(pss)),
        None => format!("{}  (PSS {})", human_bytes(app.rss), t.na),
    };
    let rows: Vec<(&str, String, Style)> = vec![
        (t.label_user, owners(app, state), theme.text()),
        (t.label_uptime, uptime, theme.text()),
        (
            t.label_processes,
            lang.processes_and_threads(app.procs.len(), threads),
            theme.text(),
        ),
        (
            "PIDs",
            pid_list(lang, &app.pids(), value_width),
            theme.text(),
        ),
        (
            "CPU",
            format!("{:.1}%", app.cpu_pct),
            theme.load(app.cpu_pct),
        ),
        ("RAM", ram, theme.text()),
        (
            t.label_disk,
            disk_text(lang, state.detail_of_selected().map(|d| d.io)),
            theme.text(),
        ),
        (
            "GPU",
            gpu_text(lang, state.detail_of_selected().map(|d| &d.gpu)),
            theme.text(),
        ),
        // Solo TCP y sin lo de conexiones efímeras: la etiqueta lo avisa.
        (
            t.label_net,
            net_text(lang, state.detail_of_selected().map(|d| d.net)),
            theme.text(),
        ),
    ];

    let line = |y: u16| Rect::new(info.x, info.y + y, info.width, 1);
    let name_style = theme.icon(icon.color).add_modifier(Modifier::BOLD);
    if info.height > 0 {
        write_spans(buf, line(0), &[(app.name.clone(), name_style)]);
    }
    if info.height > 1 {
        write_spans(buf, line(1), &[(kind_label(app, lang), theme.dim())]);
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

/// Escribe un título a la izquierda y, a la derecha, la primera de las
/// notas atenuadas que entre en el espacio que queda.
fn title_row(
    buf: &mut Buffer,
    area: Rect,
    title: (String, Style),
    notes: &[String],
    note_style: Style,
) {
    let end = write_spans(buf, area, &[title]);
    let right = area.x + area.width;
    for note in notes {
        let width = note.chars().count() as u16;
        if right > end + width {
            buf.set_string(right - width, area.y, note, note_style);
            return;
        }
    }
}

/// Dibuja los gráficos de CPU (arriba) y RAM (abajo) de los últimos 2 minutos.
fn draw_graphs(buf: &mut Buffer, area: Rect, app: &App, state: &State) {
    if area.height < 4 {
        return;
    }
    let theme = state.theme.clone();
    let empty = History::default();
    let history = state.history.get(&app.id).unwrap_or(&empty);
    let columns = area.width as usize * 2;
    // Cada gráfico ocupa la mitad del alto: una fila de título y el resto de área.
    let half = area.height / 2;
    let plot_height = half - 1;

    let top = state.lang.t().axis_top;
    let cpu = history.resample(state.clock, columns, |p| p.cpu_pct as f64);
    let cpu_peak = cpu.iter().flatten().fold(0.0f64, |a, b| a.max(*b));
    let cpu_max = cpu_scale(cpu_peak);
    let cpu_color = theme.load(app.cpu_pct);
    title_row(
        buf,
        Rect::new(area.x, area.y, area.width, 1),
        (
            format!("CPU {:.1}%", app.cpu_pct),
            cpu_color.add_modifier(Modifier::BOLD),
        ),
        &[
            format!("{top} {cpu_max:.0}% · 2 min"),
            format!("{top} {cpu_max:.0}%"),
        ],
        theme.dim(),
    );
    let plot = Rect::new(area.x, area.y + 1, area.width, plot_height);
    draw_area(buf, plot, &cpu, cpu_max, cpu_color);

    let ram = history.resample(state.clock, columns, |p| p.rss as f64);
    let ram_peak = ram.iter().flatten().fold(0.0f64, |a, b| a.max(*b));
    // Un 25 % de aire sobre el pico; mínimo 1 MiB para no dividir por cero.
    let ram_max = (ram_peak * 1.25).max(1024.0 * 1024.0);
    let ram_color = theme.ram();
    let y = area.y + half;
    title_row(
        buf,
        Rect::new(area.x, y, area.width, 1),
        (
            format!("RAM {}", human_bytes(app.rss)),
            ram_color.add_modifier(Modifier::BOLD),
        ),
        &[format!("{top} {}", human_bytes(ram_max as u64))],
        theme.dim(),
    );
    let plot = Rect::new(area.x, y + 1, area.width, area.height - half - 1);
    draw_area(buf, plot, &ram, ram_max, ram_color);
}

/// Dibuja el detalle de la app seleccionada.
pub fn draw(buf: &mut Buffer, area: Rect, state: &mut State, store: &mut IconStore) {
    let Some(app) = state.selected_app().cloned() else {
        let message = state.lang.t().app_gone;
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

    // Debajo, separado por una línea en blanco, el árbol de procesos.
    let used = HEADER_HEIGHT + 1;
    if area.height > used + 1 {
        let tree_area = Rect::new(area.x, area.y + used, area.width, area.height - used);
        tree::draw(buf, tree_area, &app, state);
    }
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
    fn graphs_get_the_spare_width_and_disappear_when_too_narrow() {
        assert_eq!(
            graph_width(80),
            20,
            "a 80 columnas se achica el bloque de datos"
        );
        assert_eq!(graph_width(100), 26);
        assert_eq!(graph_width(140), 60, "nunca más que el máximo");
        assert_eq!(graph_width(79), 0);
        assert_eq!(graph_width(10), 0);
    }

    #[test]
    fn disk_row_distinguishes_rate_warming_and_no_permission() {
        use crate::collect::IoRate;
        let rate = IoReading::Rate(IoRate {
            read_bps: 1_572_864.0,
            write_bps: 0.0,
        });
        assert_eq!(disk_text(Lang::Es, Some(rate)), "L 1.5M/s  E 0B/s");
        assert_eq!(
            disk_text(Lang::Es, Some(IoReading::Unavailable)),
            "n/d (sin permiso)"
        );
        assert_eq!(disk_text(Lang::Es, Some(IoReading::Warming)), "midiendo…");
        assert_eq!(disk_text(Lang::Es, None), "midiendo…");
        assert_eq!(disk_text(Lang::En, Some(rate)), "R 1.5M/s  W 0B/s");
        assert_eq!(
            disk_text(Lang::En, Some(IoReading::Unavailable)),
            "n/a (no permission)"
        );
        assert_eq!(disk_text(Lang::En, None), "measuring…");
    }

    #[test]
    fn gpu_row_covers_every_reading() {
        let stats = GpuStats {
            busy_pct: 12.4,
            engine: "gfx".into(),
            memory: 344 << 20,
            dedicated: true,
            driver: "amdgpu".into(),
            clients: 1,
        };
        assert_eq!(
            gpu_text(Lang::Es, Some(&GpuReading::Stats(stats.clone()))),
            "12% gfx · VRAM 344M"
        );
        let integrated = GpuStats {
            dedicated: false,
            ..stats.clone()
        };
        assert_eq!(
            gpu_text(Lang::Es, Some(&GpuReading::Warming(integrated))),
            "midiendo… · 344M RAM"
        );
        let idle = GpuStats {
            busy_pct: 0.0,
            engine: String::new(),
            ..stats.clone()
        };
        assert_eq!(
            gpu_text(Lang::Es, Some(&GpuReading::Stats(idle))),
            "0% · VRAM 344M"
        );
        assert_eq!(gpu_text(Lang::Es, Some(&GpuReading::NotUsed)), "sin uso");
        assert_eq!(
            gpu_text(Lang::Es, Some(&GpuReading::Unavailable)),
            "n/d (sin permiso)"
        );
        assert_eq!(gpu_text(Lang::Es, None), "midiendo…");
        assert_eq!(gpu_text(Lang::En, Some(&GpuReading::NotUsed)), "not in use");
        assert_eq!(gpu_text(Lang::En, None), "measuring…");
    }

    #[test]
    fn net_row_covers_every_reading() {
        use crate::collect::NetRate;
        let rate = NetRate {
            down_bps: 12_288.0,
            up_bps: 3174.0,
            sockets: 4,
        };
        assert_eq!(
            net_text(Lang::Es, Some(NetReading::Rate(rate))),
            "↓12.0K/s ↑3.1K/s"
        );
        assert_eq!(
            net_text(Lang::Es, Some(NetReading::Rate(NetRate::default()))),
            "sin conexiones TCP"
        );
        assert_eq!(
            net_text(Lang::Es, Some(NetReading::Unavailable)),
            "n/d (sin permiso)"
        );
        assert_eq!(net_text(Lang::Es, None), "midiendo…");
        assert_eq!(
            net_text(Lang::En, Some(NetReading::Rate(NetRate::default()))),
            "no TCP connections"
        );
    }

    #[test]
    fn pid_list_truncates_with_a_count() {
        assert_eq!(pid_list(Lang::Es, &[1, 22, 333], 40), "1 22 333");
        assert_eq!(
            pid_list(Lang::Es, &[1000, 2000, 3000, 4000], 14),
            "1000 2000 +2"
        );
        assert_eq!(pid_list(Lang::Es, &[1000, 2000], 9), "1000 2000");
        assert_eq!(pid_list(Lang::Es, &[123456, 2], 5), "2 en total");
        assert_eq!(pid_list(Lang::En, &[123456, 2], 7), "2 total");
        assert_eq!(pid_list(Lang::Es, &[], 10), "");
    }
}
