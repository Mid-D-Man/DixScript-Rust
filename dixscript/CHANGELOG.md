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
  `alloc_expr_list` and `alloc_prop_list` store a run of ids for a parser that builds a list node by
  node. `Document` implements
  `Display`, and `Arena::display` pairs a `Doc*` section with its arena as an `ArenaDisplay` that
  prints; both print exactly what the owned types print. The `@SECURITY`, `@RAW`,
  `@SCHEMA` and `@DATA` section parsers build their values in an arena and export the finished section,
  with their public interfaces unchanged; the rest of the compiler still uses the owned types. It
  is the first piece of the arena AST described in `docs/dixscript/arena-ast.md`.

### Removed

- **`DCompressor.bzip2` and `DCompressor.lzma`.** gzip is the only DLM
  compression codec. The `bzip2` and `lzma-rust2` dependencies are gone
  (10 fewer crates in the default build). A file that names either codec
  now fails analysis with a `DLM004` "no longer supported" error, and a key
  file that records one fails to decompress with the same kind of message.
  The `bzip2-support` and `xz-support` Cargo features are removed too, so a
  `features = [...]` list that names either one no longer resolves. Files
  written by 1.0.0 with these codecs can still be read with 1.0.0.
- `binary_serialization_benchmark` compares gzip only.

### Performance

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
