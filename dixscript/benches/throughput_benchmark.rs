use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::Config::OperationalSettings;
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

fn benchmark_lexer(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer_throughput");

    // Shared settings — constructed once, borrowed per iteration.
    let settings = OperationalSettings::default();

    // ------------------------------------------------------------------
    // Real file
    // ------------------------------------------------------------------
    let small = std::fs::read_to_string("../../mdix_files/advanced/all_datatypes_test.dixscript")
        .expect("Failed to read file");

    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_function("small_file", |b| {
        b.iter(|| {
            let tokenizer = Tokenizer::new(black_box(&small), &settings);
            black_box(tokenizer.tokenize())
        });
    });

    // ------------------------------------------------------------------
    // Large synthetic
    // ------------------------------------------------------------------
    let mut large = String::with_capacity(10000 * 22);
    large.push_str("@DATA(\n");
    for i in 0..10000 {
        large.push_str(&format!("    var{} = {},\n", i, i * 2));
    }
    large.push_str(")");

    group.throughput(Throughput::Bytes(large.len() as u64));
    group.bench_function("large_synthetic", |b| {
        b.iter(|| {
            let tokenizer = Tokenizer::new(black_box(&large), &settings);
            black_box(tokenizer.tokenize())
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_lexer);
criterion_main!(benches);
