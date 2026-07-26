# Pane — Guía de usuario

> English guide: [user-guide.en.md](user-guide.en.md)

Pane es un visor/revisor rápido y de sensación nativa para macOS. Abre archivos
gigantes (logs, JSON, dumps) al instante, y funciona como puerta de aprobación
para los cambios que generan los agentes de IA. Es un visor/revisor — la edición
está en el roadmap, todavía no existe.

## 1. Instalación

### Homebrew (recomendado — no requiere Rust)

```bash
brew install amondrave/tap/pane
```

Binario universal: funciona en Macs Apple Silicon e Intel. Las descargas de
Homebrew no llevan quarantine, así que no aparece el diálogo de Gatekeeper de
"desarrollador no verificado".

### Desde el código fuente

Requiere un toolchain estable de Rust ([rustup](https://rustup.rs)):

```bash
git clone https://github.com/amondrave/pane
cd pane
./install.sh
```

`install.sh` compila el binario `pane`, lo instala en tu PATH y registra la
integración con agentes de IA (skill de Claude Code). Para compilar sin
instalar: `cargo build --release` → `target/release/pane`.

### Verificar

```bash
pane --version
```

> **Nota (descarga manual):** si bajas el tarball del release desde GitHub con
> el navegador en vez de usar Homebrew, macOS puede poner el binario en
> cuarentena. Se limpia con `xattr -d com.apple.quarantine ./pane`, o con
> clic derecho → Abrir.

## 2. Abrir archivos

```bash
pane server.log          # abre una ventana, al instante — incluso con varios GB
pane data.json
```

- Los archivos enormes se abren por memory-mapping con un **índice de líneas
  perezoso**: Pane solo lee las partes que realmente ves, así que un log de 1 GB
  abre en una fracción de milisegundo usando unos pocos MB de RAM.
- Los archivos de menos de 4 MB en un lenguaje soportado llevan **syntax
  highlighting** (Tree-sitter): JSON, Rust, TOML, Markdown, Java.
- Las líneas largas no se envuelven — panea en horizontal para leerlas.
- `pane --stat <archivo>` imprime métricas de apertura sin abrir ventana, útil
  para benchmarks.

## 3. Navegación y teclas

| Tecla / gesto | Acción |
|---|---|
| Rueda / trackpad | Scroll vertical **y** horizontal |
| `↑` / `↓` | Una línea arriba / abajo |
| `PageUp` / `PageDown` | Una página arriba / abajo |
| `Home` | Inicio del archivo (también resetea el pan horizontal) |
| `End` | Final del archivo (fuerza indexado completo en archivos enormes) |
| `←` / `→` | Pan horizontal (líneas largas) |
| `Esc` | Cerrar (modo visor) / limpiar búsqueda / rechazar (modo review) |

La barra de scroll a la derecha muestra tu posición. En archivos enormes de
carga perezosa refleja lo indexado hasta el momento, y se refina al hacer scroll.

## 4. Búsqueda

| Tecla | Acción |
|---|---|
| `/` | Abrir la búsqueda y escribir la consulta |
| `Enter` | Ejecutar y saltar a la primera coincidencia |
| `n` / `N` | Coincidencia siguiente / anterior (centrada en pantalla) |
| `Esc` | Limpiar la búsqueda (no cierra la ventana) |

La barra de estado muestra `actual/total` de coincidencias. Hoy la búsqueda es
por subcadena literal; el toggle de regex está en el roadmap (el motor ya lo
soporta).

## 5. Modo review — aprobar o rechazar cambios

El modo review convierte a Pane en una **puerta de aprobación bloqueante**: la
ventana se abre, lees, decides, y el veredicto se devuelve como **exit code**
del proceso.

```bash
pane --review archivo.rs                 # revisar un archivo
pane --review --diff viejo.rs nuevo.rs   # revisar un diff coloreado (viejo ↔ nuevo)
pane --review --git                      # revisar TODOS los cambios sin commitear
pane --review --changeset lista.tsv      # revisar un conjunto explícito de pares
```

Teclas de veredicto (aparecen en el pie de la ventana):

| Tecla | Veredicto | Exit code |
|---|---|---|
| `A` o `Enter` | **Aprobar** | `0` |
| `R` o `Esc` | **Rechazar** | `1` |
| `Q` o cerrar la ventana | **Cancelar** | `2` |

- Puedes hacer scroll y buscar libremente antes de decidir.
- `--json` imprime además `{"verdict":"approved"}` a stdout.
- En los diffs: verde = añadido, rojo = borrado, gris = contexto. Los números de
  línea siguen al archivo nuevo (las líneas borradas no llevan número).
- `--git` muestra todos los archivos modificados, añadidos y borrados en una
  sola ventana con un solo veredicto. Los archivos nuevos aparecen completos
  como adiciones.
- El manifiesto de changeset es una línea por archivo:
  `ruta_vieja<TAB>ruta_nueva<TAB>etiqueta` (vieja vacía = archivo nuevo; nueva
  vacía = borrado).

## 6. Usar Pane con agentes de IA

La integración es **agnóstica al modelo**: es solo un comando CLI y sus exit
codes, así que funciona con Claude Code, Codex, Cursor, Gemini — cualquier cosa
que ejecute comandos de shell. Dos patrones:

**Revisar después de editar** — el agente edita tu working tree y corre
`pane --review --git`; si sale `1`, revierte o corrige.

**Revisar antes de escribir** — el agente escribe su propuesta a un archivo
temporal, corre `pane --review --diff actual.rs /tmp/propuesto.rs`, y solo
aplica el cambio si sale `0`. Nada aterriza sin tu aprobación.

Configuración:
- Cualquier agente: pega [`integrations/agents-snippet.md`](../integrations/agents-snippet.md)
  en el archivo de instrucciones de agentes de tu proyecto (`AGENTS.md`,
  `.cursor/rules`, …).
- Claude Code: `install.sh` instala el skill `pane-review` globalmente, así que
  basta decir *"revisa esto en Pane"* en cualquier proyecto.

Requiere un humano frente a la pantalla — es una puerta interactiva, no sirve
para CI headless.

## 7. Solución de problemas

- **`pane: command not found`** — asegúrate de que `~/.cargo/bin` (instalación
  desde fuente) o el prefijo de Homebrew estén en tu PATH. Para agentes que usan
  shells no interactivos, añade `. "$HOME/.cargo/env"` a tu `~/.zshrc`.
- **No aparece ventana por SSH/headless** — Pane necesita un display; usa
  `--stat` para métricas o córrelo en local.
- **Un archivo no muestra colores** — el highlighting aplica a extensiones
  soportadas (`.json .rs .toml .md .java`) de menos de 4 MB; los archivos más
  grandes quedan en plano a propósito.
- **Reportar problemas** — <https://github.com/amondrave/pane/issues>.
