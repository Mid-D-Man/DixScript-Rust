#!/usr/bin/env python3
"""
Parse `cargo test --color=never` output → test-results.json

Input:   test-raw.txt      (CWD)
Output:  test-results.json (CWD)

Environment variables (injected from CI context):
  BUILD_NUM  BRANCH  COMMIT  BUILD_DATE  RUST_VERSION
"""

import json
import os
import re

RE_ANSI = re.compile(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])")


def strip_ansi(text):
    return RE_ANSI.sub("", text)


def build_suite(name, tests, dur_s=0.0):
    return {
        "name":       name,
        "passed":     sum(1 for t in tests if t["status"] == "passed"),
        "failed":     sum(1 for t in tests if t["status"] == "failed"),
        "ignored":    sum(1 for t in tests if t["status"] == "ignored"),
        "duration_s": round(dur_s, 4),
        "tests":      tests,
    }


def main():
    raw  = open("test-raw.txt", encoding="utf-8", errors="replace").read()
    text = strip_ansi(raw)

    print("=== test-raw.txt (first 30 lines) ===")
    for i, line in enumerate(text.splitlines()[:30], 1):
        print(f"  {i:3}: {repr(line)}")
    print("======================================")

    # Collect per-test stdout blocks
    test_output = {}
    for m in re.finditer(
        r"---- ([\w:]+(?:::\w+)*) stdout ----\n(.*?)"
        r"(?=\n---- |\n\n(?:successes|failures):\n|\ntest result:|\Z)",
        text,
        re.DOTALL,
    ):
        key     = m.group(1)
        content = m.group(2).strip()
        if content:
            test_output[key] = content

    suites    = []
    cur_name  = None
    cur_tests = []
    cur_dur   = 0.0

    RE_RUN    = re.compile(r"Running\s+(?:unittests?\s+)?(\S+)\s+\(")
    RE_TEST   = re.compile(
        r"^test\s+([\w:]+(?:::\w+)*)\s+\.\.\.\s+"
        r"(ok|FAILED|ignored)(?:\s+\(([\d.]+)s\))?"
    )
    RE_RESULT = re.compile(r"test result:.*?finished in ([\d.]+)s")

    for line in text.splitlines():
        m = RE_RUN.search(line)
        if m:
            if cur_name is not None and cur_tests:
                suites.append(build_suite(cur_name, cur_tests, cur_dur))
            base = os.path.splitext(os.path.basename(m.group(1)))[0]
            cur_name  = (base + " (unit tests)") if base in ("lib", "main") else base
            cur_tests, cur_dur = [], 0.0
            continue

        m = RE_TEST.match(line)
        if m:
            if cur_name is None:
                cur_name, cur_tests, cur_dur = "lib (unit tests)", [], 0.0
            tname, status, dur = m.group(1), m.group(2), m.group(3)
            cur_tests.append({
                "name":        tname,
                "status":      {"ok": "passed", "FAILED": "failed", "ignored": "ignored"}.get(
                    status, "unknown"
                ),
                "duration_ms": int(float(dur) * 1000) if dur else 0,
                "output":      test_output.get(tname, ""),
            })
            continue

        m = RE_RESULT.search(line)
        if m:
            cur_dur = float(m.group(1))
            if cur_name is not None and cur_tests:
                suites.append(build_suite(cur_name, cur_tests, cur_dur))
                cur_name, cur_tests, cur_dur = None, [], 0.0

    if cur_name and cur_tests:
        suites.append(build_suite(cur_name, cur_tests, cur_dur))

    total   = sum(s["passed"] + s["failed"] + s["ignored"] for s in suites)
    passed  = sum(s["passed"]  for s in suites)
    failed  = sum(s["failed"]  for s in suites)
    ignored = sum(s["ignored"] for s in suites)
    dur     = sum(s["duration_s"] for s in suites)

    result = {
        "build":        os.environ.get("BUILD_NUM",    "0"),
        "branch":       os.environ.get("BRANCH",       "unknown"),
        "commit":       os.environ.get("COMMIT",       "unknown")[:8],
        "date":         os.environ.get("BUILD_DATE",   ""),
        "rust_version": os.environ.get("RUST_VERSION", "stable"),
        "summary": {
            "total":      total,
            "passed":     passed,
            "failed":     failed,
            "ignored":    ignored,
            "duration_s": round(dur, 3),
        },
        "suites": suites,
    }

    with open("test-results.json", "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)

    print(f"Suites:{len(suites)}  Total:{total}  Passed:{passed}  Failed:{failed}")


if __name__ == "__main__":
    main()
