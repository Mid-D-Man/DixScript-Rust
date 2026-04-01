#!/usr/bin/env python3
"""
Lua test runner for mdix-lua CI.

Locates the built shared library, copies it into the test directory so that
Lua's require("mdix") can find it, runs the Lua test suite, and writes
lua-test-results.json for the HTML report.

Exit code mirrors the Lua runner (0 = all pass, 1 = failures).
"""

import json
import os
import platform
import shutil
import subprocess
import sys


# ── Helpers ────────────────────────────────────────────────────────────────────

def log(msg: str) -> None:
    print(msg, file=sys.stderr, flush=True)


def write_failure_json(path: str, env: dict, error_msg: str) -> None:
    data = {
        "build":       env.get("BUILD_NUM", "0"),
        "branch":      env.get("BRANCH", "unknown"),
        "commit":      env.get("COMMIT", "unknown")[:8],
        "date":        env.get("BUILD_DATE", ""),
        "lua_version": "unknown",
        "tests": {
            "total": 1, "passed": 0, "failed": 1, "duration_s": 0.0,
        },
        "suites": [{
            "name":       "bootstrap",
            "passed":     0,
            "failed":     1,
            "duration_s": 0.0,
            "tests": [{
                "name":        "library_available",
                "status":      "failed",
                "duration_ms": 0,
                "output":      error_msg,
            }],
        }],
    }
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)
    log(f"Wrote failure placeholder → {path}")


# ── Main ───────────────────────────────────────────────────────────────────────

def main() -> int:
    env = {
        "BUILD_NUM":  os.environ.get("BUILD_NUM", "0"),
        "BRANCH":     os.environ.get("BRANCH", "unknown"),
        "COMMIT":     os.environ.get("COMMIT", "unknown"),
        "BUILD_DATE": os.environ.get("BUILD_DATE", ""),
    }

    output_json = "lua-test-results.json"

    # ── 1. Locate built library ────────────────────────────────────────────────

    system = platform.system()
    if system == "Darwin":
        lib_name, dest_name = "libmdix.dylib", "mdix.so"
    elif system == "Windows":
        lib_name, dest_name = "mdix.dll",      "mdix.dll"
    else:
        lib_name, dest_name = "libmdix.so",    "mdix.so"

    lib_path = os.path.join("target", "release", lib_name)
    if not os.path.exists(lib_path):
        msg = (
            f"Library not found: {lib_path}\n"
            "Run 'cargo build -p mdix-lua --release' first."
        )
        log(f"ERROR: {msg}")
        write_failure_json(output_json, env, msg)
        return 1

    log(f"Found library: {lib_path}  ({os.path.getsize(lib_path):,} bytes)")

    # ── 2. Copy library into test directory ────────────────────────────────────

    test_dir = os.path.join("mdix-lua", "tests")
    os.makedirs(test_dir, exist_ok=True)
    dest = os.path.join(test_dir, dest_name)

    log(f"Copying {lib_path} → {dest}")
    shutil.copy2(lib_path, dest)

    # ── 3. Locate Lua 5.4 interpreter ─────────────────────────────────────────

    for candidate in ("lua5.4", "lua54", "lua"):
        if shutil.which(candidate):
            lua_bin = candidate
            break
    else:
        msg = "Lua 5.4 interpreter not found on PATH."
        log(f"ERROR: {msg}")
        write_failure_json(output_json, env, msg)
        return 1

    ver_result = subprocess.run(
        [lua_bin, "-v"], capture_output=True, text=True
    )
    log(f"Using: {lua_bin}  {ver_result.stderr.strip() or ver_result.stdout.strip()}")

    # ── 4. Run test suite ──────────────────────────────────────────────────────

    runner = os.path.join(test_dir, "run_tests.lua")
    run_env = {**os.environ, **env}

    log(f"Running: {lua_bin} {runner}")

    result = subprocess.run(
        [lua_bin, runner],
        capture_output=True,
        text=True,
        env=run_env,
        cwd=os.getcwd(),
    )

    # Forward stderr from the Lua runner
    if result.stderr:
        log(result.stderr)

    # ── 5. Parse JSON output ───────────────────────────────────────────────────

    raw = result.stdout.strip()
    if not raw:
        msg = (
            f"Lua runner produced no JSON output (exit={result.returncode}).\n"
            f"stderr: {result.stderr[:400]}"
        )
        log(f"ERROR: {msg}")
        write_failure_json(output_json, env, msg)
        return result.returncode or 1

    try:
        data = json.loads(raw)
    except json.JSONDecodeError as exc:
        msg = f"Failed to parse Lua runner output: {exc}\nOutput: {raw[:300]}"
        log(f"ERROR: {msg}")
        write_failure_json(output_json, env, msg)
        return 1

    with open(output_json, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)

    tests  = data.get("tests", {})
    total  = tests.get("total",  0)
    passed = tests.get("passed", 0)
    failed = tests.get("failed", 0)
    dur    = tests.get("duration_s", 0.0)

    log(f"Results → Total:{total}  Passed:{passed}  Failed:{failed}  ({dur:.2f}s)")
    log(f"Wrote {output_json}")

    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
