# dixscript — Utilities

Part of [dixscript](../dixscript.md). Covers `dixscript/src/Utilities/` —
specifically the **dependency-reduction pass** started in this part. The
older files in `Utilities/` (`keyword_definitions.rs`, `mid_logger.rs`,
`token_debug_printer.rs`, ...) are not documented here yet; treat them as
not yet documented rather than undocumented on purpose.

## Overview

Goal: shrink the core crate's `[dependencies]` by removing anything dead and
hand-rolling anything small enough to own. Dev-dependencies (`criterion`,
`tempfile`, `bincode`, `rmp-serde`, `postcard`, `ciborium`, `bumpalo`) are out
of scope.

Every decision below came from reading real call sites and, where a
replacement was built, from testing it against the exact locked version of the
crate it replaces. Counts are from a scripted scan of `dixscript/src`.

| Crate | Real usage | Public API? | Status |
|---|---|---|---|
| `itertools` | 0 call sites anywhere | — | **Removed** |
| `unicase` | 0 call sites anywhere | — | **Removed** |
| `serde` | `#[derive(Serialize)]` on `DixValue`; 5 sibling binding crates serialize `&DixValue` through it | yes | **Kept** — first judged removable, wrongly |
| `hex` | 2 call sites (`hex::encode(sha256)`) | no | **Wired in** |
| `lazy_static` | 18 `static ref` items in 6 blocks / 6 files | no | **Wired in** |
| `bitflags` | 1 invocation (`SectionFlags`) | **yes** | **Wired in** — faithful port of 2.10.0 |
| `rustc-hash` | 188 mentions of `FxHashMap`/`FxHashSet`, 13 files | **yes (11 items)** | **Wired in** — port of 2.1.1, 13 imports via the scaffold patch |
| `base64` | 35 calls, 11 files, `general_purpose::STANDARD` only | no | **Wired in** |
| `uuid` | 13 call sites, 4 files (`new_v4`, `parse_str`, `nil`, `from_bytes`) | no | **Wired in** |
| `hostname` | 1 call site, diagnostics only | no | **Wired in** (best-effort, see its section) |
| `async-trait` | 2 attribute sites on the `CloudStorageProvider` trait | **yes** | **Wired in** — hand-desugared to the macro's exact expansion |
| `url` | 1 call site | no | **Kept for now** — see Open items |

