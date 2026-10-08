# Cargo features and the dependency graph

`dixscript` is feature-gated so a build only compiles what it uses. This is the
second half of the dependency-reduction pass: [utilities.md](utilities.md) covers
the crates that were removed or hand-rolled, this file covers the ones that were
made optional. Everything below is **on by default**, so `cargo build` behaves as
before; the gating only matters to anyone passing `--no-default-features`.

## The feature graph

| Feature | What it enables | Pulls in |
|---|---|---|
| `cloud-import` | `https://` imports (`@IMPORTS`) | `reqwest`, `tokio` |
| `rayon-support` | parallel work | `rayon` |
| `toml-support` | `DixConverter::to_toml` / `from_toml` | `toml` |
| `dlm` | the DLM *pipeline*: executors, key-file manifest, the module traits, `DixLoader::{load_encrypted, load_from_encrypted_bytes, compile_with_dlm_from_str, decompile_with_dlm_from_bytes}`, `Runtime::key_resolver` | nothing extra |
| `dlm-auditor` | `DAuditor` — `DiyAuditor`, `EnhancedAuditor`, `.mdix.au` files | nothing extra |
| `dlm-compressor` | `DCompressor` — gzip | nothing extra (`flate2` is not optional, see below) |
| `dlm-encryptor` | `DEncryptor` — XOR | nothing extra |
| `bzip2-support` | `DCompressor.bzip2` (implies `dlm-compressor`) | `bzip2` |
| `xz-support` | `DCompressor.lzma` (implies `dlm-compressor`) | `lzma-rust2` |
| `aes128-support` | `DEncryptor.aes128` (implies `dlm-encryptor`) | `aes-gcm` |
| `aes256-support` | `DEncryptor.aes256`, and the encryptor a bare `DEncryptor` means (implies `dlm-encryptor`) | `aes-gcm` (shared with `aes128-support`) |
| `chacha20-support` | `DEncryptor.chacha20` (implies `dlm-encryptor`) | `chacha20poly1305` |
| `argon2-support` | Argon2id: `DEncryptor` password mode and password-protected key files (implies `dlm-encryptor`) | `argon2` |
| `encryption-support` | umbrella: the four features above | all of them |

`dlm-auditor`, `dlm-compressor` and `dlm-encryptor` each imply `dlm`. Cargo unions
features, so "default minus `dlm`" still has `dlm` on because `dlm-auditor` implies
it; to get a build without a family, list the features you do want.

## What "off" means: parse always, execute optionally

`@DLM(...)` is part of the **language**. Its AST, parser, analyzer, binary format
and error types are compiled in every configuration, so a file that declares DLM
modules still parses, validates, serializes and round-trips in a build with no DLM
feature at all. What the features decide is whether a module can **run**.

Asking for a module that is not compiled in is always a clear runtime error that
names the feature to enable. It is never silently skipped, because skipping a
compression, encryption or audit step the author asked for would leave the author
believing it happened:

| Build | A file that declares... | Result |
|---|---|---|
| no `dlm` | any `@DLM` module | `load_text` fails: *"...compiled without the 'dlm' feature..."* |
| no `dlm` | no `@DLM` module | loads exactly as before |
| `dlm`, no `dlm-auditor` | `DAuditor` | *"...'dlm-auditor' feature..."* |
| `dlm`, no `dlm-compressor` | `DCompressor` | *"...'dlm-compressor' feature..."* |
| `dlm`, no `dlm-encryptor` | `DEncryptor` | *"...'dlm-encryptor' feature..."* |
| no `aes128-support` / `aes256-support` / `chacha20-support` | `DEncryptor.aes128` / `.aes256` / `.chacha20` (a bare `DEncryptor` is AES-256) | the same kind of error, naming that algorithm's feature; the other algorithms and `DEncryptor.xor` still work |
| no `argon2-support` | `DEncryptor` in password mode, or a password-protected (Argon2id) key file | *"...compiled without the 'argon2-support' feature..."*; keyfile mode still works |
| no `bzip2-support` / `xz-support` | `DCompressor.bzip2` / `.lzma` | the same kind of error (this pattern pre-dates the pass and the others copy it) |

**Reading an audited file is stricter.** If a key file records that the original
pipeline had a `DAuditor`, then reading it is supposed to leave an audit trail. A
build without `dlm-auditor` refuses to decrypt that file rather than decrypting
without writing the trail.

### Where the gates sit

The pipeline and reverse executors (`dlm_pipeline_executor.rs`,
`dlm_reverse_executor.rs`) tie all three families together through trait objects
(`IAuditor`, `ICompressor`, `IEncryptor`). So the **trait files and `KeyManagement`
stay in the `dlm` core**, and the gates sit on the *implementations* and on the
*factories*: each of `create_auditor`, `create_compressor`, `create_encryptor`,
`create_decryptor`, `create_decompressor` has a twin whose body is the "not compiled
in" error. The loader's `determine_dlm_behavior` and `generate_audit_only` are
split the same way. `KeyManagement` is core, not encryptor-only, because the key
file is the whole pipeline's manifest (compression metadata is in it too).
`Argon2KDF` is compiled whenever `dlm` is, so key files can be read and written
(parameters, salt, metadata) in any build; only its one call into the `argon2` crate
is behind `argon2-support`, which is what lets Argon2 be independent of the
algorithms. The three encryptors needed no changes for that: without Argon2 their
password mode fails at key derivation with the error above.

