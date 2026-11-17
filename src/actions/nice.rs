//! Cambio de prioridad (`nice`) de los procesos de una app.

use std::fs;
use std::path::Path;

use super::signal::{current_starttime, Target};
use crate::i18n::Lang;

/// Rango válido de `nice`: -20 (más prioridad) a 19 (menos).
pub const NICE_MIN: i32 = -20;
pub const NICE_MAX: i32 = 19;

/// Resultado de cambiar la prioridad de varios procesos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NiceReport {
    pub changed: usize,
    pub vanished: usize,
    /// El kernel no deja subir la prioridad (bajar el valor) sin privilegios.
    pub needs_privilege: usize,
    /// El proceso es de otro usuario.
    pub not_owner: usize,
    pub failed: usize,
}

/// Motivo por el que falló `setpriority`, según `errno`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NiceError {
    Vanished,
    NeedsPrivilege,
    NotOwner,
    Other,
}

pub fn classify(errno: i32) -> NiceError {
    match errno {
        libc::ESRCH => NiceError::Vanished,
        // EACCES: se intentó bajar el valor sin CAP_SYS_NICE.
        libc::EACCES => NiceError::NeedsPrivilege,
        // EPERM: el proceso pertenece a otro usuario.
        libc::EPERM => NiceError::NotOwner,
        _ => NiceError::Other,
    }
}

impl NiceReport {
    fn record(&mut self, error: NiceError) {
        match error {
            NiceError::Vanished => self.vanished += 1,
            NiceError::NeedsPrivilege => self.needs_privilege += 1,
            NiceError::NotOwner => self.not_owner += 1,
            NiceError::Other => self.failed += 1,
        }
    }

    pub fn is_problem(&self) -> bool {
        self.changed == 0 || self.needs_privilege + self.not_owner + self.failed > 0
    }

    /// Frase para el usuario, en su idioma; explica qué permiso falta.
    pub fn summary(&self, lang: Lang, value: i32, label: &str) -> String {
        lang.nice_summary(
            value,
            label,
            self.changed,
            self.needs_privilege,
            self.not_owner,
            self.vanished,
            self.failed,
        )
    }
}

/// Hilos de un proceso. En Linux `nice` es por hilo: para cambiar la
/// prioridad del proceso entero hay que recorrerlos todos.
fn thread_ids(root: &Path, pid: i32) -> Vec<i32> {
    let mut ids: Vec<i32> = fs::read_dir(root.join(pid.to_string()).join("task"))
        .map(|dir| {
            dir.flatten()
                .filter_map(|e| e.file_name().to_string_lossy().parse().ok())
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        ids.push(pid);
    }
    ids
}

fn set_thread_nice(tid: i32, value: i32) -> Result<(), NiceError> {
    // SAFETY: setpriority no toca memoria; solo recibe enteros.
    let result = unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, value) };
    if result == 0 {
        Ok(())
    } else {
        let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        Err(classify(errno))
    }
}

/// Aplica `value` a todos los hilos de cada proceso de `targets`.
pub fn set_nice(targets: &[Target], value: i32) -> NiceReport {
    set_nice_in(Path::new("/proc"), targets, value)
}

pub fn set_nice_in(root: &Path, targets: &[Target], value: i32) -> NiceReport {
    let value = value.clamp(NICE_MIN, NICE_MAX);
    let mut report = NiceReport::default();
    for target in targets {
        if target.pid <= 1 {
            report.failed += 1;
            continue;
        }
        if current_starttime(root, target.pid) != Some(target.starttime) {
            report.vanished += 1;
            continue;
        }
        // El proceso cuenta como cambiado si se pudo con todos sus hilos;
        // un hilo que terminó en el medio no es un error.
        let mut error = None;
        for tid in thread_ids(root, target.pid) {
            match set_thread_nice(tid, value) {
                Ok(()) | Err(NiceError::Vanished) => {}
                Err(other) => {
                    error = Some(other);
                    break;
                }
            }
        }
        match error {
            None => report.changed += 1,
            Some(error) => report.record(error),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_errno() {
        assert_eq!(classify(libc::ESRCH), NiceError::Vanished);
        assert_eq!(classify(libc::EACCES), NiceError::NeedsPrivilege);
        assert_eq!(classify(libc::EPERM), NiceError::NotOwner);
        assert_eq!(classify(libc::EINVAL), NiceError::Other);
    }

    #[test]
    fn summaries_explain_the_missing_permission() {
        let ok = NiceReport {
            changed: 3,
            ..Default::default()
        };
        assert_eq!(
            ok.summary(Lang::En, 10, "Firefox"),
            "Priority 10 applied to 3 processes of Firefox"
        );
        assert!(!ok.is_problem());

        let denied = NiceReport {
            needs_privilege: 2,
            ..Default::default()
        };
        assert_eq!(
            denied.summary(Lang::En, -5, "Firefox"),
            "No permission for 2: raising priority (lowering nice) requires root or CAP_SYS_NICE"
        );
        assert_eq!(
            denied.summary(Lang::Es, -5, "Firefox"),
            "Sin permiso en 2: subir la prioridad (bajar nice) requiere root o CAP_SYS_NICE"
        );
        assert!(denied.is_problem());
    }

    #[test]
    fn stale_and_special_targets_are_never_touched() {
        let tmp = tempfile::tempdir().unwrap();
        let targets = [
            Target {
                pid: 1,
                starttime: 1,
            },
            Target {
                pid: 4242,
                starttime: 9,
            },
        ];
        let report = set_nice_in(tmp.path(), &targets, 5);
        assert_eq!((report.changed, report.failed, report.vanished), (0, 1, 1));
    }
}