"Public API?" means the crate's own types appear in a signature or field of
something reachable from outside the crate. That column decided how the wiring-in
was staged: the five rows marked **no** were wired in first; the three rows
marked **yes** change what downstream users of the published crate see, so they
were held back until the decision was made. It was: **hand-roll every crate on
the list and stick to the plan** — so all eight are now wired in, and the three
public-API ones are `pub` modules whose ports reproduce the real crates' exact
public shape (see each module's section and "Decisions").

Kept on purpose, with the reason, so nobody re-litigates it:

- `regex` — 22 files, includes a user-facing `Regex` value type. Not hand-rollable.
- `serde` — `DixValue`'s derived `Serialize` is the JSON escape hatch for
  `mdix-ffi` (`mdix_get_json`), `mdix-wasm` (`getJson`), `mdix-lua`
  (`get_json`), `mdix-python` and `mdix-java`.
- `serde_json`, `toml` — real *parsing* for JSON/TOML import, not just writing.
- `phf`, `memchr` — lexer hot path; hand-rolling risks a throughput regression
  the bench suite would only catch after the fact.
- `sha2`, `aes-gcm`, `chacha20poly1305`, `argon2` — cryptographic primitives.
  Never hand-rolled regardless of size.
- `rand` — the crypto path (key/salt/nonce via `OsRng`) requires it, and the
  new `Uuid` builds on it.
- `flate2`, `bzip2`, `lzma-rust2` — real codecs.
- `chrono` — backs real `Date`/`Timestamp` literal parsing with calendar
  validation. Hand-rollable in principle, easy to get subtly wrong.
- `web-time`, `web-sys`, `rayon`, `reqwest`, `tokio`, `getrandom` — already
  minimal, gated, or required.

## Conventions

- **Casing:** directories under `Utilities/` are PascalCase and named after the
  crate they replace (`Hex`, `Base64`, `Uuid`, ...); files inside are
  snake_case. This matches the rest of the crate (`Compiler/DLM/Auditor/`,
  `Compiler/Core/SectionParsers/`, `Builtins/Static/`).
- **Visibility:** the five internal-only modules (`Base64`, `Hex`, `Hostname`,
  `LazyStatic`, `Uuid`) are `pub(crate)` and not re-exported. The three whose
  types reach this crate's public API (`AsyncTrait`, `Bitflags`, `RustcHash`)
  are `pub mod`, because downstream code must be able to name `FxHashMap`,
  `SectionFlags`'s `Flags` impl and `BoxFuture`. The `bitflags!` macro and its
  helper macros stay crate-private.
- **Drop-in surface:** each module reproduces the *call-site syntax* of the crate
  it replaces where it can (same macro grammar, type names, function names,
  `Engine` trait and `general_purpose::STANDARD`), so wiring in was one `use`
  line (or one inline path) per site. `AsyncTrait` is the exception: it is a
  recipe applied by hand at the two sites (the trait and its one impl).
- **Differential tests, and the real crates as oracles.** Each module that
  replaces a crate with checkable behavior has tests that run the same inputs
  through both and require identical results. They call the real crate as
  `::name` (leading `::` forces the extern crate, since a same-named local
  module exists). When a crate is dropped from `[dependencies]`, **move it to
  `[dev-dependencies]` instead of deleting it** — the tests then keep guarding
  the replacement for good. That is what was done for `async-trait`, `base64`,
  `bitflags`, `hex`, `hostname`, `rustc-hash` and `uuid`; `hostname` and `uuid`
  sit under `[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]`.
  `lazy_static` has no oracle test and was removed outright.
- **`allow(...)` on the module lines in `Utilities/mod.rs`.** The five internal
  modules keep `allow(dead_code, unused_imports)` because each deliberately
  offers a little more than the crate calls (`Hex::decode`, extra re-exports).
  The three public ones keep `allow(dead_code, unused_imports, unused_macros)`:
  the ports reproduce whole upstream APIs, most of which this crate never calls.

## Modules

### `Utilities/Hex/hex.rs`

`encode(bytes) -> String` (lowercase) and `decode(&str) -> Result<Vec<u8>, String>`
(either case accepted).

- Only `encode` is called anywhere (`enhanced_auditor.rs`, `diy_auditor.rs`,
  each hashing a SHA-256 digest). `decode` is included for symmetry and is
  **not exercised by any real caller** — don't treat it as battle-tested.
- `decode` returns `Result<_, String>`, not the real crate's `FromHexError`;
  nothing consumes the error type.
- Differential tests against `hex` 0.4: encode on random bytes; decode on
  random valid and damaged strings (accept/reject and bytes).

### `Utilities/Bitflags` — a port of `bitflags` 2.10.0

`bitflags_macro.rs` (the `bitflags!` macro and the helper macros it expands to),
`traits.rs` (`Flags`, `Flag`, `Bits`, `Primitive`, `PublicFlags`), `iter.rs`
(`Iter`, `IterNames`, `IterDefinedNames`), `parser.rs` (the text format).
`mod.rs` is `pub`; the macro itself is `pub(crate)` (only `SectionFlags`, in
`binary_format.rs`, invokes it).

**Why a port and not a re-derivation.** `SectionFlags` is a public type used in
public signatures (`BinaryHeader::add_section`, `has_section`, the public field
`BinaryHeader::flags`). The real macro generates a two-layer type — a public
tuple struct wrapping a hidden `InternalBitFlags` — and implements `Flags`,
`Debug` (printing names), `Display`, `FromStr`, `Binary`/`Octal`/`LowerHex`/
`UpperHex`, `Extend`, `FromIterator`, `IntoIterator`, every set operator, and a
const-fn surface. An earlier draft of this module was a trimmed macro written
from the documentation; it printed `SectionFlags(5)` where the real crate prints
`SectionFlags(CONFIG | DATA)`, and lacked `iter()`, `from_name()` and the `Flags`
trait. It was replaced, not patched: the real upstream source was read and
ported, mechanically by a throwaway script that is not kept in the repo
(extract the macros, rewrite every `$crate::` to `$crate::Utilities::Bitflags::`,
turn the macro exports into crate-private `pub(crate) use`, keep the upstream
`std` shape of `parser.rs`), so nothing was retyped by hand.

- **Ported:** everything the `bitflags!` macro generates in `struct` mode and
  `impl` mode, unnamed flags (`const _ = !0;`), `#[cfg]` on flags, signed and
  128-bit bit types, the full `Flags` trait, the three parser modes
  (`from_str`, `_truncate`, `_strict`) and their writers, and upstream's `const fn`
  set.
- **Not ported:** the serde / arbitrary / bytemuck glue (`external`), the
  `bitflags_match!` macro, and the deprecated `BitFlags` trait.
- **Differential tests (12)** declare the same flags type with the real crate
  (`::bitflags`, a dev-dependency) and with the port and compare every observable
  result: all 256 values through every accessor and every text format
  (`{:?}`, `{:#?}`, `{:b}`, `{:o}`, `{:x}`, `{:X}`), all 65,536 value pairs
  through every binary operation, `-Assign` forms and mutators, the parser on 36
  adversarial strings in all three modes (error text included), iterators
  including `remaining()`, `Extend`/`FromIterator`/`IntoIterator` on 2,000
  random sets, the const-fn surface evaluated in `const` context, and the
  `impl` / unnamed / `#[cfg]` / signed / `u128` / zero / empty forms.
- **Upstream's own unit tests (43)** are copied under `upstream_tests/` and run
  against the port with only the `crate::` paths rewritten.
- `section_flags_debug_prints_flag_names_like_the_real_crate` pins the one
  observable change from the old draft on the real, wired type.

### `Utilities/LazyStatic/lazy_static_macro.rs`

A `lazy_static!` macro expanding to
`static NAME: std::sync::LazyLock<T> = LazyLock::new(|| expr);`.

- Barely hand-rolling: `LazyLock` (stable since Rust 1.80; `rust-version` is
  1.85) already does what `lazy_static` exists for. The macro only preserves
  the existing `static ref NAME: Type = expr;` syntax so the 18 items don't
  change.
- Only the macro is used across the crate — no `lazy_static::initialize()`, no
  `pub static ref` — but visibility and a final item with no trailing semicolon
  are supported for parity with the real grammar.
- One real difference: the real macro generates a distinct hidden type per
  static; `LazyLock<T>` is one std type. Every use is `NAME.method()` or
  `&*NAME`, both fine either way.
- Not public API: every static is private.

**Two things the wiring-in taught.**
- *The recursive step had to become path-based.* A block with several items
  expands one at a time by calling the macro again, and that inner call was
  written with the bare name, which only resolves where the caller has imported
  the macro. `quickfuncs_section_parser.rs` invokes it by full path with no
  import, and even a single-item block makes a final empty recursive call. The
  inner call is now `$crate::Utilities::LazyStatic::lazy_static!`.
- *The swap exposed two dead statics.* The real macro wraps each static in a
  hidden type carrying `#[allow(dead_code)]`, so an unused `static ref` never
  warned; a bare `LazyLock` static does. Wiring it in raised the crate's warning
  count from 129 to 131 — `INTERP_METHOD_CALL_RE` and `INTERP_PROPERTY_RE` in
  `quickfuncs_section_parser.rs` are never used and had been invisible. The macro
  now adds `#[allow(dead_code)]` to every static it generates so the swap changes
  no warnings (129 before, 129 after, same set). The two statics are a **cleanup
  candidate**, not deleted here: they look like leftovers of the interpolation
  code that already produces `parse_interpolated_expression is never used`.

**Verified against the real `LazyLock`:** the earlier gap (sandbox rustc 1.75
could not compile `LazyLock`) is closed. The macro's four tests and every wired
static now run in the real crate on Rust 1.85.1, the crate's declared
`rust-version`.

### `Utilities/RustcHash` — two hashers, one default

`rustc_hash_v2.rs` is the default export: a port of `rustc-hash` **2.1.1**, the
version `Cargo.lock` resolves. `fx_hash.rs` is the classic FxHash copied from
`mid-engine` (`crates/mid-collections/src/fx_hash.rs`), kept and exported as
`RustcHash::classic` (crate-private). To make the classic one the default,
change one `pub use` line in `RustcHash/mod.rs`.

**Why the default is not the file that was asked for.** The instruction was to
copy `mid-engine`'s FxHash. That was done, but comparing it with the crate it
would replace showed it is a different algorithm (the classic 1.x
rotate-xor-multiply, versus 2.x's add-multiply with a rotating `finish` and a
wyhash-style `hash_bytes`), and not a safe drop-in. Release build, 100k
entries, minimum of 9 runs, one sandbox VM — indicative, not a guarantee for
other hardware:

| Workload | classic (mid-engine) | my 2.1.1 port | real `rustc-hash` 2.1.1 |
|---|---|---|---|
| `&str` keys, insert + lookup | 6.46 ms | 4.45 ms | 4.84 ms |
| `u64` keys, stride 1 | 1.67 ms | 1.76 ms | 1.83 ms |
| `u64` keys, stride 4096 | **77.08 ms** | 1.75 ms | 1.71 ms |
| 48-key set, 20k lookups each | 6.96 ms | 6.25 ms | 6.44 ms |

- The classic algorithm gives **different hash values**, so a map built with it
  iterates in a different order than one built with `rustc-hash` 2.1.1 (shown
  directly: the same 12 inserts iterate as `[9,6,0,5,…]` vs `[0,11,10,4,…]`).
- It is roughly **a third slower on string keys**, which are ~100 of this
  crate's ~125 `FxHashMap`/`FxHashSet` type sites.
- It is **~44× slower on integer keys with a power-of-two stride**: weak low
  bits, and hash tables pick buckets from the low bits. That is the weakness the
  2.x rewrite exists to fix. The crate has only three `i32`-keyed maps, so it is
  unlikely to bite here.

**The port is checked for exact equivalence, not similarity:** identical
`finish()` values on 50,000 random sequences of every `write_*` call and byte
strings of 0–70 bytes (covering each branch of `hash_bytes`); identical
`hash_one` for `String`, `&str`, `i32`, tuples and a derived-`Hash` enum; and
**identical iteration order** for `FxHashMap` and `FxHashSet` after 300 random
insert/remove sequences each. It also carries upstream's own known-answer
vectors. Speed is at parity within noise.

**Iteration-order audit (why order matters, and where it reaches).** Twelve
sites in `src/` iterate an `FxHashMap`/`FxHashSet`; a looser second scan found
no more that were real. Ten are order-independent (`.any()`, or collected into
a standard `HashMap`, whose order is already randomized per process, or
explicitly sorted first). One is a verbose debug log line
(`ast_walker.rs`). One (`FunctionRegistry::function_names`) has no callers.
So hash order does not reach user-visible output in `src/`. This was a static
scan; it cannot see iteration through a returned value it couldn't type, which
is one more reason to prefer the port, since identical hashes make the question
moot.

**Licensing.** `rustc_hash_v2.rs` is derived from `rustc-hash` 2.1.1, licensed
`Apache-2.0 OR MIT`. The file header attributes it. Whether to add an entry to a
third-party notices file is not decided here.

**32-bit targets.** Both pointer widths are ported (`wasm32` is 32-bit).
Upstream's 32-bit known-answer vectors are included but could only be run on
such a target, which wasn't possible here.

### `Utilities/Base64/base64_codec.rs`

Standard-alphabet, padded base64: `encode`, `decode`, a `DecodeError` with the
real crate's variants and `Display` text, and the `Engine` trait plus
`general_purpose::STANDARD` so `STANDARD.encode(..)` / `.decode(..)` call sites
keep their shape. Replaces `base64` 0.21.7.

- **Why decode is a careful port, not a sketch:** most call sites use decode as
  a *validity check on user text* — `data_section_analyzer.rs` rejects a `.mdix`
  blob whose payload fails `decode(..).is_err()`, `binary_format.rs` uses
  `.is_ok()`, and the DLM encryptors fall back to a generated key when a key
  string won't decode. A decoder even slightly looser or stricter changes which
  files are accepted. `dix_value.rs` also formats the error into a user-visible
  message, so the variant, offset, byte and `Display` text all match.
- The real decoder's rules, in the order they take precedence: (1) a length
  remainder of 1 mod 4 is `InvalidLength` — checked *first*, before any byte,
  unless the last byte is itself an invalid non-padding symbol; (2) everything
  before the final ≤8-byte group must be alphabet symbols (a `=` there is
  `InvalidByte`); (3) in the final group, padding only at position 2 or 3 of a
  4-symbol block, running to the end, and anything after it is `InvalidByte`
  pointing at the *first* padding byte; (4) padding must be canonical
  (`InvalidPadding`); (5) non-zero leftover bits in the last symbol are
  `InvalidLastSymbol`.
- **Differential tests** against the real crate: 20,000 random encodes; 150,000
  valid encodings with one random edit (replace / delete / insert / truncate /
  append); 300,000 random inputs from a symbol-weighted pool; and *exhaustive*
  enumeration of every string up to length 8 over `{A,B,=,!}` and up to length
  12 over `{A,=,!}` (about 885,000 strings, covering every padding position and
  every length around the 8-byte group boundary). About 1.3 million decodes are
  compared for accept/reject, decoded bytes, error variant and offset, and
  `Display` text. Each test asserts that every outcome (`Ok` and all four error
  variants) actually occurred, so it cannot pass by never reaching a branch.
- **The tests can fail:** three deliberate bugs were injected in a scratch copy
  (padding allowed one position early; padding and trailing-bit checks swapped;
  the length pre-check's last-byte special case removed) and each was caught
  with a concrete input.
- **A hand-written expectation of mine was wrong** and only the real crate
  showed it: I asserted `decode("Zm 9v")` was `InvalidByte(2, ' ')`, but it is
  `InvalidLength` — five bytes, and the length rule fires first. The decoder was
  right; the test was corrected. This is why the rules above were ported from
  the source rather than from expectation.
- Not public API: `base64` types appear in no public signature.
- The 17 `use base64::…` lines are five shapes (13 are
  `use base64::{Engine as _, engine::general_purpose};`), so wiring it in was a
  mechanical swap of 17 `use` lines in 11 files.

### `Utilities/Uuid/uuid.rs`

A `Uuid` type with exactly the API the crate calls: `new_v4`, `parse_str`, `nil`,
`from_bytes`, `as_bytes`, `as_fields`, `Display` (hyphenated, lowercase),
`simple()` and `hyphenated()`. Replaces `uuid` 1.19.0.

- `Guid.parse` / `validate` / `tryParse` expose the accepted spellings directly
  to `.mdix` authors, so `parse_str` follows the real parser: dispatch on byte
  length (32 simple, 36 hyphenated with hyphens exactly at offsets 8/13/18/23,
  38 braced, 45 `urn:uuid:` + hyphenated), hex digits either case, anything else
  rejected (including multi-byte UTF-8, which changes the byte length).
- `new_v4` draws 16 bytes from `rand::rngs::OsRng` and sets the version and
  RFC 4122 variant bits. On `wasm32` that reaches the browser through
  `getrandom` 0.2 with its `js` feature, which the crate already enables and
  which the DLM encryptors already rely on unconditionally. Not exercised on a
  wasm target here.
- `UuidParseError` carries no detail: every caller discards it.
- **Differential tests** against the real crate: text forms and `as_fields` on
  20,000 random UUIDs; `parse_str` accept/reject and parsed bytes on 200,000
  generated inputs (all four spellings, uppercase, wrong-bracket, trailing
  hyphen, then 0–2 random near-miss edits including a multi-byte character) and
  100,000 arbitrary short strings. Both the accepted and rejected sides are
  asserted to be large, so the corpus can't be lopsided. Two injected bugs (a
  hyphen accepted at offset 9; uppercase rejected) were both caught.
- Not public API: `uuid::Uuid` appears in no public signature.
- Dropping the crate also drops its `v4`, `serde` and `js` features.

### `Utilities/Hostname/hostname.rs`

`get() -> io::Result<OsString>`, best-effort, safe `std` only. **This is not
equivalent to the real crate, on purpose.** The real crate calls the OS's
`gethostname` / `GetComputerNameExW`; doing that without a dependency means
`unsafe` FFI or `libc`, which isn't worth it for one cosmetic line in a
diagnostic dump (`diagnostic_dumper.rs`, which `wasm32` never reaches).

Lookup order: Windows `COMPUTERNAME`; Unix `/proc/sys/kernel/hostname`, then
`/etc/hostname`, then `HOSTNAME`, then `HOST`, then the `hostname` program
(covers macOS and the BSDs, which have neither file); else `Err(NotFound)`,
which the call site's `unwrap_or_default()` turns into an empty string exactly
as it did when the real crate failed.

Known ways it can differ: Windows `COMPUTERNAME` is the upper-cased NetBIOS name
and can differ from the DNS host name; `/etc/hostname` can lag a name changed at
runtime (only reached when `/proc` is unavailable); an environment variable can
be stale; on Android none of the files exist and the `hostname` program may not.
Every one of those changes a line of diagnostic text and nothing else.

Tests: `clean()` trimming, and a Linux-only differential test asserting the
result equals the real crate's byte for byte (on Linux the kernel value *is*
what `gethostname` returns). It compared real values in the sandbox (both `vm`),
not just the "found nothing" branch.

