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

**What it does:** Parses one `@RAW(...)` occurrence's tokens into a `RawBlock`.

**Decisions:**
- `parse_section_doc` is the real parse and returns the `DocRawBlock` section with its nodes left in the
  parser's own arena, and `into_arena` returns that arena. `parse_section` is a wrapper that exports,
  so existing callers are unchanged, and `general_parser.rs` uses the first two to merge the arenas.
- Structure mirrors `security_section_parser.rs` closely (same recovery helpers, same field-list
  parsing for `meta_data` and `using`). `content`'s right-hand side is different from every other
  section value: it is never `{` or `}` tokens at the parser level, because the lexer already
  consumed the whole delimited block into one `RawContent` token. Parsing `content -> ...` is just
  "consume the arrow, consume exactly one `RawContent` token".
- Unlike `@SECURITY`, `meta_data`, `using` and `content` are the only three recognized block
  names. An unrecognized key is a parse error here rather than silently accepted.
- This is the second parser built on the arena AST. The parser owns an `Arena`, builds every field
  value as a node there, keeps the fields as `DocRawField` records that hold the value ids, and
  exports the finished block once, at the end of `parse_section`, through `DocRawBlock::export`.
  The `content` token is a plain `RawContent` that the arena does not touch. The public interface
  and the result are unchanged.
- As in the `@SECURITY` parser, it exports at the end of the section because sections are parsed in
  parallel, and `parse_raw_value` reads the current token through a copy of the shared slice so the
  arena can be borrowed mutably while the token is read. The end-of-file token is a module-level
  `static` shared by `current()` and `parse_raw_value`.
- The public items now carry doc comments, and the module documentation describes the arena.

**Tests:** `tests/raw_parser_tests.rs` pins the parser itself (positions, error strategies and
diagnostics). `tests/raw_section_tests.rs` has multiple distinct blocks and the feature-gating
tests (`raw_section_allowed_by_default_advanced_mode`, `raw_section_allowed_with_explicit_feature`,
`raw_section_rejected_without_feature_enabled`), which exercise this parser through the full
`DixLoader` pipeline.

### `Compiler/Core/SectionAnalyzers/raw_section_analyzer.rs`

**What it does:** Semantic validation across every `@RAW` block in a file.

**Decisions:**
- Takes every block at once, not a single section: the cross-block checks
  (unique `meta_data.id`, unique delimiter tag) need every block visible
  at once, which no single block has on its own. Per-block checks
  (required `id`/`format`/`content`, type-checking `size`/`checksum` when
  present) run first; cross-block uniqueness runs second, using an
  `FxHashMap` keyed by id/tag. The map borrows its keys from the arena and
  from the block, so the check allocates no strings.
- **One implementation, two entry points.** `analyze_doc(&[DocRawBlock],
  &Arena, ...)` holds the checks and reads values through the arena.
  `analyze(&[RawBlock], ...)` stays for callers that only have an owned
  `DixScript`: it copies the blocks into a scratch arena and calls
  `analyze_doc`, so the two cannot disagree. `GeneralSemanticAnalyzer` calls
  `analyze_doc` when it was given a `Document` and `analyze` when it was not.
- `using` is deliberately not validated against a fixed key set — a
  `module`-specific decoder legitimately needs different hint keys, so
  only the well-known ones (`filter`/`compression`/`threads`/`module`)
  get a type check when present.

**Tests:** `tests/raw_section_tests.rs` (through the loader) and
`tests/raw_analyzer_document.rs` (the diagnostics themselves, on both paths).

**Benchmarks:** `benches/raw_section_benchmark.rs`'s
`raw_section_full_pipeline` group tracks how the uniqueness check scales
with block count (10 / 100 / 1,000 blocks in one file).

**Tests:** `tests/raw_section_tests.rs` — duplicate id, duplicate tag,
missing id/format/content, wrong type for `size`.

### `Builtins/Static/schema_object.rs`

**What it does:** The `Schema` builtin static object — one method per field
type (`Schema.Int`, `Schema.String`, `Schema.Bool`, `Schema.Enum`, ...), each
returning a canonical descriptor Object (`type` / `required` / whichever
constraint keys were supplied).

**Decisions:**
- Method names are **capitalised**. The lowercase spellings (`int`,
  `string`, `bool`, `enum`, ...) are all lexer keywords, and
  `identifier_pattern_analyzer.rs` only accepts a plain `Identifier` after
  `.`, so `Schema.int(...)` could never be written in source. The
  descriptor's own `"type"` value stays lowercase — that is the DataType
  name, not a method name.
- A constraint that wasn't supplied is omitted from the Object entirely
  rather than stored as null, so `.get(key)` cleanly means "no constraint".
- The builtin is the single source of truth for which methods exist and
  which arguments each accepts; the analyzer calls it rather than
  re-implementing argument rules.

### `Compiler/AST/schema.rs`

**What it does:** `SchemaBlock` and `SchemaField` — the AST shape for
`@SCHEMA(...)`.

**Decisions:**
- `DixScript.schema` is `Option<SchemaBlock>` — a singleton like
  `@SECURITY`, not a `Vec` like `@RAW`: one schema describes one file's one
  `@DATA` section, so there is nothing to keep independent.
- A `SchemaField` stores the descriptor **as written** (`method` + literal
  `arguments`), not an evaluated result. Evaluation is the analyzer's job,
  by calling the `Schema` builtin. Same parse/semantic split every other
  section uses.
- `path` is the same `TablePath` `@DATA` uses for dotted addressing.

### `Compiler/Core/SectionParsers/schema_section_parser.rs`

**What it does:** Parses one `@SCHEMA(...)` into a `SchemaBlock`. Grammar:
`path = Schema.<Type>(literal, ...)`, commas between fields optional.

**Decisions:**
- `parse_section_doc` is the real parse and returns the `DocSchema` section with its nodes left in the
  parser's own arena, and `into_arena` returns that arena. `parse_section` is a wrapper that exports,
  so existing callers are unchanged, and `general_parser.rs` uses the first two to merge the arenas.
- Its own mini-parser rather than wrapping `DataSectionParser`. The original plan was to reuse
  `@DATA`'s value grammar, but `@DATA`'s parser has no static-builtin-call arm
  (`IdentifierPatternType::StaticMethodCall` is only handled inside QuickFuncs), so it cannot parse
  `Schema.Int(...)`. The descriptor is narrow enough, a fixed call with literal arguments, that no
  expression grammar is needed.
- A lowercase type name is caught and reported with the capitalised spelling as a hint, since it is
  the one mistake people will make.
- `first_error()` exists so `GeneralParser` can fail loudly. A section parser returning `None`
  becomes an **absent section**, not a compile error. That is harmless for most sections, but an
  absent `@SCHEMA` means *no validation at all*, so a typo would silently switch checking off for
  the whole file. Under Halt, `GeneralParser` now turns a malformed `@SCHEMA` into an error
  instead.
- This is the third parser built on the arena AST. The parser owns an `Arena`, builds every literal
  argument as a node there, keeps each field as a `DocSchemaField` record whose arguments are a run
  of value ids, and exports the finished section once, at the end of `parse_section`, through
  `DocSchema::export`. The arguments of one call are collected in a scratch `Vec<ValueId>` and
  stored once with `Arena::alloc_value_list`, because their count is unknown until the closing
  parenthesis. A call that fails part way leaves its nodes unreferenced in the arena, which is
  harmless because the arena is append-only and is dropped with the parser. The public interface,
  including `first_error()`, and the result are unchanged.
- As in the other converted parsers, it exports at the end of the section, `parse_argument` reads
  the current token through a copy of the shared slice, and the end-of-file token is a module-level
  `static` shared by `current()` and `parse_argument`.
- The public items now carry doc comments, and the module documentation describes the arena.

**Tests:** `tests/schema_parser_tests.rs` pins the parser itself (positions, literal kinds, error
strategies and the halt message). `tests/schema_section_tests.rs` has flat and dotted paths,
optional commas, keyword-named path segments, negative bounds, lowercase type name, malformed
section and non-literal argument, through the loader.

### `Compiler/AST/Visitors/type_inference_visitor.rs`

**What it does:** Infers the type of a value or an expression from the symbol table and from the
types of the local variables it was given: literals, function and method calls, arithmetic,
qualified identifiers (enum access, namespaced calls, methods on typed collections), index and
property access. The `@DATA` analyzer, the `@QUICKFUNCS` analyzer and the language server use it.

**Decisions:**
- **The four owned entry points are wrappers.** `infer_type_from_value`, `infer_type_from_expression`,
  `infer_element_type_from_value` and `infer_element_type_from_expression` copy their argument into a
  scratch arena and call the entry point of the same name with an `_id` suffix, which lives in
  `type_inference_arena.rs` (see `arena-ast.md`). There is one implementation of the walk, so the two
  forms cannot disagree. A caller that already holds an arena should call the `_id` forms: they copy
  nothing.
- **What depends on names alone stays here and is shared:** the method tables, the identifier and
  function lookups, the function return types, the numeric check and the tuple slot of a positional
  method. The tree-walking code that used to be here (about 750 lines) and the two method tables only it
  used are gone.
- **The cost for the language server is not measured.** It calls the owned entry points on single
  expressions, so each call now copies that expression's subtree into an arena first. That is linear in
  the size of the expression, and the expressions it asks about are small, but a very deep expression
  asked about at every level of its own walk would pay for the copy at each level.
- The public items of this file now have rustdoc, with two examples that run as doctests.
- `tests/type_inference_arena.rs` pins the answers to the ones the owned walker gave before it was
  removed.

### `Compiler/Core/SectionAnalyzers/data_section_analyzer.rs`

**What it does:** Checks the `@DATA` section: the two-tier ordering (flat properties before tables
and group arrays), unique table paths, group array paths, flat names and table property names,
reserved keywords, declared against inferred types (with the int to long and int to float
widenings), array homogeneity, nesting depth (12 levels), object literal duplicates, tuples (6
elements), blob and regex constructors, enum values and function calls. It builds the two indexes
the later stages use (short name to full paths, and path to type) and registers every property in
the symbol table.

**Decisions:**
- **One implementation, two entry points.** `analyze_doc(&DocData, &Arena, ...)` holds the checks
  and reads values through the arena. `analyze(&DataSection, ...)` stays for callers that only have
  an owned `DixScript`: it copies the section into a scratch arena and calls `analyze_doc`, so the
  two cannot disagree.
- **Types are inferred with the arena entry points of `TypeInferenceVisitor`,** at every nesting
  level as before. Exporting each value to the owned type to call the owned visitor would copy every
  subtree at every level, which would make the largest section slower than it was.
- The arena is passed down through the validators as an argument. Storing a reference to it in the
  analyzer would add a second lifetime to a type that is built in several places.
- The checks and the order they run in are unchanged, so the diagnostics, their positions, the
  indexes and the symbol table entries are the same. The exemption that lets two objects sit
  together in an array is redundant (two objects already compare equal) and was left as it was.
- Some branches cannot be reached from text, because the parser merges or drops the input first: a
  flat property after a table, an expression value, a blob or regex with no or a non-string
  argument, a nested array value. Tests build those sections by hand.

**Tests:** `tests/data_parser_tests.rs` (the parser) and `tests/data_analyzer_document.rs` (the
diagnostics, indexes and symbol table entries, on both paths).

### `Compiler/Core/SectionAnalyzers/security_section_analyzer.rs`

**What it does:** Checks the `@SECURITY` section and reports what will fall back to a default: an
empty section, an unknown or empty block, a missing or mistyped `mode`, `algorithm` or `kdf`, a weak
algorithm (`xor`), key derivation parameters below their recommended minimum, a keystore
`backup_count` outside 0 to 10, and a manual mode whose plaintext key warning was not accepted.

**Decisions:**
- **It only warns.** Every fallback is applied when the file is encrypted, so none of these stops a
  compile and `result.errors` stays empty. Warning ids are `SEC_WARN001` to `SEC_WARN008`. The rule
  that `@DLM` with an encryptor needs a `@SECURITY` section (`SEM0002`) belongs to the orchestrator,
  not to this analyzer.
- **One implementation, two entry points.** `analyze_doc(&DocSecurity, &Arena, ...)` holds the checks
  and reads values through the arena. `analyze(&SecuritySection, ...)` stays for callers that only
  have an owned `DixScript`: it copies the section into a scratch arena and calls `analyze_doc`, so
  the two cannot disagree.
- **A repeated key or block resolves in two different ways, and both are kept.** In the encryption
  config, the keystore block and the validation block the last field with a key wins. The manual mode
  check takes the first `manual_key_warning_accepted`. Of two blocks with the same name the last one
  wins. These look inconsistent, but they are what the analyzer always did, so tests pin each one.
- Block names and the `kdf` and checksum algorithm names are matched ignoring ASCII case. The `mode`
  and `algorithm` names are matched exactly.
- Key derivation parameters must be integers. A value written as a long, such as `100L`, is not
  compared against the minimum and is ignored.

**Tests:** `tests/security_section_tests.rs` (the parser) and `tests/security_analyzer_document.rs`
(the diagnostics themselves, on both paths).

### `Compiler/Core/SectionAnalyzers/schema_section_analyzer.rs`

**What it does:** Validates `@SCHEMA` descriptors, then checks `@DATA`
against them: presence, type, numeric `min`/`max`, string
`minLength`/`maxLength`, array `minItems`/`maxItems`, enum name.

**Decisions:**
- Runs **twice**. From the semantic phase (`analyze_phase7b_schema`) against
  the `@DATA` AST as parsed — plain literals are fully checked, so
  `mdix validate` and the LSP see real schema errors, while values not
  knowable yet (QuickFunc calls, references, expressions) classify as
  deferred and are skipped rather than failed. Then from `DixLoader`
  (Stage 9) against the resolved AST, where computed values are concrete —
  the only place a schema can constrain a QuickFunc-produced value.
- Numeric compatibility widens, never narrows: `Int` takes an integer,
  `Long` an integer or long, `Float`/`Double` any numeric. A bare `1.5` is
  a double, so rejecting it for `Schema.Float` would force an `f` suffix on
  every value for no benefit.
- An explicit `<type>` annotation on the `@DATA` property takes precedence
  over the literal's own kind, since it is what the runtime stores.
- `is_success` is derived from the error list in a wrapper around the
  checks, not at the end of them — the Halt strategy returns early from
  inside the checks, and `SectionAnalysisResult::new` defaults it to false.
- Error ids `SCH001`–`SCH007`; warning `SCHEMA_WARN001` (empty schema).
- **One implementation, three entry points.** `analyze_doc(&DocSchema, Option<&DocData>, &Arena, ...)`
  holds the checks and reads both sections through the arena. `analyze` and `validate_resolved` keep
  their owned signatures and copy the schema and the data into a scratch arena before calling it, so
  the three cannot disagree.
- **The post-resolution pass pays for a copy.** `DixLoader` calls `validate_resolved` with the tree
  that value resolution produced, which is still the owned type, so Stage 9 copies the resolved
  `@DATA` section into an arena to check it. The semantic phase no longer copies anything. The extra
  cost is one pass over the resolved data, and only for a file that has a `@SCHEMA`. It goes away
  when the resolver reads the arena. It has not been measured.
- The `Schema` builtin takes owned values, so `build_descriptor` copies the arguments of each field
  out of the arena before calling it. They are a handful of literals, not the data section.
- A string's length is its number of characters, not its bytes, so `"héllo wörld"` is 11.

**Not done:** no nested-object shape validation (`Schema.Object` checks
presence and kind only), no wildcard paths, no runtime validation of data
supplied after compilation — compile-time only.

