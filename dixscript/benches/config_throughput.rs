// benches/config_throughput.rs
//! Config Section Benchmark — updated for token-based pipeline (v1.0.0+)
//!
//! Loader pipeline order:
//!   Stage 1: Tokenizer::new(full_source).tokenize()
//!   Stage 2: split_config_tokens(tokens)  → config_tokens + rest_tokens
//!   Stage 3: ConfigSectionHandler::process_config_tokens(&config_tokens)
//!   Stage 4: GeneralParser::new(rest_tokens, ...)
//!
//! Groups:
//!   A. full_pipeline      — all three stages together (what callers pay)
//!   B. tokens_only        — stage 3 in isolation (pre-tokenised inputs)
//!   C. stage_breakdown    — per-stage incremental cost on the large input
//!   D. strategy_comparison— halt vs continue on clean input (stage 3 only)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use std::time::Duration;

// =============================================================================
// Test inputs
// =============================================================================

/// Full @CONFIG — exercises the complete parse path (8 keys, all recognised).
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

/// @CONFIG present but empty — exercises the empty-content fast-path.
const EMPTY_CONFIG: &str = "@CONFIG()\n@DATA(\n    x = 1\n)";

/// No @CONFIG keyword at all — exercises the zero-config_tokens path.
const NO_CONFIG: &str = "@DATA(\n    x = 1,\n    y = 2,\n    z = 3\n)";

/// Partial config (author + debug + compat only) — exercises default-injection
/// and validation-warning paths for missing required keys.
const PARTIAL_CONFIG: &str = r#"@CONFIG(
    author             -> "Test Author",
    debug_mode         -> "verbose",
    compatibility_mode -> "permissive"
)
@DATA(
    result = 42
)"#;

// =============================================================================
// Helpers
// =============================================================================

/// Run the full three-stage config pipeline on a raw source string.
/// Mirrors what DixLoader::compile_source does internally.
#[inline(always)]
fn full_config_pipeline(source: &str) {
    let initial = OperationalSettings::default();
    let tok     = Tokenizer::new(source, &initial).tokenize();
    let split   = split_config_tokens(tok.tokens);
    let mut handler = ConfigSectionHandler::new(None);
    let _ = handler.process_config_tokens(&split.config_tokens);
}

// =============================================================================
// Benchmark
// =============================================================================

