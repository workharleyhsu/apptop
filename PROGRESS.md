# PROGRESS — apptop

## Fase 1 — Base
- [x] Proyecto cargo, estructura de módulos (`collect/`, `apps/`, `icons/`, `ui/`, `actions/`).
- [x] Lectura de procesos: pid, ppid, nombre, exe, cmdline, CPU %, RAM (RSS/PSS), estado.
- [x] Agrupación en apps por cgroup con los fallbacks descritos. `--list` funcionando.
- [x] Asociación con `.desktop` (nombre visible + ruta del icono).

## Fase 2 — Iconos
- [x] Localizar icono según tema activo, con fallback a hicolor y pixmaps.
- [x] Pipeline SVG/PNG → bordes → braille + color dominante. `--dump-icon` funcionando.
- [x] Iconos genéricos (servicio, app sin icono) en el mismo estilo.
- [x] Caché en disco.

## Fase 3 — Pantalla principal
- [x] Grilla de iconos adaptable, con nombre + CPU/RAM bajo cada uno. `--snapshot`.
- [x] Navegación: flechas y hjkl, Enter para entrar, mouse (clic y scroll), `q` salir.
- [x] Orden (CPU, RAM, nombre), filtro por texto (`/`), separación apps / servicios.
- [x] Barra superior con totales del sistema (CPU, RAM, swap) y barra inferior de atajos.
- [x] Refresco configurable (default 1 s) sin parpadeo.

## Fase 4 — Vista de detalle
- [x] Cabecera con icono grande, nombre, PIDs, uptime, usuario.
- [x] Árbol de procesos de la app con CPU, RAM, hilos y estado por proceso.
- [x] Gráficos históricos de CPU y RAM (braille, últimos ~2 min).
- [x] Disco: lectura/escritura por segundo desde `/proc/<pid>/io`.
- [x] GPU/VRAM por proceso desde `/proc/<pid>/fdinfo` (claves `drm-engine-*`,
      `drm-memory-*`; objetivo principal: AMD). Deduplicar por `drm-client-id`.
- [x] Red por app: aproximación sin root usando sockets de la app (netlink `sock_diag`
      + `/proc/<pid>/fd`). Etiquetar como "aprox." en la UI. Si no es viable, `n/d`
      y documentar el motivo en `NOTES.md`.
- [x] `Esc` vuelve a la grilla conservando la selección.

## Fase 5 — Acciones
- [x] Cerrar app (SIGTERM a todo el grupo) y matar (SIGKILL), con confirmación.
- [x] Pausar/reanudar (SIGSTOP/SIGCONT).
- [x] Cambiar prioridad (nice), informando con claridad si falta permiso.
- [x] Menú para elegir señal, y aplicar cualquier acción a un solo subproceso del árbol.
- [ ] Tests de acciones usando solo procesos `sleep` lanzados por el test.

## Fase 6 — Pulido
- [ ] Config en `~/.config/apptop/config.toml` (refresco, orden, tamaño de icono, tema).
- [ ] Manejo de resize, terminales chicas (mensaje si es menor a 80x24) y restauración
      de la terminal ante panic.
- [ ] Rendimiento: medir y optimizar hasta cumplir los criterios.
- [ ] README con capturas en texto, atajos e instalación.

## Criterios de aceptación
- [ ] `cargo clippy -- -D warnings` y `cargo test` pasan limpios.
- [ ] En reposo con refresco de 1 s: menos de 2 % de un núcleo y menos de 40 MB de RAM.
- [ ] Arranque en menos de 300 ms con la caché de iconos caliente.
- [ ] `--dump-icon` de una app instalada con icono produce un contorno reconocible.
- [ ] Ningún panic al morir procesos durante el refresco (test de estrés con `sleep`).
- [ ] Todas las tareas marcadas o justificadas como `[BLOQUEADA]`.
