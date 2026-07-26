# HANDOFF — Pane

> Estado vivo entre sesiones: qué está hecho, qué sigue, y contexto para retomar sin
> releer todo. Actualizar al final de cada sesión de trabajo. Fuente de verdad del
> alcance: [PRD.md](PRD.md). Guía operativa: [CLAUDE.md](CLAUDE.md).

_Última actualización: 2026-07-20._

## En una línea

Pane = visor/revisor nativo de archivos gigantes para macOS (Rust + GPU), para el flujo
con agentes IA. Fase actual: **v0.2 completo (ventana GPU + índice perezoso). Próximo: v1.**

## Hecho

- **Workspace Cargo**: `pane-core` (motor) + `pane-app` (binario `pane`).
- **v0 spike paso 1 — core** (validado): mmap + índice de líneas + acceso O(1).
  - Medido en 1 GB / 8.1M líneas: abrir+índice **148 ms** caliente, salto O(1) **~0 µs**,
    índice heap **67 MB**, **RSS pico ~1 GB** (por índice completo).
  - 3 tests verdes en `pane-core`. Comentarios/código en **inglés**.
- **v0.2 paso 2 — ventana GPU** (✅ verificado visualmente por el usuario): `winit 0.30` +
  `wgpu 30` + `glyphon 0.12` con **viewport virtualizado** (solo se pasan a glyphon las
  líneas visibles). Scroll con rueda y flechas/PageUp-Down/Home/End. Tema dark, monospace.
  - Versiones alineadas: glyphon 0.12 → wgpu **30.0.0** + cosmic-text **0.19**.
  - Usuario confirmó: el texto se lee bien y el scroll va bien (probado con JSON).
  - Observaciones pendientes: (a) al final del archivo se ve fondo vacío (scroll-beyond-last-line,
    esperado — decidir si clamp para mantener la última página llena); (b) POR CONFIRMAR si hay
    caracteres tenues en el borde izquierdo en vivo (posible bug de clipping/glifos residuales).
- **Docs**: `idea.md` (visión), `PRD.md` (scope + hallazgos), `README.md` (público, con Usage),
  `CLAUDE.md` (guía), `LICENSE` (MIT, Angel Mondragon).

## Hecho (cont.)

- **Índice de líneas perezoso** (`pane-core`): indexado bajo demanda al scroll con `Mutex`
  para interior-mutability. Medido en 1 GB: abrir **0.05 ms**, primer viewport **0.16 ms**,
  RSS pico **9.2 MB** (antes ~1 GB). 5 tests verdes. Modo headless `pane --stat <archivo>`.
  - Caveat: `End`/`line_count()` fuerzan full scan → RSS sube a ~tamaño archivo (mitigable con `madvise`).

## Review mode — ⭐ foco actual

