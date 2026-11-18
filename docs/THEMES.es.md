# Temas

apptop toma todos sus colores de un tema: un archivo TOML que nombra **roles**
(fondo, texto, acento, ok / aviso / peligro…) en vez de colores sueltos.
Vienen 30 temas incluidos y puedes agregar los tuyos.

Read in English: [THEMES.md](THEMES.md).

## Elegir un tema

Pulsa **`T`** en la grilla o en el detalle.

| Tecla | Acción |
|---|---|
| `↑` `↓` `RePág` `AvPág` `Inicio` `Fin` | Moverse. Toda la pantalla pasa al tema bajo el cursor al instante. |
| cualquier letra | Filtrar la lista por nombre, id o familia. `Retroceso` borra. |
| `Enter` | Dejar el tema y guardarlo en la configuración. |
| `Esc` | Cerrar y volver al tema que tenías. |

`*` marca el tema que estaba activo al abrir el selector. Los cinco puntos
junto a cada nombre son colores de ese tema (acento, ok, aviso, peligro y uno
más).

`Enter` escribe la elección como `name` bajo `[theme]` en
`~/.config/apptop/config.toml` (o el archivo dado con `--config`):

```toml
[theme]
name = "nord"
```

Los comentarios y las demás opciones del archivo no se tocan.

## Temas incluidos

| Familia | Temas (id) |
|---|---|
| Default | `apptop` |
| Popular | `catppuccin-mocha`, `catppuccin-macchiato`, `catppuccin-frappe`, `catppuccin-latte`, `dracula`, `everforest`, `gruvbox-dark`, `gruvbox-light`, `kanagawa`, `monokai`, `nord`, `one-dark`, `rose-pine`, `solarized-dark`, `solarized-light`, `tokyo-night` |
| Light & pastel | `paper`, `pastel-dawn`, `mint-cream`, `lavender-mist` |
| Retro & neon | `phosphor-green`, `amber`, `mono-gray`, `cyberpunk`, `synthwave` |
| Accessible | `high-contrast-dark`, `high-contrast-light`, `okabe-ito-dark`, `okabe-ito-light` |

`apptop` es el aspecto original: conserva el fondo y el color de texto de la terminal.
Los dos temas Okabe-Ito son seguros para el daltonismo rojo-verde; los de
alto contraste tienen 21:1 de contraste de texto.

Donde una paleta publicada tenía un color demasiado tenue sobre su propio
fondo, se lo acercó al color del texto lo justo para llegar a 3:1.

## Crear tu propio tema

1. Copia [`example-theme.toml`](example-theme.toml) a
   `~/.config/apptop/themes/<nombre>.toml` (crea la carpeta si no existe; con
   `XDG_CONFIG_HOME` definido es `$XDG_CONFIG_HOME/apptop/themes`).
2. Cambia los colores.
3. Abre apptop y pulsa `T`. Tu tema aparece bajo su `family` ("User" si no
   pones otra).

El nombre del archivo sin `.toml`, en minúsculas, es el **id** del tema. Un
archivo con el id de un tema incluido lo reemplaza.

Si el archivo tiene un error, apptop arranca igual. Aparece un aviso, y el
selector lista el archivo bajo "No se pudo cargar" con la línea y el motivo,
por ejemplo `colors.warn: missing required role`.

### Colores

| Escribes | Significa |
|---|---|
| `"#rrggbb"` | Un color de 24 bits. |
| `"terminal"` | El color por defecto de la terminal (fondo o texto). No vale en degradados. |
| `"red"`, `"lightblue"`, `"gray"`… | Uno de los 16 colores de la terminal, por nombre. |
| `"214"` | Un índice de la paleta de 256 colores. |

### Claves generales

| Clave | Valores | Por defecto |
|---|---|---|
| `name` | Texto que muestra el selector | el id |
| `family` | Título del grupo en el selector | `"User"` |
| `appearance` | `"dark"` o `"light"` | se deduce de `bg` |
| `icon_mode` | `"app"`, `"tint"` o `"mono"` | `"app"` |
| `high_contrast` | `true` o `false` | `false` |
| `assumed_bg` | Un color; solo se usa si `bg = "terminal"` | negro o blanco, según `appearance` |

`icon_mode` dice cómo se colorean los iconos de las apps: `app` dibuja cada
icono con el color dominante del icono real, `tint` usa el color del tema de
tono más cercano y `mono` dibuja todos con el color `icon`.
`[theme] icon_mode = "…"` en la configuración lo impone para todos los temas.

### Roles obligatorios (`[colors]`)

| Rol | Se usa en |
|---|---|
| `bg` | Fondo de toda la pantalla |
| `fg` | Texto normal |
| `dim` | Texto secundario, ayudas |
| `accent` | Atajos, títulos de diálogos, el elemento activo |
| `border` | Marcos y separadores |
| `selection_fg`, `selection_bg` | La fila o app seleccionada |
| `ok` | Carga baja; lo que se achicó |
| `warn` | Carga media; avisos que no son errores |
| `danger` | Carga alta; errores; borrado |

### Roles opcionales (`[colors]`)

| Rol | Se usa en | Si falta |
|---|---|---|
| `header` | Títulos de sección y etiquetas de las barras | `fg` |
| `ram` | Gráfico y cifras de memoria | `accent` |
| `icon` | Iconos en modo `mono`; iconos grises en modo `tint` | `accent` |
| `status_fg`, `status_bg` | Reservados para una barra de estado | `selection_fg`, `selection_bg` |

Estos roles se aceptan para que un archivo de tema pueda traerlos, pero
apptop no dibuja nada con ellos: `connector`, `dir`, `marked`, `matched`,
`grew`, `shrank`, `new`, `value`.

### Degradados (`[gradients]`)

Dos listas de dos o más colores, de menor a mayor: `graph` y `size`. Las dos
valen `[ok, warn, danger]` si faltan. apptop colorea la carga por umbrales
con `ok`, `warn` y `danger`, así que puedes omitirlas.

### Legibilidad

Los temas incluidos pasan un test de contraste: texto sobre fondo al menos
4.5:1 (7:1 en los de alto contraste), selección y barra de estado al menos
4.5:1, y cada rol de color al menos 3:1 sobre el fondo. Tus temas no se
revisan, así que puedes saltarte esos mínimos; son una buena meta si algo
cuesta leer.

## Terminales con menos colores

Los temas se escriben en 24 bits y se convierten a lo que la terminal puede
mostrar:

| Terminal | Qué pasa |
|---|---|
| `COLORTERM=truecolor` o `24bit` | Los colores tal como están escritos. |
| `TERM` contiene `256` | El color más cercano de la paleta de 256. |
| Cualquier otra | El más cercano de los 16 colores básicos; el texto nunca cae sobre el color de su propio fondo. |
| `NO_COLOR` definido, o `TERM=dumb` | Sin colores: solo negrita, tenue y video inverso. |

`[theme] color = "truecolor" | "256" | "16"` en la configuración fuerza una
profundidad.
