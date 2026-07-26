# Changelog

All notable changes to Pane are documented here. Format loosely follows
[Keep a Changelog](https://keepachangelog.com); versions follow SemVer.

## [0.1.0] — 2026-07-25

First tagged release. Pane is a fast, native-feeling viewer/reviewer for macOS,
built for the AI-agent workflow.

### Added
- **Giant-file engine**: memory-mapped open with a lazy line index — a 1 GB file
  opens in ~0.05 ms with ~9 MB peak RSS; O(1) jump to any line.
- **GPU viewer**: `winit` + `wgpu` + `glyphon` window, virtualized viewport,
  no word-wrap (one logical line = one row), line-number gutter, dark theme
  (Material-dark-informed palette).
- **Search**: `/` to type a query, Enter to run, `n`/`N` to jump (matches
  centered), Esc clears. Literal (SIMD memmem) and regex engines in `pane-core`.
- **Syntax highlighting** (Tree-sitter) for JSON, Rust, TOML, Markdown (block)
  and Java, applied to files under 4 MB; huge logs stay plain and lazy.
- **Review mode** — a human-in-the-loop gate for AI coding agents:
  - `pane --review <file>` — approve (A/Enter), reject (R/Esc) or cancel (Q);
    the verdict is the exit code (0/1/2). `--json` prints `{"verdict":"…"}`.
  - `pane --diff old new` / `--review --diff` — unified colored diff.
  - `pane --review --changeset <manifest.tsv>` — multi-file review, one verdict.
  - `pane --review --git` — review the repo's working-tree changes directly.
- **Scrolling**: wheel/trackpad (both axes), arrows, PageUp/Down, Home/End,
  horizontal pan for long lines, and a proportional scrollbar indicator.
- **Agent integration** (model-agnostic, CLI + exit codes): `AGENTS.md` guide,
  drop-in snippet, Claude Code skill, `install.sh`.
- `--stat` headless metrics mode and `--version`.
