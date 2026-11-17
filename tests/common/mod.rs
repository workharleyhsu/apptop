//! Utilidades compartidas por los tests de integración.
//!
//! Regla de seguridad: los tests solo envían señales a procesos `sleep`
//! que ellos mismos lanzaron. `Sleeper` es el único modo de obtener un pid.

#![allow(dead_code)]

use std::fs;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use apptop::actions::Target;

/// Un proceso `sleep` propio. Al soltarse se mata y se espera.
pub struct Sleeper {
    child: Child,
}

impl Sleeper {
    pub fn spawn() -> Self {
        let child = Command::new("sleep")
            .arg("600")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("no se pudo lanzar sleep");
        Self { child }
    }

    pub fn pid(&self) -> i32 {
        self.child.id() as i32
    }

    /// Objetivo con el `starttime` real del proceso.
    pub fn target(&self) -> Target {
        Target {
            pid: self.pid(),
            starttime: stat_field(self.pid(), 19)
                .expect("starttime")
                .parse()
                .expect("número"),
        }
    }

    /// Letra de estado según `/proc/<pid>/stat` (`None` si ya no existe).
    pub fn state(&self) -> Option<char> {
        stat_field(self.pid(), 0).and_then(|s| s.chars().next())
    }

    pub fn nice(&self) -> Option<i32> {
        stat_field(self.pid(), 16).and_then(|s| s.parse().ok())
    }

    /// Espera hasta `timeout` a que el proceso termine. Devuelve la señal
    /// que lo terminó, si fue una señal.
    pub fn wait_exit(&mut self, timeout: Duration) -> Option<Option<i32>> {
        use std::os::unix::process::ExitStatusExt;
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status.signal()),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => return None,
            }
        }
    }

    /// Espera hasta `timeout` a que el estado cumpla la condición.
    pub fn wait_state(&self, timeout: Duration, wanted: impl Fn(Option<char>) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if wanted(self.state()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        wanted(self.state())
    }
}

impl Drop for Sleeper {
    fn drop(&mut self) {
        // `Child::kill` solo puede afectar a nuestro propio hijo.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Campo de `/proc/<pid>/stat` contado desde el estado (índice 0), es decir
/// después del nombre entre paréntesis.
fn stat_field(pid: i32, index: usize) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(index).map(str::to_string)
}
