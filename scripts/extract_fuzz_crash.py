#!/usr/bin/env python3
"""
Extract the real signal from a cargo-fuzz / DixLoader log, suppressing the
expected per-iteration Lexer/Parser noise.

Every `load_from_str` call emits an "Info" line plus one structured error
block per Lexer/Parser/AstEnhancement/ValueResolution failure — that's
*expected*, not a crash (see dixscript/fuzz/fuzz_targets/parse_mdix.rs: the
harness only flags an actual panic). Under libFuzzer that noise repeats
thousands of times and buries the one thing you actually want to know: did
libFuzzer report a crash, a timeout, an OOM, or a Rust panic — and if so,
where.

Usage:
  python3 scripts/extract_fuzz_crash.py <raw_log_file>
  cargo fuzz run parse_mdix -- -max_total_time=60 2>&1 | python3 scripts/extract_fuzz_crash.py

Input:  raw_log_file (arg) or stdin — either a local `cargo fuzz run`
        capture or a downloaded GitHub Actions step log.
Output: always printed to stdout. Additionally appended to
        $GITHUB_STEP_SUMMARY when running in CI (falls back to stdout-only
        for local use, same convention as generate_summary.py).
"""

import os
import re
import sys

RE_ANSI = re.compile(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])")


def strip_ansi(text):
    return RE_ANSI.sub("", text)


# A DixLoader structured error dump always starts with one of these two
# header shapes and ends at the next blank line:
#   [Error] [Lexer/Parser/...] [CODE] Fatal: ... at line N, column N
#   Error: 0:HH:MM:SS.mmm] [Error] [Lexer/Parser/...] ...   (GH Actions'
#     truncated-timestamp rendering of the same line when it gets picked up
#     as an annotation)
# Everything between that header and the next blank line — Message/Source/
# the raw garbage input line/the caret/Suggestion/Quick Fixes bullets — is
# part of that one block and equally expected noise, so it's suppressed by
# position, not by pattern.
BLOCK_START_RE = re.compile(
    r"\[Error\]\s*\[(Lexer|Parser|AstEnhancement|ValueResolution|Semantic|"
    r"Imports|DLM|BinarySerialization|Config|General)\]"
)
INFO_LINE_RE = re.compile(r"^\s*\[Info\]")
BLANK_RE = re.compile(r"^\s*$")

# Lines that indicate the actual thing you're looking for. Checked first —
# a signal line is never suppressed even mid-block.
SIGNAL_PATTERNS = [
    r"panicked at",
    r"RUST_BACKTRACE",
    r"stack backtrace:",
    r"^\s*\d+:\s+0x[0-9a-f]+",   # backtrace frame lines
    r"==\d+==\s*ERROR",         # ASan/libFuzzer sanitizer report
    r"ERROR: libFuzzer",
    r"SUMMARY:\s*(AddressSanitizer|libFuzzer)",
    r"Test unit written to",
    r"artifact_prefix",
    r"deadly signal",
    r"out-of-memory",
    r"timeout after",
    r"^==\d+==",
    r"^cargo-fuzz:",
    r"^thread '.*' panicked",
    r"error: process didn't exit successfully",
]
SIGNAL_RE = re.compile("|".join(f"(?:{p})" for p in SIGNAL_PATTERNS))

OTHER_NOISE_PATTERNS = [
    r"Running AST enhancement",
    r"Starting AST enhancement",
    r"AST enhancement complete",
    r"Skipping value resolution",
    r"String source loaded successfully",
    r"Loading from string source",
]
OTHER_NOISE_RE = re.compile("|".join(f"(?:{p})" for p in OTHER_NOISE_PATTERNS))

# GitHub's own inline-viewer truncation notice — not part of the fuzz output
# at all, but worth flagging explicitly rather than silently dropping, since
# it's evidence in its own right (the step produced more log than GH will
# even display).
GH_TRUNCATION_RE = re.compile(r"This step has been truncated due to its large size")


