#!/usr/bin/env python3
"""
Write GitHub Actions step summary markdown from CI JSON results.

Usage:
  python3 scripts/generate_summary.py <mode> <json_file>

  mode      : tests | bench | cli
  json_file : path to the relevant results JSON

Appends to $GITHUB_STEP_SUMMARY (renders in the Actions UI → Summary tab).
Falls back to stdout when the env var is not set (useful for local testing).
"""

import json
import os
import sys


# ── Formatting helpers ────────────────────────────────────────────────────────

def fmt_ns(ns):
    if not ns:
        return "—"
    if ns >= 1e9:
        return f"{ns / 1e9:.3f} s"
    if ns >= 1e6:
        return f"{ns / 1e6:.3f} ms"
    if ns >= 1e3:
        return f"{ns / 1e3:.3f} µs"
    return f"{ns:.1f} ns"


def fmt_dur(sec):
    if sec < 1:
        return f"{sec * 1000:.0f}ms"
    if sec < 60:
        return f"{sec:.2f}s"
    return f"{int(sec / 60)}m {int(sec % 60)}s"


# ── Test summary ──────────────────────────────────────────────────────────────

def tests_summary(data):
    s      = data.get("summary", {})
    build  = data.get("build",  "?")
    branch = data.get("branch", "?")
    commit = data.get("commit", "?")
    passed  = s.get("passed",  0)
    failed  = s.get("failed",  0)
    ignored = s.get("ignored", 0)
    total   = s.get("total",   0)
    dur     = s.get("duration_s", 0)
    suites  = data.get("suites", [])

    icon = "✅" if failed == 0 else "❌"
    lines = [
        f"## {icon} Test Results — Build #{build}",
        f"**Branch:** `{branch}` &nbsp;·&nbsp; **Commit:** `{commit}`",
        "",
        "| Metric | |",
        "|--------|--|",
        f"| ✅ Passed   | **{passed}** |",
        f"| ❌ Failed   | **{failed}** |",
        f"| ⊘ Ignored  | {ignored} |",
        f"| 📊 Total    | {total} |",
        f"| ⏱ Duration | {fmt_dur(dur)} |",
        f"| 🗂 Suites   | {len(suites)} |",
        "",
    ]

    # Failed tests at the top
    failed_tests = [
        (suite["name"], t)
        for suite in suites
        for t in suite.get("tests", [])
        if t.get("status") == "failed"
    ]
    if failed_tests:
        lines += [
            "### ❌ Failed Tests",
            "",
            "| Suite | Test | Output |",
            "|-------|------|--------|",
        ]
        for sname, t in failed_tests:
            snippet = (t.get("output") or "").strip().replace("\n", " ")[:140] or "*(no output)*"
            lines.append(f"| `{sname}` | `{t['name']}` | {snippet} |")
        lines.append("")

    # Suite table
    lines += [
        "### 🗂 Suite Breakdown",
        "",
        "| Suite | ✅ | ❌ | ⊘ | ⏱ |",
        "|-------|----|----|----|---|",
    ]
    for suite in suites:
        p    = suite.get("passed",  0)
        f    = suite.get("failed",  0)
        i    = suite.get("ignored", 0)
        d    = suite.get("duration_s", 0)
        flag = "❌" if f > 0 else "✅"
        lines.append(f"| {flag} `{suite['name']}` | {p} | {f} | {i} | {fmt_dur(d)} |")
    lines.append("")
    return "\n".join(lines)


# ── Bench summary ─────────────────────────────────────────────────────────────

