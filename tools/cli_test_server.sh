#!/usr/bin/env bash
# tools/cli_test_server.sh
# Build the CLI (debug) then start the test server.
# Usage:  ./tools/cli_test_server.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

GREEN="\033[0;32m"; YELLOW="\033[0;33m"; RESET="\033[0m"

echo -e "${YELLOW}→ building mdix-cli (debug)…${RESET}"
cargo build -p mdix-cli --quiet
echo -e "${GREEN}✓ build ok${RESET}"
echo ""
echo -e "${YELLOW}→ starting test server…${RESET}"
python3 tools/cli_test_server.py
