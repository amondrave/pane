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

## Measurements

Opening a 1 GB / 8.1M-line file (Apple Silicon, release build) with the lazy line index:

| | 1 GB file |
|---|---|
| Open (mmap, lazy) | **0.05 ms** |
| First screenful | **0.16 ms** |
| Peak RSS | **9 MB** |
| Jump to any line | O(1) |

> Pane only scans the lines you actually view, so opening a 1 GB file touches
> ~9 MB of RAM instead of ~1 GB. Jumping to the very end forces a full scan
> (as any editor must). These are early engine numbers, not a shipped product.

## Status & roadmap

- [x] **v0 spike** — mmap + line index + O(1) viewport access (validated)
- [x] **v0.2** — GPU window (`winit`/`wgpu`/`glyphon`), fluid scroll, lazy line index (~9 MB RSS on a 1 GB file)
- [ ] **v1 (MVP)** — basic editing, regex search, viewport Tree-sitter highlighting, benchmarks
- [ ] **v2** — multi-cursor, diff viewer, log explorer
- [ ] **v3** — JSON/SQL/Markdown tools, session restore, plugins, optional offline AI

## Install & build

Requires a recent stable Rust toolchain (via [rustup](https://rustup.rs)).

```bash
git clone https://github.com/pane-editor/pane
cd pane
./install.sh          # builds, installs `pane` on your PATH, registers the agent integration
```

Or just build it: `cargo build --release` → `target/release/pane`.

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

### Review mode (for AI coding agents)

Open a file — e.g. code an agent just generated — for a **blocking review**. Approve or
reject, and the window closes returning the verdict as the process **exit code**. That
makes Pane a human-in-the-loop gate an agent can spawn and branch on:

```bash
pane --review path/to/generated.rs     # A / Enter = approve · R / Esc = reject · Q = cancel
echo $?                                 # 0 approved · 1 rejected · 2 cancelled

pane --review --json path/to/file.rs   # also prints {"verdict":"approved"} to stdout
```

Review a change as a unified colored diff (green additions, red deletions):

```bash
pane --diff old.rs new.rs               # view the diff
pane --review --diff old.rs new.rs      # review it, verdict → exit code
```

Review **everything an agent just changed** in one window with a single verdict — this is
the command agents use:

```bash
pane --review --git       # every modified/added/deleted file in the git working tree
```

New files are shown in full (as all-additions), not just a fragment. You can scroll
(wheel / arrows / page / home / end) through the whole changeset before deciding. For an
explicit set: `pane --review --changeset changes.tsv` (lines of `old<TAB>new<TAB>label`).

**Search** — press `/` to search, `n` / `N` to jump between matches. Works in any mode
(file, diff, changeset), even while reviewing.

**Syntax highlighting** — Tree-sitter colors for JSON, Rust, TOML, Markdown and Java,
applied to files under 4 MB (huge logs stay plain and load lazily).

**Agent integration is model-agnostic** — it is just a CLI command and its exit codes, so
it works with Claude, Codex, Cursor, Gemini or anything that can run a shell command. See
[`integrations/`](integrations/): a drop-in snippet for any `AGENTS.md`, plus a Claude Code
skill. A local MCP server (`pane --mcp`) is on the roadmap.

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
