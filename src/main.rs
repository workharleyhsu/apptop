use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use clap::Parser;

use apptop::apps::{group_apps_with, AppKind, DesktopIndex, Sample, Sampler};
use apptop::collect::Collector;
use apptop::config::Config;
use apptop::icons::{BrailleIcon, GenericKind, IconLocator, DETAIL_ICON};
use apptop::ui::format::human_bytes;
use apptop::ui::state::clamp_refresh;
use apptop::ui::{render_to_text, RunOptions};

/// Monitor de recursos para terminal centrado en aplicaciones.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Imprime las apps detectadas, sus PIDs y recursos en texto plano.
    #[arg(long)]
    list: bool,

    /// Imprime el icono de una app convertido a braille. Acepta el nombre de
    /// una app, el id de un `.desktop`, un nombre de icono, una ruta, o
    /// `generic:service` / `generic:app` para los iconos genéricos.
    #[arg(long, value_name = "APP")]
    dump_icon: Option<String>,

    /// Con `--dump-icon`: usa el tamaño grande de la vista de detalle.
    #[arg(long)]
    large: bool,

    /// Fuerza la salida con color (`always`), sin color (`never`) o según
    /// la terminal (`auto`).
    #[arg(long, value_name = "MODO", default_value = "auto")]
    color: String,

    /// Segundos entre refrescos (0.25 a 10).
    #[arg(long, short = 'r', value_name = "SEGUNDOS")]
    refresh: Option<f64>,

    /// Archivo de configuración (por defecto ~/.config/apptop/config.toml).
    #[arg(long, value_name = "ARCHIVO")]
    config: Option<PathBuf>,

    /// Imprime la configuración efectiva en formato TOML y termina.
    #[arg(long)]
    print_config: bool,

    /// Renderiza un cuadro de la interfaz a texto y termina.
    #[arg(long)]
    snapshot: bool,

    /// Con `--snapshot`: muestra el detalle de esta app en vez de la grilla.
    #[arg(long, value_name = "NOMBRE")]
    app: Option<String>,

    /// Con `--snapshot`: tamaño del cuadro, por ejemplo `120x40`.
    #[arg(long, value_name = "ANCHOxALTO", default_value = "100x30")]
    size: String,

    /// Provoca un panic tras el primer cuadro (prueba de restauración).
    #[arg(long, hide = true)]
    panic_test: bool,

    /// Imprime todos los procesos leídos (depuración).
    #[arg(long)]
    list_procs: bool,
}

fn main() {
    let cli = Cli::parse();

    // Configuración: archivo (opcional) y encima las opciones de la línea
    // de comandos. Las advertencias no impiden arrancar.
    let (mut config, warnings) = Config::load(cli.config.as_deref());
    for warning in &warnings {
        eprintln!("apptop: {warning}");
    }
    if let Some(seconds) = cli.refresh.filter(|s| s.is_finite() && *s > 0.0) {
        config.refresh = clamp_refresh(Duration::from_secs_f64(seconds.min(3600.0)));
    }
    if cli.print_config {
        print!("{}", config.to_toml());
        return;
    }
    let mut options = RunOptions::from_config(&config);
    options.panic_test = cli.panic_test;

    if cli.list {
        list_apps(&options);
        return;
    }
    if let Some(target) = &cli.dump_icon {
        let size = if cli.large {
            DETAIL_ICON
        } else {
            options.icon_size
        };
        std::process::exit(dump_icon(target, size, &cli.color, &options));
    }
    if cli.snapshot {
        let (width, height) = parse_size(&cli.size).unwrap_or((100, 30));
        match snapshot(width, height, cli.app.as_deref(), &options) {
            Ok(text) => print!("{text}"),
            Err(message) => {
                eprintln!("apptop: {message}");
                std::process::exit(1);
            }
        }
        return;
    }
    if cli.list_procs {
        list_procs();
        return;
    }
    if !std::io::stdout().is_terminal() {
        eprintln!("apptop: la salida no es una terminal; usá --list, --snapshot o --dump-icon");
        std::process::exit(2);
    }
    if let Err(err) = apptop::ui::run(&options) {
        eprintln!("apptop: {err}");
        std::process::exit(1);
    }
}

fn list_procs() {
    let mut collector = Collector::new();
    collector.sample();
    sleep(Duration::from_millis(250));
    println!(
        "{:>8} {:>8} {:1} {:>6} {:>10} {:>4} {:<16} EXE",
        "PID", "PPID", "S", "CPU%", "RSS(kB)", "THR", "NOMBRE"
    );
    for p in collector.sample() {
        let exe = p
            .exe
            .as_ref()
            .map(|e| e.display().to_string())
            .unwrap_or_else(|| "n/d".into());
        println!(
            "{:>8} {:>8} {:1} {:>6.1} {:>10} {:>4} {:<16} {}",
            p.pid,
            p.ppid,
            p.state,
            p.cpu_pct,
            p.rss / 1024,
            p.threads,
            p.name,
            exe
        );
    }
}

