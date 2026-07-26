# Publishing Pane to Homebrew (tap)

One-time setup, then a small update per release. Users install with:

```bash
brew install amondrave/tap/pane
```

Because Homebrew downloads don't get the quarantine attribute, this avoids the
Gatekeeper "unverified developer" dialog without needing notarization.

## One-time: create the tap

1. Create a **public** GitHub repo named exactly `homebrew-tap` under `amondrave`.
2. In it, create `Formula/pane.rb` with the contents of [`pane.rb`](pane.rb).

## Per release

1. Bump `version` in the workspace `Cargo.toml`, update `CHANGELOG.md`, commit.
2. Tag and push: `git tag v0.1.0 && git push origin v0.1.0`.
   The `release` workflow builds the universal binary and publishes the GitHub
   Release with the tarball and its `.sha256`.
3. Copy the sha from the `.sha256` release asset into `Formula/pane.rb`
   (`version` + `sha256`), commit, push the tap.
4. Verify: `brew update && brew install amondrave/tap/pane && pane --version`.

## Notes

- The prerequisite is only that the `amondrave/pane` repo is **public** (release
  assets must be downloadable anonymously).
- Signing/notarization is NOT needed for this path; it becomes relevant when we
  ship a `.app` bundle (see BACKLOG, Tier 3).
