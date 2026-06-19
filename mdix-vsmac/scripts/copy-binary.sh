#!/usr/bin/env bash
# Copies the compiled mdix-lsp binary into bin/{platform}/ so it gets
# bundled alongside the addin DLL when packaging with mdtool.
#
# Usage: scripts/copy-binary.sh [--release]
#   --release   copies from target/release (default: target/debug)

set -euo pipefail

PROFILE="debug"
if [[ "${1:-}" == "--release" ]]; then
  PROFILE="release"
fi

ARCH="$(uname -m)"
case "$ARCH" in
  arm64)  PLATFORM="darwin-arm64" ;;
  x86_64) PLATFORM="darwin-x64" ;;
  *)      echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SRC="$WORKSPACE_ROOT/target/$PROFILE/mdix-lsp"
DEST_DIR="$SCRIPT_DIR/../bin/$PLATFORM"
DEST="$DEST_DIR/mdix-lsp"

if [[ ! -f "$SRC" ]]; then
  echo "Binary not found: $SRC" >&2
  if [[ "$PROFILE" == "release" ]]; then
    echo "Run: cargo build -p mdix-lsp --release" >&2
  else
    echo "Run: cargo build -p mdix-lsp" >&2
  fi
  exit 1
fi

mkdir -p "$DEST_DIR"
cp "$SRC" "$DEST"
chmod +x "$DEST"

echo "Copied: $SRC"
echo "    to: $DEST"
