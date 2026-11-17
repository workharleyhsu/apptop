use std::thread::sleep;
use std::time::Duration;

use clap::Parser;

use apptop::collect::Collector;

/// Monitor de recursos para terminal centrado en aplicaciones.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Imprime todos los procesos leídos (depuración).
    #[arg(long)]
    list_procs: bool,
}

fn main() {
    let cli = Cli::parse();
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
