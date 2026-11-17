//! Estado de la interfaz, independiente del dibujo.

use crate::apps::{App, AppKind};
use crate::icons::GRID_ICON;

use super::grid::GridLayout;
use super::theme::Theme;

pub struct State {
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
