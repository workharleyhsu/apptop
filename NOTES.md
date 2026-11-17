# NOTES — apptop

Decisiones y problemas, por iteración.

## Entorno de desarrollo
- Rust 1.99.1, Fedora 44, cgroup v2, terminal truecolor, tema de iconos `breeze`.
- GPU local: Intel i915 (no AMD). `cargo-insta` no instalado: los snapshots se aceptan con
  `INSTA_UPDATE=always cargo test` y se revisa el diff del `.snap`.

## Iteración 1 — estructura
- `lib.rs` + `main.rs` fino para que los tests de integración usen la librería.
- Las dependencias se agregan en la tarea que las necesita, verificando la API en docs.rs.

## Iteración 2 — lectura de procesos
- `procfs` sin features por defecto (sin chrono/flate2). `Collector::with_root` permite fixtures.
- CPU % por delta de ticks; identidad `(pid, starttime)` contra reutilización de PIDs.
- exe/cmdline/cgroup se cachean, pero se releen las 3 primeras muestras de un proceso nuevo:
  systemd lo mueve a su scope después del fork. PSS solo bajo demanda (`Collector::pss`).
- `--list-procs` queda como modo de depuración.

## Iteración 3 — agrupación
- El nombre de la unidad se parte por `-` ANTES de desescapar: los guiones del AppID vienen
  como `\x2d`. Instancias del mismo AppID se fusionan en una app.
- `app-*.service` (autostart) cuenta como servicio pero conserva el AppID para el `.desktop`.
- Sin unidad útil (session-N.scope, init.scope, run-*.service): un grupo por árbol de padres,
  con el nombre del ejecutable de la raíz. Hilos del kernel fuera.
- Pendiente para la tarea 4: firefox lanzado desde una terminal vive en el scope de alacritty;
  hay que separar subárboles que correspondan a otro `.desktop`.

## Iteración 4 — .desktop
- `freedesktop-desktop-entry` sin la feature `gettext` (evita enlazar libintl).
- Búsqueda: id exacto → último segmento del id → `StartupWMClass` → ejecutable de `Exec=`.
- Separación de subárboles: dentro de un scope de app (o sin cgroup útil), un proceso cuyo
  ejecutable apunta a OTRO `.desktop` arrastra a sus hijos a una app propia. Caso real:
  Firefox lanzado desde Alacritty. La clave de app es el id del `.desktop`, así se fusiona
  con instancias que sí tienen scope propio.
- El índice por ejecutable ignora entradas `Terminal=true`/`NoDisplay=true` y lanzadores
  genéricos (sh, python, env...): `fish.desktop` no debe partir una terminal.
- Los servicios nunca se parten; solo toman icono (y nombre si son `app-*.service`).
- La ruta real del icono se resuelve en la tarea siguiente (Fase 2); acá queda `Icon=`.
