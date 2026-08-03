#!/usr/bin/env bash
# build.sh — build the DixScript LSP server (and optionally the CLI)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# ── defaults ──────────────────────────────────────────────────────────────────
PROFILE="${PROFILE:-release}"   # override: PROFILE=dev ./build.sh
TARGET_DIR="target/$PROFILE"

# ── helpers ───────────────────────────────────────────────────────────────────
bold()  { printf '\033[1m%s\033[0m\n' "$*"; }
ok()    { printf '\033[32m✓\033[0m  %s\n' "$*"; }
info()  { printf '\033[34m→\033[0m  %s\n' "$*"; }
err()   { printf '\033[31m✗\033[0m  %s\n' "$*" >&2; exit 1; }

# ── sanity checks ─────────────────────────────────────────────────────────────
command -v cargo &>/dev/null || err "cargo not found — install Rust from https://rustup.rs"

bold "DixScript build  (profile: $PROFILE)"
echo ""

# ── build LSP ────────────────────────────────────────────────────────────────
info "Building mdix-lsp …"
if [[ "$PROFILE" == "release" ]]; then
    cargo build -p mdix-lsp --release 2>&1
else
    cargo build -p mdix-lsp 2>&1
fi
ok "mdix-lsp → $TARGET_DIR/mdix-lsp"

# ── build CLI (optional, skip with SKIP_CLI=1) ────────────────────────────────
if [[ "${SKIP_CLI:-0}" != "1" ]]; then
    info "Building mdix-cli …"
    if [[ "$PROFILE" == "release" ]]; then
        cargo build -p mdix-cli --release 2>&1
    else
        cargo build -p mdix-cli 2>&1
    fi
    ok "mdix-cli  → $TARGET_DIR/mdix"
fi

echo ""
bold "Done."
echo ""
echo "  LSP binary : $SCRIPT_DIR/$TARGET_DIR/mdix-lsp"
if [[ "${SKIP_CLI:-0}" != "1" ]]; then
echo "  CLI binary : $SCRIPT_DIR/$TARGET_DIR/mdix"
fi
echo ""
echo "  VS Code / Neovim: point your mdix extension at the LSP binary above."
echo "  Run tests:        cargo test -p mdix-lsp"
echo "  Run LSP tests:    MDIX_LSP_BIN=$SCRIPT_DIR/$TARGET_DIR/mdix-lsp \\"
echo "                      cargo test -p mdix-lsp --test lsp_integration"
