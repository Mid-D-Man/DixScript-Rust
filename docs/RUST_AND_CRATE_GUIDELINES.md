# Rust and Crate Guidelines

## Purpose

`DOCUMENTATION_AND_COMMENTING_GUIDELINES.md` covers how to write comments and
docs, in any language. This one covers Rust and Cargo specifics: how a crate
in this workspace is supposed to be laid out, when something gets a feature
flag, how unsafe code gets handled, what a dev build looks like versus a
release build, and how versions move.

This is a first pass. Some of it (the workspace section especially) depends
on a decision that has not been made yet, and is written that way on purpose.

## 1. Workspace Structure

mid-engine is a real Cargo workspace: `[workspace]`, `resolver = "2"`, 24
members under `crates/`, `benches/`, and `examples/` (recounted directly
against the member list this pass — an earlier draft of this doc said 20,
which was already stale), already wired together with `path = "../.."`-style
dependencies (`mid-anim` on `mid-math`, `mid-app` on `mid-ecs`, `mid-ecs` on
`mid-collections` and `mid-math`, `mid-physics` on `mid-math` and
`mid-geom`, and more). The root `Cargo.toml` also carries a growing block of
real, dated comments documenting every per-crate MSRV/toolchain wall found
so far (rayon on `mid-ecs`, `web-transport-quinn` on
`mid-net-transport-quinn`, the `edition2024`-via-criterion wall on
`mid-collections`/`mid-arena`, and others) — read those before assuming a
bare `cargo build`/`cargo test` with no `-p` flag will resolve cleanly;
several members deliberately need a newer toolchain than this project's
rustc-1.75 floor, and the comments say exactly which ones and why.

Found and fixed while adding the tables below (unrelated to them, just
noticed in the same file): the `members` list had `"tools/mdix-compiler"`
and `"examples/headless-server"` each listed twice — one occurrence from
before the "New this pass" comment block, one after. Harmless in practice
(Cargo doesn't error on an exact-duplicate member path), but real drift
from copy-pasting the block; deduplicated.

The workspace now has a `[workspace.lints]` table and a
`[workspace.dependencies]` table (previously this section's own "still
open" item — resolved). Every crate used to repeat its own lint
configuration (where it had one at all) and pin its own dependency
versions independently — the exact kind of drift that already happened
once with `mid-math`'s own `glam` dev-dependency going five minor releases
stale before anyone noticed. What's actually in the workspace root today:

```toml
[workspace.lints.rust]
unsafe_code  = "deny"
missing_docs = "warn"

[workspace.lints.clippy]
undocumented_unsafe_blocks = "warn"

[workspace.dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
```

No `[workspace.package]` table (the illustrative example in an earlier
draft of this doc showed one with `license = "MIT OR Apache-2.0"`) — no
LICENSE file exists in this repo and no crate declares a `license` field
today, confirmed directly rather than assumed. That's a real decision
still to make, not a technical default to copy in silently, so it's left
out until it's actually decided.

`criterion` is the only workspace.dependencies entry so far — confirmed by
grepping every crate's Cargo.toml before adding this table: six crates
(`mid-arena`, `mid-collections`, `mid-ecs`, `mid-geom`, `mid-log`,
`mid-math`) already pinned the identical `{ version = "0.5", features =
["html_reports"] }`, all six now reference `criterion.workspace = true`
instead. `glam` stays a `mid-math`-only dev-dependency, not promoted here —
only `mid-math` depends on it, so there's nothing to centralize yet.

A crate opts into the shared lint table with `[lints] workspace = true`
instead of repeating the list -- not automatic just because the workspace
table exists. Only `mid-math` has opted in so far (see section 3) — the
other five crates that share the criterion pin above have NOT been audited
for unsafe-code posture, so opting them into `unsafe_code = "deny"` blind
is a separate, disclosed follow-up, not done in this pass.

## 2. Feature Gating

If a piece of a crate is substantial and not everyone who depends on that
crate needs it, it gets a Cargo feature. This is already happening in two
places in this repo:

- `mid-collections`: `ffi = ["dep:zerocopy"]`, `default = []`. The
  `zerocopy` dependency and whatever it enables in `component.rs` only
  compile in for a consumer that actually asked for the FFI span mechanism.
- `mid-math`: `mint = ["dep:mint"]`, gating the mint interop conversions.

`mid-math`'s own `ffi/` module (C-ABI exports for essentially every type in
the crate — well over 1,300 `#[no_mangle] extern "C"` functions as of this
pass, counted directly rather than estimated) was the next candidate named
in an earlier draft of this doc, and is now applied:

```toml
[features]
default = []
ffi = []
```

`mid-ecs` made the opposite call for its own, much smaller `ffi.rs`: always
compiled, no separate feature, explained directly in its `mid-collections`
dependency comment. That is the right call for something that small.

Applying this to mid-math had one real consequence beyond mid-math itself:
`mid-geom`'s own `src/ffi.rs` (unconditional, not itself feature-gated)
imports `mid_math::ffi::{CVec2, CVec3, CQuat, CMat4}` directly, and its
`Cargo.toml` depended on mid-math with no `features = [...]` at all — so
the moment mid-math's `ffi` module stopped being unconditional, mid-geom's
own default build would have broken. Confirmed by grepping every `.rs` file
and every `Cargo.toml` in the workspace for `mid_math::ffi` and `mid-math =`
before shipping mid-math's change, not discovered after the fact: mid-geom
is the only one of mid-math's four dependents (`mid-physics`, `mid-ecs`,
`mid-anim`, `mid-geom`) that touches the C ABI at all. Fixed by adding
`features = ["ffi"]` to mid-geom's own `mid-math` dependency line —
mid-geom's own ffi surface keeps compiling by default exactly as it did
before, nothing changes for its consumers; only crates that never touched
`mid_math::ffi` in the first place (`mid-physics`, `mid-ecs`, `mid-anim`)
get the actual compile-time/binary-size win.

