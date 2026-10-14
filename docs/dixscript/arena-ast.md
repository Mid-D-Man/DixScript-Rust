# dixscript: arena AST scoping

Scope of the change that would make the AST arena-allocated, measured against the
real code and the real `.mdix` corpus. This is a scoping document, not a design for
a finished migration: it records what was measured, what a migration would touch,
which design options exist, and what has to be decided before any code moves.

## Summary

- **An arena AST is faster, but the AST is not where a compile spends its time.**
  The resolved AST is 2.7% to 3.9% of a compile's heap allocations; a whole-tree
  clone costs 1.3% to 3.5% of compile time. Value resolution is 36% to 84% of compile
  time, and parsing is 7% to 26%.
- **Two cheaper findings came out of the measurement, and neither needs an arena:**
  the parsers reserve vector capacity from the length of the *whole token stream*
  at every nesting level (capping 17 sites cut requested bytes by 39% to 98%, parse-stage
  time by 9% to 37% and total compile time by 3% to 7%), and value resolution grows faster than linearly with
  input size (a 32x larger input took about 144x as long in that stage).
- **The migration is large and mostly mechanical:** 4,472 mentions of the 40 AST
  types across 132 of 196 core files, 134 public function signatures, and
  803 mentions in `mdix-lsp`. Mutation is rare (33 sites), which makes an arena
  tractable, but three things constrain the design: `mdix-lsp` stores the AST in a
  long-lived `Document`, the parser and the binary unpacker build sections in
  parallel, and the public API would change for every sibling crate.
- **Recommendation:** do the two arena-independent fixes first, re-measure, then decide
  between a reference arena and an index arena with a prototype of the index arena
  in the existing benchmark. See "Recommended sequence" and "Open decisions".
- **The index arena (option B) was benchmarked against the bumpalo arena (option A)**
  (Measurement 5, run in a sandbox and on a GitHub Actions runner). On the mirror types, B
  builds, copies and churns at least as fast as A on both machines, copies 2.2x to 8.8x
  faster, and costs more only on read-only traversal of a small tree (47% more
  instructions on the realistic profile). The structural points under "Design options"
  point the same way.
- **The arena for the real `Value` family exists** (`Compiler/AST/arena`, step 6 started).
  Nodes are 40, 48 and 48 bytes against 144, 160 and 384 for the owned `Value`,
  `Expression` and `QuickFuncStatement`, and a corpus round trip through the real compiler
  returns equal trees. Handles that pair a node with its arena print and compare exactly
  like the owned types. Nothing in the compiler uses it yet.

## How this was measured

All numbers are from one Linux x86_64 sandbox, Rust 1.85.1, `--release`, best of five
runs unless stated, on files from `mdix_files/`. Nothing in the repo was modified to
measure; the probe sources are in `others/arena_scope_probes/` with a README.

- **Allocation counts** use a global allocator that counts `alloc` and `realloc` calls
  and the bytes requested. "Bytes" means capacity *requested*, not memory touched or
  resident; it was not checked against RSS.
- **The AST's own allocation count** is measured by cloning it: every `String`, `Vec`
  and `Box` in the tree is one allocation and `clone()` makes exactly one per
  original. This measures the **final resolved** AST only. It excludes spare
  capacity, the parse-time AST, and any temporary allocation made on the way, so it
  is a lower bound on what the AST costs the pipeline.
- **Stage timing** uses marks inserted at the stage boundaries of
  `compile_source_from_bytes` in a scratch copy of the crate. The marks are labelled
  by the event that *starts* an interval, so in the probe's table the row named
  `config_done` is the parse stage, `parsed` is semantic analysis, `analyzed` is
  enhancement and `enhanced` is value resolution. The tables below use the stage names.
- **Corpus:** five of the 84 `.mdix` files, 2.5 KB to 445 KB, plus synthetic
  inputs built the way `chemistry_db_comparison_benchmark.rs` builds its scaled
  fixtures (the same 5-element block repeated N times, keys renamed).

## The AST today

`dixscript/src/Compiler/AST/` is 2,776 lines in 17 files and defines 40 public
types (excluding `Position`). Everything is owned: 24 `Box<` in total, `Vec<...>`
for children, and `String` for every identifier, literal and operator. Each
expression variant carries a `position: Position`, and operators are stored as
`String` (`operator: String`) rather than as an enum. All 40 types derive `Debug`,
`Clone` and `PartialEq`; nine simple ones are also `Copy`.

## Where a compile spends its time

Share of `compile_to_resolved_ast` wall time per stage:

| Stage | FullFunctionTest (53 KB) | ComplexFull (17 KB) | chemistry_db (445 KB) |
|---|---|---|---|
| Tokenize | 6.8% | 5.9% | 2.3% |
| Split `@CONFIG` tokens | 2.4% | 1.7% | 1.0% |
| Process `@CONFIG` | 0.2% | 0.4% | 0.0% |
| **Parse** | **26.3%** | **13.7%** | **7.0%** |
| Semantic analysis | 17.4% | 20.0% | 3.6% |
| Enhancement | 6.5% | 6.0% | 0.9% |
| **Value resolution** | **36.2%** | **48.6%** | **84.1%** |
| Homogenize, `@SCHEMA` | ~0% | ~0% | ~0% |
| Total (best of five) | 9.4 ms | 5.0 ms | 315 ms |

An arena can only touch the AST-building and AST-copying parts of these stages.

## Measurement 1: the AST's share of a compile

| File | Source | Allocs in a compile | Allocs in the resolved AST | Share of allocs | Share of bytes | One clone / compile time |
|---|---|---|---|---|---|---|
| CompleteFeature | 2.5 KB | 5,920 | 213 | 3.6% | 3.5% | 2.9% |
| ComplexData | 3.8 KB | 6,011 | 191 | 3.2% | 2.2% | 3.5% |
| ComplexFull | 17 KB | 49,425 | 1,384 | 2.8% | 1.5% | 2.1% |
| FullFunctionTest | 53 KB | 92,790 | 3,654 | 3.9% | 0.6% | 2.4% |
| chemistry_db | 445 KB | 1,160,240 | 30,864 | 2.7% | 0.1% | 1.3% |

