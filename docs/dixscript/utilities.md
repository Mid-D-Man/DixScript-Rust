# dixscript — Utilities

Part of [dixscript](../dixscript.md). Covers `dixscript/src/Utilities/` —
specifically the **dependency-reduction pass** started in this part. The
older files in `Utilities/` (`keyword_definitions.rs`, `mid_logger.rs`,
`token_debug_printer.rs`, ...) are not documented here yet; treat them as
not yet documented rather than undocumented on purpose.

## Overview

Goal: shrink the core crate's `[dependencies]` by removing anything that is
dead, and hand-rolling anything small enough to own. Dev-dependencies
(`criterion`, `tempfile`, `bincode`, `rmp-serde`, `postcard`, `ciborium`,
`bumpalo`) are out of scope for this pass.

Every decision below came from grepping real call sites, not from a crate's
reputation for being "small" or "big". The audit found:

| Crate | Real usage | Verdict |
|---|---|---|
| `itertools` | 0 call sites anywhere | **Removed** |
| `unicase` | 0 call sites anywhere | **Removed** |
| `serde` | `#[derive(Serialize)]` on `DixValue`; 5 sibling binding crates serialize `&DixValue` through it | **Kept** — first judged removable, wrongly (see Fixes and Problems) |
| `lazy_static` | 6 files, 18 `static ref` blocks, macro only | Hand-rolled (built, not wired in) |
| `rustc-hash` | 178 sites (`FxHashMap`/`FxHashSet`), 13 files | Hand-rolled (built, not wired in) |
| `bitflags` | 1 site: 8-bit `SectionFlags` | Hand-rolled (built, not wired in) |
| `hex` | 2 sites, both `hex::encode(sha256_digest)` | Hand-rolled (built, not wired in) |
| `base64` | 17 sites, `general_purpose::STANDARD` only | Planned, not built |
| `async-trait` | 2 methods on a `dyn` trait | Planned, not built |
| `uuid` | 5 sites | Planned, lower priority, not built |
| `hostname` | 1 diagnostic-only site | Planned: drop rather than hand-roll |
| `url` | 1 site | **Kept for now** — see Open items |

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
- `rand` — the crypto path (key/salt/nonce via `OsRng`) requires it.
- `flate2`, `bzip2`, `lzma-rust2` — real codecs.
- `chrono` — backs real `Date`/`Timestamp` literal parsing with calendar
  validation. Hand-rollable in principle, easy to get subtly wrong.
- `web-time`, `web-sys`, `rayon`, `reqwest`, `tokio`, `getrandom` — already
  minimal, gated, or required.

## Conventions

- **Casing:** directories under `Utilities/` are PascalCase and named after the
  crate they replace (`Hex`, `Bitflags`, `LazyStatic`, `RustcHash`); files
  inside are snake_case. This matches the rest of the crate (`Compiler/DLM/Auditor/`,
  `Compiler/Core/SectionParsers/`, `Builtins/Static/`, ...).
- **Visibility:** everything here is `pub(crate)`. These replace dependencies;
  they are not part of the crate's public API, and they are deliberately *not*
  re-exported through `Utilities/mod.rs`'s `pub use` list.
- **Drop-in surface:** each module reproduces the *call-site syntax* of the
  crate it replaces (same macro grammar, same type names, same function
  names), so the later wiring-in pass is a one-line `use` change per file,
  not a rewrite of the call sites.
- **`allow(dead_code, unused_imports, unused_macros)`** sits on the four
  `mod` lines in `Utilities/mod.rs`. It is temporary: nothing calls these yet.
  Remove it when they are wired in.

## Modules

### `Utilities/Hex/hex.rs`

**What it does:** `encode(bytes) -> String` (lowercase) and
`decode(&str) -> Result<Vec<u8>, String>` (accepts either case).

**Decisions:**
- Only `encode` is called anywhere (`enhanced_auditor.rs`, `diy_auditor.rs`,
  both hashing a SHA-256 digest). `decode` is included for symmetry and is
  **not exercised by any real caller yet** — don't treat it as battle-tested.
- `decode` returns `Result<_, String>`, not the real crate's `FromHexError`.
  Nothing consumes the error type today, and a `String` avoids inventing an
  error enum for zero callers.

**Tests:** inline `#[cfg(test)]` — known vectors, round trip over all 256 byte
values, odd-length and non-hex rejection, mixed-case decode.

### `Utilities/Bitflags/bitflags_macro.rs`

**What it does:** a `bitflags!` macro accepting the same invocation shape as
the real crate (`#[derive(..)] pub struct Name: u8 { const A = 0x01; ... }`).

**Decisions:**
- Scoped to the exact API `SectionFlags` uses, checked against every call site
  in `BinarySerialization/`: associated consts, `.bits()`, `.contains()`,
  `.insert()`, `from_bits_truncate()`, `|`. `.remove()`, `.is_empty()` and `|=`
  are included although nothing calls them today.
