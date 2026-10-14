// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Measurement 5: option A against
// option B"
// ============================================================================
//! Owned AST against option A (bumpalo, `&'a` nodes) against option B (index
//! arena, `u32` ids).
//!
//! This does NOT exercise dixscript's real `Compiler::AST` types. It is a
//! standalone mirror used to decide, before any migration work starts, which
//! arena shape is worth the rework. The three representations live in
//! `arena_shapes/` (`owned.rs`, `bump.rs`, `index.rs`) and build identical trees
//! from the profiles in `arena_shapes/mod.rs`. Before any timing, `main` checks
//! that all three agree on every profile (`verify_representations`).
//!
//! Profile sizes are grounded in mdix_files/advanced/FullFunctionTest.mdix (162
//! functions, about 2.6 statements per function, mostly depth 1 to 2
//! expressions). "realistic" reproduces that shape; "deep_expressions" and
//! "large_file" stress the two axes (expression depth, function count) separately.
//!
//! Groups (the benchmark ids `owned` and `arena` keep their earlier meaning, so
//! history in the published summary stays comparable):
//!   arena_vs_box_construct     build one fresh tree; teardown is not timed
//!   arena_vs_box_clone_rebuild the `ast.clone()` of general_ast_enhancer.rs::enhance().
//!                              Option A has no clone: it walks and re-allocates
//!                              into a FRESH arena, with no structural sharing,
//!                              which is the fair, conservative case. Option B's
//!                              clone is a clone of its Vecs.
//!   arena_vs_box_clone_mutate  the whole enhancer step: copy the tree, then add to
//!                              every Value. Option B is measured two ways: a
//!                              linear pass over its Vec (cheap because nodes of a
//!                              kind are contiguous) and a tree walk (the
//!                              like-for-like version of what owned does).
//!   arena_vs_box_traverse      walk an already-built tree, repeatedly; no allocation
//!   arena_vs_box_churn         build and drop in one timed unit, the mdix-lsp
//!                              reparse-on-keystroke pattern. Uses iter_custom
//!                              because the drop is exactly what is being measured.
//!
//! `index` reserves its Vecs from the same byte budget the bump arena gets;
//! `index_noreserve` starts empty and shows the cost of growing.

mod arena_shapes;

use arena_shapes::{bump as arena, index, owned, Profile, MUTATE_DELTA, PROFILES};
use criterion::{black_box, criterion_group, BenchmarkId, Criterion, Throughput};
use std::time::{Duration, Instant};

/// (sample_size, measurement_time) tuned per profile, same philosophy as
/// stress_test_benchmark.rs: fewer samples and longer windows as tree size (and
/// therefore per-iteration cost) grows.
fn tuning_for(profile: &Profile) -> (usize, Duration) {
    match profile.name {
        "large_file" => (20, Duration::from_secs(15)),
        "deep_expressions" => (30, Duration::from_secs(15)),
        _ => (60, Duration::from_secs(8)),
    }
}

// =============================================================================
// Benchmark 1: construct (build one fresh tree from scratch)
//
// iter_with_large_drop keeps teardown out of the timed region for every
// representation, so this group measures building only. Teardown is measured in
// the churn group.
// =============================================================================

