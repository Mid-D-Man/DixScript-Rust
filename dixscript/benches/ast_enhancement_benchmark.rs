// benches/ast_enhancement_benchmark.rs
//! AST Enhancement Benchmark — DixScript (token-based pipeline)
//!
//! Pipeline order:
//!   Stage 1: Tokenizer::new(source).tokenize()
//!   Stage 2: split_config_tokens(tokens)
//!   Stage 3: ConfigSectionHandler::process_config_tokens(&config_tokens)
//!   Stage 4: GeneralParser::new(rest_tokens, ...).parse()
//!   Stage 5: GeneralSemanticAnalyzer::new(&ast, &settings).analyze()
//!   Stage 6: GeneralAstEnhancer::new(&settings).enhance(&ast, Some(&sem))  ← measured here
//!
//! Groups:
//!   ast_enhancement     — stage 6 in isolation (pre-parsed + pre-analysed)
//!   enhancement_pipeline— stages 1-6 end-to-end, with stage 1-5 baseline

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Enhancement::GeneralAstEnhancer;
use dixscript::Compiler::Core::GeneralParser;
use dixscript::Compiler::Core::Semantics::GeneralSemanticAnalyzer;
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::DixScript;
use std::time::Duration;

// =============================================================================
// Test inputs (same set as semantics_benchmark — covers the same spectrum)
// =============================================================================

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
    base  = 10,
    v1    = double(base),
    v2    = triple(base),
    v3    = add(v1, v2),
    v4    = concat("hello", "_world"),
    v5    = makePoint(v1, v2),
    v6    = clamp(v3, 0, 100),
    v7    = double(double(base)),
    v8    = add(triple(base), double(base)),
    v9    = buildLabel("App", "1.0.0"),
    v10   = makePoint(double(5), triple(3))
)"#;

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
// Pipeline helpers — updated for token-based flow
// =============================================================================

/// Stages 1-4: source → parsed AST.
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

/// Stages 1-5: source → parsed AST + semantic result.
/// Enhancement (stage 6) needs both, so we pre-build them here.
fn parse_and_analyze(
    source: &str,
) -> (DixScript, <GeneralSemanticAnalyzer as Analyzer>::Output, OperationalSettings) {
    let (ast, settings) = parse_to_ast(source);
    let sem = GeneralSemanticAnalyzer::new(&ast, &settings).analyze();
    (ast, sem, settings)
}