**Tests:** `tests/schema_section_tests.rs` — missing/optional fields, wrong
type, numeric/string/array bounds, table and group-array paths, enums, the
post-resolution stage, feature gating. `tests/schema_analyzer_document.rs`
pins the diagnostics themselves and compares the three entry points.

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
- Stage 9 (after numeric-array homogenization) runs
  `SchemaSectionAnalyzer::validate_resolved` against the resolved `@DATA`,
  so a schema can constrain values a QuickFunc computed. A file whose plain
  literals already failed never reaches it, so nothing is reported twice.
- Stage 4 calls `GeneralParser::parse_document` and exports the `Document` once, into the owned
  tree that the stages which have not moved still read. Stage 5 gets both, through
  `with_document`. Stage 6 gets the document again, `&mut` this time, because the `@QUICKFUNCS`
  enhancer allocates its output in the document's arena. Stage 7 gets it too, through
  `ValueResolver::with_document`, because the first phase of value resolution (enum
  pre-resolution) reads and rewrites `@DATA` in that arena. The document is dropped after stage 7,
  whether or not the stage ran, because nothing later reads the arena. When the last stage that
  reads the owned tree has moved, the export goes away. Imported
  files are still parsed by the imports resolver with `parse`, so their `@RAW`, `@SECURITY` and
  `@SCHEMA` sections take the owned path. Stage 9 still passes the resolved owned tree to
  `SchemaSectionAnalyzer::validate_resolved`, which copies it into a scratch arena.

### `Compiler/Core/general_parser.rs`

**What it does:** Top-level section dispatch: extracts each section's
token stream, parses it, and assigns the result onto a `Document`, which `parse` exports as the
`DixScript` AST.

**Decisions:**
- **`parse_document` is the real parse, and `parse` is a wrapper.** `parse_document` returns a
  `Document`: one arena plus the sections. The sections are parsed in parallel, so each section
  parser builds into its own arena (`parse_section_doc`, then `into_arena`), and the result of each
  one comes back as a section together with its arena. The arenas are appended to the document's
  arena in source order, after the parallel part, whichever thread finished first, so the result is
  the same on every run. `parse` is `parse_document` followed by one `export`, so every existing
  caller is unchanged. The stages that move to the arena call `parse_document` and read the
  `Document`.
- `@SECURITY` is the one section that is sometimes created by the parser: a file with an encryptor
  module gets a section completed by `SecurityUtilities`, which works on the owned type. That file's
  section is exported, completed and imported again into the document's arena.
- `assign_section_to_document` needed a third assignment mode for `@RAW`,
  alongside the existing two: overwrite (singleton sections) and merge
  (`@DATA`/`@QUICKFUNCS`/`@ENUMS`, which combine same-name blocks into one
  collection). `@RAW` appends — `ParsedSection::Raw(Some((block, arena)))` appends
  the arena and pushes the shifted block onto `document.raw` rather than replacing anything, so multiple `@RAW`
  blocks in one file each land correctly instead of the last one
  overwriting the others.
- `has_raw_enabled` follows the same `operational_settings.is_feature_enabled("raw")`
  pattern as every other gated section.
- `@SCHEMA` is a singleton, so `ParsedSection::Schema` uses a plain
  overwrite in `assign_section_to_document` (unlike `Raw`, which appends).
  Its dispatch arm also refuses a malformed section under Halt — see
  `schema_section_parser.rs`'s `first_error`.
- `is_section_keyword_token` now lists `SectionRaw` and `SectionSchema`
  (see Fixes and Problems).

### `Compiler/Core/general_semantics_analyzer.rs`

**What it does:** Orchestrates every section's semantic analysis phase.

**Decisions:**
- `analyze_phase6b_raw` mirrors `analyze_phase6_independent` (`@DLM`)
  exactly — runs unconditionally after it, does not feed into or depend
  on any other phase.
- `analyze_phase7b_schema` runs straight after `@DATA`'s own phase,
  because `@SCHEMA` constrains `@DATA` and needs it analysed first. It only
  sees the unresolved AST; the post-resolution half lives in `DixLoader`.
- **An optional `Document`.** `with_document(&Document)` gives the analyzer the arena form of the
  file. A section whose analyzer has moved to the arena reads it from the document, and a section
  that has not moved reads the owned `DixScript` as before. `@RAW`, `@SECURITY`, `@SCHEMA` and
  `@DATA` and `@QUICKFUNCS` have moved. Their phases (`analyze_phase6b_raw`,
  `analyze_phase8_generated`, `analyze_phase7b_schema`, `analyze_phase7_data_driven` and
  `analyze_phase5_functions`) call the section analyzer's
  `analyze_doc` with the document's section and
  arena when one is attached, and the owned `analyze` (which copies into a scratch arena) when none
  is. `@SCHEMA` takes the `@DATA` section from the document too, and the indexes the `@DATA`
  analyzer builds are returned from the section that was read. Whether a section is present is decided by the same tree that is analyzed,
  and that includes the check that `@DLM` with an encryptor has a `@SECURITY` section. The
  constructors are unchanged, so the imports resolver, the language server and every test that
  builds the analyzer from an owned tree compile and behave as they did.
- **`@IMPORTS`, `@ENUMS` and `@DLM` are read from the tree that is read as well.** The document
  holds the owned types for them, so they need no arena entry point; three small helpers,
  `imports_section`, `enums_section` and `dlm_section`, return the document's section when one is
  attached and the owned tree's otherwise. The five places that read these sections use them: the
  imports in the semantic phase and in the resolution phase, the enums, the `@DLM` phase, and the
  `@DLM` that decides whether `@SECURITY` is required. Every section the orchestrator looks at now
  follows the one rule, so the owned export can go as soon as the last stage after the orchestrator has
  moved. For a file the loader produces the result is the same as before. For a pair that disagrees on
  purpose, the `@DLM` of the document now decides whether an encryptor needs a `@SECURITY` section; it
  used to be the `@DLM` of the owned tree.
- The analyzer does not check that the document and the owned tree describe the same file. A pair
  that disagrees is analysed as a mixture of both, and two tests build such a pair on purpose to
  show which half a section reads.

### `Compiler/VersionControl/version_constraints.rs` and `version_manager.rs`

**What they do:** `version_constraints.rs` validates a file's declared
`@CONFIG` `features` list against the set of recognized section names.
`version_manager.rs` tracks which features a given DixScript language
version actually supports, and gates individual tokens/sections against
that.

**Decisions:** see Fixes and Problems below — both files had a hardcoded
section-name list that needed `"raw"`/`"raw_section"` added, found while
wiring the feature gate through, not part of any pre-existing plan.

**Reading the Document.** `VersionConstraints::validate_script_doc(&Document)` does the validation of
`validate_script(&DixScript)` for a file held as a `Document`, and the orchestrator's version phase
calls it when a document is attached. Both are one skeleton, `validate_sections`, that takes the
`@CONFIG`, `@DLM`, the presence of `@IMPORTS` and `@ENUMS` (all the same owned types in a document) and
a `Deep` value holding `@QUICKFUNCS` and `@DATA` in whichever form the caller has. Only those two are
looked into, and only for the parameters of each function and the top-level kind of each value.
- **The leaf scans are written once for each form instead of copying a section into an arena to scan
  it once.** Each is a loop over the entries with a handful of field tests. Copying all of `@DATA` into
  an arena to run it would cost far more than the scan, and the language server, which has no
  document, would pay for it on every analysis. The messages are shared by `call_in_data` and two
  constants.
- **`TypeSystemManager::validate_doc_quick_function_signature` is the arena form of the signature check.**
  It copies out only the value of a default, to be typed by the code the owned form uses, and never
  touches a body. Both forms dropped the two checks that could not fail, a return type and a parameter
  type being tested with `is_valid_data_type`, which returns true for every type.
- **The scan for calls in `@DATA` runs only when quick functions are not available**, since that is the
  only case in which a call is an error, instead of testing every value either way.
- **Two small costs went.** `is_valid_section_list` built a set of eight strings on each call and is a
  `match` now; `VersionManager::supports_feature` allocated a `String` to look a `&str` up in a set of
  `String`s and no longer does.

**Dead code removed.** Everything below had no reference anywhere in the workspace (the library, its
tests and benches, every sibling crate, and the docs), was a check that always passed, or was a
duplicate:
- `VersionConstraints`: `with_forward_compat`, and with it the `forward_compat_manager` field and the two
  branches only it could reach (`handle_unknown_feature` and the unknown-section fallback of
  `is_valid_section_type`); `is_valid_value_type`, `is_valid_default_value`, `is_valid_type_annotation`
  (it returned true for any argument), `requires_advanced_mode`, `is_valid_function_type`,
  `is_valid_static_method_call`, `is_valid_instance_method_call`, `is_valid_dix_function_call`,
  `validate_quick_function_signature` and `validate_data_entry` (their work is in the skeleton now).
- `VersionManager`: `validate_script_features`, `supports_feature_control`, `supports_dual_parsers`,
  `get_recommended_parser`, and `get_current_version`, a deprecated duplicate of `current_version`
  whose callers (the lexer, `ForwardCompatibilityManager` and `VersionConstraints`) now call that.
- `ValidationResult`: the `warnings` and `detected_version` fields, which nothing ever set, and the
  parts of its `Display` that printed them.

**Tests:** `tests/version_constraints_document.rs`.

**Language version 2.0.0.** The manager knows two language versions, 1.0.0 and
2.0.0, and a file picks one with `@CONFIG` `version`. They are the same language
except for the DLM codecs: bzip2 and lzma are valid in 1.0.0 and invalid in 2.0.0.
- The per-version feature set carries one key per codec (`dlm_codec_gzip` in both,
  `dlm_codec_bzip2` and `dlm_codec_lzma` in 1.0.0 only). `is_valid_dlm_module` reads
  those keys, so a 2.0.0 file that names a removed codec fails the version check with
  a message that says it was removed. A 1.0.0 file passes the version check and is
  stopped later by the DLM analyzer (`DLM004`), because the codecs no longer exist in
  the implementation for either version.
- `is_compatible_with(v)` now means "this manager can read files up to version v".
  It used to be an exact match against a set that only held 1.0.0.
- `VersionConstraints::get_version_constraints` keeps `SupportedDLMModules` as what
  actually runs (gzip) and adds `RemovedDLMModules` for tooling that wants to explain
  a removed codec.
- The defaults did not move. `OperationalSettings`, the config schema defaults, the
  minimal config and the manager's fallback are all still 1.0.0, because tests assert
  on them. A file opts into 2.0.0 by declaring it. Moving the defaults is a separate
  step that also has to move those tests.
- The manager is a process-wide singleton set when a file's `@CONFIG` is processed.
  Compiling files that declare different versions on different threads can see the
  other file's version for the codec check. The outcome is the same either way (both
  versions end in an error for a removed codec), only the message differs. The new
  unit tests build their own `VersionManager` and use the pure helpers so they never
  touch the singleton.

### `Compiler/AST/mod.rs`

**What it does:** Declares the AST submodules and re-exports their types flat, so
`Compiler::AST::Value` and `Compiler::AST::DixScript` are the usual paths. `pub mod arena;`
is declared here and is the one submodule that is not re-exported flat, so it is reached as
`Compiler::AST::arena`. Its types are documented in `docs/dixscript/arena-ast.md`.

**Decisions:**
- The arena is not re-exported flat, so its names (`Arena`, `Document`, `ValueNode`) cannot
  collide with the owned AST names.
- The module documentation now says that `arena` exists and which parsers use it so far (the
  the five section parsers).

### `Compiler/Core/SectionParsers/security_section_parser.rs`

**What it does:** Parses the `@SECURITY(...)` section into a `SecuritySection`: blocks such as
`encryption -> { mode = "password" }`, each holding `key = value` fields whose values are string,
integer, long, float, double, boolean, hex colour, `null` or `auto`.

**Decisions:**
- `parse_section_doc` is the real parse and returns the `DocSecurity` section with its nodes left in the
  parser's own arena, and `into_arena` returns that arena. `parse_section` is a wrapper that exports,
  so existing callers are unchanged, and `general_parser.rs` uses the first two to merge the arenas.
- This is the first parser built on the arena AST, as a pilot. The parser owns an `Arena`, builds
  every field value as a node there, keeps entries and fields as `DocSecurityEntry` and
  `DocSecurityField` records that hold the value ids, and exports the finished section once, at
  the end of `parse_section`. The public interface and the result are unchanged.
- It exports at the end of the section, rather than handing the arena on, because the sections are
  parsed in parallel and their arenas cannot yet be merged. Until a consumer reads a `Document`,
  this adds one export copy per section, so it is not faster than before. The `@SECURITY` section
  is small, so the cost is small too.
- `parse_security_value` reads the current token through a copy of the shared token slice, so the
  token does not borrow the parser and the arena can be borrowed mutably while it is read.
- The end-of-file token is a module-level `static` shared by `current()` and
  `parse_security_value`.
- The public items now carry doc comments, and the module documentation describes the arena.

**Tests:** `tests/security_section_tests.rs`. The conversion was checked against the original
parser with a before and after dump (see Fixes and Problems below), and the same ten tests pass on
both.

### `tests/security_section_tests.rs`

**What it does:** Pins the behaviour of the `@SECURITY` parser, which had no tests of its own. It
runs the loader's first four stages (tokenize, split `@CONFIG`, process `@CONFIG`, parse) with an
isolated error manager so the parse diagnostics can be read back. The tests cover a plain section
with exact positions, every scalar value kind, optional commas between blocks and a trailing field
comma, an empty section and an empty block, that block names are not validated by the parser,
seven malformed inputs under `halt` (each must give no section and report the right message),
the `Value::Error` stand-in for a missing value under `continue` and `recover`, recovery skipping
to the next good entry, recovery around two bad values, and every `@SECURITY` section in the
corpus.

**Decisions:**
- Every expectation was taken from the original, pre-arena parser's output, and the tests were
  run against that parser as well as the converted one.
- Positions are pinned exactly in the cases where a position is the point (a plain section, the
  error-value cases). The scalar test compares the printed value and the value kind.
- The recover and continue cases put a three-line `@CONFIG` header in front, which shifts every
  source line by three.

**Tests:** `cargo test -p dixscript --test security_section_tests`. Ten tests, all passing on
Rust 1.75.

### `tests/raw_parser_tests.rs`

**What it does:** Pins the behaviour of the `@RAW` parser itself. `raw_section_tests.rs` reaches
the parser only through `DixLoader`, which cannot show source positions, what each error strategy
does with malformed input, or the diagnostics. The helper runs the loader's first four stages
(tokenize, split `@CONFIG`, process `@CONFIG`, parse) with an isolated error manager so the parse
diagnostics can be read back. The fourteen tests cover a full block with exact positions and the
content payload read back through its byte range, every scalar value kind with positions, an empty
section, optional commas between entries and after the last field, two sections in one file, the
`Value::Error` stand-in for a missing value under `continue` and `recover`, `halt` giving up on a
malformed block, a missing brace under each strategy, duplicate `meta_data` and `content` blocks,
an unknown key, an unclosed section, a `content` that is not a raw block, and a missing comma
between fields.

**Decisions:**
- Every expectation was taken from the original, pre-arena parser's output, and the tests were run
  against that parser as well as the converted one.
- The payload is compared through the `content` token's byte range into the parsed source, not
  through absolute offsets, because the offsets depend on the length of the `@CONFIG` header.
