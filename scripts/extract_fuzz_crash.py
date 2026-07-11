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

Once a real crash IS found, cargo-fuzz's minimizer re-runs it dozens to
hundreds of times while shrinking the input, printing a full panic/backtrace
block on every single attempt. Those blocks are never suppressed as noise
(that would risk hiding a real report) — instead they're grouped and
deduplicated: each distinct crash is shown once, with a repeat count,
instead of being printed in full every time it recurs.

Usage:
  python3 scripts/extract_fuzz_crash.py <raw_log_file> [output_file]
  cargo fuzz run parse_mdix -- -max_total_time=60 2>&1 | python3 scripts/extract_fuzz_crash.py

Input:  raw_log_file (arg 1) or stdin — either a local `cargo fuzz run`
        capture or a downloaded GitHub Actions step log.
Output:
  - Full deduplicated report always written to output_file (arg 2, default
    "fuzz-extracted.txt") — never size-capped, safe to upload as an artifact.
  - A capped version printed to stdout (large console output has its own
    problems — see the GH truncation notice this script watches for).
  - A capped markdown version appended to $GITHUB_STEP_SUMMARY in CI
    (GitHub hard-caps step summaries at 1024KB; this stays safely under
    that regardless of how large the deduplicated report is).
"""

import hashlib
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
CRASH_MARKER_RE = re.compile(
    r"panicked at|deadly signal|ERROR: libFuzzer|SUMMARY:|out-of-memory|"
    r"timeout after|error: process didn't exit successfully"
)

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

# Volatile bits that differ between otherwise-identical repeats of the same
# crash: memory addresses, artifact filenames (a fresh hash every attempt),
# thread/process ids, timestamps. Stripped before hashing a block so repeats
# of the *same* crash dedupe even though these details differ each time.
VOLATILE_RE = re.compile(
    r"0x[0-9a-f]{4,}"                       # addresses
    r"|crash-[0-9a-f]{16,}"                 # cargo-fuzz artifact filenames
    r"|\b[0-9a-f]{32,}\b"                   # bare long hashes
    r"|==\d+=="                             # pid-tagged sanitizer markers
    r"|\d{2}:\d{2}:\d{2}\.\d+"              # timestamps
)

STDOUT_CHAR_CAP = 60_000
SUMMARY_CHAR_CAP = 800_000  # GitHub hard-caps at 1024KB (1,048,576B); stay well clear


def extract(text):
    """First pass: split into (index, line) pairs of everything that isn't
    expected DixLoader parse-reject noise."""
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

    real_crash = any(CRASH_MARKER_RE.search(l) for _, l in kept)
    return kept, noise_count, truncated_by_github, real_crash


def group_into_blocks(kept):
    """Second pass: group kept lines that are contiguous in the original
    file (no noise line removed between them) into single blocks — a panic
    + its backtrace prints as one uninterrupted run, so this reliably keeps
    each crash report together as one unit for deduplication."""
    blocks = []
    current = []
    prev_index = None

    for i, line in kept:
        if prev_index is not None and i != prev_index + 1:
            blocks.append(current)
            current = []
        current.append((i, line))
        prev_index = i

    if current:
        blocks.append(current)

    return blocks


def dedup_blocks(blocks):
    """Third pass: collapse blocks that are the same crash recurring (e.g.
    libFuzzer's minimizer re-triggering the same panic on every shrink
    attempt) down to one representative occurrence + a repeat count."""
    seen = {}
    ordered = []

    for block in blocks:
        raw = "\n".join(line for _, line in block)
        key = VOLATILE_RE.sub("#", raw)
        digest = hashlib.sha1(key.encode("utf-8", errors="replace")).hexdigest()

        if digest in seen:
            seen[digest]["count"] += 1
        else:
            entry = {"block": block, "count": 1}
            seen[digest] = entry
            ordered.append(entry)

    return ordered


def render_blocks(entries, char_cap=None):
    """Render deduplicated blocks as plain text, each preceded by its
    original line-number range and, if it recurred, a repeat count. Stops
    once char_cap is hit (if given) and notes how much was left out —
    everything is still in the uncapped output file."""
    out = []
    omitted_blocks = 0
    total_repeats_omitted = 0

    for idx, entry in enumerate(entries):
        block = entry["block"]
        count = entry["count"]
        piece_lines = [f"{i:>6}: {line}" for i, line in block]
        if count > 1:
            piece_lines.append(
                f"        ... this exact block repeated {count} times total "
                f"(deduplicated — see full output for every instance)"
            )
        piece = "\n".join(piece_lines)

        if char_cap is not None and sum(len(p) for p in out) + len(piece) > char_cap:
            omitted_blocks = len(entries) - idx
            total_repeats_omitted = sum(e["count"] for e in entries[idx:])
            break

        out.append(piece)

    text = "\n\n".join(out)
    if omitted_blocks:
        text += (
            f"\n\n... {omitted_blocks} more distinct block(s) "
            f"({total_repeats_omitted} occurrence(s) total) omitted here for size — "
            f"see the full uncapped report in the uploaded artifact."
        )
    return text


def render_report(entries, noise_count, truncated_by_github, real_crash, char_cap=None):
    total_occurrences = sum(e["count"] for e in entries)
    out = [f"Suppressed {noise_count} expected parser/lexer noise lines."]
    if len(entries) != total_occurrences:
        out.append(
            f"Deduplicated {total_occurrences} signal blocks down to "
            f"{len(entries)} distinct one(s) — repeats are almost always the "
            f"same crash re-triggered during cargo-fuzz's input minimization."
        )
    if truncated_by_github:
        out.append(
            "NOTE: GitHub's inline log viewer truncated this step for size before "
            "reaching the end — this extraction only covers what was captured up "
            "to that point. Download the raw log for the full picture if the "
            "verdict below looks incomplete."
        )
    out.append("")

    if not entries:
        out.append("No non-noise lines found — the whole log was expected parse-error chatter.")
        return "\n".join(out)

    out.append("Distinct signal blocks (in first-seen order):")
    out.append("")
    out.append(render_blocks(entries, char_cap))
    out.append("")

    if real_crash:
        out.append("Verdict: looks like a genuine libFuzzer crash/timeout/OOM/panic report above.")
        out.append("         Reproduce locally with the artifact path libFuzzer printed:")
        out.append("           cargo fuzz run parse_mdix <path-to-artifact>")
    else:
        out.append("Verdict: no panic/crash/timeout/OOM markers found in the non-noise lines.")
        out.append("         If the job still failed, check exit code / step timeout / log size")
        out.append("         truncation rather than assuming a parser Err is the cause.")

    return "\n".join(out)


def render_summary_markdown(entries, noise_count, truncated_by_github, real_crash, char_cap):
    total_occurrences = sum(e["count"] for e in entries)
    icon = "🐛" if real_crash else "❓"
    lines = [
        f"## {icon} Fuzz failure — extracted signal",
        "",
        f"Suppressed **{noise_count}** expected parser/lexer noise lines.",
    ]
    if len(entries) != total_occurrences:
        lines.append(
            f"Deduplicated **{total_occurrences}** signal blocks down to "
            f"**{len(entries)}** distinct one(s) — repeats are almost always "
            f"the same crash re-triggered during cargo-fuzz's input minimization."
        )
    lines.append("")
    if truncated_by_github:
        lines += [
            "> ⚠️ GitHub's inline log viewer truncated this step before reaching the "
            "end — download the raw log for the full picture if this looks incomplete.",
            "",
        ]

    if not entries:
        lines.append("No non-noise lines found — the whole log was expected parse-error chatter.")
        return "\n".join(lines) + "\n"

    lines += ["```text", render_blocks(entries, char_cap), "```", ""]

    if real_crash:
        lines += [
            "**Verdict:** genuine libFuzzer crash/timeout/OOM/panic report above. "
            "Reproduce locally with `cargo fuzz run parse_mdix <path-to-artifact>` "
            "using the artifact path libFuzzer printed. Full, non-deduplicated, "
            "non-truncated output is in the uploaded `fuzz-extracted.txt` artifact.",
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

    output_file = sys.argv[2] if len(sys.argv) > 2 else "fuzz-extracted.txt"

    kept, noise_count, truncated_by_github, real_crash = extract(text)
    blocks = group_into_blocks(kept)
    entries = dedup_blocks(blocks)

    # Full, uncapped report -> always written to disk. This is the one to
    # trust completely; stdout/summary below are deliberately capped copies.
    full_report = render_report(entries, noise_count, truncated_by_github, real_crash)
    with open(output_file, "w", encoding="utf-8") as fh:
        fh.write(full_report + "\n")
    print(f"Full extracted report written -> {output_file} ({len(full_report)} chars)")

    console_report = render_report(
        entries, noise_count, truncated_by_github, real_crash, char_cap=STDOUT_CHAR_CAP
    )
    print(console_report)

    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary_path:
        markdown = render_summary_markdown(
            entries, noise_count, truncated_by_github, real_crash, char_cap=SUMMARY_CHAR_CAP
        )
        with open(summary_path, "a", encoding="utf-8") as fh:
            fh.write(markdown)
        print(f"\nStep summary appended -> {summary_path} ({len(markdown)} chars)")


if __name__ == "__main__":
    main()
