<!--
  Drop this into your project's AGENTS.md (or CLAUDE.md / .cursor/rules /
  copilot-instructions.md — any agent instruction file). It is model-agnostic:
  it only describes a CLI command and its exit codes.
-->

## Visual review with Pane

When the user wants to review your changes visually — or says "review this in Pane",
"let me approve these changes", "show me the diff" — hand the changes to a human in a
fast native window instead of printing diffs to the terminal.

Run this inside the repo you just modified:

```bash
pane --review --git
```

It opens every changed file (modified, added, deleted) as a colored diff in one
scrollable window and **blocks** until the human decides. The exit code is the verdict:

| Exit code | Meaning | What you should do |
|---|---|---|
| `0` | approved | Keep the changes and continue. |
| `1` | rejected | Do not proceed. Revert or revise, then ask what to change. |
| `2` | cancelled | The human closed without deciding. Ask how to proceed. |
| `127` | `pane` not installed | Fall back to a terminal diff. |

### Review BEFORE writing (propose → approve → apply)

For a big or risky change, let the human approve it *before* you touch their file. Write
your proposed version to a temp file and diff it against the current one:

```bash
cp src/foo.rs /tmp/foo.current            # or: git show HEAD:src/foo.rs > /tmp/foo.current
# ...write your proposed version to /tmp/foo.proposed...
pane --review --diff src/foo.rs /tmp/foo.proposed
```

Only if the exit code is `0` (approved) do you apply the change to the real file.
On `1`/`2`, discard the proposal and ask what to adjust. This turns Pane into an
approval gate that runs *before* the edit lands, not just after.

### Other forms

```bash
pane --review --diff old.rs new.rs       # review one specific pair
pane --review --changeset changes.tsv    # explicit set; lines: old<TAB>new<TAB>label
pane <file>                              # just open a (possibly huge) file to read
```

Requirements: `pane` on PATH, a git repository, and a display with a human present
(this is an interactive gate — it does not work in headless/CI runs).