fn bench_construct(c: &mut Criterion) {
    let mut group = c.benchmark_group("arena_vs_box_construct");

    for profile in PROFILES {
        let (sample_size, measurement_time) = tuning_for(profile);
        group.sample_size(sample_size);
        group.measurement_time(measurement_time);

        group.bench_with_input(BenchmarkId::new("owned", profile.name), profile, |b, p| {
            b.iter_with_large_drop(|| owned::build_program(*p));
        });

        group.bench_with_input(BenchmarkId::new("arena", profile.name), profile, |b, p| {
            b.iter_with_large_drop(|| {
                let a = arena::AstArena::with_capacity(p.arena_capacity);
                black_box(arena::build_program(&a, *p));
                a
            });
        });

        group.bench_with_input(BenchmarkId::new("index", profile.name), profile, |b, p| {
            b.iter_with_large_drop(|| {
                let mut a = index::Arena::with_hint(*p);
                index::build_program(&mut a, *p);
                a
            });
        });

        group.bench_with_input(BenchmarkId::new("index_noreserve", profile.name), profile, |b, p| {
            b.iter_with_large_drop(|| {
                let mut a = index::Arena::default();
                index::build_program(&mut a, *p);
                a
            });
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 2: clone / rebuild (the ast.clone() operation in
// general_ast_enhancer.rs::enhance()). The source tree is built once outside the
// timed region; only the copy itself is measured.
// =============================================================================

fn bench_clone_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("arena_vs_box_clone_rebuild");

    for profile in PROFILES {
        let (sample_size, measurement_time) = tuning_for(profile);
        group.sample_size(sample_size);
        group.measurement_time(measurement_time);

        let owned_prog = owned::build_program(*profile);
        group.bench_with_input(BenchmarkId::new("owned_clone", profile.name), profile, |b, _| {
            b.iter_with_large_drop(|| owned_prog.clone());
        });

        let src_arena = arena::AstArena::with_capacity(profile.arena_capacity);
        let src_prog = arena::build_program(&src_arena, *profile);
        group.bench_with_input(BenchmarkId::new("arena_rebuild", profile.name), profile, |b, p| {
            b.iter_with_large_drop(|| {
                let dst = arena::AstArena::with_capacity(p.arena_capacity);
                black_box(arena::rebuild_program(&dst, &src_prog, 0));
                dst
            });
        });

        let mut index_prog = index::Arena::with_hint(*profile);
        index::build_program(&mut index_prog, *profile);
        group.bench_with_input(BenchmarkId::new("index_clone", profile.name), profile, |b, _| {
            b.iter_with_large_drop(|| index_prog.clone());
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 3: clone and mutate (the enhancer's clone-then-edit step)
// =============================================================================

fn bench_clone_mutate(c: &mut Criterion) {
    let mut group = c.benchmark_group("arena_vs_box_clone_mutate");

    for profile in PROFILES {
        let (sample_size, measurement_time) = tuning_for(profile);
        group.sample_size(sample_size);
        group.measurement_time(measurement_time);

        let owned_prog = owned::build_program(*profile);
        group.bench_with_input(BenchmarkId::new("owned_clone_walk", profile.name), profile, |b, _| {
            b.iter_with_large_drop(|| {
                let mut copy = owned_prog.clone();
                owned::bump_values(&mut copy, MUTATE_DELTA);
                copy
            });
        });

        let src_arena = arena::AstArena::with_capacity(profile.arena_capacity);
        let src_prog = arena::build_program(&src_arena, *profile);
        group.bench_with_input(BenchmarkId::new("arena_rebuild_mutate", profile.name), profile, |b, p| {
            b.iter_with_large_drop(|| {
                let dst = arena::AstArena::with_capacity(p.arena_capacity);
                black_box(arena::rebuild_program(&dst, &src_prog, MUTATE_DELTA));
                dst
            });
        });

        let mut index_prog = index::Arena::with_hint(*profile);
        index::build_program(&mut index_prog, *profile);
        group.bench_with_input(BenchmarkId::new("index_clone_linear", profile.name), profile, |b, _| {
            b.iter_with_large_drop(|| {
                let mut copy = index_prog.clone();
                index::bump_values_linear(&mut copy, MUTATE_DELTA);
                copy
            });
        });
        group.bench_with_input(BenchmarkId::new("index_clone_walk", profile.name), profile, |b, _| {
            b.iter_with_large_drop(|| {
                let mut copy = index_prog.clone();
                index::bump_values_walk(&mut copy, MUTATE_DELTA);
                copy
            });
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 4: traverse (walk an already-built tree, no allocation)
// =============================================================================

fn bench_traverse(c: &mut Criterion) {
    let mut group = c.benchmark_group("arena_vs_box_traverse");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(8));

    for profile in PROFILES {
        let owned_prog = owned::build_program(*profile);
        group.throughput(Throughput::Elements(owned::count_program(&owned_prog)));
        group.bench_with_input(BenchmarkId::new("owned", profile.name), profile, |b, _| {
            b.iter(|| black_box(owned::count_program(&owned_prog)));
        });

        let src_arena = arena::AstArena::with_capacity(profile.arena_capacity);
        let src_prog = arena::build_program(&src_arena, *profile);
        group.bench_with_input(BenchmarkId::new("arena", profile.name), profile, |b, _| {
            b.iter(|| black_box(arena::count_program(&src_prog)));
        });

        let mut index_prog = index::Arena::with_hint(*profile);
        index::build_program(&mut index_prog, *profile);
        group.bench_with_input(BenchmarkId::new("index", profile.name), profile, |b, _| {
            b.iter(|| black_box(index::count_program(&index_prog)));
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 5: build and drop churn (mdix-lsp reparse-on-keystroke simulation)
//
// Uses iter_custom so the drop is timed inside the loop: it is the thing being
// measured (many small deallocations for owned, one chunk free for option A, a
// handful of Vec frees for option B).
// =============================================================================

fn bench_churn(c: &mut Criterion) {
    let mut group = c.benchmark_group("arena_vs_box_churn");

    for profile in PROFILES {
        let (sample_size, measurement_time) = tuning_for(profile);
        group.sample_size(sample_size);
        group.measurement_time(measurement_time);

        group.bench_with_input(BenchmarkId::new("owned", profile.name), profile, |b, p| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let start = Instant::now();
                    let prog = owned::build_program(*p);
                    black_box(&prog);
                    drop(prog);
                    total += start.elapsed();
                }
                total
            });
        });

        group.bench_with_input(BenchmarkId::new("arena", profile.name), profile, |b, p| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let start = Instant::now();
                    let a = arena::AstArena::with_capacity(p.arena_capacity);
                    let prog = arena::build_program(&a, *p);
                    black_box(&prog);
                    // `prog` is Copy (references into `a`), so nothing drops there.
                    // `a` holds the allocation; dropping it is the teardown cost.
                    drop(a);
                    total += start.elapsed();
                }
                total
            });
        });

        group.bench_with_input(BenchmarkId::new("index", profile.name), profile, |b, p| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let start = Instant::now();
                    let mut a = index::Arena::with_hint(*p);
                    index::build_program(&mut a, *p);
                    black_box(&a);
                    drop(a);
                    total += start.elapsed();
                }
                total
            });
        });

        group.bench_with_input(BenchmarkId::new("index_noreserve", profile.name), profile, |b, p| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let start = Instant::now();
                    let mut a = index::Arena::default();
                    index::build_program(&mut a, *p);
                    black_box(&a);
                    drop(a);
                    total += start.elapsed();
                }
                total
            });
        });
    }

    group.finish();
}

// =============================================================================
// Registration. `main` is written out instead of using criterion_main! so the
// equivalence checks run before any timing.
// =============================================================================

criterion_group!(
    benches,
    bench_construct,
    bench_clone_rebuild,
    bench_clone_mutate,
    bench_traverse,
    bench_churn
);

fn main() {
    arena_shapes::verify_representations();
    arena_shapes::report_layout();
    benches();
    Criterion::default().configure_from_args().final_summary();
}
