// benches/stress_test_benchmark.rs
//! Stress Test Benchmark — DixScript compiler limits
//!
//! Generates large .mdix inputs entirely in memory and measures how well the
//! compiler pipeline scales. Intended to find performance cliffs, O(n²)
//! regressions, and practical throughput ceilings under CI constraints.
//!
//! Generated input sizes:
//!   ~1 MB  — 28 000 DATA entries  (mixed types)
//!   ~5 MB  — 140 000 DATA entries
//!   ~10 MB — 280 000 DATA entries
//!
//! Func-call stress (separate axis — tests call graph resolution at scale):
//!   500 / 1 000 / 2 000 function calls in DATA
//!
//! Groups:
//!   stress_tokenize      — stage 1 only  (raw lexer throughput)
//!   stress_tokenize_split— stages 1-2    (add split_config_tokens cost)
//!   stress_parse         — stages 1-4    (full parse, no semantic)
//!   stress_func_calls    — func-call-heavy inputs at increasing call counts
//!
//! Run:
//!   cargo bench -p dixscript --bench stress_test_benchmark

use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use dixscript::Compiler::Core::{
    Config::{ConfigSectionHandler, OperationalSettings},
    GeneralParser,
    Tokenizer::{split_config_tokens, Tokenizer},
};
use std::sync::OnceLock;
use std::time::Duration;

// =============================================================================
// Input generators
// =============================================================================

/// Generate a DATA-only source with `n` incrementing entries.
/// Cycles through 6 data types to exercise different token paths:
///   0: int    — data_entry_N = N
///   1: string — str_entry_N  = "value_N"
///   2: bool   — bool_entry_N = true/false
///   3: float  — float_entry_N = N.5f
///   4: object — obj_entry_N  = { id = N, name = "item_N" }
///   5: array  — arr_entry_N  = [N, N+1, N+2]
fn generate_data_entries(n: usize) -> String {
    let header = concat!(
        "@CONFIG(\n",
        "    version        -> \"1.0.0\",\n",
        "    features       -> \"advanced\",\n",
        "    error_handling -> \"halt\"\n",
        ")\n",
        "@DATA(\n"
    );
    let footer = ")\n";

    // Pre-allocate: average ~35 bytes per entry + header/footer
    let mut s = String::with_capacity(n * 36 + header.len() + footer.len());
    s.push_str(header);

    for i in 0..n {
        match i % 6 {
            0 => s.push_str(&format!("    data_entry_{i} = {i},\n")),
            1 => s.push_str(&format!("    str_entry_{i} = \"value_{i}\",\n")),
            2 => s.push_str(&format!("    bool_entry_{i} = {},\n", i % 2 == 0)),
            3 => s.push_str(&format!("    float_entry_{i} = {i}.5f,\n")),
            4 => s.push_str(&format!(
                "    obj_entry_{i} = {{ id = {i}, name = \"item_{i}\" }},\n"
            )),
            _ => s.push_str(&format!(
                "    arr_entry_{i} = [{i}, {}, {}],\n",
                i + 1,
                i + 2
            )),
        }
    }

    // Remove trailing comma so the last entry is valid
    if s.ends_with(",\n") {
        s.truncate(s.len() - 2);
        s.push('\n');
    }

    s.push_str(footer);
    s
}

/// Generate a function-call-heavy source with `n_calls` DATA entries,
/// each calling one of 5 QuickFuncs. Tests the call graph at scale.
fn generate_func_stress(n_calls: usize) -> String {
    let prelude = concat!(
        "@CONFIG(\n",
        "    version        -> \"1.0.0\",\n",
        "    features       -> \"advanced\",\n",
        "    error_handling -> \"halt\"\n",
        ")\n",
        "@QUICKFUNCS(\n",
        "    ~double<int>(x<int>) {\n",
        "        return x * 2\n",
        "    }\n",
        "    ~triple<int>(x<int>) {\n",
        "        return x * 3\n",
        "    }\n",
        "    ~add<int>(a<int>, b<int>) {\n",
        "        return a + b\n",
        "    }\n",
        "    ~makeLabel<string>(name<string>, n<int>) {\n",
        "        return $\"{name}_{n}\"\n",
        "    }\n",
        "    ~buildObj<object>(id<int>, val<int>) {\n",
        "        return { id = id, value = val, doubled = double(val) }\n",
        "    }\n",
        ")\n",
        "@DATA(\n",
        "    calls::"
    );

    let mut s = String::with_capacity(n_calls * 32 + prelude.len() + 4);
    s.push_str(prelude);

    for i in 0..n_calls {
        match i % 5 {
            0 => s.push_str(&format!("\n        double({i}),")),
            1 => s.push_str(&format!("\n        triple({i}),")),
            2 => s.push_str(&format!("\n        add({i}, {}),", i + 1)),
            3 => s.push_str(&format!("\n        makeLabel(\"item\", {i}),")),
            _ => s.push_str(&format!("\n        buildObj({i}, {}),", i * 2)),
        }
    }

    if s.ends_with(',') { s.pop(); }
    s.push_str("\n)\n");
    s
}

