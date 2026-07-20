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
- **Paso 3 (siguiente):** envoltura MCP local / skill para agentes (solo humano-presente + display). CLI primero.

## Notas de arquitectura (review mode)

- `Source` enum en `main.rs`: `File(TextFile)` (lazy/mmap) o `Diff(Vec<DiffLine>)` (coloreado).
  Unifica el render: ambos pasan por `set_rich_text` con color por línea.
- Diff se computa completo al abrir (los archivos a comparar son código normal, no GB).
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
