# dixscript: compiler

Lexer, parser, semantic analysis, version/feature gating, and the pipeline
entry point that drives them (`Runtime/loader.rs` — Runtime-layer, but
included here since it is the thing that actually calls tokenize, parse,
and analyze in sequence).

## Modules

### `Compiler/Core/Tokenizer/token.rs`

**What it does:** `SectionId` and `TokenType` — every token variant the
lexer can produce, including the section keywords (`@CONFIG`, `@DATA`,
etc.) and now `@RAW`.

**Decisions:**
- `TokenType::SectionRaw` and `TokenType::RawContent { tag, start, end }`
  were added following the exact existing pattern for every other section
  keyword: an enum variant, an `as_str`/`from_context_str` pair, and a
  `get_section_context` entry. `RawContent` is a new kind of token: instead
  of holding parsed text, it holds byte offsets into the tokenizer's own
  input, so the payload is never copied or UTF-8-validated at lex time.

### `Compiler/Core/Tokenizer/lexer.rs`

**What it does:** Scans a source buffer into a `Vec<Token>`.

**Decisions:**
- `Tokenizer`'s input is `&[u8]`, not `&str`. This looks like a bigger
  change than it is: the scanning code already worked byte-wise
  internally (`self.input.as_bytes()` everywhere, and the `peek`/`advance`
  helpers already did a raw `byte as char` cast, not real UTF-8 decoding)
  — only `TokenizerState::slice` ever did real UTF-8 validation, and it
  still does, on `&[u8]` input instead of `&str`. `new`/
  `new_with_error_manager` stay as `&str`-taking wrappers over new
  `_from_bytes` cores, so none of the roughly twenty other call sites
  (benches, mdix-lsp, mdix-cli, the import resolver, the QuickFuncs
  section parser) needed to change.
- This is what makes `@RAW` able to hold real binary payloads: the bytes
  between a `content -> { ---tag--- … ---tag--- }` block's delimiters
  never pass through `str::from_utf8` at all. Every other token still
  does, exactly as before.
- `scan_raw_content_block` finds the opening `---tag`, scans for the
  matching closing delimiter with `memchr::memmem::find` (same approach
  the existing multi-line-comment scanner already used), and requires an
  exact tag match — a stray `---something-else---` inside the payload is
  skipped rather than mistaken for the closer. Triggered only when the
  last two tokens pushed were `content` `->`, so `meta_data -> {` and
  `using -> {` in the same section are tokenized completely normally.

**Benchmarks:** see `benches/raw_section_benchmark.rs` for content-scan
throughput across payload sizes.

**Tests:** `tests/raw_section_tests.rs` covers section-keyword
recognition, correct byte-range extraction, mismatched open/close tags,
an unterminated block, and a payload containing an unrelated `---`
sequence that must not be mistaken for the closer.

### `Compiler/AST/raw.rs`

**What it does:** `RawBlock`, `RawField`, `RawContent` — the AST shape for
one `@RAW(...)` occurrence.

**Decisions:**
- `DixScript.raw` is `Vec<RawBlock>`, not `Option<RawSection>`. Every
  other section either exists once or (`@DATA`/`@QUICKFUNCS`/`@ENUMS`)
  merges multiple textual blocks into one logical collection. `@RAW`
  blocks do not merge — each one is independently identified and
  complete — so multiple `@RAW(...)` occurrences in a file are just
  multiple `RawBlock`s, not fragments of something shared.
- `content` is `Option<RawContent>`, not required, even though a real
  block always needs one. The parser stays permissive and builds the
  best AST it can; `raw_section_analyzer.rs` is where "missing content"
  becomes an actual error.

### `Compiler/Core/SectionParsers/raw_section_parser.rs`

**What it does:** Parses one `@RAW(...)` occurrence's tokens into a
`RawBlock`.

**Decisions:**
- Structure mirrors `security_section_parser.rs` closely (same recovery
  helpers, same field-list parsing for `meta_data`/`using`). `content`'s
  right-hand side is different from every other section value: it is
  never `{`/`}` tokens at the parser level, because the lexer already
  consumed the whole delimited block into one `RawContent` token — so
  parsing `content -> …` is just "consume the arrow, consume exactly one
  `RawContent` token."
- Unlike `@SECURITY`, `meta_data`/`using`/`content` are the only three
  recognized block names — an unrecognized key is a parse error here
  rather than silently accepted.

**Tests:** `tests/raw_section_tests.rs` — multiple distinct blocks, and
the feature-gating tests (`raw_section_allowed_by_default_advanced_mode`,
`raw_section_allowed_with_explicit_feature`,
`raw_section_rejected_without_feature_enabled`) exercise this parser
through the full `DixLoader` pipeline.

### `Compiler/Core/SectionAnalyzers/raw_section_analyzer.rs`

**What it does:** Semantic validation across every `@RAW` block in a file.

**Decisions:**
- Takes `&[RawBlock]`, not a single section — the cross-block checks
  (unique `meta_data.id`, unique delimiter tag) need every block visible
  at once, which no single `RawBlock` has on its own. Per-block checks
  (required `id`/`format`/`content`, type-checking `size`/`checksum` when
  present) run first; cross-block uniqueness runs second, using an
  `FxHashMap` keyed by id/tag.
- `using` is deliberately not validated against a fixed key set — a
  `module`-specific decoder legitimately needs different hint keys, so
  only the well-known ones (`filter`/`compression`/`threads`/`module`)
  get a type check when present.

**Benchmarks:** `benches/raw_section_benchmark.rs`'s
`raw_section_full_pipeline` group tracks how the uniqueness check scales
with block count (10 / 100 / 1,000 blocks in one file).