## What the gating buys

Unique crates in the dependency graph (`cargo tree -p dixscript -e normal`):

| Configuration | Crates |
|---|---|
| default | 162 |
| `--no-default-features` | **44** (it was 99 before this pass) |
| + `dlm`, `dlm-auditor`, `dlm-compressor`, `dlm-encryptor` | 44 |
| ... + `aes128-support` or `aes256-support` (or both) | 55 |
| ... + `chacha20-support` | 54 |
| ... + `argon2-support` | 49 |
| ... + `encryption-support` (all four) | 63 |
| `toml-support` only | 53 |
| `cloud-import` only | 121 |

The four DLM family features add **no** crates: everything they use (`flate2`,
`sha2`, `rand`, `chrono`) is unconditional. The savings come from the crates that
were made optional (the three crypto crates: 18 crates in total, which the split now lets you take one algorithm at a time; `toml`: 9) and from
removing the `url` dependency (28 crates, mostly the ICU/IDNA stack; see
`Utilities/Url` in [utilities.md](utilities.md)). The 44 that remain are
`serde`, `serde_json`, `flate2`, `sha2`, `regex`, `chrono`, `phf`, `memchr`, `rand`
and `web-time` plus their own dependencies.

In **default** builds `url` is still in the graph, because `reqwest` depends on it;
the saving is for builds without `cloud-import`.

## Decisions that shaped this

- **`serde_json` stays unconditional.** It is not just the converter: it backs the
  JSON error payloads in `ErrorManager` and the `version_constraints` API, and
  `DixValue`'s derived `Serialize` is consumed through it by `mdix-ffi`, `-wasm`,
  `-lua`, `-python` and `-java`. Gating it would have saved three crates.
- **`flate2` (gzip) stays unconditional.** It is the default codec (`DCompressor`
  with no subtype means gzip) and the only compressor without a heavy dependency.
  As a consequence, a build with `dlm` off still compiles `flate2` and its four
  small dependencies without using them. Making it optional is a one-line change
  (`optional = true` plus `dep:flate2` in `dlm-compressor`) if that cost matters.
- **chrono's `serde` feature was removed.** Nothing used it: `DixValue` stores dates
  as `String`, and it is the only `Serialize` derive in the crate.
- **Encryption is split per algorithm, with `encryption-support` as the umbrella.**
  AES-128 and AES-256 are separate features that share the one `aes-gcm` crate, so
  enabling either costs the same 11 crates and enabling both costs nothing extra.
  `encryption-support` still exists and still means "all of it", so an existing
  `features = ["encryption-support"]` keeps working.
- **`url` is gone, not gated.** See `Utilities/Url`.

## Test targets and `required-features`

A test or bench that needs a feature declares it in `Cargo.toml`
(`required-features`), so `cargo test --no-default-features` skips it instead of
failing to compile: `enum_converter_json_toml_regression`, `enum_extract`, and the
two TOML benches need `toml-support`; `enum_metadata_binary_regression` needs `dlm`;
`binary_serialization_benchmark` needs `bzip2-support` and `xz-support` (it was
already uncompilable without defaults). With that, `cargo check -p dixscript
--no-default-features --all-targets` is clean for the first time.

## Verification

- `cargo test -p dixscript` (defaults): **703 passed, 0 failed**.
- `cargo test -p dixscript --no-default-features`: **674 passed, 0 failed**.
- `cargo check -p dixscript --lib` for every feature alone (14 of them), the defaults
  with each non-implied one removed, and the family-removed sets; plus the encryptor
  combinations that matter (no algorithms, AES-256 only, all algorithms but no
  Argon2, Argon2 only), each with its gating tests. All compile, and no reduced build
  has a warning the default build does not.
- The error paths in the table above are tested directly (`Runtime::loader::tests`)
  under the exact feature set that puts each one in play: no `dlm`, no
  `dlm-auditor`, no `dlm-compressor`, no `dlm-encryptor`, and each of
  `aes128-support`, `aes256-support`, `chacha20-support` missing on its own (with the
  other algorithms present, so a leak between algorithms would show). A test in
  `argon2_kdf.rs` covers a missing `argon2-support`. A last test checks a plain file
  loads in every configuration.
- `--all-targets` (defaults and none), `mdix-lua`, `mdix-java`: all compile.
- **Not built:** `wasm32`.

## Open questions

- **Binary pack/unpack lives under `dlm`.** `DixLoader::compile_with_dlm_from_str`
  and `decompile_with_dlm_from_bytes` are also the crate's in-memory binary
  serialization API, and work for files with no DLM modules at all; they are gated
  because they return `DLMPipelineResult`. A lean build therefore has no in-memory
  binary API. A small DLM-free pair (`BinaryPacker` is already outside `dlm`) would
  fix that.

**Decided:** a declared-but-unavailable module fails the whole load rather than being
skipped (skipping would drop a compression, encryption or audit step the author asked
for), and encryption is split per algorithm.