- The `continue` and `recover` cases put a fourth line in the `@CONFIG` header (the
  `error_handling` key), which shifts every source line by one compared with the `halt` cases.

**Tests:** `cargo test -p dixscript --test raw_parser_tests`. Fourteen tests, all passing on Rust
1.75.

### `tests/schema_parser_tests.rs`

**What it does:** Pins the behaviour of the `@SCHEMA` parser itself. `schema_section_tests.rs`
reaches the parser only through `DixLoader`. The helper is the same as in `raw_parser_tests.rs`,
except that it returns the error the parser raised, which is what the `halt` strategy does with a
malformed `@SCHEMA`. The ten tests cover literal arguments of every kind with exact positions,
negative numbers as single literals, an empty argument list and an empty section, dotted paths and
keyword path segments, optional commas between fields, ten fields in order, eleven malformed inputs
under `halt` (each checked against the full message with its line and column), `continue` against
`recover` after a bad field, a bad argument under both, and an unclosed section keeping its fields.

**Decisions:**
- Every expectation was taken from the original, pre-arena parser's output, and the tests were run
  against that parser as well as the converted one.
- The `halt` messages are checked in full, with line and column, because the message is what the
  user sees and `first_error()` is the only thing that makes `GeneralParser` raise it.
- The tests record that `continue` ends the section at the first bad field, while `recover` resumes
  at the next one. They record the behaviour that existed and do not change it.

**Tests:** `cargo test -p dixscript --test schema_parser_tests`. Ten tests, all passing on Rust
1.75.

### `tests/data_parser_tests.rs`

**What it does:** Pins the behaviour of the `@DATA` parser itself. The loader tests reach it only
through `DixLoader`, which cannot show what each error strategy does with malformed input or
which diagnostics are reported. The helper runs the loader's first four stages with an isolated
error manager and sets the strategy on the settings after `@CONFIG` is processed, so the sources
carry no `error_handling` key. Each of the 35 tests prints the parsed section with the owned
`Display` and compares it, and the sorted diagnostic messages, with the recorded text, under one
to three strategies (61 runs). One case also pins the section's `Debug` text, which holds every
source position.

**Decisions:**
- The cases cover every value kind and sign, every type annotation form including the fused `>>=`,
  nested arrays, objects and tuples, group arrays and tables, calls (local and imported) with
  arguments of every expression kind, entries without commas, a flat property after grouped data,
  missing values, unclosed arrays, objects and calls, bad object properties and annotations, the
  nesting depth limit, an unclosed section, a missing opening parenthesis, and stray tokens.
- **The `does_not_leak` and `keep_their_own` tests are the point of the file.** Each puts a list
  that fails (or succeeds) inside another list of the same or another kind: an array in an array,
  an object in an object, a tuple in an array, a call in a call, a call in an array in a call, an
  imported call in an array. They check that the enclosing list holds only its own children.
- The expectations were recorded from the converted parser and checked against the output of the
  original parser, which is byte for byte the same on all 61 runs.

**Tests:** `cargo test -p dixscript --test data_parser_tests`. 35 tests, all passing on Rust 1.75.
Nine deliberate bugs were injected to check them (see the Fixes entry below).

### `tests/quickfuncs_parser_tests.rs`

**What it does:** Pins the behaviour of the `@QUICKFUNCS` parser itself. The loader tests reach it
only through `DixLoader`, which cannot show what each error strategy does with malformed input or
which diagnostics are reported. The helper is the same as in `data_parser_tests.rs`. Each of the 41
tests prints the parsed section with the owned `Display` and compares it, and the sorted
diagnostic messages, with the recorded text, under one to three strategies (70 runs). One case
also pins the section's `Debug` text, which holds every source position.

**Decisions:**
- The cases cover the function signatures (return types, scopes, parameter defaults), every
  statement kind, the `if`/`elif`/`else` chain and nested blocks, `chk` switches with `miss` and
  nested switches, operator precedence, unary operators and ternaries, postfix chains, calls,
  every literal, containers and constructors, interpolated strings, lambdas, the nesting depth
  limits, and malformed input in each construct under all three strategies.
- **The `does_not_leak` and `keep_their_own` tests are the point of the file.** They put a list
  that fails (or succeeds) inside another list: an array in an array, an object in an object, a
  tuple in an array, a call in a call, a call in an array in a call, a statement block in a
  statement block, and a lambda block in a block. They check that the enclosing list holds only its
  own children.
- The expectations were recorded from the converted parser and checked against the output of the
  original parser, which is byte for byte the same on all 70 runs.

**Tests:** `cargo test -p dixscript --test quickfuncs_parser_tests`. 41 tests, all passing on Rust
1.75. Ten deliberate bugs were injected to check them (see the Fixes entry below).

### `tests/raw_analyzer_document.rs`

**What it does:** Pins the diagnostics of the `@RAW` analyzer and checks that it gives the same ones
when it reads a `Document` as when it reads the owned tree. The loader tests in
`raw_section_tests.rs` mostly check only that a source is accepted or rejected, so they do not show
which diagnostics were produced.

**Decisions:**
- **Twenty sources, three strategies.** Each source exercises one check: every error code
  `RAW001` to `RAW008`, both warnings, the types `size`, `checksum` and `threads` may take, a blank
  id, an id that is not a string, a repeated key, and `@RAW` next to `@DATA` and `@QUICKFUNCS`. The
  comparison test runs each under `halt`, `continue` and `recover`, with and without a document, and
  compares success, errors, warnings, positions, the per-section results and what reached the error
  manager (60 comparisons).
- **The expected ids were recorded, not derived.** The table in
  `raw_diagnostic_matrix_matches_the_recording` was written from the output of the analyzer before it
  moved, captured by the before and after harness (see the last Fixes entry). One row shows that
  `halt` stops after the first block with errors and so never reaches the duplicate checks, and a
  second test shows `continue` going on to report them.
- **Two tests build a document and an owned tree that disagree on purpose.** One has duplicate ids
  in the document and no blocks in the tree, the other the reverse. Nothing else can show which half
  the analyzer reads, because for a matching pair the answers are the same.
- The helpers `DocRawBlock::id` and `format` are compared with the owned `RawBlock::id` and `format`
  on every block of every source, and a repeated key shows that the first one wins on both sides.

**Tests:** `cargo test -p dixscript --test raw_analyzer_document`. 11 tests, all passing on Rust
1.75. Eleven deliberate bugs were injected to check them (see the last Fixes entry).

### `Compiler/Core/SectionAnalyzers/quickfuncs_section_analyzer.rs`

**What it does:** Checks the `@QUICKFUNCS` section: function names, parameters and defaults, return
types and scopes, body size and nesting, every statement and expression form, variable declaration
and assignment rules, collection limits, call arity, enum and namespace access, unused variables and
unreachable return paths. It records the qualified identifier resolutions the enhancer reads, and it
registers each function (except a name defined twice) in the symbol table. About 80 error and warning
codes are reachable from text or from hand-built input.

**Decisions:**
- **One implementation, two entry points.** `analyze_doc(&DocQuickFuncs, &Arena, ...)` holds the
  checks and reads statements, expressions and values through the arena. `analyze(&QuickFuncsSection,
  ...)` stays for callers that only have an owned `DixScript`: it copies the section into a scratch
  arena and calls `analyze_doc`, so the two cannot disagree.
- **Types are inferred with the arena entry points of `TypeInferenceVisitor`.** The owned entry points
  are wrappers over them now and are used by the language server.
- **The arena is passed down as an argument** (31 validators take it), as in the `@DATA` analyzer.
- **Resolutions are keyed by position and name parts, not by node,** so the map the enhancer reads is
  the same whichever tree was analyzed.
- **A default value is copied out of the arena** when a function is registered, because the symbol
  table keeps `ParameterInfo.default_value` as the owned `Expression`.
- **Three codes cannot be reached and are left as they are.** `QFUNC003B` and `QFUNC007` check that a
  data type is valid, and every data type is. `QFUNC073` checks the depth budget, but the statement
  nesting limit (50) stops the recursion long before the budget (100 or more) can run out.
- **Two things in the output depend on hash order and are not changed here.** The "Valid values",
  "Available functions" and "Available enums" suggestions list names in the order of a `HashMap`, and
  the cycle detector starts each reported cycle from a different function on each run. Tests and the
  verification harness normalize both. Sorting them in the analyzer would make the diagnostics
  deterministic.

**Tests:** `tests/quickfuncs_parser_tests.rs` (the parser) and `tests/quickfuncs_analyzer_document.rs`
(diagnostics, resolutions and registered functions, on both paths).

### `Compiler/Core/Functions/call_collector.rs` and `cycle_detector.rs`

The call collector walks every function and adds a call edge for each call to a function that is not a
local; the cycle detector builds the graph from it, reports cycles and calls to functions that do not
exist. Both now read the arena (`analyze_function_doc`, `validate_function_calls_doc`,
`get_execution_order_doc`) and keep their owned entry points as wrappers that copy into a scratch arena.
The `@QUICKFUNCS` analyzer is their only caller.

### `tests/data_analyzer_document.rs`

**What it does:** Pins the diagnostics, the two indexes and the symbol table entries of the `@DATA`
analyzer, and checks that it gives the same ones when it reads a `Document` as when it reads the
owned tree. The parser tests in `data_parser_tests.rs` stop at the parse, and the loader tests only
ask whether a file is accepted.

**Decisions:**
- **Forty-nine parsed sources, three strategies.** They cover all 17 `@DATA` error codes except the
  ones the parser cannot produce, the nesting limit on both sides of the boundary, mixed and nested
  arrays, group arrays, tables, typed and untyped properties, null values, enums with and without a
  namespace, function calls, constructors and short-name indexing. The comparison test runs each
  with and without a document (147 comparisons) and also compares the indexes.
- **The expected output was recorded, not derived.** `RECORDING` holds, per source, the section's
  diagnostics, the short-name index, the type index and the registered variables, written from the
  analyzer before it moved (63 cases: one under `continue` per source, and one under `halt` or
  `recover` where that strategy changes something).
- **Twenty-one sections are built by hand,** for the branches the parser cannot reach. `HAND_RECORDING`
  holds the recorded output of each under three strategies (63 blocks), and the test runs every one
  through `analyze` and through `analyze_doc`.
- Two tests build a document and an owned tree that disagree on purpose, as the other analyzer
  tests do, and a third takes the document of one file and the tree of another to show that the
  indexes describe the section that was read.

**Tests:** `cargo test -p dixscript --test data_analyzer_document`. 11 tests, all passing on Rust
1.75. Eighteen deliberate bugs were injected to check them (see the last Fixes entry).

### `tests/quickfuncs_analyzer_document.rs`

**What it does:** Pins the diagnostics, the qualified identifier resolutions and the registered
functions of the `@QUICKFUNCS` analyzer, and checks that it gives the same ones when it reads a
`Document` as when it reads the owned tree.

**Decisions:**
- **Fifty-nine parsed sources, three strategies,** covering 79 of the 83 codes together with the
  hand-built sections. The three largest sources are built by code instead of written out.
- **Nine sections are built by hand,** for the checks the parser prevents: invalid names, operators and
  types, `ObjectCreation` statements (the parser never produces them), imported namespaces and enums
  from a pre-populated symbol table, and an expression deeper than the depth budget. `HAND_RECORDING`
  holds the recorded output of each under three strategies (27 blocks), and every one runs through
  `analyze` and through `analyze_doc`.
- **The two hash-order effects are normalized** in the same way as in the harness that made the
  recording, so the test is not flaky. It passed eight runs in a row.
- Two tests build a document and an owned tree that disagree on purpose, and others show that the
  resolutions and the registered functions come from the section that was read.
- **Eighteen tests were added after the bug injection** (see the last Fixes entry), because the first
  13 left most injected bugs unnoticed. They pin the edge of each size limit (the largest accepted
  size and one more), the expression depth budget (the smallest nesting that is refused, 110 for a
  declaration or return, 100 for a log in a short body, 120 in a body of 200 statements), a call in each
  of 27 kinds of construct reaching the call graph, local names that must not become calls, a
  variable used in each of 32 kinds of construct not being reported as unused, the error manager's
  records for undefined callees and cycles with their positions, the owned entry points of
  `CycleDetectionValidator` and `FunctionCallCollector`, a hand-built section for the calls the parser
  cannot write, and the halt and feature branches of the orchestrator.

**Tests:** `cargo test -p dixscript --test quickfuncs_analyzer_document`. 31 tests, all passing on Rust 1.75.

### `tests/security_analyzer_document.rs`

**What it does:** Pins the diagnostics of the `@SECURITY` analyzer and checks that it gives the same
ones when it reads a `Document` as when it reads the owned tree. The parser tests in
`security_section_tests.rs` stop at the parse, so nothing else showed which warnings an analysis
produced.

**Decisions:**
- **Thirty-eight sources, three strategies.** The sources cover every warning `SEC_WARN001` to
  `SEC_WARN008`, all three modes, every algorithm class, the key derivation parameters (too low, the
  wrong type, a long), keystore ranges at both ends, upper case block names, repeated keys, repeated
  blocks, and `@SECURITY` next to `@DATA`, `@RAW` and `@DLM`. The comparison test runs each with and
  without a document under `halt`, `continue` and `recover` (114 comparisons).
- **The expected warnings were recorded, not derived.** The `RECORDING` table was written from the
  output of the analyzer before it moved, captured by the before and after harness (see the last
  Fixes entry). The strategy changes nothing here because the analyzer only warns, so each source
  has one recorded case.
- **`SEM0002` is reached through a pair that disagrees.** The parser adds a default `@SECURITY`
  section when `@DLM` asks for an encryptor, so a parsed file never lacks one. The test takes the
  section away from the owned tree, or attaches a document that has none, to show that the check
  reads the same tree as the section.
- Two tests build a document and an owned tree that disagree on purpose, one in each direction, as
  `raw_analyzer_document.rs` does.

**Tests:** `cargo test -p dixscript --test security_analyzer_document`. 12 tests, all passing on Rust
1.75. Eleven deliberate bugs were injected to check them (see the last Fixes entry).

### `tests/schema_analyzer_document.rs`

**What it does:** Pins the diagnostics of the `@SCHEMA` analyzer and checks that it gives the same
ones when it reads a `Document` as when it reads the owned tree, through all three entry points.

**Decisions:**
- **Thirty-one sources, three strategies.** The sources cover every error `SCH001` to `SCH007` and
  the empty-schema warning, every value kind the analyzer classifies (integer, long, float, double,
  string, bool, hex, date, timestamp, array, object, tuple, blob, regex, enum, null and a deferred
  call), every path form (a property, an object, a table, a group array and a prefix of either),
  explicit type annotations, repeated data names, unicode string lengths, and a QuickFunc value that
  is only known after resolution. The comparison test runs each with and without a document (93
  comparisons), and a second test compares `analyze`, `analyze_doc` and `validate_resolved`.
- **The expected diagnostics were recorded, not derived.** The `RECORDING` table has a case per
  source under `continue`, plus one under `halt` or `recover` where that strategy changes the
  outcome (49 cases). It was written from the output of the analyzer before it moved.
- **Two inputs are built by hand.** No parser produces a `NestedArray` value, so the recording can
  never reach that arm, and the long-value range test needs a number the recording did not hold. Both
  are written as explicit tests.
- Four tests build a document and an owned tree that disagree, for the schema and for the data, in
  each direction.

**Tests:** `cargo test -p dixscript --test schema_analyzer_document`. 17 tests, all passing on Rust
1.75. Twenty-two deliberate bugs were injected to check them (see the last Fixes entry).

