#!/usr/bin/env python3
"""Writes scaled copies of the chemistry database next to it, for scaling measurements.

    python3 make_scaled.py DIR_WITH_elements_database.mdix [MULTIPLIER ...]

Each `.scaled_xN.mdix` holds N back-to-back copies of the five-element block (hydrogen to
boron) with the element keys renamed so they do not collide, under the same header and
`@IMPORTS` as the real file. This is the same construction as `build_scaled_source` in
`dixscript/benches/chemistry_db_comparison_benchmark.rs`. To measure without debug output,
copy the whole `mdix_files` tree first and rewrite every `debug_mode` to "off".
"""
import sys

KEYS = ["hydrogen", "helium", "lithium", "beryllium", "boron"]

def main():
    directory = sys.argv[1]
    sizes = [int(a) for a in sys.argv[2:]] or [1, 2, 4, 8, 16, 32]
    src = open(f"{directory}/elements_database.mdix").read()
    start = src.index("elements.hydrogen.")
    end = src.index("elements.carbon.", start)
    header, block = src[:start], src[start:end]
    for n in sizes:
        parts = [header]
        for b in range(n):
            blk = block
            if b > 0:
                for k in KEYS:
                    blk = blk.replace(f"elements.{k}", f"elements.{k}_b{b}")
            parts.append(blk + "\n")
        parts.append(")")
        open(f"{directory}/.scaled_x{n}.mdix", "w").write("".join(parts))
        print(f"wrote {directory}/.scaled_x{n}.mdix")

main()