- The tuple field is left private, matching the real crate's encapsulation:
  outside the defining module, a value can only be built through the consts,
  `from_bits_truncate`, or `insert`.
- `from_bits_truncate` masks against the OR of every declared flag, computed at
  expansion time — same contract as the real one (never fails, drops unknown bits).
- Deliberately absent: custom `Debug`, iteration over set bits, serde, and
  overlap checking. None is used by the one real caller.

**Tests:** inline — bits round trip, unknown-bit truncation, `contains`,
`insert`, `remove`, `is_empty`, `|`.

### `Utilities/LazyStatic/lazy_static_macro.rs`

**What it does:** a `lazy_static!` macro that expands to
`static NAME: std::sync::LazyLock<T> = LazyLock::new(|| expr);`.

**Decisions:**
- This is barely hand-rolling: `LazyLock` (stable since Rust 1.80; the crate's
  `rust-version` is 1.85) already does what `lazy_static` exists for. The macro
  only preserves the existing `static ref NAME: Type = expr;` syntax so the
  18 blocks don't change.
- Only the macro is used across the crate — no `lazy_static::initialize()`,
  no `pub static ref` — but visibility and the no-trailing-semicolon final item
  are supported anyway for parity with the real grammar.
- One real difference: the real macro generates a distinct hidden type per
  static; `LazyLock<T>` is one std type. Every use is `NAME.method()` or
  `&*NAME`, both fine either way. Anything that named the generated type
  would break; nothing does.

**Tests:** inline — plain value, multi-statement block initializer, doc
comments passing through, initializer running exactly once across repeated
reads, final item without a semicolon.

**Verification gap (important):** the sandbox toolchain is rustc/cargo 1.75,
where `LazyLock` is still unstable (E0658, `lazy_cell`; confirmed directly). This
file **cannot be compiled as written in that environment.** What was done
instead: the macro was compiled and its four tests run against a stand-in
`LazyLock` built on `std::sync::OnceLock` (stable since 1.70) in a scratch crate.
That proves the macro grammar, recursion, attribute pass-through, and
run-exactly-once behavior. It does **not** prove the real
`LazyLock::new(|| ..)` expression type-checks in `static` position (closure to
`fn() -> T` coercion). That is the documented use of `LazyLock`, so it is
expected to hold, but the confirmation is CI on the real toolchain.

### `Utilities/RustcHash/fx_hash.rs`

**What it does:** `FxHasher`, `FxBuildHasher`, `FxHashMap`, `FxHashSet` — the
same public surface as `rustc-hash`.

**Provenance:** copied from `Mid-D-Man/mid-engine`,
`crates/mid-collections/src/fx_hash.rs` (that file traces itself to rustc's
internal hasher, originally from Firefox). Algorithm, seed constant and
methods are unchanged. Two changes: the top doc comment was rewritten (the
original explains itself in terms of `SpatialHash` cell keys, which don't exist
here), and the `FxHashMap`/`FxHashSet` type aliases were **added** — the
mid-collections copy only exposes the hasher and `FxBuildHasher`, but this
crate's 178 call sites all use the aliases.

**Decisions:**
- Not DoS-hardened, deliberately: every key is something the compiler computed
  while parsing a file it already controls (symbol names, section ids, table
  paths) — no adversarial-input model to defend against.
- The only constructors the crate calls are `FxHashMap::default()` and
  `with_capacity_and_hasher(cap, hasher)` (and the `FxHashSet` equivalents).
  Both are inherent `HashMap`/`HashSet` methods that work automatically once
  the alias and `BuildHasherDefault<FxHasher>: Default` line up, so no
  wrapper functions were needed.

**Tests:** inline — default construction and storage for map and set,
`with_capacity_and_hasher` for both, hashing determinism, and that different
keys hash differently (i.e. `add()` is actually mixing bits).

**Not measured:** there is no benchmark yet comparing this against the real
`rustc-hash`. It is the same algorithm, so parity is expected, but that is an
expectation, not a result. Worth a bench before the 178-site sweep lands.

### Verification summary (what was and wasn't actually run)