### `Compiler/Core/SectionEnhancers/qualified_identifier_resolver.rs`

**What it does:** Replaces the qualified identifiers in function bodies with the node semantic
analysis found each one to be: a local or imported enum access, a static call or constant, an
imported function call, an instance method call or a property chain, or a namespace enum reference.
When no resolution was recorded (the analyzer does not visit lambda bodies) it chooses by shape: a
two part call on a static object is a static call, any other two part call is an instance method
call, a longer call is a call of a function with a dotted name, and a name with no arguments is a
property chain.

**Decisions:**
- **One implementation, over the arena.** `resolve_statement_list`, `resolve_statement_id` and
  `resolve_expression_id` read nodes through an `Arena` and allocate the nodes that replace them. The
  owned `resolve_statement`, `resolve_statements` and `resolve_expression` copy their argument into a
  scratch arena, call those and copy the result out, so the two cannot disagree.
- **A node whose children did not change is kept, not rebuilt.** The owned resolver rebuilt every
  node it walked. Here a node is allocated again only when a child id changed, so a body with nothing
  to resolve comes back as the same run of statement ids and uses no arena space. A qualified
  identifier always changes.
- **What the resolver does not enter is kept as it was.** Casts, built-in and static function calls,
  plain function calls, nested arrays, ranges and a quick function call written as a value are copied
  unchanged, so a qualified identifier inside one of them stays unresolved. That is what the owned
  resolver did and the recorded output depends on it.
- **A dotted identifier value with exactly one dot becomes an enum value;** one with two or more dots
  or none stays an identifier.

**Tests:** `tests/quickfuncs_enhancer_document.rs`.

### `Compiler/Core/SectionEnhancers/quickfuncs_ast_enhancer.rs`

**What it does:** Gives every parameter that has a type and no default the synthetic default of its
type, and runs the resolver over each function body. The resolver is built once per section, and only
when semantic analysis recorded at least one resolution.

**Decisions:**
- **`enhance_doc(&DocQuickFuncs, &mut Arena, Option<&SectionAnalysisResult>) -> DocQuickFuncs`
  holds the work.** The result shares every node that did not change with the input section, and the
  input section still means what it meant. `enhance(&QuickFuncsSection, ...)` copies into a scratch
  arena, calls it and copies the result out.
- **The default of a parameter comes from `TypeSystemManager::synthetic_parameter_default`,** which
  `apply_defaults_to_parameters` now calls too, so both produce the same value and write the same debug
  line. The default of a date is today's date and the default of a timestamp is the time of the run, so
  two runs of the same file differ in those two values.

**Tests:** `tests/quickfuncs_enhancer_document.rs`.

### `Compiler/Core/general_ast_enhancer.rs`

**What it does:** Runs the section enhancers over a copy of the owned tree and counts what they did.
Only `@QUICKFUNCS` has one.

**Decisions:**
- **`with_document(&mut Document)` makes the enhancer read the section from the document.** Whether
  there is a section is decided by the tree that is read, as in the semantic analyzer: with a document
  the document says, without one the owned tree does. The enhanced section is exported out of the
  document's arena and set in the copy of the owned tree; the rest of that tree is a clone, as before.
- **The document is `&mut` because the enhanced nodes are allocated in its arena.** They are added
  after the existing ones, so the document is still valid and means the same.
- **Imported files and the language server still call `enhance` without a document,** so they take
  the owned path through a scratch arena.

**Tests:** `tests/quickfuncs_enhancer_document.rs`.

### `tests/quickfuncs_enhancer_document.rs`

**What it does:** Pins the enhancer's output to what the owned-tree enhancer produced before the
change, and checks which tree is read.

**Decisions:**
- **The recording is a fingerprint, not the output.** `RECORDING` and `HAND_RECORDING` hold, for each
  crafted source and each hand-built case, the number of defaults applied and a 64 bit FNV-1a hash of
  the enhanced section's debug text, produced by running the original code. A failure prints the
  section. Quoted dates and timestamps are masked before hashing, because the default of a date or
  timestamp parameter is the day or time of the run; a separate test checks their shape.
- **The 25 crafted sources** cover static and instance calls, property chains, local enums, lambdas,
  values in collections, interpolation, control flow, typed, untyped and defaulted parameters, many
  functions, a file with no section and a file with an empty one. **The hand-built cases** (ten, in
  three modes each: the recorded resolutions, an analysis result with none, no analysis result) reach
  every resolution type, the fallback, a key that does not match, every expression, value and
  statement form the resolver walks and the ones it does not, and every parameter type.
- **Which tree is read is tested in both directions,** with a document and an owned tree that
  disagree on purpose.
- **Sharing is tested by counting:** a section with nothing to enhance leaves the arena exactly as
  large as it was, and only the path to a resolved identifier is allocated.

**Tests:** `cargo test -p dixscript --test quickfuncs_enhancer_document`. 15 tests, all passing on
Rust 1.75.

### `tests/orchestrator_sections_document.rs`

**What it does:** Shows which tree the orchestrator reads `@IMPORTS`, `@ENUMS` and `@DLM` from. Each
test gives the analyzer an owned tree and a document that disagree on purpose, one with the section
and one without, attaches the document or not, and checks what the result holds.

**Decisions:**
- **Both directions for each section.** The document has it and the tree does not, then the other way
  round, each with the document attached and detached. A read that ignores the document, or ignores
  the tree, fails one of the four.
- **The feature checks are reached by switching the features after parsing,** because the parser itself
  refuses a section whose feature is off. The imports check runs in two phases, so the error is reported
  twice; the test pins that.
- **The encryptor requirement is reached by taking the generated `@SECURITY` section out** of the tree
  that is read, since the parser adds a default one whenever `@DLM` asks for an encryptor.
- **Equality with the code from before the change is not repeated here.** It was checked once, over the
  corpus and 75 crafted sources with the features on and off (see the Fixes entry).

**Tests:** `cargo test -p dixscript --test orchestrator_sections_document`. 7 tests, all passing on
Rust 1.75.

### `tests/version_constraints_document.rs`

**What it does:** Pins the result of `VersionConstraints` for six hand-built scripts under 24 `@CONFIG`
variants to what the code before the change gave, and checks that the owned and document forms agree.

**Decisions:**
- **The variants are the point.** The parser refuses a section whose feature is off, so the checks that
  gate sections on the `features` setting cannot be reached from text. The tests replace the config of
  a parsed or hand-built script with 24 variants instead: feature lists and strings, advanced and basic,
  unknown names, a number where a string belongs, first entry wins, versions, debug modes, no config and
  an empty one.
- **`RECORDING` holds, for each script, a fingerprint of its 24 result lines,** produced by running the
  original code, and the test checks it through both forms. The six scripts cover every shape of data
  entry (with a call in a table, a group and a plain property, and an empty path), quick function
  parameters with defaults that convert and that do not, every `@DLM` module and codec, and `@IMPORTS` and
  `@ENUMS` on their own and together with everything else.
- **The corpus is compared form against form** under the same 24 variants (84 files, and at least 200
  of the results are errors, so the variants do bite).
- **Which tree the version phase reads** is tested with an owned tree and a document that disagree.

**Tests:** `cargo test -p dixscript --test version_constraints_document`. 5 tests, all passing on Rust 1.75.

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
- The singleton was a `std::sync::LazyLock`, which needs Rust 1.80. It is now the
  crate's own `Lazy` (see [utilities.md](utilities.md)), so the manager builds on 1.75.
- A file that declared `version -> "2.0.0"` was accepted by the config schema (its
  version regex allows any `N.N.N`) but the manager silently treated it as 1.0.0, and
  `initialize_singletons` logged a version mismatch. 2.0.0 is now a real version.
- `Core/Tokenizer/lexer.rs` kept a fast path only for versions starting with `1.`.
  A manager at 2.0.0 would have sent every token down the slow per-token feature
  check. The fast path now covers `2.` as well.

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

### `@SCHEMA` wiring, and the nick-nacks found while doing it
- `general_parser.rs`'s `is_section_keyword_token` listed neither
  `SectionRaw` nor (naturally) `SectionSchema`. That function decides where
  an unclosed section ends, so an `@RAW` following an unclosed section would
  have been swallowed as part of it. Both added.
- `config_token_splitter.rs`'s stop-scanning list for an unclosed `@CONFIG`
  was missing `SectionRaw`; added alongside `SectionSchema`.
- The lexer's `scan_raw_content_block` doc comment, `token.rs`'s
  `RawContent` doc comment, `raw.rs`'s `RawContent` doc comment, and
  `raw_section_analyzer.rs`'s `RAW003` suggestion all showed the wrong
  `---tag---` (trailing dashes) form — the very shorthand that misled the
  original `@RAW` tests and benchmark generator. Corrected to the real
  `---tag`-newline form from `others/raw_section_spec.md`.
- `config_schema.rs`'s `validate_features` allowlist did not contain `"raw"`
  (or `"schema"`), so `features -> "raw"` failed validation and was silently
  replaced by the default (`advanced`, which unlocks everything) — meaning
  `raw_section_allowed_with_explicit_feature` passed vacuously and never
  exercised the explicit-feature path. Both are now in the list. **Behaviour
  change to watch in CI:** that raw test now genuinely runs with
  `features = raw` only.
- `version_constraints.rs`'s `ValidFeatureControls` constraint list was
  missing `"raw"`; added with `"schema"`.
- Adding `DixScript.schema` touched the same 20 struct-literal construction
  sites `DixScript.raw` did (`schema: None`), found by grepping for
  `raw: Vec::new()` rather than waiting for CI. `DixScript::with_sections`
  gained a `schema` parameter (it has no callers).
- `SectionId::Schema` was appended after `Raw`, so no existing variant's
  numeric value moves.

### Open items
- `mdix-lsp/src/features/goto_definition.rs`'s `section_keyword_len` is an
  exhaustive `match` on `SectionId` with no wildcard and no `Raw` arm. It
  compiles today only because `mdix-lsp` depends on the published crates.io
  `dixscript` 1.0.0, which predates `Raw`. When that dependency moves to a
  release containing `Raw` and `Schema`, it will stop compiling. Not touched
  here: adding the arms now would break the current build.
- `mdix merge` does not merge `@SCHEMA` (same gap as `@RAW`).
- Editor support for `@SCHEMA` (VS Code grammar, LSP hover/semantic tokens)
  is not done.

### `Compiler/Utilities/identifier_pattern_analyzer.rs`

**What it does:** Looks ahead in the token stream to recognise identifier patterns
(for example whether a name is followed by a call).

**Fixes and Problems:**
- Two lookahead checks used `Option::is_none_or`, stable only since Rust 1.82. They are
  now `map_or(true, ..)`, which means the same thing and builds on 1.75.

### `Compiler/ImportsResolution/imports_resolver.rs`

**What it does:** Resolves `@IMPORTS` entries recursively, with cycle detection and
cloud download. For each imported file it reads the text, tokenizes and parses it, resolves the
file's own imports first, analyzes and enhances it, and registers its global functions (the
enhanced tree of each, with its parameter defaults) and its enums as a namespace of the symbol table.

**Decisions:**
- **An imported file is parsed to a `Document`, and the analyzer and the enhancer read it.**
  `read_and_parse_raw` calls `parse_document` and exports the owned tree once, which the nested
  import walk and the extraction of functions and enums still read. `analyze_and_enhance` gives the
  document to `GeneralSemanticAnalyzer::with_document` and to `GeneralAstEnhancer::with_document`,
  the path the loader takes for the main file. Before this, each imported file was parsed to a
  document, exported, and then copied back into scratch arenas by the owned entry points of every
  analyzer and of the enhancer.
- **A file the caller parsed itself takes the owned path.** `resolve_imports`, which takes
  already parsed trees, passes no document down, so its analysis and enhancement go through the owned
  entry points as they always did. A file with no content has no document either.
- **The two paths cannot show up in the result.** The document and the owned tree of an imported
  file agree by construction, so the namespaces and the errors are the same whichever way they were
  read; the tests pin the result and compare the two entry points with each other.
- **The gain is small.** Callgrind on a debug build of the 5 file chemistry import chain counts
  1,294.8 million instructions before and 1,292.3 million after, about 0.2% fewer, which is within what
  the harness itself spends. Nothing else was measured.

**Fixes and Problems:**
- The cloud-import error path used `Result::inspect_err` (Rust 1.76). It is now
  `map_err` with the same side effect that returns the error unchanged, which builds
  on 1.75.

**Tests:** `tests/imports_resolver_document.rs`.

### `tests/imports_resolver_document.rs`

**What it does:** Pins what `ImportsResolver` registers for 16 crafted files, and checks that the
two ways in give the same namespaces.

**Decisions:**
- **The files are written to a temporary directory at run time,** because the resolver reads imports
  from disk. The directory is replaced by `<DIR>` in everything that is fingerprinted, since the error
  messages carry absolute paths.
- **`RECORDING` holds, for each file, the number of namespaces, the number of errors and a 64 bit FNV-1a
  hash of the dump,** which was produced by running the original code. The dump is the namespaces in
  full (every function with its signature and enhanced tree, every enum, the local imports) and the
  sorted imports errors. It is checked with the main file's analyzer given its own document and not.
- **The 16 files** are a library with global and scoped functions, a default value, a lambda and an
  enum; files that import it directly, twice over (a diamond) and through each other; an empty, a
  missing, an unparsable and an unanalyzable import; a file that imports itself; and one file with
  a good import, a bad one and a good one after it.
- **The caller-parsed entry point is compared with the resolver reading the files itself** for two of
  them, which is the only place the owned path is exercised on files with nested imports.

**Tests:** `cargo test -p dixscript --test imports_resolver_document`. 5 tests, all passing on
Rust 1.75.

### `Utilities/parser_collection_helper.rs`

**What it does:** Up-front capacities for the vectors the section parsers build.
`section_estimate` sizes the one vector that holds a whole section and is not capped.
`bounded_estimate` and the four `estimate_*_count` helpers size vectors inside a
construct and are capped at `MAX_RESERVED_ELEMENTS` (16).

**Decisions:**
- A parser knows the length of its whole token stream, not the size of the construct it
  is parsing. Sizing every vector from the stream length reserved thousands of elements
  for each call argument list, array, object and statement block. Compiling the 445 KB
  `elements_database.mdix` requested 7.50 GB. With the cap it requests 159 MB.
- The four `parse_section` sites (data entries, quickfuncs functions, DLM modules, enum
  declarations) keep the uncapped estimate. There the stream length is a fair guide and
  the reservation happens once per section.
- 16 comes from the experiment in `arena-ast.md`. No other value was measured.

**Tests:** `cargo test -p dixscript --lib parser_collection_helper`, and the allocation
budget test below.

### `Compiler/Core/SectionParsers/` (data, dlm, enums, quickfuncs)

**What it does:** One parser per section: `data_section_parser.rs` for `@DATA` (it has its own
section below), `quickfuncs_section_parser.rs` for `@QUICKFUNCS` (it has its own section below),
`dlm_section_parser.rs` for `@DLM` and `enums_section_parser.rs` for `@ENUMS`.

**Decisions:**
- Every token-length-derived reservation goes through the shared helper above.
  `parse_section` uses `section_estimate`, nested constructs use the capped estimates.
    This was the only change in the `@DLM` and `@ENUMS` files, so their parsing logic is
  untouched.

### `Compiler/Core/SectionParsers/data_section_parser.rs`

**What it does:** Parses one `@DATA(...)` into a `DataSection`: simple properties, table
properties, group arrays and object properties, with scalars, arrays, objects, tuple, blob and
regex constructors, identifiers, enum values and function calls (local and imported) as values.