Decidido: el **review mode** es foco principal (core del posicionamiento "revisor del flujo con
agentes IA"); el resto de v1 (piece-tree, búsqueda, Tree-sitter) sigue en el backlog.
- **Paso 1 (✅ verificado en vivo):** `pane --review <archivo>` — ventana bloqueante con footer;
  A/Enter aprueba, R/Esc rechaza, Q/cerrar cancela; veredicto por **exit code** (0/1/2). `--json`
  imprime `{"verdict":"..."}`. El scroll funciona en review. El usuario confirmó que los botones van.
- **Paso 2 (✅ verificado en vivo):** `pane --diff old new` y `pane --review --diff old new` — diff
  unificado con `similar 3.1`, color +/- (verde/rojo) vía `set_rich_text` (color por línea).
  Abstracción `Source` (File lazy | Diff) en `pane-app`. El usuario aprobó un diff en vivo (exit 0).
- **Paso 3a (✅ hecho y verificado por el usuario):** changeset multi-archivo + integración de agentes.
  - `pane --review --changeset <manifest.tsv>` — N diffs en una ventana, un veredicto. TSV
    `old<TAB>new<TAB>label` (lado vacío = archivo nuevo/borrado).
  - **`pane --review --git`** — la lógica de git vive DENTRO del binario (`std::process::Command`):
    lee `git status --porcelain -uall`, saca old de `HEAD:<path>` y new del disco. **Eliminado el
    helper `.sh`** — ya no hace falta script suelto. Detecta modificado/nuevo/borrado/renombrado,
    salta directorios. Errores claros: fuera de repo → exit 2, sin cambios → exit 0.
- **Agnóstico al modelo (✅):** `AGENTS.md` en la raíz es ahora la guía canónica; `CLAUDE.md` es solo
  un puntero. `integrations/` tiene `agents-snippet.md` (drop-in para cualquier AGENTS.md/rules) +
  `claude-code/SKILL.md`. La integración es solo un comando CLI + exit codes → sirve a cualquier agente.
- **Instalador (✅):** `./install.sh` — compila e instala `pane` en el PATH (`cargo install`) y copia el
  skill a `~/.claude/skills/pane-review/`, así funciona desde cualquier proyecto sin setup por repo.
- **Tier 1 quick wins (✅ hecho, falta verificación visual):** números de línea (gutter right-aligned,
  ancho medido de `layout_runs().line_w`; en diff numera por archivo nuevo vía `change.new_index()`),
  `clamp_scroll` (no más pantalla vacía al final), y el patrón review-antes-de-escribir documentado.
  Refactor: `DiffLine`→`ViewLine{text,color,num}`; `Source::clamp_scroll(idx,visible)` sustituye a
  `clamp_to_line` en el scroll. 5 tests core verdes, compila.
- **Backlog reorganizado por tiers** (valor/esfuerzo) en `BACKLOG.md`. Decidido: MCP baja a Tier 4
  (la integración CLI+exit-codes ya es agnóstica → cualquier agente usa Pane hoy; MCP no da capacidad nueva).
- **Tier 2 búsqueda (✅ hecho, falta test visual):** `pane-core::search(pat, use_regex, max)` escanea
  todo el archivo (literal `memmem` / regex `regex::bytes`), devuelve índices de línea. 6 tests core.
  UI en `main.rs`: `/` abre input, teclea query, Enter busca, `n`/`N` navegan, resalta la coincidencia
  (color `HL`), barra de estado inferior unificada (search | footer de review). El input de búsqueda
  captura TODO el teclado (las letras no disparan veredicto mientras escribes). Regex está en el motor;
  falta el toggle en la UI (hoy la barra hace literal).
- **Tema Material dark (✅ vía agente en paralelo):** aplicada la paleta de `docs/theme-proposal.md` —
  fondo `#131316` (casi neutro, antes azul-gris), acentos desaturados, gutter subido a ~3.8:1 (antes
  2.86:1, bajo el mínimo de 3:1 → era la causa real del cansancio de los números). Constantes en `main.rs`.
- **Fixes de UX (tras prueba del usuario):** (1) En búsqueda, `Esc` limpia la búsqueda en vez de cerrar
  Pane. (2) Las coincidencias se **centran** al saltar (Enter y `n`/`N`). (3) `Resized` re-clampa el scroll.
- **BUG DE SCROLL (causa raíz, ventana chica no llegaba al fondo):** se usaba el MISMO conteo de líneas
  para dibujar (`visible_lines` = `ceil+1`, con fila parcial abajo) y para el clamp/paginado. El sobre-conteo
  hacía que las últimas 1-2 líneas se dibujaran recortadas bajo el viewport y el clamp no dejara bajar más;
  en ventana chica el sobre-conteo es proporcionalmente grande → no llegabas al final; maximizada casi no se
  nota. **Fix:** nuevo `page_lines()` = `floor(h/lh)` (líneas completas) para clamp/paginado/centrado; `visible_lines`
  (ceil+1) queda solo para cuántas dibujar. Todos los `clamp_scroll` y el paginado usan `page_lines`.
  Pendiente si aún se siente tosco con trackpad: acumular deltas sub-línea del wheel (hoy se truncan a 0).
- **CAUSA RAÍZ REAL del scroll (2ª iteración):** era el **word-wrap**. Las líneas largas (p. ej. un sha512)
  se partían en varias filas visuales, lo que (a) desalineaba los números del gutter y (b) hacía que un
  pantallazo tuviera menos líneas LÓGICAS que `page_lines`, así que el clamp se quedaba corto y no llegabas
  al fondo. **Fix:** `text_buffer.set_wrap(Wrap::None)` (y footer/gutter). Una línea lógica = una fila.
  Efecto: las líneas largas ahora se cortan a la derecha → falta scroll horizontal (backlog).
- **Pedido del usuario:** barra de scroll vertical tipo nativa a la derecha (backlog Tier 2).
- **Tema:** el usuario dice que se ve algo mejor; NO prioritario, se refinará luego.
- **Syntax highlighting (✅ hecho, falta test visual):** módulo `pane-app/src/syntax.rs` con Tree-sitter
  para **JSON, Rust, TOML, Markdown (bloque), Java** (`tree-sitter` 0.26, `tree-sitter-highlight` 0.26,
  gramáticas + `tree-sitter-toml-ng`, `tree-sitter-md`). Paleta One-Dark-ish (KEYWORD/STRING/COMMENT/
  NUMBER/FUNCTION/TYPE/PROPERTY/PUNCT). `HighlightConfiguration::new(Language, name, hl, "", "")` +
  `configure(HIGHLIGHT_NAMES)`; eventos `Source/HighlightStart/End` → rangos coloreados.
  - **Refactor grande:** `ViewLine` pasó de `{text, color}` a `{spans: Vec<(String,Color)>, num}` (multicolor);
    `Source::Diff`→`Source::Lines` (diff, changeset y highlighted comparten Vec<ViewLine>); `layout_text`
    concatena spans + `\n`. Se resalta la búsqueda recoloreando spans.
  - **Gate:** highlight solo si extensión conocida Y `< 4 MB` (`HL_MAX_BYTES`); logs enormes → `Source::File` plano/lazy.
  - Smoke test: los 5 lenguajes renderizan sin panic. 6 tests core verdes.
- **Scroll horizontal + barra de scroll (✅ hecho, falta test visual):**
  - `hscroll: f32` en px físicos; rueda/trackpad eje X + flechas ←/→ (60px·scale por pulso); Home
    resetea. Clamp en `layout_text` contra el ancho real shaped de las líneas visibles (`line_w`).
    El `TextArea` del contenido se desplaza `left: content_left - hscroll` pero sus bounds clipean
    en el borde del gutter (los números nunca se tapan). `text_buffer.set_size(None, …)` para no
    cull-ear glifos panneados (con `Wrap::None` el ancho no afecta el layout).
  - **`QuadRenderer`**: mini pipeline wgpu propio (shader WGSL inline, vértices pos+color NDC,
    `ALPHA_BLENDING`, `vertex_attr_array!`) porque glyphon solo pinta texto. Dibuja track sutil
    (alpha 0.05) + thumb proporcional (0.22) a la derecha; con archivo lazy el thumb usa las líneas
    indexadas hasta ahora (se encoge al descubrir más). Solo indicador, sin drag todavía.
  - Smoke test OK (300 líneas de anchos variados, sin panic). 6 tests core verdes.
- **Tier 3 distribución (✅ infraestructura lista, 2026-07-25):**
  - Versionado `0.1.0` en el workspace (crates lo heredan), `pane --version`, `CHANGELOG.md`.
  - `scripts/build-universal.sh`: build de ambos targets + `lipo` → validado local (fat binary
    arm64+x86_64, 21 MB, tarball+sha256 en `dist/`, gitignored). Target x86_64 instalado.
  - `.github/workflows/release.yml`: push de tag `v*` → check tag==versión → tests → build universal
    → GitHub Release con assets. Runner macos-14.
  - `packaging/homebrew/pane.rb` (fórmula template) + `packaging/homebrew/README.md` (proceso).
  - URLs corregidas al repo real: `github.com/amondrave/pane` (antes placeholder pane-editor).
  - ⚠️ **Pasos manuales del usuario para estrenar**: repo público + `git tag v0.1.0 && git push origin
    v0.1.0` + crear `amondrave/homebrew-tap` con la fórmula (sha del asset .sha256 del release).
- **Siguiente candidato:** pulido de búsqueda (toggle regex + substring highlight + go-to-line),
  scrollbar interactiva (drag), o markdown inline. También pendiente: primer release real (tag).

## Notas de arquitectura (review mode)

- `Source` enum en `main.rs`: `File(TextFile)` (lazy/mmap) o `Diff(Vec<DiffLine>)` (coloreado).
  Unifica el render: ambos pasan por `set_rich_text` con color por línea. El changeset multi-archivo
  también es `Source::Diff` (headers de archivo en color ACCENT + diffs concatenados).
- Diff se computa completo al abrir (los archivos a comparar son código normal, no GB). `similar 3.1`.
- Verdict por exit code se resuelve tras `run_app`, leyendo `app.verdict`.
- Pendiente hardening (backlog): ignorar key events `is_synthetic` en el verdict.

## Notas de implementación (v0.2)

- El render reconstruye el string visible y re-shapea con `set_text` cada frame. Barato para
  decenas de líneas; optimizable con buffers por-línea/caché si hiciera falta.
- Usa `Shaping::Advanced`; para logs ASCII `Shaping::Basic` sería más rápido.
- Render vive en `pane-app` (pragmático); separar a `pane-render` cuando estabilice.
- Índice perezoso con `Mutex<LineIndex>`; `TextFile` sigue siendo `Send+Sync` (necesario para `Arc` en winit).

## Backlog inmediato (v1)

1. **Índice perezoso/muestreado** — bajar RSS de ~1 GB a decenas de MB (indexar bajo
   demanda al scroll + `madvise`). Es la decisión descubierta en el v0 (PRD §9).
2. **Piece-tree editable** — edición real (mmap read-only + append buffer + undo/redo).
3. Búsqueda/regex en streaming. Highlighting Tree-sitter solo del viewport.

## Contexto / gotchas

- `cargo` no está en el PATH de shells no interactivos: `export PATH="$HOME/.cargo/bin:$PATH"`.
- Archivos de prueba grandes: generar en el scratchpad de sesión y **borrarlos al terminar**.
- Render vive en `pane-app` por ahora (pragmático); se separará a `pane-render` cuando estabilice.

## Decisiones abiertas

- Handle real para el bundle id (`dev.<handle>.pane`), hoy provisional `dev.pane.app`.
- Estrategia exacta del índice perezoso (`madvise`, tamaño de muestreo).
