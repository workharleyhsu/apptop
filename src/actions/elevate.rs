//! Reintento con sudo de una acción que el kernel rechazó por permisos.
//!
//! apptop nunca corre como root: solo el comando puntual (`kill` o `renice`)
//! pasa por sudo, con argumentos numéricos y sin una shell de por medio.
//!
//! La contraseña se pide con `sudo -v` en la terminal normal (la interfaz se
//! suspende mientras tanto). Recién después de aceptarla se vuelve a
//! comprobar que cada pid siga siendo el mismo proceso y se ejecuta el
//! comando con `sudo -n`, que ya no pregunta nada: así el tiempo que el
//! usuario tarda en escribir no deja una ventana para que un pid reutilizado
//! reciba la acción.

use std::env;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use nix::sys::signal::Signal;

use super::nice::{thread_ids, NICE_MAX, NICE_MIN};
use super::signal::{current_starttime, Target};
use crate::i18n::Lang;

/// Qué se reintenta como root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elevated {
    Signal(Signal),
    Nice(i32),
}

/// Una acción rechazada por permisos, lista para reintentarse con sudo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElevateRequest {
    pub action: Elevated,
    /// Solo los procesos que rechazaron la acción.
    pub targets: Vec<Target>,
    pub label: String,
}

/// Resultado del reintento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElevateOutcome {
    /// El comando terminó bien para `applied` procesos; `vanished` ya no
    /// existían al volver a comprobarlos.
    Done { applied: usize, vanished: usize },
    /// La contraseña no se aceptó o el usuario canceló.
    NotAuthorized,
    /// Todos los procesos terminaron mientras se pedía la contraseña.
    AllVanished,
    /// El comando falló; lleva la primera línea de su salida de error.
    Failed(String),
}

impl ElevateOutcome {
    pub fn is_problem(&self) -> bool {
        !matches!(self, ElevateOutcome::Done { .. })
    }

    /// Frase para el usuario, en su idioma.
    pub fn summary(&self, lang: Lang, request: &ElevateRequest) -> String {
        let label = &request.label;
        match self {
            ElevateOutcome::Done { applied, vanished } => {
                let text = match request.action {
                    Elevated::Signal(signal) => {
                        lang.signal_summary(signal, label, *applied, 0, *vanished, 0)
                    }
                    Elevated::Nice(value) => {
                        lang.nice_summary(value, label, *applied, 0, 0, *vanished, 0)
                    }
                };
                format!("sudo: {text}")
            }
            ElevateOutcome::NotAuthorized => lang.t().sudo_not_authorized.to_string(),
            ElevateOutcome::AllVanished => lang.t().sudo_all_vanished.to_string(),
            ElevateOutcome::Failed(message) if message.starts_with("sudo:") => message.clone(),
            ElevateOutcome::Failed(message) => format!("sudo: {message}"),
        }
    }
}

impl ElevateRequest {
    /// Programa y argumentos que se ejecutan como root sobre `ids`.
    fn args(&self, ids: &[i32]) -> Vec<String> {
        let mut args: Vec<String> = match self.action {
            Elevated::Signal(signal) => {
                let name = signal.as_str().trim_start_matches("SIG");
                vec!["kill".into(), "-s".into(), name.into(), "--".into()]
            }
            Elevated::Nice(value) => vec![
                "renice".into(),
                "-n".into(),
                value.clamp(NICE_MIN, NICE_MAX).to_string(),
                "-p".into(),
            ],
        };
        args.extend(ids.iter().map(|id| id.to_string()));
        args
    }

    /// Ids sobre los que actúa el comando: las señales van al pid; `nice`
    /// es por hilo en Linux, así que `renice` recibe todos los hilos.
    fn ids(&self, root: &Path, targets: &[Target]) -> Vec<i32> {
        match self.action {
            Elevated::Signal(_) => targets.iter().map(|t| t.pid).collect(),
            Elevated::Nice(_) => targets
                .iter()
                .flat_map(|t| thread_ids(root, t.pid))
                .collect(),
        }
    }

    /// El comando completo tal como se ejecutará, para mostrarlo en la
    /// terminal antes de pedir la contraseña.
    pub fn command_line(&self) -> String {
        let ids = self.ids(Path::new("/proc"), &self.targets);
        format!("sudo {}", self.args(&ids).join(" "))
    }

    /// Versión corta del comando para el diálogo: los primeros `max` pids y
    /// cuántos más hay.
    pub fn preview(&self, max: usize) -> String {
        let pids: Vec<i32> = self.targets.iter().map(|t| t.pid).collect();
        let shown = &pids[..pids.len().min(max)];
        let mut text = format!("sudo {}", self.args(shown).join(" "));
        if pids.len() > shown.len() {
            text.push_str(&format!(" +{}", pids.len() - shown.len()));
        }
        text
    }
}

