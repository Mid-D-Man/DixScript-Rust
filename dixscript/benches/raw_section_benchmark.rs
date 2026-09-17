// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md (referenced from the Compiler modules it
// benchmarks)
// ============================================================================
//! Benchmarks for the `@RAW` section: lexer-level content-block scanning
//! throughput across payload sizes, and full-pipeline (tokenize -> parse ->
//! semantic analysis) overhead as the number of `@RAW` blocks in a file
//! grows -- the analyzer's cross-block uniqueness checks are the one part
//! of this feature whose cost scales with block count rather than payload
//! size, so that's tracked separately from raw scanning throughput.
//!
//! All input is generated inline -- no filesystem reads -- so this
//! benchmark works correctly in CI without any fixture files present.

use dixscript::Compiler::Core::Config::OperationalSettings;
use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Runtime::DixLoader;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

// ── Input generators ──────────────────────────────────────────────────────────

fn generate_raw_block(id_suffix: usize, payload_bytes: usize) -> String {
    let payload: String = (0..payload_bytes).map(|i| (b'a' + (i % 26) as u8) as char).collect();
    format!(
        "@RAW(\n  meta_data -> {{ id = \"asset_{id_suffix}\", format = \"BIN\", size = {payload_bytes} }}\n  using -> {{ compression = \"none\" }}\n  content -> {{ ---tag_{id_suffix}--- {payload} ---tag_{id_suffix}--- }}\n)\n"
    )
}

fn generate_single_raw_input(payload_bytes: usize) -> String {
    generate_raw_block(0, payload_bytes)
}

fn generate_multi_raw_input(num_blocks: usize, payload_bytes_each: usize) -> String {
    let mut input = String::with_capacity(num_blocks * (payload_bytes_each + 150));
    for i in 0..num_blocks {
        input.push_str(&generate_raw_block(i, payload_bytes_each));
    }
    input
}

// ── Lexer-level: content-block scanning throughput ──────────────────────────

fn raw_lexer_scan_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("raw_section_lexer_scan");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));
    let settings = OperationalSettings::default();

    // Pure memchr-based scanning (see scan_raw_content_block), no
    // parsing/analysis -- unlikely to be the actual cost driver, but cut
    // proactively alongside the pipeline group rather than leave an
    // unverified assumption at 1MB.
    for &payload_size in &[256usize, 4_096, 65_536] {
        let input = generate_single_raw_input(payload_size);
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("payload_bytes", payload_size),
            &input,
            |b, input| {
                b.iter(|| {
                    let t = Tokenizer::new(black_box(input), &settings);
                    black_box(t.tokenize())
                });
            },
        );
    }

    group.finish();
}

// ── Full pipeline: tokenize -> parse -> semantic analysis ───────────────────
//
// Tracks how `raw_section_analyzer.rs`'s cross-block id/tag uniqueness
// checks (FxHashMap-based, see that file) actually scale as block count
// grows -- separate concern from raw scanning throughput above, which
// only ever looks at one block.

fn raw_full_pipeline_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("raw_section_full_pipeline");
    // sample_size(10) is criterion's documented floor. measurement_time
    // is short and flat across all three tiers on purpose: two earlier
    // attempts at "reasonable-sounding" larger tiers with progressive
    // scaling (1,000 blocks / flat sample_size(30); then 150 blocks /
    // tiered) both turned out to run far longer in real CI than expected,
    // without ever being empirically measured first. This trades
    // statistical richness for a bound that can't blow up regardless of
    // per-iteration cost -- widen it later once a run actually confirms
    // real per-iteration timing.
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    for &num_blocks in &[5usize, 15, 40] {
        let input = generate_multi_raw_input(num_blocks, 64);
        group.throughput(Throughput::Elements(num_blocks as u64));
        group.bench_with_input(
            BenchmarkId::new("blocks", num_blocks),
            &input,
            |b, input| {
                b.iter(|| {
                    let loader = DixLoader::new();
                    black_box(
                        loader
                            .compile_to_resolved_ast_from_str(black_box(input), "raw_bench")
                            .expect("generated input should always compile"),
                    )
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, raw_lexer_scan_benchmark, raw_full_pipeline_benchmark);
criterion_main!(benches);
