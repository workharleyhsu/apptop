use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use clap::Parser;

use apptop::apps::group::exe_name;
use apptop::apps::{
    classify, group_apps_with, AppKind, DesktopIndex, ExecMatch, Sample, Sampler, Unit,
};
use apptop::collect::Collector;
use apptop::config::{Config, Warning};
use apptop::i18n::Lang;
use apptop::icons::{BrailleIcon, GenericKind, IconLocator, DETAIL_ICON};
use apptop::ui::format::human_bytes;
use apptop::ui::state::clamp_refresh;
use apptop::ui::{render_to_text, RunOptions};

// Los textos de ayuda van solo en inglés: clap los toma en tiempo de
// compilación, y es lo primero que lee quien todavía no configuró nada.

/// App-centric resource monitor for the terminal.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Print the detected apps, their PIDs and resource usage as plain text.
    #[arg(long)]
    list: bool,

    /// Print an app's icon converted to braille. Accepts an app name, a
    /// `.desktop` id, an icon name, a file path, or `generic:service` /
    /// `generic:app` for the generic icons.
    #[arg(long, value_name = "APP")]
    dump_icon: Option<String>,

    /// With `--dump-icon`: use the large size of the detail view.
    #[arg(long)]
    large: bool,

    /// Force coloured output (`always`), plain output (`never`) or decide
    /// from the terminal (`auto`).
    #[arg(long, value_name = "MODE", default_value = "auto")]
    color: String,

    /// Seconds between refreshes (0.25 to 10).
    #[arg(long, short = 'r', value_name = "SECONDS")]
    refresh: Option<f64>,

    /// Interface language: `en` (default) or `es`.
    #[arg(long, value_name = "LANG")]
    lang: Option<String>,

    /// Configuration file (default: ~/.config/apptop/config.toml).
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Print the effective configuration as TOML and exit.
    #[arg(long)]
    print_config: bool,

    /// Render one frame of the interface as text and exit.
    #[arg(long)]
    snapshot: bool,

    /// With `--snapshot`: show this app's detail view instead of the grid.
    #[arg(long, value_name = "NAME")]
    app: Option<String>,

    /// With `--snapshot`: frame size, for example `120x40`.
    #[arg(long, value_name = "WIDTHxHEIGHT", default_value = "100x30")]
    size: String,

    /// Panic after the first frame (terminal restoration test).
    #[arg(long, hide = true)]
    panic_test: bool,

    /// Explain how the processes matching NAME were grouped: their cgroup,
    /// the unit it maps to and the `.desktop` entry that was matched. Useful
    /// when an app shows up in the wrong section.
    #[arg(long, value_name = "NAME")]
    explain: Option<String>,

    /// Print every process that was read (debugging).
    #[arg(long)]
    list_procs: bool,
}

