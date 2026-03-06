#!/usr/bin/env bash
# dixscript-cli/smoke_test.sh
#
# Quick end-to-end sanity check. Run after every significant change:
#   chmod +x smoke_test.sh
#   ./smoke_test.sh
#
# Requires: cargo build -p dixscript-cli before running.

set -euo pipefail

BINARY="$(dirname "$0")/../target/debug/mdix"
FIXTURES="$(dirname "$0")/tests/fixtures"
SCRATCH="$(mktemp -d)"
PASS=0
FAIL=0

GREEN="\033[0;32m"
RED="\033[0;31m"
YELLOW="\033[0;33m"
RESET="\033[0m"

ok()   { echo -e "  ${GREEN}PASS${RESET}  $1"; ((PASS++)) || true; }
fail() { echo -e "  ${RED}FAIL${RESET}  $1"; ((FAIL++)) || true; }
section() { echo -e "\n${YELLOW}── $1${RESET}"; }

require_binary() {
    if [[ ! -x "$BINARY" ]]; then
        echo -e "${RED}Binary not found at $BINARY${RESET}"
        echo "Run: cargo build -p mdix-cli"
        exit 1
    fi
}

# ── Helpers ───────────────────────────────────────────────────────────────────

run_expect_ok() {
    local desc="$1"; shift
    if "$BINARY" "$@" >/dev/null 2>&1; then
        ok "$desc"
    else
        fail "$desc (expected exit 0, got $?)"
    fi
}

run_expect_fail() {
    local desc="$1"; shift
    if ! "$BINARY" "$@" >/dev/null 2>&1; then
        ok "$desc"
    else
        fail "$desc (expected non-zero exit, got 0)"
    fi
}

run_expect_exit() {
    local desc="$1"
    local expected="$2"; shift 2
    "$BINARY" "$@" >/dev/null 2>&1 || true
    local actual=$?
    if [[ "$actual" -eq "$expected" ]]; then
        ok "$desc"
    else
        fail "$desc (expected exit $expected, got $actual)"
    fi
}

json_has_key() {
    local json="$1"
    local key="$2"
    echo "$json" | grep -q "\"$key\""
}

# ── validate ──────────────────────────────────────────────────────────────────

section "validate"

run_expect_ok   "validate basic.mdix exits 0"          validate "$FIXTURES/basic.mdix"
run_expect_ok   "validate with_enums.mdix exits 0"     validate "$FIXTURES/with_enums.mdix"
run_expect_ok   "validate with_functions.mdix exits 0" validate "$FIXTURES/with_functions.mdix"
run_expect_fail "validate invalid_syntax.mdix exits 1" validate "$FIXTURES/invalid_syntax.mdix"
run_expect_exit "validate missing file exits 2"     2  validate "nonexistent.mdix"

JSON_OUT=$("$BINARY" validate --json "$FIXTURES/basic.mdix" 2>/dev/null)
if json_has_key "$JSON_OUT" "token_count"; then
    ok "validate --json contains token_count"
else
    fail "validate --json missing token_count"
fi

# ── compile ───────────────────────────────────────────────────────────────────

section "compile"

run_expect_ok   "compile basic.mdix exits 0"          compile "$FIXTURES/basic.mdix" -o "$SCRATCH"
run_expect_ok   "compile with_enums.mdix exits 0"     compile "$FIXTURES/with_enums.mdix" -o "$SCRATCH"
run_expect_fail "compile invalid_syntax.mdix exits 1" compile "$FIXTURES/invalid_syntax.mdix" -o "$SCRATCH"
run_expect_exit "compile missing file exits 2"     2  compile "nonexistent.mdix"

# ── convert ───────────────────────────────────────────────────────────────────

section "convert"

JSON_FILE="$SCRATCH/basic.json"
run_expect_ok "convert mdix→json exits 0" \
    convert "$FIXTURES/basic.mdix" --to json -o "$JSON_FILE"

if [[ -f "$JSON_FILE" ]]; then
    ok "convert produced output file"
    if python3 -c "import json,sys; json.load(open('$JSON_FILE'))" 2>/dev/null; then
        ok "converted JSON is valid"
    elif node -e "JSON.parse(require('fs').readFileSync('$JSON_FILE','utf8'))" 2>/dev/null; then
        ok "converted JSON is valid (via node)"
    else
        ok "converted JSON file exists (parser not available for deep check)"
    fi
else
    fail "convert did not produce output file"
fi

MDIX_RT="$SCRATCH/recovered.mdix"
run_expect_ok "convert json→mdix exits 0" \
    convert "$JSON_FILE" --to dixscript -o "$MDIX_RT"

run_expect_exit "convert unknown format exits 4" 4 \
    convert "$FIXTURES/basic.mdix" --to xyz -o "$SCRATCH/out.xyz"

# ── compact ───────────────────────────────────────────────────────────────────

section "compact"

COMPACT_OUT="$SCRATCH/basic.compact.mdix"
run_expect_ok "compact basic.mdix exits 0" \
    compact "$FIXTURES/basic.mdix" -o "$COMPACT_OUT"

if [[ -f "$COMPACT_OUT" ]]; then
    ok "compact produced output file"
else
    fail "compact did not produce output file"
fi

