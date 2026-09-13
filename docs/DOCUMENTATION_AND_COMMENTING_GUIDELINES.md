# Documentation and Commenting Guidelines

## Purpose

Code comments explain what the code does right now. The documentation file explains why it looks the way it does, what changed, and what broke along the way. Keeping those two things apart keeps source files readable and keeps project history somewhere it can actually be found later.

These rules are written for your own projects. If a repo ever picks up outside contributors, this spec still holds, just add an onboarding note at the top of that crate's doc file.

## 1. Crate and File Structure

- Favor modular sub-files over one large file. If a file is doing more than one clear job, split it.
- Group related sub-files under a module folder instead of piling unrelated logic into a single source file.
- Every crate or package gets a documentation file for itself: `docs/<CRATE_NAME>.md`, living inside that crate's own directory, not a shared repo-wide docs folder.
- If a crate has enough files that one doc file gets unwieldy, split the documentation by part instead of forcing everything into a single file. See section 4 for how that's structured.

## 2. Top-of-File Notice Header

Every source file opens with a short header pointing to where its real documentation lives. Point it at whichever file actually holds that file's section: the crate's single doc file for a normal crate, or the specific part file for a crate that's been split. Make it visible, not buried three lines down where it'll get skipped.

Template (swap the comment token for the language: `//` for Rust, C#, JS, TS, C-family; `#` for Python, shell, TOML):

```
// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/<CRATE_NAME>.md, section "<file_name>"
// ============================================================================
```

Filled-in example, single-doc crate (mid-math):

```rust
// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/mid-math.md, section "mid_vec.rs"
// ============================================================================
```

Filled-in example, split-doc crate (dixscript, Go wrapper file):

```rust
// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/wrappers-go.md, section "go_wrapper.rs"
// ============================================================================
```

## 3. Inline Code Comments

- Explain what the code does and how, only where it isn't obvious from reading it. Skip comments on lines that already say what they do.
- Don't under-explain either. If something is genuinely tricky (an unsafe block, a non-obvious algorithm choice, a workaround for a platform quirk), say what it's doing and why, in a line or two.
- Comments never carry a fix history, a bug report, or a decision log. That content belongs in the crate's doc file, not the source.

Bad:

```rust
// Loop through the vector
for i in 0..n {
    // add one to i
    sum += arr[i];
}
```

Good:

```rust
// SIMD path needs 16-byte alignment; falls back to scalar add below
// that threshold to avoid touching unaligned memory.
if arr.len() >= SIMD_THRESHOLD {
    sum = simd_sum(arr);
} else {
    sum = arr.iter().sum();
}
```

## 4. The Crate Documentation File

### Default: one file per crate

Path: `docs/<CRATE_NAME>.md`.

Structure, top to bottom:

**Overview** - what the crate does, in a few lines.

**Modules** - one `###` heading per source file, matching the actual sub-file layout. Each section covers:
- What the file does, briefly
- Decisions made and why
- Benchmark results, if there's a corresponding bench file (link it, summarize the numbers)
- Tests, if any (link the test file, note what it covers)

**CI and Workflows** - list the `.yml` workflow files relevant to this crate and what each one checks: build, test, publish, bench runs, whatever applies. If a workflow depends on a helper script, commonly a `.py` file under a root-level `scripts/` directory, reference that script too: path and what it does.

**Fixes and Problems** - always the last section in the file. A log of bugs found, fixes made, and problems run into, organized by file so each one is easy to locate. New entries go under the file they belong to.

Skeleton:

```markdown
# mid-math

## Overview
Custom SIMD-dispatched vector and matrix math library for mid-engine.

## Modules

### `mid_vec.rs`
**What it does:** Hand-rolled small-vector container, union + MaybeUninit backed.

**Decisions:**
- Chose union + MaybeUninit over a Vec-backed fallback to avoid heap
  allocation for the common small-N case.

**Benchmarks:** see `benches/mid_vec_bench.rs`. Roughly 3x faster than
Vec<T> for N <= 8.

**Tests:** `tests/mid_vec_tests.rs`. Covers alloc/dealloc paths and
edge cases at N = 0 and N = 1.

## CI and Workflows

- `.github/workflows/mid-math-ci.yml` - build and test on push
- `.github/workflows/mid-math-bench.yml` - runs the benchmark suite, posts
  results. Depends on `scripts/plot_bench.py` to turn raw numbers into a chart.

## Fixes and Problems

### `mid_vec.rs`
- Fixed a double-free at N = 0 (uninitialized union member was being dropped).
```

### Large crates: splitting by part

Once a crate has grown enough files that a single doc file becomes hard to navigate, split it by part instead. A "part" is whatever subsystem grouping already exists in the code, for example dixscript's wrappers versus its LSP versus its compiler.

Layout:

- `docs/<CRATE_NAME>.md` becomes the index. It keeps the crate-level Overview, a short list of parts with a one-line description and a link to each part file, and the full CI and Workflows section (crate-wide workflows and any root scripts they depend on).
- `docs/<CRATE_NAME>/<PART_NAME>.md` holds one part's own Modules section (same per-file structure as above, scoped to files in that part) and its own Fixes and Problems section at the bottom, scoped to that part's files. If a workflow only touches that part, reference it here too, in addition to the index.

Index skeleton:

```markdown
# dixscript

## Overview
Multi-language scripting and configuration runtime with LSP support.

## Parts

- [Language wrappers](dixscript/wrappers.md) - Go, Odin, and Lua bindings
- [LSP](dixscript/lsp.md) - hover, completion, diagnostics, formatting
- [Compiler](dixscript/compiler.md) - parser, type checker, codegen

## CI and Workflows

- `.github/workflows/dixscript-publish.yml` - publishes the crate to crates.io
```

Part file skeleton (`docs/dixscript/wrappers.md`):

```markdown
# dixscript: language wrappers

## Modules

### `go_wrapper.rs`
**What it does:** Go language bindings for the dixscript runtime.

**Decisions:**
- ...

**Tests:** `tests/go_wrapper_tests.rs`

## Fixes and Problems

### `go_wrapper.rs`
- ...
```

## 5. Writing Style

- Third person, or first-person plural ("we chose", "the system falls back to"). Not "I", and not addressing the reader as "you" for decision write-ups.
- Human phrasing. No em dashes. No "leverage", "utilize", "seamless", or other filler that doesn't actually say anything. State what happened, plainly.
- Use words a working developer would actually use out loud, not marketing language.

## 6. Incremental Update Rule

When you touch a file:

1. Check whether its crate has documentation covering it: a single `docs/<CRATE_NAME>.md`, or, for a split crate, the relevant `docs/<CRATE_NAME>/<PART_NAME>.md`. If neither exists yet, create the appropriate one.
2. Check whether that file has its own section in the doc. If not, add one.
3. Clean up comments in the file you touched, following the rules above. Don't sweep the rest of the crate unless that's explicitly asked for.
4. If the file you're editing has fix history, decision notes, or problem logs sitting in its comments, move that content into the doc's Fixes and Problems section (or the relevant module section) and strip it out of the source.
