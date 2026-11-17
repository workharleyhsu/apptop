//! Estado de la interfaz, independiente del dibujo.

use std::time::Duration;

use std::collections::HashMap;

use crate::actions::{
    filter_targets, needs_confirmation, Signal, Target, NICE_MAX, NICE_MIN, SIGNAL_MENU,
};
use crate::apps::{App, AppKind, DetailData, Sample};
use crate::collect::SystemStats;
use crate::icons::GRID_ICON;

use super::graph::{History, Point};
use super::grid::GridLayout;
use super::theme::Theme;
use super::tree::{tree_rows, TreeView};

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

/// Criterio de orden dentro de cada sección.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Name,
    Cpu,
    Ram,
}

impl SortKey {
    pub fn next(self) -> Self {
        match self {
            SortKey::Name => SortKey::Cpu,
            SortKey::Cpu => SortKey::Ram,
            SortKey::Ram => SortKey::Name,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "nombre",
            SortKey::Cpu => "CPU",
            SortKey::Ram => "RAM",
        }
    }

    /// Interpreta el valor de configuración (`name`/`nombre`, `cpu`, `ram`).
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "name" | "nombre" => Some(SortKey::Name),
            "cpu" => Some(SortKey::Cpu),
            "ram" | "mem" | "memoria" => Some(SortKey::Ram),
            _ => None,
        }
    }
}

/// Qué secciones se muestran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sections {
    #[default]
    All,
    Apps,
    Services,
}

impl Sections {
    pub fn next(self) -> Self {
        match self {
            Sections::All => Sections::Apps,
            Sections::Apps => Sections::Services,
            Sections::Services => Sections::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Sections::All => "todo",
            Sections::Apps => "apps",
            Sections::Services => "servicios",
        }
    }

    fn shows(self, kind: AppKind) -> bool {
        match self {
            Sections::All => true,
            Sections::Apps => kind == AppKind::App,
            Sections::Services => kind == AppKind::Service,
        }
    }
}

/// Modo de entrada del teclado.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    /// Escribiendo el texto del filtro.
    Filter,
    /// Esperando confirmación (s/n) de una acción.
    Confirm(ActionRequest),
    /// Eligiendo el valor de `nice` a aplicar.
    Nice(NiceRequest),
    /// Eligiendo una señal del menú; el número es la fila del cursor.
    SignalMenu(usize),
}

/// Un cambio de prioridad en preparación o listo para aplicarse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NiceRequest {
    pub targets: Vec<Target>,
    pub label: String,
    pub single_process: bool,
    /// Valores actuales más bajo y más alto entre los procesos.
    pub current: (i64, i64),
    /// Valor elegido.
    pub value: i32,
}

/// Una señal lista para enviarse a un conjunto de procesos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRequest {
    pub signal: Signal,
    pub targets: Vec<Target>,
    /// Nombre de la app, o del proceso si es uno solo.
    pub label: String,
    /// `true` si apunta a un único proceso del árbol y no a toda la app.
    pub single_process: bool,
    /// Procesos dejados afuera para no afectar al propio apptop.
    pub excluded: usize,
}

impl ActionRequest {
    /// Título del diálogo de confirmación.
    pub fn title(&self) -> &'static str {
        match (self.signal, self.single_process) {
            (Signal::SIGTERM, false) => "Cerrar aplicación",
            (Signal::SIGTERM, true) => "Cerrar proceso",
            (Signal::SIGKILL, false) => "Matar aplicación",
            (Signal::SIGKILL, true) => "Matar proceso",
            (_, false) => "Enviar señal a la aplicación",
            (_, true) => "Enviar señal al proceso",
        }
    }
}

/// Aviso breve que se muestra en la barra inferior.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub text: String,
    pub is_error: bool,
    /// Instante (`State::clock`) hasta el que se muestra.
    pub until: f64,
}

/// Segundos que permanece un aviso en pantalla.
pub const MESSAGE_SECS: f64 = 5.0;

/// Límites del intervalo de refresco.
pub const MIN_REFRESH: Duration = Duration::from_millis(250);
pub const MAX_REFRESH: Duration = Duration::from_secs(10);
pub const DEFAULT_REFRESH: Duration = Duration::from_secs(1);

/// Lleva un intervalo al rango permitido.
pub fn clamp_refresh(interval: Duration) -> Duration {
    interval.clamp(MIN_REFRESH, MAX_REFRESH)
}

