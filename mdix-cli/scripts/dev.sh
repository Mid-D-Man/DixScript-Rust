#!/usr/bin/env bash
# Development helper — rebuilds the CLI in debug mode and runs any command
# against it, equivalent to the C# project's dix-dev symlink workflow.
#
# Usage:
#   ./scripts/dev.sh                        # print help
#   ./scripts/dev.sh validate tests/fixtures/basic.mdix
#   ./scripts/dev.sh compile tests/fixtures/with_enums.mdix -o /tmp/out
#   ./scripts/dev.sh convert tests/fixtures/basic.mdix --to json
#
# After first run the alias 'mdix-dev' is available in the current shell if
# you source the script:
#   source ./scripts/dev.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="$REPO_ROOT/target/debug/mdix"

GREEN="\033[0;32m"
RED="\033[0;31m"
YELLOW="\033[0;33m"
RESET="\033[0m"

rebuild() {
    echo -e "${YELLOW}→ rebuilding mdix-cli (debug)...${RESET}"
    cargo build -p mdix-cli --quiet 2>&1 | tail -5
    if [[ $? -ne 0 ]]; then
        echo -e "${RED}✗ build failed${RESET}"
        exit 1
    fi
    echo -e "${GREEN}✓ build ok${RESET}"
}

# Always rebuild before running so you're never testing stale code.
rebuild

# If no args, print help and stop.
if [[ $# -eq 0 ]]; then
    "$BINARY" --help
    echo ""
    echo -e "${YELLOW}tip:${RESET} source this script to get the 'mdix-dev' alias in your shell"
    exit 0
fi

# Run the binary with all forwarded arguments.
echo -e "${YELLOW}→ running: mdix $*${RESET}"
"$BINARY" "$@"
