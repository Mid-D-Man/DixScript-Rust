use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::Config::OperationalSettings;
use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use std::time::Duration;

fn comprehensive_lexer_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer_performance");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    // OperationalSettings is shared across all iterations — one allocation,
    // never measured. Tokenizer borrows it.
    let settings = OperationalSettings::default();

    // ------------------------------------------------------------------
    // Test 1: Real .mdix file
    // ------------------------------------------------------------------
    let real_file = std::fs::read_to_string("mdix_files/advanced/all_datatypes_test.mdix")
        .expect("Failed to read all_datatypes_test.mdix");

    group.throughput(Throughput::Bytes(real_file.len() as u64));
    group.bench_function("real_mdix_file", |b| {
        // input is borrowed inside the closure — no clone measured
        b.iter(|| {
            let tokenizer = Tokenizer::new(black_box(&real_file), &settings);
            black_box(tokenizer.tokenize())
        });
    });

    // ------------------------------------------------------------------
    // Test 2: Small synthetic (~10 KB)
    // ------------------------------------------------------------------
    let small = generate_synthetic_input(300);
    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_function("small_10kb", |b| {
        b.iter(|| {
            let tokenizer = Tokenizer::new(black_box(&small), &settings);
            black_box(tokenizer.tokenize())
        });
    });

    // ------------------------------------------------------------------
    // Test 3: Medium synthetic (~100 KB)
    // ------------------------------------------------------------------
    let medium = generate_synthetic_input(3000);
    group.throughput(Throughput::Bytes(medium.len() as u64));
    group.bench_function("medium_100kb", |b| {
        b.iter(|| {
            let tokenizer = Tokenizer::new(black_box(&medium), &settings);
            black_box(tokenizer.tokenize())
        });
    });

    // ------------------------------------------------------------------
    // Test 4: Large synthetic (~1 MB)
    // ------------------------------------------------------------------
    let large = generate_synthetic_input(30000);
    group.throughput(Throughput::Bytes(large.len() as u64));
    group.bench_function("large_1mb", |b| {
        b.iter(|| {
            let tokenizer = Tokenizer::new(black_box(&large), &settings);
            black_box(tokenizer.tokenize())
        });
    });

    // ------------------------------------------------------------------
    // Test 5: String-heavy workload
    // ------------------------------------------------------------------
    let string_heavy = generate_string_heavy_input(2000);
    group.throughput(Throughput::Bytes(string_heavy.len() as u64));
    group.bench_function("string_heavy", |b| {
        b.iter(|| {
            let tokenizer = Tokenizer::new(black_box(&string_heavy), &settings);
            black_box(tokenizer.tokenize())
        });
    });

    // ------------------------------------------------------------------
    // Test 6: Comment-heavy workload
    // ------------------------------------------------------------------
    let comment_heavy = generate_comment_heavy_input(2000);
    group.throughput(Throughput::Bytes(comment_heavy.len() as u64));
    group.bench_function("comment_heavy", |b| {
        b.iter(|| {
            let tokenizer = Tokenizer::new(black_box(&comment_heavy), &settings);
            black_box(tokenizer.tokenize())
        });
    });

    // ------------------------------------------------------------------
    // Test 7: Error-handling strategy comparison
    //
    // Measures whether Continue vs Halt changes hot-path cost.
    // Should be nearly identical on clean input; diverge on error-heavy files.
    // ------------------------------------------------------------------
    use dixscript::Compiler::Core::Config::operational_settings::ErrorHandlingStrategy;

    let mut halt_settings     = OperationalSettings::default();
    let mut continue_settings = OperationalSettings::default();
    halt_settings.error_handling_strategy     = ErrorHandlingStrategy::Halt;
    continue_settings.error_handling_strategy = ErrorHandlingStrategy::Continue;

    group.throughput(Throughput::Bytes(medium.len() as u64));
    for (label, stg) in &[("halt", &halt_settings), ("continue", &continue_settings)] {
        group.bench_with_input(
            BenchmarkId::new("strategy_comparison", label),
            stg,
            |b, s| {
                b.iter(|| {
                    let tokenizer = Tokenizer::new(black_box(&medium), s);
                    black_box(tokenizer.tokenize())
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Input generators
// =============================================================================

fn generate_synthetic_input(statements: usize) -> String {
    let mut input = String::with_capacity(statements * 40);
    input.push_str("@CONFIG(\n    version -> \"1.0.0\",\n    features -> \"advanced\"\n)\n\n");
    input.push_str("@DATA(\n");
    for i in 0..statements {
        match i % 10 {
            0 => input.push_str(&format!("    var_{} = {},\n",              i, i)),
            1 => input.push_str(&format!("    str_{} = \"value_{}\",\n",    i, i)),
            2 => input.push_str(&format!("    float_{} = {}.5f,\n",         i, i)),
            3 => input.push_str(&format!("    bool_{} = {},\n",              i, i % 2 == 0)),
            4 => input.push_str(&format!("    arr_{} = [{}, {}, {}],\n",    i, i, i+1, i+2)),
            5 => input.push_str(&format!("    date_{} = 2025-01-{:02},\n",  i, (i % 28) + 1)),
            6 => input.push_str(&format!("    hex_{} = #FF{:04X},\n",       i, i % 0xFFFF)),
            7 => input.push_str(&format!("    calc_{} = {} + {} * {},\n",   i, i, i+1, i+2)),
            8 => input.push_str(&format!("    obj_{} = {{ id = {}, name = \"item_{}\" }},\n", i, i, i)),
            _ => input.push_str(&format!("    ts_{} = 2025-01-15T{:02}:30:00Z,\n", i, i % 24)),
        }
    }
    input.push_str(")\n");
    input
}

fn generate_string_heavy_input(count: usize) -> String {
    let mut input = String::with_capacity(count * 80);
    input.push_str("@DATA(\n");
    for i in 0..count {
        input.push_str(&format!(
            "    s{} = \"This is a longer string with escape sequences\\n\\t and special chars: {}\",\n",
            i, i
        ));
    }
    input.push_str(")\n");
    input
}

fn generate_comment_heavy_input(count: usize) -> String {
    let mut input = String::with_capacity(count * 60);
    input.push_str("@DATA(\n");
    for i in 0..count {
        input.push_str(&format!("    // This is comment number {}\n", i));
        input.push_str(&format!("    var_{} = {},\n", i, i));
        if i % 10 == 0 {
            input.push_str(&format!(
                "    /* Multi-line comment {}\n       with multiple lines */\n",
                i
            ));
        }
    }
    input.push_str(")\n");
    input
}

criterion_group!(benches, comprehensive_lexer_benchmark);
criterion_main!(benches);
