#!/usr/bin/env python3
"""
Parse `cargo bench` output → bench-results.json

Input:   bench-raw.txt      (CWD, written by run_benches.py)
Output:  bench-results.json (CWD)

Falls back to scanning target/criterion/**/estimates.json when bench-raw.txt
contains no Criterion output lines (e.g. compile-only run).

Environment variables (injected from CI context):
  BUILD_NUM  BRANCH  COMMIT  BUILD_DATE
"""

import glob
import json
import os
import re
import sys

RE_ANSI = re.compile(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])")


def strip_ansi(text):
    return RE_ANSI.sub("", text)


def to_ns(val_str, unit):
    v = float(val_str.replace(",", ""))
    u = unit.strip().rstrip("/")
    return v * {"ns": 1, "µs": 1e3, "us": 1e3, "ms": 1e6, "s": 1e9}.get(u, 1)


def main():
    raw  = open("bench-raw.txt", encoding="utf-8", errors="replace").read()
    text = strip_ansi(raw)

    benches = []
    seen    = set()

    RE_CRIT = re.compile(
        r"^([\w /\-]+?)\s{2,}time:\s+\[\s*"
        r"([\d.,]+)\s*(ns|µs|us|ms|s)\s+"
        r"([\d.,]+)\s*(ns|µs|us|ms|s)\s+"
        r"([\d.,]+)\s*(ns|µs|us|ms|s)\s*\]",
        re.MULTILINE,
    )

    # run_benches.py inserts "=== <target_name> ===" markers
    sections = re.split(r"^=== (\w+) ===$", text, flags=re.MULTILINE)
    pairs = []
    if len(sections) >= 3:
        i = 1
        while i + 1 < len(sections):
            pairs.append((sections[i], sections[i + 1]))
            i += 2
    else:
        pairs = [("benchmarks", text)]

    for suite_name, body in pairs:
        for m in RE_CRIT.finditer(body):
            name  = m.group(1).strip()
            lower = to_ns(m.group(2), m.group(3))
            mean  = to_ns(m.group(4), m.group(5))
            upper = to_ns(m.group(6), m.group(7))
            key   = f"{suite_name}/{name}"
            if key in seen:
                continue
            seen.add(key)
            parts      = name.rsplit("/", 1)
            group      = parts[0] if len(parts) == 2 else suite_name
            bench_name = parts[-1]
            benches.append({
                "suite":     suite_name,
                "group":     group,
                "name":      bench_name,
                "full_name": f"{suite_name}/{name}",
                "lower_ns":  round(lower, 2),
                "mean_ns":   round(mean,  2),
                "upper_ns":  round(upper, 2),
                "std_ns":    round((upper - lower) / 4, 2),
            })

    # Fallback: scan Criterion JSON artefacts
    if not benches:
        print("No Criterion lines found — falling back to estimates.json scan")
        for est in sorted(glob.glob("target/criterion/**/**/estimates.json", recursive=True)):
            try:
                data = json.load(open(est, encoding="utf-8"))
                parts = est.replace("target/criterion/", "").split("/")
                if len(parts) < 3:
                    continue
                suite_name, bench_name = parts[0], parts[1]
                if bench_name in ("new", "base", "baseline"):
                    continue
                mn  = data.get("mean", {})
                ci  = mn.get("confidence_interval", {})
                sd  = data.get("std_dev", {}).get("point_estimate", 0)
                key = f"{suite_name}/{bench_name}"
                if key in seen:
                    continue
                seen.add(key)
                benches.append({
                    "suite":     suite_name,
                    "group":     suite_name,
                    "name":      bench_name,
                    "full_name": key,
                    "lower_ns":  round(ci.get("lower_bound", mn.get("point_estimate", 0)), 2),
                    "mean_ns":   round(mn.get("point_estimate", 0), 2),
                    "upper_ns":  round(ci.get("upper_bound", mn.get("point_estimate", 0)), 2),
                    "std_ns":    round(sd, 2),
                })
            except Exception as e:
                print(f"Warn: {est}: {e}", file=sys.stderr)

    suite_map = {}
    for b in benches:
        suite_map.setdefault(b["suite"], []).append(b)
    suites = [{"name": k, "benchmarks": v} for k, v in suite_map.items()]

    result = {
        "build":   os.environ.get("BUILD_NUM", "0"),
        "branch":  os.environ.get("BRANCH",    "unknown"),
        "commit":  os.environ.get("COMMIT",    "unknown")[:8],
        "date":    os.environ.get("BUILD_DATE", ""),
        "summary": {"total": len(benches), "suites": len(suites)},
        "suites":  suites,
    }

    with open("bench-results.json", "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)

    print(f"Suites:{len(suites)}  Benchmarks:{len(benches)}")


if __name__ == "__main__":
    main()
