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

## Iteración 12 — barras
- `/proc/stat` y `/proc/meminfo` se parsean a mano (raíz inyectable para tests). RAM usada =
  MemTotal − MemAvailable. Lo ilegible se muestra `n/d`; sin swap se muestra "sin swap".
- La barra inferior depende de vista y modo; cada atajo tiene prioridad y los menos
  importantes se descartan si no entran (a 80 columnas se cae "mover").
- Con menos de 3 filas se omiten las barras y queda solo el contenido.

## Iteración 13 — refresco
- `--refresh/-r <seg>` y teclas `+`/`-` (pasos de 250 ms, entre 0.25 s y 10 s); el intervalo
  se ve en la barra superior. El intervalo vive en `State` para que la tecla rija al instante.
- Sin parpadeo: solo se dibuja si algo cambió y ratatui manda solo las celdas distintas.
  Verificado bajo pty: 0 secuencias `ESC[2J` en 7 cuadros, ~0.8 kB por refresco.

## Iteración 14 — cabecera del detalle
- `Sampler::sample_with_detail(id)` mide lo caro (por ahora PSS vía `smaps_rollup`) solo para
  la app abierta; al entrar al detalle se fuerza una muestra inmediata.
- Uptime = uptime del sistema − starttime del proceso más antiguo. Usuario vía
  `nix::unistd::User::from_uid`, cacheado por uid (respeta NSS, a diferencia de leer passwd).
- PSS ilegible (procesos ajenos) → se suman solo los legibles; ninguno legible → `n/d`.
- `--snapshot --app <nombre>` renderiza el detalle (toma una segunda muestra con detalle).

## Iteración 15 — árbol de procesos
- Raíces = procesos cuyo padre no pertenece a la app; hermanos por pid; tolera ciclos.
- Cursor de proceso: arranca en "toda la app" (sin fila elegida); ↓ baja al primer proceso y
  ↑ desde el primero vuelve a "toda la app". Lo usarán las acciones de la Fase 5.
- Limitación conocida: un auxiliar reparentado con otro ejecutable (p. ej. `crashhelper` de
  Firefox lanzado desde una terminal) queda en el grupo de la terminal.

## Iteración 16 — gráficos históricos
- Se guarda historial (CPU %, RSS, con marca de tiempo) de TODAS las apps, no solo la abierta:
  así el gráfico ya tiene datos al entrar. Ventana de 120 s por tiempo, no por cantidad de
  muestras, para que valga con cualquier intervalo de refresco.
- El gráfico siempre abarca 2 min: la ventana se reparte en 2 columnas de puntos por celda y
  cada tramo toma el pico. A 80 columnas quedan 22 celdas (≈2.7 s por punto).
- Eje de CPU = múltiplo de 100 % que contiene el pico; eje de RAM = pico + 25 %, desde 0.
- Verificado en vivo bajo pty reconstruyendo la pantalla final con un emulador mínimo.

## Iteración 17 — disco
- `read_bytes`/`write_bytes` de `/proc/<pid>/io` (E/S real a almacenamiento, no rchar/wchar),
  sumados sobre los procesos de la app abierta. Tres estados: tasa, "midiendo…" (primera
  lectura) y `n/d (sin permiso)` — `io` solo es legible para procesos propios.
- Un pid nuevo o reciclado solo fija referencia; lo escrito por procesos ya muertos se pierde.

## Iteración 18 — GPU
- Solo se lee el fdinfo de descriptores cuyo enlace apunta a `/dev/dri/*` (no el de cada
  archivo abierto). Clientes deduplicados por `drm-pdev` + `drm-client-id` entre procesos.
- GPU % = Δns del motor / Δt, sumado entre clientes y dividido por `drm-engine-capacity-*`;
  se muestra el motor más ocupado con su nombre. Memoria: región `vram` si existe (dedicada),
  si no la suma de `drm-resident-*` (integrada, etiquetada "RAM").
- **AMD no verificado en hardware**: esta máquina tiene Intel i915. amdgpu se cubre con un
  fixture con el formato de la documentación del kernel; i915 se verificó en vivo
  (Firefox reproduciendo vídeo: "21% video").
- Estados: tasa, "midiendo…", "sin uso" (legible y sin DRM) y `n/d (sin permiso)`.

## Iteración 19 — red por app (aprox.)
- Viable sin root: inodos `socket:[N]` de `/proc/<pid>/fd` cruzados con el volcado TCP de
  netlink `sock_diag` (INET_DIAG_INFO → `tcp_info.bytes_acked` / `bytes_received`).
  Crates: `netlink-packet-sock-diag` (feature `rich_nlas`), `netlink-sys`, `netlink-packet-core`.
- Por qué es "aprox." (así se etiqueta en la UI): solo TCP (UDP/QUIC no tienen contadores en
  sock_diag → vídeo por HTTP/3 no se ve), se pierde lo de conexiones que abren y cierran
  entre dos muestras, y "subida" son bytes confirmados por el otro extremo.
- Solo para procesos propios (`fd` ajeno no es legible) → `n/d (sin permiso)`.
- Verificado con el kernel real: test con conexión TCP local propia, y en vivo una
  transferencia local de ~4 MB/s se midió como ↓3.8M/s ↑3.8M/s.
- De paso: bloque de datos del detalle con etiquetas de 11 columnas y mínimo de 32.

## Iteración 20 — volver a la grilla
- `Esc`/Backspace vuelven del detalle sin tocar selección, scroll, orden ni filtro (ya salía
  así de la iteración 10); se agregaron tests que lo fijan, incluido que el cuadro de la
  grilla se redibuja idéntico.

