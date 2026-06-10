#!/usr/bin/env python3
"""
Run DixScript Criterion benchmarks — selected target or all.

Usage:
  python3 scripts/run_benches.py [target]

  target : exact bench name, prefix, or 'all' (default). Case-insensitive.
           Partial prefix match is supported for convenience.

Output:
  bench-raw.txt written to CWD.

Exit codes:
  0  all requested targets ran (individual failures logged, non-fatal)
  1  unknown target name
"""

import subprocess
import sys

BENCH_TARGETS = [
    "format_comparison_benchmark",
    "throughput_benchmark",
    "lexer_throughput",
    "config_throughput",
    "general_parser_benchmark",
    "semantics_benchmark",
    "ast_enhancement_benchmark",
    "value_resolution_benchmark",
    "binary_serialization_benchmark",
    "runtime_benchmark",
    "stress_test_benchmark",
]

SEP = "─" * 60


def resolve_target(arg):
    """Return the list of bench targets matching *arg*."""
    arg = arg.strip().lower()
    if not arg or arg == "all":
        return list(BENCH_TARGETS)
    exact = [t for t in BENCH_TARGETS if t.lower() == arg]
    if exact:
        return exact
    prefix = [t for t in BENCH_TARGETS if t.lower().startswith(arg)]
    if prefix:
        return prefix
    print(
        f"ERROR: unknown bench target '{arg}'.\n"
        f"Valid targets:\n  all\n  " + "\n  ".join(BENCH_TARGETS),
        file=sys.stderr,
    )
    sys.exit(1)


def run_bench(target, fh):
    """Run a single bench target, tee output to *fh*. Returns True on success."""
    header = f"=== {target} ==="
    print(header, flush=True)
    fh.write(header + "\n")
    fh.flush()

    proc = subprocess.run(
        ["cargo", "bench", "--bench", target, "--color=never"],
        capture_output=True,
        text=True,
    )
    output = proc.stdout + (proc.stderr or "")
    fh.write(output)
    print(output, end="", flush=True)
    return proc.returncode == 0


def main():
    arg = (sys.argv[1] if len(sys.argv) > 1 else "all") or "all"
    targets = resolve_target(arg)

    print(f"\n{SEP}")
    print(f"DixScript Bench Runner — {len(targets)} / {len(BENCH_TARGETS)} target(s)")
    if len(targets) < len(BENCH_TARGETS):
        print(f"  Selected: {', '.join(targets)}")
    print(f"{SEP}\n", flush=True)

    statuses = {}
    with open("bench-raw.txt", "w", encoding="utf-8") as f:
        for t in targets:
            statuses[t] = run_bench(t, f)

    ok  = sum(1 for v in statuses.values() if v)
    bad = sum(1 for v in statuses.values() if not v)

    print(f"\n{SEP}")
    print(f"Complete: {ok} OK, {bad} failed")
    for t, passed in statuses.items():
        print(f"  {'✓' if passed else '✗'}  {t}")
    print(SEP)
    print("Output → bench-raw.txt")


if __name__ == "__main__":
    main()
