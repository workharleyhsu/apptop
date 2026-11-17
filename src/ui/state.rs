//! Estado de la interfaz, independiente del dibujo.

use crate::apps::{App, AppKind};
use crate::icons::GRID_ICON;

use super::grid::GridLayout;
use super::theme::Theme;

/// Pantalla que se está mostrando.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Grid,
    /// Detalle de la app seleccionada.
    Detail,
}

/// Movimiento de la selección dentro de la grilla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
}

pub struct State {
    pub view: View,
    /// Apps de la última muestra.
    pub apps: Vec<App>,
    /// Índices de `apps` en el orden en que se muestran.
    pub order: Vec<usize>,
    /// Id de la app seleccionada (sobrevive a los refrescos).
    pub selected: Option<String>,
    /// Desplazamiento vertical de la grilla, en líneas.
    pub scroll: u16,
    /// Tamaño de los iconos de la grilla, en celdas.
    pub icon_size: (u16, u16),
    pub theme: Theme,
    /// Disposición del último cuadro dibujado (para el mouse y el scroll).
    pub layout: GridLayout,
}

impl State {
    pub fn new(theme: Theme) -> Self {
        Self {
            view: View::Grid,
            apps: Vec::new(),
            order: Vec::new(),
            selected: None,
            scroll: 0,
            icon_size: GRID_ICON,
            theme,
            layout: GridLayout::default(),
        }
    }

    /// Reemplaza las apps por una muestra nueva conservando la selección.
    pub fn set_apps(&mut self, apps: Vec<App>) {
        self.apps = apps;
        self.reorder();
    }

    /// Recalcula el orden de presentación: apps primero, luego servicios.
    pub fn reorder(&mut self) {
        let mut order: Vec<usize> = (0..self.apps.len()).collect();
        order.sort_by(|a, b| {
            let (a, b) = (&self.apps[*a], &self.apps[*b]);
            (a.kind, a.name.to_lowercase(), &a.id).cmp(&(b.kind, b.name.to_lowercase(), &b.id))
        });
        self.order = order;
        let still_there = self
            .selected
            .as_ref()
            .is_some_and(|id| self.order.iter().any(|i| &self.apps[*i].id == id));
        if !still_there {
            self.selected = self.order.first().map(|i| self.apps[*i].id.clone());
        }
    }

    /// Apps en orden de presentación.
    pub fn visible(&self) -> impl Iterator<Item = &App> {
        self.order.iter().map(|i| &self.apps[*i])
    }

    /// Posición de la app seleccionada dentro de las visibles.
    pub fn selected_index(&self) -> Option<usize> {
        let id = self.selected.as_ref()?;
        self.visible().position(|a| &a.id == id)
    }

    pub fn selected_app(&self) -> Option<&App> {
        let id = self.selected.as_ref()?;
        self.apps.iter().find(|a| &a.id == id)
    }

    pub fn count(&self, kind: AppKind) -> usize {
        self.visible().filter(|a| a.kind == kind).count()
    }

    /// Selecciona la app en la posición `index` de las visibles.
    pub fn select_index(&mut self, index: usize) {
        if let Some(app) = self.order.get(index).and_then(|i| self.apps.get(*i)) {
            self.selected = Some(app.id.clone());
        }
    }

    /// Mueve la selección según la geometría de la grilla: arriba y abajo
    /// buscan la baldosa más cercana en la fila vecina, cruzando secciones.
    pub fn move_selection(&mut self, movement: Move) {
        let count = self.order.len();
        if count == 0 {
            return;
        }
        let current = self.selected_index().unwrap_or(0);
        let rows_per_page = {
            let row = (self.layout.tile_height + 1).max(1);
            (self.layout.area.height / row).max(1) as usize
        };
        let target = match movement {
            Move::Left => current.saturating_sub(1),
            Move::Right => (current + 1).min(count - 1),
            Move::First => 0,
            Move::Last => count - 1,
            Move::Up => self.vertical_neighbour(current, false),
            Move::Down => self.vertical_neighbour(current, true),
            Move::PageUp => {
                (0..rows_per_page).fold(current, |i, _| self.vertical_neighbour(i, false))
            }
            Move::PageDown => {
                (0..rows_per_page).fold(current, |i, _| self.vertical_neighbour(i, true))
            }
        };
        self.select_index(target);
    }

    /// Índice de la baldosa más cercana en la fila de abajo (o de arriba).
    fn vertical_neighbour(&self, index: usize, down: bool) -> usize {
        let tiles = &self.layout.tiles;
        let Some(here) = tiles.iter().find(|t| t.index == index) else {
            // Sin disposición todavía: se mueve de a uno.
            return if down {
                (index + 1).min(self.order.len().saturating_sub(1))
            } else {
                index.saturating_sub(1)
            };
        };
        let row_y = if down {
            tiles.iter().map(|t| t.y).filter(|y| *y > here.y).min()
        } else {
            tiles.iter().map(|t| t.y).filter(|y| *y < here.y).max()
        };
        let Some(row_y) = row_y else {
            return index;
        };
        tiles
            .iter()
            .filter(|t| t.y == row_y)
            .min_by_key(|t| t.x.abs_diff(here.x))
            .map(|t| t.index)
            .unwrap_or(index)
    }

