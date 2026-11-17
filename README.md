# apptop

Monitor de recursos para terminal Linux, al estilo de btop pero centrado en
**aplicaciones** en lugar de procesos sueltos.

- La pantalla principal es una grilla con el icono de cada app y servicio en
  ejecución. Cada icono es el icono real de la app convertido a un contorno de
  líneas en braille (sin relleno ni fondo), pintado con su color dominante.
- Al abrir un icono se ve el detalle de esa app: todos sus procesos en árbol,
  CPU, RAM, disco, GPU y red, con gráficos de los últimos dos minutos.
- Desde ahí (o desde la grilla) se puede cerrar, matar, pausar, cambiar la
  prioridad o enviar cualquier señal, a la app entera o a un solo proceso.

No necesita root: lo que no se puede leer se muestra como `n/d`.

## Capturas

Grilla (80×24). Las apps sin icono usan una ventana genérica y los servicios
un engranaje:

```text
apptop  CPU ━━━━────── 37%  RAM ━━━━────── 6.0G/16.0G  Swap ━───────── 512M/8.0G
── Aplicaciones (3) ────────────────────────────────────────────────────────────
      ⢀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⡀  ╭ ⢀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⡀ ╮  ⢀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⡀
     ⢸⠁⡖⡆⢰⢲⢀⡖⡆     ⠈⡇ │⢸⠁⡖⡆⢰⢲⢀⡖⡆     ⠈⡇│ ⢸⠁⡖⡆⢰⢲⢀⡖⡆     ⠈⡇
     ⢸⠤⠭⠥⠬⠭⠤⠭⠥⠤⠤⠤⠤⠤⠤⡇ │⢸⠤⠭⠥⠬⠭⠤⠭⠥⠤⠤⠤⠤⠤⠤⡇│ ⢸⠤⠭⠥⠬⠭⠤⠭⠥⠤⠤⠤⠤⠤⠤⡇
     ⢸              ⡇ │⢸              ⡇│ ⢸              ⡇
     ⢸              ⡇ │⢸              ⡇│ ⢸              ⡇
     ⢸              ⡇ │⢸              ⡇│ ⢸              ⡇
     ⢸⡀            ⢀⡇ │⢸⡀            ⢀⡇│ ⢸⡀            ⢀⡇
      ⠈⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠁  │ ⠈⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠁ │  ⠈⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠁
        Alacritty     │    Firefox     │ Un nombre largu…
        2.1% 310M     ╰   23.5% 5.3G   ╯   120.0% 12.0M

── Servicios (2) ───────────────────────────────────────────────────────────────
           ⢰⠒⠒⡆              ⢰⠒⠒⡆
       ⡔⠉⠲⠔⠊  ⠑⠢⠖⠉⢢      ⡔⠉⠲⠔⠊  ⠑⠢⠖⠉⢢
       ⢘⠆  ⣀⠤⠤⣀  ⠰⡃      ⢘⠆  ⣀⠤⠤⣀  ⠰⡃
     ⢰⠒⠊  ⡜⠁  ⠈⢣  ⠑⠒⡆  ⢰⠒⠊  ⡜⠁  ⠈⢣  ⠑⠒⡆
     ⠸⠤⢄  ⢣⡀  ⢀⡜  ⡠⠤⠇  ⠸⠤⢄  ⢣⡀  ⢀⡜  ⡠⠤⠇
       ⢨⠆  ⠉⠒⠒⠉  ⠰⡅      ⢨⠆  ⠉⠒⠒⠉  ⠰⡅
       ⠣⣀⠴⠢⢄  ⡠⠔⠦⣀⠜      ⠣⣀⠴⠢⢄  ⡠⠔⠦⣀⠜
           ⠸⠤⠤⠇              ⠸⠤⠤⠇
          docker           pipewire
Enter abrir  / filtrar  s orden: nombre  x cerrar  K matar  p pausar  q salir
```

Detalle de una app (80×24):

