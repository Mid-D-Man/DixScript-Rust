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
edit and drop the last one. The benchmark does not cover an index arena, which is the
other serious option (below).

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
  pattern matching over children is more verbose. Its speed is **unmeasured**: the
  benchmark does not model it, and it will be somewhat slower than A (bounds-checked
  index reads, ids instead of pointers) though far faster than owned `Box` trees.

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
   `size_of` of the real types. Decide A or B from numbers.
6. **If proceeding**, migrate in this order, each step compiling with the 703/674 test
   counts intact: AST types and the binary unpacker and parser (constructors first);
   then the read-only consumers (analyzers, converter, serializers); then the mutating
   ones (enhancer, resolver); then `Runtime`; then DLM. Siblings move last, behind a
   major version bump of `dixscript`; `mdix-lsp` last of all.

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
  and does not model option B or the parallel build.

## Open decisions

1. What is the goal: compile throughput, LSP latency and memory, or code structure?
   The measurements favour different answers for each.
2. Option A or B, once B has a number: is a lifetime on `DixScript` acceptable, or
   must it stay an owned `'static` value?
3. Is a major version bump (and the work in `mdix-lsp`) acceptable?
4. Do steps 1 to 3 go first, before any arena work?

## Not measured

- RSS or peak resident memory (only requested bytes).
- The parse-time AST and intermediate clones (only the final resolved AST).
- Any LSP request path (semantic tokens, hover, completion) or `Document` memory.
- Option B's speed, and the arena in the real pipeline.
- Whether the value-resolution suspect above is the cause.
- Non-Linux, 32-bit and wasm32 behaviour.