def bench_summary(data):
    s      = data.get("summary", {})
    build  = data.get("build",  "?")
    branch = data.get("branch", "?")
    commit = data.get("commit", "?")
    total  = s.get("total",  0)
    suites = data.get("suites", [])

    all_benches = [
        (suite["name"], b)
        for suite in suites
        for b in suite.get("benchmarks", [])
    ]

    lines = [
        f"## 📊 Benchmark Results — Build #{build}",
        f"**Branch:** `{branch}` &nbsp;·&nbsp; **Commit:** `{commit}`",
        "",
        "| Metric | |",
        "|--------|--|",
        f"| 📐 Benchmarks | **{total}** |",
        f"| 🗂 Suites     | {len(suites)} |",
        "",
    ]

    if all_benches:
        by_mean = sorted(all_benches, key=lambda x: x[1].get("mean_ns", 0))
        fn, fb  = by_mean[0]
        sn, sb  = by_mean[-1]
        lines += [
            "| | Benchmark | Suite | Time |",
            "|--|-----------|-------|------|",
            f"| ⚡ Fastest | `{fb['name']}` | `{fn}` | {fmt_ns(fb['mean_ns'])} |",
            f"| 🐢 Slowest | `{sb['name']}` | `{sn}` | {fmt_ns(sb['mean_ns'])} |",
            "",
        ]

    # One collapsible block per suite so the summary stays scannable
    for suite in suites:
        benches = sorted(suite.get("benchmarks", []), key=lambda b: b.get("mean_ns", 0))
        if not benches:
            continue
        max_ns = max(b.get("mean_ns", 0) for b in benches) or 1
        min_ns = min(b.get("mean_ns", 0) for b in benches)

        lines += [
            f"<details><summary>🔬 <strong>{suite['name']}</strong>"
            f" &nbsp;—&nbsp; {len(benches)} benchmark(s)</summary>",
            "",
            "| | Benchmark | Mean | Range | Relative |",
            "|--|-----------|------|-------|----------|",
        ]
        for b in benches:
            mean = b.get("mean_ns", 0)
            lo   = b.get("lower_ns", 0)
            hi   = b.get("upper_ns", 0)
            pct  = (mean / max_ns * 100) if max_ns > 0 else 0
            icon = "⚡" if mean == min_ns else ("🐢" if mean == max_ns else "")
            lines.append(
                f"| {icon} | `{b['name']}` | {fmt_ns(mean)} "
                f"| {fmt_ns(lo)} … {fmt_ns(hi)} | {pct:.0f}% |"
            )
        lines += ["", "</details>", ""]

    return "\n".join(lines)


# ── CLI test summary ──────────────────────────────────────────────────────────

def cli_summary(data):
    s      = data.get("summary", {})
    build  = data.get("build",  "?")
    branch = data.get("branch", "?")
    commit = data.get("commit", "?")
    passed = s.get("passed", 0)
    failed = s.get("failed", 0)
    total  = s.get("total",  0)
    suites = data.get("suites", [])

    icon = "✅" if failed == 0 else "❌"
    lines = [
        f"## {icon} CLI Test Results — Build #{build}",
        f"**Branch:** `{branch}` &nbsp;·&nbsp; **Commit:** `{commit}`",
        "",
        "| Metric | |",
        "|--------|--|",
        f"| ✅ Passed | **{passed}** |",
        f"| ❌ Failed | **{failed}** |",
        f"| 📊 Total  | {total} |",
        f"| 🗂 Suites | {len(suites)} |",
        "",
    ]

    failed_cmds = [
        (suite["name"], cmd)
        for suite in suites
        for cmd in suite.get("commands", [])
        if cmd.get("status") == "failed"
    ]
    if failed_cmds:
        lines += [
            "### ❌ Failed Commands",
            "",
            "| Suite | Command | Exit |",
            "|-------|---------|------|",
        ]
        for sname, cmd in failed_cmds:
            lines.append(f"| `{sname}` | `{cmd['name']}` | `{cmd.get('exit_code', '?')}` |")
        lines.append("")

    lines += [
        "### 🗂 Suite Breakdown",
        "",
        "| Suite | ✅ | ❌ |",
        "|-------|----|---|",
    ]
    for suite in suites:
        p    = suite.get("passed", 0)
        f    = suite.get("failed", 0)
        flag = "❌" if f > 0 else "✅"
        lines.append(f"| {flag} `{suite['name']}` | {p} | {f} |")
    lines.append("")
    return "\n".join(lines)


# ── Main ──────────────────────────────────────────────────────────────────────

GENERATORS = {"tests": tests_summary, "bench": bench_summary, "cli": cli_summary}


def main():
    if len(sys.argv) < 3:
        print("Usage: generate_summary.py <tests|bench|cli> <json_file>", file=sys.stderr)
        sys.exit(1)

    mode, json_path = sys.argv[1].lower(), sys.argv[2]

    if not os.path.exists(json_path):
        print(f"WARNING: {json_path} not found — skipping summary generation", file=sys.stderr)
        return

    if mode not in GENERATORS:
        print(f"Unknown mode '{mode}'. Valid: {', '.join(GENERATORS)}", file=sys.stderr)
        sys.exit(1)

    with open(json_path, encoding="utf-8") as fh:
        data = json.load(fh)

    content = GENERATORS[mode](data)

    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary_path:
        with open(summary_path, "a", encoding="utf-8") as fh:
            fh.write(content + "\n")
        print(f"Step summary appended → {summary_path}")
    else:
        print(content)


if __name__ == "__main__":
    main()
