use dixscript::Compiler::Core::Config::ConfigSectionHandler;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

// =============================================================================
// Test inputs
// =============================================================================

/// Full @CONFIG with every recognised key — exercises the complete parse path.
const LARGE_CONFIG: &str = r#"@CONFIG(
    version            -> "1.0.0",
    encoding           -> "utf-8",
    author             -> "MidManStudio",
    created            -> "2025-01-15T10:30:00Z",
    features           -> "advanced",
    debug_mode         -> "off",
    error_handling     -> "halt",
    compatibility_mode -> "strict"
)
@DATA(
    name = "benchmark"
)"#;

/// @CONFIG present but empty — exercises the "empty content" fast-path.
const EMPTY_CONFIG: &str = "@CONFIG()\n@DATA(\n    x = 1\n)";

/// No @CONFIG keyword at all — exercises the "no section found" early return.
const NO_CONFIG: &str = "@DATA(\n    x = 1,\n    y = 2,\n    z = 3\n)";

/// @CONFIG with required keys (version, encoding) absent — exercises
/// default-injection and validation-warning paths.
const PARTIAL_CONFIG: &str = r#"@CONFIG(
    author             -> "Test Author",
    debug_mode         -> "verbose",
    compatibility_mode -> "permissive"
)
@DATA(
    result = 42
)"#;

// =============================================================================
// Benchmark
// =============================================================================

fn benchmark_config_section(c: &mut Criterion) {
    let mut group = c.benchmark_group("config_section");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(100);

    // ------------------------------------------------------------------
    // 1. Large (all keys present) — nominal happy path
    // ------------------------------------------------------------------
    group.throughput(Throughput::Bytes(LARGE_CONFIG.len() as u64));
    group.bench_function("large_all_keys", |b| {
        b.iter(|| {
            let mut handler = ConfigSectionHandler::new(None);
            black_box(handler.process_config_section(black_box(LARGE_CONFIG)))
        });
    });

    // ------------------------------------------------------------------
    // 2. Empty @CONFIG() — exercises empty-content fast-path
    // ------------------------------------------------------------------
    group.throughput(Throughput::Bytes(EMPTY_CONFIG.len() as u64));
    group.bench_function("empty_config_block", |b| {
        b.iter(|| {
            let mut handler = ConfigSectionHandler::new(None);
            black_box(handler.process_config_section(black_box(EMPTY_CONFIG)))
        });
    });

    // ------------------------------------------------------------------
    // 3. No @CONFIG at all — exercises the early-return path
    // ------------------------------------------------------------------
    group.throughput(Throughput::Bytes(NO_CONFIG.len() as u64));
    group.bench_function("no_config_section", |b| {
        b.iter(|| {
            let mut handler = ConfigSectionHandler::new(None);
            black_box(handler.process_config_section(black_box(NO_CONFIG)))
        });
    });

    // ------------------------------------------------------------------
    // 4. Partial config (missing required keys) — exercises validation
    //    and default-injection paths
    // ------------------------------------------------------------------
    group.throughput(Throughput::Bytes(PARTIAL_CONFIG.len() as u64));
    group.bench_function("partial_missing_required", |b| {
        b.iter(|| {
            let mut handler = ConfigSectionHandler::new(None);
            black_box(handler.process_config_section(black_box(PARTIAL_CONFIG)))
        });
    });

    // ------------------------------------------------------------------
    // 5. Error-handling strategy comparison on the large input.
    //    Verifies that strategy selection doesn't add measurable overhead
    //    on clean input.
    // ------------------------------------------------------------------
    let halt_input = format!(
        "@CONFIG(\n    version -> \"1.0.0\",\n    encoding -> \"utf-8\",\n    error_handling -> \"halt\"\n)\n@DATA(x = 1)"
    );
    let continue_input = halt_input.replace("\"halt\"", "\"continue\"");

    group.throughput(Throughput::Bytes(halt_input.len() as u64));
    for (label, input) in &[
        ("halt_strategy",     halt_input.as_str()),
        ("continue_strategy", continue_input.as_str()),
    ] {
        group.bench_with_input(
            BenchmarkId::new("strategy_comparison", label),
            input,
            |b, s| {
                b.iter(|| {
                    let mut handler = ConfigSectionHandler::new(None);
                    black_box(handler.process_config_section(black_box(s)))
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_config_section);
criterion_main!(benches);
