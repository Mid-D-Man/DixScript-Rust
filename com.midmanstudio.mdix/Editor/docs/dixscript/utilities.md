# dixscript — Utilities

Part of [dixscript](../dixscript.md). Covers `dixscript/src/Utilities/` —
specifically the **dependency-reduction pass** started in this part. The
older files in `Utilities/` (`keyword_definitions.rs`, `mid_logger.rs`,
`token_debug_printer.rs`, ...) are not documented here yet; treat them as
not yet documented rather than undocumented on purpose.

## Overview

Goal: shrink the core crate's `[dependencies]` by removing anything dead and
hand-rolling everything small enough to own. The plan was to hand-roll them all and
stick to it; the work was done in three stages -- build each replacement in
isolation and test it against the real crate, wire the internal-only ones in, then
wire in the three that change public API. All eight are now wired in. Dev-dependencies
(`criterion`, `tempfile`, `bincode`, `rmp-serde`, `postcard`, `ciborium`, `bumpalo`)
were out of scope.

Every decision below came from reading real call sites and, where a replacement was
built, from testing it against the exact locked version of the crate it replaces.
For the three largest ports (`bitflags`, `rustc-hash`, `async-trait`) the real
upstream source was read and followed, not approximated. Counts are from a scripted
scan of `dixscript/src`.

| Crate | Real usage | Public API? | Status |
|---|---|---|---|
| `itertools` | 0 call sites anywhere | — | **Removed** |
| `unicase` | 0 call sites anywhere | — | **Removed** |
| `serde` | `#[derive(Serialize)]` on `DixValue`; 5 sibling binding crates serialize `&DixValue` through it | yes | **Kept** — first judged removable, wrongly |
| `hex` | 2 call sites (`hex::encode(sha256)`) | no | **Wired in** |
| `lazy_static` | 18 `static ref` items in 6 blocks / 6 files | no | **Wired in** |
| `base64` | 35 calls, 11 files, `general_purpose::STANDARD` only | no | **Wired in** |
| `uuid` | 13 call sites, 4 files (`new_v4`, `parse_str`, `nil`, `from_bytes`) | no | **Wired in** |
| `hostname` | 1 call site, diagnostics only | no | **Wired in** (best-effort, see its section) |
| `bitflags` | 1 invocation (`SectionFlags`) | **yes** | **Wired in** — full port of 2.10.0 |
| `rustc-hash` | 188 mentions of `FxHashMap`/`FxHashSet`, 13 files | **yes (11 items)** | **Wired in** — port of 2.1.1; the 11 items' types changed (approved) |
| `async-trait` | 2 attribute sites on the `CloudStorageProvider` trait | **yes** | **Wired in** — hand-expanded to the macro's exact signature; implementors unaffected (verified) |
| `url` | 1 call site | no | **Kept for now** — see Open items |

"Public API?" means the crate's own types appear in a signature or field of something
reachable from outside the crate. For the three rows marked **yes** the replacement
module is itself `pub` (`dixscript::Utilities::{Bitflags, RustcHash, AsyncTrait}`) so
the types stay nameable by callers, exactly as the real crates' types were.

Kept on purpose, with the reason, so nobody re-litigates it:

- `regex` — 22 files, includes a user-facing `Regex` value type. Rust's standard library has no regex engine, so there is no native alternative; hand-rolling one is a project in itself. Staying.
- `serde` — `DixValue`'s derived `Serialize` is the JSON escape hatch for
  `mdix-ffi` (`mdix_get_json`), `mdix-wasm` (`getJson`), `mdix-lua`
  (`get_json`), `mdix-python` and `mdix-java`.
- `serde_json`, `toml` — real *parsing* for JSON/TOML import, not just writing. Not replaced; the plan is to **feature-gate** JSON and TOML support in the feature-gating pass instead, so a build that doesn't need the converters doesn't carry them.
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
- **Visibility:** five modules (`Base64`, `Hex`, `Hostname`, `LazyStatic`, `Uuid`) are
  `pub(crate)`: they replace dependencies and are not part of the public API. Three
  (`Bitflags`, `RustcHash`, `AsyncTrait`) are `pub`, because this crate's public API
  names their types. None is re-exported through `Utilities/mod.rs`'s `pub use` list.
