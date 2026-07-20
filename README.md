<div align="center">

# Pane

**A fast, native-feeling file viewer/reviewer for macOS — built for the age of AI agents.**

Open giant logs, JSON, dumps and diffs *instantly*, without spinning up an IDE.

`Rust` · `wgpu` · `Tree-sitter` · `MIT`

> ⚠️ **Pre-alpha / work in progress.** The core engine is being validated. Not usable yet.

</div>

---

## Why Pane?

When AI agents generate a lot of code and output, the human bottleneck stops being *writing* and becomes *reviewing and understanding*: a 900 MB log, a huge JSON blob, a diff you need to read right now. VS Code is heavy for that. TextEdit can't do it. Terminal tools work but not everyone wants to live there.

Pane is the tool you reach for to **open a giant file at once and understand it** — fast, native-feeling on macOS, and out of your way.

It is **not** another IDE and not trying to be. See [`idea.md`](idea.md) for the vision and [`PRD.md`](PRD.md) for the concrete v1 scope.

## What makes it different

- **Opens files of several GB without freezing.** Memory-mapped I/O + a virtualized viewport: jumping to line 4,000,000 costs microseconds.
- **Native-feeling and light.** Rust + a custom GPU renderer (`wgpu` + `glyphon`), not Electron, not a WebView.
- **Built for the agent workflow.** Fast open, search, and (soon) diff review of what your agents produce.
- **Offline, no telemetry, no subscriptions.** MIT, forever.

## v0 spike measurements

Early numbers from the core engine on a 1 GB / 8.1M-line file (Apple Silicon, release build):

| | Warm |
|---|---|
| Open + index | **148 ms** |
| Jump to any offset | **~0 µs** (O(1)) |
| Line-index heap | 67 MB |

> These are `v0` spike numbers, not a shipped product. Reducing peak RSS on multi-GB files (via a lazy/sampled index) is the next core task — see [`PRD.md`](PRD.md) §9.

## Status & roadmap

- [x] **v0 spike** — mmap + line index + O(1) viewport access (validated)
- [ ] **v0.2** — GPU window (`winit`/`wgpu`/`glyphon`), fluid 60fps scroll
- [ ] **v1 (MVP)** — lazy index, basic editing, regex search, viewport Tree-sitter highlighting, benchmarks
- [ ] **v2** — multi-cursor, diff viewer, log explorer
- [ ] **v3** — JSON/SQL/Markdown tools, session restore, plugins, optional offline AI

## Install & build

Requires a recent stable Rust toolchain (via [rustup](https://rustup.rs)).

```bash
git clone https://github.com/pane-editor/pane
cd pane
cargo build --release
```

The binary is produced at `target/release/pane`.

## Usage

> **Today (v0):** Pane is a CLI that opens a file, indexes it, and prints how fast
> it did so — the harness that validates the engine. The GPU viewer window lands in v0.2.

Open any file and see the engine's metrics:

```bash
cargo run --release -p pane-app -- /path/to/huge.log
# or, using the built binary directly:
./target/release/pane /path/to/huge.log
```

Example output:

```
─ Pane v0 · métricas de apertura ─────────────────
archivo:            /path/to/huge.log
tamaño:             1000.0 MB (1000000000 bytes)
líneas:             8130082
abrir + índice:     148.0 ms
salto mitad+fin:    0 µs
RSS pico:           1018.8 MB
──────────────────────────────────────────────────
```

Run the tests:

```bash
cargo test -p pane-core
```

**Where it's headed:** `pane <file>` will open a native GPU window you scroll, search
and edit; dragging a file onto the app (or `Pane.app`) will do the same. Those flows
are on the roadmap below — not wired up yet.

## Architecture

Pragmatic Rust workspace — modular where it earns its keep, no enterprise layering:

```
crates/
  pane-core/     # buffer + mmap loading + line index + search (no UI deps, benchmarkable)
  pane-syntax/   # Tree-sitter highlighting, scoped to the visible viewport (planned)
  pane-render/   # wgpu + glyphon viewport renderer (planned)
  pane-app/      # winit event loop, input, wiring
```

## License

[MIT](LICENSE) — free and open source, always.
