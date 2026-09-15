# dixscript

## Overview

DixScript is a `.mdix` file format and compiler: config, type definitions
(enums), encrypted secrets, executable QuickFuncs, and (as of v1.0.1) raw
binary/text payloads, all in one file. The `dixscript` crate is the core
Rust implementation — lexer, parser, semantic analyzer, value resolver,
and runtime loader — that every language binding (Python, Go, Java, Lua,
Odin, PHP, C#, WASM) and the CLI/LSP tools build on.

This index only covers the parts actually touched so far (documentation
was added mid-project, not at crate creation) — see each part file for
what it actually covers, and treat any file not mentioned in a part file
as not yet documented rather than assumed undocumented-on-purpose.

## Parts

- [Compiler](dixscript/compiler.md) — lexer, parser, semantic analysis,
  version/feature gating, and the `@RAW` section added in v1.0.1
- Benchmarks comparing DixScript against JSON/TOML live in
  `dixscript/benches/format_comparison_benchmark.rs` (synthetic small/
  medium/large fixtures) and `dixscript/benches/chemistry_db_comparison_benchmark.rs`
  (the real `mdix_files/chemistry_db` data, scaled up by duplicating its
  actual element-block structure rather than synthetic flat fields —
  written to answer whether DixScript's parse-time edge over TOML grows
  with file size, using JSON/TOML fixtures generated at bench-setup time
  via `DixConverter::to_json`/`to_toml` rather than hand-transcribed or
  committed as static files)

## CI and Workflows

- `.github/workflows/a-validate.yml` — build (default features, no
  default features, wasm32 target), `cargo test -p dixscript`, and a
  manual-only fuzz smoke test against `parse_mdix`. Clippy runs in the
  same workflow but is informational-only (`continue-on-error: true`) —
  a real, disclosed warning backlog, not yet a gate.
- `.github/workflows/dixscript-bench-publish.yml` — runs the crates under
  `dixscript/benches/`, parses the criterion output, and publishes a
  summary to GitHub Pages. Depends on `scripts/run_benches.py` (the
  target list and runner) and `scripts/parse_bench_results.py` /
  `scripts/generate_summary.py`.
- `.github/workflows/apply-replacements.yml` / `apply-patch.yml` — the
  mdix-scaffold replacement/patch mechanism this repo uses to receive
  file changes; see `Mid-D-Man/mdix-scaffold`'s own skill doc for how
  these work, not documented again here.