fn benchmark_config_section(c: &mut Criterion) {
    let mut group = c.benchmark_group("config_section");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(100);

    let initial = OperationalSettings::default();

    // =========================================================================
    // Group A — full pipeline: tokenize + split + process_config_tokens
    // =========================================================================

    group.throughput(Throughput::Bytes(LARGE_CONFIG.len() as u64));
    group.bench_function("full_pipeline/large_all_keys", |b| {
        b.iter(|| full_config_pipeline(black_box(LARGE_CONFIG)));
    });

    group.throughput(Throughput::Bytes(EMPTY_CONFIG.len() as u64));
    group.bench_function("full_pipeline/empty_config_block", |b| {
        b.iter(|| full_config_pipeline(black_box(EMPTY_CONFIG)));
    });

    group.throughput(Throughput::Bytes(NO_CONFIG.len() as u64));
    group.bench_function("full_pipeline/no_config_section", |b| {
        b.iter(|| full_config_pipeline(black_box(NO_CONFIG)));
    });

    group.throughput(Throughput::Bytes(PARTIAL_CONFIG.len() as u64));
    group.bench_function("full_pipeline/partial_missing_required", |b| {
        b.iter(|| full_config_pipeline(black_box(PARTIAL_CONFIG)));
    });

    // =========================================================================
    // Group B — token-processing only (stage 3 isolated)
    // Pre-tokenise and split each input once outside the timed loop so only
    // ConfigSectionHandler::process_config_tokens is measured.
    // =========================================================================

    let large_config_tokens = {
        let t = Tokenizer::new(LARGE_CONFIG, &initial).tokenize();
        split_config_tokens(t.tokens).config_tokens
    };
    let empty_config_tokens = {
        let t = Tokenizer::new(EMPTY_CONFIG, &initial).tokenize();
        split_config_tokens(t.tokens).config_tokens
    };
    let no_config_tokens = {
        let t = Tokenizer::new(NO_CONFIG, &initial).tokenize();
        split_config_tokens(t.tokens).config_tokens
    };
    let partial_config_tokens = {
        let t = Tokenizer::new(PARTIAL_CONFIG, &initial).tokenize();
        split_config_tokens(t.tokens).config_tokens
    };

    group.throughput(Throughput::Bytes(LARGE_CONFIG.len() as u64));
    group.bench_function("tokens_only/large_all_keys", |b| {
        b.iter(|| {
            let mut h = ConfigSectionHandler::new(None);
            black_box(h.process_config_tokens(black_box(&large_config_tokens)))
        });
    });

    group.throughput(Throughput::Bytes(EMPTY_CONFIG.len() as u64));
    group.bench_function("tokens_only/empty_config_block", |b| {
        b.iter(|| {
            let mut h = ConfigSectionHandler::new(None);
            black_box(h.process_config_tokens(black_box(&empty_config_tokens)))
        });
    });

    group.throughput(Throughput::Bytes(NO_CONFIG.len() as u64));
    group.bench_function("tokens_only/no_config_section", |b| {
        b.iter(|| {
            let mut h = ConfigSectionHandler::new(None);
            black_box(h.process_config_tokens(black_box(&no_config_tokens)))
        });
    });

    group.throughput(Throughput::Bytes(PARTIAL_CONFIG.len() as u64));
    group.bench_function("tokens_only/partial_missing_required", |b| {
        b.iter(|| {
            let mut h = ConfigSectionHandler::new(None);
            black_box(h.process_config_tokens(black_box(&partial_config_tokens)))
        });
    });

    // =========================================================================
    // Group C — per-stage breakdown on the large input
    // Derive incremental costs:
    //   stage2_cost = tokenize_and_split   - tokenize_only
    //   stage3_cost = full_pipeline        - tokenize_and_split
    // =========================================================================

    group.throughput(Throughput::Bytes(LARGE_CONFIG.len() as u64));

    group.bench_function("stage_breakdown/1_tokenize_only", |b| {
        b.iter(|| {
            black_box(Tokenizer::new(black_box(LARGE_CONFIG), &initial).tokenize())
        });
    });

    group.bench_function("stage_breakdown/1_2_tokenize_and_split", |b| {
        b.iter(|| {
            let tok = Tokenizer::new(black_box(LARGE_CONFIG), &initial).tokenize();
            black_box(split_config_tokens(tok.tokens))
        });
    });

    group.bench_function("stage_breakdown/1_2_3_full_config_pipeline", |b| {
        b.iter(|| full_config_pipeline(black_box(LARGE_CONFIG)));
    });

    // =========================================================================
    // Group D — error-handling strategy comparison (stage 3 only, clean input)
    // Verifies that strategy selection adds negligible overhead on valid input.
    // =========================================================================

    let halt_src = format!(
        "@CONFIG(\n    version -> \"1.0.0\",\n    encoding -> \"utf-8\",\n    \
         error_handling -> \"halt\"\n)\n@DATA(x = 1)"
    );
    let continue_src = halt_src.replace("\"halt\"", "\"continue\"");

    let halt_config_tokens = {
        let t = Tokenizer::new(&halt_src, &initial).tokenize();
        split_config_tokens(t.tokens).config_tokens
    };
    let continue_config_tokens = {
        let t = Tokenizer::new(&continue_src, &initial).tokenize();
        split_config_tokens(t.tokens).config_tokens
    };

    group.bench_function("strategy_comparison/halt", |b| {
        b.iter(|| {
            let mut h = ConfigSectionHandler::new(None);
            black_box(h.process_config_tokens(black_box(&halt_config_tokens)))
        });
    });

    group.bench_function("strategy_comparison/continue", |b| {
        b.iter(|| {
            let mut h = ConfigSectionHandler::new(None);
            black_box(h.process_config_tokens(black_box(&continue_config_tokens)))
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_config_section);
criterion_main!(benches);
