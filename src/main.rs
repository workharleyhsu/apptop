use std::thread::sleep;
use std::time::Duration;

use clap::Parser;

use apptop::apps::{group_apps, AppKind};
use apptop::collect::Collector;
use apptop::ui::format::human_bytes;

/// Monitor de recursos para terminal centrado en aplicaciones.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Imprime las apps detectadas, sus PIDs y recursos en texto plano.
    #[arg(long)]
    list: bool,

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
    let apps = group_apps(collector.sample());
    for (kind, title) in [
        (AppKind::App, "APLICACIONES"),
        (AppKind::Service, "SERVICIOS"),
    ] {
        let section: Vec<_> = apps.iter().filter(|a| a.kind == kind).collect();
        println!("== {title} ({}) ==", section.len());
        println!(
            "{:<32} {:<13} {:>5} {:>6} {:>9}  PIDS",
            "NOMBRE", "TIPO", "PROCS", "CPU%", "RAM"
        );
        for app in section {
            let pids: Vec<String> = app.procs.iter().map(|p| p.pid.to_string()).collect();
            println!(
                "{:<32} {:<13} {:>5} {:>6.1} {:>9}  {}",
                truncate(&app.name, 32),
                app.origin.label(),
                app.procs.len(),
                app.cpu_pct,
                human_bytes(app.rss),
                pids.join(",")
            );
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
