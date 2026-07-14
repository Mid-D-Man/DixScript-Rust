//! Semantic Analysis Benchmark — DixScript (token-based pipeline)
//!
//! Pipeline order:
//!   Stage 1: Tokenizer::new(source).tokenize()
//!   Stage 2: split_config_tokens(tokens)
//!   Stage 3: ConfigSectionHandler::process_config_tokens(&config_tokens)
//!   Stage 4: GeneralParser::new(rest_tokens, ...).parse()
//!   Stage 5: GeneralSemanticAnalyzer::new(&ast, &settings).analyze()  ← measured here
//!
//! Groups:
//!   sem_analysis   — stage 5 in isolation (pre-parsed ASTs, clone in setup)
//!   sem_pipeline   — stages 1-5 end-to-end, with stage 4 baseline for delta

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::GeneralParser;
use dixscript::Compiler::Core::GeneralSemanticAnalyzer;
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::AST::DixScript;
use std::time::Duration;

// =============================================================================
// Test inputs — chosen to exercise different semantic analysis paths
// =============================================================================

/// Pure DATA, no function calls or enum references.
/// Minimal semantic work: just type-checks and symbol registration.
const SIMPLE_DATA: &str = r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    error_handling -> "halt"
)
@DATA(
    name            = "BenchTest",
    count<int>      = 42,
    active<bool>    = true,
    ratio<float>    = 3.14f,
    items<array>    = [1, 2, 3, 4, 5],
    label           = "benchmark",
    point<object>   = { x = 10, y = 20 },
    ts<timestamp>   = 2025-01-15T10:00:00Z,
    dt<date>        = 2025-01-15,
    nested<object>  = { a = 1, b = "two", c = [3, 4, 5] }
)"#;

/// DATA with enum references — exercises enum variant resolution.
const ENUM_HEAVY: &str = r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    error_handling -> "halt"
)
@ENUMS(
    Status    { ACTIVE = 1, INACTIVE = 2, PENDING = 3, DELETED = 4 }
    LogLevel  { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3, FATAL = 4 }
    Direction { NORTH = 1, SOUTH = 2, EAST = 3, WEST = 4 }
    Priority  { LOW = 1, MEDIUM = 2, HIGH = 3, CRITICAL = 4 }
)
@DATA(
    s1<enum>  = Status.ACTIVE,
    s2<enum>  = Status.PENDING,
    s3<enum>  = Status.INACTIVE,
    ll1<enum> = LogLevel.INFO,
    ll2<enum> = LogLevel.WARN,
    ll3<enum> = LogLevel.ERROR,
    d1<enum>  = Direction.NORTH,
    d2<enum>  = Direction.EAST,
    p1<enum>  = Priority.HIGH,
    p2<enum>  = Priority.CRITICAL
)"#;

/// DATA with many QuickFunc calls — exercises call-site symbol resolution
/// and argument type matching on every DATA entry.
const FUNC_CALL_HEAVY: &str = r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    error_handling -> "halt"
)
@QUICKFUNCS(
    ~double<int>(x<int>) {
        return x * 2
    }
    ~triple<int>(x<int>) {
        return x * 3
    }
    ~add<int>(a<int>, b<int>) {
        return a + b
    }
    ~concat<string>(a<string>, b<string>) {
        return $"{a}{b}"
    }
    ~makePoint<object>(x<int>, y<int>) {
        return { x = x, y = y }
    }
    ~clamp<int>(val<int>, lo<int>, hi<int>) {
        return val < lo ? lo : val > hi ? hi : val
    }
    ~buildLabel<string>(name<string>, version<string>) {
        return $"{name}-v{version}"
    }
)
@DATA(
    base       = 10,
    v1         = double(base),
    v2         = triple(base),
    v3         = add(v1, v2),
    v4         = concat("hello", "_world"),
    v5         = makePoint(v1, v2),
    v6         = clamp(v3, 0, 100),
    v7         = double(double(base)),
    v8         = add(triple(base), double(base)),
    v9         = buildLabel("App", "1.0.0"),
    v10        = makePoint(double(5), triple(3))
)"#;