// =============================================================================
// Cached inputs — generated once, reused across all benchmark runs
// =============================================================================

static INPUT_1MB:  OnceLock<String> = OnceLock::new();
static INPUT_5MB:  OnceLock<String> = OnceLock::new();
static INPUT_10MB: OnceLock<String> = OnceLock::new();

static FUNCS_500:  OnceLock<String> = OnceLock::new();
static FUNCS_1000: OnceLock<String> = OnceLock::new();
static FUNCS_2000: OnceLock<String> = OnceLock::new();

fn input_1mb()    -> &'static str { INPUT_1MB.get_or_init(|| generate_data_entries(28_000)) }
fn input_5mb()    -> &'static str { INPUT_5MB.get_or_init(|| generate_data_entries(140_000)) }
fn input_10mb()   -> &'static str { INPUT_10MB.get_or_init(|| generate_data_entries(280_000)) }

fn funcs_500()    -> &'static str { FUNCS_500.get_or_init(|| generate_func_stress(500)) }
fn funcs_1000()   -> &'static str { FUNCS_1000.get_or_init(|| generate_func_stress(1_000)) }
fn funcs_2000()   -> &'static str { FUNCS_2000.get_or_init(|| generate_func_stress(2_000)) }

// Print a one-time size report before measurements start
fn print_size_report() {
    // Trigger generation so sizes are accurate in the report
    let s1  = input_1mb().len();
    let s5  = input_5mb().len();
    let s10 = input_10mb().len();
    let f5  = funcs_500().len();
    let f10 = funcs_1000().len();
    let f20 = funcs_2000().len();

    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║           DixScript Stress Test — Generated Input Sizes          ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║  Data-entry inputs                                               ║");
    println!("║    ~1 MB  :  {:>10} bytes  ({:>7.1} KB)                      ║", s1,  s1 as f64 / 1024.0);
    println!("║    ~5 MB  :  {:>10} bytes  ({:>7.1} KB)                      ║", s5,  s5 as f64 / 1024.0);
    println!("║    ~10 MB :  {:>10} bytes  ({:>7.1} KB)                      ║", s10, s10 as f64 / 1024.0);
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║  Function-call-heavy inputs                                      ║");
    println!("║     500 calls :  {:>8} bytes                                   ║", f5);
    println!("║    1000 calls :  {:>8} bytes                                   ║", f10);
    println!("║    2000 calls :  {:>8} bytes                                   ║", f20);
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
}

// =============================================================================
// Pipeline helpers
// =============================================================================

#[inline]
fn run_tokenize(source: &str, initial: &OperationalSettings) -> usize {
    Tokenizer::new(source, initial).tokenize().tokens.len()
}

#[inline]
fn run_tokenize_and_split(source: &str, initial: &OperationalSettings) {
    let tok   = Tokenizer::new(source, initial).tokenize();
    let split = split_config_tokens(tok.tokens);
    let _     = (split.config_tokens.len(), split.rest_tokens.len());
}

#[inline]
fn run_full_parse(source: &str) {
    let initial    = OperationalSettings::default();
    let tok_result = Tokenizer::new(source, &initial).tokenize();
    let split      = split_config_tokens(tok_result.tokens);
    let mut handler = ConfigSectionHandler::new(None);
    let cfg        = handler.process_config_tokens(&split.config_tokens);
    let settings   = cfg.operational_settings.clone();
    let parser     = GeneralParser::new(split.rest_tokens, &cfg.config_section, &settings)
        .expect("parser init");
    let _ = parser.parse().expect("parse failed");
}