**Tests:** `tests/raw_section_tests.rs` — duplicate id, duplicate tag,
missing id/format/content, wrong type for `size`.

### `Runtime/loader.rs`

**What it does:** Runtime-layer, but the actual pipeline entry point —
reads a file (or takes source text directly), then drives tokenize,
parse, and semantic analysis in sequence.

**Decisions:**
- `compile_source` split into a `&str` wrapper (unchanged behavior for
  `load_from_str`/`compile_with_dlm_from_str`/
  `compile_to_resolved_ast_from_str`, which already have text in hand) and
  `compile_source_from_bytes`, the real pipeline. The two actual
  file-reading paths (`load_text`, `compile_to_resolved_ast`) call
  `fs::read` instead of `fs::read_to_string` and hand bytes to the new
  core directly — needed so a file with a genuinely binary `@RAW` payload
  can still load at all.

### `Compiler/Core/general_parser.rs`

**What it does:** Top-level section dispatch — extracts each section's
token stream, parses it, and assigns the result onto the `DixScript` AST.

**Decisions:**
- `assign_section_to_script` needed a third assignment mode for `@RAW`,
  alongside the existing two: overwrite (singleton sections) and merge
  (`@DATA`/`@QUICKFUNCS`/`@ENUMS`, which combine same-name blocks into one
  collection). `@RAW` appends — `ParsedSection::Raw(Some(block))` pushes
  onto `script.raw` rather than replacing anything, so multiple `@RAW`
  blocks in one file each land correctly instead of the last one
  overwriting the others.
- `has_raw_enabled` follows the same `operational_settings.is_feature_enabled("raw")`
  pattern as every other gated section.

### `Compiler/Core/general_semantics_analyzer.rs`

**What it does:** Orchestrates every section's semantic analysis phase.

**Decisions:**
- `analyze_phase6b_raw` mirrors `analyze_phase6_independent` (`@DLM`)
  exactly — runs unconditionally after it, does not feed into or depend
  on any other phase.

### `Compiler/VersionControl/version_constraints.rs` and `version_manager.rs`

**What they do:** `version_constraints.rs` validates a file's declared
`@CONFIG` `features` list against the set of recognized section names.
`version_manager.rs` tracks which features a given DixScript language
version actually supports, and gates individual tokens/sections against
that.

**Decisions:** see Fixes and Problems below — both files had a hardcoded
section-name list that needed `"raw"`/`"raw_section"` added, found while
wiring the feature gate through, not part of any pre-existing plan.

## Fixes and Problems

### `Compiler/VersionControl/version_constraints.rs`
- `is_valid_section_list`'s `valid_sections` set (used to validate
  `@CONFIG`'s `features -> "..."` list) was missing `"raw"`. Without this,
  `features -> "raw"` would have been rejected as an invalid features
  list even with every other part of `@RAW` correctly wired.

### `Compiler/VersionControl/version_manager.rs`
- `initialize_features_for_version` (the per-version feature set) was
  missing `"raw_section"`. Same class of problem as above, one layer
  down: without it, `@RAW` would never be considered a supported feature
  of version 1.0.0 at all, regardless of a file's own `@CONFIG`.

### Adding `DixScript.raw` broke every other struct-literal construction site
- Real CI (`cargo bench --no-run`, the actual crate compiled with every
  file together) caught 11 more `DixScript { ... }` construction sites
  across 9 files that a per-file `rustc --crate-type lib` check cannot
  catch, because it never sees the other files that construct the same
  struct: `Compiler/Core/BinarySerialization/binary_unpacker.rs`,
  `Runtime/converter.rs` (x2), `Runtime/data_builder.rs`, `Runtime/merge.rs`,
  `Runtime/dix_data.rs` (x9, test helpers), `Runtime/array_homogenizer.rs`,
  `Runtime/dix_deserialize.rs`, `benches/runtime_benchmark.rs` (x2),
  `tests/runtime_tests.rs` (x2). All fixed the same way: `raw: Vec::new()`,
  since none of these code paths (binary deserialization, JSON/TOML-to-AST
  conversion, the programmatic AST builder, `mdix merge`, test fixtures)
  currently produce or need `@RAW` blocks. `mdix merge` specifically is a
  known, disclosed gap rather than a real fix — it doesn't merge `@RAW`
  blocks across sources at all yet; a real `merge_raw` (concatenate plus
  cross-source id/tag collision detection, mirroring `merge_data`) is real
  new work, not attempted here.
- `Utilities/token_debug_printer.rs`'s `format_token_type` match was
  non-exhaustive once `SectionRaw`/`RawContent` existed — same reason,
  a file this session's per-file checks never had a reason to open.
- `raw_section_parser.rs`'s `recover_to_entry_boundary` had a genuine bug
  a per-file check also missed, but for a different reason: `matches!(self.current().token_type, TokenType::Identifier(id) if ...)`
  tried to move a `String` out through a shared reference. The per-file
  check never got far enough to reach this, because `TokenType` itself
  couldn't resolve standalone — the borrow-check step never ran on code
  past an unresolved-type error, so this was masked by an error already
  being tolerated as "expected." Fixed with `TokenType::Identifier(ref id)`.
- Takeaway: single-file `rustc --crate-type lib` checks catch real syntax
  errors, but can both miss cross-file-only errors (this whole `raw` field
  gap) and mask real bugs behind an already-tolerated resolution error.
  Real CI is the only actual gate — every batch's verification note already
  said this, and this is the concrete case that shows why.