pub struct State {
    pub view: View,
    /// Intervalo entre muestras.
    pub refresh: Duration,
    pub mode: Mode,
    pub sort: SortKey,
    pub sections: Sections,
    /// Texto del filtro; vacío = sin filtro.
    pub filter: String,
    /// Apps de la última muestra.
    pub apps: Vec<App>,
    /// Totales del sistema de la última muestra.
    pub system: SystemStats,
    pub ticks_per_second: f64,
    /// Mediciones de la app abierta en detalle.
    pub detail: Option<DetailData>,
    /// Historial de CPU y RAM de cada app (por id), para los gráficos.
    pub history: HashMap<String, History>,
    /// Segundos transcurridos al tomar la última muestra.
    pub clock: f64,
    started: std::time::Instant,
    /// apptop y sus ancestros (pid propio primero): nunca se les envían
    /// señales que congelarían o matarían la interfaz.
    pub lineage: Vec<i32>,
    /// Acción confirmada que el bucle principal debe ejecutar.
    pub pending: Option<ActionRequest>,
    /// Cambio de prioridad confirmado que el bucle debe aplicar.
    pub pending_nice: Option<NiceRequest>,
    pub message: Option<Message>,
    /// Proceso elegido en el árbol del detalle; `None` = toda la app.
    pub proc_selected: Option<i32>,
    /// Dónde quedó dibujado el árbol de procesos.
    pub tree: TreeView,
    /// Nombres de usuario ya resueltos (uid → nombre).
    pub users: HashMap<u32, String>,
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
            refresh: DEFAULT_REFRESH,
            mode: Mode::Normal,
            sort: SortKey::default(),
            sections: Sections::default(),
            filter: String::new(),
            apps: Vec::new(),
            system: SystemStats::default(),
            ticks_per_second: 100.0,
            detail: None,
            lineage: Vec::new(),
            pending: None,
            pending_nice: None,
            message: None,
            history: HashMap::new(),
            clock: 0.0,
            started: std::time::Instant::now(),
            proc_selected: None,
            tree: TreeView::default(),
            users: HashMap::new(),
            order: Vec::new(),
            selected: None,
            scroll: 0,
            icon_size: GRID_ICON,
            theme,
            layout: GridLayout::default(),
        }
    }

    /// Incorpora una muestra completa (apps + sistema).
    pub fn apply(&mut self, sample: Sample) {
        self.system = sample.system;
        if sample.ticks_per_second > 0.0 {
            self.ticks_per_second = sample.ticks_per_second;
        }
        self.detail = sample.detail;
        self.set_apps(sample.apps);
        self.record_history(self.started.elapsed().as_secs_f64());
    }

    /// Anota en el historial el estado actual de cada app, en el instante
    /// `now` (segundos), y olvida las apps que ya no existen.
    pub fn record_history(&mut self, now: f64) {
        self.clock = now;
        for app in &self.apps {
            self.history.entry(app.id.clone()).or_default().push(Point {
                at: now,
                cpu_pct: app.cpu_pct,
                rss: app.rss,
            });
        }
        self.history.retain(|_, h| h.last_at() == Some(now));
    }

    /// Reemplaza las apps por una muestra nueva conservando la selección.
    pub fn set_apps(&mut self, apps: Vec<App>) {
        self.apps = apps;
        self.reorder();
    }

    /// Recalcula qué apps se muestran y en qué orden: filtro de texto,
    /// secciones visibles, y dentro de cada sección el criterio elegido.
    /// Las apps van siempre antes que los servicios.
    pub fn reorder(&mut self) {
        let needle = self.filter.trim().to_lowercase();
        let mut order: Vec<usize> = (0..self.apps.len())
            .filter(|i| {
                let app = &self.apps[*i];
                self.sections.shows(app.kind) && (needle.is_empty() || matches(app, &needle))
            })
            .collect();
        let sort = self.sort;
        order.sort_by(|a, b| {
            let (a, b) = (&self.apps[*a], &self.apps[*b]);
            let by_name = || (a.name.to_lowercase(), &a.id).cmp(&(b.name.to_lowercase(), &b.id));
            let by_key = match sort {
                SortKey::Name => std::cmp::Ordering::Equal,
                SortKey::Cpu => b.cpu_pct.total_cmp(&a.cpu_pct),
                SortKey::Ram => b.rss.cmp(&a.rss),
            };
            a.kind.cmp(&b.kind).then(by_key).then_with(by_name)
        });
        self.order = order;
        let still_there = self
            .selected
            .as_ref()
            .is_some_and(|id| self.order.iter().any(|i| &self.apps[*i].id == id));
        if !still_there {
            self.selected = self.order.first().map(|i| self.apps[*i].id.clone());
        }
        // El proceso elegido en el árbol pudo haber terminado.
        if let Some(pid) = self.proc_selected {
            let alive = self
                .selected_app()
                .is_some_and(|a| a.procs.iter().any(|p| p.pid == pid));
            if !alive {
                self.proc_selected = None;
            }
        }
    }

    /// Hace el refresco más lento (`slower`) o más rápido, de a 250 ms.
    pub fn adjust_refresh(&mut self, slower: bool) {
        let step = Duration::from_millis(250);
        let next = if slower {
            self.refresh.saturating_add(step)
        } else {
            self.refresh.saturating_sub(step)
        };
        self.refresh = clamp_refresh(next);
    }

    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        self.reorder();
    }

    pub fn cycle_sections(&mut self) {
        self.sections = self.sections.next();
        self.reorder();
    }

    /// Empieza a escribir un filtro (conserva el texto anterior).
    pub fn start_filter(&mut self) {
        self.mode = Mode::Filter;
    }

    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.reorder();
    }

    pub fn filter_pop(&mut self) {
        self.filter.pop();
        self.reorder();
    }

    /// Termina de escribir y deja el filtro aplicado.
    pub fn accept_filter(&mut self) {
        self.mode = Mode::Normal;
    }

    /// Borra el filtro y vuelve al modo normal.
    pub fn clear_filter(&mut self) {
        self.mode = Mode::Normal;
        self.filter.clear();
        self.reorder();
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

    /// Id de la app cuyo detalle hay que medir en la próxima muestra.
    pub fn detail_target(&self) -> Option<&str> {
        match self.view {
            View::Detail => self.selected.as_deref(),
            View::Grid => None,
        }
    }

    /// Mediciones de detalle, solo si son de la app seleccionada.
    pub fn detail_of_selected(&self) -> Option<&DetailData> {
        let id = self.selected.as_ref()?;
        self.detail.as_ref().filter(|d| &d.app_id == id)
    }

    /// Nombre del usuario `uid`; consulta al sistema una sola vez por uid.
    pub fn user_name(&mut self, uid: u32) -> String {
        self.users
            .entry(uid)
            .or_insert_with(|| {
                nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid))
                    .ok()
                    .flatten()
                    .map(|u| u.name)
                    .unwrap_or_else(|| uid.to_string())
            })
            .clone()
    }

    /// Procesos sobre los que actúa una acción ahora mismo: el proceso
    /// elegido en el árbol del detalle, o todos los de la app seleccionada.
    /// Devuelve (objetivos, nombre a mostrar, ¿es un solo proceso?).
    pub fn action_scope(&self) -> Option<(Vec<Target>, String, bool)> {
        let app = self.selected_app()?;
        let target = |p: &crate::collect::ProcInfo| Target {
            pid: p.pid,
            starttime: p.starttime,
        };
        let chosen = match (self.view, self.proc_selected) {
            (View::Detail, Some(pid)) => app.procs.iter().find(|p| p.pid == pid),
            _ => None,
        };
        Some(match chosen {
            Some(proc) => (
                vec![target(proc)],
                format!("{} (pid {})", proc.name, proc.pid),
                true,
            ),
            None => (
                app.procs.iter().map(target).collect(),
                app.name.clone(),
                false,
            ),
        })
    }

    /// Prepara el envío de `signal`. Devuelve `None` (y deja un aviso) si no
    /// queda ningún proceso al que enviarla.
    pub fn build_request(&mut self, signal: Signal) -> Option<ActionRequest> {
        let (targets, label, single_process) = self.action_scope()?;
        let (targets, excluded) = filter_targets(&targets, signal, &self.lineage);
        if targets.is_empty() {
            self.notify(
                "No se puede: afectaría al propio apptop o a su terminal",
                true,
            );
            return None;
        }
        Some(ActionRequest {
            signal,
            targets,
            label,
            single_process,
            excluded,
        })
    }

    /// ¿Están detenidos todos los procesos sobre los que actuaría una acción?
    pub fn scope_is_paused(&self) -> bool {
        let Some(app) = self.selected_app() else {
            return false;
        };
        let chosen = match (self.view, self.proc_selected) {
            (View::Detail, Some(pid)) => Some(pid),
            _ => None,
        };
        let mut procs = app
            .procs
            .iter()
            .filter(|p| chosen.is_none_or(|pid| p.pid == pid))
            .peekable();
        procs.peek().is_some() && procs.all(|p| is_stopped(p.state))
    }

    /// Pausa (SIGSTOP) o reanuda (SIGCONT) según el estado actual. No pide
    /// confirmación: es reversible con la misma tecla.
    pub fn toggle_pause(&mut self) {
        let signal = if self.scope_is_paused() {
            Signal::SIGCONT
        } else {
            Signal::SIGSTOP
        };
        if let Some(request) = self.build_request(signal) {
            self.pending = Some(request);
        }
    }

    /// Abre el menú de señales (si hay algo a lo que enviarlas).
    pub fn open_signal_menu(&mut self) {
        if self.action_scope().is_some() {
            self.mode = Mode::SignalMenu(0);
        }
    }

    /// Mueve el cursor del menú de señales.
    pub fn signal_menu_move(&mut self, delta: isize) {
        if let Mode::SignalMenu(cursor) = &mut self.mode {
            let last = SIGNAL_MENU.len() as isize - 1;
            *cursor = (*cursor as isize).saturating_add(delta).clamp(0, last) as usize;
        }
    }

    /// Elige la señal bajo el cursor: las peligrosas pasan por el diálogo
    /// de confirmación; pausar y reanudar se envían directamente.
    pub fn signal_menu_choose(&mut self) {
        let Mode::SignalMenu(cursor) = std::mem::take(&mut self.mode) else {
            return;
        };
        let Some((signal, _)) = SIGNAL_MENU.get(cursor) else {
            return;
        };
        if needs_confirmation(*signal) {
            self.ask_signal(*signal);
        } else if let Some(request) = self.build_request(*signal) {
            self.pending = Some(request);
        }
    }

    /// Cierra el menú de señales sin enviar nada.
    pub fn close_signal_menu(&mut self) {
        if matches!(self.mode, Mode::SignalMenu(_)) {
            self.mode = Mode::Normal;
        }
    }

    /// Abre el selector de prioridad para el alcance actual.
    pub fn ask_nice(&mut self) {
        let Some((targets, label, single_process)) = self.action_scope() else {
            return;
        };
        let Some(app) = self.selected_app() else {
            return;
        };
        let values: Vec<i64> = app
            .procs
            .iter()
            .filter(|p| targets.iter().any(|t| t.pid == p.pid))
            .map(|p| p.nice)
            .collect();
        let (Some(low), Some(high)) = (values.iter().min(), values.iter().max()) else {
            return;
        };
        self.mode = Mode::Nice(NiceRequest {
            targets,
            label,
            single_process,
            current: (*low, *high),
            value: (*high as i32).clamp(NICE_MIN, NICE_MAX),
        });
    }

    /// Cambia el valor elegido en el selector de prioridad.
    pub fn nice_adjust(&mut self, delta: i32) {
        if let Mode::Nice(request) = &mut self.mode {
            request.value = (request.value + delta).clamp(NICE_MIN, NICE_MAX);
        }
    }

    /// Cierra el selector; con `apply` el cambio queda listo para ejecutar.
    pub fn nice_finish(&mut self, apply: bool) {
        if let Mode::Nice(request) = std::mem::take(&mut self.mode) {
            if apply {
                self.pending_nice = Some(request);
            } else {
                self.notify("Cancelado", false);
            }
        }
    }

    /// Pide confirmación para enviar `signal` (cerrar, matar).
    pub fn ask_signal(&mut self, signal: Signal) {
        if let Some(request) = self.build_request(signal) {
            self.mode = Mode::Confirm(request);
        }
    }

    /// Respuesta al diálogo: con `true` la acción queda lista para ejecutar.
    pub fn answer_confirm(&mut self, yes: bool) {
        if let Mode::Confirm(request) = std::mem::take(&mut self.mode) {
            if yes {
                self.pending = Some(request);
            } else {
                self.notify("Cancelado", false);
            }
        }
    }

    /// Muestra un aviso durante unos segundos.
    pub fn notify(&mut self, text: impl Into<String>, is_error: bool) {
        self.message = Some(Message {
            text: text.into(),
            is_error,
            until: self.clock + MESSAGE_SECS,
        });
    }

    /// Aviso vigente, si lo hay.
    pub fn current_message(&self) -> Option<&Message> {
        self.message.as_ref().filter(|m| self.clock <= m.until)
    }

    /// Mueve el cursor del árbol de procesos `delta` filas. Por encima de la
    /// primera fila no hay proceso elegido: las acciones valen para toda la app.
    pub fn move_proc_cursor(&mut self, delta: isize) {
        let Some(app) = self.selected_app() else {
            return;
        };
        let rows = tree_rows(app);
        if rows.is_empty() {
            self.proc_selected = None;
            return;
        }
        // -1 representa "toda la app".
        let current = self
            .proc_selected
            .and_then(|pid| rows.iter().position(|r| r.pid == pid))
            .map_or(-1, |i| i as isize);
        let target = current
            .saturating_add(delta)
            .clamp(-1, rows.len() as isize - 1);
        self.proc_selected = (target >= 0).then(|| rows[target as usize].pid);
    }

    /// Elige la fila `index` del árbol (clic del mouse).
    pub fn select_proc_row(&mut self, index: usize) {
        if let Some(app) = self.selected_app() {
            self.proc_selected = tree_rows(app).get(index).map(|r| r.pid);
        }
    }

    /// Entra al detalle de la app seleccionada, si hay alguna.
    pub fn enter_detail(&mut self) {
        if self.selected_app().is_some() {
            self.view = View::Detail;
            self.proc_selected = None;
            self.tree = TreeView::default();
        }
    }

    /// Vuelve a la grilla conservando la selección.
    pub fn back_to_grid(&mut self) {
        self.view = View::Grid;
    }
}