- **Drop-in surface:** each module reproduces the *call-site syntax* of the crate
  it replaces where it can (same macro grammar, type names, function names,
  `Engine` trait and `general_purpose::STANDARD`), so wiring in was one `use`
  line (or one inline path) per site. `AsyncTrait` is the exception: it is a
  recipe applied by hand at three sites, and has not been applied.
- **Differential tests, and the real crates as oracles.** Each module that
  replaces a crate with checkable behavior has tests that run the same inputs
  through both and require identical results. They call the real crate as
  `::name` (leading `::` forces the extern crate, since a same-named local
  module exists). When a crate is dropped from `[dependencies]`, **move it to
  `[dev-dependencies]` instead of deleting it** — the tests then keep guarding
  the replacement for good. That is what was done for `base64`, `hex`,
  `hostname` and `uuid`, all under
  `[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]` (`hostname` was
  already a non-wasm dependency). `lazy_static` has no oracle test and was
  removed outright.
- **`allow(...)` on the module lines in `Utilities/mod.rs`.** The five `pub(crate)`
  modules keep `allow(dead_code, unused_imports)` because each deliberately offers a
  little more than the crate calls (`Hex::decode`, extra re-exports). The three `pub`
  modules need none. Elsewhere lints are narrowed to the specific item (the classic
  hasher, the never-read `ParseError` payloads, every `lazy_static` static).

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

### `Utilities/Bitflags/`

A port of `bitflags` **2.10.0** (the locked version), `MIT OR Apache-2.0`, written from
upstream's `traits.rs`, `iter.rs`, `parser.rs`, `public.rs`, `internal.rs` and the
`bitflags!` macro. Files: `traits.rs` (`Flag`, `Flags`, `Bits`, `Primitive`,
`PublicFlags`), `iter.rs` (`Iter`, `IterNames`, `IterDefinedNames`), `parser.rs`
(`to_writer`, `from_str` and the strict/truncate variants, `ParseError`),
`bitflags_macro.rs`, and `tests.rs`. Each file carries an attribution header.

**A first draft was wrong, and was replaced.** The first version was a trimmed macro
covering only what `SectionFlags` called inside the crate, then extended with the set
algebra. It still printed `SectionFlags(5)` for a derived `Debug`, where the real crate
prints `SectionFlags(CONFIG | DATA)`, and it lacked `iter()`, `from_name()`, the
`Flags` trait and the formatting traits. Reading the real source showed why:
upstream's public type is a *wrapper around a hidden `InternalBitFlags`*, and a derived
`Debug` on the wrapper delegates to the hidden type's hand-written `Debug`. That
two-layer shape is now reproduced, and with it the exact text form.