Reading this correctly: the final AST is small compared with the work done to
produce it. The parse stage alone makes 27,110 allocations for FullFunctionTest, 7x
more than the finished AST contains, because tokens, lexeme strings and temporary
buffers are allocated and dropped on the way. Part of that is arena-addressable
(zero-copy `&str` into the source, nodes bump-allocated), part is not.

## Measurement 2: the arena benchmark

`dixscript/benches/arena_vs_box_benchmark.rs` run as a standalone crate (criterion
0.5, bumpalo 3; 30 samples, 3 s measurement, 1 s warm-up). It mirrors the shape of the
expression and statement types; it does **not** use the real types. Median time,
owned `Box` AST vs bumpalo arena AST:

| Operation | deep_expressions | large_file | realistic |
|---|---|---|---|
| Construct | 108.8 ms vs 48.1 ms (2.26x) | 6.10 ms vs 2.63 ms (2.32x) | 201 us vs 94 us (2.15x) |
| Clone vs rebuild | 133.8 ms vs 73.7 ms (1.82x) | 5.62 ms vs 1.60 ms (3.53x) | 197 us vs 46 us (4.26x) |
| Build and drop (churn) | 107.4 ms vs 27.0 ms (3.97x) | 6.14 ms vs 2.59 ms (2.37x) | 208 us vs 96 us (2.16x) |
| Traverse | 8.08 ms vs 5.38 ms (1.50x) | 344 us vs 322 us (1.07x) | 9.2 us vs 7.9 us (1.17x) |