```text
apptop  CPU n/d  RAM n/d  Swap n/d                   ⟳ 1s · 3 apps · 2 servicios
                          Firefox                           CPU 23.5%  tope 200%
⢀⠔⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠢⡀  aplicación · app                             ⢠⣤⡄
⢸ ⢰⡋⢙⡆⢸⠉⢹ ⣞⠉⣳          ⡇                                               ⢸⣿⡇
⢸⣀⣀⣉⣉⣀⣈⣉⣉⣀⣈⣉⣁⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⡇  Usuario    ana                               ⢸⣿⡇
⢸                      ⡇  Activa     2h 03m                            ⢸⣿⡇
⢸                      ⡇  Procesos   3 · 172 hilos               ⣀⣀⣀⣀⣀⣤⣼⣿⣷⣶⣶⣾⣿⣿⣿
⢸                      ⡇  PIDs       4100 4130 4222         RAM 5.3G   tope 6.6G
⢸                      ⡇  CPU        23.5%
⢸                      ⡇  RAM        5.3G  (PSS 1.2G)              ⢀⣀⣀⣀⣤⣤⣤⣤⣶⣶⣶⣾⣿
⢸                      ⡇  Disco      L 2.3M/s  E 342K/s          ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
⠈⠢⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠔⠁  GPU        18% gfx · VRAM 512M         ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
                          Red aprox. ↓820K/s ↑50.8K/s            ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿

     PID  PROCESO                                  CPU%     RAM HILOS  ESTADO
    4100  firefox                                   1.5    900M   120  S duerme
    4130  ├─ firefox -contentproc -isForBrowser…    1.5    400M    30  S duerme
    4222  └─ firefox                               97.2    250M    22  T pausado





x cerrar  K matar  p pausar  n prioridad  S señal  Esc volver  q salir
```

Toda acción destructiva pide confirmación mostrando a qué y a cuántos procesos
afecta:

```text
⠈⠢⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤│ (se excluye 1 proceso del que depende apptop) │ ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
               │                                               │ ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
               │ [s] sí     [n] no                             │
     PID  PROCE╰───────────────────────────────────────────────╯HILOS  ESTADO
    4100  firefox                                   1.5    900M   120  S duerme
    4130  ├─ firefox -contentproc -isForBrowser…    1.5    400M    30  S duerme
    4222  └─ firefox                               97.2    250M    22  T pausado


```

Un icono real convertido (`apptop --dump-icon firefox --large`) y los dos
iconos genéricos:

```text
             ⢀⡤⢲⡀
            ⢠⠋  ⠑⣄
   ⣠⠞⡇⢀⡠⡄  ⢠⠇    ⠈⠳⡀⣀
  ⡜⠁ ⠳⠫⢤⣇⡤⠔⠚⠒⠲⢤⣀   ⠘⢿⣣⡀
 ⡼  ⣀⣀⣀⣀⠉⠳⢤⣀  ⠰⣾⠷⡀  ⠈⡆⢣
 ⡇ ⠘⢧⣀⡀⠈⣉⣓⡶⠟   ⠘⡆⠙⡆ ⢀⢧⢸⡀
⠸⣄    ⠇⡞⠁       ⢸ ⢸⢀⡞⢸⢠⠇
 ⣿⡄   ⠈⢳⡀      ⣠⠇⢀⡾⠋ ⡏⢸
 ⠘⡽⣄    ⠙⠲⠤⢄⡤⠤⠞⠁⠒⠙⠂⢀⡼⢡⠇
  ⠘⢎⠳⣄⡀          ⢀⡴⠋⡰⠃
    ⠑⢤⡙⠒⠤⢤⣀⣀⣀⣀⡤⠤⠚⢉⡤⠊
      ⠈⠑⠲⠤⠤⠤⠤⠤⠤⠖⠊⠁
```

