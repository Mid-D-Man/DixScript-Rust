//! General tokenizer throughput benchmark.
//!
//! Uses inline-generated input so CI never needs fixture files on disk.

use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::Config::OperationalSettings;
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

fn generate_large_input(n: usize) -> String {
    let mut s = String::with_capacity(n * 22);
    s.push_str("@DATA(\n");
    for i in 0..n {
        s.push_str(&format!("  var{i} = {}\n", i * 2));
    }
    s.push_str(")\n");
    s
}

fn generate_mixed_input(n: usize) -> String {
    let mut s = String::with_capacity(n * 50);
    s.push_str("@CONFIG(\n  version -> \"1.0.0\"\n)\n@DATA(\n");
    for i in 0..n {
        match i % 5 {
            0 => s.push_str(&format!("  v{i} = {i}\n")),
            1 => s.push_str(&format!("  s{i} = \"text_{i}\"\n")),
            2 => s.push_str(&format!("  f{i} = {i}.5f\n")),
            3 => s.push_str(&format!("  b{i} = {}\n", i % 2 == 0)),
            _ => s.push_str(&format!("  // comment {i}\n  x{i} = {i}\n")),
        }
    }
    s.push_str(")\n");
    s
}

fn benchmark_lexer(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer_throughput");

    let settings = OperationalSettings::default();

    // Small file (~220 KB)
    let small = generate_large_input(10_000);
    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_function("large_synthetic_integers", |b| {
        b.iter(|| {
            let t = Tokenizer::new(black_box(&small), &settings);
            black_box(t.tokenize())
        });
    });

    // Mixed types (~500 KB)
    let mixed = generate_mixed_input(10_000);
    group.throughput(Throughput::Bytes(mixed.len() as u64));
    group.bench_function("mixed_types", |b| {
        b.iter(|| {
            let t = Tokenizer::new(black_box(&mixed), &settings);
            black_box(t.tokenize())
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_lexer);
criterion_main!(benches);