**Decisions:**
- `parse_section_doc` is the real parse and returns the `DocData` section with its nodes left in the
  parser's own arena, and `into_arena` returns that arena. `parse_section` is a wrapper that exports,
  so existing callers are unchanged, and `general_parser.rs` uses the first two to merge the arenas.
- This is the fourth parser built on the arena AST. The parser owns an `Arena`, builds every value
  and expression there as a node, keeps the entries as `DocDataEntry` records that hold the ids,
  and exports the finished section once, at the end of `parse_section`, through
  `DocData::export`. The public interface and the result are unchanged.
- **Lists use scratch stacks.** A list is complete only when its closing token is reached, so
  array literals, tuple constructors and group arrays push the ids of their finished children on
  `scratch_values`, object literals push `ObjectPropNode`s on `scratch_props`, and the four call
  forms push `ExprId`s on `scratch_exprs`. The list is stored in the arena once, with
  `alloc_value_list`, `alloc_prop_list` or `alloc_expr_list`. This avoids a temporary `Vec` per
  list, which the owned parser had to allocate and keep.
- **Every way out of a list truncates its stack.** A list parser is a thin wrapper that records
  the stack height, calls the real parser (`..._inner(base)`) and truncates back to the height
  whatever the result is. The parser has many early returns, and a list that failed part way
  must not leave entries for the list that encloses it. The enclosing list would take them as its
  own children, and because the leaked ids name nodes that already contain the same ids, the
  exported tree grows exponentially. The tests below show it: with the truncation removed, a
  66-deep array exhausts memory.
- `current()` and `peek_ahead()` return `&'a Token`, a reference into the token slice and not
  into the parser, so a token can be held while the arena is written. That is why the
  conversion needed no copies of the current token.
- A function call in a value position builds a `ValueNode::QuickFuncCall` whose arguments are
  expressions, and an argument that is a literal builds two nodes, a value and an expression
  that wraps it (`expr_value`). The fallbacks that the owned parser wrote as `unwrap_or(...)`
  build their node only when it is needed (`expr_value_or`).
- A call that fails part way leaves its nodes unreferenced in the arena. This is harmless: the
  arena is append-only and is dropped with the parser.
- The public items now carry doc comments, the module documentation describes the arena and the
  scratch stacks, and the em dashes were removed from the comments.

**Tests:** `tests/data_parser_tests.rs` pins the parser itself. Every file in `mdix_files/` that
compiles also goes through this parser in `tests/arena_roundtrip.rs`, `tests/arena_document.rs`
and `tests/arena_document_display.rs`.

### `Compiler/Core/SectionParsers/quickfuncs_section_parser.rs`

**What it does:** Parses one `@QUICKFUNCS(...)` into a `QuickFuncsSection`: functions of the form
`~name<type> => scope (parameters) { body }`. A body holds statements (`let` and `const`,
assignments, `log:`, `if:`/`elif:`/`else`, `chk:` switches with a `miss` case, `return` and
expression statements). They are built from an operator-precedence expression parser with unary
operators, ternaries, postfix chains (members, indexes, calls), literals, arrays, objects, tuple,
blob and regex constructors, interpolated strings and lambdas.

**Decisions:**
- `parse_section_doc` is the real parse and returns the `DocQuickFuncs` section with its nodes left in the
  parser's own arena, and `into_arena` returns that arena. `parse_section` is a wrapper that exports,
  so existing callers are unchanged, and `general_parser.rs` uses the first two to merge the arenas.
- This is the fifth and last section parser built on the arena AST. The parser owns an `Arena`,
  builds every value, expression and statement there as a node, keeps the functions as
  `DocQuickFunction` records that hold the ids, and exports the finished section once, at the end
  of `parse_section`, through `DocQuickFuncs::export`. The public interface and the result are
  unchanged.
- **Lists use scratch stacks, as in the `@DATA` parser.** Statement blocks and lambda block bodies
  use `scratch_stmts`, function arguments use `scratch_exprs`, array literals and tuple
  constructors use `scratch_values`, and object literals use `scratch_props`. Each list parser is
  a thin wrapper that records the stack height, calls the real parser (`..._inner(base)`) and
  truncates back to the height whatever the result is. Without the truncation a failed list leaks
  entries into the list that encloses it, and the exported tree grows exponentially (the tests
  show a 66-deep array or call exhausting memory).
- `current()` returns `&'a Token`, a reference into the token slice and not into the parser, so a
  token can be held while the arena is written.
- **The `elif` chain is built once.** The owned parser built an `If` statement for every `elif`
  and then took them apart in reverse to nest them. The arena version collects the condition, body
  and position of each `elif` and builds the nested `If` nodes in reverse, which gives the same
  tree without the intermediate statements.
- **Code that inspects what it built reads a copy of the node.** Postfix handling and value
  conversion copy the node out of the arena (`*self.arena.expr(id)`, because nodes are `Copy`)
  before they write, so no borrow of the arena is held across a write. `convert_expression_to_value`
  reuses the identifier's text span and does not copy the text.
- **Interpolated strings share the arena.** A placeholder is parsed by a sub-parser over its own
  tokens. The sub-parser is handed this parser's arena for the call (`std::mem::swap`) and gives it
  back, so the id it returns names a node in the same arena. No second arena has to be imported
  afterwards.
- A call on an index access (`a[0](1)`) still wraps the index access as the instance of a
  `__call__` method call, as before.
- A switch's `miss` case is stored with `alloc_case` and its ordinary cases as one run with
  `alloc_case_list`. The arena gained `alloc_stmt_list`, `alloc_str_list`, `alloc_case` and
  `alloc_case_list` for this parser.
- The public items now carry doc comments, the module documentation describes the arena and the
  scratch stacks, and the em dashes were removed from the comments.

**Tests:** `tests/quickfuncs_parser_tests.rs` pins the parser itself. Every file in `mdix_files/`
that compiles also goes through this parser in `tests/arena_roundtrip.rs`,
`tests/arena_document.rs` and `tests/arena_document_display.rs`.

### `tests/parser_alloc_budget.rs`

**What it does:** Counts the bytes one compile of `tests/fixtures/alloc_budget.mdix` asks
the allocator for, and fails above 20 MB.

**Decisions:**
- It counts requested bytes because that figure is deterministic and wall time is not.
- The binary holds one test. The counter is process-wide, so a second test running in
  parallel would be counted too.
- The fixture is a copy of `mdix_files/advanced/FullFunctionTest.mdix` kept inside the
  crate, so the test also runs from the published package.
- The budget sits between 13.2 MB (with the cap) and 88.0 MB (without it). Both figures
  were measured by running this test.

**Tests:** `cargo test -p dixscript --test parser_alloc_budget`

### Fixes and Problems: parser reservations

- The module comment of `parser_collection_helper.rs` said pre-allocation cut allocations
  by 15 to 20 percent. The allocation count does not change with the cap (92,794 before
  and after for `FullFunctionTest`). What changes is the bytes requested, and with them
  the parse time.

### `Compiler/Core/ValueResolution/value_resolver.rs`

**What it does:** Resolves values at compile time in five phases: enum pre-resolution, the
initial data context, discovery of quickfunction calls, iterative execution of those calls
with the result written back into the AST (phase 4), and the remaining identifier
references. Phases 1 to 3 are thin layers over `enum_resolution.rs`, `data_context.rs` and
`ast_walker.rs` and read the arena; phases 4 and 5 and the interpreter still read and rewrite
the owned tree.

**Decisions:**
- Phase 4 finds where every call sits in the data tree once, before its loop, as a path of
  child indices per call position. Writing a result back then costs the depth of the node.
  The path is checked against the node's position before it is used, and if an earlier
  write-back changed the tree under the index the old search runs instead, so the result is
  the same as searching every time. A position that was not in the tree when the index was
  built is left alone, which is what the old search would also have done.
- Only the positions of the calls being resolved are indexed, so the index costs one small
  vector per call and not one per node.
- **Phases 1 to 3 run on the arena and read one data source.** `ValueResolver::with_document`
  gives them the `Document` the file was parsed into, and they then read the document's `@DATA`
  in the document's arena (phase 1 rewrites it there). Without a document the resolver copies the
  owned data into a scratch arena once, when it starts, and the three phases share that copy; this
  is what the language server and `ValueResolver::new` callers get. The source is a private
  `DataSource`, and whether a data section exists is decided by the tree that is read, so a
  document without `@DATA` leaves phases 2 and 3 with nothing to find even when the owned tree has
  data.
- **The owned data is refreshed only when it has to be.** After the walk the owned section the
  later phases read is replaced by an export of the resolved section when the walk changed
  something, and also when the owned tree had no data of its own. A file with no enum
  expression in its data, which is most files, is neither rebuilt nor exported.
- **The document is updated too.** When the walk changed something the document's `@DATA`
  holds the resolved section afterwards, so a later phase that moves to the arena finds it
  there. `@ENUMS` is not updated in the document: the merge of imported enums below goes to the
  owned tree, which is the one the result is built from.
- **Imported enums are merged into the owned `@ENUMS` only through the dotted value form.** A
  data value written `Types.Priority.HIGH` is checked and recorded, and the whole enum is added
  to `@ENUMS` under its qualified name (the section is created when the file has none). An enum
  access inside an expression is checked and becomes an integer but records nothing. This is how
  it always behaved and it is pinned by a test; the merge itself is unchanged.
- **Dead code removed.** `resolve_call_arguments`, `resolve_expr_to_dix` and
  `resolve_value_to_dix` (about 240 lines, private) were never called; the compiler reported
  them as unused and only they referred to each other. They are gone.
- **More removed with phase 2.** The owned `try_value_to_dix` (about 110 lines; phase 2 was its
  only caller) is replaced by the arena version, the alias pass no longer clones the whole entries
  vector to iterate it, and the conversion of an object literal no longer rebuilds its `HashMap` a
  second time on the way into `DixValue::from_object`.

**Tests:** `cargo test -p dixscript --test value_resolver_enums_document`,
`cargo test -p dixscript --test value_resolver_context_document`,
`cargo test -p dixscript --test value_resolution_scaling`, and the whole suite.

### `Compiler/Core/ValueResolution/enum_resolution.rs`

**What it does:** Phase 1 of value resolution over the arena. `resolve_data_enums` walks a
`DocData`, checks every enum reference against the symbol table and rewrites the references
that must become integers, allocating the new nodes in the arena and replacing only the entries
that changed.

**Decisions:**
- **Rewriting is by allocation, and unchanged nodes keep their ids.** A node whose children did
  not change returns its own id, and a list of children is allocated again only from the first
  element that changed. A data section without an enum expression therefore allocates nothing,
  which a test checks by the arena's value count, expression count and byte count.
- **The walk replaces the owned walker of about 890 lines** that rebuilt every entry with the
  same shape repeated for each kind it entered. Sharing makes the rebuild decision one
  comparison of ids, and one generic list walker serves the value and the expression lists.
- **Two quirks of the owned walker are kept on purpose.** A bare enum value at a data position
  is checked and left in place (later stages need the enum), while an enum access inside an
  expression becomes an integer literal. And a nested array that held any enum reference comes
  back as a plain array, as it always did, even when the reference itself was left in place; the
  walk counts the references it sees to know this, because an unchanged element does not say it.
- **Only the expression kinds that can lead to an enum access are entered:** value wrappers,
  the three call kinds, the four binary operators, unary, conditional, parenthesized, property
  and index access. Instance method calls, casts, built-in and static function calls, lambdas,
  ranges and the rest are returned as they are, so a bad reference inside one is not reported.
- **An object entry is only walked when its value is an object literal,** as before.
- **The first bad reference ends the walk with its error and leaves the section as it was.**
  Nodes allocated before the error stay in the arena, unreferenced.
- The error messages and locations are exactly the original ones, for the two ways a value
  reference is written and the two ways an expression reference is written.

**Tests:** `cargo test -p dixscript --test value_resolver_enums_document`

### `tests/value_resolver_enums_document.rs`

**What it does:** Pins phase 1 of value resolution to the behavior of the owned walker it
replaced, and checks the properties of the new one.

**Decisions:**
- **`RECORDING` and `HAND_RECORDING` hold a fingerprint of the whole result for each input,**
  produced by running the original code (the original files were put back, the test built and run
  with the one line that attaches a document turned into a no-op) and then required of the new
  code through the document and through the owned entry point. The inputs are 13 crafted sources,
  two corpus files, and 38 hand-built data sections run against two bases, so 15 and 76 results.
- **The hand-built sections reach what the grammar cannot write.** `@DATA` has no arithmetic,
  conditional, property access, interpolation or call-argument expressions, so every expression
  kind, the kinds that are not entered, nested arrays of every depth, lists that change at the
  first, a middle and the last position, an operator that changes on one side only, tables,
  groups, object entries, wrapped values and 21 sections with a bad reference in different
  positions are built by hand.
  Each node gets a position of its own, so a rewrite that mixes positions up changes the result.
- **The error messages are asserted in full** for all nine ways a reference can be wrong, through
  both paths, and the first error in data order is the one reported.
- **Which tree is read** is tested with an owned tree and a document that disagree, in both
  directions, with a document that has no data, and with an owned tree that has none.
- **Sharing is tested by the arena's counters:** data with nothing to resolve allocates nothing,
  not even a list, and data with one change allocates the two values and one expression it
  needs and leaves the entries around it on their old ids.
- **A debug-log test** checks the counts and the merge message, since the counts also decide
  what happens to a nested array. The message for a missing data section is emitted at debug
  level and did not appear in either debug mode of the test's error manager, so it is not asserted.
- Sources that call a function returning an object, or a random or clock builtin, are left out of
  the recording, because their output cannot be fingerprinted. The `@CONFIG` entries, kept in a
  hash map, are sorted before fingerprinting.
- **Equality with the original is not argued from the tests alone.** The recording was made from
  the original code. Of the 11 tests that are not recording checks, the 8 that do not need a
  document were also run on the original and pass there; the 3 that need one cannot.

**Tests:** `cargo test -p dixscript --test value_resolver_enums_document`. 15 tests and one
ignored recorder (`print_recording`), all passing on Rust 1.75.

### `Compiler/Core/ValueResolution/data_context.rs`

**What it does:** Phase 2 of value resolution over the arena. It builds the data context, the
map from the path of every literal in `@DATA` (`DATA.server.port`, `DATA.items[2]`) to its runtime
value, and then resolves the aliases (`tax = base_price`) against it. `populate_entry` and
`resolve_aliases_in_entry` read one `DocDataEntry` and an `Arena`; `try_value_to_dix` converts one
literal.

**Decisions:**
- **Insertion order is the order of the data section, depth first.** The context is a hash map whose
  iteration order decides which key `lookup_by_name_or_suffix` finds first when a name matches the
  end of several paths, so the order of insertion is part of the behavior and is kept.
- **A container is in the context next to its parts, but only whole when every part converted.** An
  array with a call in it has its other elements in the context and is not there itself; an empty
  array is not inserted and an empty object is; an object is inserted after its members.
- **What converts.** The scalar literals, an enum value as the integer 0 (the enum pass has
  checked it), the template of an interpolated string as text, and a date or timestamp that does not
  parse as the time of the run. A prefixed constructor converts only for `b` (blob, empty without a
  string argument), `r` (regex, `.*` without one) and `t` (tuple, at most six elements), whatever
  the case of the prefix, and a call, an expression or an unresolved identifier does not convert.