```text
      ⢰⠒⠒⡆         ⢀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⡀
  ⡔⠉⠲⠔⠊  ⠑⠢⠖⠉⢢    ⢸⠁⡖⡆⢰⢲⢀⡖⡆     ⠈⡇
  ⢘⠆  ⣀⠤⠤⣀  ⠰⡃    ⢸⠤⠭⠥⠬⠭⠤⠭⠥⠤⠤⠤⠤⠤⠤⡇
⢰⠒⠊  ⡜⠁  ⠈⢣  ⠑⠒⡆  ⢸              ⡇
⠸⠤⢄  ⢣⡀  ⢀⡜  ⡠⠤⠇  ⢸              ⡇
  ⢨⠆  ⠉⠒⠒⠉  ⠰⡅    ⢸              ⡇
  ⠣⣀⠴⠢⢄  ⡠⠔⠦⣀⠜    ⢸⡀            ⢀⡇
      ⠸⠤⠤⠇         ⠈⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠁
```

En la terminal los contornos van en color; acá se ven en texto plano.

## Instalación

Requiere Linux con cgroup v2 y systemd, y Rust estable (edición 2021).

```sh
git clone <url-del-repo> apptop
cd apptop
cargo install --path .        # instala en ~/.cargo/bin/apptop
```

O solo compilar: `cargo build --release` deja el binario en
`target/release/apptop`. Es un único ejecutable de unos 3 MB sin dependencias
en tiempo de ejecución.

No lo ejecutes con `sudo`: no hace falta, y como root las acciones afectarían a
procesos de todo el sistema.

Para ver bien los iconos hace falta una fuente con braille y una terminal con
color de 24 bits (si no, se degrada a 256 colores). El tamaño mínimo es 80×24.

## Atajos

### Grilla

| Tecla | Acción |
|-------|--------|
| `←` `↑` `↓` `→` / `h` `j` `k` `l` | Mover la selección |
| `Inicio` / `g`, `Fin` / `G`, `RePág`, `AvPág` | Saltar |
| `Enter` | Abrir el detalle de la app |
| `/` | Filtrar por texto (nombre, id o nombre de algún proceso) |
| `s` | Cambiar el orden: nombre → CPU → RAM |
| `Tab` | Ver todo → solo apps → solo servicios |
| `Esc` | Quitar el filtro |
| `q` / `Ctrl+C` | Salir |

Mouse: un clic selecciona, un clic sobre la app ya seleccionada la abre, y la
rueda mueve una fila.

### Detalle

| Tecla | Acción |
|-------|--------|
| `↑` `↓` / `j` `k` | Elegir un proceso del árbol (por encima del primero: toda la app) |
| `Esc` / `Retroceso` | Volver a la grilla, con la misma selección |
| `q` | Salir |

### Acciones (en la grilla y en el detalle)

Actúan sobre la app seleccionada. En el detalle, si hay un proceso elegido en
el árbol, actúan solo sobre ese proceso.

| Tecla | Acción |
|-------|--------|
| `x` | Cerrar (SIGTERM), con confirmación |
| `K` | Matar (SIGKILL), con confirmación |
| `p` | Pausar / reanudar (SIGSTOP / SIGCONT) |
| `n` | Cambiar la prioridad (nice) |
| `S` | Menú para elegir cualquier señal |
| `+` / `-` | Refresco más rápido / más lento |

En los diálogos solo `s` confirma; cualquier otra tecla cancela.

apptop nunca se envía señales a sí mismo, y no pausa a la shell ni a la
terminal que lo contienen. Subir la prioridad de un proceso (bajar su nice)
requiere privilegios: en ese caso se informa qué permiso falta.

## Línea de comandos

```text
apptop                        interfaz interactiva
apptop -r 2                   refresco cada 2 segundos (0.25 a 10)
apptop --list                 apps detectadas, PIDs y recursos, en texto plano
apptop --dump-icon firefox    icono convertido (--large para el tamaño grande)
apptop --snapshot             un cuadro de la grilla, en texto
apptop --snapshot --app firefox --size 120x40
                              un cuadro del detalle de una app
apptop --print-config         configuración efectiva
apptop --config otro.toml     usar otro archivo de configuración
```