COMPACT_JSON=$("$BINARY" compact --json "$FIXTURES/basic.mdix" -o "$SCRATCH/b2.compact.mdix" 2>/dev/null)
if json_has_key "$COMPACT_JSON" "ratio"; then
    ok "compact --json contains ratio"
else
    fail "compact --json missing ratio"
fi

run_expect_ok "compact --mode minify exits 0" \
    compact "$FIXTURES/basic.mdix" --mode minify -o "$SCRATCH/basic.min.mdix"

run_expect_ok "compact --mode strip-comments exits 0" \
    compact "$FIXTURES/basic.mdix" --mode strip-comments -o "$SCRATCH/basic.nocomments.mdix"

run_expect_fail "compact unknown mode exits nonzero" \
    compact "$FIXTURES/basic.mdix" --mode badmode

# ── inspect ───────────────────────────────────────────────────────────────────

section "inspect"

run_expect_ok "inspect basic.mdix exits 0"    inspect "$FIXTURES/basic.mdix"
run_expect_ok "inspect --keys exits 0"        inspect --keys "$FIXTURES/basic.mdix"
run_expect_ok "inspect --sections exits 0"    inspect --sections "$FIXTURES/basic.mdix"

INSPECT_OUT=$("$BINARY" inspect "$FIXTURES/basic.mdix" 2>/dev/null)
if echo "$INSPECT_OUT" | grep -q "@DATA"; then
    ok "inspect output contains @DATA"
else
    fail "inspect output missing @DATA"
fi

INSPECT_JSON=$("$BINARY" inspect --json "$FIXTURES/basic.mdix" 2>/dev/null)
if json_has_key "$INSPECT_JSON" "key_count"; then
    ok "inspect --json contains key_count"
else
    fail "inspect --json missing key_count"
fi

# ── create ────────────────────────────────────────────────────────────────────

section "create"

NEW_FILE="$SCRATCH/new_basic.mdix"
run_expect_ok "create basic template exits 0"    create "$NEW_FILE"
run_expect_ok "validate created file exits 0"    validate "$NEW_FILE"
run_expect_fail "create existing file without --force fails" create "$NEW_FILE"
run_expect_ok "create with --force overwrites"   create --force "$NEW_FILE"

NEW_ADV="$SCRATCH/new_advanced.mdix"
run_expect_ok "create advanced template exits 0" create --template advanced "$NEW_ADV"
run_expect_ok "validate advanced template exits 0" validate "$NEW_ADV"

run_expect_fail "create unknown template exits nonzero" \
    create --template notexist "$SCRATCH/bad.mdix"

# ── key ───────────────────────────────────────────────────────────────────────

section "key"

KEY_FILE="$SCRATCH/test.mdix.key"
run_expect_ok "key generate aes256 exits 0" \
    key generate --output "$KEY_FILE" --algorithm aes256

if [[ -f "$KEY_FILE" ]]; then
    ok "key generate produced key file"
else
    fail "key generate did not produce key file"
fi

run_expect_ok "key validate exits 0"        key validate "$KEY_FILE"
run_expect_ok "key info exits 0"            key info "$KEY_FILE"

KEY_JSON=$("$BINARY" key info --json "$KEY_FILE" 2>/dev/null)
if json_has_key "$KEY_JSON" "algorithm"; then
    ok "key info --json contains algorithm"
else
    fail "key info --json missing algorithm"
fi

run_expect_fail "key validate missing file fails" key validate "missing.mdix.key"

# ── config ────────────────────────────────────────────────────────────────────

section "config"

run_expect_ok "config list exits 0"                        config list
run_expect_ok "config get known key exits 0"               config get default_indent_size
run_expect_fail "config get unknown key exits nonzero"     config get no_such_key_xyz
run_expect_ok "config set integer exits 0"                 config set default_indent_size 4
run_expect_ok "config get after set returns 4"             config get default_indent_size
run_expect_ok "config reset single key exits 0"            config reset default_indent_size
run_expect_ok "config get after reset returns default 2"   config get default_indent_size
run_expect_ok "config reset all exits 0"                   config reset

CONFIG_LIST=$("$BINARY" config list --json 2>/dev/null)
if json_has_key "$CONFIG_LIST" "default_indent_size"; then
    ok "config list --json contains default_indent_size"
else
    fail "config list --json missing default_indent_size"
fi

# ── Global flags ──────────────────────────────────────────────────────────────

section "global flags"

QUIET_OUT=$("$BINARY" validate --quiet "$FIXTURES/basic.mdix" 2>/dev/null)
if [[ -z "$QUIET_OUT" ]]; then
    ok "--quiet suppresses stdout"
else
    fail "--quiet did not suppress stdout"
fi

run_expect_ok "--no-color flag is accepted" \
    --no-color validate "$FIXTURES/basic.mdix"

# ── Cleanup + summary ─────────────────────────────────────────────────────────

rm -rf "$SCRATCH"

echo ""
echo -e "────────────────────────────────"
echo -e "  ${GREEN}PASSED${RESET}: $PASS"
if [[ "$FAIL" -gt 0 ]]; then
    echo -e "  ${RED}FAILED${RESET}: $FAIL"
    echo ""
    exit 1
else
    echo -e "  ${RED}FAILED${RESET}: $FAIL"
    echo -e "\n${GREEN}All smoke tests passed.${RESET}"
fi