def extract(text):
    lines = strip_ansi(text).splitlines()

    kept = []
    noise_count = 0
    truncated_by_github = False
    in_error_block = False

    for i, line in enumerate(lines):
        if GH_TRUNCATION_RE.search(line):
            truncated_by_github = True
            continue

        if SIGNAL_RE.search(line):
            kept.append((i, line))
            in_error_block = False
            continue

        if in_error_block:
            noise_count += 1
            if BLANK_RE.match(line):
                in_error_block = False
            continue

        if BLOCK_START_RE.search(line):
            in_error_block = True
            noise_count += 1
            continue

        if INFO_LINE_RE.match(line) or OTHER_NOISE_RE.search(line) or BLANK_RE.match(line):
            noise_count += 1
            continue

        kept.append((i, line))

    real_crash = any(
        re.search(
            r"panicked at|deadly signal|ERROR: libFuzzer|SUMMARY:|out-of-memory|"
            r"timeout after|error: process didn't exit successfully",
            l,
        )
        for _, l in kept
    )

    return kept, noise_count, truncated_by_github, real_crash


def render_report(kept, noise_count, truncated_by_github, real_crash):
    out = [f"Suppressed {noise_count} expected parser/lexer noise lines."]
    if truncated_by_github:
        out.append(
            "NOTE: GitHub's inline log viewer truncated this step for size before "
            "reaching the end — this extraction only covers what was captured up "
            "to that point. Download the raw log for the full picture if the "
            "verdict below looks incomplete."
        )
    out.append("")

    if not kept:
        out.append("No non-noise lines found — the whole log was expected parse-error chatter.")
        return "\n".join(out), real_crash

    out.append("Remaining lines (in original order):")
    out.append("")
    for i, line in kept:
        out.append(f"{i:>6}: {line}")
    out.append("")

    if real_crash:
        out.append("Verdict: looks like a genuine libFuzzer crash/timeout/OOM/panic report above.")
        out.append("         Reproduce locally with the artifact path libFuzzer printed:")
        out.append("           cargo fuzz run parse_mdix <path-to-artifact>")
    else:
        out.append("Verdict: no panic/crash/timeout/OOM markers found in the non-noise lines.")
        out.append("         If the job still failed, check exit code / step timeout / log size")
        out.append("         truncation rather than assuming a parser Err is the cause.")

    return "\n".join(out), real_crash


def render_summary_markdown(kept, noise_count, truncated_by_github, real_crash):
    icon = "🐛" if real_crash else "❓"
    lines = [
        f"## {icon} Fuzz failure — extracted signal",
        "",
        f"Suppressed **{noise_count}** expected parser/lexer noise lines.",
        "",
    ]
    if truncated_by_github:
        lines += [
            "> ⚠️ GitHub's inline log viewer truncated this step before reaching the "
            "end — download the raw log for the full picture if this looks incomplete.",
            "",
        ]

    if not kept:
        lines.append("No non-noise lines found — the whole log was expected parse-error chatter.")
        return "\n".join(lines) + "\n"

    lines += ["```text"]
    for i, line in kept:
        lines.append(f"{i:>6}: {line}")
    lines += ["```", ""]

    if real_crash:
        lines += [
            "**Verdict:** genuine libFuzzer crash/timeout/OOM/panic report above. "
            "Reproduce locally with `cargo fuzz run parse_mdix <path-to-artifact>` "
            "using the artifact path libFuzzer printed.",
        ]
    else:
        lines += [
            "**Verdict:** no panic/crash/timeout/OOM markers found. If the job still "
            "failed, check exit code / step timeout / log size truncation rather than "
            "assuming a parser `Err` is the cause.",
        ]

    return "\n".join(lines) + "\n"


def main():
    if len(sys.argv) > 1:
        with open(sys.argv[1], encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    else:
        text = sys.stdin.read()

    kept, noise_count, truncated_by_github, real_crash = extract(text)

    report, _ = render_report(kept, noise_count, truncated_by_github, real_crash)
    print(report)

    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary_path:
        with open(summary_path, "a", encoding="utf-8") as fh:
            fh.write(render_summary_markdown(kept, noise_count, truncated_by_github, real_crash))
        print(f"\nStep summary appended -> {summary_path}")


if __name__ == "__main__":
    main()