    /// Entra al detalle de la app seleccionada, si hay alguna.
    pub fn enter_detail(&mut self) {
        if self.selected_app().is_some() {
            self.view = View::Detail;
        }
    }

    /// Vuelve a la grilla conservando la selección.
    pub fn back_to_grid(&mut self) {
        self.view = View::Grid;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::apps::Origin;

    pub(crate) fn app(name: &str, kind: AppKind, cpu: f32, rss_mb: u64) -> App {
        App {
            id: format!("{kind:?}:{name}"),
            name: name.to_string(),
            kind,
            origin: Origin::Scope,
            app_id: None,
            desktop: None,
            procs: Vec::new(),
            cpu_pct: cpu,
            rss: rss_mb * 1024 * 1024,
        }
    }

    pub(crate) fn sample_apps() -> Vec<App> {
        vec![
            app("pipewire", AppKind::Service, 0.4, 18),
            app("Firefox", AppKind::App, 23.5, 5400),
            app("Alacritty", AppKind::App, 2.1, 310),
            app("docker", AppKind::Service, 0.0, 108),
            app("Un nombre larguísimo de app", AppKind::App, 120.0, 12),
        ]
    }

    #[test]
    fn orders_apps_before_services_and_selects_the_first() {
        let mut s = State::new(Theme::default());
        s.set_apps(sample_apps());
        let names: Vec<&str> = s.visible().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Alacritty",
                "Firefox",
                "Un nombre larguísimo de app",
                "docker",
                "pipewire"
            ]
        );
        assert_eq!(s.selected_app().unwrap().name, "Alacritty");
        assert_eq!(s.count(AppKind::App), 3);
    }

    /// Estado con `n` apps y `m` servicios ya dispuesto en `width` columnas.
    pub(crate) fn laid_out(n: usize, m: usize, width: u16, height: u16) -> State {
        let mut s = State::new(Theme::default());
        let mut apps: Vec<App> = (0..n)
            .map(|i| app(&format!("app{i:02}"), AppKind::App, 0.0, 1))
            .collect();
        apps.extend((0..m).map(|i| app(&format!("svc{i:02}"), AppKind::Service, 0.0, 1)));
        s.set_apps(apps);
        let kinds: Vec<AppKind> = s.visible().map(|a| a.kind).collect();
        s.layout = GridLayout::compute(&kinds, width, s.icon_size);
        s.layout.area = ratatui::layout::Rect::new(0, 0, width, height);
        s
    }

    #[test]
    fn moves_by_grid_geometry_across_sections() {
        // 80 columnas → 4 por fila. Apps: 6 (filas de 4 y 2). Servicios: 5.
        let mut s = laid_out(6, 5, 80, 24);
        let at = |s: &State| s.selected_index().unwrap();
        assert_eq!(at(&s), 0);
        s.move_selection(Move::Left);
        assert_eq!(at(&s), 0, "no pasa del principio");
        s.move_selection(Move::Right);
        s.move_selection(Move::Right);
        s.move_selection(Move::Right);
        assert_eq!(at(&s), 3);
        s.move_selection(Move::Down);
        assert_eq!(at(&s), 5, "fila corta: va a la baldosa más cercana");
        s.move_selection(Move::Down);
        assert_eq!(at(&s), 7, "cruza a servicios manteniendo la columna");
        s.move_selection(Move::Down);
        assert_eq!(at(&s), 10);
        s.move_selection(Move::Down);
        assert_eq!(at(&s), 10, "no pasa del final");
        s.move_selection(Move::Up);
        s.move_selection(Move::Up);
        assert_eq!(at(&s), 4);
        s.move_selection(Move::Last);
        assert_eq!(at(&s), 10);
        s.move_selection(Move::First);
        assert_eq!(at(&s), 0);
        s.move_selection(Move::PageDown);
        assert!(at(&s) >= 4);
        s.move_selection(Move::PageUp);
        assert_eq!(at(&s), 0);
    }

    #[test]
    fn movement_is_safe_without_layout_or_apps() {
        let mut empty = State::new(Theme::default());
        empty.move_selection(Move::Down);
        empty.enter_detail();
        assert_eq!(empty.view, View::Grid);

        let mut s = State::new(Theme::default());
        s.set_apps(sample_apps());
        s.move_selection(Move::Down);
        assert_eq!(s.selected_index(), Some(1));
        s.enter_detail();
        assert_eq!(s.view, View::Detail);
        s.back_to_grid();
        assert_eq!((s.view, s.selected_index()), (View::Grid, Some(1)));
    }

    #[test]
    fn selection_survives_refresh_and_moves_when_the_app_dies() {
        let mut s = State::new(Theme::default());
        s.set_apps(sample_apps());
        s.selected = Some("App:Firefox".into());
        s.set_apps(sample_apps());
        assert_eq!(s.selected_index(), Some(1));
        let without: Vec<App> = sample_apps()
            .into_iter()
            .filter(|a| a.name != "Firefox")
            .collect();
        s.set_apps(without);
        assert_eq!(s.selected_app().unwrap().name, "Alacritty");
        s.set_apps(Vec::new());
        assert!(s.selected.is_none() && s.selected_index().is_none());
    }
}
