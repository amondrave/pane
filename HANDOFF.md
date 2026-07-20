# HANDOFF — Pane

> Estado vivo entre sesiones: qué está hecho, qué sigue, y contexto para retomar sin
> releer todo. Actualizar al final de cada sesión de trabajo. Fuente de verdad del
> alcance: [PRD.md](PRD.md). Guía operativa: [CLAUDE.md](CLAUDE.md).

_Última actualización: 2026-07-20._

## En una línea

Pane = visor/revisor nativo de archivos gigantes para macOS (Rust + GPU), para el flujo
con agentes IA. Fase actual: **v0.2 — ventana GPU (compila y arranca; falta verificación visual)**.

## Hecho

- **Workspace Cargo**: `pane-core` (motor) + `pane-app` (binario `pane`).
- **v0 spike paso 1 — core** (validado): mmap + índice de líneas + acceso O(1).
  - Medido en 1 GB / 8.1M líneas: abrir+índice **148 ms** caliente, salto O(1) **~0 µs**,
    índice heap **67 MB**, **RSS pico ~1 GB** (por índice completo).
  - 3 tests verdes en `pane-core`. Comentarios/código en **inglés**.
- **v0.2 paso 2 — ventana GPU** (compila + smoke test OK): `winit 0.30` + `wgpu 30` +
  `glyphon 0.12` con **viewport virtualizado** (solo se pasan a glyphon las líneas visibles).
  Scroll con rueda y flechas/PageUp-Down/Home/End. Tema dark, fuente monospace.
  - Versiones alineadas: glyphon 0.12 → wgpu **30.0.0** + cosmic-text **0.19**.
  - Smoke test: arranca sin panic (adapter/device/surface/glyphon OK) y corre el event loop.
- **Docs**: `idea.md` (visión), `PRD.md` (scope + hallazgos), `README.md` (público, con Usage),
  `CLAUDE.md` (guía), `LICENSE` (MIT, Angel Mondragon).

## Siguiente paso

- ⚠️ **Verificación visual pendiente (usuario)**: correr `cargo run --release -p pane-app -- <archivo>`
  en el Mac y confirmar que el texto se ve bien y el scroll es fluido a 60fps sobre un archivo grande.
  El entorno de Claude no tiene display, así que esto solo lo puedes validar tú.
- Luego, **índice perezoso** (backlog #1) para bajar el RSS.

## Notas de implementación (v0.2)

- El render reconstruye el string visible y re-shapea con `set_text` cada frame. Barato para
  decenas de líneas; optimizable con buffers por-línea/caché si hiciera falta.
- Usa `Shaping::Advanced`; para logs ASCII `Shaping::Basic` sería más rápido.
- Render vive en `pane-app` (pragmático); separar a `pane-render` cuando estabilice.

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
