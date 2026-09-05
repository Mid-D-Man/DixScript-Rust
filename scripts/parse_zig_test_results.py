#!/usr/bin/env python3
"""parse_zig_test_results.py — used by .github/workflows/zig-ci.yml.

Parses `zig build test-<suite> --summary all` raw output and reshapes it
into the same {build, branch, commit, date, <lang>_version, package,
tests{}, suites[]} JSON shape go-results.json / odin-results.json
already use, so zig-test-template.html can reuse go-test-template.html's
render() JS wholesale (same reasoning odin-ci.yml's own "Generate
odin-results.json" step gives). A standalone script rather than an
inline `python3 - << PYEOF` block in the workflow (the pattern
odin-ci.yml/go-ci.yml use) since this covers two separate build stages
and grew hard to follow as one growing heredoc.

Two modes:

  parse-log <suite> <logfile>
      Reads one Zig `--summary all` build log and writes <suite>_status/
      _total/_passed/_failed/_duration_s/_failed_names to $GITHUB_OUTPUT.
      Parses the `Build Summary: A/B steps succeeded; C/D tests passed`
      line for total/passed/failed (Zig 0.16's build-driven test runner
      reports step summaries, not a "Test [n/m] name...OK" line per
      test the way a bare `zig test` invocation does — confirmed against
      real CI output, not assumed) and every `error: 'name' <reason>`
      line for failed_names (crashed and leaked tests are both reported
      this way; a leak still counts toward "tests passed" in Zig's own
      tally, so failed_names can be non-empty even when passed == total
      — that's expected, not a bug in this script). `suite` is a short
      key (e.g. "ffi", "mdix") — also used, upper-cased, as the prefix
      for the <SUITE>_EXIT / <SUITE>_DURATION_S environment variables
      this reads the real exit code and timing from. That's deliberate:
      the exit code is the authoritative pass/fail signal (a leak fails
      the build even though Zig still counts the test itself as
      passed), decoupled from whatever the text happened to parse to —
      same split odin-ci.yml keeps between $TEST_EXIT and its JSON
      report.

  build-results
      Reads FFI_*/MDIX_*/BUILD_*/BRANCH/COMMIT/BUILD_DATE/ZIG_VER env
      vars (the two parse-log runs' step outputs, threaded through the
      workflow as job outputs) and writes zig-results.json in the
      shared shape — one suite entry per Zig test binary this project
      currently defines (mdix_ffi, mdix; see mdix-zig/build.zig's
      `test-ffi` / `test-mdix` steps).

See .github/workflows/zig-ci.yml for the exact invocation of each mode.
"""

import argparse
import json
import os
import re
import sys

TEST_LINE_RE = re.compile(r"^Test \[(\d+)/(\d+)\] (.*?)\.\.\.(OK|FAIL)(.*)$")
ALL_PASSED_RE = re.compile(r"^All (\d+) tests? passed\.$")
MIXED_RE = re.compile(r"^(\d+) passed; (\d+) skipped; (\d+) failed\.$")

# Zig 0.16's `--summary all` (the format actually used here — see
# mdix-zig/build.zig) reports build STEPS, not individual tests, as its
# primary summary line — the three patterns above matched an older/
# different Zig test-runner text format that was never real for this
# project's toolchain; they're kept only because the fallback branch
# below still tries them for anyone piping in different output. The one
# that actually appears in this project's CI logs:
#   Build Summary: 3/3 steps succeeded; 5/5 tests passed
#   Build Summary: 1/3 steps succeeded (1 failed); 0/3 tests passed (3 crashed)
SUMMARY_RE = re.compile(
    r"Build Summary:\s*\d+/\d+\s*steps succeeded(?:\s*\(\d+\s*failed\))?;\s*"
    r"(\d+)/(\d+)\s*tests passed(?:\s*\((\d+)\s*crashed\))?"
)
# Zig reports a failing/crashing/leaking test as `error: 'name' <reason>`
# — this is the only place a test's name appears in --summary all output
# (there's no per-test "Test [n/m] name...OK/FAIL" line the way a plain
# `zig test` invocation prints one).
FAILED_TEST_RE = re.compile(r"error: '([^']+)'")

# Suites this project currently defines a `zig build test-<key>` step
# for — see mdix-zig/build.zig. Extend this alongside build.zig if a new
# suite gets its own step.
SUITES = (("ffi", "mdix_ffi"), ("mdix", "mdix"), ("behavioral", "mdix_tests"))