- **Aliases resolve in up to eight passes.** A property whose value is a plain identifier is
  looked up by its exact key, then by the end of a path (`".name"`, the first match). It is skipped
  once its own path is in the context. A chain declared backwards needs one pass per link, so the
  ninth link of a chain does not resolve; a cycle and a self reference never do; with two
  properties of one name the last wins, and an alias to it follows.
- **The alias pass does not copy the data section.** The owned version cloned the whole entries
  vector to get past the borrow of the tree; the arena is only read, so there is nothing to
  clone.

**Tests:** `cargo test -p dixscript --test value_resolver_context_document`

### `Compiler/Core/ValueResolution/ast_walker.rs`

**What it does:** Phase 3 of value resolution: finds every quickfunction call in `@DATA`, with its
place (location, scope and the variables in scope), and hands each to the interpreter as a
`FunctionCallInfo`. `find_all_in(&Arena, &DocData)` is the implementation; the owned
`find_all(&DataSection)` copies the section into a scratch arena and calls it.

**Decisions:**
- **Only the arguments of a found call are copied out of the arena,** once each, into the owned
  `Expression`s the interpreter takes. The rest of the walk only reads.
- **What the walk enters is part of the contract and is pinned by tests.** In a value, a call is
  found together with the calls in its arguments; arrays and nested arrays, objects, interpolated
  strings, constructor arguments, both ends of a range and a lambda body are entered. In an
  expression, the binary operators except the bitwise ones, unary, conditional, property and index
  access, static and instance method calls, and the arguments of an imported function call are
  entered. A call inside the arguments of a call that is itself in an expression is not found, and
  neither is anything under a bitwise operator, parentheses, a cast, a built-in, static or plain
  function call, a qualified identifier or a value wrapped in an expression.
- **A name is split only for an imported namespace and one dot** (`Base.scaledValue`); an unknown
  namespace and a name with two dots stay whole, and a trailing dot gives an empty function name
  in the imported namespace.
- **Scope is reset at the start of each entry,** so an unbalanced exit inside an entry cannot reach
  the next one. That is also why the table's own `clear_scope_variables` call was redundant, and
  it was removed.
- The file had no NOTICE header, and its variant-name function matched `BitwiseOp` twice (the
  compiler's unreachable pattern warning). Both are fixed.

**Tests:** `cargo test -p dixscript --test value_resolver_context_document`

### `tests/value_resolver_context_document.rs`

**What it does:** Pins phases 2 and 3 of value resolution to the behavior of the owned versions
they replaced, and checks the properties of the new ones.

**Decisions:**
- **Four recordings, all produced by running the original code** (the original files were put back
  and the test built and run, with the one line that calls the arena finder turned into a no-op),
  then required of the new code. `RECORDING` fingerprints the whole result of the resolver,
  history of calls included, for five crafted sources and four corpus files; `HAND_RECORDING`,
  `CONTEXT_RECORDING` and `WALK_RECORDING` do the same for the result, the data context and the
  found calls of 15 hand-built data sections. The results are required through the document and
  through the owned entry point.
- **The hand-built sections reach what the grammar cannot write:** every value kind the context
  converts, constructors, containers, tables and groups, alias chains past the pass limit, and
  every expression kind the call finder enters or does not, with the quickfunctions they call
  defined in the base.
- **The data context is read from the verbose log,** which prints every path and value sorted.
  Objects print their entries in hash-map order, so they are sorted before fingerprinting, and the
  value of a date or timestamp that did not parse is replaced by a marker, since it is the time of
  the run.
- **The rules are asserted outright as well:** what converts and what does not, how aliases resolve
  and stop (in tables, groups, by full key, by the end of a path against decoys that only end the
  same way), which expressions the finder enters and which it does not, how names split, the
  locations in tables, groups, arrays and objects, and what the log says about each pass and each
  count.
- **Which tree is read** is tested with an owned tree and a document that disagree, with a document
  that has no data, and with an owned tree that has none. The owned `find_all` is compared with the
  arena `find_all_in` for every case, and phases 2 and 3 are shown to allocate nothing in the
  document's arena.
- Of the 16 tests that are not recording checks, 14 were also run on the original and pass there;
  the two that need the arena finder or a document to be read in phases 2 and 3 cannot.

**Tests:** `cargo test -p dixscript --test value_resolver_context_document`. 23 tests and three
ignored helpers (`print_recording`, `print_contexts`, `print_logs`), all passing on Rust 1.75.

### `tests/value_resolution_scaling.rs`

**What it does:** Compiles a generated source of 400 resolved calls and one of 1,600 and
requires the larger to take less than 7 times as long.

**Decisions:**
- It compares two timings from the same process, so machine speed cancels out. Linear
  growth gives about 4.3 and a search of the whole tree per call gives about 10; the limit
  of 7 sits between them. Each size is timed three times and the fastest is used.
- Each call follows a data entry of 100 values, so a whole-tree search per call has a lot to
  walk past. With 20 values the old behavior only reached 7.6 and the test passed it.

**Tests:** `cargo test -p dixscript --test value_resolution_scaling`

### Fixes and Problems: value resolution

- **Quadratic write-back.** `replace_value_in_ast_by_location` searched the whole data tree
  for the node at each resolved call's position, depth first, and ignored its `location`
  argument. With N calls in a tree of N nodes that is N squared. A 4x larger input took 16x
  as long in that function. The suspect in `arena-ast.md` (the formatting and timestamp in the
  loop) was not the cause. Found by comparing callgrind profiles of a 4x and a 16x input.

### Fixes and Problems: arena AST pilot

- **`Compiler/AST/mod.rs`:** a comment on the `ElemType` re-export read "new typed-collection
  element type", which is history rather than explanation. It was removed.
- **`Compiler/Core/SectionParsers/security_section_parser.rs`:** the parser had no tests and no
  documentation section. The conversion was checked with a throwaway before and after harness:
  the loader's tokenize, config and parse stages run over the 84 corpus files and 28 crafted
  sources (valid scalars, an empty section, and malformed sections), each crafted source under all
  three error strategies (84 runs), dumping the parsed section and the parse diagnostics. The
  output was byte-identical across the 168 runs. Fourteen of them contain the `Value::Error`
  fallback's message, so a change to it would show in the diff.
- **The first harness missed the fallback.** It ran only the default `halt` strategy, under which
  the parser gives up before it reaches the `Value::Error` stand-in for a missing value, so that
  path was uncovered. Running the crafted sources under `continue` and `recover` reached it. A
  deliberate bug in the integer arm showed up in the first harness's diff.
- **`tests/security_section_tests.rs`:** the parser accepts any block name, so `bogus -> { a = 1 }`
  parses. Rejecting it is the semantic analyzer's job, and a test records this.
- **Where the arena stops paying:** the sections are parsed in parallel, so each parser builds
  into its own arena and exports at the end of its section. That adds a copy, and it stays until
  the arenas can be merged by id offset and consumers read a `Document`.

### Fixes and Problems: arena AST RAW and SCHEMA parsers

- **`Compiler/Core/SectionParsers/raw_section_parser.rs` and `schema_section_parser.rs`:** both
  conversions were checked with a throwaway before and after harness: the loader's tokenize,
  config and parse stages run over 22 crafted `@RAW` sources and 21 crafted `@SCHEMA` sources, each
  under all three error strategies (129 runs), dumping the parsed section and the parse and
  lexical diagnostics. The baseline came from the original parsers and was identical across two
  runs. The converted parsers' output was byte-identical to it. Six of the runs contain the
  `Value::Error` fallback's message, so a change to it would show in the diff. The corpus holds no
  `@RAW` and no `@SCHEMA` section, so no corpus file was run through this check.
- **Every strategy is run.** Under the default `halt` strategy the `@RAW` parser gives up on a
  block with a missing value before it reaches the fallback, so a halt-only harness would not cover
  it. The harness runs `halt`, `continue` and `recover` for every source, as the pilot's did.
- **`tests/raw_parser_tests.rs` and `tests/schema_parser_tests.rs`:** neither parser had direct
  tests, only loader tests that cannot see positions or diagnostics. The new files hold fourteen
  and ten tests, and all of them passed on the original parsers before the conversion and on the
  converted ones after it. Six deliberate bugs were injected to check them: a `Long` value off by
  one, an error message that dropped the key, a `using` block dropped on export, a
  `first_error()` that never recorded, the arguments reversed, and an `alloc_value_list` that
  returned a run one entry short. Each was caught by the tests that check that behaviour, and the
  last one also by the `alloc_value_list` doctest.
- **`Compiler/AST/arena/mod.rs`:** the only code that built a list run was `import_value_list`,
  which is private to the arena module, so a parser could not allocate the argument list of a call.
  `Arena::alloc_value_list` was added, with documentation, a `# Panics` section and a doctest. The
  other list kinds get the same kind of allocator when the parser that needs one is converted.
- **Behaviour recorded, not changed:** under `halt` a `@RAW` block with a missing field value is
  dropped whole, a duplicate block name is reported and the later block wins, `@SCHEMA` raises the
  error through `first_error()`, and under `continue` the `@SCHEMA` parser ends the section at the
  first bad field while `recover` resumes at the next one.
- **Comments:** the history and the em dashes were removed from the comments in both parsers.
  Three diagnostic strings that contain an em dash were left as they are, because the text is
  shown to users and a change would alter the diagnostics.
- **Where the arena stops paying:** the same as in the pilot. Each converted parser builds into its
  own arena and exports at the end of its section, which adds a copy until the arenas can be merged
  by id offset and consumers read a `Document`. The `@RAW` and `@SCHEMA` sections are small, so the
  cost is small too.

### Fixes and Problems: arena AST @DATA parser

- **`Compiler/Core/SectionParsers/data_section_parser.rs`:** the conversion was checked with a
  throwaway before and after harness: the loader's tokenize, config and parse stages run over
  the 84 corpus files and 218 crafted sources (89 `@DATA` sources harvested from the existing
  tests and 129 written for each production and error path), each under all three error
  strategies (906 runs). The crafted sources dump the parsed section and the diagnostics in full.
  The corpus files dump the entry count and a hash of the section's `Debug` and `Display` text,
  because their dumps run to megabytes. Sections parse in parallel, so the diagnostic lines are
  sorted before they are compared. The original parser's output was identical across two runs,
  and the converted parser's output was byte-identical to it.
- **The first crafted sources did not test calls.** They wrote calls as `~f(1)`, the form used to
  define a function in `@QUICKFUNCS`. In `@DATA` a call is written `f(1)`, so those sources
  failed at the `~` and never reached the call-argument parsing, which holds one of the three
  scratch stacks. This was noticed while recording the test cases, because a case that should have
  parsed gave no section. The sources were corrected, the original parser was swapped back in to
  record a new baseline, and the comparison was repeated: 906 runs, byte-identical.
- **`tests/data_parser_tests.rs`:** 35 tests. A first set of six injected bugs (missing truncation
  in the array, object, call and tuple wrappers, a `Long` off by one, a wrong imported enum name)
  showed gaps in the tests. The array and tuple truncations and the `Long` were caught. The object
  and call truncations and the enum name were not caught by a test that could fail only because
  of them, because no test put an object in an object, a call in a container in a call, or an
  imported enum in the three-part form the code handles. Tests for each were added and the check
  was repeated with nine bugs (the four call wrappers counted separately). All nine are now caught:
  16 tests failed and one was killed by the memory blow-up described above. The four call-wrapper
  bugs were injected together, so which of them each test catches was not separated.
- **The group array's truncation cannot be observed.** Group arrays appear only at the top level of
  the section, never inside another list, so a leaked entry cannot reach an enclosing list. The
  truncation stays because it costs nothing and keeps the rule simple, but no test can fail without
  it.
- **Behaviour recorded, not changed:** an array nested 66 deep reports the limit of 64, a call
  nested 12 deep parses without a diagnostic, and a failed nested array under `continue` keeps
  the elements before the error (`[1, [2 3], 4]` gives `[1, [2], 4]`).
- **`NOTICE` header:** the file's header named the section "Compiler/Core/SectionParsers/
  data_section_parser.rs", but `compiler.md` only had a combined entry for the four parsers, so the
  heading did not exist. The section above is that heading.
- **Where the arena stops paying:** the same as in the earlier parsers. The section is built in its
  own arena and copied out at the end, which adds a copy until consumers read a `Document`. The
  scratch stacks do remove a temporary `Vec` per list, but this was not measured.

### Fixes and Problems: arena AST @QUICKFUNCS parser

- **`Compiler/Core/SectionParsers/quickfuncs_section_parser.rs`:** the conversion was checked with
  a throwaway before and after harness: the loader's tokenize, config and parse stages run over
  the 84 corpus files and 204 sources (18 harvested from the existing tests and 186 written for
  each statement, expression and error path), each under all three error strategies (864 runs).
  The crafted sources dump the parsed section and the diagnostics in full. The corpus files dump
  the function count and a hash of the section's `Debug` and `Display` text. The diagnostic lines
  are sorted before they are compared, because sections parse in parallel. The original parser's
  output was identical across two runs, and the converted parser's output was byte-identical to it.
- **The crafted sources were checked before the conversion.** After the `@DATA` parser's sources
  turned out not to reach the code they were written for, the baseline was searched for each
  construct (`if`, `chk`, lambdas, interpolated strings, ternaries, postfix chains, constructors)
  before any code was changed, and every one was present.
- **`tests/quickfuncs_parser_tests.rs`:** 41 tests, designed with the `@DATA` lessons in hand:
  nested lists of every kind that share a scratch stack, in both the failing and the passing form.
  Ten injected bugs (the truncation removed from each of the six list wrappers, the `elif` chain
  nested in the wrong order, the `__call__` method name changed, the dotted join in value
  conversion changed, and the interpolation sub-parser not handing the arena back) were all caught:
  23 tests failed and two were killed by the memory blow-up. The bugs were injected together, so
  where several of them break the same test, which of them it catches was not separated.
- **Behaviour recorded, not changed:** after a malformed list the parser's recovery turns the
  tokens that follow the error into expression statements. For example `return [1, [2 3], 4]`
  under `continue` gives `return [1, [2]]` followed by the statements `3`, `null`, `null`, `4` and
  `null`. A call, an array or a parenthesis nested 66 deep stops being parsed part way, so the
  recorded section is cut off at the depth limit and the diagnostics are a run of `Expected ')'`
  or `Expected ']'` errors, not a message about nesting.
- **`NOTICE` header:** the file's header named the section "Compiler/Core/SectionParsers/
  quickfuncs_section_parser.rs", but `compiler.md` only had a combined entry for the four parsers,
  so the heading did not exist. The section above is that heading.
- **Dead code left as it was:** `parse_interpolated_expression`, the `MAX_STUCK_COUNT` constant and
  the `last_position` and `stuck_count` fields are never used, and the compiler already warned
  about them. The method was converted so the conversion is complete, and none of the four was
  removed.
- **Where the arena stops paying:** the same as in the earlier parsers. The section is built in
  its own arena and copied out at the end, which adds a copy until consumers read a `Document`.
  This completes the section parsers; every consumer still reads the owned types.

### Fixes and Problems: merging the section arenas

- **`Compiler/Core/general_parser.rs`:** the change was checked with a throwaway harness that runs the
  loader's tokenize, config and parse stages over the 84 corpus files and 422 crafted sources (the
  `@DATA` and `@QUICKFUNCS` ones from the parser checks), each under all three strategies (1,518
  runs). It hashes every section of the AST that `parse` returns (data, quick functions, security,
  schema, raw, imports, DLM and enums) and the diagnostics. The output of the pristine origin and of
  the changed tree is byte-identical, and each is the same on a second run. The hash of the whole
  file and of `@CONFIG` is left out, because the compiler's output is not deterministic across runs in
  the order of the `@CONFIG` entries (a `HashMap`), and including it made the baseline differ from
  itself.
