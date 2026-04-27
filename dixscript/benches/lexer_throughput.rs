//! Lexer throughput benchmarks.
//!
//! All input is generated inline — no filesystem reads — so this benchmark
//! works correctly in CI without any fixture files present.

use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::Config::OperationalSettings;
use dixscript::Compiler::Core::Config::operational_settings::ErrorHandlingStrategy;
use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use std::time::Duration;

// ── Input generators ──────────────────────────────────────────────────────────

fn generate_synthetic_input(statements: usize) -> String {
    let mut input = String::with_capacity(statements * 50);
    input.push_str("@CONFIG(\n  version -> \"1.0.0\"\n  features -> \"advanced\"\n)\n\n");
    input.push_str("@ENUMS(\n  AIType { PASSIVE = 0, NEUTRAL = 1, AGGRESSIVE = 2, BOSS = 3 }\n  LogLevel { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }\n)\n\n");
    input.push_str("@QUICKFUNCS(\n  ~enemy<object>(name, health<int>, damage<int>) {\n    return { name = name, health = health, damage = damage, armor = health / 10 }\n  }\n)\n\n");
    input.push_str("@DATA(\n");
    for i in 0..statements {
        match i % 12 {
            0  => input.push_str(&format!("  var_{i} = {i}\n")),
            1  => input.push_str(&format!("  str_{i} = \"value_{i}\"\n")),
            2  => input.push_str(&format!("  flt_{i} = {i}.5f\n")),
            3  => input.push_str(&format!("  dbl_{i} = {i}.14159\n")),
            4  => input.push_str(&format!("  bool_{i} = {}\n", i % 2 == 0)),
            5  => input.push_str(&format!("  arr_{i}:: {i}, {}, {}\n", i+1, i+2)),
            6  => input.push_str(&format!("  date_{i} = 2025-01-{:02}\n", (i % 28) + 1)),
            7  => input.push_str(&format!("  hex_{i} = #FF{:04X}\n", i % 0xFFFF)),
            8  => input.push_str(&format!("  ts_{i} = 2025-01-15T{:02}:30:00Z\n", i % 24)),
            9  => input.push_str(&format!("  obj_{i} = {{ id = {i}, name = \"item_{i}\" }}\n")),
            10 => input.push_str(&format!("  // comment line {i}\n  val_{i} = {i}\n")),
            _  => input.push_str(&format!("  calc_{i} = {i} + {} * {}\n", i+1, i+2)),
        }
    }
    input.push_str(")\n");
    input
}

fn generate_string_heavy_input(count: usize) -> String {
    let mut input = String::with_capacity(count * 90);
    input.push_str("@DATA(\n");
    for i in 0..count {
        input.push_str(&format!(
            "  s{i} = \"String with escape sequences\\n\\t and special chars: {i}\"\n"
        ));
    }
    input.push_str(")\n");
    input
}

fn generate_comment_heavy_input(count: usize) -> String {
    let mut input = String::with_capacity(count * 70);
    input.push_str("@DATA(\n");
    for i in 0..count {
        input.push_str(&format!("  // single-line comment {i}\n"));
        input.push_str(&format!("  var_{i} = {i}\n"));
        if i % 10 == 0 {
            input.push_str(&format!(
                "  /* multi-line comment {i}\n     spanning two lines */\n"
            ));
        }
    }
    input.push_str(")\n");
    input
}

fn generate_quickfuncs_heavy_input(count: usize) -> String {
    let mut input = String::with_capacity(count * 60);
    input.push_str("@QUICKFUNCS(\n");
    for i in 0..count {
        input.push_str(&format!(
            "  ~func{i}<object>(a<int>, b<int>, c<bool>) {{\n    if: c {{\n      return {{ x = a + b, y = a * b }}\n    }} else {{\n      return {{ x = a - b, y = a / b }}\n    }}\n  }}\n"
        ));
    }
    input.push_str(")\n@DATA(\n  x = 1\n)\n");
    input
}