So the arena's real advantage is allocation and teardown (2x to 4x); reading the
tree is barely affected (1.1x to 1.5x). "Churn" is the LSP pattern: build an AST per
edit and drop the last one. This table predates Measurement 5, which rewrote the
benchmark: the construct rows here include teardown on both sides (criterion's plain
`iter` drops the routine's output inside the timed loop), which is consistent with
construct and churn being nearly equal for the realistic profile. The index arena, the other serious
option, is measured in Measurement 5.

## Measurement 3: the parsers over-reserve capacity

The section parsers size their vectors from `self.tokens.len()`, the length of the
**entire token stream**, at every nesting level: each call's argument list, each
statement list, each array, each object. In `quickfuncs_section_parser.rs` and the
small parsers this is written inline (8 sites, for example
`Vec::with_capacity(usize::max(2, self.tokens.len() / 50))`); in
`data_section_parser.rs` it goes through `estimate_properties_count` and
`estimate_array_items_count` (9 sites). A 50,000-token file therefore reserves
capacity for roughly a thousand elements for every function call it parses.

Experiment, in a scratch copy only: cap all 17 reservations at 16 elements.

| File | Bytes requested before | after | Parse stage (median) | Total compile (median) |
|---|---|---|---|---|
| FullFunctionTest | 87.5 MB | 12.7 MB (-85%) | 2.88 -> 2.27 ms (-21%) | 10.87 -> 10.10 ms (-7%) |
| chemistry_db | 7.50 GB | 155.7 MB (-98%) | 22.3 -> 14.0 ms (-37%) | 333.6 -> 315.3 ms (-5.5%) |
| ComplexFull | 14.0 MB | 8.5 MB (-39%) | 0.73 -> 0.66 ms (-9%) | 4.71 -> 4.56 ms (-3%) |

Times are the median of nine interleaved runs of each binary, each run itself the
best of five. The byte figures are deterministic; the times are not: the same binary's
chemistry_db total ranged from about 310 ms to 334 ms across runs, so a total-time
difference under roughly 10% is within noise and only the parse-stage figures and the
direction of the totals should be relied on. An earlier single comparison suggested
-14% total on chemistry_db; the interleaved medians above replace it.

Allocation counts are unchanged (the vectors still grow, a handful more
reallocations). Requested bytes are not resident bytes, so the effect on RSS is
unmeasured. The change itself is mechanical and does not depend on any of the AST
design.

## Measurement 4: value resolution does not scale linearly

Same-shape input at 1x to 32x (5 to 160 elements; 19 KB to 565 KB):

| Multiplier | Parse | Value resolution | Total | Resolution allocations |
|---|---|---|---|---|
| 1x | 0.97 ms | 4.42 ms | 10.6 ms | 43.7 k |
| 2x | 2.34 ms | 9.56 ms | 18.6 ms | 86.7 k |
| 4x | 3.28 ms | 21.3 ms | 32.9 ms | 172.7 k |
| 8x | 6.87 ms | 50.9 ms | 70.3 ms | 344.7 k |
| 16x | 13.3 ms | 153 ms | 187 ms | 688.8 k |
| 32x | 26.9 ms | 634 ms | 699 ms | 1,377 k |

Parse is linear (about 2x per doubling). Value resolution's cost per doubling rises:
2.2x, 2.2x, 2.4x, 3.0x, 4.1x, which approaches the 4x of quadratic growth, while its
allocation count stays linear. That combination points at repeated scanning of
existing data rather than at allocation, so an arena would not help it. At 32x,
value resolution is 91% of the compile.

The likely place to look first is the "Phase 4" fixpoint loop in
`ValueResolution/value_resolver.rs` (around line 1604), which makes repeated passes
over a `pending` list of unresolved calls and, per call, formats the arguments with
`{:?}` and stamps `Utc::now()` into a history record. **This is a suspect, not a
diagnosis:** it was found by reading, not by profiling. The next step is a profile of
the 32x input.

## Measurement 5: option A against option B

`dixscript/benches/arena_vs_box_benchmark.rs` now compares three representations of the
same mirror AST: the owned `Box` tree, option A (bumpalo, `&'a` nodes) and option B (an
owned arena with `u32` ids into per-kind `Vec`s). All three build identical trees from the
same profiles, and an equivalence check proves it before any timing starts. The types are
still mirrors, not the real ones. Two runs are reported. The first comes from a
single-core Linux sandbox, Rust 1.75.0, `--release`, criterion 0.5.1 with shortened
windows (20 to 30 samples, 2 to 3 seconds, 1 second warm-up) and one run per group. The
second comes from the GitHub Actions runner (`ubuntu-latest`, stable Rust, full windows)
through the bench workflow, and is listed after the sandbox tables. Treat differences
under about 15 percent as noise in either. Sandbox medians:

| Operation | Profile | Owned | A (bumpalo) | B (index) |
|---|---|---|---|---|
| Construct, teardown excluded | realistic | 295 us | 163 us | 136 us |
| | deep_expressions | 126 ms | 63.2 ms | 52.4 ms |
| | large_file | 6.53 ms | 6.54 ms | 4.46 ms |
| Clone (A rebuilds) | realistic | 264 us | 87.0 us | 34.4 us |
| | deep_expressions | 173 ms | 90.2 ms | 41.4 ms |
| | large_file | 8.51 ms | 4.38 ms | 1.05 ms |
| Clone then edit every `Value` | realistic | 325 us | 87.2 us | 29.3 us (linear), 45.5 us (walk) |
| | deep_expressions | 190 ms | 89.6 ms | 44.0 ms (linear), 55.7 ms (walk) |
| | large_file | 9.79 ms | 2.90 ms | 1.30 ms (linear), 1.54 ms (walk) |
| Traverse | realistic | 8.5 us | 10.6 us | 12.5 us |
| | deep_expressions | 23.7 ms | 19.6 ms | 24.1 ms |
| | large_file | 408 us | 445 us | 328 us |
| Build and drop (churn) | realistic | 289 us | 118 us | 105 us |
| | deep_expressions | 177 ms | 32.0 ms | 29.9 ms |
| | large_file | 14.1 ms | 3.30 ms | 3.29 ms |

B without up-front reservation (`index_noreserve`) cost more than B with it on
construct and churn (for example 3.29 ms against 4.40 ms on `large_file` churn; 6% to
34% across the six cases, and 11% more instructions), so a
parser that cannot size the arena from the source length still keeps most of the gain.

Instruction counts are deterministic, so they are the safer comparison. Callgrind,
realistic profile, instructions per iteration:

| Operation | Owned | A | B |
|---|---|---|---|
| Build and drop | 2,523,125 | 1,064,019 | 988,516 (1,101,601 without reservation) |
| Clone (A rebuilds) | 2,156,540 | 373,735 | 161,747 |
| Clone then edit every `Value` | 2,214,461 | 373,753 | 177,449 (linear), 268,330 (walk) |
| Traverse | 84,422 | 84,221 | 123,394 |

Layout and allocations, realistic profile:

| | Owned | A | B |
|---|---|---|---|
| `Expression` / `Statement` size | 72 / 136 bytes | 56 / 104 bytes | 48 / 48 bytes |
| Allocations for one build | 4,534 | 1,459 | 1,466 (1,514 without reservation) |
| Peak live bytes for one build | 250,261 | 1,052,666 (reserved chunk) | 1,180,170 (reserved), 239,658 (unreserved) |

About 1,450 of the allocations in the A and B columns are the benchmark's own `format!`
temporaries for names, which all three representations pay. A real parser slices names out
of the source, so the real allocation gap between arena and owned is wider than this
table shows.

### GitHub Actions run

The same bench through the bench workflow (full sample windows, `ubuntu-latest`, stable
Rust). Medians; `unreserved` is B starting empty. Node sizes were 64 / 128 bytes for owned
(a newer compiler lays the owned enums out tighter than 1.75 does), and 56 / 104 and
48 / 48 for A and B as before.

| Operation | Profile | Owned | A (bumpalo) | B (index) |
|---|---|---|---|---|
| Construct, teardown excluded | realistic | 83.0 us | 108 us | 91.1 us (70.1 us unreserved) |
| | deep_expressions | 70.1 ms | 24.4 ms | 19.2 ms (25.3 ms unreserved) |
| | large_file | 2.96 ms | 2.79 ms | 2.67 ms (2.47 ms unreserved) |
| Clone (A rebuilds) | realistic | 99.0 us | 71.6 us | 8.16 us |
| | deep_expressions | 105 ms | 48.2 ms | 10.1 ms |
| | large_file | 2.94 ms | 2.86 ms | 1.11 ms |
| Clone then edit every `Value` | realistic | 118 us | 71.4 us | 10.0 us (linear), 19.4 us (walk) |
| | deep_expressions | 115 ms | 48.5 ms | 12.2 ms (linear), 18.3 ms (walk) |
| | large_file | 3.98 ms | 2.82 ms | 1.25 ms (linear), 1.48 ms (walk) |
| Traverse | realistic | 6.84 us | 5.70 us | 6.69 us |
| | deep_expressions | 16.4 ms | 7.58 ms | 6.62 ms |
| | large_file | 281 us | 266 us | 205 us |
| Build and drop (churn) | realistic | 167 us | 57.2 us | 48.5 us (54.9 us unreserved) |
| | deep_expressions | 91.3 ms | 22.9 ms | 16.1 ms (23.7 ms unreserved) |
| | large_file | 4.76 ms | 1.76 ms | 1.33 ms (1.78 ms unreserved) |

What the numbers say, across both runs:

- **B is at least as fast as A on construct, copy, copy-then-edit and churn, on both
  machines.** A plain copy is 2.2x to 8.8x faster on B (sandbox 2.2x to 4.2x, runner 2.6x to
  8.8x). Copy-then-edit is 1.6x to 7.1x faster, the linear pass beating the tree walk by
  1.2x to 1.9x. Churn is 1.2x to 1.4x faster on the runner and within 12% in the sandbox.
  Construct takes 4% to 22% less time on the runner and 17% to 32% less in the sandbox;
  the sandbox instruction count for build and drop is 7% lower.
- **Reading is B's cost, and it depends on tree size.** A traversal of the realistic tree
  runs 47% more instructions on B and was 17% to 18% slower in wall clock on both
  machines. On `large_file` B was faster in both runs (23% on the runner, 26% in the
  sandbox), and on `deep_expressions` it was 23% slower in the sandbox and 13% faster on
  the runner. The likely reading is that the extra instructions show only while the tree
  fits in cache, and B's smaller nodes win once it does not; that was not isolated.
  Semantic analysis and the converter only read, so this lands where an arena helps least
  anyway (see "Where an arena helps, and where it does not").
- **Construct against the owned tree depends on the machine.** In the sandbox both arenas
  were 1.8x to 2.4x faster than owned on `realistic` and `deep_expressions`. On the runner
  owned beat both arenas on `realistic` (83 us against 108 us and 91 us), the arenas were
  2.9x and 3.7x faster on `deep_expressions`, and all three were within 11% on
  `large_file`. B starting empty was the fastest build on `realistic` (70 us). The fixed
  byte budgets reserve about 1 MB for a tree that uses about 170 KB, and this group hands
  every iteration fresh memory, which is the probable cause; it was not isolated.
- **Churn, which reuses memory, is the steadier measure.** Both arenas were at least 2.4x
  faster than owned on both machines, and B was 1.2x to 1.4x faster than A on the runner.
  That edge comes largely from reserving: B starting empty took 13% to 47% longer than B
  reserved and landed within 4% of A.
- **Copy against the owned tree.** B was 2.6x to 12x faster than an owned clone on the
  runner. A's rebuild was 1.4x faster than an owned clone on `realistic` and tied it on
  `large_file`, so A's advantage on copy shrinks where the trees are small or shallow.

### What the benchmark files do

`benches/arena_vs_box_benchmark.rs` holds the criterion groups and `main`. The three
representations live in `benches/arena_shapes/`: `owned.rs`, `bump.rs` (option A) and
`index.rs` (option B), with `mod.rs` holding the profiles, the tree fingerprint and the
checks. Each representation builds the same programs, counts nodes, fingerprints itself
and implements the copy and edit operations the groups time. The index arena keeps one
`Vec` per node kind, `(start, len)` ranges for lists and for text, and two scratch stacks
so nested lists land contiguously. Groups: construct, clone and rebuild, clone then edit,
traverse, and build-and-drop churn.

### Decisions

- The ids `owned` and `arena` keep their old meaning, so the published summary history
  stays comparable; B is `index`, and `index_noreserve` is B starting empty.
- Operators are `&'static str` in both A and B builders, so neither pays to build them.
  Option A's rebuild still copies them into the new arena (the conservative reading), while
  B's clone never needs to. A real migration would use an operator enum.
- B reserves its buffers from the byte budget the bump arena gets, divided by the bump
  arena's real cost per expression (64 bytes). The equivalence check asserts that no buffer
  grew during a hinted build, so the label is true.
- The construct and clone groups use `iter_with_large_drop` so teardown is never in
  the timed region; teardown is what the churn group measures.
- The clone-then-edit group measures B two ways: a linear pass (cheap because nodes of a
  kind are contiguous, valid only for passes that ignore structure) and a tree walk (the
  like-for-like comparison with the owned walk).

### Tests

`verify_representations` runs before any timing. For every profile it builds each
representation, compares a structural fingerprint (node count, value sum, name bytes,
operator bytes, positions) with the owned tree, then checks every copy and edit against
the expected fingerprint. It also asserts that the hinted index arena never grew. A fourth,
untimed profile (`branches`) exercises the `If` paths. Result: all four profiles agree.
The bench compiles warning-free on Rust 1.75.0.

### Fixes and Problems

- **The old header comment was wrong about criterion.** It said plain `iter` defers drops
  until after timing. In criterion 0.5.1 it drops the output inside the loop, so the
  earlier construct numbers included teardown on both sides. Fixed by using
  `iter_with_large_drop` for construct and clone.
- **The timed profiles never emit an `If`.** `stmts_per_function` is 3 and
  `if_every_n_stmts` is 5, so `si % 5 == 4` never holds. The profiles were left as they
  are so results stay comparable; the untimed `branches` profile covers the code.
- **Option A's large-file rebuild varies with heap state.** The same rebuild measured 4.38
  ms in one group and 2.90 ms in another run, a spread of about 30%. The direction of every
  conclusion above is unaffected, but magnitudes on that profile are loose.
- **Construct order against owned is machine dependent.** The sandbox and the runner
  disagree for `realistic` and `large_file` construct, while the sandbox instruction counts
  and both churn results agree. The reservation budget is the first thing to test if
  construct matters.
- **Not covered:** the real types (`Value` carries `String`s), a parser that slices names
  from the source, parallel section builds, and the pipeline end to end.

## Blast radius

"Mention" below means a word-boundary occurrence of any of the 40 AST type names. It
is a proxy for the edit surface, not an exact count of required edits.

### Core (`dixscript/src`)

4,472 mentions in 132 of 196 files (75,558 lines outside `Compiler/AST` and
`Utilities`).

| Subsystem | Mentions | Files using the AST |
|---|---|---|
| `Runtime` | 938 | 15 of 17 |
| `Compiler/Core/SectionAnalyzers` | 758 | 9 of 9 |
| `Compiler/Core/SectionParsers` | 750 | 9 of 9 |
| `Compiler/Core/ValueResolution` | 678 | 6 of 6 |
| `Compiler/Core/BinarySerialization` | 468 | 19 of 24 |
| `Compiler/Extensions` | 216 | 1 of 2 |
| everything else | 664 | |

It is concentrated: 14 files hold 2,721 of the 4,472 mentions (61%), led by
`quickfuncs_section_parser.rs` (336), `quickfuncs_section_analyzer.rs` (331),
`value_resolver.rs` (283), `Runtime/converter.rs` (280) and
`function_interpreter.rs` (218).

### Public API

134 `pub fn` signatures mention an AST type in a parameter or return type. These are
the breaking changes: any sibling that touches them has to move.

### Sibling crates

| Crate | Mentions | AST types used | Depends on dixscript |
|---|---|---|---|
| `mdix-lsp` | 803 in 22 files | 23 | published 1.0.0 |
| `mdix-cli` | 52 | 4 | published 1.0.0 |
| `mdix-wasm` | 26 | 2 | published 1.0.0 |
| `mdix-lua` | 24 | 2 | local path |
| `mdix-ffi` | 9 | 2 | published 1.0.0 |
| `mdix-python` | 8 | 2 | published 1.0.0 |
| `mdix-java` | 6 | 2 | local path |

The bindings touch only two types (`DixScript` and `Value`), so their change is small
and shallow. `mdix-lsp` is the real consumer. Because most siblings depend on the
*published* 1.0.0, none of them breaks until it bumps its dependency; `mdix-lua` and
`mdix-java` use the local path and break immediately.

## How the AST is used

- **Mutation is rare.** 33 mutation sites (`&mut AstType`, `iter_mut`, `as_mut`) across
  the core, of which 15 are `&mut AstType` parameters. That is the single most
  favourable fact for an arena: an immutable arena AST is workable.
- **Whole-tree clones are few and identifiable.** About 15 sites clone a whole
  `DixScript`, including the enhancer (`general_ast_enhancer.rs`, clone-then-mutate),
  the value resolver (twice) and the loader (to inject `@SECURITY` and for DLM).
  Another 102 lines mix an AST type with a `.clone()`; that is an upper bound on
  node-level cloning, not a count.
- **The tree is already shared by reference in places.** `GeneralSemanticAnalyzer<'a>`
  holds `&'a DixScript`; `EnhancementResult` owns `enhanced_ast: DixScript`.
- **Long-lived storage is the hard part.** `mdix-lsp`'s `Document` owns
  `Option<DixScript>` and a `SemanticAnalysisResult` across edits. A `DixScript<'a>`
  borrowing from a `Bump` cannot sit in that struct without a self-referential
  owner. Ubel Stratum hit the same wall: its `crates/lsp/src/document.rs` says its
  arena AST "borrows an arena and cannot simply be stored beside the source", and its
  LSP does not cache the AST.
- **Three places run in parallel** (non-wasm, `rayon-support`):
  `general_parser.rs` parses sections with `into_par_iter`, `binary_unpacker.rs`
  decodes sections with `par_iter`, and `binary_packer.rs` encodes sections with
  `par_iter`. Reading an immutable arena AST from several threads is fine; *allocating*
  into one from several threads is not (`bumpalo::Bump` is not `Sync`). The parser and
  unpacker would need an arena per section or per thread.

## Design options

### A. Reference arena (bumpalo, `&'a` nodes): what the benchmark measured

Nodes are `&'a Expression<'a>`; strings are `&'a str` (into the source or the arena);
children are `&'a [T]`. Matches the benchmark and Ubel Stratum.

- **For:** the largest allocation speed-up (2x to 4x), zero-copy strings, cheapest
  teardown; `bumpalo` is already in the workspace for the benchmark.
- **Against:** the lifetime `'a` reaches all 4,472 mentions and 134 public signatures
  and every sibling. `mdix-lsp`'s `Document` needs a self-referential owner
  (`ouroboros` or `self_cell`, a new dependency that cuts against the
  dependency-reduction work, or hand-written unsafe lifetime erasure). Parallel
  section parsing needs one `Bump` per section, so the root has to own several
  arenas. `Bump` is not `Sync`, so any future parallel pass that allocates is blocked.
  Clone-then-mutate has to become rebuild.

### B. Index arena (`NodeId(u32)` into per-kind `Vec`s)

`DixScript` owns an `Arena` of `Vec<Expression>`, `Vec<Statement>`, and so on; child
links are `u32` ids; strings become interned `Symbol(u32)` or `(start, end)` spans
into the source.

- **For:** no lifetime anywhere, so `DixScript` stays an owned, `'static`, `Send + Sync`
  value and the `Document` problem disappears. A clone is a `Vec` clone (a handful of
  allocations instead of thousands). Mutation in place is cheap, which suits the
  clone-then-mutate pipeline. Parallel sections build their own `Arena` and merge
  with id offsets.
- **Against:** every access needs the arena (`arena.expr(id)`), so signatures and
  pattern matches change shape rather than just gaining a lifetime; node-by-node
  pattern matching over children is more verbose. Measurement 5 measured its speed on
  the mirror types, in a sandbox and on a GitHub Actions runner: it builds, copies and
  churns at least as fast as A, copies 2.2x to 8.8x faster, and reads slower on a small
  tree (47% more instructions on a traversal), and it is far faster than owned `Box`
  trees on copy and churn.

### C. Stay owned and cut the waste

No arena. Replace `operator: String` with an operator enum (it appears in several
expression variants), intern repeated identifiers as `Arc<str>`, stop cloning the whole
tree where a section-level change would do (the loader's `@SECURITY` injection, the
enhancer's clone-then-mutate), and fix Measurements 3 and 4.

- **For:** no API-shape change beyond the operator enum, no lifetimes, and it captures
  the measured waste directly.
- **Against:** does not reach the 2x to 4x construct and churn gains, which matter most
  to the LSP.

## What an arena could buy end to end

This is an estimate, built from the measured stage shares and the benchmark ratios; it
is **not** a measurement of an arena in the real pipeline.

- Parse is 7% to 26% of compile time; if an arena roughly halves it, that is about
  3% to 13% of a compile.
- Whole-tree clones cost 1.3% to 3.5% of compile time each; removing or shrinking the
  two or three on the compile path is worth a few percent more.
- So roughly **5% to 15% end to end on small and medium files, under 5% on large,
  data-heavy ones**, where value resolution dominates. The LSP rebuilds on every edit
  and so gains more from the churn numbers, but no LSP-level measurement was made.

For comparison, Measurement 3 recovered 9% to 37% of parse time and 3% to 7% of total
compile time with a 17-site change, and Measurement 4 is a 91% share of the largest
synthetic input.

## Recommended sequence

1. **Profile and fix value resolution's scaling** (Measurement 4). It is by far the
   largest measured cost and it is independent of the AST.
2. **Cap the parser reservations** (Measurement 3, 17 sites). Small, mechanical,
   immediately measurable. Re-run `others/arena_scope_probes` afterwards.
3. **Replace `operator: String` with an enum**, and remove the whole-tree clones that a
   section-level change could avoid (the loader's security injection first).
4. **Re-measure.** Re-run the allocation and stage probes. The case for an arena is
   whatever is left of parse, enhancement and LSP churn after steps 1 to 3.
5. **Prototype option B in the existing benchmark** next to the bumpalo version, using
   the real shapes (`Value` with its `String`s, the real expression variants) and
   `size_of` of the real types. Decide A or B from numbers. (Done on the mirror types:
   Measurement 5. A run on the real types is still open.)
6. **If proceeding**, migrate in this order, each step compiling with the 703/674 test
   counts intact: AST types and the binary unpacker and parser (constructors first);
   then the read-only consumers (analyzers, converter, serializers); then the mutating
   ones (enhancer, resolver); then `Runtime`; then DLM. Siblings move last, behind a
   major version bump of `dixscript`; `mdix-lsp` last of all. (Started: the arena for the
   `Value` family exists and round-trips; see "Compiler/AST/arena".)

## Risks

- **A public API break across every sibling**, unavoidable under A or B; it must ship
  as a major version.
- **`mdix-lsp` document caching** under option A (self-referential owner or no cache).
- **Parallel sections** under A (one arena per section; the root owns several).
- **A new dependency** if A is chosen with `ouroboros`/`self_cell`.
- **The migration is 61% concentrated in 14 files**, so it can be sliced, but
  `quickfuncs_section_parser.rs` and `quickfuncs_section_analyzer.rs` alone are over
  6,600 lines.
- **Benchmark fidelity:** the existing benchmark uses mirror types, not the real ones,
  and does not model the parallel build (option B is modelled since Measurement 5).

## Open decisions

1. What is the goal: compile throughput, LSP latency and memory, or code structure?
   The measurements favour different answers for each.
2. Option A or B: B has numbers now (Measurement 5) and is as fast or faster in every
   operation except traversal of a small tree, so the remaining question is whether a
   lifetime on `DixScript` is acceptable, or whether it must stay an owned `'static` value.
3. Is a major version bump (and the work in `mdix-lsp`) acceptable?
4. Do steps 1 to 3 go first, before any arena work?

## Not measured

- RSS or peak resident memory (only requested bytes).
- The parse-time AST and intermediate clones (only the final resolved AST).
- Any LSP request path (semantic tokens, hover, completion) or `Document` memory.
- Either arena in the real pipeline, on the real types, or with parallel section builds.
- Whether the value-resolution suspect above is the cause.
- Non-Linux, 32-bit and wasm32 behaviour.

## Progress

Work done against the recommended sequence. Every step runs the loop: baseline, one
change, correctness gate, repeated measurement, promotion. The measurements below come
from a single-core Linux sandbox, Rust 1.75.0, `--release`, `default-features = false`.
Sub-millisecond timings move by about 10 percent between sessions there, so only the
larger files and the byte counts are used to judge a change.

### Step 2 done: parser reservations capped

Medians of four interleaved rounds (before, cap-all, final), each round the best of five
runs per file. "Final" is what shipped: nested vectors capped at 16, the four
whole-section vectors uncapped.

| File | Requested bytes before | After | Parse stage before | After | Total before | After |
|---|---|---|---|---|---|---|
| CompleteFeature (2.5 KB) | 642,976 | 624,816 (-2.8%) | 0.123 ms | 0.125 ms (noise) | 0.636 ms | 0.655 ms (noise) |
| ComplexData (3.8 KB) | 981,672 | 732,720 (-25.4%) | 0.058 ms | 0.053 ms | 0.634 ms | 0.582 ms |
| ComplexFull (17 KB) | 14,255,465 | 8,839,569 (-38.0%) | 0.829 ms | 0.735 ms (-11.4%) | 5.185 ms | 5.206 ms |
| FullFunctionTest (53 KB) | 87,960,472 | 13,156,162 (-85.0%) | 2.628 ms | 2.090 ms (-20.5%) | 9.943 ms | 9.318 ms (-6.3%) |
| elements_database (445 KB) | 7,501,904,105 | 159,365,102 (-97.9%) | 19.73 ms | 14.16 ms (-28.3%) | 341.2 ms | 311.8 ms (-8.6%) |

- An earlier three-round run of the cap-all variant measured the 445 KB parse stage at
  23.7 ms to 13.0 ms (-45%) and the total at -6.6%. The parse-stage gain on that file is
  therefore somewhere between 28% and 45%, the total between 6% and 9%.
- Allocation counts did not move (at most 6 more reallocations), which is why this is a
  bytes and time win and not an allocation-count win.
- Cap-all (every site at 16) and the shipped variant are within noise of each other. The
  shipped one keeps the full estimate where the stream length is a good guide.
- Correctness gate: every `.mdix` file under `mdix_files` (84 files) compiles to the same
  result before and after. 80 are compared by an order-insensitive fingerprint, 17 of
  those also by a strict one, and 4 whose output contains random or time-based values
  are compared by compile status only. The gate is `others/arena_scope_probes/golden_main.rs`.
- `cargo test -p dixscript` passes (710 tests), and a new allocation-budget test fails at
  88.0 MB without the cap and passes at 13.2 MB with it.

### Finding: compile output is not deterministic

The gate had to be order-insensitive because the compiler's own output changes from run
to run. Two compiles of the same file in two processes differ in the order of the `@CONFIG`
entries (every file) and in the order of object properties (for example
`ComparisonFiles/MDIX/AdvancedTest.mdix` and `Unholy.mdix`), apart from the wall-clock
`created` timestamp. That points at a `std::collections::HashMap` being iterated, whose
order is randomized per process. It matters for the arena work in two ways: a golden
comparison cannot be strict, and the binary packer will produce different bytes for the
same source. It was not investigated further.

### Step 1 done: value resolution scaling

The cause was not the suspect in this document. It was the write-back of each resolved call
into the data tree: `replace_value_in_ast_by_location` searched every entry, depth first, for
the node at the call's position, once per call. N calls in a tree of N nodes is N squared. It
was found by profiling a 4x and a 16x scaled input under callgrind (instruction counts, so no
timing noise) and comparing each function's inclusive cost between the two:

| Function | 4x input | 16x input | Growth for 4x more input |
|---|---|---|---|
| `ValueResolver::replace_in_value` | 50.8M instructions | 802.1M | 15.8x (quadratic) |
| `format!` (`alloc::fmt::format_inner`) | 24.5M | 97.3M | 4.0x (linear) |
| `String::clone` | 20.7M | 69.7M | 3.4x (linear) |
| Whole compile | 200.5M | 1,373.8M | 6.9x |

Positions are now indexed once before phase 4 and a result is written back by path. Same
sandbox, debug mode off in the inputs, release build, one run per figure from one
interleaved invocation (so read the percentages, not the absolute milliseconds):

| Input | Total before | Total after | Value resolution before | After |
|---|---|---|---|---|
| 4x scaled | 27.0 ms | 20.9 ms (-23%) | 18.6 ms | 12.6 ms |
| 16x scaled | 221.3 ms | 81.4 ms (-63%) | 196.3 ms | 52.9 ms |
| 32x scaled | 1,108 ms | 189 ms (-83%) | 1,049 ms | 135 ms |
| `elements_database` (445 KB) | 320.6 ms | 104.2 ms (-67%) | 289.7 ms | 74.2 ms |

Callgrind at 16x: 1,374M to 502M instructions (-63%); parsing (50.4M) and semantic analysis
(53.8M) did not change, so the whole gain is in value resolution.

- Correctness gate: the same 84-file comparison as above passes against the original
  baseline (80 files by content, 17 also by a strict hash, 4 by compile status only). The
  strict set is now taken from 12 baseline runs, since three can agree by luck.
- `cargo test -p dixscript` passes, including `value_resolution_scaling`, which fails with
  the old behavior (ratio 10.0 against a limit of 7) and passes with the fix (ratio 4.3).
- Value resolution is still 62 to 72 percent of a compile at these sizes. What is left in
  it: `format!` is 19.4 percent of all instructions at 16x (arguments and locations are
  formatted into strings for every resolved call), and `String` and `Vec` clones another 13.6
  and 14.4 percent. Those are the next targets, and the clones are what an arena changes.

### Step 6 started: the arena for the Value family

The first piece of the migration is additive, so every delivered drop compiles with the
existing tests intact. `Value`, `Expression` and `QuickFuncStatement` (with `ObjectProperty`
and `SwitchCase`) are one mutually recursive family, so they move together: a `Value` holds
`Expression`s, an `Expression` holds a `Value`, and a lambda `Value` holds statements that hold
both. They also hold most of a compile's allocations. The new `Compiler/AST/arena` module
stores the family in one owned `Arena` and copies trees in and out losslessly, so consumers can
move over one at a time. A round trip through the real compiler over the whole `mdix_files`
corpus returns equal trees, and handles that pair a node with its arena print and compare
exactly like the owned types. See the sections "Compiler/AST/arena" and "tests/arena_roundtrip.rs"
at the end of this document.

### Where an arena helps, and where it does not

A callgrind profile of one `FullFunctionTest` compile (59.8M instructions, debug mode as
written in the file) gives the shares: value resolution 30.8 percent, parsing 23.6 percent,
semantic analysis 20.5 percent. Allocation and deallocation together are about 38 percent of
all instructions.

- **Semantic analysis only reads the AST,** so an arena speeds it up very little. Its cost is
  in the analyzers' own work: the quickfuncs analyzer is 80 percent of it (16.3 percent of the
  compile), of which call-graph validation is 8.2 percent (5.5 percent is
  `CallGraph::get_statistics`, which only runs when `debug_mode` is regular or verbose) and
  `Keywords::is_reserved_in_context` is 2.4 percent (it allocates a lowercase and an
  uppercase `String` per call). `String` clones in the whole stage were 0.27 percent.
- **The benchmark corpus runs with debug output on.** Every file in the benchmark set sets
  `debug_mode` to verbose or regular, and only 5 of the 32 corpus files set it to off. With it
  forced off the same files compile 17 to 21 percent faster and make 10 to 16 percent fewer
  allocations. The stage shares barely change. Use debug mode off for performance work.
- **Cloning is where an arena pays.** In value resolution, `DixScript::clone` is 2.2 percent
  of that profile, `QuickFunction::clone` 1.3 percent (342 calls) and a `Vec` clone 0.9.

### Not done yet

- Step 2 follow-ups inside value resolution: the `format!` per resolved call, and the clones.
- Cheap semantic analysis fixes: the keyword check's allocations.
- Steps 3 and 4 (operator enum, whole-tree clones, re-measure).
- The rest of step 6. Done: arena-aware `Display`, `Debug` and equality through handles
  (`ValueRef`, `ExprRef`, `StmtRef`). Planned drops, each compiling with the tests intact:
  2. An arena-resident `DixScript`: the section structs hold ids into one owned arena, with
     import and export for the whole file and the same corpus round trip.
  3. Parser constructors that build into the arena (with scratch stacks for lists, since child
     counts are unknown while parsing), converting at the end of the parse until consumers move.
  4. Read-only consumers, then mutators (enhancer, resolver), then `Runtime`, then DLM. Siblings
     last behind the 2.0.0 bump, `mdix-lsp` last of all.

## Compiler/AST/arena

### What it does

`dixscript/src/Compiler/AST/arena/` is option B for the real `Value`, `Expression` and
`QuickFuncStatement` family. One owned `Arena` holds a `Vec` per node kind (`ValueNode`,
`ExprNode`, `StmtNode`, plus object properties, switch cases and unknown-value records), a
`Vec` per list kind, and one `String` for every piece of text. Children are `u32` ids
(`ValueId`, `ExprId`, `StmtId`), lists are `(start, len)` spans (`ValueList`, `ExprList`,
`StmtList`, `StrList`, `PropList`, `CaseList`) and text is a span (`Str`). The arena has no
lifetime, so it is a `'static`, `Send + Sync`, `Clone` value, and every node is `Copy`.

`Arena::import_value`, `import_expr` and `import_stmt` copy an owned tree in and return the id
of its root. `export_value`, `export_expr` and `export_stmt` copy a tree back out as the
original owned types. `export(import(x)) == x` for every tree. That pair is the migration
seam: a consumer that has moved reads the arena, and one that has not gets the owned tree it
expects. The module is `pub mod arena` in `Compiler/AST/mod.rs`, which is the only existing
file it touches. It is reached as `Compiler::AST::arena`, not re-exported flat.

A bare id cannot be printed or compared, so `Arena::value_ref`, `expr_ref` and `stmt_ref`
return `Copy` handles (`ValueRef`, `ExprRef`, `StmtRef`) that hold an id and its `&Arena`. They
implement `Display`, `Debug` and `PartialEq` the way the owned types do: `Display` prints the same
text, `Debug` prints the same text as the owned derived `Debug`, and `==` is structural and works
across two different arenas.

Layout, measured with `size_of` on Rust 1.75, x86_64:

| Owned type | Bytes | Arena node | Bytes |
|---|---|---|---|
| `Value` | 144 | `ValueNode` | 40 |
| `Expression` | 160 | `ExprNode` | 48 |
| `QuickFuncStatement` | 384 | `StmtNode` | 48 |

The owned sizes are large because the variants carry `String`s and `Vec`s inline, `Value`
carries the three-string `Unknown` variant, and `QuickFuncStatement::ObjectCreation` carries a
whole `Value`. This is the layout the benchmark's mirror types stood in for: the mirror's owned
types were 72 and 136 bytes.

### Decisions

- **Additive first.** Nothing in the compiler uses the arena. The alternative, changing
  `Value` in place, breaks about 4,500 mentions in 132 files at once and cannot be delivered
  as a compiling drop. Consumers move one at a time through import and export instead.
- **The whole family in one arena.** The three types reference each other, so no one of them can
  move alone.
- **`Option` is kept where the owned type has it.** `Option<ExprList>` keeps `None` (a property
  or enum access) apart from `Some` of an empty list (a call with no arguments) for
  `QualifiedIdentifier` and `BuiltinFunction`, and `Option<StmtList>` and `Option<CaseId>` do the
  same for `else` and `default`. A copy that merged them would change meaning.
- **`Unknown` values live out of line.** Their `CompatibilityResult` owns strings and would
  have made every `ValueNode` larger, so the node holds an `UnknownId` into a side pool.
- **Names and operators are `Str` spans in one text buffer.** They are not interned and not an
  operator enum yet (that is step 3), so the same operator text is stored once per use.
- **`Position` is kept as it is** (two `usize`, 16 bytes) so the copy is lossless. Narrowing it
  is a later size lever.
- **Lists are imported by reserving the whole run first** with placeholder entries and filling
  them as each child is imported. Children with lists of their own append after the run, so
  every run stays contiguous without a scratch stack. A parser, which does not know a list's
  length in advance, will need the scratch stacks the benchmark's index arena uses.
- **No wildcard arms.** Every `match` in `import.rs` and `export.rs` lists every variant, so
  adding a variant to an owned type fails to compile until the arena handles it.
- **Ids are `u32`.** An arena that would exceed about 4.29 billion nodes, list entries or bytes
  of text panics with a clear message instead of wrapping.
- **`Display` and equality are written natively, not by exporting.** Converting to the owned
  tree to print or compare would allocate the whole tree, and the compiler formats call
  arguments for every resolved call. The wording and the comparison rules are copied from the
  owned types and gated by tests that compare the two.
- **`Debug` delegates to the owned type's `Debug`** by exporting the node, so its output is
  identical (including `{:#?}`). It allocates, so it is for diagnostics only.
- **Equality follows the owned rules.** Same variant, equal fields, children, text and
  positions, with floats compared as IEEE numbers, so a tree holding `NaN` is not equal to
  itself, exactly as with the owned types.
- **The node types do not implement `PartialEq`.** Their children are ids, so a derived `==`
  would compare ids and say two equal trees in different arenas differ. Trees are compared
  through the handles.
### Tests

`tests/arena_roundtrip.rs`; see its section below.

### Fixes and Problems

- **Bare nodes still cannot be printed with their text.** A node's own `Debug` shows ids and
  spans. Consumers print and compare through the handles; a consumer that holds only a node
  must be given the arena.
- **`Debug` on a handle allocates**, because it exports the node first.
- **Importing copies.** Import and export each copy the tree, so using them at a boundary adds a
  copy. They are a migration tool, not the end state.

## tests/arena_roundtrip.rs

### What it does

Proves the arena holds the family completely, and that its handles print and compare like the
owned types. Hand-built samples cover every variant of
`Value` (25), `Expression` (24) and `QuickFuncStatement` (9), plus edge cases: `None` against an
empty list, a switch with and without a default, an `else` that is `None` against empty, empty
arrays, objects and lambdas, and each `CompatibilityResult` variant inside `Unknown`. The same
samples are imported three times into one arena to catch ids or spans that collide. Then every
`.mdix` file under `mdix_files/` is compiled with the real compiler and every `Value`,
`Expression` and statement reachable from its `@DATA`, `@RAW`, `@SECURITY`, `@SCHEMA` and
`@QUICKFUNCS` sections is imported into one arena per file and exported again; its `Display`
output is compared with the owned type's, and it is compared with a second copy imported into a
different arena in reverse order, so the same tree has different ids there.

### Decisions

- The variant names come from `match` expressions with no wildcard arm, and the samples are
  checked against them, so a new variant stops the file compiling until a sample exists.
- Files that do not compile are skipped, and a floor on the number of files that compile and
  on the number of roots checked keeps the corpus test from passing on nothing.
- Every root of a file is imported before any is exported, so interleaved ids and spans are
  exercised on data the compiler really produces.
- A static assertion checks that `Arena` is `Send + Sync + Clone + 'static`.
- Equality is checked three ways, because each catches what the others miss: every ordered
  pair of samples across two arenas must agree with the owned `==`; 125 hand-built
  pairs that differ in exactly one field must compare unequal (the all-pairs check cannot
  prove a field is compared when only one sample uses it, which a deliberate bug confirmed);
  and neighbouring corpus roots must agree with the owned answer.
- Special floats (whole, negative zero, `NaN`, infinity, very large and very small) are
  included because `Display` and equality treat them specially.

### Tests

Sixteen tests, all passing on Rust 1.75: the sample-coverage check, one round trip each for
values, expressions and statements, `None` against an empty list, everything imported into one
arena, a cloned arena, readable positions, `Display` against owned `Display`, `Debug` against
owned `Debug`, equality against owned equality for every pair, every compared field, `NaN`,
trees that differ in one leaf, node size against owned size, and the corpus. In the
sandbox the corpus has 84 files, of which 70 compile, and the test round-trips 5,993 values,
1,173 expressions and 1,712 statements (41,651 arena nodes including nested ones). The suite
passes: 547 unit tests, 178 integration tests including these sixteen, and 2 doc tests.

A mutation check confirmed the tests are not vacuous: with `Boolean` equality ignoring its value,
`is_mutable` left out of the statement comparison, and `as<` changed in `Display`, the
every-compared-field test and the `Display` test failed, while the all-pairs test missed the
`Boolean` bug. The files were restored afterwards.

### Fixes and Problems

- **14 corpus files do not compile in the sandbox.** Four need network access to download cloud
  imports (`06_cloud_basic`, `07_cloud_verify_invalid`, `09_cloud_transitive`,
  `10_cloud_cyclic`), so a run with network access round-trips more files. Others are deliberate
  failures (`02_bzip2` and `03_lzma`, whose codecs are removed, and the circular-import files)
  or fail analysis for their own reasons (`EnumsWithStuffImported`, `all_datatypes_test`,
  `datetime_static_test`, `random_static_test`, `FullDataTestNoFunc`, `01_basic_import`). The
  test skips them.
- **The corpus contains no `TypeCast` expressions**, so `Display` and equality for that variant
  are covered by the samples only. The mutation check above showed the corpus test does not
  notice a `Display` change there.
- **Only section values are roots.** `@CONFIG` values use their own `ConfigValue` type and are
  not part of this family, so they are not exercised.
