#!/usr/bin/env python3
"""
DixScript CLI Test Runner
Executes mdix commands against test fixtures and writes cli-test-results.json.

Run from workspace root via CI:
    MDIX_BINARY=target/debug/mdix python3 tools/run_cli_tests.py
"""

import json
import os
import subprocess
import sys
import time
from pathlib import Path

REPO_ROOT   = Path(__file__).parent.parent.resolve()
BINARY      = os.environ.get("MDIX_BINARY", "target/debug/mdix")
OUTPUT_FILE = "cli-test-results.json"
TMP_DIR     = "/tmp/mdix_cli_test"

# ── Test fixtures ─────────────────────────────────────────────────────────────

TEST_FILES = [
    "mdix_files/tests/cli/01_basic.mdix",
    "mdix_files/tests/cli/02_enums.mdix",
    "mdix_files/tests/cli/03_quickfuncs.mdix",
    "mdix_files/tests/cli/04_all_types.mdix",
    "mdix_files/tests/cli/05_game_config.mdix",
    "mdix_files/tests/cli/06_app_config.mdix",
]

# (display_name, args_list)
# {file} is substituted with the actual file path
FILE_COMMANDS = [
    ("validate",                      ["validate", "{file}"]),
    ("validate --strict",             ["validate", "--strict", "{file}"]),
    ("validate --json",               ["validate", "--json", "{file}"]),
    ("validate --quiet",              ["validate", "--quiet", "{file}"]),
    ("compile --skip-dlm",            ["compile", "--skip-dlm", "{file}", "-o", TMP_DIR + "/compile_out"]),
    ("compile --skip-dlm --json",     ["compile", "--skip-dlm", "--json", "{file}", "-o", TMP_DIR + "/compile_json_out"]),
    ("inspect",                       ["inspect", "{file}"]),
    ("inspect --keys",                ["inspect", "--keys", "{file}"]),
    ("inspect --sections",            ["inspect", "--sections", "{file}"]),
    ("inspect --json",                ["inspect", "--json", "{file}"]),
    ("convert --to json",             ["convert", "--to", "json", "{file}", "-o", TMP_DIR + "/out.json"]),
    ("convert --to toml",             ["convert", "--to", "toml", "{file}", "-o", TMP_DIR + "/out.toml"]),
    ("convert --to json --json",      ["convert", "--to", "json", "--json", "{file}", "-o", TMP_DIR + "/out_env.json"]),
    ("compact",                       ["compact", "{file}", "-o", TMP_DIR + "/out.compact.mdix"]),
    ("compact --mode minify",         ["compact", "--mode", "minify", "{file}", "-o", TMP_DIR + "/out.min.mdix"]),
    ("compact --mode strip-comments", ["compact", "--mode", "strip-comments", "{file}", "-o", TMP_DIR + "/out.stripped.mdix"]),
    ("compact --ratio",               ["compact", "--ratio", "{file}", "-o", TMP_DIR + "/out.ratio.mdix"]),
    ("format --check",                ["format", "--check", "{file}"]),
]

# Commands that are not tied to a specific fixture file
GLOBAL_COMMANDS = [
    ("create basic",
     ["create", TMP_DIR + "/new_basic.mdix"]),
    ("create --force (overwrite)",
     ["create", "--force", TMP_DIR + "/new_basic.mdix"]),
    ("create --template advanced",
     ["create", "--template", "advanced", TMP_DIR + "/new_advanced.mdix"]),
    ("create --template security",
     ["create", "--template", "security", TMP_DIR + "/new_security.mdix"]),
    ("create --template dlm",
     ["create", "--template", "dlm", TMP_DIR + "/new_dlm.mdix"]),
    ("create unknown template (expect fail)",
     ["create", "--template", "badtemplate", TMP_DIR + "/bad.mdix"]),
    ("key generate aes256",
     ["key", "generate", "--output", TMP_DIR + "/test_aes256.mdix.key", "--algorithm", "aes256"]),
    ("key generate aes128",
     ["key", "generate", "--output", TMP_DIR + "/test_aes128.mdix.key", "--algorithm", "aes128"]),
    ("key generate chacha20",
     ["key", "generate", "--output", TMP_DIR + "/test_chacha20.mdix.key", "--algorithm", "chacha20"]),
    ("key generate --password",
     ["key", "generate", "--password", "--output", TMP_DIR + "/test_pw.mdix.key"]),
    ("key validate",
     ["key", "validate", TMP_DIR + "/test_aes256.mdix.key"]),
    ("key validate missing (expect exit 2)",
     ["key", "validate", TMP_DIR + "/ghost.mdix.key"]),
    ("key info",
     ["key", "info", TMP_DIR + "/test_aes256.mdix.key"]),
    ("key info --json",
     ["key", "info", "--json", TMP_DIR + "/test_aes256.mdix.key"]),
    ("config list",
     ["config", "list"]),
    ("config list --json",
     ["--json", "config", "list"]),
    ("config get default_indent_size",
     ["config", "get", "default_indent_size"]),
    ("config get unknown key (expect fail)",
     ["config", "get", "no_such_key_xyz"]),
    ("config set default_indent_size 4",
     ["config", "set", "default_indent_size", "4"]),
    ("config reset default_indent_size",
     ["config", "reset", "default_indent_size"]),
    ("config reset all",
     ["config", "reset"]),
    ("validate nonexistent file (expect exit 2)",
     ["validate", TMP_DIR + "/ghost.mdix"]),
    ("compile nonexistent file (expect exit 2)",
     ["compile", TMP_DIR + "/ghost.mdix"]),
    ("convert unknown format (expect exit 4)",
     ["convert", "mdix_files/tests/cli/01_basic.mdix", "--to", "xyz",
      "-o", TMP_DIR + "/bad.xyz"]),
]

