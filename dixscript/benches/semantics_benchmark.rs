// benches/semantics_benchmark.rs
//! Semantic Analysis Benchmark — DixScript v1.0.0
//!
//! Three benchmark groups that mirror the parser bench convention:
//!
//! 1. `section_analyzers`  — each section analyzer in isolation.
//!    ASTs are pre-built once outside the timed loop; only analysis cost is measured.
//!
//! 2. `combined_semantics` — all sections via GeneralSemanticAnalyzer (small / medium / large).
//!    Uses `iter_batched` to amortise `GeneralSemanticAnalyzer::new()` (which consumes self).
//!
//! 3. `full_pipeline`      — end-to-end: ConfigHandler → Tokenizer → GeneralParser →
//!    GeneralSemanticAnalyzer. Every allocation is inside the timed loop — accurate wall time.
//!
//! NOTE: The `semantics_benchmark` target must be declared in Cargo.toml:
//!   [[bench]]
//!   name    = "semantics_benchmark"
//!   harness = false

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::{
    Config::{ConfigSectionHandler, OperationalSettings},
    GeneralParser, GeneralSemanticAnalyzer,
    SectionAnalyzers::{
        DataSectionAnalyzer, DlmSectionAnalyzer, EnumsSectionAnalyzer,
        QuickFuncsSectionAnalyzer, SecuritySectionAnalyzer,
    },
    Tokenizer::Tokenizer,
};
use dixscript::Compiler::Utilities::SymbolTable;
use std::time::Duration;

// =============================================================================
// Static section inputs — identical to those used in general_parser_benchmark
// so results are directly comparable.
// =============================================================================

const ENUMS_SMALL: &str = r#"@CONFIG(
    version        -> "1.0.0",
    encoding       -> "utf-8",
    features       -> "advanced",
    error_handling -> "halt"
)
@ENUMS(
    Status   { ACTIVE = 1, INACTIVE = 2, PENDING = 3 }
    LogLevel { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
)
@DATA( x = 1 )"#;

const ENUMS_LARGE: &str = r#"@CONFIG(
    version        -> "1.0.0",
    encoding       -> "utf-8",
    features       -> "advanced",
    error_handling -> "halt"
)
@ENUMS(
    Status     { ACTIVE = 1, INACTIVE = 2, PENDING = 3, DELETED = 4 }
    LogLevel   { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3, FATAL = 4 }
    HttpMethod { GET = 1, POST = 2, PUT = 3, DELETE = 4, PATCH = 5, HEAD = 6 }
    Environment { DEV = 1, STAGING = 2, PROD = 3, TEST = 4 }
    Permission { READ = 1, WRITE = 2, DELETE = 3, ADMIN = 7 }
    UserRole   { GUEST = 0, USER = 1, MOD = 2, ADMIN = 3, SUPER = 4 }
    Priority   { LOW = 1, NORMAL = 2, HIGH = 3, CRITICAL = 4, BLOCKER = 5 }
    Direction  { NORTH = 1, SOUTH = 2, EAST = 3, WEST = 4 }
)
@DATA( x = 1 )"#;

const DLM_INPUT: &str = r#"@CONFIG(
    version        -> "1.0.0",
    features       -> "advanced",
    error_handling -> "halt"
)
@DLM(
    DAuditor.enhanced,
    DCompressor.gzip,
    DEncryptor.aes256
)
@DATA( x = 1 )"#;

const SECURITY_INPUT: &str = r#"@CONFIG(
    version        -> "1.0.0",
    features       -> "advanced",
    error_handling -> "halt"
)
@SECURITY(
    encryption -> { mode = "password", algorithm = "aes256-gcm" },
    keystore   -> { auto_generate = true, backup_count = 3 },
    validation -> { strict = true }
)
@DATA( x = 1 )"#;

const QUICKFUNCS_SIMPLE: &str = r#"@CONFIG(
    version        -> "1.0.0",
    features       -> "advanced",
    error_handling -> "halt"
)
@QUICKFUNCS(
    ~add<int>(a<int>, b<int>) {
        return a + b
    }
    ~clamp<int>(val<int>, lo<int>, hi<int>) {
        return val < lo ? lo : val > hi ? hi : val
    }
    ~negate<bool>(b<bool>) {
        return !b
    }
)
@DATA( x = 1 )"#;

