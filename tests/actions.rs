//! Acciones reales (señales y prioridad) sobre procesos `sleep` lanzados
//! por el propio test. Ningún otro proceso recibe nada.

mod common;

use std::time::Duration;

use apptop::actions::{filter_targets, send_signal, set_nice, Signal, Target};
use apptop::collect::Collector;
use apptop::i18n::Lang;
use common::Sleeper;

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn sigterm_closes_every_process_of_the_group() {
    let mut group: Vec<Sleeper> = (0..3).map(|_| Sleeper::spawn()).collect();
    let targets: Vec<Target> = group.iter().map(Sleeper::target).collect();

    let report = send_signal(&targets, Signal::SIGTERM);
    assert_eq!((report.delivered, report.total()), (3, 3));
    assert!(!report.is_problem());
    for sleeper in &mut group {
        assert_eq!(sleeper.wait_exit(WAIT), Some(Some(libc::SIGTERM)));
    }
}

#[test]
fn sigkill_kills_the_group() {
    let mut group: Vec<Sleeper> = (0..2).map(|_| Sleeper::spawn()).collect();
    let targets: Vec<Target> = group.iter().map(Sleeper::target).collect();
    assert_eq!(send_signal(&targets, Signal::SIGKILL).delivered, 2);
    for sleeper in &mut group {
        assert_eq!(sleeper.wait_exit(WAIT), Some(Some(libc::SIGKILL)));
    }
}

#[test]
fn sigstop_pauses_and_sigcont_resumes() {
    let sleeper = Sleeper::spawn();
    let targets = [sleeper.target()];

    assert_eq!(send_signal(&targets, Signal::SIGSTOP).delivered, 1);
    assert!(
        sleeper.wait_state(WAIT, |s| s == Some('T')),
        "{:?}",
        sleeper.state()
    );

    assert_eq!(send_signal(&targets, Signal::SIGCONT).delivered, 1);
    assert!(
        sleeper.wait_state(WAIT, |s| matches!(s, Some('S' | 'R'))),
        "{:?}",
        sleeper.state()
    );
}

#[test]
fn one_process_can_be_signalled_without_touching_its_siblings() {
    let mut chosen = Sleeper::spawn();
    let mut sibling = Sleeper::spawn();
    let report = send_signal(&[chosen.target()], Signal::SIGTERM);
    assert_eq!(report.delivered, 1);
    assert_eq!(chosen.wait_exit(WAIT), Some(Some(libc::SIGTERM)));
    assert_eq!(
        sibling.wait_exit(Duration::from_millis(200)),
        None,
        "el otro sigue vivo"
    );
}

#[test]
fn a_recycled_pid_is_left_alone() {
    let mut sleeper = Sleeper::spawn();
    // Mismo pid, pero el `starttime` de "otro proceso".
    let stale = Target {
        pid: sleeper.pid(),
        starttime: sleeper.target().starttime + 1,
    };
    let report = send_signal(&[stale], Signal::SIGKILL);
    assert_eq!((report.delivered, report.vanished), (0, 1));
    assert!(report.is_problem());
    assert_eq!(
        sleeper.wait_exit(Duration::from_millis(200)),
        None,
        "no recibió la señal"
    );
}

#[test]
fn processes_that_already_exited_count_as_vanished() {
    let mut sleeper = Sleeper::spawn();
    let target = sleeper.target();
    assert_eq!(send_signal(&[target], Signal::SIGKILL).delivered, 1);
    assert!(sleeper.wait_exit(WAIT).is_some());
    // Ya fue recogido con wait(): el pid no existe más.
    let report = send_signal(&[target], Signal::SIGTERM);
    assert_eq!((report.delivered, report.vanished), (0, 1));
}

#[test]
fn nice_can_be_raised_and_lowering_it_reports_the_missing_privilege() {
    let sleeper = Sleeper::spawn();
    let targets = [sleeper.target()];
    assert_eq!(sleeper.nice(), Some(0));

    let report = set_nice(&targets, 7);
    assert_eq!(report.changed, 1, "{report:?}");
    assert_eq!(sleeper.nice(), Some(7));
    assert_eq!(
        report.summary(Lang::En, 7, "sleep"),
        "Priority 7 applied to 1 process of sleep"
    );

    // Volver a 0 es subir la prioridad: sin CAP_SYS_NICE el kernel lo
    // rechaza (salvo que RLIMIT_NICE lo permita en esta máquina).
    let report = set_nice(&targets, 0);
    if report.changed == 1 {
        assert_eq!(sleeper.nice(), Some(0));
    } else {
        assert_eq!(report.needs_privilege, 1, "{report:?}");
        assert_eq!(sleeper.nice(), Some(7), "no cambió");
        assert!(report.is_problem());
        assert!(report
            .summary(Lang::En, 0, "sleep")
            .contains("requires root or CAP_SYS_NICE"));
    }
}

#[test]
fn nice_values_are_clamped_and_dead_processes_reported() {
    let mut sleeper = Sleeper::spawn();
    let target = sleeper.target();
    assert_eq!(set_nice(&[target], 99).changed, 1);
    assert_eq!(sleeper.nice(), Some(19));

    send_signal(&[target], Signal::SIGKILL);
    assert!(sleeper.wait_exit(WAIT).is_some());
    let report = set_nice(&[target], 5);
    assert_eq!((report.changed, report.vanished), (0, 1));
}

#[test]
fn targets_built_from_a_real_sample_reach_the_right_processes() {
    let mut group: Vec<Sleeper> = (0..2).map(|_| Sleeper::spawn()).collect();
    let mine: Vec<i32> = group.iter().map(Sleeper::pid).collect();

    // El mismo camino que usa la interfaz: muestra → procesos → objetivos.
    let sample = Collector::new().sample();
    let targets: Vec<Target> = sample
        .iter()
        .filter(|p| mine.contains(&p.pid))
        .map(|p| Target {
            pid: p.pid,
            starttime: p.starttime,
        })
        .collect();
    assert_eq!(targets.len(), 2);
    assert!(sample
        .iter()
        .filter(|p| mine.contains(&p.pid))
        .all(|p| p.name == "sleep"));

    // Si el test fuera uno de los objetivos, el filtro lo quitaría.
    let me = std::process::id() as i32;
    let mut with_self = targets.clone();
    with_self.push(Target {
        pid: me,
        starttime: 0,
    });
    let (kept, excluded) = filter_targets(&with_self, Signal::SIGTERM, &[me]);
    assert_eq!((kept.len(), excluded), (2, 1));

    assert_eq!(send_signal(&kept, Signal::SIGTERM).delivered, 2);
    for sleeper in &mut group {
        assert_eq!(sleeper.wait_exit(WAIT), Some(Some(libc::SIGTERM)));
    }
}
