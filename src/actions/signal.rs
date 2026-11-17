//! Envío de señales a los procesos de una app.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use nix::errno::Errno;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

/// Un proceso al que se le quiere enviar una señal. `starttime` identifica
/// al proceso concreto: si el pid ya fue reutilizado por otro, no se toca.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Target {
    pub pid: i32,
    pub starttime: u64,
}

/// Resultado de enviar una señal a varios procesos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SignalReport {
    /// Procesos que recibieron la señal.
    pub delivered: usize,
    /// Ya no existían (o el pid pasó a ser de otro proceso).
    pub vanished: usize,
    /// Sin permiso (procesos de otro usuario).
    pub denied: usize,
    /// Otros errores.
    pub failed: usize,
}

impl SignalReport {
    pub fn total(&self) -> usize {
        self.delivered + self.vanished + self.denied + self.failed
    }

    /// Frase para mostrar al usuario.
    pub fn summary(&self, signal: Signal, label: &str) -> String {
        let mut text = match self.delivered {
            0 => format!(
                "{} no se envió a ningún proceso de {label}",
                signal.as_str()
            ),
            1 => format!("{} enviada a 1 proceso de {label}", signal.as_str()),
            n => format!("{} enviada a {n} procesos de {label}", signal.as_str()),
        };
        if self.denied > 0 {
            text.push_str(&format!(" · sin permiso para {}", self.denied));
        }
        if self.vanished > 0 {
            text.push_str(&format!(" · {} ya habían terminado", self.vanished));
        }
        if self.failed > 0 {
            text.push_str(&format!(" · {} con error", self.failed));
        }
        text
    }

    /// ¿Hubo algo que el usuario deba notar como problema?
    pub fn is_problem(&self) -> bool {
        self.delivered == 0 || self.denied > 0 || self.failed > 0
    }
}

/// `starttime` actual del pid según `/proc/<pid>/stat`, o `None` si no existe.
pub fn current_starttime(root: &Path, pid: i32) -> Option<u64> {
    let stat = fs::read_to_string(root.join(pid.to_string()).join("stat")).ok()?;
    // El nombre va entre paréntesis y puede contener espacios: se parte
    // después del último ')'. starttime es el campo 22 (el 20.º tras el nombre).
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(19)?.parse().ok()
}