## Iteración 21 — cerrar y matar
- `x` = SIGTERM, `K` (mayúscula, para no confundir con "arriba") = SIGKILL. Siempre pasan por
  un diálogo que muestra nombre y cantidad de procesos; solo `s`/`y` confirma, cualquier otra
  tecla cancela y el mouse queda inactivo mientras está abierto.
- El manejo de teclas solo deja la acción en `State::pending`; el bucle la ejecuta. Así los
  tests de entrada nunca envían señales.
- Guardas en `actions/signal.rs`: antes de cada `kill` se compara el `starttime` (pid
  reutilizado → no se toca); nunca pid ≤ 1; apptop se excluye siempre, y con señales de
  detención también sus ancestros (shell y terminal), para no congelar la interfaz.
- Verificación de punta a punta SIN tocar procesos ajenos: `systemd-run --user --scope
  --unit=app-apptoptestNNN-1.scope` con dos `sleep` propios; se comprueba que `--list` vea
  exactamente esos PIDs y recién entonces la TUI (bajo pty) filtra por ese nombre, `x`, `n`
  (siguen vivos) y `x`, `s` (terminan los 3).

## Iteración 22 — pausar/reanudar
- `p` alterna: si todos los procesos del alcance están detenidos (`T`) manda SIGCONT, si no
  SIGSTOP. Sin confirmación: es reversible con la misma tecla. El atajo de la barra dice
  "pausar" o "reanudar" según corresponda y la grilla marca la app con "‖ pausada".
- SIGSTOP nunca va a apptop ni a sus ancestros; si no queda a quién enviarla, aviso de error.
- Verificado con `sleep` propios en un scope propio: S S S → T T T → S S S.

## Iteración 23 — prioridad (nice)
- `n` abre un selector (-20..19, ←/→ o +/-, Enter aplica). `setpriority` vía `libc` (nix no
  lo envuelve). En Linux nice es por hilo: se recorre `/proc/<pid>/task/*`, si no solo
  cambiaría el hilo principal (como hace `renice`).
- Mensajes distintos por errno: EACCES → "subir la prioridad (bajar nice) requiere root o
  CAP_SYS_NICE"; EPERM → "son de otro usuario". El selector avisa antes de aplicar si el
  valor elegido sube la prioridad.
- Verificado con `sleep` propios: 0 → 5 aplicado a los 3; 5 → 0 rechazado con el mensaje.

## Iteración 24 — menú de señales y acciones por proceso
- `S` (mayúscula; `s` sigue siendo ordenar) abre un menú con 9 señales. Todas pasan por la
  confirmación salvo SIGSTOP/SIGCONT, que se deshacen entre sí.
- Alcance de TODA acción (cerrar, matar, pausar, prioridad, señal): en el detalle, si hay un
  proceso elegido en el árbol actúa solo sobre ese; si no (o en la grilla), sobre la app
  entera. El diálogo siempre dice a qué y a cuántos procesos afecta.
- Verificado con `sleep` propios: SIGTERM desde el menú a una fila del árbol terminó solo
  ese proceso; los otros dos siguieron vivos.

## Iteración 25 — tests de acciones
- `tests/actions.rs`: SIGTERM/SIGKILL a un grupo, SIGSTOP→`T`→SIGCONT, un solo proceso sin
  tocar hermanos, pid "reciclado" (starttime distinto) intacto, proceso ya terminado, nice
  (subir, tope 19, y el rechazo al bajar). Todo sobre `sleep 600` creados por
  `tests/common::Sleeper`, cuyo `Drop` mata y espera solo a su propio hijo.
- `tests/stress.rs` (criterio de aceptación): un hilo crea y mata tandas de 8 `sleep` cada
  2 ms durante 3 s mientras se muestrea con detalle (PSS, disco, GPU, red) y se dibuja.
- Tras correr la suite no queda ningún `sleep 600` vivo.

## Iteración 26 — configuración
- `~/.config/apptop/config.toml` (o `--config <archivo>`): `refresh`, `sort`, `icon_size`
  (small 8×4 / medium 12×6 / large 16×8) y `[theme]` con `color` (auto/truecolor/256),
  `accent` (#rrggbb) e `icons` (tema de iconos). `--print-config` muestra la efectiva.
- Tolerante: se valida clave por clave; lo inválido o desconocido genera una advertencia en
  stderr y usa el valor por defecto. TOML roto → todo por defecto. Un archivo ausente solo se
  avisa si se pidió con `--config`. La línea de comandos (`--refresh`) pisa al archivo.
- `toml` fijado en `~1.1`: la 1.2.1 pide `toml_writer ^1.2.0`, que no resolvía en el índice.
- Con iconos chicos el resumen se acorta por pasos ("11.0% 5.9G" → "11% 5.9G" → "120% 12M"
  → solo CPU) para no invadir la baldosa vecina.

## Iteración 27 — resize, terminal chica, panic
- Menos de 80×24 → aviso centrado con el tamaño actual y el mínimo, en vez de la interfaz
  (`State::enforce_min_size`; en los tests unitarios está apagado por defecto porque dibujan
  a propósito en lienzos chicos, y el aviso tiene su propio test).
- Resize: ratatui reajusta el buffer en cada `draw`; el evento solo pide redibujar. El scroll
  y la selección se recalculan contra el área nueva en cada cuadro.
- Panic: hook propio que primero quita la captura de mouse y restaura la terminal, y recién
  después deja imprimir el mensaje. `--panic-test` (oculto) lo provoca para probarlo.
- SIGTERM/SIGHUP/SIGINT: manejador que solo marca un `AtomicBool`; el bucle sale ordenado.
- Verificado en vivo bajo pty: achicar a 60×20 muestra el aviso y agrandar lo quita; panic →
  código 101 con terminal restaurada y mensaje legible; SIGTERM/SIGHUP → salida limpia.
