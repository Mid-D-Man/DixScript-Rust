# arena_scope_probes

Scratch measurement code behind `docs/dixscript/arena-ast.md`. It is not part of
the workspace (there is deliberately no `Cargo.toml` in this folder: a crate inside
the workspace directory but outside `members` would make cargo refuse to build),
so build each probe as its own crate somewhere else.

## alloc_probe_main.rs

How much of a real compile is the AST? Counts heap allocations with a global
allocator. The AST's own allocation count is measured by cloning it: every `String`,
`Vec` and `Box` in the tree is one allocation, and `clone()` makes exactly one per
original.

```toml
# alloc_probe/Cargo.toml
[package]
name = "alloc_probe"
version = "0.0.0"
edition = "2021"
rust-version = "1.85"
[dependencies]
dixscript = { path = "/path/to/DixScript-Rust/dixscript", default-features = false }
```

Copy `alloc_probe_main.rs` to `alloc_probe/src/main.rs`, then
`CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo build --release` (the
fallback keeps the resolver on crate versions that build with Rust 1.85) and run it
with one or more `.mdix` paths.

## stage_probe_main.rs and patch_stage_marks.py

Per-stage time and allocation breakdown. `patch_stage_marks.py <dest>` copies
`dixscript/` to `<dest>` and adds marks to `compile_source_from_bytes`;
point a second crate's `dixscript` dependency at `<dest>/dixscript` and use
`stage_probe_main.rs` as its `src/main.rs`. Add `--cap-capacity` to the script to
repeat the parser-reservation experiment.

The table's rows are labelled by the mark that STARTS the interval, so the row
named `config_done` is the parse stage, `parsed` is semantic analysis, `analyzed` is
enhancement and `enhanced` is value resolution.

## The benchmark

`dixscript/benches/arena_vs_box_benchmark.rs` only needs `criterion` and `bumpalo`,
so it also runs as a tiny standalone crate (`[dev-dependencies]` criterion 0.5 with
default features off, bumpalo 3 with `collections`, one `[[bench]]` with
`harness = false`).