- **The first comparison compared a binary with itself.** The harness for the pristine origin and for
  my tree were first built into one shared target directory, and cargo did not rebuild the library for
  the second tree, so both binaries were the same program and the "byte-identical" result meant
  nothing. This was noticed because a check of the two binaries found them equal. The two were
  rebuilt one after the other, with `cargo clean -p dixscript` between them, and each was checked for
  symbols that only the changed tree can contain (`ArenaOffsets`, `parse_section_doc`, `into_arena`,
  `assign_section_to_document`) before the comparison was repeated. Building a second full target
  directory also filled the disk.
- **`tests/arena_merge.rs`:** see the Fixes entry in `arena-ast.md`. Ten injected bugs were each
  caught by the main merge test, after two were found to go undetected by the first version.
- The arena of a parser is appended whole, so the `Document` that `parse_document` returns also holds
  the unreferenced nodes that a failed sub-expression left behind. They are harmless and are copied by
  the merge. A section that fails to parse at all returns no arena and adds nothing.
- **Not changed in that drop:** the loader and the language server still called `parse`, and the
  analyzers still read the owned AST. Nothing outside `general_parser.rs` called `parse_document`.
  The loader has called it since the `@RAW` analyzer moved (see the last entry below).

### Fixes and Problems: the `@RAW` analyzer reads the Document

- **`Compiler/Core/SectionAnalyzers/raw_section_analyzer.rs`, `general_semantics_analyzer.rs` and
  `Runtime/loader.rs`:** the change was checked with a throwaway harness that runs the loader's
  tokenize, config, parse and semantic stages over the 84 corpus files and 21 crafted sources, the
  crafted ones under all three strategies (147 runs, 2,198 lines). It prints the success flag, the
  sorted errors and warnings with positions, the per-section results, the records the analyzers
  left in the error manager, the short name and type indexes and the symbol table, with every map
  sorted. It does not print the AST or `@CONFIG`, so the `HashMap` order of `@CONFIG` does not
  matter. The output of the original code is the same on a second run, and the changed tree gives
  byte-for-byte the same output through the owned entry point and through the `Document` one. The
  comparison was repeated on the final build. The harness and its output are not delivered.
- **The two binaries were checked to be different programs.** The symbol `analyze_doc` occurs four
  times in the changed binary and not at all in the original one. This follows the earlier entry
  about a comparison of a binary with itself.
- **The corpus has no `@RAW` section,** so only the crafted sources reach the analyzer. The baseline
  was checked to contain every error and warning code the analyzer can give, under all three
  strategies, before it was relied on.
- **`tests/raw_analyzer_document.rs`:** eleven bugs were injected behind an environment switch, in
  one build and each run alone, with a control run that must pass: the document ignored by the
  orchestrator, the wrapper copying only the first block, `size` rejecting an integer, `checksum`
  expecting an integer, duplicate tags compared on their first character, the last of a repeated
  key winning, `halt` never stopping, the duplicate id error pointing at the first block, the
  emptiness check reading the owned tree, a blank id accepted, and `threads` rejecting a long. Ten
  were caught by the tests as they stood when the check began. The eleventh, a `threads` value of
  long type being rejected, was caught by nothing, so a source and two table rows for it were added
  and it is caught now.
- **The 16 tests in `raw_section_tests.rs` caught none of the eleven bugs.** They check through the
  loader whether a source is accepted, so a wrong position, a wrong message or a duplicate that is
  found for the wrong reason does not fail them.
- **What no test can catch:** the loader forgetting to call `with_document`. The result is the same
  by design, so no output differs. It was checked by reading the code, and an allocation count
  taken once more consumers have moved would show it.
- **Not changed:** `version_manager.rs` still tests `script.raw.is_empty()` on the owned tree, an
  emptiness check with nothing to gain from the arena. The imports resolver and the language server
  still call `parse`.
- **Not measured:** speed and peak memory. During stage 5 the document and its export now exist at the
  same time, where `parse` used to drop the arena inside the export. The corpus has no `@RAW`
  section, so no gain is expected on it, and the `@RAW` analyzer is the smallest of the five (311
  lines).
- **Not run:** CI on stable Rust and the bench workflow. The results of the owner's runs could not be
  read, because GitHub's API rate-limits the sandbox.

### Fixes and Problems: the `@SECURITY` and `@SCHEMA` analyzers read the Document

- **The change was checked with the same before and after harness as the `@RAW` analyzer,** extended
  with 69 sources (38 for `@SECURITY` and 31 for `@SCHEMA`, some of which mix in other sections). It now
  runs the 84 corpus files and 90 crafted sources, the crafted ones under all three strategies (354
  runs, 5,227 lines), and for every source with a `@SCHEMA` it also calls `validate_resolved` and
  prints its diagnostics. The output of the original code is the same on a second run, and the
  changed tree gives byte-for-byte the same output through the owned entry points and through the
  `Document` ones. The harness and its output are not delivered.
- **One group of crafted sources never reached the code it was meant for, and one coverage gap was
  found, both before the baseline was relied on.** A blob written as `b:("x")` is not valid base64,
  so the `@DATA` analyzer stopped those files before `@SCHEMA` ran. Separately, a value kind that the
  schema analyzer classifies only shows up in a message when it mismatches the schema, and only six
  kinds had appeared, so sources that pair each kind with `Schema.Int` were added. After that every
  kind that can appear in a mismatch message was seen, and every `SCH` and `SEC_WARN` code was seen
  under all three strategies in the recording. A value that is only known after resolution is
  accepted without a message by design, so it appears in the sources but not in a message.
- **The two binaries were checked to be different programs.** The symbol `run_owned` occurs ten times
  in the changed binary and not at all in the original one.
- **The baseline was re-recorded once from the original code** to add three sources the mutation
  checks needed. The changed files were put aside, the original ones restored, the harness rebuilt
  and run twice, and the changes put back.
- **`tests/security_analyzer_document.rs` and `tests/schema_analyzer_document.rs`:** 33 bugs were
  injected behind an environment switch, in one build and each run alone, with a control run that
  must pass. Eleven were in the `@SECURITY` path: the document ignored, the presence check reading
  the owned tree, the wrapper copying an empty section, the first instead of the last repeated key,
  the last instead of the first manual acceptance, the first instead of the last block, an exclusive
  backup range, a minimum compared with `<=`, case-sensitive names, longs accepted as integers, and a
  warning with no position. Twenty-two were in the `@SCHEMA` path, among them the document ignored
  for the schema or for the data, the wrapper dropping the data, a path walk that ignores the key, the
  last entry winning, byte length for strings, group arrays off by one, a tuple read as a blob, the
  literal winning over the annotation, qualified enum names not matching, `halt` never stopping, a
  wrong position, descriptor arguments truncated, a required null accepted, and a long truncated to
  32 bits. Thirty-one were caught by the first version of the tests.
- **Two bugs escaped, both gaps in the inputs, and tests were added for them.** A `NestedArray`
  item count ignored was caught by nothing because no parser creates that value, so a test builds
  the data by hand and runs it through both entry points. A long truncated to 32 bits before the
  range check was caught by nothing because no source held a long above `i32::MAX` against a long
  range, so a source for it was added. Both bugs are caught now.
- **The existing tests caught few of the bugs.** The 42 loader-level tests in
  `schema_section_tests.rs` caught seven of the 22 schema bugs, and the 10 parser tests in
  `security_section_tests.rs` caught none of the 11 security bugs. They check whether a source is
  accepted, or what it parses to, so a wrong position, a wrong message or a silent change of which
  duplicate wins does not fail them.
- **`SEM0002` cannot be reached from a parsed file.** The parser adds a default `@SECURITY` section
  when `@DLM` asks for an encryptor. The first plan was to pin the error with a source that has an
  encryptor and no section, and the recording showed that source analyzed without one, so the check
  is tested through a pair of a tree and a document that disagree instead.
- **What no test can catch:** the loader or the orchestrator calling the wrong entry point when both
  give the same result. Results are the same by design, so no output differs.
- **Not changed:** the `@DATA` analyzer, the `@QUICKFUNCS` analyzer and enhancer, and the resolver
  still read the owned tree. The imports resolver and the language server still call `parse`.
- **Not measured:** speed and peak memory. Stage 9 now copies the resolved `@DATA` section into an
  arena, where it read the owned tree directly before, and the corpus has no `@SCHEMA` section to
  show it. The corpus has twelve `@SECURITY` sections, and that analyzer is small.
- **Not run:** CI on stable Rust and the bench workflow, because GitHub's API rate-limits the sandbox.
  The language server, which now depends on `dixscript` by path, could not be type-checked here
  either, because Cargo 1.75 cannot build its dependencies. No existing signature changed, and the
  language server calls only constructors that are unchanged.

### Fixes and Problems: the `@DATA` analyzer reads the Document

- **The change was checked with the same before and after harness as the earlier analyzers,** with 49
  `@DATA` sources added. It now runs 84 corpus files and 138 crafted sources, the crafted ones under
  three strategies (498 runs, 7,164 lines), and a second mode analyzes 21 hand-built sections under
  three strategies (507 lines). The output of the original code is the same on a second run, and the
  changed tree gives byte-for-byte the same output through the owned entry points and through the
  `Document` ones, for both modes. The two binaries were checked to be different programs: the symbol
  `infer_type_from_value_id` occurs four times in the changed binary and not at all in the original.
- **Some of the first sources never reached the code they were meant for.** A table followed by a
  flat property is merged into the table by the parser, so `ORDERING_VIOLATION` never appeared. The
  expression values (`1 + 2`) produced no diagnostic. Blob and regex constructors with no argument or
  a non-string argument produced none either, and a source that held several constructors recorded
  only the first. The constructors were split into one source each, and what could not be reached
  from text moved into the hand-built mode. After that all 17 error codes were seen.
- **`tests/data_analyzer_document.rs`:** eighteen bugs were injected behind an environment switch, in
  one build and each run alone, with a control run that must pass: the document ignored, the presence
  check reading the owned tree, the wrapper copying an empty section, the nesting limit off by one,
  the nesting depth never counted, duplicate object keys never reported, a group array error pointing
  at the array, blob and regex checks skipped, the tuple limit off by one, a group array not counted
  as grouped data, duplicate table property names not reported, the wrong item index in the short-name
  index, constructor prefixes matched with case, tuple arguments not checked, inference switched off,
  and the enum name and value swapped. Seventeen were caught.
- **One bug could not be caught because it changes nothing.** Dropping the exemption that lets two
  objects sit together in an array is an equivalent mutant: two objects already compare equal, so the
  second condition of that check is false whenever the exemption applies.
- **The existing loader tests in `runtime_tests.rs` caught one of the eighteen bugs** (the swapped enum
  name and value). They check whether a file is accepted, so a wrong position, a wrong index entry or
  a skipped check does not fail them.
- **Not changed:** the `@QUICKFUNCS` analyzer and enhancer, the value resolver and the runtime still
  read the owned tree, and so does `version_constraints.rs`, whose `@DATA` and `@QUICKFUNCS` checks
  run as a separate stage on the exported tree. `AstVisitorBase`, which only the language server
  implements, is untouched.
- **Speed:** a timing of the harness binaries on the corpus, three runs each, in a debug build, gave
  the same time (about 28.7 seconds) for the original code, the owned entry point of the changed code
  and the `Document` path. That is not a benchmark, and it stops at semantic analysis. It shows no
  gain and no loss. Memory was not measured.
- **Not run:** CI on stable Rust and the bench workflow, because GitHub's API rate-limits the sandbox.
  The language server could not be type-checked either, because Cargo 1.75 cannot build its
  dependencies. No existing signature changed.

### Fixes and Problems: the `@QUICKFUNCS` analyzer reads the Document

- **The change was checked with the same before and after harness as the other analyzers,** extended
  with 59 `@QUICKFUNCS` sources and a mode that analyzes nine hand-built sections. The harness now also
  prints the qualified identifier resolutions. It runs 84 corpus files and 197 crafted sources (10,903
  lines) and the hand-built mode (265 lines). The output of the original code is the same on three
  runs, and the changed tree gives byte-for-byte the same output through the owned entry point and
  through the `Document` one, for both modes. The two binaries were checked to be different programs.
- **The baseline was not deterministic at first,** for two reasons that belong to the original
  analyzer: some suggestions list names in hash order, and the cycle detector starts a reported cycle
  from a different function on each run. Both are normalized in the harness and in the tests. They
  were found by running the baseline twice and then three times, not by reading the code.
- **Coverage:** 79 of 83 codes are reached. Three cannot be (see the analyzer section). The first set of
  sources reached 49, and the rest came from sources written to the code's conditions and from
  hand-built input for the checks the parser prevents.
- **The sensitivity of the tests was checked afterwards, and the first 13 tests were not sensitive
  enough.** 122 bugs were injected behind an environment variable into `quickfuncs_section_analyzer.rs`,
  `call_collector.rs`, `cycle_detector.rs` and `general_semantics_analyzer.rs` (one build, each bug run
  alone, a control run that passed, the mutated files confirmed to differ from the clean ones and
  restored and compared byte for byte afterwards). They moved each limit by one in both directions,
  changed the depth budget, skipped each kind of node in the call collector and in the unused variable
  collector, dropped each local scope rule, emptied each owned wrapper, ignored the document in the
  orchestrator and forced two of its branches. 85 of the 122 survived the first 13 tests. A second
  sweep suppressed each of the 83 codes one at a time: 80 were noticed and the three that were not
  are the three that cannot be reached.
- **The 85 survivors showed what the recording did not pin:** only the first size past each limit
  was tested, never the largest accepted size; the call graph's undefined callee records (written to
  the error manager, not to the result) were compared only for cycles; no source used a variable
  only inside a ternary, a switch, an index or a lambda; the owned wrappers of the call graph tools
  were never called; and the expression depth budget was tested at 100 and 130 levels, which are
  both far from its edge. Eighteen tests close these. With them, 114 of the 122 bugs are noticed.
- **The eight that remain cannot change an output, or no input was found that does.** The `.min(500)`
  cap of the depth budget (a body is limited to 1000 statements, so the budget stays at 200 or below),
  the statement nesting check against the depth budget (the cap of 50 stops first) and the
  `is_success` flag set after the cycle check (overwritten by the error count one line later) are
  equivalent, five bugs in all. Treating a parameter name as a reference in the unused variable
  collector is equivalent, because a variable with a parameter's name is refused with `QFUNC069`
  before it is registered. Leaving the element type of a typed array or tuple parameter out of the
  scope tracker changes nothing that was found: the type inference visitor finds the element type
  from the parameter's own type when no hint is given. This last one is not proven equivalent.
- **Found while writing the tests, not changed:** the method chain depth counts the calls and
  property accesses below the last call, so a chain of 12 calls has a depth of 10 and 13 is the first
  to be refused; `QFUNC074` cannot be reached from text because a parenthesized expression is
  parsed to its content, so the depth tests use hand-built sections; and a cycle sets the flag in
  the result for a moment but is reported only to the error manager, so a source with only a cycle
  gives a successful analysis result.
- **Not done in this drop:** the new tests were written against the code as it is now, with each
  assertion checked on the clean build and on the mutated one; they were not run through the
  original code from before the move. The enhancer (`quickfuncs_ast_enhancer.rs`,
  `qualified_identifier_resolver.rs`, `general_ast_enhancer.rs`) still read and rewrote the owned
  tree in that drop (they moved in the next one, below), and the owned `TypeInferenceVisitor` has not
  become a wrapper yet.

