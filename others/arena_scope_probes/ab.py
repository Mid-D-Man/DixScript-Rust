#!/usr/bin/env python3
"""Interleaved A/B timing of stage probe builds, so machine drift hits every build equally.

    python3 ab.py ROUNDS label=/path/to/stage_probe [label=/path/to/stage_probe ...] -- FILE.mdix [FILE.mdix ...]

Each round runs every build once, in the order given. Each run reports the best of five
compiles per file. The table prints the median over rounds of the total time and of the
parse stage (the row labelled `config_done`), plus requested bytes and allocation counts,
and the change of each later build against the first.
"""
import re, statistics, subprocess, sys

def unit(s):
    m = re.match(r'([\d.]+)(ns|µs|ms|s)$', s)
    return float(m.group(1)) * {"ns": 1e-6, "µs": 1e-3, "ms": 1.0, "s": 1e3}[m.group(2)]

def run(binary, files):
    out = subprocess.run([binary] + files, capture_output=True, text=True).stdout
    res, cur = {}, None
    for line in out.splitlines():
        m = re.match(r'(\S+\.mdix)\s+total (\S+), (\d+) allocs, (\d+) bytes', line)
        if m:
            cur = m.group(1)
            res[cur] = {"total": unit(m.group(2)), "allocs": int(m.group(3)), "bytes": int(m.group(4))}
            continue
        m = re.match(r'\s+(\w+)\s+(\S+)\s+[\d.]+%\s+(\d+)\s+[\d.]+%\s+(\d+)', line)
        if m and cur:
            res[cur][m.group(1)] = unit(m.group(2))
    return res

def main():
    rounds = int(sys.argv[1])
    sep = sys.argv.index("--")
    builds = [a.split("=", 1) for a in sys.argv[2:sep]]
    files = sys.argv[sep + 1:]
    results = {label: [] for label, _ in builds}
    for _ in range(rounds):
        for label, binary in builds:
            results[label].append(run(binary, files))
    base = builds[0][0]
    for f in files:
        print(f)
        for label, _ in builds:
            med = lambda k: statistics.median(r[f][k] for r in results[label])
            first = results[label][0][f]
            line = f'  {label:12} total {med("total"):9.3f} ms   parse {med("config_done"):8.3f} ms   bytes {first["bytes"]:>14,}   allocs {first["allocs"]:>9,}'
            if label != base:
                bt = statistics.median(r[f]["total"] for r in results[base])
                bp = statistics.median(r[f]["config_done"] for r in results[base])
                line += f'   (total {100 * (med("total") / bt - 1):+.1f}%, parse {100 * (med("config_done") / bp - 1):+.1f}%)'
            print(line)

main()