fn main() {
    // Rust ignora SIGPIPE y `println!` hace panic si el lector cierra antes
    // (`apptop --list | head`). Con la acción por defecto el proceso termina
    // en silencio, como cualquier herramienta de consola.
    // SAFETY: se llama al arrancar, antes de crear hilos o manejadores.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();

    // Configuración: archivo (opcional) y encima las opciones de la línea
    // de comandos. Las advertencias no impiden arrancar.
    let loaded = Config::load(cli.config.as_deref());
    let mut config = loaded.config.clone();
    let mut bad_lang = None;
    if let Some(text) = &cli.lang {
        match Lang::parse(text) {
            Some(lang) => config.language = lang,
            None => bad_lang = Some(text.clone()),
        }
    }
    // Recién ahora se sabe en qué idioma mostrar las advertencias.
    let lang = config.language;
    for message in loaded.messages(lang) {
        eprintln!("apptop: {message}");
    }
    if let Some(text) = bad_lang {
        let warning = Warning::Expected("--lang".into(), text, "en, es");
        eprintln!("apptop: {}", warning.text(lang));
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
    if let Some(name) = &cli.explain {
        explain(name);
        return;
    }
    if cli.list_procs {
        list_procs(lang);
        return;
    }
    if !std::io::stdout().is_terminal() {
        eprintln!("apptop: {}", lang.t().not_a_terminal);
        std::process::exit(2);
    }
    if let Err(err) = apptop::ui::run(&options) {
        eprintln!("apptop: {err}");
        std::process::exit(1);
    }
}

fn list_procs(lang: Lang) {
    let mut collector = Collector::new();
    collector.sample();
    sleep(Duration::from_millis(250));
    println!(
        "{:>8} {:>8} {:1} {:>6} {:>10} {:>4} {:<16} EXE",
        "PID",
        "PPID",
        "S",
        "CPU%",
        "RSS(kB)",
        "THR",
        lang.t().list_name
    );
    for p in collector.sample() {
        let exe = p
            .exe
            .as_ref()
            .map(|e| e.display().to_string())
            .unwrap_or_else(|| lang.t().na.into());
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
    let lang = options.lang;
    let t = lang.t();
    println!("{}: {}\n", t.list_icon_theme, locator.theme());
    for (kind, title) in [
        (AppKind::App, t.list_applications),
        (AppKind::Service, t.list_services),
    ] {
        let section: Vec<_> = apps.iter().filter(|a| a.kind == kind).collect();
        println!("== {title} ({}) ==", section.len());
        println!(
            "{:<28} {:<13} {:>5} {:>6} {:>9}  PIDS",
            t.list_name, t.list_type, "PROCS", "CPU%", "RAM"
        );
        for app in section {
            let pids: Vec<String> = app.procs.iter().map(|p| p.pid.to_string()).collect();
            println!(
                "{:<28} {:<13} {:>5} {:>6.1} {:>9}  {}",
                truncate(&app.name, 28),
                app.origin.label(lang),
                app.procs.len(),
                app.cpu_pct,
                human_bytes(app.rss),
                pids.join(",")
            );
            if let Some(icon) = app.desktop.as_ref().and_then(|d| d.icon.as_deref()) {
                let path = locator
                    .locate(icon)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| t.na.into());
                println!("{:<28} {}: {icon} -> {path}", "", t.list_icon);
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
            eprintln!("apptop: {}", options.lang.no_icon_for(target));
            return 1;
        };
        let Some(icon) = store.file(&path, size.0, size.1) else {
            let path = path.display().to_string();
            eprintln!("apptop: {}", options.lang.cannot_convert(&path));
            return 1;
        };
        let (r, g, b) = icon.color;
        let source = if store.conversions == 0 {
            options.lang.t().icon_cached
        } else {
            options.lang.t().icon_converted
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
            .ok_or_else(|| options.lang.no_app_named(wanted))?;
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

/// Diagnóstico de agrupación (`--explain`). Salida técnica, solo en inglés:
/// está pensada para pegarla en un reporte de error.
fn explain(name: &str) {
    let index = DesktopIndex::load();
    let procs = Collector::new().sample();
    let apps = group_apps_with(procs.clone(), Some(&index));
    let wanted = name.to_lowercase();
    let group_of = |pid: i32| apps.iter().find(|a| a.procs.iter().any(|p| p.pid == pid));

    let mut shown = 0;
    for p in procs.iter().filter(|p| !p.kthread) {
        let exe = exe_name(p);
        let group = group_of(p.pid);
        let hit = p.name.to_lowercase().contains(&wanted)
            || exe.to_lowercase().contains(&wanted)
            || group.is_some_and(|g| g.name.to_lowercase().contains(&wanted));
        if !hit {
            continue;
        }
        shown += 1;
        let path = p.exe.as_ref().map(|e| e.display().to_string());
        println!("pid {} ({})", p.pid, p.name);
        println!(
            "  exe      {}",
            path.as_deref().unwrap_or("n/a (not readable)")
        );
        println!("  cgroup   {}", p.cgroup.as_deref().unwrap_or("n/a"));
        let unit = p.cgroup.as_deref().map(classify).unwrap_or(Unit::None);
        match &unit {
            Unit::AppScope { app_id, flatpak } => {
                let kind = if *flatpak { "flatpak app" } else { "app" };
                println!("  unit     {kind} unit, AppID '{app_id}'");
                match index.by_app_id(app_id) {
                    Some(d) => println!("  desktop  {} (matched by AppID)", d.path.display()),
                    None => println!("  desktop  none for AppID '{app_id}'"),
                }
            }
            Unit::Service { name, user, app_id } => {
                let owner = if *user { "user" } else { "system" };
                println!("  unit     {owner} service '{name}'");
                if let Some(app_id) = app_id {
                    match index.by_app_id(app_id) {
                        Some(d) if d.visible => println!(
                            "  desktop  {} (visible in the menu: treated as an app)",
                            d.path.display()
                        ),
                        Some(d) => println!(
                            "  desktop  {} (NoDisplay: stays a service)",
                            d.path.display()
                        ),
                        None => println!("  desktop  none for AppID '{app_id}': stays a service"),
                    }
                }
            }
            Unit::Container { runtime, id } => println!("  unit     {runtime} container {id}"),
            Unit::None => {
                println!("  unit     none useful (session scope, transient unit or root)")
            }
        }
        match index.match_exec(&exe) {
            Some((d, ExecMatch::ExecLine)) => {
                println!(
                    "  by exe   '{exe}' is the Exec= program of {}",
                    d.path.display()
                )
            }
            Some((d, ExecMatch::DesktopId)) => {
                println!("  by exe   '{exe}' is the id of {}", d.path.display())
            }
            None => println!("  by exe   '{exe}' matches no visible .desktop entry"),
        }
        match group {
            Some(g) => {
                let section = match g.kind {
                    AppKind::App => "Applications",
                    AppKind::Service => "Services",
                };
                println!(
                    "  result   '{}' in {section} [{}], {} processes",
                    g.name,
                    g.id,
                    g.procs.len()
                );
            }
            None => println!("  result   not shown"),
        }
        println!();
    }
    if shown == 0 {
        println!("no process, executable or app matches '{name}'");
    }
}