Also touched in the same pass, for the same reason mid-geom needed
fixing: mid-math's own `src/tests/int64_tests.rs` and `f64_tests.rs` had
ten `#[test]` functions using `crate::ffi::...` directly with no cfg gate
of their own (nine found by pattern-matching indentation, a tenth —
`ffi_dvec3_roundtrip` — found only on a second, broader pass, since its
original indentation didn't match the pattern used to find the other
nine). Each is now `#[cfg(feature = "ffi")]`, individually, rather than
restructured into a submodule — the smallest change that fixes
`cargo test -p mid-math` with default (no-`ffi`) features. Doc-comment
`C interop: see crate::ffi::...` intra-doc links elsewhere in mid-math
(`f32/`, `f64/`, `int32/`, `int64/`) are unaffected by any of this — they
don't compile as code, only as rustdoc links, and only actually break (as
broken-link warnings, not errors) for a LOCAL `cargo doc` run without
`--features ffi`; see mid-math's own `Cargo.toml` for the
`[package.metadata.docs.rs] all-features = true` that fixes this for the
published docs.rs build specifically (section 6's own former "still open"
item on this — also resolved now).

A few rules for feature flags in this workspace:

- One-line comment above every feature explaining what it turns on, not just
  what it's named. `# Enable interoperation with the real mint crate's
  Vector/Point/Quaternion/Matrix types.`, not silence.
- Optional dependencies always use the `dep:` prefix in the feature list
  (`ffi = ["dep:zerocopy"]`), never the older implicit
  same-named-feature-from-an-optional-dependency form. Explicit here costs
  nothing and avoids Cargo's own feature-unification surprises.
- `default = []` unless there's a genuinely good reason a bare
  `dependency = { path = "..." }` with no `features = [...]` should still
  get something extra. Most of this workspace's crates should default to
  the smallest useful surface, not the largest.
- A feature that only forwards to an optional dependency's own feature of
  the same purpose uses the `?` form so requesting it does not silently turn
  the dependency on: `serde = ["dep:serde", "some-dep?/serde"]`.

## 3. Unsafe Code

mid-math in particular carries a lot of unsafe SIMD intrinsic code, and most
of it does not yet carry a `# Safety` doc comment or a `// SAFETY:` comment
at the call site. `docs/roadmap.md`'s own Decision 5 already flagged this as
real, unenforced debt. The workspace lint table (section 1) now exists, so
the rule below is applied, not aspirational:

