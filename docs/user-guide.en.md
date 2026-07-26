# Pane — User Guide

> Guía en español: [user-guide.es.md](user-guide.es.md)

Pane is a fast, native-feeling viewer/reviewer for macOS. It opens giant files
(logs, JSON, dumps) instantly, and works as an approval gate for changes made by
AI coding agents. It is a viewer/reviewer — editing is on the roadmap, not here yet.

## 1. Installation

### Homebrew (recommended — no Rust required)

```bash
brew install amondrave/tap/pane
```

Universal binary: works on Apple Silicon and Intel Macs. Homebrew downloads skip
the Gatekeeper quarantine, so there is no "unverified developer" dialog.

### From source

Requires a stable Rust toolchain ([rustup](https://rustup.rs)):

```bash
git clone https://github.com/amondrave/pane
cd pane
./install.sh
```

`install.sh` builds the `pane` binary, installs it on your PATH and registers
the AI-agent integration (Claude Code skill). To build without installing:
`cargo build --release` → `target/release/pane`.

### Verify

```bash
pane --version
```

> **Note (manual download):** if you download the release tarball from GitHub in
> a browser instead of using Homebrew, macOS may quarantine the binary. Clear it
> with `xattr -d com.apple.quarantine ./pane`, or right-click → Open.

## 2. Opening files

```bash
pane server.log          # opens a window, instantly — even for multi-GB files
pane data.json
```

- Huge files open via memory-mapping with a **lazy line index**: Pane only reads
  the parts you actually view, so a 1 GB log opens in a fraction of a millisecond
  using a few MB of RAM.
- Files under 4 MB in a supported language get **syntax highlighting**
  (Tree-sitter): JSON, Rust, TOML, Markdown, Java.
- Long lines don't wrap — pan horizontally to read them.
- `pane --stat <file>` prints open metrics headlessly (no window), useful for
  benchmarks.

## 3. Navigation & keys

| Key / gesture | Action |
|---|---|
| Mouse wheel / trackpad | Scroll vertically **and** horizontally |
| `↑` / `↓` | One line up / down |
| `PageUp` / `PageDown` | One page up / down |
| `Home` | Top of file (also resets horizontal pan) |
| `End` | Bottom of file (forces full indexing on huge files) |
| `←` / `→` | Pan horizontally (long lines) |
| `Esc` | Close (viewer mode) / clear search / reject (review mode) |

A scrollbar on the right shows your position. On huge lazy-loaded files it
reflects what has been indexed so far, so it refines as you scroll.

## 4. Search

| Key | Action |
|---|---|
| `/` | Open search, type your query |
| `Enter` | Run the search and jump to the first match |
| `n` / `N` | Next / previous match (centered on screen) |
| `Esc` | Clear the search (does not close the window) |

The status bar shows `current/total` matches. Search is literal substring today;
a regex toggle is on the roadmap (the engine already supports it).

## 5. Review mode — approve or reject changes

Review mode turns Pane into a **blocking approval gate**: the window opens, you
read, you decide, and the verdict is returned as the process **exit code**.

```bash
pane --review file.rs                 # review a single file
pane --review --diff old.rs new.rs    # review a colored diff (old ↔ new)
pane --review --git                   # review ALL uncommitted changes in a git repo
pane --review --changeset list.tsv    # review an explicit set of file pairs
```

Verdict keys (shown in the footer):

| Key | Verdict | Exit code |
|---|---|---|
| `A` or `Enter` | **Approve** | `0` |
| `R` or `Esc` | **Reject** | `1` |
| `Q` or closing the window | **Cancel** | `2` |

- You can scroll and search freely before deciding.
- `--json` additionally prints `{"verdict":"approved"}` to stdout.
- In diffs: green = added, red = deleted, gray = context. Line numbers follow
  the new file (deleted lines have no number).
- `--git` shows every modified, added and deleted file in one scrollable window
  with one verdict. New files appear in full as additions.
- The changeset manifest is one line per file: `old_path<TAB>new_path<TAB>label`
  (empty old = new file; empty new = deleted file).

## 6. Using Pane with AI coding agents

The integration is **model-agnostic**: it's just a CLI command and exit codes,
so it works with Claude Code, Codex, Cursor, Gemini — anything that runs shell
commands. Two patterns:

**Review after editing** — the agent edits your working tree, then runs
`pane --review --git`; on exit `1` it reverts/revises.

**Review before writing** — the agent writes its proposal to a temp file, runs
`pane --review --diff current.rs /tmp/proposed.rs`, and only applies the change
on exit `0`. Nothing lands without your approval.

Setup:
- Any agent: paste [`integrations/agents-snippet.md`](../integrations/agents-snippet.md)
  into your project's agent instruction file (`AGENTS.md`, `.cursor/rules`, …).
- Claude Code: `install.sh` installs the `pane-review` skill globally, so you can
  just say *"review this in Pane"* in any project.

Requires a human at a display — it's an interactive gate, not for headless CI.

## 7. Troubleshooting

- **`pane: command not found`** — ensure `~/.cargo/bin` (source install) or the
  Homebrew prefix is on your PATH. For agents using non-interactive shells, add
  `. "$HOME/.cargo/env"` to your `~/.zshrc`.
- **No window appears over SSH/headless** — Pane needs a display; use `--stat`
  for metrics or run it locally.
- **A file shows no colors** — highlighting applies to supported extensions
  (`.json .rs .toml .md .java`) under 4 MB; bigger files stay plain by design.
- **Reporting issues** — <https://github.com/amondrave/pane/issues>.
