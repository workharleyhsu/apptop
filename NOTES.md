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

## Iteración 5 — localizar iconos
- `freedesktop_icons::lookup` ya hace tema → padres → hicolor → pixmaps. NO se usa su
  `default_theme_gtk()` porque ejecuta `gsettings`: el tema se lee de `gtk-4.0/settings.ini`,
  `gtk-3.0/settings.ini`, `kdeglobals` o `xsettingsd.conf` (o `APPTOP_ICON_THEME`).
- Se pide tamaño 128 para reducir (no ampliar) antes de detectar bordes. Solo SVG/PNG (no XPM).

## Iteración 6 — pipeline de iconos
- resvg sin features (sin texto ni fuentes del sistema: arranque y RAM); `image` solo con PNG.
  Consecuencia: SVG con `<text>` o con imágenes raster embebidas pierden esas partes.
- Flujo: SVG a 256 px → recorte al contenido visible → reducción por promedio de área (alfa
  premultiplicado) al doble de la resolución en puntos → bordes → fusión 2×2 → braille.
- Bordes = silueta por alfa (contorno interior exacto de 1 px) + Canny simplificado sobre el
  color (Sobel por canal, supresión de no-máximos, histéresis). El umbral arranca bajo (70) y
  sube ×1.3 hasta que los bordes internos ocupen ≤ 8.5 % del lienzo: detalle en iconos de
  poco contraste (Alacritty) sin que los muy detallados se vean rellenos.
- Color dominante por histograma de TONOS (36 sectores) ponderado por saturación² y brillo;
  el histograma RGB elegía el gris de fondo de Alacritty en vez del naranja. Si el color es
  muy oscuro se aclara hasta luminancia 110 para que se lea en terminal oscura (desvío
  deliberado del color exacto). Monocromo → gris neutro.
- `--dump-icon <app> [--large] [--color auto|always|never]`: la ruta y el color van a stderr.

## Iteración 7 — iconos genéricos
- Engranaje y ventana se describen como SVG en código y pasan por el mismo pipeline que los
  iconos reales: mismo estilo garantizado, sin un segundo camino de dibujo. Color fijo por tipo.
- `--dump-icon generic:service|generic:app` para verlos.

## Iteración 8 — caché de iconos
- `~/.cache/apptop/<fnv1a(clave)>.icon`; clave = versión del pipeline + ruta + mtime (ns) +
  tamaño en bytes + tamaño en celdas. La clave va dentro del archivo y se compara al leer
  (colisiones y archivos dañados → se reconvierte). Escritura atómica (tmp + rename).
- FNV-1a propio: el hasher por defecto de Rust no es estable entre versiones.
- `IconStore` memoiza también los fallos, para no reintentar un icono roto en cada refresco.
- Si cambia el algoritmo del pipeline hay que subir `PIPELINE_VERSION` en `icons/cache.rs`.

## Iteración 9 — grilla
- ratatui 0.30 sin features por defecto (solo `crossterm`). `ui::draw` es la única función de
  dibujo: la usan la TUI, `--snapshot [--size AxB]` y los tests con `TestBackend` + insta.
- Baldosa = icono + nombre + "CPU% RAM" (18×10 con icono 16×8). La selección se marca con
  barras laterales en las columnas de margen: no gasta filas y no pinta fondo.
- `GridLayout` es puro (ancho → posiciones) y guarda el área dibujada para el mouse.
- El scroll es por líneas con recorte por línea, así una fila puede verse a medias.

## Iteración 10 — navegación
- Los eventos se traducen en `ui/input.rs` con funciones puras sobre `State` (testeables sin
  terminal). Arriba/abajo usan la geometría real de la grilla y cruzan de apps a servicios.
- Mouse: clic selecciona, clic sobre la ya seleccionada entra; la rueda mueve una fila.
- Bucle de un solo hilo: `event::poll` con timeout hasta la próxima muestra (sin despertares
  de sobra). Solo se redibuja si cambió algo.
- `script` no está instalado: el smoke test interactivo se hace con un driver de pty en
  Python (fuera del repo) que manda teclas y verifica código de salida y secuencias.
- La vista de detalle es todavía un título; se completa en la Fase 4.

## Iteración 11 — orden, filtro, secciones
- `s` cicla el orden (nombre → CPU → RAM) dentro de cada sección; las apps van siempre antes.
- `/` entra en modo filtro: ahí toda letra es texto (incluida `q`); Enter lo deja aplicado,
  Esc lo borra. Coincide con nombre, AppID o nombre de algún proceso.
- `Tab` cicla secciones visibles: todo → solo apps → solo servicios.
- La línea del filtro ocupa la última fila solo mientras hay filtro.