const QUICKFUNCS_COMPLEX: &str = r#"@CONFIG(
    version        -> "1.0.0",
    features       -> "advanced",
    error_handling -> "halt"
)
@ENUMS(
    Environment { DEV = 1, STAGING = 2, PROD = 3 }
)
@QUICKFUNCS(
    ~createServer<object>(host<string>, port<int>, ssl<bool>) {
        return {
            host = host,
            port = port,
            ssl  = ssl,
            url  = $"https://{host}:{port}"
        }
    }
    ~poolSize<int>(env<int>, base<int>) {
        let multiplier = env == 3 ? 5 : env == 2 ? 2 : 1
        return base * multiplier
    }
    ~buildDbConfig<object>(host<string>, port<int>, name<string>, env<int>) {
        let pool = poolSize(env, 10)
        return {
            host     = host,
            port     = port,
            database = name,
            pool     = pool,
            ssl      = env == 3
        }
    }
    ~validatePort<bool>(port<int>) {
        return port > 1024 && port < 65536
    }
    ~formatLabel<string>(name<string>, version<string>) {
        return $"{name}-v{version}"
    }
    ~calcXp<int>(health<int>, difficulty<int>) {
        let base = health / 2
        return base * difficulty
    }
    ~calcGold<int>(health<int>) {
        return Math.round(health / 4)
    }
    ~createEnemy<object>(name<string>, health<int>, damage<int>, difficulty<int>) {
        return {
            name   = name,
            health = health,
            damage = damage,
            armor  = health / 10,
            xp     = calcXp(health, difficulty),
            gold   = calcGold(health)
        }
    }
)
@DATA( x = 1 )"#;