fn generate_whitespace_heavy_input(count: usize) -> String {
    // Tests the SIMD whitespace-skip path — lots of blank lines between entries.
    let mut input = String::with_capacity(count * 30);
    input.push_str("@DATA(\n");
    for i in 0..count {
        input.push_str(&format!("\n\n  var_{i} = {i}\n\n"));
    }
    input.push_str(")\n");
    input
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

fn comprehensive_lexer_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer_performance");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    let settings = OperationalSettings::default();

    // Small (~10 KB)
    let small = generate_synthetic_input(300);
    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_function("small_10kb", |b| {
        b.iter(|| {
            let t = Tokenizer::new(black_box(&small), &settings);
            black_box(t.tokenize())
        });
    });

    // Medium (~100 KB)
    let medium = generate_synthetic_input(3_000);
    group.throughput(Throughput::Bytes(medium.len() as u64));
    group.bench_function("medium_100kb", |b| {
        b.iter(|| {
            let t = Tokenizer::new(black_box(&medium), &settings);
            black_box(t.tokenize())
        });
    });

    // Large (~1 MB)
    let large = generate_synthetic_input(30_000);
    group.throughput(Throughput::Bytes(large.len() as u64));
    group.bench_function("large_1mb", |b| {
        b.iter(|| {
            let t = Tokenizer::new(black_box(&large), &settings);
            black_box(t.tokenize())
        });
    });

    // String-heavy workload — exercises memchr string scanning.
    let string_heavy = generate_string_heavy_input(2_000);
    group.throughput(Throughput::Bytes(string_heavy.len() as u64));
    group.bench_function("string_heavy", |b| {
        b.iter(|| {
            let t = Tokenizer::new(black_box(&string_heavy), &settings);
            black_box(t.tokenize())
        });
    });

    // Comment-heavy workload.
    let comment_heavy = generate_comment_heavy_input(2_000);
    group.throughput(Throughput::Bytes(comment_heavy.len() as u64));
    group.bench_function("comment_heavy", |b| {
        b.iter(|| {
            let t = Tokenizer::new(black_box(&comment_heavy), &settings);
            black_box(t.tokenize())
        });
    });

    // QuickFuncs-heavy — lots of expression tokens.
    let qf_heavy = generate_quickfuncs_heavy_input(200);
    group.throughput(Throughput::Bytes(qf_heavy.len() as u64));
    group.bench_function("quickfuncs_heavy", |b| {
        b.iter(|| {
            let t = Tokenizer::new(black_box(&qf_heavy), &settings);
            black_box(t.tokenize())
        });
    });

    // Whitespace-heavy — exercises the SIMD skip_whitespace path directly.
    let ws_heavy = generate_whitespace_heavy_input(2_000);
    group.throughput(Throughput::Bytes(ws_heavy.len() as u64));
    group.bench_function("whitespace_heavy_simd", |b| {
        b.iter(|| {
            let t = Tokenizer::new(black_box(&ws_heavy), &settings);
            black_box(t.tokenize())
        });
    });

    // Error-handling strategy comparison — Halt vs Continue on clean input.
    // Should be nearly identical; divergence would indicate a hot-path cost.
    let mut halt_s     = OperationalSettings::default();
    let mut continue_s = OperationalSettings::default();
    halt_s.error_handling_strategy     = ErrorHandlingStrategy::Halt;
    continue_s.error_handling_strategy = ErrorHandlingStrategy::Continue;

    group.throughput(Throughput::Bytes(medium.len() as u64));
    for (label, stg) in &[
        ("strategy_halt",     &halt_s),
        ("strategy_continue", &continue_s),
    ] {
        group.bench_with_input(
            BenchmarkId::new("error_strategy", label),
            stg,
            |b, s| {
                b.iter(|| {
                    let t = Tokenizer::new(black_box(&medium), s);
                    black_box(t.tokenize())
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, comprehensive_lexer_benchmark);
criterion_main!(benches);
