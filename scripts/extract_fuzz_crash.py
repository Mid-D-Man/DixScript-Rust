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


# A DixLoader structured diagnostic block always starts with one of these
# header shapes:
#   [Error/Warning] [Lexer/Parser/...] [CODE] Fatal/Warning: ... at line N
#   Error: 0:HH:MM:SS.mmm] [Error] [Lexer/Parser/...] ...   (GH Actions'
#     truncated-timestamp rendering of the same line when it gets picked up
#     as an annotation)
# Both severities use the identical multi-line schema (Message/Section/
# Source/Location/Suggestion/Quick Fixes + exactly two "  - " bullets), so
# one detector covers both — a `[Warning] [Semantic] ... DuplicateDefinition`
# block is exactly as expected/benign as a `[Error] [Parser] ... Fatal`
# one, just a different severity DixLoader chose to log it at.
BLOCK_START_RE = re.compile(
    r"\[(Error|Warning)\]\s*\[(Lexer|Parser|AstEnhancement|ValueResolution|Semantic|"
    r"Imports|DLM|BinarySerialization|Config|General)\]"
)
# Every block schema seen ends with exactly two "  - reason" bullets under
# "Quick Fixes:". Terminating on the 2nd bullet (rather than waiting for a
# blank line) is what makes this robust to fuzzer-generated "source" content
# that happens to itself be blank/whitespace-only — which would otherwise
# end suppression several lines too early and leak the block's tail
# (caret/Suggestion/Quick Fixes/bullets) through as false signal.
BULLET_RE = re.compile(r"^\s*-\s+\S")
MAX_BLOCK_LINES = 20  # safety valve if a block's shape ever doesn't match
INFO_LINE_RE = re.compile(r"^\s*\[Info\]")
BLANK_RE = re.compile(r"^\s*$")

# A DixLoader summary/halt line: "[Error] DATA section parsing halted due to
# errors" and similar — a single line, no [Category] sub-tag, no follow-up
# block. Just restates that errors already reported above happened; not new
# information, not a crash.
STANDALONE_ERROR_RE = re.compile(r"\[Error\]\s+(?!\[)\S")

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
    r"ERROR: libFuzzer: out-of-memory",
    r"ERROR: libFuzzer: timeout after",
    r"^==\d+==",
    r"^cargo-fuzz:",
    r"^thread '.*' panicked",
    r"error: process didn't exit successfully",
]
SIGNAL_RE = re.compile("|".join(f"(?:{p})" for p in SIGNAL_PATTERNS))
CRASH_MARKER_RE = re.compile(
    r"panicked at|deadly signal|ERROR: libFuzzer|SUMMARY:\s*(AddressSanitizer|libFuzzer)|"
    r"error: process didn't exit successfully"
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

# libFuzzer's own periodic progress telemetry, printed continuously through
# any run — crash or not (#execs REDUCE/NEW/pulse cov: ... ft: ... corp: ...
# exec/s: ... rss: ...). Normal fuzzer housekeeping, never a crash signal.
LIBFUZZER_STATS_RE = re.compile(r"^#\d+\s+\S+\s+cov:\s*\d+\s+ft:")

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
    block_line_count = 0
    bullets_seen = 0
    seen_source_label = False
    raw_dump_consumed = False

    for i, line in enumerate(lines):
        if GH_TRUNCATION_RE.search(line):
            truncated_by_github = True
            continue

        # Block-suppression state is checked BEFORE signal detection, not
        # after. This matters: a DixLoader diagnostic block's "Source:"
        # section echoes the raw fuzz-generated input verbatim, and
        # libFuzzer's dictionary mutator (the "DE:" entries in its own
        # progress lines) literally lifts string constants out of the
        # compiled binary — including error-message text like "...timeout
        # after {}s..." from cloud_storage_provider.rs — and splices
        # fragments of them into future inputs. That garbage can and does
        # coincidentally contain substrings like "timeout after" or
        # something matching the backtrace-frame pattern. If SIGNAL_RE were
        # checked against that echoed content, it would (and did) produce
        # false "genuine crash" verdicts. So: once we're inside a
        # recognized block, we trust nothing in it — not even a line that
        # looks like a crash signal — until the block's own (DixLoader-
        # authored, trustworthy) structure says it's over.
        if in_error_block:
            block_line_count += 1
            noise_count += 1

            if seen_source_label and not raw_dump_consumed:
                # This is the ONE line immediately after "Source:" — the
                # raw fuzz-input dump. Blindly consumed, no pattern checks
                # of any kind: it is Byzantine, fuzzer-controlled content
                # and cannot be trusted to mean anything it looks like it
                # means, including "blank" (fuzzers explore empty/
                # whitespace-only inputs too) or "this looks like a panic".
                raw_dump_consumed = True
                continue

            if line.strip() == "Source:":
                seen_source_label = True
                continue

            if BULLET_RE.match(line):
                bullets_seen += 1
                if bullets_seen >= 2:
                    in_error_block = False
                continue

            if (not seen_source_label or raw_dump_consumed) and BLANK_RE.match(line):
                # Safe to trust a blank line as "block over" here: either
                # we never had a Source: section at all (the short
                # header+Message-only block shape), or we're past the one
                # untrusted raw-dump line and back to DixLoader-authored
                # text (caret/Suggestion/Quick Fixes).
                in_error_block = False
                continue

            if block_line_count < MAX_BLOCK_LINES:
                continue

            # Safety valve: shape didn't resolve within a sane number of
            # lines. Stop suppressing and let this same line be classified
            # normally below instead of risking an unbounded swallow.
            in_error_block = False

        if SIGNAL_RE.search(line):
            kept.append((i, line))
            continue

        if BLOCK_START_RE.search(line):
            in_error_block = True
            block_line_count = 0
            bullets_seen = 0
            seen_source_label = False
            raw_dump_consumed = False
            noise_count += 1
            continue

        if (
            INFO_LINE_RE.match(line)
            or OTHER_NOISE_RE.search(line)
            or LIBFUZZER_STATS_RE.match(line)
            or STANDALONE_ERROR_RE.search(line)
            or BLANK_RE.match(line)
        ):
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