const COMBINED_SMALL: &str = r#"@CONFIG(
    version        -> "1.0.0",
    encoding       -> "utf-8",
    features       -> "advanced",
    error_handling -> "halt"
)
@ENUMS(
    Status   { ACTIVE = 1, INACTIVE = 2, PENDING = 3 }
    LogLevel { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
)
@QUICKFUNCS(
    ~double<int>(x<int>) {
        return x * 2
    }
    ~isActive<bool>(status<int>) {
        return status == 1
    }
)
@DATA(
    app_name  = "TestApp",
    version   = "1.0.0",
    max_users = double(500)
)"#;

// =============================================================================
// Input generators — same logic as parser bench for comparable throughput numbers
// =============================================================================

fn generate_data_only(properties: usize) -> String {
    let mut s = String::with_capacity(properties * 40);
    s.push_str("@CONFIG(\n    version -> \"1.0.0\",\n    features -> \"advanced\",\n    error_handling -> \"halt\"\n)\n");
    s.push_str("@DATA(\n");
    for i in 0..properties {
        match i % 6 {
            0 => s.push_str(&format!("    prop_{i}<int> = {i},\n")),
            1 => s.push_str(&format!("    str_{i} = \"value_{i}\",\n")),
            2 => s.push_str(&format!("    flag_{i} = {},\n", i % 2 == 0)),
            3 => s.push_str(&format!("    rate_{i}<float> = {i}.5f,\n")),
            4 => s.push_str(&format!("    arr_{i} = [{i}, {}, {}],\n", i + 1, i + 2)),
            _ => s.push_str(&format!(
                "    obj_{i} = {{ id = {i}, name = \"item_{i}\" }},\n"
            )),
        }
    }
    s.push_str(")\n");
    s
}

fn build_combined_input(data_props: usize) -> String {
    // Strip the @DATA sentinel that terminates ENUMS_LARGE / QUICKFUNCS_COMPLEX static strings
    let enums_body = &ENUMS_LARGE[..ENUMS_LARGE.rfind("@DATA").unwrap_or(ENUMS_LARGE.len())];
    let qf_body = &QUICKFUNCS_COMPLEX
        [..QUICKFUNCS_COMPLEX.rfind("@DATA").unwrap_or(QUICKFUNCS_COMPLEX.len())];
    let data_src = generate_data_only(data_props);
    format!("{enums_body}{qf_body}{data_src}")
}

// =============================================================================
// Pipeline helpers — identical to general_parser_benchmark to keep apples-to-apples
// =============================================================================

/// Run the full front-end (ConfigHandler → Tokenizer → GeneralParser) and return
/// the finished AST plus the operational settings extracted from @CONFIG.
///
/// `GeneralParser::parse()` returns `Result<DixScript, ParseException>`.
/// We `.expect()` here so that any parse failure surfaces immediately as a
/// panic in the benchmark setup phase rather than silently producing bad data.
fn parse_to_ast(source: &str) -> (DixScript, OperationalSettings) {
    let mut handler = ConfigSectionHandler::new(None);
    let cfg = handler.process_config_section(source);
    let settings = cfg.operational_settings.clone();
    let toks = Tokenizer::new(&cfg.cleaned_input_string, &settings).tokenize();
    let parser = GeneralParser::new(toks.tokens, &cfg.config_section, &settings)
        .expect("parser init");
    // parse() returns Result<DixScript, ParseException> — unwrap for bench setup
    let ast = parser.parse().expect("parse failed in bench setup");
    (ast, settings)
}

// =============================================================================
// Benchmark 1 — individual section analyzers
//
// ASTs are pre-built ONCE in setup; the timed loop measures analysis only.
// Each iteration creates a fresh `EnumsSectionAnalyzer` (cheap — just borrows)
// and a fresh empty `SymbolTable` (cheap — empty collections).
// =============================================================================

fn bench_section_analyzers(c: &mut Criterion) {
    let mut group = c.benchmark_group("section_analyzers");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(100);

    let settings = OperationalSettings::default();

    // ── @ENUMS (small — 2 declarations) ─────────────────────────────────────
    {
        let (ast, _) = parse_to_ast(ENUMS_SMALL);
        let enums = ast.enums.as_ref().expect("ENUMS_SMALL has enums");
        group.throughput(Throughput::Bytes(ENUMS_SMALL.len() as u64));
        group.bench_function("enums_small_2decls", |b| {
            b.iter(|| {
                let mut a = EnumsSectionAnalyzer::new(black_box(&settings));
                let mut st = SymbolTable::new();
                black_box(a.analyze(black_box(enums), &mut st))
            });
        });
    }

    // ── @ENUMS (large — 8 declarations) ─────────────────────────────────────
    {
        let (ast, _) = parse_to_ast(ENUMS_LARGE);
        let enums = ast.enums.as_ref().expect("ENUMS_LARGE has enums");
        group.throughput(Throughput::Bytes(ENUMS_LARGE.len() as u64));
        group.bench_function("enums_large_8decls", |b| {
            b.iter(|| {
                let mut a = EnumsSectionAnalyzer::new(black_box(&settings));
                let mut st = SymbolTable::new();
                black_box(a.analyze(black_box(enums), &mut st))
            });
        });
    }

    // ── @DLM (3 modules) ─────────────────────────────────────────────────────
    {
        let (ast, _) = parse_to_ast(DLM_INPUT);
        let dlm = ast.dlm.as_ref().expect("DLM_INPUT has dlm");
        group.throughput(Throughput::Bytes(DLM_INPUT.len() as u64));
        group.bench_function("dlm_3modules", |b| {
            b.iter(|| {
                let mut a = DlmSectionAnalyzer::new(black_box(&settings));
                let mut st = SymbolTable::new();
                black_box(a.analyze(black_box(dlm), &mut st))
            });
        });
    }

    // ── @SECURITY (3 blocks) ─────────────────────────────────────────────────
    {
        let (ast, _) = parse_to_ast(SECURITY_INPUT);
        let sec = ast.security.as_ref().expect("SECURITY_INPUT has security");
        group.throughput(Throughput::Bytes(SECURITY_INPUT.len() as u64));
        group.bench_function("security_3blocks", |b| {
            b.iter(|| {
                let mut a = SecuritySectionAnalyzer::new(black_box(&settings));
                let mut st = SymbolTable::new();
                black_box(a.analyze(black_box(sec), &mut st))
            });
        });
    }

    // ── @QUICKFUNCS (simple — 3 fns) ─────────────────────────────────────────
    {
        let (ast, _) = parse_to_ast(QUICKFUNCS_SIMPLE);
        let qf = ast.quick_functions.as_ref().expect("QUICKFUNCS_SIMPLE has qf");
        group.throughput(Throughput::Bytes(QUICKFUNCS_SIMPLE.len() as u64));
        group.bench_function("quickfuncs_simple_3fns", |b| {
            b.iter(|| {
                let mut a = QuickFuncsSectionAnalyzer::new(black_box(&settings));
                let mut st = SymbolTable::new();
                black_box(a.analyze(black_box(qf), &mut st))
            });
        });
    }

    // ── @QUICKFUNCS (complex — 8 fns with inter-calls + enum refs) ───────────
    {
        let (ast, _) = parse_to_ast(QUICKFUNCS_COMPLEX);
        // Pre-populate symbol table with enums so the analyzer can resolve them
        let (enum_ast, _) = parse_to_ast(QUICKFUNCS_COMPLEX);
        let qf = ast.quick_functions.as_ref().expect("QUICKFUNCS_COMPLEX has qf");
        group.throughput(Throughput::Bytes(QUICKFUNCS_COMPLEX.len() as u64));
        group.bench_function("quickfuncs_complex_8fns", |b| {
            b.iter(|| {
                // Populate enum symbols first — realistic: semantics runs ENUMS before QF
                let mut st = SymbolTable::new();
                if let Some(enums) = enum_ast.enums.as_ref() {
                    let mut ea = EnumsSectionAnalyzer::new(&settings);
                    ea.analyze(enums, &mut st);
                }
                let mut a = QuickFuncsSectionAnalyzer::new(black_box(&settings));
                black_box(a.analyze(black_box(qf), &mut st))
            });
        });
    }

    // ── @DATA (small / medium / large — no enum/qf deps) ─────────────────────
    for (label, n_props) in &[("small_30", 30usize), ("medium_150", 150), ("large_500", 500)] {
        let src = generate_data_only(*n_props);
        let (ast, _) = parse_to_ast(&src);
        let data = ast.data.as_ref().expect("generated DATA section");
        let byte_count = src.len() as u64;

        group.throughput(Throughput::Bytes(byte_count));
        group.bench_with_input(
            BenchmarkId::new("data", label),
            data,
            |b, d| {
                b.iter(|| {
                    let mut a = DataSectionAnalyzer::new(&settings);
                    let mut st = SymbolTable::new();
                    black_box(a.analyze(black_box(d), &mut st))
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Benchmark 2 — all sections together via GeneralSemanticAnalyzer
//
// GeneralSemanticAnalyzer::analyze() consumes `self`, so we use iter_batched.
// The setup closure (unmeasured) calls `GeneralSemanticAnalyzer::new()`; the
// routine closure (measured) calls `.analyze()`.
//
// Comparisons with the parser bench `combined_sections` group reveal the ratio
// of analysis cost to parsing cost — a key architectural health metric.
// =============================================================================

fn bench_combined_semantics(c: &mut Criterion) {
    let mut group = c.benchmark_group("combined_semantics");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(80);

    // ── Small (CONFIG + 2 ENUMS + 2 QF + tiny DATA) ──────────────────────────
    {
        let (ast, settings) = parse_to_ast(COMBINED_SMALL);
        group.throughput(Throughput::Bytes(COMBINED_SMALL.len() as u64));
        group.bench_function("all_sections_small", |b| {
            b.iter_batched(
                || GeneralSemanticAnalyzer::new(black_box(&ast), black_box(&settings)),
                |a| black_box(a.analyze()),
                BatchSize::SmallInput,
            );
        });
    }

    // ── Medium (all sections, 150-prop DATA) ──────────────────────────────────
    {
        let medium_src = build_combined_input(150);
        let (ast, settings) = parse_to_ast(&medium_src);
        group.throughput(Throughput::Bytes(medium_src.len() as u64));
        group.bench_function("all_sections_medium_150props", |b| {
            b.iter_batched(
                || GeneralSemanticAnalyzer::new(black_box(&ast), black_box(&settings)),
                |a| black_box(a.analyze()),
                BatchSize::SmallInput,
            );
        });
    }

    // ── Large (all sections, 500-prop DATA) ───────────────────────────────────
    {
        let large_src = build_combined_input(500);
        let (ast, settings) = parse_to_ast(&large_src);
        group.throughput(Throughput::Bytes(large_src.len() as u64));
        group.bench_function("all_sections_large_500props", |b| {
            b.iter_batched(
                || GeneralSemanticAnalyzer::new(black_box(&ast), black_box(&settings)),
                |a| black_box(a.analyze()),
                BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 3 — full end-to-end pipeline including semantics
//
// Measures the complete cost a caller sees:
//   ConfigSectionHandler  (extract + validate @CONFIG)
//   → Tokenizer           (lex remaining source)
//   → GeneralParser       (parse all sections)
//   → GeneralSemanticAnalyzer (validate + populate symbol table)
//
// Compare with the parser bench `pipeline_e2e/full_pipeline_*` results to
// quantify the semantic analysis overhead on top of parsing alone.
// =============================================================================

fn bench_full_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline_with_semantics");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(80);

    // ── Semantic-analysis-only baselines (pre-parsed AST, only analyze) ──────
    // These isolate semantic cost and make it easy to derive:
    //   parse cost  = full_pipeline - semantics_only
    //   semantics % = semantics_only / full_pipeline * 100
    {
        let (ast_small, settings_small) = parse_to_ast(COMBINED_SMALL);
        group.throughput(Throughput::Bytes(COMBINED_SMALL.len() as u64));
        group.bench_function("semantics_only_small", |b| {
            b.iter_batched(
                || GeneralSemanticAnalyzer::new(&ast_small, &settings_small),
                |a| black_box(a.analyze()),
                BatchSize::SmallInput,
            );
        });
    }

    // ── Full pipeline: small source ───────────────────────────────────────────
    group.throughput(Throughput::Bytes(COMBINED_SMALL.len() as u64));
    group.bench_function("full_pipeline_small", |b| {
        b.iter(|| {
            let mut handler = ConfigSectionHandler::new(None);
            let cfg = handler.process_config_section(black_box(COMBINED_SMALL));

            let s = cfg.operational_settings.clone();
            let toks = Tokenizer::new(&cfg.cleaned_input_string, &s).tokenize();
            let parser = GeneralParser::new(toks.tokens, &cfg.config_section, &s)
                .expect("parser init");
            // parse() returns Result — unwrap inside timed loop (cost is negligible)
            let ast = parser.parse().expect("parse failed");

            let analyzer = GeneralSemanticAnalyzer::new(&ast, &s);
            black_box(analyzer.analyze())
        });
    });

    // ── Full pipeline: medium source (150-prop DATA) ───────────────────────────
    {
        let medium_src = build_combined_input(150);
        group.throughput(Throughput::Bytes(medium_src.len() as u64));
        group.bench_function("full_pipeline_medium", |b| {
            b.iter(|| {
                let mut handler = ConfigSectionHandler::new(None);
                let cfg = handler.process_config_section(black_box(&medium_src));

                let s = cfg.operational_settings.clone();
                let toks = Tokenizer::new(&cfg.cleaned_input_string, &s).tokenize();
                let parser = GeneralParser::new(toks.tokens, &cfg.config_section, &s)
                    .expect("parser init");
                let ast = parser.parse().expect("parse failed");

                let analyzer = GeneralSemanticAnalyzer::new(&ast, &s);
                black_box(analyzer.analyze())
            });
        });
    }

    // ── Real .dixscript file ───────────────────────────────────────────────────────
    if let Ok(real_src) =
        std::fs::read_to_string("../../mdix_files/advanced/all_datatypes_test.dixscript")
    {
        group.throughput(Throughput::Bytes(real_src.len() as u64));

        // Semantics-only baseline for the real file
        {
            let (real_ast, real_settings) = parse_to_ast(&real_src);
            group.bench_function("real_file_semantics_only", |b| {
                b.iter_batched(
                    || GeneralSemanticAnalyzer::new(&real_ast, &real_settings),
                    |a| black_box(a.analyze()),
                    BatchSize::SmallInput,
                );
            });
        }

        group.bench_function("real_file_full_pipeline", |b| {
            b.iter(|| {
                let mut handler = ConfigSectionHandler::new(None);
                let cfg = handler.process_config_section(black_box(&real_src));

                let s = cfg.operational_settings.clone();
                let toks = Tokenizer::new(&cfg.cleaned_input_string, &s).tokenize();
                let parser = GeneralParser::new(toks.tokens, &cfg.config_section, &s)
                    .expect("parser init");
                let ast = parser.parse().expect("parse failed");

                let analyzer = GeneralSemanticAnalyzer::new(&ast, &s);
                black_box(analyzer.analyze())
            });
        });
    }

    group.finish();
}

// =============================================================================
// Registration
// =============================================================================

criterion_group!(
    benches,
    bench_section_analyzers,
    bench_combined_semantics,
    bench_full_pipeline,
);
criterion_main!(benches);
