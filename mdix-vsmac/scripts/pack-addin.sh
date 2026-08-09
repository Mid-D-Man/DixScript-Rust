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
# UPDATE after your CI runs kept failing with "Could not resolve addin
# reference 'MonoDevelop.Core'/'MonoDevelop.Ide'": that error is NOT
# reproducible with a correct local setup like yours. It happens on GitHub's
# hosted macOS runners because GitHub quietly dropped Visual Studio for Mac
# from the hosted images after Microsoft retired the product (confirmed via
# actions/runner-images discussion #8212 — their own answer as of April 2025
# was "migrate away from it", not "here's how to keep using it"). Without
# the real app on disk, there's no MonoDevelop.Core.dll/MonoDevelop.Ide.dll
# to resolve against, full stop -- no csproj or AddinInfo.cs fix changes
# that. This script, run locally on your actual VS4Mac machine, doesn't hit
# that problem. See the CI workflow file for the self-hosted-runner path if
# you want this in CI too.
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
# Using `vstool build`, not `dotnet build`. This is the actual root cause of
# "Could not resolve addin reference 'MonoDevelop.Core'/'MonoDevelop.Ide'" --
# it's not (only) a namespace-qualifier bug in AddinInfo.cs (I fixed a real
# one of those too, worth keeping), it's that `dotnet build` alone doesn't
# reliably resolve addin references against your installed IDE the way
# `vstool build` does. Straight from Microsoft MVP Christian Helle's own
# VS4Mac CI guide (christianhelle.com/2023/03/build-vsmac-extensions-using-
# github-actions.html): "dotnet build -c Release ... works fine if you have
# previously built a Visual Studio for mac extension on the machine you're
# working on, but if you are building it in a new machine that has
# previously never built a Visual Studio for Mac extension then you most
# likely will need to run the Visual Studio Tool Runner". Since your Mac is
# the one you use VS4Mac on daily, plain `dotnet build` might genuinely work
# for you -- but `vstool build` is the documented reliable path, so that's
# what this script uses.
"$VS_APP/Contents/MacOS/vstool" build --configuration:"$PROFILE" "$ADDIN_DIR/MdixAddin.csproj"

# net7.0, not net472 -- the SDK-style csproj (Microsoft.VisualStudioMac.Sdk)
# outputs under a TargetFramework-named folder, and the addin now targets
# net7.0 to match VS 2022 for Mac's own .NET 6/7 host process.
OUT_DLL="$ADDIN_DIR/bin/$PROFILE/net7.0/MdixLanguageSupport.dll"
if [[ ! -f "$OUT_DLL" ]]; then
  echo "Built DLL not found at $OUT_DLL — check the build output path with:" >&2
  echo "  find \"$ADDIN_DIR/bin\" -name 'MdixLanguageSupport.dll'" >&2
  exit 1
fi

# 2026-08-09 — added after a FATAL ERROR from `vstool setup pack` itself
# (System.IO.DirectoryNotFoundException on bin/$PROFILE/net7.0/lsp/mdix-lsp)
# produced NO .mpack at all, silently — the DLL check above passed (the C#
# build genuinely succeeded), so nothing before this point would have caught
# it. That particular run turned out to predate the Update=/Include= csproj
# fix elsewhere in this project (Update= on bin\... items is a no-op MSBuild
# silently accepts — they're excluded from default globbing, so there was
# nothing to attach CopyToOutputDirectory metadata to), but the failure mode
# itself — vstool crashing on a file ImportAddinFile declared that isn't
# actually sitting in the build output — is real regardless of root cause,
# and worth failing on clearly here rather than via vstool's own opaque
# stack trace, however it happens next time.
OUT_LSP="$ADDIN_DIR/bin/$PROFILE/net7.0/lsp/mdix-lsp"
if [[ ! -f "$OUT_LSP" ]]; then
  echo "mdix-lsp binary not found in build output at $OUT_LSP" >&2
  echo "AddinInfo.cs declares this via [assembly: ImportAddinFile(\"lsp/mdix-lsp\")] —" >&2
  echo "vstool setup pack will FATAL ERROR on this file missing, not just skip it." >&2
  echo "copy-binary.sh ran above and didn't fail, so this means the csproj isn't" >&2
  echo "actually copying the platform binary (bin/darwin-x64/mdix-lsp or" >&2
  echo "bin/darwin-arm64/mdix-lsp, whichever matches this Mac) into the build" >&2
  echo "output — check MdixAddin.csproj uses Include=, not Update=, on that None" >&2
  echo "item (Update= is a silent no-op for anything under bin\\/obj\\)." >&2
  exit 1
fi

OUT_DIR="$ADDIN_DIR/dist"
mkdir -p "$OUT_DIR"

echo "==> Packing .mpack"
"$VSTOOL" setup pack "$OUT_DLL" -d:"$OUT_DIR"

echo "Done. Look for MdixLanguageSupport_*.mpack in $OUT_DIR"
