//! Historial de uso y gráficos de área en braille.

use std::collections::VecDeque;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

/// Ventana de tiempo que cubren los gráficos, en segundos.
pub const WINDOW_SECS: f64 = 120.0;

/// Una medición de una app en un instante.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// Segundos desde el arranque de apptop.
    pub at: f64,
    pub cpu_pct: f32,
    pub rss: u64,
}

/// Últimos ~2 minutos de mediciones de una app.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct History {
    points: VecDeque<Point>,
}

impl History {
    /// Agrega una medición y descarta las que salieron de la ventana.
    pub fn push(&mut self, point: Point) {
        self.points.push_back(point);
        while self
            .points
            .front()
            .is_some_and(|p| p.at < point.at - WINDOW_SECS)
        {
            self.points.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn last_at(&self) -> Option<f64> {
        self.points.back().map(|p| p.at)
    }

    /// Reparte la ventana que termina en `now` en `columns` tramos iguales y
    /// devuelve, para cada uno, el máximo de `value`. Los tramos sin datos
    /// (la app todavía no existía, o el refresco es más lento que el tramo)
    /// repiten el último valor conocido; antes del primer dato quedan `None`.
    pub fn resample(
        &self,
        now: f64,
        columns: usize,
        value: impl Fn(&Point) -> f64,
    ) -> Vec<Option<f64>> {
        let mut out: Vec<Option<f64>> = vec![None; columns];
        if columns == 0 {
            return out;
        }
        let span = WINDOW_SECS / columns as f64;
        let start = now - WINDOW_SECS;
        for point in &self.points {
            let offset = point.at - start;
            if offset < 0.0 {
                continue;
            }
            let column = ((offset / span) as usize).min(columns - 1);
            let v = value(point);
            out[column] = Some(out[column].map_or(v, |old: f64| old.max(v)));
        }
        let mut last = None;
        for slot in &mut out {
            match slot {
                Some(v) => last = Some(*v),
                None => *slot = last,
            }
        }
        out
    }
}

/// Bits braille de la columna izquierda y derecha, de abajo hacia arriba.
const LEFT: [u8; 4] = [0x40, 0x04, 0x02, 0x01];
const RIGHT: [u8; 4] = [0x80, 0x20, 0x10, 0x08];

/// Convierte una serie (una muestra por columna de puntos, dos por celda) en
/// filas de braille de `height` celdas, con el área bajo la curva rellena.
/// `max` es el valor que corresponde al tope del gráfico.
pub fn area_rows(values: &[Option<f64>], height: usize, max: f64) -> Vec<String> {
    let width = values.len().div_ceil(2);
    let dots_high = height * 4;
    let mut cells = vec![0u8; width * height];
    for (x, value) in values.iter().enumerate() {
        let Some(value) = value else {
            continue;
        };
        let fraction = if max > 0.0 {
            (value / max).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // Un valor conocido siempre deja al menos la línea de base.
        let lit = ((fraction * dots_high as f64).round() as usize).clamp(1, dots_high.max(1));
        let bits = if x % 2 == 0 { &LEFT } else { &RIGHT };
        for dot in 0..lit.min(dots_high) {
            let row = height - 1 - dot / 4;
            cells[row * width + x / 2] |= bits[dot % 4];
        }
    }
    cells
        .chunks(width.max(1))
        .map(|row| {
            row.iter()
                .map(|m| {
                    if *m == 0 {
                        ' '
                    } else {
                        char::from_u32(0x2800 + *m as u32).unwrap_or(' ')
                    }
                })
                .collect()
        })
        .collect()
}

/// Escala del eje de CPU: el primer tope "redondo" que contiene al pico.
/// El uso va sobre el total de la máquina, así que nunca pasa de 100 %; con
/// un eje fijo de 100 una app que usa 3 % se vería como una línea plana.
pub fn cpu_scale(peak: f64) -> f64 {
    [5.0, 10.0, 25.0, 50.0]
        .into_iter()
        .find(|top| peak <= *top)
        .unwrap_or(100.0)
}

/// Dibuja un gráfico de área en `area` (sin título).
pub fn draw_area(buf: &mut Buffer, area: Rect, values: &[Option<f64>], max: f64, style: Style) {
    for (y, row) in area_rows(values, area.height as usize, max)
        .iter()
        .enumerate()
    {
        buf.set_stringn(area.x, area.y + y as u16, row, area.width as usize, style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(at: f64, cpu: f32) -> Point {
        Point {
            at,
            cpu_pct: cpu,
            rss: 0,
        }
    }

    #[test]
    fn history_keeps_only_the_window() {
        let mut h = History::default();
        for t in 0..300 {
            h.push(point(t as f64, t as f32));
        }
        assert_eq!(h.len(), 121);
        assert_eq!(h.last_at(), Some(299.0));
    }

    #[test]
    fn resample_buckets_by_time_and_takes_the_peak() {
        let mut h = History::default();
        // Ventana [0, 120] en 4 tramos de 30 s.
        for (t, cpu) in [(10.0, 5.0), (20.0, 9.0), (70.0, 3.0), (119.0, 7.0)] {
            h.push(point(t, cpu));
        }
        let series = h.resample(120.0, 4, |p| p.cpu_pct as f64);
        // El tramo 30–60 no tiene datos: repite el último valor conocido.
        assert_eq!(series, vec![Some(9.0), Some(9.0), Some(3.0), Some(7.0)]);
    }

    #[test]
    fn resample_leaves_the_time_before_the_first_sample_empty() {
        let mut h = History::default();
        h.push(point(100.0, 4.0));
        let series = h.resample(120.0, 4, |p| p.cpu_pct as f64);
        assert_eq!(series, vec![None, None, None, Some(4.0)]);
        assert!(History::default()
            .resample(10.0, 3, |_| 1.0)
            .iter()
            .all(Option::is_none));
        assert!(h.resample(120.0, 0, |_| 1.0).is_empty());
    }

    #[test]
    fn area_rows_fill_from_the_bottom() {
        // Dos celdas de ancho, una de alto: 0 %, 50 %, 100 %, sin dato.
        let rows = area_rows(&[Some(0.0), Some(50.0), Some(100.0), None], 1, 100.0);
        // 0 % deja la línea de base; 50 % dos puntos; 100 % los cuatro.
        assert_eq!(rows, vec!["⣠⡇".to_string()]);
    }

    #[test]
    fn area_rows_span_several_rows_and_clamp() {
        let rows = area_rows(
            &[Some(100.0), Some(100.0), Some(250.0), Some(25.0)],
            2,
            200.0,
        );
        assert_eq!(rows.len(), 2);
        // 100/200 llena la fila de abajo; 250 se recorta al tope; 25 → 1 punto.
        assert_eq!(rows[0], " ⡇");
        assert_eq!(rows[1], "⣿⣇");
        assert!(area_rows(&[], 3, 1.0).iter().all(|r| r.is_empty()));
        assert_eq!(area_rows(&[Some(1.0)], 1, 0.0), vec!["⡀".to_string()]);
    }

    #[test]
    fn cpu_axis_picks_the_first_round_top_above_the_peak() {
        assert_eq!(cpu_scale(0.0), 5.0);
        assert_eq!(cpu_scale(4.9), 5.0);
        assert_eq!(cpu_scale(8.0), 10.0);
        assert_eq!(cpu_scale(18.0), 25.0);
        assert_eq!(cpu_scale(50.0), 50.0);
        assert_eq!(cpu_scale(51.0), 100.0);
        assert_eq!(cpu_scale(380.0), 100.0);
    }
}