fn list_apps(options: &RunOptions) {
    let apps = sample_once().apps;
    let store = options.new_store();
    let locator = store.locator();
    println!("Tema de iconos: {}\n", locator.theme());
    for (kind, title) in [
        (AppKind::App, "APLICACIONES"),
        (AppKind::Service, "SERVICIOS"),
    ] {
        let section: Vec<_> = apps.iter().filter(|a| a.kind == kind).collect();
        println!("== {title} ({}) ==", section.len());
        println!(
            "{:<28} {:<13} {:>5} {:>6} {:>9}  PIDS / ICONO",
            "NOMBRE", "TIPO", "PROCS", "CPU%", "RAM"
        );
        for app in section {
            let pids: Vec<String> = app.procs.iter().map(|p| p.pid.to_string()).collect();
            println!(
                "{:<28} {:<13} {:>5} {:>6.1} {:>9}  {}",
                truncate(&app.name, 28),
                app.origin.label(),
                app.procs.len(),
                app.cpu_pct,
                human_bytes(app.rss),
                pids.join(",")
            );
            if let Some(icon) = app.desktop.as_ref().and_then(|d| d.icon.as_deref()) {
                let path = locator
                    .locate(icon)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "n/d".into());
                println!("{:<28} icono: {icon} -> {path}", "");
            }
        }
        println!();
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// Resuelve el argumento de `--dump-icon` a un archivo de icono.
fn resolve_icon(target: &str, locator: &IconLocator) -> Option<PathBuf> {
    let direct = Path::new(target);
    if direct.is_file() {
        return Some(direct.to_path_buf());
    }
    let index = DesktopIndex::load();
    let wanted = target.to_lowercase();
    // 1. Una app en ejecución con ese nombre.
    let apps = group_apps_with(Collector::new().sample(), Some(&index));
    let running = apps
        .iter()
        .filter(|a| a.name.to_lowercase() == wanted || a.app_id.as_deref() == Some(target))
        .find_map(|a| a.desktop.as_ref()?.icon.clone());
    // 2. Un `.desktop` instalado; 3. el nombre de icono tal cual.
    let icon = running
        .or_else(|| index.by_app_id(target).and_then(|d| d.icon.clone()))
        .or_else(|| index.by_exec(target).and_then(|d| d.icon.clone()))
        .unwrap_or_else(|| target.to_string());
    locator.locate(&icon)
}

fn dump_icon(target: &str, size: (u16, u16), color: &str, options: &RunOptions) -> i32 {
    let mut store = options.new_store();
    let icon: Arc<BrailleIcon> = if let Some(kind) = GenericKind::from_name(target) {
        eprintln!("{}", kind.name());
        store.generic(kind, size.0, size.1)
    } else {
        let Some(path) = resolve_icon(target, store.locator()) else {
            eprintln!("apptop: no se encontró un icono para '{target}'");
            return 1;
        };
        let Some(icon) = store.file(&path, size.0, size.1) else {
            eprintln!("apptop: no se pudo convertir {}", path.display());
            return 1;
        };
        let (r, g, b) = icon.color;
        let source = if store.conversions == 0 {
            "caché"
        } else {
            "convertido"
        };
        eprintln!(
            "{}  color #{r:02x}{g:02x}{b:02x}  ({source})",
            path.display()
        );
        icon
    };
    let colored = match color {
        "always" => true,
        "never" => false,
        _ => std::io::stdout().is_terminal(),
    };
    if colored {
        print!("{}", icon.to_ansi(options.theme.truecolor));
    } else {
        print!("{}", icon.to_plain());
    }
    0
}

/// Interpreta `ANCHOxALTO`.
fn parse_size(text: &str) -> Option<(u16, u16)> {
    let (w, h) = text.split_once(['x', 'X'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// Toma dos muestras separadas por un cuarto de segundo, para tener CPU %.
fn sample_once() -> Sample {
    let mut sampler = Sampler::new();
    sleep(Duration::from_millis(250));
    sampler.sample()
}

fn snapshot(
    width: u16,
    height: u16,
    app: Option<&str>,
    options: &RunOptions,
) -> Result<String, String> {
    let mut state = options.new_state();
    let mut sampler = Sampler::new();
    sleep(Duration::from_millis(250));
    state.apply(sampler.sample());
    if let Some(wanted) = app {
        let lower = wanted.to_lowercase();
        let found = state
            .apps
            .iter()
            .find(|a| a.name.to_lowercase() == lower || a.app_id.as_deref() == Some(wanted))
            .or_else(|| {
                state
                    .apps
                    .iter()
                    .find(|a| a.name.to_lowercase().contains(&lower))
            })
            .map(|a| a.id.clone())
            .ok_or_else(|| format!("no hay ninguna app en ejecución llamada '{wanted}'"))?;
        state.selected = Some(found);
        state.enter_detail();
        // Dos muestras más con detalle: la primera fija las referencias de
        // las tasas (disco, etc.) y la segunda ya las puede calcular.
        for _ in 0..2 {
            let sample = sampler.sample_with_detail(state.detail_target());
            state.apply(sample);
            sleep(Duration::from_millis(250));
        }
        let sample = sampler.sample_with_detail(state.detail_target());
        state.apply(sample);
    }
    Ok(render_to_text(
        width,
        height,
        &mut state,
        &mut options.new_store(),
    ))
}