/// All sections, QuickFunc calls, enum usage — maximum semantic work.
const COMPLEX_ALL: &str = r#"@CONFIG(
    version        -> "1.0.0",
    encoding       -> "utf-8",
    author         -> "BenchSuite",
    features       -> "advanced",
    error_handling -> "halt"
)
@ENUMS(
    Status   { ACTIVE = 1, INACTIVE = 2, PENDING = 3 }
    LogLevel { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
    Priority { LOW = 1, MEDIUM = 2, HIGH = 3, CRITICAL = 4 }
)
@QUICKFUNCS(
    ~double<int>(x<int>) {
        return x * 2
    }
    ~makeLabel<string>(name<string>, version<string>) {
        return $"{name}-v{version}"
    }
    ~buildServer<object>(host<string>, port<int>, debug<bool>) {
        return { host = host, port = port, debug = debug, url = $"http://{host}:{port}" }
    }
    ~clamp<int>(val<int>, lo<int>, hi<int>) {
        return val < lo ? lo : val > hi ? hi : val
    }
    ~isHigh<bool>(p<int>) {
        return p > 2
    }
)
@DATA(
    app_name         = "Benchmark",
    version          = "1.0.0",
    max_conn<int>    = 100,
    timeout<int>     = 5000,
    status<enum>     = Status.ACTIVE,
    log_level<enum>  = LogLevel.INFO,
    priority<enum>   = Priority.HIGH,
    doubled          = double(max_conn),
    label            = makeLabel(app_name, version),
    server           = buildServer("localhost", 8080, false),
    clamped          = clamp(doubled, 0, 500),
    flag             = isHigh(3),
    metadata<object> = {
        created  = "2025-01-15",
        owner    = "BenchSuite",
        tags     = ["bench", "test", "perf"]
    }
)"#;

// =============================================================================
// Pipeline helper — updated for token-based flow
// =============================================================================

fn parse_to_ast(source: &str) -> (DixScript, OperationalSettings) {
    let initial    = OperationalSettings::default();
    let tok_result = Tokenizer::new(source, &initial).tokenize();
    let split      = split_config_tokens(tok_result.tokens);
    let mut handler = ConfigSectionHandler::new(None);
    let cfg        = handler.process_config_tokens(&split.config_tokens);
    let settings   = cfg.operational_settings.clone();
    let parser     = GeneralParser::new(split.rest_tokens, &cfg.config_section, &settings)
        .expect("parser init");
    let ast = parser.parse().expect("parse failed");
    (ast, settings)
}

// =============================================================================
// Benchmark 1 — semantic analysis in isolation
//
// AST is pre-built once.  iter_batched clones it in the unmeasured setup
// phase so the timed closure sees a fresh, unanalysed AST each iteration.
// =============================================================================

fn bench_sem_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("sem_analysis");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(100);

    for (label, src) in &[
        ("simple_data",     SIMPLE_DATA),
        ("enum_heavy",      ENUM_HEAVY),
        ("func_call_heavy", FUNC_CALL_HEAVY),
        ("complex_all",     COMPLEX_ALL),
    ] {
        let (ast, settings) = parse_to_ast(src);
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("analyze_only", label),
            &(),
            |b, _| {
                b.iter_batched(
                    || ast.clone(),
                    |a| {
                        let analyzer = GeneralSemanticAnalyzer::new(black_box(&a), &settings);
                        black_box(analyzer.analyze())
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    // Real .dixscript file (optional)
    if let Ok(real_src) =
        std::fs::read_to_string("../../mdix_files/intermediate/ComplexFull.mdix")
    {
        let (ast, settings) = parse_to_ast(&real_src);
        group.throughput(Throughput::Bytes(real_src.len() as u64));
        group.bench_function("real_file_analyze_only", |b| {
            b.iter_batched(
                || ast.clone(),
                |a| {
                    let analyzer = GeneralSemanticAnalyzer::new(black_box(&a), &settings);
                    black_box(analyzer.analyze())
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 2 — full pipeline up to and including semantic analysis
//
// Includes tokenize → split → config → parse → analyze.
// A "parse_only" variant is included so you can subtract it and isolate
// the pure semantic analysis cost from the pipeline total.
// =============================================================================

fn bench_sem_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("sem_pipeline");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(80);

    let initial = OperationalSettings::default();

    for (label, src) in &[
        ("simple_data",     SIMPLE_DATA),
        ("enum_heavy",      ENUM_HEAVY),
        ("func_call_heavy", FUNC_CALL_HEAVY),
        ("complex_all",     COMPLEX_ALL),
    ] {
        group.throughput(Throughput::Bytes(src.len() as u64));

        // Stage 1-4 baseline
        group.bench_with_input(
            BenchmarkId::new("tokenize_parse_only", label),
            src,
            |b, s| {
                b.iter(|| {
                    let tok   = Tokenizer::new(black_box(s), &initial).tokenize();
                    let split = split_config_tokens(tok.tokens);
                    let mut h = ConfigSectionHandler::new(None);
                    let cfg   = h.process_config_tokens(&split.config_tokens);
                    let st    = cfg.operational_settings.clone();
                    let p     = GeneralParser::new(split.rest_tokens, &cfg.config_section, &st)
                        .expect("parser init");
                    black_box(p.parse())
                });
            },
        );

        // Stages 1-5
        group.bench_with_input(
            BenchmarkId::new("tokenize_parse_analyze", label),
            src,
            |b, s| {
                b.iter(|| {
                    let tok   = Tokenizer::new(black_box(s), &initial).tokenize();
                    let split = split_config_tokens(tok.tokens);
                    let mut h = ConfigSectionHandler::new(None);
                    let cfg   = h.process_config_tokens(&split.config_tokens);
                    let st    = cfg.operational_settings.clone();
                    let p     = GeneralParser::new(split.rest_tokens, &cfg.config_section, &st)
                        .expect("parser init");
                    let ast   = p.parse().expect("parse failed");
                    let analyzer = GeneralSemanticAnalyzer::new(&ast, &st);
                    black_box(analyzer.analyze())
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Registration
// =============================================================================

criterion_group!(benches, bench_sem_analysis, bench_sem_pipeline);
criterion_main!(benches);
