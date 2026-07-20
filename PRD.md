# Pane — PRD y Scope del MVP

> Documento vivo. Define qué construimos en la v1 y, sobre todo, qué **no**.
> Estado: borrador acordado (ronda de decisiones 1 y 2).

---

## 1. Reposicionamiento en una frase

**Pane es un visor/revisor nativo-rápido para macOS que abre archivos gigantes (logs, JSON, dumps, diffs) al instante y sin congelarse, pensado para el flujo de trabajo con agentes IA — donde el cuello de botella humano ya no es escribir código, sino revisar y entender lo que se genera.**

No es "otro editor ligero" (categoría saturada: CotEditor, Sublime, Zed). Es la herramienta a la que recurres cuando un agente escupió un log de 800 MB o un JSON de 200 MB y necesitas entenderlo **ya**.

---

## 2. Decisiones fijadas

| Eje | Decisión | Implicación |
|---|---|---|
| **Naturaleza** | Herramienta personal + portafolio de sistemas | Optimizamos por profundidad técnica y por uso propio diario, no por métricas de mercado |
| **Posición** | Visor/revisor del flujo con agentes IA | Núcleo = velocidad + archivos grandes. Features de "IDE" son ruido |
| **Stack UI** | Rust + render propio GPU | "Nativo" = *se siente* nativo y rápido en macOS, **no** widgets AppKit |
| **Render** | `wgpu` + `glyphon`/`cosmic-text` | Reconstruimos scroll/selección/cursor; máximo control y valor de portafolio |
| **Buffer** | Piece-tree sobre mmap read-only + append buffer | Editar GB con poca RAM sin cargar todo. `ropey` queda como fallback de arranque |
| **Modo v1** | Edición desde el día uno | Insert/delete/save reales, no solo lectura |
| **Highlighting** | Tree-sitter, **solo del viewport visible** | Nunca parsear el GB entero |
| **Éxito v1** | Lo uso yo a diario + benchmark demostrable | "Abre X GB en Y ms con Z RAM" como número de portafolio |

---

## 3. Usuario y casos de uso

**Usuario primario:** yo (autor) — dev en macOS que trabaja con agentes IA y toca archivos grandes a diario.
**Usuario secundario:** dev/DevOps/SRE que quiere abrir logs/JSON/dumps sin arrancar un IDE.

**Casos de uso núcleo (v1 debe clavarlos):**
1. Arrastro un log de varios GB → se abre al instante, hago scroll fluido y busco (regex) sin que se congele.
2. Abro un JSON grande generado por un agente → lo veo con highlighting, salto a una línea, edito un valor, guardo.
3. Reviso un archivo modificado por un agente → lo abro rápido, lo leo, ajusto a mano, guardo.

**Fuera de los casos de uso v1:** proyectos multi-archivo, LSP, git, terminal integrada, debugging.

---

## 4. Alcance del MVP (explícito)

### Dentro (v1)
- [ ] Ventana macOS con `winit` (arranque instantáneo, drag & drop de archivo).
- [ ] **Motor de archivos gigantes**: piece-tree sobre mmap, apertura sin cargar todo en RAM.
- [ ] **Render GPU virtualizado**: solo se dibujan las líneas visibles; scroll y salto O(1) a cualquier offset.
- [ ] **Edición básica**: insertar, borrar, deshacer/rehacer, guardar (incluso en archivos grandes).
- [ ] **Búsqueda + regex** en streaming sobre el archivo completo, con primeros resultados en <200 ms.
- [ ] **Syntax highlighting (Tree-sitter)** limitado al viewport visible.
- [ ] **Go-to-line** y navegación básica (incluido por ser barato y crítico en archivos grandes).
- [ ] Detección de encoding básica (UTF-8, con fallback) y line endings (LF/CRLF).

### Fuera (v2+, anotado para no olvidar)
- Multi-cursor → v2
- Diff viewer (alineado con la posición, pero no es el núcleo) → v2
- Log explorer (highlight WARN/ERROR, colapsar líneas, buscar timestamps) → v2
- Find-in-folder / multi-archivo → v2/v3
- Markdown preview, JSON tools, SQL tools → v3
- Session restore → v3
- Sistema de plugins → v3+
- IA (explicar código/logs/stack traces) → v3+, siempre opcional y offline-first
- Firmado/notarización y release público pulido → post-v1

---

## 5. Requisitos no funcionales (los que definen "éxito")

Objetivos de rendimiento a **medir con benchmarks reproducibles** (parte del entregable de portafolio):

| Métrica | Objetivo v1 |
|---|---|
| Arranque en frío → ventana editable | < 100 ms |
| Abrir archivo de 1 GB → primer render interactivo | < 500 ms |
| Abrir archivo de 10 GB | Sin OOM; overhead RAM < ~200 MB sobre el mapeo |
| Scroll / salto a cualquier offset | < 16 ms (60 fps) |
| Búsqueda literal en 1 GB | Primeros resultados < 200 ms (streaming) |
| Footprint RAM en reposo (archivo mediano) | < 50 MB |

> Estos números son la carta de presentación del proyecto. El benchmark harness (comparar contra Sublime/VS Code abriendo los mismos archivos) es un artefacto de primera clase, no un extra.

---

## 6. Arquitectura técnica

### Principio: modularidad pragmática, no "clean architecture" dogmática
El `core` no conoce la UI (permite tests y benchmarks *headless*), pero **no** metemos capas de puertos/adaptadores estilo enterprise. Traits solo donde ganan su sitio.