| Item | Status |
|---|---|
| `Hex`, `Bitflags`, `RustcHash` | Compiled and unit-tested (23 unit tests across all four modules, of which the 4 `LazyStatic` ones ran against the stand-in described above; plus 4 scratch-only re-export identity checks that are not in the repo) in a scratch crate on rustc/cargo 1.75, **with the real `hex`, `bitflags`, `lazy_static` and `rustc-hash` crates present** in the dependency list, to catch name ambiguity between a local module/macro and a same-named extern crate. None occurred. |
| Re-export identity | Compile-time checks confirm `Utilities::Hex::decode`, `::Bitflags::bitflags`, `::LazyStatic::lazy_static` and `::RustcHash::FxHashMap` resolve to the local code, not the extern crate (e.g. `decode` is typed `Result<_, String>`, which the real crate's is not). |
| `LazyStatic` | Verified only against an `OnceLock`-based stand-in — see above. |
| The `dixscript` crate as a whole | **Not compiled.** The locked dependency graph pulls in `base64ct` 1.8.3, which needs `edition2024`; cargo 1.75 cannot even parse it. The one edited existing source file, `Utilities/mod.rs`, was parse-checked, plus a repo-wide grep confirming nothing references the two removed crates. CI is the real check. |

## Open items and deferred plan

**Explicitly deferred (per the plan for this pass):**
- **The wiring-in sweep.** Nothing calls the new modules yet. Later: repoint
  `lazy_static` (6 files), `rustc_hash` (13 files, 178 sites), `bitflags`
  (`binary_format.rs`) and `hex` (2 auditors) at `crate::Utilities::*`, then
  remove the four crates from `Cargo.toml` and drop the `allow(...)` line.
  Bench `RustcHash` before that sweep (see above).
- **Batch 2 hand-rolls, not started:** `base64` (17 sites, all
  `general_purpose::STANDARD` — one alphabet, encode + decode); `async-trait`
  (2 methods, hand-written `Pin<Box<dyn Future>>` desugaring, needed because
  the trait is used as `dyn`); `uuid` (v4 only); `hostname` (drop, report
  `"unknown"` or an env-var guess — diagnostic output only).
- **Feature-gate the DLM modules properly, plus "a few more things"** as the
  reduction continues. Known candidates already found:
  - `flate2` is **not** feature-gated, unlike its siblings `bzip2` and
    `lzma-rust2`. Confirm whether gzip is meant to be the always-on codec.
  - `async-trait` and `url` are documented in `Cargo.toml` as
    "optional — activated via the `cloud-import` feature", but **neither has
    `optional = true` nor appears under `[features]`**. They always compile in,
    in every configuration including `--no-default-features`. The old comment
    was wrong and has been replaced with an accurate one.
  - Consequence: `url` cannot yet be swapped for `reqwest::Url`
    (a re-export of the same type). `cloud_file_cache.rs` compiles
    unconditionally, but `reqwest` is genuinely optional — so the swap would
    break a `--no-default-features` build. Do the gating and the swap together.
  - `uuid` and `chrono` are declared with a `"serde"` feature. **Not
    investigated** — because `DixValue` still derives `Serialize`, it is not
    known whether those flags are load-bearing. Earlier reasoning that they
    looked droppable assumed the derive was dead, which it is not.
- **Regenerate `Cargo.lock`.** Removing `itertools` and `unicase` from
  `dixscript/Cargo.toml` leaves stale entries in `Cargo.lock` (both are
  depended on by `dixscript` only, apart from `criterion`'s own separate
  `itertools` 0.10). A plain `cargo build` prunes them by itself; a
  `--locked` build will refuse. Run the manual `a-sync-cargo-lock.yml`
  workflow after applying this change.

**Docs hygiene, unrelated to dependencies:** `docs/RUST_AND_CRATE_GUIDELINES.md`
describes the `mid-engine` workspace (`mid-math`, `mid-ecs`, ...), not this
one. It looks like a template that was never adapted for this repo. Not touched.

## Fixes and Problems

### `Cargo.toml`
- Removed `itertools` and `unicase`: zero call sites anywhere in the repo
  (`src/`, `tests/`, `benches/`, `examples/`, and every other workspace crate).
- **`serde` — removal attempted, then reversed. Two wrong conclusions in a row,
  both worth keeping on record.**
  1. The first scoping pass called `serde` removable because in
     `dixscript/src/` its only use was `#[derive(Serialize)]` on `DixValue`, and
     none of the crate's *own* `serde_json::to_string` calls serialized a
     `DixValue`. That was true and irrelevant: the consumers are in sibling
     workspace crates.
  2. A second check found `benches/binary_serialization_benchmark.rs` uses
     `serde` directly, and `serde` was moved to `[dev-dependencies]`. That fixed
     the bench but was still wrong for the same reason as (1).
  The actual evidence: `mdix-ffi` (`mdix_get_json`), `mdix-wasm` (`getJson`),
  `mdix-lua` (`get_json`), `mdix-python` and `mdix-java` all call
  `serde_json::to_string(&DixValue)`, where the argument is the `&DixValue`
  returned by `DixData::get_value`. A grep for a `DixValue`-typed argument
  found nothing (the variable is just called `value`/`v`); a grep for the
  *call shape* found all five. The derive stays, `serde` stays a runtime
  dependency, and `Cargo.toml` now carries a comment saying why.
  **Lesson: "unused" has to be checked across the whole workspace — every
  sibling crate — and by call shape, not by the argument's type name.**
- Attempted to remove `url` in favor of `reqwest::Url`; reverted after finding
  the gating mismatch above. Only the misleading comment was changed.

### `Runtime/dix_value.rs`
- No change. A `serde` derive removal was made and then reverted to the
  original file (`git checkout`); the file is identical to `HEAD`.
