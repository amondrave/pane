#!/usr/bin/env bash
#
# Install Pane on macOS and register the agent integration, so you can run
# `pane --review --git` from any project — no scripts, no per-repo setup.

set -euo pipefail
cd "$(dirname "$0")"

# cargo is often missing from non-interactive shells.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
command -v cargo >/dev/null 2>&1 || {
  echo "error: cargo not found. Install Rust from https://rustup.rs" >&2
  exit 1
}

echo "==> Building and installing the 'pane' binary"
cargo install --path crates/pane-app --force

BIN="$(command -v pane || echo "$HOME/.cargo/bin/pane")"
echo "==> Installed: $BIN"

# Warn if cargo's bin dir isn't on PATH (agents spawn non-interactive shells).
case ":$PATH:" in
  *":$HOME/.cargo/bin:"*) ;;
  *)
    echo
    echo "!! $HOME/.cargo/bin is not on your PATH."
    echo "   Add this to your ~/.zshrc so agents can find 'pane':"
    echo '     . "$HOME/.cargo/env"'
    ;;
esac

# Claude Code skill — available in every project once installed here.
SKILL_DIR="$HOME/.claude/skills/pane-review"
if [ -f integrations/claude-code/SKILL.md ]; then
  mkdir -p "$SKILL_DIR"
  cp integrations/claude-code/SKILL.md "$SKILL_DIR/SKILL.md"
  echo "==> Claude Code skill installed: $SKILL_DIR/SKILL.md"
fi

cat <<'EOF'

==> Done.

Try it: cd into any git repo with uncommitted changes and run

    pane --review --git

  A / Enter = approve (exit 0)   R / Esc = reject (exit 1)   Q = cancel (exit 2)

For other agents (Codex, Cursor, Gemini, ...), paste integrations/agents-snippet.md
into that project's agent instruction file. The integration is just a CLI command
and its exit codes, so it is model-agnostic.
EOF
