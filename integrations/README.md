# Pane integrations — use Pane with any AI coding agent

Pane works as a **native review gate** for AI coding agents: the agent makes changes,
Pane shows them as a colored diff, the human approves or rejects, and the verdict comes
back to the agent as a process **exit code**.

The integration is **model-agnostic by design** — it is just a CLI command and its exit
codes, so it works with Claude, Codex, Cursor, Gemini, or anything else that can run a
shell command.

## The contract

```bash
pane --review --git      # review the repo's working-tree changes; blocks
```

| Exit code | Verdict |
|---|---|
| `0` | approved |
| `1` | rejected |
| `2` | cancelled |
| `127` | `pane` not installed |

Requires a git repository and a display with a human present (it is an interactive
gate — not for headless/CI runs).

## Install

From the repo root:

```bash
./install.sh
```

This builds and installs the `pane` binary and registers the agent instructions.

## Wiring it into your agent

### Any agent (recommended, model-agnostic)

Paste [`agents-snippet.md`](agents-snippet.md) into your project's agent instruction
file — `AGENTS.md`, `CLAUDE.md`, `.cursor/rules/`, `.github/copilot-instructions.md`,
etc. It only documents the command and its exit codes.

### Claude Code (skill)

[`claude-code/SKILL.md`](claude-code/SKILL.md) is a proper skill so the agent invokes
Pane on its own when you ask to "review this in Pane". `install.sh` copies it to
`~/.claude/skills/pane-review/`, making it available in **every** project — no per-repo
setup and no scripts to run by hand.

## Roadmap

- A local **MCP server** (`pane --mcp`) exposing a `review_changeset` tool, so MCP-capable
  agents can call Pane directly instead of shelling out. The CLI contract above is the
  foundation that server will wrap.
