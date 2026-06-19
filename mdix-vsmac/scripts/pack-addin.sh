#!/usr/bin/env bash
# Builds the addin and packages it into a distributable .mpack using the
# Mono.Addins setup tool bundled with the legacy VS for Mac install.
#
# If this chokes on net472/Mono assembly resolution (common with the
# CLI dotnet SDK against a frozen toolchain), open MdixAddin.csproj
# directly inside VS for Mac instead and use Tools -> Package Add-in.
#
# Usage: scripts/pack-addin.sh [--release]

set -euo pipefail

PROFILE="Debug"
CARGO_RELEASE_FLAG=""
COPY_RELEASE_FLAG=""
if [[ "${1:-}" == "--release" ]]; then
  PROFILE="Release"
  CARGO_RELEASE_FLAG="--release"
  COPY_RELEASE_FLAG="--release"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ADDIN_DIR="$SCRIPT_DIR/.."
VS_APP="/Applications/Visual Studio.app"
MDTOOL="$VS_APP/Contents/MacOS/mdtool"

if [[ ! -x "$MDTOOL" ]]; then
  echo "mdtool not found at $MDTOOL" >&2
  echo "Adjust VS_APP in this script if your legacy install lives elsewhere." >&2
  exit 1
fi

echo "==> Building mdix-lsp ($PROFILE)"
( cd "$ADDIN_DIR/.." && cargo build -p mdix-lsp $CARGO_RELEASE_FLAG )

echo "==> Bundling mdix-lsp binary"
"$SCRIPT_DIR/copy-binary.sh" $COPY_RELEASE_FLAG

echo "==> Building MdixAddin ($PROFILE)"
dotnet build "$ADDIN_DIR/MdixAddin.csproj" -c "$PROFILE"

OUT_DLL="$ADDIN_DIR/bin/$PROFILE/net472/MdixLanguageSupport.dll"
if [[ ! -f "$OUT_DLL" ]]; then
  echo "Built DLL not found at $OUT_DLL — check the build output path." >&2
  exit 1
fi

echo "==> Packing .mpack"
"$MDTOOL" setup pack "$OUT_DLL"

echo "Done. Look for MdixLanguageSupport_*.mpack in $ADDIN_DIR"
