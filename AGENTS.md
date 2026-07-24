# AGENTS.md — Guía de trabajo de Pane

> Guía **agnóstica al modelo/agente** para trabajar en este repo (Claude, Codex, Cursor,
> Gemini, o humanos). La **fuente de verdad** del alcance es [PRD.md](PRD.md); la visión
> está en [idea.md](idea.md).
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
- **Stack UI:** Rust + render propio GPU (`winit 0.30` + `wgpu 30` + `glyphon 0.12`).
  "Nativo" = *se siente* nativo y rápido, **no** widgets AppKit.
- **Buffer:** piece-tree sobre mmap read-only + append buffer (NO `ropey`, que carga
  todo en RAM). Pendiente; hoy hay lectura lazy, no edición.
- **Índice de líneas:** **perezoso** (implementado). Indexa bajo demanda al hacer scroll.
  En 1 GB: abrir 0.05 ms, RSS 9.2 MB. Caveat: `End`/`line_count()` fuerzan full scan.
- **Review mode (foco actual):** gate humano-agente. Ver abajo.
- **Nombre/bundle:** "Pane", bundle id provisional `dev.pane.app`.
- **Tema:** dark único, colores centralizados como constantes en `main.rs`.
- **Config:** sin sistema de config en v1; constantes centralizadas. TOML → v2.
- **Éxito v1:** lo uso a diario + benchmark reproducible.

## Estado actual

- `pane-core`: mmap + índice de líneas **perezoso**, acceso O(1). 5 tests verdes.
- `pane-app`: ventana GPU con viewport virtualizado + **review mode** completo.
- **Review mode** (ver README): `--review`, `--diff`, `--changeset`, `--git`.
- **Siguiente:** servidor MCP local (`pane --mcp`) para agentes no-Claude.
- Backlog completo (local, gitignored): `BACKLOG.md`.

## Comandos

⚠️ **`cargo` no está en el PATH de shells no interactivos.** Anteponer:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

```bash
cargo build --release
cargo test -p pane-core
cargo run --release -p pane-app -- <archivo>          # visor
cargo run --release -p pane-app -- --stat <archivo>   # métricas headless (sin ventana)
./install.sh                                          # instala `pane` + integración de agentes
```

Generar archivo de prueba grande (y **borrarlo al terminar** — el usuario lo pide):
```bash
yes "linea de log ..." | head -c 1000000000 > <scratchpad>/big.log
```
Usar siempre el scratchpad de la sesión, nunca el repo. Limpiar lo generado.

## Convenciones

- **Modularidad pragmática, NO "clean architecture" dogmática.** `pane-core` sin deps
  de UI (testeable/benchmarkable headless). Traits solo donde ganan su sitio. Nada de
  capas puertos/adaptadores estilo enterprise — el rendimiento *es* el producto.
- Perfil `release` optimizado (LTO, codegen-units=1) — es el binario que medimos.
- **Código y comentarios en inglés** (alcance opensource). Los docs de proyecto
  (`PRD.md`, `AGENTS.md`, `HANDOFF.md`, `idea.md`) siguen en español.
- Benchmarks son ciudadanos de primera clase, no un extra.
- **`BACKLOG.md`** (local, en `.gitignore`) lista los features a desarrollar.

## Cómo trabajamos (preferencias del usuario)

- **Decisiones vía rondas de preguntas estructuradas** antes de construir algo grande,
  luego materializar el acuerdo en docs. No asumir en decisiones de rumbo.
- Tono profesional, realista, con ojo crítico. El usuario es dev de sistemas (Rust,
  macOS nativo, optimización de memoria). Se comunica en español.
- **Spikes por fases**: validar barato lo más riesgoso antes de construir encima.
- Limpiar archivos de prueba/benchmark generados después de usarlos.

## Verificación

Lo que se puede validar sin display: `cargo test`, `--stat`, `--review --git` fuera de
un repo (exit 2) o sin cambios (exit 0). **La ventana GPU y las teclas de veredicto
requieren que el usuario pruebe en vivo** — pídeselo, no lo des por hecho.
