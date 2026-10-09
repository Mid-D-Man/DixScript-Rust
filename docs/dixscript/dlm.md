# dixscript: DLM

## Modules

### `Compressor/mod.rs`

**What it does:** Declares the DLM compressor modules and re-exports them. Gzip
(`GzipCompressor`, `CompressionLevel`) is the only codec and sits behind
`dlm-compressor`. `compressor_trait.rs` is always compiled.

**Decisions:**
- bzip2 and lzma were removed instead of staying behind features. Nobody used them,
  `lzma-rust2` alone set the declared Rust floor (1.85, with `bzip2` at 1.82), and
  together they put 10 more crates in the default build (counted from the lockfile:
  `bzip2` and `libbz2-rs-sys` are 2, `lzma-rust2` and the `sha2` 0.11 chain it pulls
  are 8).
- `bzip2_compressor.rs` and `lzma_compressor.rs` are deleted.

### `dlm_pipeline_executor.rs`

**What it does:** Runs the forward pipeline at compile time (auditor start, then
compressor, then encryptor, then auditor finalize). `create_compressor` picks the
codec for a `DCompressor` subtype.

**Decisions:**
- The bzip2 and lzma arms of `create_compressor` return a "no longer supported"
  error in every build. The analyzer rejects these files earlier, so the arms are a
  backstop for callers that skip analysis.

### `dlm_reverse_executor.rs`

**What it does:** Reverses the pipeline at load time (decryptor, then decompressor).
The key file records the compression algorithm as a string and `create_decompressor`
maps it to a codec.

**Decisions:**
- `"bzip2"` and `"lzma"` are still recognized so an old key file gets a specific
  message (read it with dixscript 1.0.0 and re-compress with gzip) and not the
  generic "unknown compression algorithm" error.

### `dlm_section_analyzer.rs`

**What it does:** Semantic validation of the `@DLM` section: module types, subtypes,
duplicates, ordering, security notes.

**Decisions:**
- `DLMModuleSubtype::Bzip2` and `Lzma` stay in the AST, the parser and the keyword
  tables, so a file that names them still parses. They are no longer in the valid
  compressor set. The analyzer reports error `DLM004` (`REMOVED_MODULE_SUBTYPE`) at
  the module's position and suggests `DCompressor.gzip`. That is a clearer failure
  than an unknown-keyword error, and it works in builds with `dlm` off because the
  analyzer is always compiled.
- `VersionControl/version_constraints.rs` still treats the two names as known
  language-level modules. Only its `SupportedDLMModules` list, which tooling reads,
  changed to `["gzip"]`.

### `binary_serialization_benchmark.rs`

**What it does:** Compares the DixScript binary format with bincode, postcard and
MessagePack on encode speed and compressed size.

**Decisions:**
- Compressed size and the compression pipeline group cover gzip only now. The bench no
  longer declares `required-features`.

**Tests:** none for this file. Run it with `cargo bench -p dixscript --bench
binary_serialization_benchmark`.

## Compatibility and follow-ups

- `bzip2-support` and `xz-support` remain in `Cargo.toml` as empty stubs (each still
  implies `dlm-compressor`) and are no longer in `default`. A downstream
  `features = ["xz-support"]` keeps resolving. They can be deleted in the next
  breaking release.
- The crate version was not changed here.
- `mdix-wasm`, `mdix-cli`, `mdix-lsp`, `mdix-ffi` and `mdix-python` resolve the
  published dixscript 1.0.0, which still has both codecs. Their codec-specific
  material should change when each one moves to the release that carries this
  change: `mdix-wasm/tests/dlm_compression.rs` (the bzip2 and lzma cases),
  `mdix-lsp/src/features/completions.rs` and `hover.rs` (codec lists and the
  "not on wasm32" text), the fixtures `mdix_files/tests/dlm/02_bzip2.mdix` and
  `03_lzma.mdix` with their two entries in `tools/run_edge_case_tests.py`, and the
  feature list in the `mdix-wasm/Cargo.toml` comment. `mdix-java` and `mdix-lua` use
  the local path and have no codec references.
- `Cargo.lock` still lists the removed crates until it is regenerated. A build
  without `--locked` prunes them by itself, and the Sync Cargo.lock workflow does it
  on demand.

## Verification

- Static review only. Nothing was compiled: the environment that made this change
  has no Rust 1.85 toolchain. A search of `dixscript/src` finds no remaining
  reference to the two compressor types or the two features.
- Still to run in CI: `cargo test -p dixscript`, the same with
  `--no-default-features`, `cargo check --all-targets`, and the wasm32 build.

## Fixes and Problems

### `Compressor/mod.rs`
- The module comment said all three compressors build on every target. It now
  describes gzip only.

### `dlm_pipeline_executor.rs`
- The old "compiled without the feature" error told users to rebuild with a feature
  that no longer exists. Replaced by the "no longer supported" message.

### `dlm_reverse_executor.rs`
- Same as above for the decompressor.

### `binary_serialization_benchmark.rs`
- The bench could not compile without the two codec features. It now compiles in every
  configuration.