### Módulos (crates del workspace)
```
pane/
  crates/
    pane-core/     # buffer (piece-tree), carga mmap, búsqueda. Sin deps de UI. Testeable/benchmarkable.
    pane-syntax/   # integración Tree-sitter, highlighting incremental scoped al viewport
    pane-render/   # wgpu + glyphon: virtualización del viewport, glifos, cursor, selección
    pane-app/      # winit event loop, input, estado, wiring
  benches/         # criterion: apertura, scroll, búsqueda vs competencia
```

### Decisión clave — el buffer de texto
- **Piece-tree** (piece table en árbol balanceado) sobre el **archivo original mapeado (`memmap2`) en solo-lectura** + un **append buffer** en memoria para lo insertado.
- **Por qué, y no `ropey`:** `ropey` carga el contenido completo en memoria (un rope de un GB = >1 GB de RAM), lo que rompe "poca RAM" y "abrir 10 GB". El piece-tree deja el original en el mmap (paginado por el SO) y solo mantiene en RAM los descriptores de piezas + lo editado. Es exactamente el motivo por el que VS Code migró a un piece-tree.
- **Fallback de arranque:** si el piece-tree se vuelve un bloqueo temprano, empezar con `ropey` para archivos < N MB y meter el piece-tree como el hito técnico central. Pero la meta es el piece-tree.

### Stack de crates (candidatos)
| Área | Crate |
|---|---|
| Ventana / eventos | `winit` |
| Render GPU | `wgpu` |
| Texto (shaping + layout + render) | `glyphon` (sobre `cosmic-text`) |
| mmap | `memmap2` |
| Syntax | `tree-sitter` + `tree-sitter-highlight` + gramáticas |
| Regex / búsqueda | `regex` (+ `regex-automata`/`memchr` para streaming) |
| Benchmarks | `criterion` |

### Nota "sentirse nativo"
v1: NSWindow vía `winit`, semáforos (traffic lights), atajos macOS estándar (⌘O, ⌘S, ⌘F, ⌘G), scroll con inercia. Vibrancy/materiales y menús nativos finos → v2. No perseguir paridad con AppKit; perseguir que *se sienta* bien.

---

## 7. Riesgos y decisiones abiertas

| Riesgo / pregunta | Nota |
|---|---|
| Editar sobre mmap + piece-tree con undo/redo es la parte difícil | Es también el mayor valor de portafolio. Aislarlo y testearlo a fondo en `pane-core` |
| `glyphon`/`cosmic-text` madurez para selección/cursor precisos | Prototipar temprano: render de 100k líneas con cursor y selección antes de comprometerse |
| Tree-sitter sobre viewport requiere estado incremental correcto | Empezar por highlighting "best effort" del rango visible; refinar incrementalidad después |
| Encoding no-UTF8 en logs reales | v1: UTF-8 + fallback lossy; encodings raros → v2 |
| Guardar un archivo de GB editado (reescritura segura) | Escritura atómica (temp + rename); considerar guardado por streaming del piece-tree |

**Decisiones cerradas (ronda 3):**
- **Nombre / identidad:** se conserva **Pane**. Bundle id propio, estilo `dev.<handle>.pane` (definir handle al montar el bundle).
- **Tema:** **dark único** en v1, pero los colores viven en un `Theme`/tokens centralizado (no hardcodeados por el render). Añadir light en v2 es trivial.
- **Config:** **sin sistema de config** en v1. Valores (fuente, tamaño, tema, keybinds) en un módulo de constantes centralizado (`pane-app/config.rs` o similar). Se expone como TOML en v2.

---

## 8. Roadmap por fases

- **v0 (spike técnico):** abrir un archivo de 1 GB en mmap y renderizar el viewport con scroll fluido. Nada más. Valida el corazón.
- **v1 (MVP, este PRD):** motor de archivos gigantes + edición básica + búsqueda/regex + highlighting de viewport + benchmarks. Criterio: lo uso a diario.
- **v2:** multi-cursor, diff viewer, log explorer.
- **v3:** JSON/SQL/markdown tools, session restore, plugins, IA opcional offline-first.

---

## 9. Hallazgos del v0 spike (paso 1: core)

Medido sobre un archivo de 1.000 MB / 8.130.082 líneas en Apple Silicon (M-series), release build:

| Métrica | Frío | Caliente |
|---|---|---|
| Abrir + índice completo | 2244 ms | 148 ms |
| Viewport (60 líneas) | 5 µs | 1 µs |
| Salto a mitad/fin (acceso O(1)) | 3 µs | 0 µs |
| Índice en heap | 67 MB | 67 MB |
| RSS pico | 1019 MB | 1069 MB |

**Validado:** acceso O(1) a cualquier offset (habilita scroll fluido); heap del índice minúsculo.

**Decisión descubierta — índice perezoso obligatorio.** El índice *completo* al abrir lee todos los bytes → RSS ≈ tamaño del archivo, lo que rompe "abrir 10 GB con poca RAM". **v1 usará un índice de líneas perezoso/muestreado**: indexar bajo demanda al hacer scroll (no todo al abrir) y `madvise` para que el SO descarte páginas no visitadas. El acceso O(1) ya probado se mantiene; solo cambia *cuándo* se construye el índice. Esto también arregla el arranque (no escanear 1 GB en el hilo de apertura).

---

## 10. Métrica de éxito de la v1

> **Reemplaza mi herramienta actual para abrir logs/JSON grandes, y tengo el número: "Pane abre un archivo de N GB en M ms usando K MB de RAM", medido con un benchmark reproducible que lo compara contra Sublime y VS Code.**

Si eso se cumple, la v1 está terminada — aunque le falten features de la lista de v2+.
