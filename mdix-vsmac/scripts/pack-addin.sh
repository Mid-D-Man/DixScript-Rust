#!/usr/bin/env bash
# Builds the addin and packages it into a distributable .mpack using
# VS 2022 for Mac's setup tool.
#
# ROOT CAUSE FIX (#3 of the errors you hit): the old script shelled out to
# `mdtool`. That's the packaging tool from the pre-2022 (Xamarin Studio /
# MonoDevelop-lineage) generation of the IDE. Visual Studio 2022 for Mac
# renamed it to `vstool` — same job (`vstool setup pack <dll> -d:<outdir>`),
# same location in the app bundle, different binary name. Source:
# christianhelle.com/2023/03/extending-vsmac.html (the current, VS2022-era
# walkthrough for building third-party VS4Mac addins).
#
# If this chokes on assembly resolution, open MdixAddin.csproj directly
# inside VS for Mac instead and use Tools -> Package Add-in — vstool and
# the IDE share the same underlying packer, so that GUI path hits the same
# code either way.
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
VSTOOL="$VS_APP/Contents/MacOS/vstool"

if [[ ! -x "$VSTOOL" ]]; then
  echo "vstool not found at $VSTOOL" >&2
  echo "Adjust VS_APP in this script if your legacy install lives elsewhere." >&2
  echo "(Older installs may only have 'mdtool' at that same path — if so," >&2
  echo "set VSTOOL=\"\$VS_APP/Contents/MacOS/mdtool\" and it should behave" >&2
  echo "the same way for packing purposes.)" >&2
  exit 1
fi

echo "==> Building mdix-lsp ($PROFILE)"
( cd "$ADDIN_DIR/.." && cargo build -p mdix-lsp $CARGO_RELEASE_FLAG )

echo "==> Bundling mdix-lsp binary"
"$SCRIPT_DIR/copy-binary.sh" $COPY_RELEASE_FLAG

echo "==> Building MdixAddin ($PROFILE)"
dotnet build "$ADDIN_DIR/MdixAddin.csproj" -c "$PROFILE"

# net7.0, not net472 -- the SDK-style csproj (Microsoft.VisualStudioMac.Sdk)
# outputs under a TargetFramework-named folder, and the addin now targets
# net7.0 to match VS 2022 for Mac's own .NET 6/7 host process.
OUT_DLL="$ADDIN_DIR/bin/$PROFILE/net7.0/MdixLanguageSupport.dll"
if [[ ! -f "$OUT_DLL" ]]; then
  echo "Built DLL not found at $OUT_DLL — check the build output path with:" >&2
  echo "  find \"$ADDIN_DIR/bin\" -name 'MdixLanguageSupport.dll'" >&2
  exit 1
fi

OUT_DIR="$ADDIN_DIR/dist"
mkdir -p "$OUT_DIR"

echo "==> Packing .mpack"
"$VSTOOL" setup pack "$OUT_DLL" -d:"$OUT_DIR"

echo "Done. Look for MdixLanguageSupport_*.mpack in $OUT_DIR"
