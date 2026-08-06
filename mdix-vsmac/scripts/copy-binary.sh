#!/usr/bin/env bash
# Copies the compiled mdix-lsp AND mdix (CLI) binaries into bin/{platform}/
# so they get bundled alongside the addin DLL when packaging with vstool.
#
# Usage: scripts/copy-binary.sh [--release]
#   --release   copies from target/release (default: target/debug)
#
# 2026-08-06 — now also copies mdix (the mdix-cli crate's binary — see
# mdix-cli/Cargo.toml's [[bin]] name = "mdix"), same as the mdix-vscode
# extension's copy-binary.js. Not just for parity: mdix-lsp's own CLI
# resolution (which_mdix() in mdix-lsp/src/features/commands.rs) checks
# PATH first, then falls back to std::env::current_exe().with_file_name
# ("mdix") — i.e. "look next to whatever binary is currently running".
# Since ActivateAsync() launches the bundled bin/{platform}/mdix-lsp copy
# (via ResolveServerPath() in MdixLanguageClient.cs), putting mdix in that
# same directory means the fallback finds it automatically once it lands
# at lsp/mdix in the build output (see the matching csproj + AddinInfo.cs
# changes) — no PATH setup needed inside VS4Mac either.

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
DEST_DIR="$SCRIPT_DIR/../bin/$PLATFORM"
mkdir -p "$DEST_DIR"

# name:crate pairs -- crate is only used to print the right build command
# on failure, since mdix-cli's binary output name ("mdix") differs from its
# crate name ("mdix-cli").
BINARIES=("mdix-lsp:mdix-lsp" "mdix:mdix-cli")

HAD_ERROR=0
for entry in "${BINARIES[@]}"; do
  NAME="${entry%%:*}"
  CRATE="${entry##*:}"
  SRC="$WORKSPACE_ROOT/target/$PROFILE/$NAME"
  DEST="$DEST_DIR/$NAME"

  if [[ ! -f "$SRC" ]]; then
    echo "Binary not found: $SRC" >&2
    if [[ "$PROFILE" == "release" ]]; then
      echo "Run: cargo build -p $CRATE --release" >&2
    else
      echo "Run: cargo build -p $CRATE" >&2
    fi
    HAD_ERROR=1
    continue
  fi

  cp "$SRC" "$DEST"
  chmod +x "$DEST"
  echo "Copied: $SRC"
  echo "    to: $DEST"
done

exit $HAD_ERROR