- `unsafe_code = "deny"` at the workspace level — done (section 1). A crate
  that has a real reason to use unsafe (mid-math's SIMD intrinsics, an FFI
  boundary, anything touching raw pointers for performance) opts back in
  explicitly with `#![allow(unsafe_code)]` at its crate root, so the
  exception is visible in that one line rather than silently inherited.
  mid-math is the only crate that has actually opted into
  `[lints] workspace = true` (and correspondingly needed the
  `#![allow(unsafe_code)]` opt-out) so far — the other nineteen crates
  under `crates/` (twenty total, recounted directly against the member
  list this pass — an earlier draft of this doc assumed fourteen, which
  was already stale) haven't been audited for unsafe-code posture yet, so
  opting them in too is a separate, disclosed follow-up, not assumed to be
  risk-free.
- `undocumented_unsafe_blocks = "warn"` (clippy) — also live now, at `warn`
  rather than `deny` deliberately: turning it on immediately surfaces the
  existing backlog (most of mid-math's own unsafe blocks, including
  everything added across the AVX2 rework and the new FFI files, do not yet
  carry a `// SAFETY:` comment) as visible warnings rather than either
  hiding it or blocking the build on a large retroactive pass that hasn't
  happened. Every `unsafe { ... }` block gets a `// SAFETY:` comment
  immediately above it explaining why the invariant the block depends on
  actually holds; every `unsafe fn` gets a `# Safety` doc section explaining
  what its caller has to guarantee. `camera.rs`'s `mid_frustum_from_planes`
  in mid-math's own `ffi/` already does this correctly. That is the model,
  not a new invention — and still the exception across this crate's unsafe
  surface, not yet the norm.

## 4. Debug and Release Builds

Cargo's own `dev`/`release` profile defaults (unoptimized + debug assertions
on, versus optimized + debug assertions off) are the baseline and do not
need overriding for most of this workspace. Two additions are worth making
once the workspace exists:

```toml
[profile.wasm-release]
inherits = "release"
opt-level = "z"
lto = "fat"
codegen-units = 1
```

Already recommended in `docs/roadmap.md` for mid-math specifically, never
applied. Belongs at the workspace level now so every crate that ships to
wasm gets it, not just mid-math.

`debug_assert!`/`debug_assert_eq!` are the right tool for an invariant that
is expensive enough to skip in release but genuinely useful to catch in
day-to-day dev and testing builds. Prefer them over a plain `assert!` for
anything on a hot path that is not also a safety invariant (a safety
invariant backing an `unsafe` block still gets a real `assert!`, or the
`unsafe` block does not get to make that assumption at all).

## 5. Versioning

Every crate starts and stays at `0.0.1` for as long as it's in active
dev/testing. No incrementing 0.1.0, 0.2.0, and so on along the way. The
first actual, official release jumps straight to `1.0.0`. Nothing in
between.

## 6. Still Open

- Whether the other nineteen crates under `crates/` should also opt into
  `[lints] workspace = true` (section 3) — mid-math has, since it was the
  crate actually being worked on and already had a clear, disclosed reason
  to need `#![allow(unsafe_code)]`. None of the other nineteen have been
  audited for their own unsafe-code posture, so extending the opt-in to them
  is a separate decision, not assumed to be a safe default.
- Whether every crate that could reasonably split an `ffi` feature out
  (mid-collections and mid-math already do; anything else with a sizeable
  `ffi.rs`/`ffi/` is a candidate) should, or whether mid-ecs's
  "small enough to always compile" call is the right default and `ffi` as
  a feature is the exception, not the rule.
- Whether other crates besides mid-math should get their own
  `[package.metadata.docs.rs] all-features = true` — mid-math needed it
  specifically because gating `ffi` created doc-comment links elsewhere in
  the same crate that only resolve with that feature on; a crate without
  that specific cross-feature-doc-link situation may not need it at all.
- `[workspace.package]` (edition/license inheritance) — still not added;
  see section 1 for why (no license decision made yet).
