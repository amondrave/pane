#!/usr/bin/env bash
#
# Build a universal macOS binary (Apple Silicon + Intel) and package it for
# release: dist/pane-<version>-macos-universal.tar.gz (+ .sha256).
#
# Used by CI (.github/workflows/release.yml) and runnable locally.

set -euo pipefail
cd "$(dirname "$0")/.."

[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
echo "==> Building pane v$VERSION (aarch64 + x86_64)"

rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null

cargo build --release --target aarch64-apple-darwin -p pane-app
cargo build --release --target x86_64-apple-darwin -p pane-app

mkdir -p dist
lipo -create \
  target/aarch64-apple-darwin/release/pane \
  target/x86_64-apple-darwin/release/pane \
  -output dist/pane
lipo -info dist/pane

TARBALL="pane-$VERSION-macos-universal.tar.gz"
tar -C dist -czf "dist/$TARBALL" pane
(cd dist && shasum -a 256 "$TARBALL" | tee "$TARBALL.sha256")

echo "==> dist/$TARBALL listo"
