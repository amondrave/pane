---
name: pane-review
description: Open Pane (a fast native macOS window) to visually review the current code changes as a colored diff, and gate on the human's approve/reject verdict. Use after making a set of edits when the user wants to review changes in a native window instead of reading diffs in the terminal — e.g. "review this in Pane", "let me approve these changes", "open the diff so I can see it".
---

# Pane review — human-in-the-loop gate

Hand the current changes to a human for a fast, native visual review and branch on
their verdict. Requires `pane` on PATH, a git repository, and a display (human present).

## Steps

1. Make sure your edits are saved to disk (the git working tree reflects them).
2. From inside the repository, run:
   ```bash
   pane --review --git
   ```
   Every changed file (modified, added, deleted) is shown as a colored diff in one
   scrollable window. New files appear in full, as all-additions. The command **blocks**
   until the human decides.
3. Read the exit code — it is the verdict:
   - **0 = approved** → keep the changes and continue.
   - **1 = rejected** → do not proceed; revert or revise, then ask what to change.
   - **2 = cancelled** → closed without deciding; ask how they want to proceed.
   - **127** → `pane` is not installed; fall back to a terminal diff.

## Review BEFORE writing (propose → approve → apply)

For a big or risky change, get approval *before* editing the user's file. Write your
proposed version to a temp file and diff it against the current one:

```bash
git show HEAD:src/foo.rs > /tmp/foo.current   # or: cp src/foo.rs /tmp/foo.current
# ...write your proposed version to /tmp/foo.proposed...
pane --review --diff src/foo.rs /tmp/foo.proposed
```

Apply the change only if the exit code is `0`. On `1`/`2`, discard the proposal and ask
the user what to adjust — the edit never lands without approval.

## Other forms

```bash
pane --review --diff old.rs new.rs       # review one specific pair
pane --review --changeset changes.tsv    # explicit set; lines: old<TAB>new<TAB>label
pane <file>                              # just open a (possibly huge) file to read
```

## Notes

- The verdict is binary today (approve / reject / cancel). Structured comments are a
  future addition.
- This is an interactive gate: it needs a human at a display, so it does not apply to
  headless or CI runs.