def parse_log(suite: str, logfile: str) -> None:
    text = ""
    if os.path.exists(logfile):
        with open(logfile, encoding="utf-8", errors="replace") as f:
            text = f.read()

    total = passed = failed = 0
    m_summary = SUMMARY_RE.search(text)
    if m_summary:
        passed = int(m_summary.group(1))
        total = int(m_summary.group(2))
        failed = total - passed
    else:
        # Fall back to the older per-test-line text shape, in case
        # something upstream of this script ever pipes in a plain
        # `zig test` invocation's output instead of `--summary all`'s.
        tests: list[tuple[str, str]] = []
        for line in text.splitlines():
            m = TEST_LINE_RE.match(line.strip())
            if m:
                name, verdict = m.group(3), m.group(4)
                tests.append((name, "passed" if verdict == "OK" else "failed"))
        m_mixed = MIXED_RE.search(text)
        m_all = ALL_PASSED_RE.search(text)
        if m_mixed:
            passed, failed = int(m_mixed.group(1)), int(m_mixed.group(3))
            total = passed + failed
        elif m_all:
            total = passed = int(m_all.group(1))
        elif tests:
            total = len(tests)
            passed = sum(1 for _, s in tests if s == "passed")
            failed = total - passed
        # else: no recognizable summary at all (e.g. a compile failure
        # before any test could run) — leave 0/0/0.

    failed_names = FAILED_TEST_RE.findall(text)

    exit_code = os.environ.get(f"{suite.upper()}_EXIT", "1")
    status = "success" if exit_code == "0" else "failed"
    duration_s = os.environ.get(f"{suite.upper()}_DURATION_S", "0")

    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
        out.write(f"{suite}_status={status}\n")
        out.write(f"{suite}_total={total}\n")
        out.write(f"{suite}_passed={passed}\n")
        out.write(f"{suite}_failed={failed}\n")
        out.write(f"{suite}_duration_s={duration_s}\n")
        # '|'-joined — GITHUB_OUTPUT values are single-line; test names
        # may contain spaces/slashes but never a literal '|'.
        # build_results() below splits this back apart.
        out.write(f"{suite}_failed_names={'|'.join(failed_names)}\n")

    print(f"[{suite}] status={status} total={total} passed={passed} failed={failed}", file=sys.stderr)
    for name in failed_names:
        print(f"  FAIL  {name}", file=sys.stderr)


def _int_env(key: str, default: str = "0") -> int:
    try:
        return int(os.environ.get(key, default))
    except ValueError:
        return 0


def _names_env(key: str) -> list[str]:
    raw = os.environ.get(key, "")
    return [n for n in raw.split("|") if n]


def build_results() -> None:
    suites = []
    all_failed_names: list[str] = []
    overall_total = overall_passed = overall_failed = overall_duration = 0
    statuses = []

    for key, label in SUITES:
        prefix = key.upper()
        total = _int_env(f"{prefix}_TOTAL")
        passed = _int_env(f"{prefix}_PASSED")
        failed = _int_env(f"{prefix}_FAILED")
        duration_s = _int_env(f"{prefix}_DURATION_S")
        status = os.environ.get(f"{prefix}_STATUS", "skipped")
        failed_names = _names_env(f"{prefix}_FAILED_NAMES")

        statuses.append(status)
        overall_total += total
        overall_passed += passed
        overall_failed += failed
        overall_duration += duration_s
        all_failed_names.extend(failed_names)

        suites.append({
            "name": label,
            "package": label,
            "passed": passed,
            "failed": failed,
            "duration_s": duration_s,
            # Zig's raw test output only names FAILING tests explicitly
            # (with a "FAIL (Error)" line) — passing-test names aren't
            # reconstructed here, matching odin-results.json's own
            # suites[].tests[] which likewise only carries what its raw
            # report exposed per test.
            "tests": [
                {"name": n, "status": "failed", "duration_ms": None, "output": ""}
                for n in failed_names
            ],
        })

    if all(s == "success" for s in statuses):
        overall_status = "success"
    elif any(s == "failed" for s in statuses):
        overall_status = "failed"
    else:
        overall_status = "skipped"

    result = {
        "build": os.environ.get("BUILD_NUM", "0"),
        "branch": os.environ.get("BRANCH", "unknown"),
        "commit": os.environ.get("COMMIT", "unknown")[:8],
        "date": os.environ.get("BUILD_DATE", ""),
        "zig_version": os.environ.get("ZIG_VER", "unknown"),
        "package": "mdix-zig",
        "tests": {
            "status": overall_status,
            "total": overall_total,
            "passed": overall_passed,
            "failed": overall_failed,
            "duration_s": overall_duration,
            "failed_names": all_failed_names,
        },
        "suites": suites,
    }

    with open("zig-results.json", "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)
    print(json.dumps(result, indent=2))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="mode", required=True)

    p_log = sub.add_parser("parse-log", help="Parse one suite's raw zig test output log")
    p_log.add_argument("suite", help="Suite key (e.g. 'ffi', 'mdix') — see SUITES above")
    p_log.add_argument("logfile", help="Path to the captured `zig build test-<suite>` output")

    sub.add_parser("build-results", help="Build zig-results.json from both parse-log runs' outputs")

    args = parser.parse_args()
    if args.mode == "parse-log":
        parse_log(args.suite, args.logfile)
    else:
        build_results()


if __name__ == "__main__":
    main()
