# CLAUDE.md — Guía de trabajo de Pane

> Léeme al iniciar sesión en este repo. Resumen operativo; la **fuente de verdad**
> del alcance es [PRD.md](PRD.md), y la visión está en [idea.md](idea.md).
>
> 👉 **Estado vivo entre sesiones + próximos pasos: [HANDOFF.md](HANDOFF.md).**
> Léelo primero al retomar, y actualízalo al terminar cada sesión.

## Qué es Pane

Visor/revisor nativo-rápido para macOS que abre archivos **gigantes** (logs, JSON,
dumps, diffs) al instante, pensado para el flujo de trabajo con **agentes IA**
(el cuello de botella humano es revisar/entender, no escribir). **No es un IDE.**

Reposicionado desde el `idea.md` original ("otro editor ligero", categoría saturada
por CotEditor/Sublime/Zed) hacia "el abridor/revisor instantáneo de archivos grandes".

## Decisiones fijadas (detalle en PRD.md §2)

- **Naturaleza:** herramienta personal + portafolio de sistemas. Optimizar por
  profundidad técnica y uso propio, no por métricas de mercado.
- **Stack UI:** Rust + render propio GPU (`winit` + `wgpu` + `glyphon`/`cosmic-text`).
  "Nativo" = *se siente* nativo y rápido, **no** widgets AppKit.
- **Buffer:** piece-tree sobre mmap read-only + append buffer (NO `ropey`, que carga
  todo en RAM). Es la pieza difícil y el mayor valor de portafolio.
- **Índice de líneas:** **perezoso/muestreado** (decisión descubierta en el v0, ver
  PRD §9): indexar bajo demanda al hacer scroll, no todo al abrir; `madvise` para
  descartar páginas. Motivo: el índice completo hace RSS ≈ tamaño del archivo.
- **v1:** edición básica desde el día uno + búsqueda/regex + highlighting Tree-sitter
  **solo del viewport** + go-to-line. Multi-cursor, diff, log explorer → v2+.
- **Nombre/bundle:** "Pane", bundle id provisional `dev.pane.app`.
- **Tema:** dark único en v1, colores en un `Theme`/tokens centralizado.
- **Config:** sin sistema de config en v1; constantes centralizadas. TOML → v2.
- **Éxito v1:** lo uso a diario + benchmark reproducible ("abre N GB en M ms con K MB").

## Estado actual / bitácora

- **Hecho:** workspace Cargo (`pane-core` + `pane-app`). v0 spike paso 1 (core):
  mmap + índice de líneas + acceso O(1). Compila, 3 tests verdes.
  Medido en 1 GB / 8.1M líneas: abrir+índice 148 ms (caliente), salto O(1) ~0 µs,
  índice 67 MB, **RSS pico ~1 GB** (por índice completo → motiva índice perezoso).
- **Siguiente:** paso 2 del spike — ventana GPU (`winit`/`wgpu`/`glyphon`),
  validar scroll fluido a 60fps. Riesgo: alinear versiones wgpu/glyphon.
- **Backlog inmediato v1:** índice perezoso/muestreado (bajar RSS); piece-tree editable.

## Comandos

⚠️ **`cargo` no está en el PATH de shells no interactivos.** Anteponer:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
```
(Ya se añadió `. "$HOME/.cargo/env"` a `~/.zshrc` para terminales nuevas.)

```bash
cargo build --release
cargo test -p pane-core
cargo run --release -p pane-app -- <archivo>   # v0 CLI: imprime métricas de apertura
```

Generar archivo de prueba (y **borrarlo al terminar** — el usuario lo pide):
```bash
yes "linea de log de ejemplo ..." | head -c 1000000000 > <scratchpad>/big.log
```
Usar siempre el scratchpad de la sesión, nunca el repo. Limpiar los artefactos generados.

## Convenciones

- **Modularidad pragmática, NO "clean architecture" dogmática.** `pane-core` sin deps
  de UI (testeable/benchmarkable headless). Traits solo donde ganan su sitio. Nada de
  capas puertos/adaptadores estilo enterprise — el rendimiento *es* el producto.
- Perfil `release` optimizado (LTO, codegen-units=1) — es el binario que medimos.
- **Código y comentarios en inglés** (decidido: alcance opensource). Los docs de
  proyecto (`PRD.md`, `CLAUDE.md`, `HANDOFF.md`, `idea.md`) siguen en español.
- Benchmarks son ciudadanos de primera clase, no un extra.
- **`BACKLOG.md`** (local, en `.gitignore` — no se comparte) lista los features a
  desarrollar. El scope oficial sigue en `PRD.md`; el backlog es el cuaderno de "qué sigue".

## Cómo trabajamos (preferencias del usuario)

- **Decisiones vía rondas de preguntas estructuradas** antes de construir algo grande,
  luego materializar el acuerdo en docs (PRD/CLAUDE). No asumir en decisiones de rumbo.
- Tono profesional, realista, con ojo crítico. El usuario es dev de sistemas (Rust,
  macOS nativo, optimización de memoria). Se comunica en español.
- **Spikes por fases**: validar barato lo más riesgoso antes de construir encima.
- Limpiar archivos de prueba/benchmark generados después de usarlos.

## Decisiones abiertas

- Handle real para el bundle id (`dev.<handle>.pane`).
- ¿`madvise`/estrategia exacta del índice perezoso?