What is generated, as upstream: the public wrapper; the flag constants; `impl Flags`;
the full inherent API with upstream's `const fn` set (`empty/all/bits/from_bits/
from_bits_truncate/from_bits_retain/from_name/is_empty/is_all/intersects/contains/
insert/remove/toggle/set/intersection/union/difference/symmetric_difference/
complement/iter/iter_names`); `Binary`/`Octal`/`LowerHex`/`UpperHex`; `| & ^ - !` with
their `-Assign` forms; `Extend`, `FromIterator`, `IntoIterator`. The `Flags` trait
adds `truncate`, `clear`, `contains_unknown_bits` and `iter_defined_names`, which (as
upstream) exist only there.

Deliberate choices:
- `ParseError` is the `std`-less shape: payload `()`, no trailing `` `name` `` in its
  text, no `std::error::Error` impl. The manifest used `bitflags = "2.4"` with no
  features, so that is what the real crate produced for this project.
- The macro is one macro plus one helper instead of upstream's chain of `__declare_*`
  / `__impl_*` helpers. That changes how it is written, not what it generates.
- **Not ported:** unnamed flags (`const _ = ...`), `#[cfg]` on flag constants, the
  `serde`/`arbitrary`/`bytemuck`/`zerocopy`/`bitflags_match!` integrations, the
  deprecated `BitFlags` alias trait, and operators/formatting/iteration on the hidden
  type (nothing can name it; the wrapper's versions forward to the bits). The one
  invocation in this crate uses none of these.

**Public:** `dixscript::Utilities::Bitflags` is a `pub` module. `SectionFlags` is a
public type whose methods return `Iter`/`IterNames` and implement `Flags`, so those
names must be reachable, exactly as with the real crate.

**Tests (`tests.rs`, 31 in the module):** the same flag list is defined with both this
port and the real crate (called as `::bitflags`) and identical operations are run
through both. Five flag sets: the exact `SectionFlags` shape (a gap at `0x20`, reserved
high bits); overlapping and multi-bit flags on `u16` (including a flag defined as
`Self::A.bits() | Self::B.bits()`); a signed `i8` including `i8::MIN`; a `u32`; a
`u128`. Compared: `Debug` (plain and pretty), `{:x} {:X} {:o} {:b}` (plain and alternate),
`bits`, `is_empty`, `is_all`, `iter`, `iter_names`, `from_bits*`, `from_name` (including
junk and wrong-case names), the `FLAGS` table, `iter_defined_names`, `Flags::truncate`/
`clear`/`contains_unknown_bits`, the text round trip and a corpus of malformed text
through `from_str`, `from_str_strict` and `from_str_truncate`, every method and operator
over all 65,536 byte pairs (and 50–100k sampled pairs for the wider types),
`Extend`/`FromIterator`/`IntoIterator`, derived `Default`/`Ord`/`Eq`, and `const`
evaluation. Three deliberate bugs were injected (the iterator's overlap condition, the
`" | "` separator, the empty-value `0x0` form) and each was caught.

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
`RustcHash::classic`. To make the classic one the default, change one
`pub(crate) use` line in `RustcHash/mod.rs`.

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

**Wired in, and public.** The default exports are now what all 13 importing files use
(delivered as a patch; see "How to apply"). Because 11 public items take or return
`FxHashMap<String, _>` -- `ExecutionContext::variables`, `FunctionInterpreter::
captured_env`, `scope_context`, `ExecutionContext::new`, `get_all_variables`,
`get_scope_variables_snapshot`, `FunctionInterpreter::new`, `new_with_error_manager`,
`execute`, `evaluate_arguments_in_caller_context` and `DataSectionAnalyzer::get_indexes`
-- `dixscript::Utilities::RustcHash` is a `pub` module and the types are `pub`. The
change to those signatures was approved: a caller that built a `rustc_hash::FxHashMap`
to pass in must now build `dixscript::Utilities::RustcHash::FxHashMap` (the same
`HashMap<_, _, _>` shape with a different hasher type). The classic `mid-engine` copy
stays `pub(crate)` as `RustcHash::classic`.

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

A `BoxFuture<'a, T>` alias and a **recipe that reproduces `#[async_trait]`'s expansion
exactly** -- not a macro. `#[async_trait]` is a procedural macro; writing one means a
`proc-macro = true` crate and `syn` + `quote`, the dependency this pass exists to
remove, and it is needed in only two places (the `CloudStorageProvider` declaration and
its one implementation, `HttpCloudProvider`).

- Native `async fn` in traits isn't dyn-compatible, and the trait is used as
  `Arc<dyn CloudStorageProvider + Send + Sync>`. A method returning a boxed future is.
  That is the whole job the macro does.
- **The signature is copied from the macro, not simplified.** A trait method becomes
  `fn f<'life0, 'life1, 'async_trait>(&'life0 self, url: &'life1 str) ->
  BoxFuture<'async_trait, R> where 'life0: 'async_trait, 'life1: 'async_trait,
  Self: 'async_trait;`, and the implementation wraps its body in `Box::pin(async move
  { ... })`. Method bodies are untouched (no re-indenting, in case one contained a
  multi-line literal); `return Err(..)` and `?` keep working inside the block.
- **A first draft used a simpler single-lifetime signature and was wrong for outside
  implementors.** That version asserted, without testing, that outside implementors
  would break and that this was a public-API cost to weigh. Testing showed the problem
  was my signature, not the idea: with the single-lifetime declaration, an
  `#[async_trait] impl` is rejected with `E0195` ("lifetime parameters or bounds on
  method `fetch` do not match the trait declaration"); with the macro's exact signature
  it compiles. So **implementors are unaffected**, and the "this one changes public API"
  warning that earlier versions of this document carried no longer applies.

**Public:** `dixscript::Utilities::AsyncTrait` is a `pub` module, because
`CloudStorageProvider` is a public trait whose signatures name `BoxFuture`.

**Tests use the real `async-trait` as an oracle, in both directions:** (1) an
implementor written the way a downstream crate would have -- `#[async_trait::async_trait]
impl Trait for X` -- compiles against a trait declared with the hand-written signature
and works behind `Arc<dyn Trait + Send + Sync>`; (2) a hand-written implementation
satisfies a trait declared by the real macro. Plus a real `Pending` suspension, `return`
and `?` inside the block, borrowing a non-`'static` local, and the future being `Send`.
The old single-lifetime shape was re-introduced temporarily and confirmed to fail with
`E0195`, so the test cannot pass by accident.

**Honest scope of the saving:** the `async-trait` crate leaves `[dependencies]` (it
stays as a dev-dependency, as the oracle), but the build barely shrinks: its
`proc-macro2`, `quote` and `syn` dependencies are also pulled in by `serde_derive`.

### `Utilities/test_rng.rs`

Test-only (`#[cfg(test)]`). A deterministic xorshift64 so the differential tests
replay identically on every machine. Not used by anything at runtime;
`Uuid::new_v4` uses the OS entropy source.

## Verification summary

How it was verified: the sandbox's default toolchain is 1.75, but Ubuntu's archive also
ships `rustc-1.85`/`cargo-1.85` (`apt-get install rustc-1.85 cargo-1.85`, then
`PATH=/usr/lib/rust-1.85/bin:$PATH`), which is exactly the crate's declared
`rust-version`. That made it possible to compile and test the real crate rather than
extracted copies. Earlier statements in this project that the crate could not be
compiled here, and that `LazyLock` could not be tested, were true of 1.75 and are no
longer true.

| Item | Status |
|---|---|
| The delivery mechanism itself | Rehearsed end to end with the real scaffold engine on a pristine `git HEAD`: the archive applied through the replacements engine (`file_strategy=overwrite`: **28 created, 50 replaced, 0 skipped, 0 errors**), all 78 files byte-identical to the working tree; the patch passed `--validate-only`, a dry run left every file unchanged, and the real run rewrote all 13 files. |
| The resulting tree builds and passes | `cargo test -p dixscript` on that tree (Rust 1.85.1): **672 passed, 0 failed** — 510 library tests, every integration suite (including `schema_section_tests` 42 and `raw_section_tests` 16) and 2 doc tests. |
| Warnings | 129 before this work, 129 after, the identical set. Wiring initially added nine (unused-by-design code and a never-read error payload); each was fixed by narrowing a lint to the specific item rather than the module. No `private_interfaces` warnings from the newly public types. |
| The eight modules' own tests | 87: AsyncTrait 6, Base64 10, Bitflags 31, Hex 8, Hostname 4, LazyStatic 4, RustcHash 14, Uuid 10. |
| Differential tests | Each replacement is compared with the exact locked crate it replaces (`hex`, `bitflags`, `base64`, `uuid`, `hostname` on Linux, `rustc-hash`, `async-trait`). Roughly 1.3M base64 decodes, 300k uuid parses, 65,536 byte pairs × every bitflags operation, five bitflags flag sets, 300+300 map/set iteration-order comparisons. They run inside the real crate with the oracles as dev-dependencies. |
| Do the tests have teeth? | Deliberate bugs were injected and caught: 3 in base64, 2 in uuid, 2 in the `rustc-hash` port, 3 in the bitflags port, and the old async-trait signature was re-introduced and confirmed to fail with `E0195`. |
| Other targets | Earlier in this work (before the final three wiring changes): `--all-targets` (tests, benches, examples), `--no-default-features`, and the two sibling crates that use the local path (`mdix-lua`, `mdix-java`) all built. **Not re-run after wiring `bitflags`/`rustc-hash`/`async-trait`.** A grep confirms no sibling crate references any item whose public type changed (the only hit is `tower_lsp`'s own re-export of an async-trait macro), so a regression there is not expected, but CI is the confirmation. |
| Cargo.lock | Against `HEAD`, regenerating drops direct edges from the local `dixscript` entry and adds `bumpalo`, which was already missing from `HEAD`'s lock. A `--locked` build refuses — and **already refused on pristine `HEAD`, before any of this work** (checked) — so a Sync Cargo.lock run is needed regardless. Normal (unlocked) builds are unaffected. |
| Inactive `cfg` branches | The 32-bit constants in `rustc_hash_v2.rs` and the Windows, unix and wasm variants of `hostname::get()` were forced on, one at a time, in an isolated copy and type-checked with the real compiler: all compile. That proves they compile, not that they behave correctly on a real 32-bit or wasm target. |
| `wasm32` | **Still not built or run.** No `wasm32` standard library is packaged for this toolchain. Your CI's `cargo build -p dixscript --target wasm32-unknown-unknown` is the real check. It exercises what nothing here did: `Uuid::new_v4` through `getrandom` 0.2's `js` feature (already enabled for wasm and already relied on by the DLM encryptors), and the 32-bit hasher at runtime. |

## Decisions made, and how to apply

**Decisions.** The plan was to hand-roll every crate on the list, and that was the
instruction. An interim recommendation to keep `rustc-hash` and skip `async-trait` on
public-API grounds was set aside: all 11 public items that expose `FxHashMap` may
change. Where changing a public signature could be avoided, it was: `async-trait`'s
replacement keeps implementors compiling (verified with the real macro).

What a downstream user of the published crate sees:
- `rustc-hash` — the 11 items' `FxHashMap<String, _>` is now
  `dixscript::Utilities::RustcHash::FxHashMap<String, _>`: the same `HashMap<_, _, _>`
  shape with a different hasher type, so code that built a `rustc_hash::FxHashMap` to
  pass in must switch types. **This is a breaking change** to those signatures.
- `bitflags` — `SectionFlags` is generated by the port. The method set, operators,
  `const fn`-ness, `Debug` text, iteration and parsing match the real crate (tested);
  not ported are unnamed flags, `#[cfg]` on flags and the serde/arbitrary/bytemuck/
  zerocopy integrations (unused here).
- `async-trait` — nothing changes for implementors of `CloudStorageProvider`; callers
  still get a boxed future from each method.

**How to apply** (the order matters):
1. Put `dixscript-schema-and-dependency-reduction.tar.gz` in `.mdix/replacements/` and
   run the apply-replacements workflow (`file_strategy=overwrite`). It supersedes the
   earlier `schema-section.tar.gz` and `dependency-reduction-batch1.tar.gz`: **delete
   those two first if they are still in the folder.** The resolver extracts archives in
   unsorted order, so a leftover older archive can overwrite a newer file.
2. Put `patch.mdix` at `.mdix/patches/patch.mdix` (replacing the previous one; there is
   only ever one) and run the apply-patch workflow with `dry_run=false`. It rewrites the
   13 `use rustc_hash::...` lines to the local port.
3. **Delete the archive afterwards** (or set `delete_processed_archives`). Several of
   the 13 patched files are also inside it, so leaving it in place means a later
   replacements run re-extracts the unpatched copies over them.
4. Run the Sync Cargo.lock workflow.

The patch applies after the archive because the archive adds the `RustcHash` module and
makes it public; the patch's 13 anchors are whole `use` lines, each occurring exactly
once in its file.

## Open items and deferred plan

**The feature-gating pass** (planned; not started). Candidates already found:
- **All the DLM modules**, properly, plus "a few more things" as the reduction continues.
- **JSON and TOML** support (`serde_json`, `toml`): gate the converters rather than
  replace the crates. Nothing here is gated yet.
- `flate2` is **not** feature-gated, unlike its siblings `bzip2` and `lzma-rust2`.
  Confirm whether gzip is meant to be the always-on codec.
- `url` is documented in `Cargo.toml` as "optional — activated via the `cloud-import`
  feature", but has **no `optional = true` and is not under `[features]`**: it always
  compiles in, `--no-default-features` included. (`async-trait` shared that
  misdescription until it was removed.) `url` cannot yet be swapped for `reqwest::Url`
  (a re-export of the same type): `cloud_file_cache.rs` compiles unconditionally but
  `reqwest` is genuinely optional, so the swap would break a `--no-default-features`
  build. Do the gating and the swap together.
- `uuid`'s old `serde`/`js` features left with the runtime dependency. `chrono` still
  declares a `serde` feature, **not investigated**: because `DixValue` still derives
  `Serialize`, it is not known whether that flag is load-bearing.

**`regex` stays.** Rust's standard library has no regex engine; it backs a user-facing
`Regex` value type in 22 files.

**Regenerate `Cargo.lock`.** See the verification table. Not shipped in the replacements
archive: a root-level `Cargo.lock` has no directory in its path, so the resolver would
match it by basename and could confuse it with another crate's lockfile.

**Cleanup candidate:** `INTERP_METHOD_CALL_RE` and `INTERP_PROPERTY_RE` in
`quickfuncs_section_parser.rs` are unused. `lazy_static` had been hiding them; the
`LazyStatic` macro now suppresses `dead_code` on every static so the swap added no
warnings, but the two statics look like leftovers of interpolation code that also
produces `parse_interpolated_expression is never used`.

**`wasm32` and a real 32-bit target** are unverified; see the verification table.

**Not re-checked after the final wiring:** `mdix-lua` / `mdix-java` builds, and the
benches/`--no-default-features` builds (see the verification table).

**Docs hygiene, unrelated to dependencies:** `docs/RUST_AND_CRATE_GUIDELINES.md`
describes the `mid-engine` workspace (`mid-math`, `mid-ecs`, ...), not this one. It looks
like a template that was never adapted for this repo. Not touched.

## Fixes and Problems

### Wiring in the three public-API modules (`bitflags`, `async-trait`, `rustc-hash`)
- `SectionFlags` now comes from the local `bitflags!`; `CloudStorageProvider` and
  `HttpCloudProvider` are hand-expanded (2 files); the 13 `rustc_hash` imports are a
  patch. `Bitflags`, `AsyncTrait` and `RustcHash` became `pub` modules; `bitflags`,
  `async-trait` and `rustc-hash` moved to dev-dependency oracles.
- **An earlier claim was wrong:** it said the `rustc_hash` references included "8
  fully-qualified" inline paths. A scan showed all 13 are plain `use` lines (four shapes),
  so each file needs exactly one edit.
- **The "held back" posture was set aside** when the instruction was to stick to the plan;
  the public-API cost it was guarding against was either accepted (`rustc-hash`) or
  turned out to be avoidable (`async-trait`, once the signature was copied from the
  macro rather than simplified).
- Nine warnings appeared during wiring (the never-read `got: ()` payload in the std-less
  `ParseError`, and the unused-by-design `classic` hasher once its parent module became
  public and lost its module-level `allow`). Fixed at the narrowest scope.

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

### `Runtime/dix_value.rs`
No change. A `serde` derive removal was made and then reverted to the original
file (`git checkout`); the file is identical to `HEAD`.