/// Ruta de `sudo` en el `PATH`, si existe y es ejecutable.
pub fn find_sudo() -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join("sudo"))
        .find(|candidate| {
            candidate
                .metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

/// ¿Tiene sentido ofrecer el reintento? No si ya se es root ni sin sudo.
pub fn can_elevate() -> bool {
    !nix::unistd::geteuid().is_root() && find_sudo().is_some()
}

/// ¿sudo tiene credenciales vigentes? Entonces no hace falta suspender la
/// interfaz para pedir la contraseña.
pub fn has_cached_credentials() -> bool {
    Command::new("sudo")
        .args(["-n", "-v"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Pide la contraseña en la terminal, que debe estar en modo normal.
/// Devuelve `true` si sudo la aceptó.
pub fn authenticate() -> bool {
    Command::new("sudo")
        .arg("-v")
        .status()
        .is_ok_and(|s| s.success())
}

/// Vuelve a comprobar los procesos y ejecuta el comando con `sudo -n`.
/// Requiere credenciales vigentes (ver [`authenticate`]).
pub fn execute(request: &ElevateRequest) -> ElevateOutcome {
    execute_in(Path::new("/proc"), request, run_sudo)
}

fn run_sudo(args: &[String]) -> std::io::Result<(bool, String)> {
    let output = Command::new("sudo")
        .args(["-n", "--"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok((output.status.success(), stderr))
}

pub fn execute_in(
    root: &Path,
    request: &ElevateRequest,
    run: impl FnOnce(&[String]) -> std::io::Result<(bool, String)>,
) -> ElevateOutcome {
    // Nunca al pid 0/1, y solo si el pid sigue siendo el mismo proceso.
    let live: Vec<Target> = request
        .targets
        .iter()
        .filter(|t| t.pid > 1 && current_starttime(root, t.pid) == Some(t.starttime))
        .copied()
        .collect();
    let vanished = request.targets.len() - live.len();
    if live.is_empty() {
        return ElevateOutcome::AllVanished;
    }
    let args = request.args(&request.ids(root, &live));
    match run(&args) {
        Ok((true, _)) => ElevateOutcome::Done {
            applied: live.len(),
            vanished,
        },
        Ok((false, stderr)) => ElevateOutcome::Failed(
            stderr
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("exit status != 0")
                .to_string(),
        ),
        Err(err) => ElevateOutcome::Failed(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fake_proc(root: &Path, pid: i32, starttime: u64, threads: &[i32]) {
        let dir = root.join(pid.to_string());
        fs::create_dir_all(dir.join("task")).unwrap();
        let stat = format!("{pid} (x) S 1 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 {starttime} 0 0");
        fs::write(dir.join("stat"), stat).unwrap();
        for tid in threads {
            fs::create_dir_all(dir.join("task").join(tid.to_string())).unwrap();
        }
    }

    fn request(action: Elevated, pids: &[(i32, u64)]) -> ElevateRequest {
        ElevateRequest {
            action,
            targets: pids
                .iter()
                .map(|&(pid, starttime)| Target { pid, starttime })
                .collect(),
            label: "firefox".into(),
        }
    }

    #[test]
    fn builds_plain_commands_without_a_shell() {
        let kill = request(Elevated::Signal(Signal::SIGKILL), &[(10, 1), (11, 1)]);
        assert_eq!(
            kill.args(&[10, 11]),
            ["kill", "-s", "KILL", "--", "10", "11"]
        );
        let nice = request(Elevated::Nice(-30), &[(10, 1)]);
        assert_eq!(
            nice.args(&[10, 12]),
            ["renice", "-n", "-20", "-p", "10", "12"]
        );
        let many: Vec<(i32, u64)> = (100..110).map(|p| (p, 1)).collect();
        assert_eq!(
            request(Elevated::Signal(Signal::SIGTERM), &many).preview(3),
            "sudo kill -s TERM -- 100 101 102 +7"
        );
    }

    #[test]
    fn rechecks_targets_after_authenticating_and_renices_every_thread() {
        let tmp = tempfile::tempdir().unwrap();
        fake_proc(tmp.path(), 40, 7, &[40, 41, 42]);
        fake_proc(tmp.path(), 50, 99, &[50]); // pid reutilizado: otro starttime
        let req = request(Elevated::Nice(-5), &[(40, 7), (50, 8), (60, 1), (1, 1)]);
        let mut ran = Vec::new();
        let outcome = execute_in(tmp.path(), &req, |args| {
            ran = args.to_vec();
            Ok((true, String::new()))
        });
        assert_eq!(ran, ["renice", "-n", "-5", "-p", "40", "41", "42"]);
        assert_eq!(
            outcome,
            ElevateOutcome::Done {
                applied: 1,
                vanished: 3
            }
        );
    }

    #[test]
    fn nothing_runs_when_every_target_is_gone_and_errors_are_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let req = request(Elevated::Signal(Signal::SIGKILL), &[(40, 7)]);
        let outcome = execute_in(tmp.path(), &req, |_| panic!("no debe ejecutarse"));
        assert_eq!(outcome, ElevateOutcome::AllVanished);

        fake_proc(tmp.path(), 40, 7, &[40]);
        let outcome = execute_in(tmp.path(), &req, |_| {
            Ok((false, "\nkill: (40): Operation not permitted\n".into()))
        });
        assert_eq!(
            outcome.summary(Lang::En, &req),
            "sudo: kill: (40): Operation not permitted"
        );
        let denied = ElevateOutcome::Failed("sudo: a password is required".into());
        assert_eq!(
            denied.summary(Lang::En, &req),
            "sudo: a password is required"
        );
        assert!(denied.is_problem());
    }

    #[test]
    fn summaries_say_it_went_through_sudo() {
        let req = request(Elevated::Signal(Signal::SIGKILL), &[(40, 7), (41, 7)]);
        let done = ElevateOutcome::Done {
            applied: 2,
            vanished: 0,
        };
        assert_eq!(
            done.summary(Lang::En, &req),
            "sudo: SIGKILL sent to 2 processes of firefox"
        );
        assert!(!done.is_problem());
        let nice = request(Elevated::Nice(-5), &[(40, 7)]);
        assert_eq!(
            ElevateOutcome::Done {
                applied: 1,
                vanished: 0
            }
            .summary(Lang::Es, &nice),
            "sudo: Prioridad -5 aplicada a 1 proceso de firefox"
        );
    }
}