# Commands where a non-zero exit is the correct result
EXPECT_NONZERO = {
    "create unknown template (expect fail)",
    "key validate missing (expect exit 2)",
    "config get unknown key (expect fail)",
    "validate nonexistent file (expect exit 2)",
    "compile nonexistent file (expect exit 2)",
    "convert unknown format (expect exit 4)",
}

# format --check exits 1 when file is not formatted — still a valid run
FORMAT_CHECK_OK_EXITS = {0, 1}


# ── Runner ────────────────────────────────────────────────────────────────────

def run_cmd(args):
    full = [str(REPO_ROOT / BINARY)] + args
    start = time.monotonic()
    try:
        proc = subprocess.run(
            full,
            capture_output=True,
            text=True,
            timeout=30,
            cwd=str(REPO_ROOT),
        )
        elapsed_ms = int((time.monotonic() - start) * 1000)
        return {
            "exit_code":  proc.returncode,
            "stdout":     proc.stdout,
            "stderr":     proc.stderr,
            "elapsed_ms": elapsed_ms,
            "timed_out":  False,
        }
    except subprocess.TimeoutExpired:
        return {
            "exit_code":  -1,
            "stdout":     "",
            "stderr":     "Command timed out after 30 seconds.",
            "elapsed_ms": 30000,
            "timed_out":  True,
        }
    except Exception as exc:
        return {
            "exit_code":  -1,
            "stdout":     "",
            "stderr":     str(exc),
            "elapsed_ms": 0,
            "timed_out":  False,
        }


def determine_status(name, result):
    if result["timed_out"]:
        return "failed"
    code = result["exit_code"]
    if name in EXPECT_NONZERO:
        return "passed" if code != 0 else "failed"
    if "format --check" in name:
        return "passed" if code in FORMAT_CHECK_OK_EXITS else "failed"
    return "passed" if code == 0 else "failed"


def run_suite(suite_name, file_path, commands_spec):
    commands = []
    for cmd_name, args_template in commands_spec:
        if file_path:
            args = [a.replace("{file}", file_path) for a in args_template]
        else:
            args = list(args_template)

        full_cmd = "mdix " + " ".join(args)
        result   = run_cmd(args)
        status   = determine_status(cmd_name, result)

        tag = "PASS" if status == "passed" else "FAIL"
        print(f"  [{tag}]  {suite_name} / {cmd_name}"
              f"  (exit {result['exit_code']}, {result['elapsed_ms']}ms)")

        commands.append({
            "name":         cmd_name,
            "full_command": full_cmd,
            "status":       status,
            "exit_code":    result["exit_code"],
            "stdout":       result["stdout"],
            "stderr":       result["stderr"],
            "elapsed_ms":   result["elapsed_ms"],
        })

    suite_passed = sum(1 for c in commands if c["status"] == "passed")
    suite_failed = len(commands) - suite_passed
    return {
        "name":     suite_name,
        "file":     file_path,
        "passed":   suite_passed,
        "failed":   suite_failed,
        "commands": commands,
    }


def main():
    binary_abs = REPO_ROOT / BINARY
    print(f"[run_cli_tests] binary   : {binary_abs}")
    print(f"[run_cli_tests] repo root: {REPO_ROOT}")

    if not binary_abs.exists():
        print(f"ERROR: binary not found at {binary_abs}", file=sys.stderr)
        print("       Run: cargo build -p mdix-cli", file=sys.stderr)
        sys.exit(1)

    # Ensure tmp dir exists
    Path(TMP_DIR).mkdir(parents=True, exist_ok=True)

    suites = []
    total  = 0
    passed = 0

    # Per-file suites
    for fpath in TEST_FILES:
        name  = Path(fpath).name
        suite = run_suite(name, fpath, FILE_COMMANDS)
        suites.append(suite)
        total  += suite["passed"] + suite["failed"]
        passed += suite["passed"]

    # Global suite
    global_suite = run_suite("global (no fixture)", None, GLOBAL_COMMANDS)
    suites.append(global_suite)
    total  += global_suite["passed"] + global_suite["failed"]
    passed += global_suite["passed"]

    failed = total - passed

    data = {
        "build":        os.environ.get("BUILD_NUM",    "0"),
        "branch":       os.environ.get("BRANCH",       "unknown"),
        "commit":       os.environ.get("COMMIT",       "unknown")[:8],
        "date":         os.environ.get("BUILD_DATE",   ""),
        "rust_version": os.environ.get("RUST_VERSION", "stable"),
        "summary": {
            "total":  total,
            "passed": passed,
            "failed": failed,
        },
        "suites": suites,
    }

    with open(OUTPUT_FILE, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)

    print()
    print(f"[run_cli_tests] Total={total}  Passed={passed}  Failed={failed}")
    print(f"[run_cli_tests] Written: {OUTPUT_FILE}")

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