### Fixes and Problems: the `@QUICKFUNCS` enhancer reads the Document

- **What moved.** The enhancer (`quickfuncs_ast_enhancer.rs`), the qualified identifier resolver and
  `GeneralAstEnhancer` now work through the arena, and the loader keeps the `Document` until after
  stage 6. This is the first stage that builds new nodes rather than reading them: an arena is
  append only, so the enhancer allocates the nodes that change and re-points their parents. The
  enhanced section is still copied out into the owned tree, because the value resolver reads that.
- **How it was checked against the original.** A throwaway harness ran the original owned enhancer
  over the 84 corpus files and 59 crafted sources under two error strategies (202 runs, each with and
  without a semantic result) and over ten hand-built sections in three modes, and printed the debug and
  display text of the enhanced section, the counts, the section info and the log text. The same
  harness, run after the change through the owned entry point and through the document, printed the
  same text byte for byte, three runs of the original agreeing with each other once the one value that
  depends on the clock (the default of a timestamp parameter) was masked. The two binaries were
  checked to be different programs.
- **Bugs were injected afterwards, and the first tests missed some.** 94 bugs were put behind an
  environment variable into the resolver, the enhancer and `GeneralAstEnhancer` (skipping each child,
  swapping name parts, flipping the key, breaking each branch of the fallback, forcing allocation,
  dropping defaults, ignoring the document), each run alone against a control that passed. 87 were
  noticed by the first 12 tests. The seven that were not showed three gaps: nothing checked that
  unchanged switch cases, object properties and values are shared, nothing put a qualified identifier
  among the arguments of a call the resolver rebuilds, and nothing read the section info. Three tests
  close them, and 93 of the 94 are now noticed. The one left, always building the instance of a
  two part method call as a property chain, cannot change anything, because a chain of one name is that
  name.
- **Found while doing it, not changed.** The default of a date parameter is the date of the run, so a
  file's enhanced section differs from one day to the next. A qualified identifier inside a cast, a
  built-in or static function call, a plain function call, a nested array, a range or a quick function
  call written as a value is not resolved, which the recorded output depends on.
- **Not done in this drop.** CI on stable Rust has not been run. The owned tree is still cloned and
  the enhanced section still copied out of the arena, so there is no saving yet; speed and peak
  memory were not measured. The owned `TypeInferenceVisitor` entry points became wrappers in the next drop (see `arena-ast.md`), the
  orchestrator read `@ENUMS`, `@DLM` and `@IMPORTS` through the owned tree in that drop (it reads them from the document since the one after the next, see below), and the language
  server still calls the owned entry points.

### Fixes and Problems: the orchestrator reads `@IMPORTS`, `@ENUMS` and `@DLM` from the Document

- **What changed.** The five places in `general_semantics_analyzer.rs` that read these sections through
  the owned tree now read them through the tree that is read, with three helpers. Nothing else in the
  library changed.
- **How it was checked against the previous code.** A throwaway harness printed the whole semantic
  result (errors with ids, types, sections, messages and positions, warnings, every section result,
  the enums, functions, namespaces and data variables of the symbol table) for the 84 corpus files and
  75 crafted sources (16 of them new, for enums, `@DLM` and imports), through the owned path and through
  the document, with the default features and with the features switched to `data` only after parsing.
  The four outputs after the change are identical to the ones from before it, byte for byte; before
  the change the two paths were already identical to each other. With the features off the run reaches
  the orchestrator's own feature errors (49 for `@ENUMS`, 25 for `@IMPORTS`).
- **One existing test changed, on purpose.** The third case of
  `a_missing_section_is_looked_up_in_the_attached_document` in `security_analyzer_document.rs` gave the
  analyzer an owned tree with an encryptor and a document with no `@DLM`, and expected the requirement
  for `@SECURITY` to come from the owned tree. Under the rule that the tree that is read decides, a
  document with no `@DLM` asks for nothing, so the case now uses a document with the encryptor and no
  `@SECURITY` section (which still reports `SEM0002`), and a new assertion pins the other reading. No
  file the loader produces is affected, because its tree is exported from its document.
- **Bugs were injected afterwards.** Eight bugs reverted each read to the owned tree, one at a time
  (each helper, the two imports phases, the enums phase, the `@DLM` phase, the encryptor requirement),
  and all eight are noticed by the seven new tests.
- **Not done in this drop.** The value resolver, the runtime, the DLM auditor and the binary packer
  still read the owned types, as do `version_constraints` and `version_manager` (`ImportsResolver` moved in the next drop);
  the owned export therefore stays. CI on stable Rust has not been run.

### Fixes and Problems: `ImportsResolver` parses imported files to a Document

- **What changed.** `imports_resolver.rs` only: an imported file is parsed with `parse_document`, and
  the semantic analyzer and the enhancer are given the document. The public entry points keep their
  signatures; `resolve_imports`, which takes trees the caller parsed, still takes the owned path.
- **How it was checked against the previous code.** A throwaway harness printed, for the 84 corpus
  files (17 of which import) and 16 crafted files, the whole semantic result of the main file, every
  imported namespace in full (each function's signature and enhanced tree, the enums and the local
  imports) and the error manager's imports errors, through the owned path and through the document
  path. The four outputs after the change are identical to the ones before it, byte for byte. The
  crafted files reach a success, a diamond, a nested chain, an empty file, a missing file, a parse
  failure, a semantic failure, a self import, a mix of good and bad imports and a scoped function.
- **No bug injection was done, and none would show anything.** The document and the owned tree of
  an imported file agree, so a bug that made the resolver ignore the document would give the same
  output; the property worth testing is that the output is the original's, which the recording does.
- **The cost did not change measurably** (about 0.2% fewer instructions on the chemistry chain).
- **Found while doing it, not changed.** The resolver attaches the config section to the exported
  tree, which `parse_document` already did; the line is kept so the result stays byte for byte the
  same. Cloud imports were exercised by the corpus only as far as they go without a network.
- **Not done in this drop.** The extraction of functions and enums still reads the exported owned
  tree, the owned export therefore stays, and `version_constraints` and `version_manager` still read
  the owned tree. CI on stable Rust has not been run.

### Fixes and Problems: `VersionConstraints` reads the Document, and what was dead in it is gone

- **What changed.** `version_constraints.rs`, `version_manager.rs` and the signature check in
  `type_system_manager.rs`, plus one call in the orchestrator and the three callers of the removed
  `get_current_version`. The list of what was removed is under the modules heading.
- **How it was checked against the original.** A throwaway harness printed the whole `ValidationResult`
  of the original `validate_script` for the 84 corpus files and 75 crafted sources under 24 config
  variants (3,724 lines), and for six hand-built scripts under the same variants (144 lines), under
  language version 1.0.0 and under 2.0.0. After the change the owned form and the document form both
  print the same lines, byte for byte, once the two removed fields, which were always empty, are taken
  out of the original's text. The semantic output of the whole pipeline for the corpus and the 16 crafted
  import files is also identical to the one from before, except for today's date in the default of a date
  parameter, which follows the clock.
- **25 bugs were injected afterwards and all 25 are noticed.** They broke each branch of both data scans,
  both signature checks, the section gates, the section list, the presence tests of the two forms, the
  orchestrator's choice of tree and the feature lookup.
- **Found while doing it, not changed.** The version phase reports a section that needs a feature, and the
  analyzer's own phases report it again as a feature error. The parser also refuses such a section before
  analysis, so the version phase's feature gates are only reached with a config changed after parsing, as
  the tests do. `ForwardCompatibilityManager::handle_unknown_element` has no caller now that
  `VersionConstraints` does not call it; it is public and was left alone. `TypeSystemManager::validate_parameter`
  has no caller either and was left alone, as it was outside these files.
- **Not done in this drop.** The owned `validate_script` has no wrapper over the arena form, on purpose;
  the language server uses it. The owned export, the value resolver, the runtime, the DLM auditor and
  the binary packer remain. CI on stable Rust has not been run.

### Fixes and Problems: value resolution phase 1 (enum pre-resolution) reads the Document

- **What changed.** `value_resolver.rs` (phase 1 and its owned walker), the new `enum_resolution.rs`,
  `ValueResolution/mod.rs` and the loader. The owned walker (six functions, about 890 lines) is
  replaced by an arena walk of about 510 lines with its docs, and `ValueResolver::with_document` is
  new. Three private functions nothing called, `resolve_call_arguments`, `resolve_expr_to_dix` and
  `resolve_value_to_dix` (about 240 lines), were removed. Library warnings went from 123 to 120 and
  none comes from these files.
- **How it was checked against the original.** A throwaway harness ran the loader's stages up to the
  value resolver, with a temporary hook that stops the resolver after phase 1 and prints the whole
  tree, in the original and in the changed code. It ran the 84 corpus files and 15 crafted files (13
  sources and two import libraries), carrying on after semantic errors so the resolver's own error
  paths were reached, and 30 hand-built data sections against two bases. The output was identical
  byte for byte in the three ways the code can be reached (document attached, scratch arena, and the
  owned path of the language server), with debug logging off, regular and verbose. With the hook off
  and the whole resolver running, 92 of the 99 files are identical (74 byte for byte, 18 once the
  order of object properties that function results carry, a hash map, is fixed). The other seven
  differ from run to run in the original too: they use a clock, random or GUID builtin or list enum
  names in hash order, so phase 1 is what was compared for them.
- **81 bugs were injected afterwards and all 81 are noticed.** The first round caught 79. The two that
  survived showed what the inputs lacked: an owned tree with no data and a document whose data has
  nothing to resolve, and an expression that wraps a value whose contents change. Both inputs were
  added and the recording was made again from the original. The bugs break each expression and value
  arm, every count, the nested array rule, the list walker (prefix, suffix, sharing), table, group and
  object entries, the choice between the document and the owned tree, the two exports, the merge into
  `@ENUMS` and all error messages and locations. One of them (the merge's sort) depends on hash order
  and was caught in 12 runs of 12. Two things were not injected because they cannot be seen: the
  merge's check for an enum already merged, which only matters if `resolve` ran twice and it consumes
  the resolver, and the loader passing the document, which changes allocation and not the result.
- **Measured, not assumed.** Callgrind, debug build, the whole pipeline for one file, original against
  changed: `EnumUsage.mdix` 36.74M against 36.49M instructions (0.7 percent fewer), `BigBoy.mdix`
  82.78M against 82.50M (0.3), `Unholy.mdix` 487.7M against 466.3M (4.4), `basic/a_test.mdix`, which
  has no enums, 24.21M against 23.84M (1.5) and `elements_database.mdix` 2,574M against 2,533M (1.6).
  The gain is from data that no longer is rebuilt or exported when nothing changed. Release builds,
  wall-clock time and peak memory were not measured. The document now lives through stage 7, so
  its peak may be higher.
- **Found while doing it, not changed.** An enum used only inside an expression is checked and
  resolved but not recorded, so it is not merged into `@ENUMS`; only the dotted value form is. That
  is how it was and it is pinned. The merge into `@ENUMS` and the order of a merged enum's fields
  between equal values follow hash order. `ValueResolutionResult::original_ast` is still a clone of
  the whole tree that nothing reads. The `@DATA` grammar has no arithmetic, conditional or
  interpolation, so most expression arms of phase 1 can only be reached with a hand-built tree.
- **Not done in this drop.** Phases 2 to 5 of the resolver (the call finder, the interpreter, the
  remaining identifiers and the result) still read and rewrite the owned tree, and the document's
  `@ENUMS` is left as it was. The runtime, the DLM auditor, the binary packer, the language server
  (which has not been type-checked here) and the sibling crates remain. CI on stable Rust has not
  been run.

### Fixes and Problems: value resolution phases 2 and 3 (data context and call finder) read the Document

- **What changed.** `value_resolver.rs` (phases 2 and 3, and a private `DataSource` that phases 1 to
  3 read from), the new `data_context.rs`, `ast_walker.rs` (rewritten over the arena, `find_all_in`
  added, `find_all` kept as a wrapper) and `ValueResolution/mod.rs`. `ValueResolver::with_document`
  now covers phases 1 to 3, and without a document the owned data is copied into a scratch arena
  once for all three. Removed: the owned `try_value_to_dix` (about 110 lines), the clone of the
  whole entries vector in the alias pass, a second `HashMap` rebuild in the object conversion, a
  `clear_scope_variables` call that the reset at the start of each entry made redundant, and a
  duplicate `BitwiseOp` match arm. Library warnings went from 120 to 119 and none comes from these
  files.
- **How it was checked against the original.** The throwaway harness of the previous drop was
  extended with a hook that stops the resolver after phase 3 and prints the sorted data context
  and every found call in full, in the original and in the changed code. It ran the 84 corpus
  files and 22 crafted files (18 sources and four copies of import libraries), carrying on after
  semantic errors, and the 30 hand-built sections of the previous drop plus 12 new ones for these
  phases, each against two bases. The output was identical byte for byte through the three ways
  the code can be reached (document attached, scratch arena, and the owned path of the language
  server), with debug logging off, regular and verbose. With the hook off and the whole resolver
  running, 99 of the 106 files are identical (81 byte for byte and 18 once the order of object
  properties that function results carry, a hash map, is fixed); the other seven differ from run to
  run in the original too, and phases 2 and 3 are what was compared for them.
- **85 bugs were injected afterwards and 83 are noticed.** The first round caught 71 of 86. Of the 15
  that survived, 12 were gaps in the inputs: an alias in the first entry, chains inside a table and a
  group, decoy keys that only end like a name, an alias that names a full key, and the log lines of
  the alias passes, the count and the call finder's verbose output. Those inputs and a test of the
  log were added and the recording made again from the original. One survivor was a call that the
  reset at the start of each entry makes redundant, and it was removed. The two that remain are
  equivalent: an exit from a table's or a group's scope that is skipped cannot be seen, because the
  next entry starts from the root.
- **Measured, not assumed.** Callgrind, debug build, the whole pipeline for one file, V1 against
  V2: `EnumUsage.mdix` 36.48M against 36.32M instructions (0.4 percent fewer), `BigBoy.mdix`
  82.52M against 82.37M (0.2), `Unholy.mdix` 466.3M against 463.8M (0.5), `basic/a_test.mdix`
  23.85M against 23.71M (0.6) and `elements_database.mdix` 2,533M against 2,515M (0.7). On that
  last file phase 2 went from 104.6M to 81.7M (22 percent fewer; the alias pass 65.9M to 42.2M) and
  phase 3 from 165.5M to 170.1M (3 percent more: copying a call's arguments out of the arena costs a
  little more than cloning owned ones). The gain is from the alias pass no longer copying the data
  section. Release builds, wall-clock time and peak memory were not measured.
- **Found while doing it, not changed.** `ScopeTracker::clear_scope_variables` has no caller now but
  is public and stays. The text of the call finder's per-entry log lines and the context dump are
  in hash order for objects. A value that does not parse as a date or timestamp becomes the time
  of the run, so the context is not reproducible for such a source.
- **Not done in this drop.** The write-back of results and phase 5 still read and rewrite the owned
  tree, and the interpreter (about 2,800 lines, 14.7 percent of the pipeline on the largest file)
  still takes owned arguments and owned function bodies. The runtime, the DLM auditor, the binary
  packer, the language server (not type-checked here) and the sibling crates remain. CI on stable
  Rust has not been run.
