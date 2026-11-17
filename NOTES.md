# NOTES — apptop

Decisiones y problemas, por iteración.

## Entorno de desarrollo
- Rust 1.99.1, Fedora 44, cgroup v2, terminal truecolor, tema de iconos `breeze`.
- GPU local: Intel i915 (no AMD). `cargo-insta` no instalado: los snapshots se aceptan con
  `INSTA_UPDATE=always cargo test` y se revisa el diff del `.snap`.

## Iteración 1 — estructura
- `lib.rs` + `main.rs` fino para que los tests de integración usen la librería.
- Las dependencias se agregan en la tarea que las necesita, verificando la API en docs.rs.
