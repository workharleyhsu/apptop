//! Criterio de aceptación: ningún panic cuando los procesos mueren durante
//! el refresco. Un hilo lanza y mata `sleep` propios en bucle mientras se
//! recolecta, agrupa, mide el detalle y se dibuja.

mod common;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use apptop::apps::Sampler;
use apptop::icons::{IconLocator, IconStore};
use apptop::ui::{render_to_text, State, Theme};
use common::Sleeper;

#[test]
fn sampling_survives_processes_dying_mid_refresh() {
    let stop = Arc::new(AtomicBool::new(false));
    let churned = Arc::new(AtomicUsize::new(0));

    let churn = {
        let (stop, churned) = (stop.clone(), churned.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                // Tandas de procesos que nacen y mueren enseguida.
                let batch: Vec<Sleeper> = (0..8).map(|_| Sleeper::spawn()).collect();
                std::thread::sleep(Duration::from_millis(2));
                churned.fetch_add(batch.len(), Ordering::Relaxed);
                drop(batch); // mata y espera a cada uno
            }
        })
    };

    let mut sampler = Sampler::new();
    let mut store = IconStore::with(IconLocator::with_theme("hicolor"), None);
    let mut state = State::new(Theme::default());
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut samples = 0;
    while Instant::now() < deadline {
        // Detalle de la app seleccionada: ejercita PSS, disco, GPU y red,
        // que leen archivos de procesos que pueden desaparecer.
        let sample = sampler.sample_with_detail(state.selected.as_deref());
        state.apply(sample);
        if samples % 2 == 0 {
            state.enter_detail();
        } else {
            state.back_to_grid();
        }
        // Recorre todas las apps para que también toque la de los `sleep`.
        let next = state
            .selected_index()
            .map_or(0, |i| (i + 1) % state.order.len().max(1));
        state.select_index(next);
        let text = render_to_text(100, 30, &mut state, &mut store);
        assert!(!text.is_empty());
        samples += 1;
    }

    stop.store(true, Ordering::Relaxed);
    churn.join().expect("el hilo de churn no debe fallar");
    assert!(samples >= 10, "solo {samples} muestras");
    assert!(
        churned.load(Ordering::Relaxed) >= 100,
        "pocos procesos: {churned:?}"
    );
}