`--list`, `--dump-icon` y `--snapshot` no necesitan una terminal: sirven para
scripts y para ver qué detecta apptop.

## Configuración

Opcional, en `~/.config/apptop/config.toml`:

```toml
refresh = 1.0          # segundos entre refrescos (0.25 a 10)
sort = "name"          # name | cpu | ram
icon_size = "large"    # small (8x4) | medium (12x6) | large (16x8)

[theme]
color = "auto"         # auto | truecolor | 256
accent = "#7aa2f7"     # color de la selección y los atajos
icons = "breeze"       # tema de iconos; por defecto, el del escritorio
```

Un valor inválido o una clave desconocida producen una advertencia y se usa el
valor por defecto; nunca impiden arrancar.

## Cómo funciona

**Agrupar procesos en apps.** Se usa el cgroup de cada proceso
(`/proc/<pid>/cgroup`): los scopes `app-*.scope` son apps gráficas (los
`app-flatpak-*`, Flatpak), los `*.service` son servicios de usuario o de
sistema, y los scopes de contenedores se muestran como servicios. Varias
ventanas de la misma app se juntan en una sola. Si un programa con su propio
`.desktop` fue lanzado desde otra app (un navegador abierto desde una
terminal), se separa como app propia. Lo que no tiene un cgroup útil se agrupa
por ejecutable y árbol de padres.

**Nombre e icono.** Salen del archivo `.desktop`, buscado por el nombre del
scope, `StartupWMClass` o `Exec`. El icono se localiza en el tema activo, con
respaldo en hicolor y pixmaps.

**Iconos.** El SVG o PNG se rasteriza, se recorta a su contenido, se detectan
la silueta (canal alfa) y los bordes de color internos (Sobel con supresión de
no-máximos e histéresis), y el resultado se pasa a braille de 2×4 puntos por
celda. El color es el tono dominante, ignorando transparentes, blancos, negros
y grises. Los iconos convertidos se guardan en `~/.cache/apptop/` y se
invalidan si cambia la ruta, la fecha o el tamaño del archivo.

**Datos.** Todo sale de `/proc` y `/sys`, sin ejecutar otros programas.

**CPU.** Todos los porcentajes van sobre el total de la máquina, igual que la
barra superior: 100 % son todos los núcleos ocupados. En un equipo de 12
núcleos, una app que satura un núcleo figura como 8 %, no como 100 %.

## Limitaciones

- **Red: aproximada** (así figura en la interfaz). Sin root el kernel no da
  contadores por proceso; apptop cruza los sockets de la app con los contadores
  TCP de `sock_diag`. Solo cuenta TCP (no UDP ni QUIC) y pierde lo transferido
  por conexiones que se abren y cierran entre dos refrescos.
- **Procesos de otros usuarios:** disco, GPU, red y PSS solo se pueden leer de
  los procesos propios; para el resto se muestra `n/d (sin permiso)`.
- **GPU:** se lee de `fdinfo` (`drm-engine-*`, `drm-memory-*`). Está verificado
  en hardware con Intel (i915); el formato de AMD (amdgpu) está cubierto por
  tests con datos de ejemplo, no probado en una máquina real.
- **Iconos:** se soportan SVG y PNG. El texto dentro de un SVG no se dibuja.
- **Auxiliares reparentados:** un proceso auxiliar con otro ejecutable que
  quedó huérfano (por ejemplo un `crashhelper`) puede aparecer bajo la app
  desde la que se lanzó el programa.

## Desarrollo

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Los tests de interfaz usan `ratatui::backend::TestBackend` con snapshots de
`insta` (`INSTA_UPDATE=always cargo test` para regenerarlos). Los tests de
acciones solo envían señales a procesos `sleep` que ellos mismos lanzan.

`PROGRESS.md` lleva el estado de las tareas y `NOTES.md` las decisiones de
diseño y las mediciones de rendimiento.