/// Estados del kernel que indican un proceso detenido por una señal.
pub fn is_stopped(state: char) -> bool {
    matches!(state, 'T' | 't')
}

/// ¿Está pausada la app entera (todos sus procesos detenidos)?
pub fn app_is_paused(app: &App) -> bool {
    !app.procs.is_empty() && app.procs.iter().all(|p| is_stopped(p.state))
}

/// ¿La app coincide con el texto (ya en minúsculas)? Busca en el nombre
/// visible, el AppID y los nombres de sus procesos.
fn matches(app: &App, needle: &str) -> bool {
    app.name.to_lowercase().contains(needle)
        || app
            .app_id
            .as_ref()
            .is_some_and(|id| id.to_lowercase().contains(needle))
        || app
            .procs
            .iter()
            .any(|p| p.name.to_lowercase().contains(needle))
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

    fn names(s: &State) -> Vec<&str> {
        s.visible().map(|a| a.name.as_str()).collect()
    }

    #[test]
    fn sorts_within_each_section() {
        let mut s = State::new(Theme::default());
        s.set_apps(sample_apps());
        s.cycle_sort();
        assert_eq!(s.sort, SortKey::Cpu);
        assert_eq!(
            names(&s),
            vec![
                "Un nombre larguísimo de app",
                "Firefox",
                "Alacritty",
                "pipewire",
                "docker"
            ]
        );
        s.cycle_sort();
        assert_eq!(s.sort, SortKey::Ram);
        assert_eq!(
            names(&s),
            vec![
                "Firefox",
                "Alacritty",
                "Un nombre larguísimo de app",
                "docker",
                "pipewire"
            ]
        );
        s.cycle_sort();
        assert_eq!(s.sort, SortKey::Name);
        assert_eq!(names(&s)[0], "Alacritty");
    }

    #[test]
    fn filters_by_text_and_keeps_a_valid_selection() {
        let mut s = State::new(Theme::default());
        s.set_apps(sample_apps());
        s.selected = Some("Service:docker".into());
        s.start_filter();
        assert_eq!(s.mode, Mode::Filter);
        for c in "FIRE".chars() {
            s.filter_push(c);
        }
        assert_eq!(names(&s), vec!["Firefox"]);
        assert_eq!(s.selected_app().unwrap().name, "Firefox");
        for _ in 0..4 {
            s.filter_pop();
        }
        assert_eq!(names(&s).len(), 5, "filtro vacío = sin filtro");
        s.filter_push('I');
        s.filter_push('r');
        assert_eq!(names(&s), vec!["Firefox", "pipewire"], "'ir' está en ambos");
        s.accept_filter();
        assert_eq!((s.mode.clone(), s.filter.as_str()), (Mode::Normal, "Ir"));
        // El filtro sigue aplicado tras un refresco.
        s.set_apps(sample_apps());
        assert_eq!(names(&s).len(), 2);
        s.clear_filter();
        assert_eq!(names(&s).len(), 5);

        s.filter_push('ñ');
        assert!(names(&s).is_empty() && s.selected.is_none());
    }

    #[test]
    fn filter_also_matches_process_names_and_app_ids() {
        let mut s = State::new(Theme::default());
        let mut browser = app("Navegador", AppKind::App, 0.0, 1);
        browser.app_id = Some("org.mozilla.firefox".into());
        let mut term = app("Terminal", AppKind::App, 0.0, 1);
        term.procs
            .push(crate::apps::group::tests::proc(1, 0, "/usr/bin/fish", "/"));
        s.set_apps(vec![browser, term]);
        s.filter = "mozilla".into();
        s.reorder();
        assert_eq!(names(&s), vec!["Navegador"]);
        s.filter = "fish".into();
        s.reorder();
        assert_eq!(names(&s), vec!["Terminal"]);
    }

    #[test]
    fn cycles_visible_sections() {
        let mut s = State::new(Theme::default());
        s.set_apps(sample_apps());
        s.cycle_sections();
        assert_eq!((s.sections, s.count(AppKind::Service)), (Sections::Apps, 0));
        assert_eq!(names(&s).len(), 3);
        s.cycle_sections();
        assert_eq!(names(&s), vec!["docker", "pipewire"]);
        assert_eq!(s.selected_app().unwrap().name, "docker");
        s.cycle_sections();
        assert_eq!(names(&s).len(), 5);
    }

    #[test]
    fn process_cursor_moves_through_the_tree_and_back_to_the_whole_app() {
        use crate::apps::group::tests::proc;
        let mut s = State::new(Theme::default());
        let mut a = app("x", AppKind::App, 0.0, 1);
        a.procs = vec![
            proc(10, 1, "/b/a", "/"),
            proc(11, 10, "/b/a", "/"),
            proc(12, 10, "/b/a", "/"),
        ];
        s.set_apps(vec![a.clone()]);
        s.enter_detail();
        assert_eq!(s.proc_selected, None);
        s.move_proc_cursor(1);
        assert_eq!(s.proc_selected, Some(10));
        s.move_proc_cursor(5);
        assert_eq!(s.proc_selected, Some(12), "se detiene en la última fila");
        s.move_proc_cursor(-1);
        assert_eq!(s.proc_selected, Some(11));
        s.select_proc_row(2);
        assert_eq!(s.proc_selected, Some(12));
        // El proceso elegido muere en el siguiente refresco.
        a.procs.pop();
        s.set_apps(vec![a]);
        assert_eq!(s.proc_selected, None);
        s.move_proc_cursor(-3);
        assert_eq!(s.proc_selected, None);
    }

    #[test]
    fn history_tracks_every_app_and_forgets_dead_ones() {
        let mut s = State::new(Theme::default());
        s.set_apps(sample_apps());
        s.record_history(1.0);
        s.record_history(2.0);
        assert_eq!(s.history.len(), 5);
        assert_eq!(s.history["App:Firefox"].len(), 2);
        s.set_apps(
            sample_apps()
                .into_iter()
                .filter(|a| a.name != "Firefox")
                .collect(),
        );
        s.record_history(3.0);
        assert!(!s.history.contains_key("App:Firefox"));
        assert_eq!(s.history["Service:docker"].len(), 3);
        assert_eq!(s.clock, 3.0);
    }

    fn state_with_procs() -> State {
        use crate::apps::group::tests::proc;
        let mut s = State::new(Theme::default());
        let mut a = app("Editor", AppKind::App, 0.0, 1);
        a.procs = vec![
            proc(10, 1, "/b/ed", "/"),
            proc(11, 10, "/b/helper", "/"),
            proc(12, 10, "/b/ed", "/"),
        ];
        s.set_apps(vec![a]);
        s
    }

    #[test]
    fn close_asks_first_and_only_queues_after_yes() {
        let mut s = state_with_procs();
        s.ask_signal(Signal::SIGTERM);
        let Mode::Confirm(request) = s.mode.clone() else {
            panic!("debe pedir confirmación");
        };
        assert_eq!(request.title(), "Cerrar aplicación");
        assert_eq!(
            (request.label.as_str(), request.targets.len()),
            ("Editor", 3)
        );
        assert!(s.pending.is_none(), "nada se ejecuta antes de confirmar");

        s.answer_confirm(false);
        assert!(s.pending.is_none() && s.mode == Mode::Normal);
        assert_eq!(s.current_message().unwrap().text, "Cancelado");

        s.ask_signal(Signal::SIGKILL);
        s.answer_confirm(true);
        let pending = s.pending.take().unwrap();
        assert_eq!(
            (pending.signal, pending.targets.len()),
            (Signal::SIGKILL, 3)
        );
        assert_eq!(
            pending.targets[0],
            Target {
                pid: 10,
                starttime: 10
            }
        );
    }

    #[test]
    fn actions_target_the_chosen_process_only_inside_detail() {
        let mut s = state_with_procs();
        s.proc_selected = Some(11);
        // En la grilla el cursor de proceso no cuenta.
        assert_eq!(s.action_scope().unwrap().0.len(), 3);
        s.enter_detail();
        s.move_proc_cursor(2);
        let (targets, label, single) = s.action_scope().unwrap();
        assert_eq!(
            targets,
            vec![Target {
                pid: 11,
                starttime: 11
            }]
        );
        assert_eq!((label.as_str(), single), ("helper (pid 11)", true));
        s.ask_signal(Signal::SIGKILL);
        let Mode::Confirm(request) = &s.mode else {
            panic!()
        };
        assert_eq!(request.title(), "Matar proceso");
    }

    #[test]
    fn apptop_and_its_terminal_are_never_targets() {
        let mut s = state_with_procs();
        s.lineage = vec![12, 10, 1];
        // Cerrar: se excluye solo apptop (pid 12).
        s.ask_signal(Signal::SIGTERM);
        let Mode::Confirm(request) = std::mem::take(&mut s.mode) else {
            panic!()
        };
        assert_eq!((request.targets.len(), request.excluded), (2, 1));
        // Pausar: también su ancestro (pid 10).
        let request = s.build_request(Signal::SIGSTOP).unwrap();
        assert_eq!(
            request.targets,
            vec![Target {
                pid: 11,
                starttime: 11
            }]
        );
        assert_eq!(request.excluded, 2);
        // Si no queda nadie, no hay diálogo sino un aviso de error.
        s.enter_detail();
        s.proc_selected = Some(12);
        s.ask_signal(Signal::SIGTERM);
        assert_eq!(s.mode, Mode::Normal);
        assert!(s.current_message().unwrap().is_error);
    }

    #[test]
    fn pause_toggles_between_stop_and_continue_without_asking() {
        let mut s = state_with_procs();
        s.lineage = vec![999];
        assert!(!s.scope_is_paused());
        s.toggle_pause();
        assert_eq!(s.mode, Mode::Normal, "pausar no pide confirmación");
        let request = s.pending.take().unwrap();
        assert_eq!(
            (request.signal, request.targets.len()),
            (Signal::SIGSTOP, 3)
        );

        // Con todos detenidos, la misma tecla reanuda.
        s.apps[0].procs.iter_mut().for_each(|p| p.state = 'T');
        assert!(s.scope_is_paused() && app_is_paused(&s.apps[0]));
        s.toggle_pause();
        assert_eq!(s.pending.take().unwrap().signal, Signal::SIGCONT);

        // Con uno solo corriendo, la app no cuenta como pausada...
        s.apps[0].procs[1].state = 'S';
        assert!(!s.scope_is_paused() && !app_is_paused(&s.apps[0]));
        // ...pero en el detalle se mira solo el proceso elegido.
        s.enter_detail();
        s.proc_selected = Some(10);
        assert!(s.scope_is_paused());
        s.toggle_pause();
        let request = s.pending.take().unwrap();
        assert_eq!(
            (
                request.signal,
                request.targets.len(),
                request.single_process
            ),
            (Signal::SIGCONT, 1, true)
        );
        s.proc_selected = Some(11);
        s.toggle_pause();
        assert_eq!(s.pending.take().unwrap().signal, Signal::SIGSTOP);
    }

    #[test]
    fn pausing_never_freezes_apptop_or_its_terminal() {
        let mut s = state_with_procs();
        // apptop es el pid 12, hijo del 10: en la app solo queda pausable el 11.
        s.lineage = vec![12, 10, 1];
        s.toggle_pause();
        let request = s.pending.take().unwrap();
        assert_eq!(
            request.targets,
            vec![Target {
                pid: 11,
                starttime: 11
            }]
        );
        assert_eq!(request.excluded, 2);
        // Y si todo lo que queda es apptop y sus ancestros, no se envía nada.
        s.apps[0].procs.remove(1);
        s.toggle_pause();
        assert!(s.pending.is_none());
        assert!(s.current_message().unwrap().is_error);
        assert!(!app_is_paused(&app("vacía", AppKind::App, 0.0, 0)));
    }

    #[test]
    fn nice_dialog_starts_at_the_current_value_and_clamps() {
        let mut s = state_with_procs();
        s.apps[0].procs[0].nice = 0;
        s.apps[0].procs[1].nice = 5;
        s.apps[0].procs[2].nice = -3;
        s.ask_nice();
        let Mode::Nice(request) = s.mode.clone() else {
            panic!("debe abrir el selector");
        };
        assert_eq!(
            (request.current, request.value, request.targets.len()),
            ((-3, 5), 5, 3)
        );
        s.nice_adjust(1);
        s.nice_adjust(100);
        assert!(matches!(&s.mode, Mode::Nice(r) if r.value == 19));
        s.nice_adjust(-100);
        assert!(matches!(&s.mode, Mode::Nice(r) if r.value == -20));
        s.nice_adjust(30);
        assert!(s.pending_nice.is_none(), "nada se aplica hasta Enter");
        s.nice_finish(true);
        let applied = s.pending_nice.take().unwrap();
        assert_eq!((applied.value, applied.label.as_str()), (10, "Editor"));

        s.ask_nice();
        s.nice_finish(false);
        assert!(s.pending_nice.is_none() && s.mode == Mode::Normal);
        // En el detalle, sobre un solo proceso.
        s.enter_detail();
        s.proc_selected = Some(12);
        s.ask_nice();
        assert!(
            matches!(&s.mode, Mode::Nice(r) if r.single_process && r.current == (-3, -3) && r.value == -3)
        );
    }

    #[test]
    fn signal_menu_confirms_dangerous_signals_and_sends_pause_directly() {
        let mut s = state_with_procs();
        s.open_signal_menu();
        assert_eq!(s.mode, Mode::SignalMenu(0));
        s.signal_menu_move(-3);
        assert_eq!(s.mode, Mode::SignalMenu(0));
        s.signal_menu_move(2);
        s.signal_menu_choose();
        // SIGINT: pasa por confirmación, con título genérico.
        let Mode::Confirm(request) = s.mode.clone() else {
            panic!("debe pedir confirmación");
        };
        assert_eq!(
            (request.signal, request.title()),
            (Signal::SIGINT, "Enviar señal a la aplicación")
        );
        s.answer_confirm(true);
        assert_eq!(s.pending.take().unwrap().signal, Signal::SIGINT);

        // SIGCONT (fila 6): directo, sin diálogo.
        s.open_signal_menu();
        s.signal_menu_move(6);
        s.signal_menu_choose();
        assert_eq!(s.mode, Mode::Normal);
        assert_eq!(s.pending.take().unwrap().signal, Signal::SIGCONT);

        // El cursor no pasa del final y Esc no envía nada.
        s.open_signal_menu();
        s.signal_menu_move(500);
        assert_eq!(s.mode, Mode::SignalMenu(SIGNAL_MENU.len() - 1));
        s.close_signal_menu();
        assert!(s.mode == Mode::Normal && s.pending.is_none());
    }

    #[test]
    fn every_action_can_target_a_single_process_of_the_tree() {
        let mut s = state_with_procs();
        s.enter_detail();
        s.move_proc_cursor(3); // tercera fila del árbol: pid 12
        let only = vec![Target {
            pid: 12,
            starttime: 12,
        }];

        s.ask_signal(Signal::SIGTERM);
        assert!(matches!(&s.mode, Mode::Confirm(r) if r.targets == only && r.single_process));
        s.answer_confirm(true);
        assert_eq!(s.pending.take().unwrap().label, "ed (pid 12)");

        s.toggle_pause();
        assert_eq!(s.pending.take().unwrap().targets, only);

        s.ask_nice();
        assert!(matches!(&s.mode, Mode::Nice(r) if r.targets == only));
        s.nice_finish(true);
        assert_eq!(s.pending_nice.take().unwrap().targets, only);

        s.open_signal_menu();
        s.signal_menu_move(1);
        s.signal_menu_choose();
        assert!(
            matches!(&s.mode, Mode::Confirm(r) if r.signal == Signal::SIGKILL && r.targets == only)
        );
        assert!(matches!(&s.mode, Mode::Confirm(r) if r.title() == "Matar proceso"));
        s.answer_confirm(false);

        // Volver a "toda la app": el alcance vuelve a ser los tres procesos.
        s.move_proc_cursor(-10);
        s.ask_signal(Signal::SIGTERM);
        assert!(matches!(&s.mode, Mode::Confirm(r) if r.targets.len() == 3 && !r.single_process));
    }

    #[test]
    fn messages_expire() {
        let mut s = State::new(Theme::default());
        s.clock = 10.0;
        s.notify("hola", false);
        assert!(s.current_message().is_some());
        s.clock = 10.0 + MESSAGE_SECS + 0.1;
        assert!(s.current_message().is_none());
        s.ask_signal(Signal::SIGTERM);
        assert_eq!(s.mode, Mode::Normal, "sin app seleccionada no pasa nada");
    }

    #[test]
    fn refresh_interval_adjusts_within_limits() {
        let mut s = State::new(Theme::default());
        assert_eq!(s.refresh, Duration::from_secs(1));
        s.adjust_refresh(false);
        assert_eq!(s.refresh, Duration::from_millis(750));
        for _ in 0..10 {
            s.adjust_refresh(false);
        }
        assert_eq!(s.refresh, MIN_REFRESH);
        for _ in 0..100 {
            s.adjust_refresh(true);
        }
        assert_eq!(s.refresh, MAX_REFRESH);
        assert_eq!(clamp_refresh(Duration::ZERO), MIN_REFRESH);
        assert_eq!(
            clamp_refresh(Duration::from_secs(3)),
            Duration::from_secs(3)
        );
    }

    #[test]
    fn parses_sort_keys() {
        assert_eq!(SortKey::parse("CPU"), Some(SortKey::Cpu));
        assert_eq!(SortKey::parse(" nombre "), Some(SortKey::Name));
        assert_eq!(SortKey::parse("ram"), Some(SortKey::Ram));
        assert_eq!(SortKey::parse("otro"), None);
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