### `Utilities/AsyncTrait/boxed_future.rs`

A `BoxFuture<'a, T>` alias and a written recipe — **not a macro**. `#[async_trait]`
is a proc-macro; writing one means a `proc-macro = true` crate and `syn` +
`quote`, the dependency this pass exists to remove, and it is needed in only two
places (the `CloudStorageProvider` trait and `HttpCloudProvider`'s impl).
`mod.rs` is `pub`: the trait is public and its methods return `BoxFuture`.

- Native `async fn` in traits isn't dyn-compatible, and `CloudStorageProvider`
  is used as `Arc<dyn CloudStorageProvider + Send + Sync>`. A method returning a
  boxed future is. That is the whole job `async-trait` does.
- **The recipe reproduces the macro's exact expansion** — three lifetimes
  (`'life0`, `'life1`, `'async_trait`) and the `'life0: 'async_trait`,
  `'life1: 'async_trait`, `Self: 'async_trait` bounds — not a simpler
  one-lifetime signature. The simple form was the first draft and it would have
  broken every external `#[async_trait]` implementor of this public trait with
  **E0195** ("lifetime parameters or bounds on method do not match the trait
  declaration"). That was checked directly: an `#[async_trait] impl` against a
  one-lifetime trait fails with E0195 on the locked `async-trait` 0.1.89; against
  the exact expansion it compiles. Earlier notes in this file said the change
  "breaks implementors"; with the exact signature it does not.
- Bodies are untouched: `Box::pin(async move { <original body> })`.
  `return Err(..)` inside the block still returns from the block; the one caller
  (`block_on(provider.download_file_async(url))`) is unchanged.
- **Tests (6), with the real crate as oracle in both directions:** a hand-written
  impl behind `Arc<dyn Trait + Send + Sync>` (crossing a real `Pending`), `return`
  and `?` inside the block, borrows of non-`'static` locals, `Send`-ness of the
  future, **an `#[async_trait]` impl compiled against the hand-written trait**,
  and **a hand-written impl compiled against an `#[async_trait]` trait**.
- **Honest scope of the saving:** it removes the `async-trait` crate but not
  much of the build — its `proc-macro2`, `quote` and `syn` dependencies are also
  pulled in by `serde_derive`.

### `Utilities/test_rng.rs`

Test-only (`#[cfg(test)]`). A deterministic xorshift64 so the differential tests
replay identically on every machine. Not used by anything at runtime;
`Uuid::new_v4` uses the OS entropy source.

## Verification summary

How it was verified: the sandbox's default toolchain is 1.75, but Ubuntu's
archive also ships `rustc-1.85`/`cargo-1.85` (`apt-get install rustc-1.85
cargo-1.85`, then `PATH=/usr/lib/rust-1.85/bin:$PATH`), which is exactly the
crate's declared `rust-version`. That made it possible to compile and test the
real crate rather than extracted copies. Earlier statements in this project that
the crate could not be compiled here, and that `LazyLock` could not be tested,
were true of 1.75 and are no longer true.

| Item | Status |
|---|---|
| The real crate builds | `cargo check -p dixscript --lib` on Rust 1.85.1: **0 errors**. |
| The full test suite | `cargo test -p dixscript`: **696 passed, 0 failed** (534 library tests plus every integration suite and the doc tests). Includes `schema_section_tests` (42) and `raw_section_tests` (16). |
| Warnings | **129**. Pristine `HEAD` (before the three public-API modules were wired) had 140: the extra 11 were `private_interfaces` warnings from `pub` items that already named the crate-private `FxBuildHasher`; making `RustcHash` `pub` removes exactly those, and no other warning appeared or disappeared. |
| Other targets | `cargo check -p dixscript --all-targets` (tests, benches, examples) and `--lib --no-default-features` both succeed. |
| Sibling crates | `mdix-lua` and `mdix-java` — the only two that depend on the local path — both compile against the wired crate. `mdix-lsp`, `mdix-wasm`, `mdix-ffi`, `mdix-python` and `mdix-cli` resolve the *published* `dixscript` 1.0.0 deliberately (their manifests say so), so they were not rebuilt. |
| Differential tests | Every replacement is compared with the exact locked crate it replaces (`async-trait`, `base64`, `bitflags`, `hex`, `hostname` on Linux, `rustc-hash`, `uuid`): ~1.3M base64 decodes, 300k uuid parses, every byte and byte pair for bitflags plus the parser and the const-fn surface, 300+300 map/set iteration-order comparisons, and both directions of `#[async_trait]` interoperability. |
| Upstream's own tests | The 43 unit tests of `bitflags` 2.10.0 run unchanged (only `crate::` paths rewritten) against the port. |
| Do the tests have teeth? | Deliberate bugs were injected and caught: 3 in base64, 2 in uuid, 2 in the `rustc-hash` port, 3 in the bitflags port (set difference, complement, text separator — 2, 4 and 5 tests failed respectively), and the one-lifetime `async-trait` signature fails with E0195. |
| Names don't collide | No existing type, module or glob import in `src/` shares a name with the new modules. |
| Cargo.lock | **Still stale on `master`; run Sync Cargo.lock.** `HEAD`'s lock still lists `itertools`, `lazy_static` and `unicase` as dependencies of `dixscript` (removed from `Cargo.toml` in the first batch, lock never regenerated), so a `--locked` build refuses with "needs to be updated" — on pristine `HEAD` too, independent of this delivery. A plain build rewrites it; the whole delta is exactly those three edges (no package leaves the graph, because other crates still use them), and moving crates to `[dev-dependencies]` adds nothing. It is not shipped in the archive: a root-level `Cargo.lock` has no directory in its path, so the resolver would match it by basename. |
| Inactive `cfg` branches | The 32-bit constants in `rustc_hash_v2.rs` and the Windows, unix and wasm variants of `hostname::get()` were forced on, one at a time, in an isolated copy and type-checked with the real compiler: all compile. That proves they compile, not that they behave correctly on a real 32-bit or wasm target. |
| `wasm32` | **Still not built or run.** No `wasm32` standard library is packaged for this toolchain. Your CI's `cargo build -p dixscript --target wasm32-unknown-unknown` is the real check. What it exercises that nothing here did: `Uuid::new_v4` through `getrandom` 0.2 with its `js` feature (already enabled for wasm and already relied on by the DLM encryptors), and the 32-bit hasher at runtime. |

## Decisions

The three replacements that touch **public API of the published crate** were
held back until decided. The decision: **hand-roll every crate on the list and
stick to the plan**; all of these may change. What that means for each:

1. **`rustc-hash` — 11 public items expose `FxHashMap<String, …>`**
   (`ExecutionContext::{variables, new, get_all_variables}`, `FunctionInterpreter::{captured_env, new,
   new_with_error_manager, execute, evaluate_arguments_in_caller_context}`,
   `FunctionCallInfo::scope_context`, `ScopeTracker::get_scope_variables_snapshot`,
   `DataSectionAnalyzer::get_indexes`). The type a downstream user sees changes
   from `HashMap<_, _, rustc_hash::FxBuildHasher>` to
   `HashMap<_, _, dixscript::Utilities::RustcHash::FxBuildHasher>`, so code that
   passes a `rustc_hash::FxHashMap` in stops compiling. **This is a breaking
   change** and was accepted as one; it needs a version bump when published. The
   port reproduces `rustc-hash` 2.1.1's hashes and iteration order exactly, so
   behaviour (as opposed to type identity) is unchanged.
2. **`bitflags` — `SectionFlags` is public.** Because the port is the upstream
   source and not a re-derivation, the public surface is the real crate's:
   `SectionFlags` implements `Flags`, `Debug` prints names, and `iter()`,
   `iter_names()`, `from_name()` exist. The one change visible to a downstream
   user is that `Flags`/`Bits` are now `dixscript::Utilities::Bitflags::{Flags,
   Bits}` rather than `bitflags::{Flags, Bits}`; code that calls the inherent
   methods is unaffected.
3. **`async-trait` — `CloudStorageProvider` is a public extension point.** The
   hand-desugared signatures are the macro's exact expansion, so an external
   `#[async_trait] impl CloudStorageProvider for X` still compiles (tested against
   the real macro). Not source-visible, but the trait no longer carries the
   attribute, so documentation tools show the boxed-future signature.

**Wired in:** everything on the list. First batch — `hex`, `lazy_static`,
`base64`, `uuid`, `hostname` (23 files). Second batch — `rustc-hash` (13
imports, applied with the scaffold patch because it was the same edit in many
files), `bitflags` (`binary_format.rs`), `async-trait` (the trait declaration and
`http_cloud_provider.rs`). `hostname` carries the documented behavior gap versus
the real crate; the others are checked equivalent.

Per the convention above, each wired crate went to `[dev-dependencies]` as an
oracle rather than being deleted (except `lazy_static`).

## Open items and deferred plan

**Explicitly deferred (per the plan for this pass):**
- **Feature-gate the DLM modules properly, plus JSON/TOML and "a few more
  things".** `regex` stays (Rust's standard library has no regex engine);
  `serde_json` and `toml` are to be *gated*, not replaced. Candidates already found:
  - `flate2` is **not** feature-gated, unlike its siblings `bzip2` and
    `lzma-rust2`. Confirm whether gzip is meant to be the always-on codec.
  - `url` is documented in `Cargo.toml` as "optional — activated via the
    `cloud-import` feature", but has **no `optional = true` and does not
    appear under `[features]`**. It always compiles in, in every configuration
    including `--no-default-features`. (`async-trait` had the same problem; it is
    gone now.) The old comment was wrong and has been replaced with an accurate one.
  - Consequence: `url` cannot yet be swapped for `reqwest::Url` (a re-export of
    the same type). `cloud_file_cache.rs` compiles unconditionally, but `reqwest`
    is genuinely optional, so the swap would break a `--no-default-features`
    build. Do the gating and the swap together.
  - `chrono` is declared with a `"serde"` feature. **Not investigated** — because
    `DixValue` still derives `Serialize`, it is not known whether that flag is
    load-bearing.
- **`wasm32`:** not built here; see the verification table.
- **Version bump.** The `rustc-hash` change is source-breaking for downstream
  users of the 11 public items (see Decisions).

**Optional follow-ups:**
- **Cleanup candidate:** `INTERP_METHOD_CALL_RE` and `INTERP_PROPERTY_RE` in
  `quickfuncs_section_parser.rs` are referenced only by
  `parse_interpolated_expression`, which is itself never used, so they are dead
  transitively (see `LazyStatic`). Not deleted here.
- Benchmark the `RustcHash` port against the real crate on `dixscript`'s own
  workloads (parity was measured only in a scratch crate).
- `Hostname` on Windows/macOS behaves differently from `gethostname` in the ways
  its section documents; nothing here ran on those platforms.
- Decide whether to add a third-party-notices entry for the files derived from
  Apache-2.0 / MIT sources (`Bitflags/*` from `bitflags` 2.10.0, `RustcHash/rustc_hash_v2.rs`
  from `rustc-hash` 2.1.1). The file headers already attribute them.

**Not part of this pass but found on the way:**
- `@SCHEMA` has no editor support yet (VS Code grammar, LSP hover and semantic
  tokens), and `mdix merge` does not merge `@SCHEMA`.
- `mdix-lsp`'s `goto_definition.rs` has an exhaustive `match` on `SectionId`
  with no `Raw`/`Schema` arm; it compiles only because `mdix-lsp` uses the
  published `dixscript` 1.0.0, and will break the day that dependency moves.

**Docs hygiene, unrelated to dependencies:** `docs/RUST_AND_CRATE_GUIDELINES.md`
describes the `mid-engine` workspace (`mid-math`, `mid-ecs`, ...), not this one.
It looks like a template that was never adapted for this repo. Not touched.

## Fixes and Problems

### The wiring-in sweep (23 files)
Rewrote call sites for `lazy_static` (6 files), `hex` (2), `base64` (11 files, 17
`use` lines in five shapes), `uuid` (4) and `hostname` (1); moved `base64`, `hex`,
`hostname` and `uuid` to dev-dependencies as oracles and removed `lazy_static`
outright. Each scripted replacement asserted its exact match count, and a final
scan confirmed no reference to the five crates remains in `src/` outside the
replacement modules.
- A first draft of the sync step dropped the last file in the list (a shell loop
  reading a newline-terminated list from a file written without a trailing
  newline); caught by comparing every changed file with its copy before trusting
  the result.
- The macro's bare-name recursion and the `dead_code` masking were both found by
  compiling for real, not by review — see `LazyStatic`.

### `Cargo.toml`
- Moved `base64`, `hex`, `hostname` and `uuid` to dev-dependencies and removed
  `lazy_static` (see above). `uuid`'s `v4`, `serde` and `js` features went with
  the runtime dependency.
- Removed `itertools` and `unicase`: zero call sites anywhere in the repo
  (`src/`, `tests/`, `benches/`, `examples/`, and every other workspace crate).
- **`serde` — removal attempted, then reversed. Two wrong conclusions in a row.**
  1. The first scoping pass called `serde` removable because in `dixscript/src/`
     its only use was `#[derive(Serialize)]` on `DixValue`, and none of the
     crate's *own* `serde_json::to_string` calls serialized a `DixValue`. True,
     and irrelevant: the consumers are in sibling workspace crates.
  2. A second check found `benches/binary_serialization_benchmark.rs` uses `serde`
     directly, and `serde` was moved to `[dev-dependencies]`. That fixed the bench
     but was wrong for the same reason as (1).
  The evidence: `mdix-ffi` (`mdix_get_json`), `mdix-wasm` (`getJson`), `mdix-lua`
  (`get_json`), `mdix-python` and `mdix-java` all call
  `serde_json::to_string(&DixValue)`, where the argument is the `&DixValue`
  returned by `DixData::get_value`. A grep for a `DixValue`-typed argument found
  nothing (the variable is called `value`/`v`); a grep for the *call shape* found
  all five (`mdix-lua` and `mdix-java` depend on the local path, so removing the
  derive would have failed this workspace's own build at once; `mdix-ffi`,
  `mdix-wasm` and `mdix-python` resolve the published 1.0.0 and would have broken
  on the next release). The derive stays, `serde` stays a runtime dependency, and
  `Cargo.toml` carries a comment saying why.
  **Lesson: "unused" has to be checked across the whole workspace, every sibling
  crate, and by call shape rather than by the argument's type name.**
- Attempted to remove `url` in favor of `reqwest::Url`; reverted after finding the
  gating mismatch above. Only the misleading comment was changed.

### `Utilities/RustcHash` — the first delivery overclaimed
The first batch shipped only the `mid-engine` FxHash and described it as "the same
algorithm" as `rustc-hash`, with "parity expected" and a swap that would be "a
pure import-path change". None of that was checked, and all of it was wrong: it is
a different algorithm from the 2.1.1 the crate uses (different hashes, different
iteration order, slower on strings, pathological on strided integers), and 11
public items expose the type. The docs and module headers were corrected, the
2.1.1 port was added and made the default, and the classic file was kept as
`classic`. The port is the one place this pass departs from the literal
instruction ("copy it from mid-engine"); reverting is one line in `RustcHash/mod.rs`.

### `Utilities/Base64` — a wrong test expectation, caught by the oracle
See the base64 section: an assertion written from reading the source contradicted
the real crate, and the real crate was right.

### Usage counts corrected
Earlier summaries said `base64` had "17 sites"/"16 files", `uuid` "5 sites" and
the Fx types "178 call sites". Recounted by script: 35 calls in 11 files, 13 call
sites in 4 files, and 188 mentions in 13 files. Code comments and this document
were fixed.

### `Utilities/Bitflags` — the first draft was the wrong shape
The held-back draft was a trimmed `bitflags!` macro written from the crate's
documentation. Run against the real crate it was a different type: `Debug` printed
`SectionFlags(5)` instead of `SectionFlags(CONFIG | DATA)`, and `iter()`,
`iter_names()`, `from_name()` and the `Flags` trait were missing. It was thrown away
and replaced with a mechanical port of the real source. Lesson: copy upstream's
real signatures and behavior instead of simplifying.

### `Utilities/AsyncTrait` — the "breaks implementors" claim was wrong, then right
The first recipe used one lifetime (`fn f<'a>(&'a self, url: &'a str)`). The docs
said that would break external `#[async_trait]` implementors, which was true of
*that* signature (E0195, reproduced) and would have been avoided entirely by using the
macro's own three-lifetime expansion. The module now does exactly that, and the
tests use the real macro in both directions instead of asserting it.

### The consolidated delivery was only partly applied
A check of `master` found the schema work, the first-batch wiring and the
`rustc-hash` patch landed, but the consolidated archive's second half (the final
`Bitflags` and `AsyncTrait` modules, the `SectionFlags` and `CloudStorageProvider`
wiring, the visibility changes and the `Cargo.toml` moves) had not: `Utilities/mod.rs`
still said "NOT WIRED IN" for three modules, `Bitflags` and `AsyncTrait` were the
first drafts (13 and 4 tests instead of 55 and 6), and 11 `private_interfaces`
warnings were present because `rustc-hash` imports were patched while
`RustcHash` was still `pub(crate)`. The second half was rebuilt from the real
upstream sources on top of `master`. **Lesson: after a multi-step apply, compare the
tree against the intended end state before trusting it** — a half-applied
delivery compiles.

### `Runtime/dix_value.rs`
No change. A `serde` derive removal was made and then reverted to the original
file (`git checkout`); the file is identical to `HEAD`.
