use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use clap::Parser;

use apptop::apps::{group_apps_with, AppKind, DesktopIndex};
use apptop::collect::Collector;
use apptop::icons::{convert_file, IconLocator, DETAIL_ICON, GRID_ICON};
use apptop::ui::format::human_bytes;

/// Monitor de recursos para terminal centrado en aplicaciones.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Imprime las apps detectadas, sus PIDs y recursos en texto plano.
    #[arg(long)]
    list: bool,

    /// Imprime el icono de una app convertido a braille. Acepta el nombre de
    /// una app, el id de un `.desktop`, un nombre de icono o una ruta.
    #[arg(long, value_name = "APP")]
    dump_icon: Option<String>,

    /// Con `--dump-icon`: usa el tamaño grande de la vista de detalle.
    #[arg(long)]
    large: bool,

    /// Fuerza la salida con color (`always`), sin color (`never`) o según
    /// la terminal (`auto`).
    #[arg(long, value_name = "MODO", default_value = "auto")]
    color: String,

    /// Imprime todos los procesos leídos (depuración).
    #[arg(long)]
    list_procs: bool,
}

fn main() {
    let cli = Cli::parse();
    if cli.list {
        list_apps();
        return;
    }
    if let Some(target) = &cli.dump_icon {
        let size = if cli.large { DETAIL_ICON } else { GRID_ICON };
        std::process::exit(dump_icon(target, size, &cli.color));
    }
    if cli.list_procs {
        list_procs();
        return;
    }
    println!("apptop {}", env!("CARGO_PKG_VERSION"));
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

fn list_apps() {
    let mut collector = Collector::new();
    collector.sample();
    sleep(Duration::from_millis(250));
    let index = DesktopIndex::load();
    let locator = IconLocator::detect();
    println!("Tema de iconos: {}\n", locator.theme());
    let apps = group_apps_with(collector.sample(), Some(&index));
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

fn dump_icon(target: &str, size: (u16, u16), color: &str) -> i32 {
    let locator = IconLocator::detect();
    let Some(path) = resolve_icon(target, &locator) else {
        eprintln!("apptop: no se encontró un icono para '{target}'");
        return 1;
    };
    let Some(icon) = convert_file(&path, size.0, size.1) else {
        eprintln!("apptop: no se pudo convertir {}", path.display());
        return 1;
    };
    let colored = match color {
        "always" => true,
        "never" => false,
        _ => std::io::stdout().is_terminal(),
    };
    let (r, g, b) = icon.color;
    eprintln!("{}  color #{r:02x}{g:02x}{b:02x}", path.display());
    if colored {
        let truecolor = std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit");
        print!("{}", icon.to_ansi(truecolor));
    } else {
        print!("{}", icon.to_plain());
    }
    0
}