/// `ppid` actual del pid, o `None` si no existe.
fn parent_of(root: &Path, pid: i32) -> Option<i32> {
    let stat = fs::read_to_string(root.join(pid.to_string()).join("stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(1)?.parse().ok()
}

/// El proceso `pid` y todos sus ancestros (sin incluir el pid 0).
pub fn ancestry(root: &Path, pid: i32) -> Vec<i32> {
    let mut chain = vec![pid];
    let mut current = pid;
    // El límite evita ciclos si /proc cambia durante la lectura.
    for _ in 0..64 {
        match parent_of(root, current) {
            Some(parent) if parent > 0 && !chain.contains(&parent) => {
                chain.push(parent);
                current = parent;
            }
            _ => break,
        }
    }
    chain
}

/// Señales que detienen al proceso: enviarlas a apptop o a la terminal que
/// lo contiene congelaría la interfaz sin forma de reanudarla.
pub fn is_stopping(signal: Signal) -> bool {
    matches!(
        signal,
        Signal::SIGSTOP | Signal::SIGTSTP | Signal::SIGTTIN | Signal::SIGTTOU
    )
}

/// Quita de `targets` los procesos que no deben recibir `signal`:
/// apptop mismo siempre, y con señales de detención también sus ancestros.
/// `lineage` es apptop seguido de sus ancestros. Devuelve los que quedan y
/// cuántos se excluyeron.
pub fn filter_targets(targets: &[Target], signal: Signal, lineage: &[i32]) -> (Vec<Target>, usize) {
    let protected: HashSet<i32> = if is_stopping(signal) {
        lineage.iter().copied().collect()
    } else {
        lineage.iter().take(1).copied().collect()
    };
    let kept: Vec<Target> = targets
        .iter()
        .filter(|t| t.pid > 1 && !protected.contains(&t.pid))
        .copied()
        .collect();
    let excluded = targets.len() - kept.len();
    (kept, excluded)
}

/// Envía `signal` a cada proceso de `targets`, verificando antes que el pid
/// siga siendo el mismo proceso.
pub fn send_signal(targets: &[Target], signal: Signal) -> SignalReport {
    send_signal_in(Path::new("/proc"), targets, signal)
}

pub fn send_signal_in(root: &Path, targets: &[Target], signal: Signal) -> SignalReport {
    let mut report = SignalReport::default();
    for target in targets {
        // Nunca al pid 0/1 ni a grupos (pid negativo).
        if target.pid <= 1 {
            report.failed += 1;
            continue;
        }
        if current_starttime(root, target.pid) != Some(target.starttime) {
            report.vanished += 1;
            continue;
        }
        match kill(Pid::from_raw(target.pid), signal) {
            Ok(()) => report.delivered += 1,
            Err(Errno::ESRCH) => report.vanished += 1,
            Err(Errno::EPERM) => report.denied += 1,
            Err(_) => report.failed += 1,
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(pid: i32) -> Target {
        Target { pid, starttime: 1 }
    }

    #[test]
    fn reads_starttime_and_parent_even_with_odd_names() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("77");
        fs::create_dir_all(&dir).unwrap();
        let stat = "77 (a (b) c) S 12 77 77 0 -1 4194304 1 0 0 0 5 5 0 0 20 0 1 0 424242 1000 10";
        fs::write(dir.join("stat"), stat).unwrap();
        assert_eq!(current_starttime(tmp.path(), 77), Some(424242));
        assert_eq!(parent_of(tmp.path(), 77), Some(12));
        assert_eq!(current_starttime(tmp.path(), 78), None);
    }

    #[test]
    fn ancestry_walks_up_to_init() {
        let chain = ancestry(Path::new("/proc"), std::process::id() as i32);
        assert_eq!(chain[0], std::process::id() as i32);
        assert_eq!(chain.last(), Some(&1));
        assert!(chain.len() >= 2);
    }

    #[test]
    fn never_targets_apptop_itself_and_protects_ancestors_from_stop() {
        let lineage = [50, 40, 30];
        let all = [
            target(50),
            target(40),
            target(30),
            target(99),
            target(1),
            target(0),
        ];
        // SIGTERM: solo se excluye apptop (y pid 0/1).
        let (kept, excluded) = filter_targets(&all, Signal::SIGTERM, &lineage);
        assert_eq!(kept, vec![target(40), target(30), target(99)]);
        assert_eq!(excluded, 3);
        // SIGSTOP: tampoco la shell ni la terminal que lo contienen.
        let (kept, excluded) = filter_targets(&all, Signal::SIGSTOP, &lineage);
        assert_eq!(kept, vec![target(99)]);
        assert_eq!(excluded, 5);
        assert!(is_stopping(Signal::SIGTSTP) && !is_stopping(Signal::SIGCONT));
    }

    #[test]
    fn refuses_special_pids_and_stale_targets_without_signalling() {
        let tmp = tempfile::tempdir().unwrap();
        // Raíz de /proc vacía: ningún pid "existe", así que nada se envía.
        let report = send_signal_in(
            tmp.path(),
            &[target(0), target(1), target(-5), target(4242)],
            Signal::SIGTERM,
        );
        assert_eq!(
            report,
            SignalReport {
                delivered: 0,
                vanished: 1,
                denied: 0,
                failed: 3
            }
        );
        assert!(report.is_problem());
    }

    #[test]
    fn summaries_are_readable() {
        let ok = SignalReport {
            delivered: 12,
            ..Default::default()
        };
        assert_eq!(
            ok.summary(Signal::SIGTERM, "Firefox"),
            "SIGTERM enviada a 12 procesos de Firefox"
        );
        assert!(!ok.is_problem());
        let mixed = SignalReport {
            delivered: 1,
            vanished: 2,
            denied: 3,
            failed: 0,
        };
        assert_eq!(
            mixed.summary(Signal::SIGKILL, "x"),
            "SIGKILL enviada a 1 proceso de x · sin permiso para 3 · 2 ya habían terminado"
        );
        let none = SignalReport {
            denied: 4,
            ..Default::default()
        };
        assert_eq!(
            none.summary(Signal::SIGTERM, "docker"),
            "SIGTERM no se envió a ningún proceso de docker · sin permiso para 4"
        );
        assert_eq!(mixed.total(), 6);
    }
}
