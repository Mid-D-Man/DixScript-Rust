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

## [Unreleased]

### Removed

- **`DCompressor.bzip2` and `DCompressor.lzma`.** gzip is the only DLM
  compression codec. The `bzip2` and `lzma-rust2` dependencies are gone
  (10 fewer crates in the default build). A file that names either codec
  now fails analysis with a `DLM004` "no longer supported" error, and a key
  file that records one fails to decompress with the same kind of message.
  The `bzip2-support` and `xz-support` features remain as empty stubs so
  existing `features = [...]` lists still resolve, and they are no longer in
  `default`. Files written by 1.0.0 with these codecs can still be read with
  1.0.0.
- `binary_serialization_benchmark` compares gzip only.

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