// =============================================================================
// Benchmark 1 — tokenize only (raw lexer throughput)
// =============================================================================

fn bench_stress_tokenize(c: &mut Criterion) {
    print_size_report();

    let mut group = c.benchmark_group("stress_tokenize");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(30);

    let initial = OperationalSettings::default();

    for (label, src) in &[
        ("1mb",  input_1mb()),
        ("5mb",  input_5mb()),
        ("10mb", input_10mb()),
    ] {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("tokenize_only", label),
            *src,
            |b, s| b.iter(|| black_box(run_tokenize(black_box(s), &initial))),
        );
    }

    group.finish();
}

// =============================================================================
// Benchmark 2 — tokenize + split (adds split_config_tokens cost)
// =============================================================================

fn bench_stress_tokenize_split(c: &mut Criterion) {
    let mut group = c.benchmark_group("stress_tokenize_split");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(30);

    let initial = OperationalSettings::default();

    for (label, src) in &[
        ("1mb",  input_1mb()),
        ("5mb",  input_5mb()),
        ("10mb", input_10mb()),
    ] {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("tokenize_and_split", label),
            *src,
            |b, s| b.iter(|| run_tokenize_and_split(black_box(s), &initial)),
        );
    }

    group.finish();
}

// =============================================================================
// Benchmark 3 — full parse (stages 1-4, no semantic)
//
// sample_size lowered progressively for larger inputs since each iteration
// takes hundreds of milliseconds at 10 MB.
// =============================================================================

fn bench_stress_parse(c: &mut Criterion) {
    // ── 1 MB ─────────────────────────────────────────────────────────────────
    {
        let src = input_1mb();
        let mut group = c.benchmark_group("stress_parse");
        group.measurement_time(Duration::from_secs(15));
        group.sample_size(20);
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_function("full_parse_1mb", |b| {
            b.iter(|| run_full_parse(black_box(src)))
        });
        group.finish();
    }

    // ── 5 MB ─────────────────────────────────────────────────────────────────
    {
        let src = input_5mb();
        let mut group = c.benchmark_group("stress_parse");
        group.measurement_time(Duration::from_secs(30));
        group.sample_size(10);
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_function("full_parse_5mb", |b| {
            b.iter(|| run_full_parse(black_box(src)))
        });
        group.finish();
    }

    // ── 10 MB ────────────────────────────────────────────────────────────────
    {
        let src = input_10mb();
        let mut group = c.benchmark_group("stress_parse");
        group.measurement_time(Duration::from_secs(60));
        group.sample_size(10);
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_function("full_parse_10mb", |b| {
            b.iter(|| run_full_parse(black_box(src)))
        });
        group.finish();
    }
}

// =============================================================================
// Benchmark 4 — function-call-heavy stress
//
// Holds file size roughly constant while scaling call count.
// Tests the parser's call-graph handling and QuickFuncs section parsing.
// =============================================================================

fn bench_stress_func_calls(c: &mut Criterion) {
    let mut group = c.benchmark_group("stress_func_calls");
    group.measurement_time(Duration::from_secs(15));
    group.sample_size(20);

    for (label, src) in &[
        ("500_calls",  funcs_500()),
        ("1000_calls", funcs_1000()),
        ("2000_calls", funcs_2000()),
    ] {
        group.throughput(Throughput::Bytes(src.len() as u64));

        // Stage 1 only
        group.bench_with_input(
            BenchmarkId::new("tokenize_only", label),
            *src,
            |b, s| {
                let initial = OperationalSettings::default();
                b.iter(|| black_box(run_tokenize(black_box(s), &initial)))
            },
        );

        // Stages 1-4
        group.bench_with_input(
            BenchmarkId::new("full_parse", label),
            *src,
            |b, s| b.iter(|| run_full_parse(black_box(s))),
        );
    }

    group.finish();
}

// =============================================================================
// Registration
// =============================================================================

criterion_group!(
    benches,
    bench_stress_tokenize,
    bench_stress_tokenize_split,
    bench_stress_parse,
    bench_stress_func_calls,
);
criterion_main!(benches);
