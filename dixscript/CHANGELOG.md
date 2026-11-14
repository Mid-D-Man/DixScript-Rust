# Changelog

All notable changes to the `dixscript` crate are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
versioning follows [Semantic Versioning](https://semver.org/).

For a snapshot of the *current* public API shape (not a log of changes to
it), see [`APICATALOG.md`](./APICATALOG.md) instead.

This covers the `dixscript` core crate only. The CLI (`mdix-cli`), FFI
bindings, and per-language wrappers (WASM/Python/C#/Go/Java/Lua/PHP) are
separate, independently-versioned packages in this same workspace with
their own release history.

## [2.0.0] — Unreleased

Breaking release.

### Added

- **`Compiler::AST::arena`**, an index arena for the `Value`, `Expression` and
  `QuickFuncStatement` family: `Arena`, the node types `ValueNode`, `ExprNode` and `StmtNode`
  (40, 48 and 48 bytes, against 144, 160 and 384 for the owned types), and lossless
  `import_*` and `export_*` between an owned tree and the arena, and the handles `ValueRef`,
  `ExprRef` and `StmtRef` (`Arena::value_ref` and friends) whose `Display` and `Debug` print
  exactly what the owned types print and whose `==` compares trees structurally, across
  arenas. `arena::Document` is the arena-resident form of a whole `DixScript`
  (`Document::from_owned`, `Document::export`, structural `==`), with `Doc*` twins of the five
  sections that hold values, each with its own `from_owned` and `export`. `Arena::alloc_value_list`,
  `alloc_expr_list`, `alloc_prop_list`, `alloc_stmt_list`, `alloc_str_list` and `alloc_case_list`
  store a run of ids (or names, or cases) for a parser that builds a list node by node, and
  `alloc_case` stores a switch's default case. `Arena::append` copies one arena onto the end of another and returns an `ArenaOffsets`, which the `shifted` method of each `Doc*` section applies to move the section with it. `GeneralParser::parse_document` parses a file into one `Document` by appending the arenas of the section parsers in source order, and each section parser has `parse_section_doc` and `into_arena` for it. `GeneralParser::parse` is `parse_document` followed by one export. `Document` implements
  `Display`, and `Arena::display` pairs a `Doc*` section with its arena as an `ArenaDisplay` that
  prints; both print exactly what the owned types print. All five section parsers (`@SECURITY`, `@RAW`, `@SCHEMA`, `@DATA` and `@QUICKFUNCS`) build their values in an arena and export the finished section,
  with their public interfaces unchanged; the rest of the compiler still uses the owned types. It
  is the first piece of the arena AST described in `docs/dixscript/arena-ast.md`.
- **`GeneralSemanticAnalyzer::with_document`**, which gives the analyzer the arena form of the file it
  is analyzing, and **`RawSectionAnalyzer::analyze_doc`**, which checks `@RAW` blocks through an
  `Arena`. `DocRawBlock::id` and `DocRawBlock::format` read a block's `meta_data` as
  `RawBlock::id` and `format` do. The `@RAW` analyzer is the first consumer to read a `Document`;
  `DixLoader` parses into one and passes it to the analyzer. Diagnostics are unchanged, and
  `RawSectionAnalyzer::analyze` keeps its signature.
- **`SecuritySectionAnalyzer::analyze_doc`** and **`SchemaSectionAnalyzer::analyze_doc`**, which check
  `@SECURITY` and `@SCHEMA` (with the `@DATA` section it constrains) through an `Arena`, and
  **`DocSchemaField::path_string`**. `GeneralSemanticAnalyzer::with_document` now covers these two
  sections as well as `@RAW`. `analyze` and `SchemaSectionAnalyzer::validate_resolved` keep their
  signatures and copy their input into a scratch arena, so `DixLoader`'s post-resolution schema check
  copies the resolved `@DATA` section. Diagnostics are unchanged.
- **`DataSectionAnalyzer::analyze_doc`**, which checks `@DATA` through an `Arena`, and the four arena
  entry points of `TypeInferenceVisitor` (`infer_type_from_value_id`, `infer_type_from_expression_id`,
  `infer_element_type_from_value_id` and `infer_element_type_from_expression_id`), which give the
  answers of their owned twins for the same tree. `Arena::value_ids`, `expr_ids` and `stmt_ids` visit
  every node. `GeneralSemanticAnalyzer::with_document` now covers `@DATA` as well, and
  `DataSectionAnalyzer::analyze` keeps its signature. Diagnostics, indexes and symbol table entries
  are unchanged.
- **`QuickFuncsSectionAnalyzer::analyze_doc`**, which checks `@QUICKFUNCS` through an `Arena`, and the
  arena entry points of the call collector and the cycle detector (`FunctionCallCollector::analyze_function_doc`,
  `CycleDetectionValidator::validate_function_calls_doc` and `get_execution_order_doc`).
  `GeneralSemanticAnalyzer::with_document` now covers `@QUICKFUNCS` as well, and the owned entry points
  keep their signatures. Diagnostics, qualified identifier resolutions and registered functions are unchanged.
- **`QuickFunctionsAstEnhancer::enhance_doc`**, which enhances `@QUICKFUNCS` through an `Arena`
  (parameter defaults and qualified identifier resolution), the arena entry points of the resolver
  (`QualifiedIdentifierResolver::resolve_statement_list`, `resolve_statement_id` and
  `resolve_expression_id`), and `GeneralAstEnhancer::with_document`, which makes the enhancer read the
  `@QUICKFUNCS` section from a `Document` and copy only the enhanced section into the tree it returns.
  The owned entry points (`QuickFunctionsAstEnhancer::enhance`, `QualifiedIdentifierResolver::resolve_statement`,
  `resolve_statements` and `resolve_expression`, and `GeneralAstEnhancer::enhance` without a document)
  keep their signatures and give the same result. `TypeSystemManager::synthetic_parameter_default` is the
  default `apply_defaults_to_parameters` gives a typed parameter, now shared with the arena enhancer.
  The loader keeps the `Document` through stage 6 and passes it to the enhancer.
- **`TypeInferenceVisitor`'s owned entry points are wrappers.** `infer_type_from_value`,
  `infer_type_from_expression`, `infer_element_type_from_value` and `infer_element_type_from_expression`
  keep their signatures and results, and now copy their argument into a scratch arena and call the
  `_id` entry points. The code that walked the owned tree is gone, and the public items of
  `type_inference_visitor.rs` have rustdoc with examples.
- **`GeneralSemanticAnalyzer::with_document` now covers `@IMPORTS`, `@ENUMS` and `@DLM`** as well, so
  every section the analyzer looks at is read from the document when one is attached. They are the same
  owned types in both trees, so the results do not change for a file the loader produces. For a
  document and an owned tree that disagree, the `@DLM` of the document decides whether an encryptor
  needs a `@SECURITY` section.
- **`ImportsResolver` parses each imported file to a `Document`** and gives it to the semantic analyzer
  and the enhancer, as the loader does for the main file. Its entry points keep their signatures and
  results; `resolve_imports`, which takes trees the caller parsed, still analyzes them through the owned
  entry points.
- **`VersionConstraints::validate_script_doc(&Document)`** validates a file held as a `Document` and gives
  the same result as `validate_script` for it; the orchestrator's version phase calls it when a document is
  attached. `TypeSystemManager::validate_doc_quick_function_signature` is the arena form of the signature
  check.
- **`ValueResolver::with_document(&mut Document)`** gives the first three phases of value
  resolution (enum pre-resolution, the data context and the call finder) the document the file was
  parsed into. They then read the document's `@DATA` in its arena instead of a copy of the owned data
  section, and the loader passes the document it already holds. Without one, `ValueResolver::new` and
  the other constructors work as before, and the owned data is copied once for the three phases.
- **`ASTWalker::find_all_in(&Arena, &DocData)`** is the arena entry of the call finder.
  `find_all(&DataSection)` stays and copies the section into a scratch arena.

### Removed
- `VersionConstraints::with_forward_compat`, `is_valid_value_type`, `is_valid_default_value`,
  `is_valid_type_annotation`, `requires_advanced_mode`, `is_valid_function_type`,
  `is_valid_static_method_call`, `is_valid_instance_method_call`, `is_valid_dix_function_call`,
  `validate_quick_function_signature` and `validate_data_entry`. None had a caller in the workspace;
  `is_valid_type_annotation` returned true for any argument.
- `VersionManager::validate_script_features`, `supports_feature_control`, `supports_dual_parsers`,
  `get_recommended_parser` and `get_current_version` (use `current_version`).
- `ValidationResult::warnings` and `detected_version`, which nothing ever set.
- **`DCompressor.bzip2` and `DCompressor.lzma`.** gzip is the only DLM
  compression codec. The `bzip2` and `lzma-rust2` dependencies are gone
  (10 fewer crates in the default build). A file that names either codec
  now fails analysis with a `DLM004` "no longer supported" error, and a key
  file that records one fails to decompress with the same kind of message.
  The `bzip2-support` and `xz-support` Cargo features are removed too, so a
  `features = [...]` list that names either one no longer resolves. Files
  written by 1.0.0 with these codecs can still be read with 1.0.0.
- `binary_serialization_benchmark` compares gzip only.
- Three private functions of `ValueResolver` that nothing called, `resolve_call_arguments`,
  `resolve_expr_to_dix` and `resolve_value_to_dix` (about 240 lines).
- The owned `try_value_to_dix` of `ValueResolver` (private, about 110 lines), replaced by the arena
  version in `data_context.rs`.
- `ValueResolutionResult::original_ast` (a public field, so a breaking change): the clone of the whole
  AST that `ValueResolver::resolve` took before it started. Nothing in the workspace read it.
- The owned write-back of `ValueResolver` (nine private functions, 250 lines: the index of call
  positions, the path and search helpers and `value_position`), replaced by the arena version in
  `write_back.rs`.
- The owned phase 5 of `ValueResolver` (334 lines: the identifier walk over entries and values),
  replaced by the arena version in `identifier_resolution.rs`.
- `ResolutionRecord::arguments` (a public field, so a breaking change): the `Debug` text of each
  argument expression, built for every call attempt and read by nothing in the workspace.

### Performance

- Value resolution no longer builds a `Debug` string for every argument of every call it records: the
  whole pipeline (debug build, prepare and resolve) is 5.6 percent cheaper on the largest corpus file,
  4.1 to 4.6 percent on two others, and unchanged on files with few calls.
- Value resolution's remaining-identifier pass no longer builds a formatted path string and clones an
  entry for every node it visits: on the largest corpus file its cost fell from about 146M to 16M
  instructions, and the whole pipeline (debug build, prepare and resolve) is 2.3 to 9.8 percent
  cheaper than before the arena migration of value resolution on the five files measured.
- **Parser reservations are capped.** The section parsers sized the Vec for every nested
  call argument list, array, object and statement block from the length of the whole token
  stream. Those reservations are now capped at 16 elements; the one vector that holds a
  whole section keeps its full estimate. Compiling `elements_database.mdix` (445 KB)
  requests 159 MB instead of 7.50 GB, and its parse stage takes about 30 to 45 percent
  less time. Output is unchanged. A new test, `parser_alloc_budget`, guards the figure.

- **Value resolution is linear in the number of resolved calls.** Writing each resolved
  call back into the data tree searched the whole tree for the call's position, so a 4x
  larger input took 16x as long in that step. Positions are now indexed once. Compiling
  `elements_database.mdix` (445 KB, debug mode off) takes about a third of the time it did,
  and a 32x scaled input takes about a sixth. Output is unchanged. A new test,
  `value_resolution_scaling`, guards the growth rate.

### Changed

- **Enum pre-resolution runs on the arena.** The first phase of value resolution, which checks every
  enum reference in `@DATA` and turns the ones inside expressions into integers, has one
  implementation over the arena (`ValueResolution/enum_resolution.rs`) in place of the owned walker of
  about 890 lines. The result, the errors and the log lines are unchanged. Data with no enum
  expression is no longer rebuilt, and a whole-pipeline instruction count is 0.3 to 4.4 percent
  lower on the five files measured. The loader now keeps its `Document` until stage 7 is done.
- **The data context and the call finder run on the arena.** The second and third phases of value
  resolution read the arena (`ValueResolution/data_context.rs` and `ast_walker.rs`) in place of the
  owned tree. The context, the calls found and every log line are unchanged. The alias pass no longer
  clones the whole data section, and the whole-pipeline instruction count is a further 0.2 to 0.7
  percent lower on the same five files.
- **The last two phases of value resolution run on the arena.** The write-back of results (phase 4,
  `ValueResolution/write_back.rs`) allocates each result and a copy of each array, object and
  constructor on the way to it, and shares the rest; the remaining identifiers (phase 5,
  `ValueResolution/identifier_resolution.rs`) are resolved the same way, with the path of an
  identifier built only when it resolves. With a document attached both read and rewrite the
  document's data, which holds the results afterwards; the owned data section is replaced by one
  export at the end, only when a phase changed the data or the owned tree had none. Results, errors,
  the history of calls and every log line are unchanged, quirks included (a node that shares a
  position with a call takes its result, a call whose position no node has is not written, a nested
  array that held a resolved identifier comes back as a plain array).
- **Language version 2.0.0.** `@CONFIG` `version -> "2.0.0"` is now a recognized
  language version. Before, the version manager accepted the string but treated the
  file as 1.0.0. 1.0.0 files still load. The two versions differ only in the DLM
  codecs: bzip2 and lzma are valid in 1.0.0 and invalid in 2.0.0, where the version
  check says they were removed. `VersionManager::is_compatible_with` now means "can
  read files up to that version", and `get_version_constraints` gains a
  `RemovedDLMModules` entry. The defaults (`OperationalSettings`, the config schema
  and the manager's fallback) stay at 1.0.0 for now.
- **Minimum supported Rust version is 1.75** (was 1.85). The crate and its tests build
  and pass on Rust 1.75.0. Four code changes made that possible: `std::sync::LazyLock`
  is replaced by a small `Lazy` type on `OnceLock`, two `Option::is_none_or` calls
  became `map_or(true, ..)`, one `Result::inspect_err` became `map_err`, and a
  temporary-lifetime pattern in `Runtime/format_options.rs` that only Rust 1.79 and
  later accepts was rewritten. The repository's `Cargo.lock` pins seven dependencies
  below their newest versions to stay inside 1.75.

## [1.0.0] — 2026-07-21

### Added

- **Language & compiler pipeline** — full tokenize → parse → semantic
  analysis → AST enhancement → value resolution → array homogenization
  pipeline for the `.mdix` format: `@CONFIG`, `@IMPORTS` (local + cloud),
  `@ENUMS`, `@QUICKFUNCS` (compile-time expression evaluation), `@DATA`
  (flat properties, tables, group arrays, nested objects), `@SECURITY`.
- **Runtime data access** — `DixLoader` for loading from disk, string, or
  encrypted bytes; `DixData` for O(1) flattened dotted-path reads;
  `DixValue` covering every DixScript type.
- **Struct (de)serialization** — `DixSerialize`/`DixDeserialize` traits
  for converting between `DixData` and plain Rust structs without an
  intermediate hashmap.
- **Format conversion** — `DixConverter` for DixScript ⇄ JSON ⇄ TOML ⇄
  `HashMap<String, DixValue>`, in both directions.
- **DLM (Data Lifecycle Modules)** — compression (gzip/bzip2/lzma, all
  pure Rust) and encryption (AES-256-GCM, ChaCha20-Poly1305) applied at
  compile time via `@DLM(...)`, reversed at load time via `DixLoader`'s
  encrypted-load path. Keyfile and password (Argon2id-derived) modes.
- **Auditing** — `@DLM(DAuditor...)` DIY and Enhanced auditors, producing
  append-only `.mdix.au` compilation history alongside the compiled
  output. Native filesystem backend and browser `localStorage` backend
  (wasm32), selected automatically per target.
- **AST-level merge** — `MdixMerger` for merging multiple DixScript
  sources with weight-based (`WeightedPriority`, `PrimaryWins`,
  `SecondaryWins`) or strict (`ThrowOnConflict`) conflict resolution,
  configurable per-section and per-array-strategy.
- **Schema validation** — `SchemaBuilder`/`ValidationReport` for
  fluent, non-panicking runtime validation of a loaded `DixData` against
  an expected shape, collecting every violation rather than failing fast.
- **Querying** — `DixQuery`, LINQ-style chaining (`where_`, `order_by`,
  `select`, ...) over array fields and `GroupArray`s, plus
  wildcard-pattern queries across sibling paths.
- **Hot reload** — `HotReloadWatcher`, a poll-based file-change watcher
  for Rust consumers driving a game loop or tick.
- **Builder API** — `DixDataBuilder` and friends for constructing
  `DixData` (or writing a `.mdix` file directly) without a template file.
- **Source-text tooling** — `DixCompactor` for minifying/compacting
  `.mdix` source (token-based, not regex) and stripping comments.
- **Cross-platform** — builds on native (Linux/macOS/Windows/Android/iOS)
  and `wasm32-unknown-unknown`; every optional compression/encryption
  backend is pure Rust with no native C linking anywhere in the default
  feature set.
- **Error handling** — `ErrorManager` with per-instance isolated state,
  phase-classified errors (`DixError` over eleven compilation/runtime
  phases) with severity levels, plus `DiagnosticDumper` for standalone
  diagnostic reports.
