// benches/value_resolution_benchmark.rs
//! Value Resolution Benchmark — DixScript v1.0.0
//!
//! Three benchmark groups:
//!
//! 1. `resolver_by_complexity` — ValueResolver in isolation against inputs that
//!    exercise different phase mixes:
//!    - no_funcs: phases 1 + 2 + 5 only (no function calls in DATA)
//!    - simple_funcs: phases 1–5, shallow call graph
//!    - complex_funcs: phases 1–5, deep call graph with inter-function calls
//!
//! 2. `combined_resolution` — ValueResolver at small / medium / large DATA sizes
//!    (mirrors the parser and semantics bench shapes for apples-to-apples comparison).
//!
//! 3. `full_pipeline_with_resolution` — complete front-end cost:
//!    ConfigHandler → Tokenizer → GeneralParser → GeneralSemanticAnalyzer →
//!    GeneralAstEnhancer → ValueResolver.
//!    Compare with `full_pipeline_with_semantics` to quantify resolution overhead.

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, Criterion, Throughput,
};
use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::{
    Config::{ConfigSectionHandler, OperationalSettings},
    GeneralAstEnhancer, GeneralParser, GeneralSemanticAnalyzer,
    Tokenizer::Tokenizer,
    ValueResolution::{ValueResolver, ValueResolutionResult},
};
use dixscript::Compiler::AST::data_types::DebugMode;
use dixscript::Compiler::Utilities::SymbolTable;
use std::time::Duration;

// =============================================================================
// Static inputs
// =============================================================================

/// DATA section with only literal values — exercises phases 1, 2, and 5.
/// No QuickFuncs means phase 3 (discovery) and phase 4 (execution) are skipped.
const NO_FUNCS_INPUT: &str = r#"@CONFIG(
    version        -> "1.0.0",
    encoding       -> "utf-8",
    features       -> "advanced",
    error_handling -> "halt"
)
@ENUMS(
    Status   { ACTIVE = 1, INACTIVE = 2, PENDING = 3 }
    LogLevel { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
)
@DATA(
    app_name      = "BenchApp",
    version       = "1.0.0",
    max_retries   = 5,
    timeout_ms    = 3000,
    debug_enabled = false,
    log_level<enum> = LogLevel.INFO,
    status<enum>    = Status.ACTIVE,
    server.config:
        host = "bench-server.local",
        port = 8080,
        ssl  = true
    database.primary:
        host = "db.local",
        port = 5432,
        name = "bench_db",
        pool = 10
    tags:: "production", "bench", "v1"
)"#;

/// DATA section with simple, non-recursive function calls — exercises all 5 phases
/// with a shallow call graph (no inter-function dependencies).
const SIMPLE_FUNCS_INPUT: &str = r#"@CONFIG(
    version        -> "1.0.0",
    encoding       -> "utf-8",
    features       -> "advanced",
    error_handling -> "halt"
)
@ENUMS(
    Environment { DEV = 1, STAGING = 2, PROD = 3 }
    LogLevel    { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
)
@QUICKFUNCS(
    ~serverPort<int>(env<int>) {
        return env == 3 ? 443 : env == 2 ? 8443 : 8080
    }
    ~poolSize<int>(env<int>) {
        return env == 3 ? 50 : env == 2 ? 25 : 10
    }
    ~makeLabel<string>(name<string>, version<string>) {
        return $"{name}-v{version}"
    }
    ~isProduction<bool>(env<int>) {
        return env == 3
    }
    ~calcTimeout<int>(base<int>, env<int>) {
        return base * (env == 3 ? 2 : 1)
    }
)
@DATA(
    current_env<enum> = Environment.PROD,
    app_label = makeLabel("BenchApp", "1.0.0"),
    server.config:
        host    = "prod.bench.local",
        port    = serverPort(3),
        pool    = poolSize(3),
        ssl     = isProduction(3),
        timeout = calcTimeout(5000, 3)
    staging.config:
        host    = "staging.bench.local",
        port    = serverPort(2),
        pool    = poolSize(2),
        ssl     = isProduction(2),
        timeout = calcTimeout(5000, 2)
    dev.config:
        host    = "localhost",
        port    = serverPort(1),
        pool    = poolSize(1),
        ssl     = isProduction(1),
        timeout = calcTimeout(5000, 1)
)"#;

/// DATA section with a complex call graph including inter-function calls and
/// object-returning functions — exercises the full iterative resolution loop.
const COMPLEX_FUNCS_INPUT: &str = r#"@CONFIG(
    version        -> "1.0.0",
    encoding       -> "utf-8",
    features       -> "advanced",
    error_handling -> "halt"
)
@ENUMS(
    AIType      { PASSIVE = 0, NEUTRAL = 1, AGGRESSIVE = 2, BOSS = 3 }
    Environment { DEV = 1, STAGING = 2, PROD = 3 }
)
@QUICKFUNCS(
    ~calcXp<int>(health<int>, difficulty<int>) {
        let base = health / 2
        return base * difficulty
    }
    ~calcGold<int>(health<int>) {
        return Math.round(health / 4)
    }
    ~calcArmor<int>(health<int>) {
        return health / 10
    }
    ~spawnRate<float>(ai<int>) {
        return ai == 3 ? 0.01f : ai == 2 ? 0.15f : 0.30f
    }
    ~createEnemy<object>(name<string>, health<int>, damage<int>, ai<int>, difficulty<int>) {
        return {
            name       = name,
            health     = health,
            damage     = damage,
            armor      = calcArmor(health),
            xp         = calcXp(health, difficulty),
            gold       = calcGold(health),
            spawn_rate = spawnRate(ai)
        }
    }
    ~poolSize<int>(env<int>, base<int>) {
        let multiplier = env == 3 ? 5 : env == 2 ? 2 : 1
        return base * multiplier
    }
    ~serverConfig<object>(env<int>, suffix<string>) {
        return {
            host      = $"{suffix}-server.local",
            port      = 8080,
            pool      = poolSize(env, 10),
            timeout   = 5000,
            ssl       = env == 3
        }
    }
    ~formatVersion<string>(major<int>, minor<int>, patch<int>) {
        return $"{major}.{minor}.{patch}"
    }
)
@DATA(
    app_version = formatVersion(1, 0, 0),
    enemies::
        createEnemy("Goblin",  50,  10, 2, 1),
        createEnemy("Orc",    100,  20, 2, 2),
        createEnemy("Troll",  200,  40, 2, 2),
        createEnemy("Dragon", 1000, 150, 3, 3)
    servers::
        serverConfig(1, "dev"),
        serverConfig(2, "staging"),
        serverConfig(3, "prod")
)"#;

// =============================================================================
// Input generators
// =============================================================================

fn generate_flat_literals(n: usize) -> String {
    let mut s = String::with_capacity(n * 40);
    s.push_str(
        "@CONFIG(\n    version -> \"1.0.0\",\n    features -> \"advanced\",\n    error_handling -> \"halt\"\n)\n@DATA(\n",
    );
    for i in 0..n {
        match i % 5 {
            0 => s.push_str(&format!("    prop_{i}<int>    = {i},\n")),
            1 => s.push_str(&format!("    str_{i}          = \"value_{i}\",\n")),
            2 => s.push_str(&format!("    flag_{i}<bool>   = {},\n", i % 2 == 0)),
            3 => s.push_str(&format!("    rate_{i}<float>  = {i}.5f,\n")),
            _ => s.push_str(&format!("    obj_{i}          = {{ id = {i}, name = \"item_{i}\" }},\n")),
        }
    }
    s.push_str(")\n");
    s
}

fn generate_func_heavy(n_enemies: usize) -> String {
    let funcs = r#"@CONFIG(
    version        -> "1.0.0",
    features       -> "advanced",
    error_handling -> "halt"
)
@ENUMS(
    AIType { PASSIVE = 0, AGGRESSIVE = 1, BOSS = 2 }
)
@QUICKFUNCS(
    ~calcXp<int>(health<int>) {
        return health / 2
    }
    ~calcGold<int>(health<int>) {
        return Math.round(health / 4)
    }
    ~createEnemy<object>(name<string>, health<int>, damage<int>) {
        return {
            name   = name,
            health = health,
            damage = damage,
            armor  = health / 10,
            xp     = calcXp(health),
            gold   = calcGold(health)
        }
    }
)
@DATA(
    enemies::"#;

    let mut s = String::from(funcs);
    let names = ["Goblin", "Orc", "Troll", "Ogre", "Wraith", "Vampire", "Dragon", "Golem"];
    for i in 0..n_enemies {
        let name = names[i % names.len()];
        let health = 50 * ((i % 8) + 1);
        let damage = health / 5;
        s.push_str(&format!(
            "\n        createEnemy(\"{name}_{i}\", {health}, {damage}),"
        ));
    }
    if s.ends_with(',') {
        s.pop();
    }
    s.push_str("\n)\n");
    s
}

// =============================================================================
// Pipeline helpers
// =============================================================================

/// Run the full front-end pipeline through AST enhancement and return the
/// enhanced AST plus its populated SymbolTable.
///
/// Phase order: ConfigHandler → Tokenizer → GeneralParser →
/// GeneralSemanticAnalyzer → GeneralAstEnhancer.
///
/// `ValueResolver` in the isolation benchmarks receives the enhanced AST so
/// that qualified-identifier annotations produced by the enhancer are present,
/// matching real-world resolver inputs.
fn parse_analyze_and_enhance(source: &str) -> (DixScript, SymbolTable, OperationalSettings) {
    let mut handler = ConfigSectionHandler::new(None);
    let cfg = handler.process_config_section(source);
    let settings = cfg.operational_settings.clone();
    let toks = Tokenizer::new(&cfg.cleaned_input_string, &settings).tokenize();
    let parser =
        GeneralParser::new(toks.tokens, &cfg.config_section, &settings).expect("parser init");
    let ast = parser.parse().expect("parse failed in bench setup");

    // Semantic analysis — returns SemanticAnalysisResult directly (not Result<…>).
    let sem_result = GeneralSemanticAnalyzer::new(&ast, &settings).analyze();

    // Clone the symbol table before the enhancer borrows sem_result.
    let symbol_table = sem_result
        .symbol_table
        .clone()
        .expect("symbol table missing after semantic analysis");

    // AST enhancement — resolves qualified identifiers and applies parameter
    // defaults.  The enhanced AST is what ValueResolver should operate on.
    let enhancer = GeneralAstEnhancer::new(&settings);
    let enhancement = enhancer.enhance(&ast, Some(&sem_result));

    (enhancement.enhanced_ast, symbol_table, settings)
}

/// Run the complete 5-stage pipeline including AST enhancement and resolution.
fn run_full_pipeline(source: &str) -> ValueResolutionResult {
    let mut handler = ConfigSectionHandler::new(None);
    let cfg = handler.process_config_section(source);
    let s = cfg.operational_settings.clone();
    let toks = Tokenizer::new(&cfg.cleaned_input_string, &s).tokenize();
    let parser = GeneralParser::new(toks.tokens, &cfg.config_section, &s).expect("parser init");
    let ast = parser.parse().expect("parse failed");

    // Semantic analysis — returns SemanticAnalysisResult directly.
    let sem_result = GeneralSemanticAnalyzer::new(&ast, &s).analyze();

    let symbol_table = sem_result
        .symbol_table
        .clone()
        .expect("symbol table missing after semantic analysis");

    // AST enhancement between semantics and resolution.
    let enhancer = GeneralAstEnhancer::new(&s);
    let enhancement = enhancer.enhance(&ast, Some(&sem_result));

    let resolver =
        ValueResolver::new(enhancement.enhanced_ast, &symbol_table, DebugMode::Off);
    resolver.resolve()
}

// =============================================================================
// Benchmark 1 — resolver in isolation by input complexity
//
// The enhanced AST + SymbolTable are pre-built once in setup; only
// ValueResolver::resolve() is timed.  iter_batched is required because
// resolve() consumes self.
// =============================================================================

fn bench_resolver_by_complexity(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolver_by_complexity");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(100);

    // ── No function calls (phases 1 + 2 + 5 only) ────────────────────────────
    {
        let (ast, st, _) = parse_analyze_and_enhance(NO_FUNCS_INPUT);
        group.throughput(Throughput::Bytes(NO_FUNCS_INPUT.len() as u64));
        group.bench_function("no_funcs", |b| {
            b.iter_batched(
                || ValueResolver::new(black_box(ast.clone()), black_box(&st), DebugMode::Off),
                |r| black_box(r.resolve()),
                BatchSize::SmallInput,
            );
        });
    }

    // ── Simple function calls (flat call graph) ───────────────────────────────
    {
        let (ast, st, _) = parse_analyze_and_enhance(SIMPLE_FUNCS_INPUT);
        group.throughput(Throughput::Bytes(SIMPLE_FUNCS_INPUT.len() as u64));
        group.bench_function("simple_funcs", |b| {
            b.iter_batched(
                || ValueResolver::new(black_box(ast.clone()), black_box(&st), DebugMode::Off),
                |r| black_box(r.resolve()),
                BatchSize::SmallInput,
            );
        });
    }

    // ── Complex function calls (inter-function deps, object return types) ─────
    {
        let (ast, st, _) = parse_analyze_and_enhance(COMPLEX_FUNCS_INPUT);
        group.throughput(Throughput::Bytes(COMPLEX_FUNCS_INPUT.len() as u64));
        group.bench_function("complex_funcs", |b| {
            b.iter_batched(
                || ValueResolver::new(black_box(ast.clone()), black_box(&st), DebugMode::Off),
                |r| black_box(r.resolve()),
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 2 — resolver at scale (small / medium / large)
//
// Small:  literal-only DATA — measures base overhead with no resolution work.
// Medium: 20-enemy generated input — moderate iterative resolution loop.
// Large:  60-enemy generated input — stress-tests the iterative pass and
//         data-context grow/lookup costs.
// =============================================================================

fn bench_combined_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("combined_resolution");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(80);

    // ── Small: 50 literal properties, no funcs ────────────────────────────────
    {
        let src = generate_flat_literals(50);
        let (ast, st, _) = parse_analyze_and_enhance(&src);
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_function("literals_50props", |b| {
            b.iter_batched(
                || ValueResolver::new(black_box(ast.clone()), black_box(&st), DebugMode::Off),
                |r| black_box(r.resolve()),
                BatchSize::SmallInput,
            );
        });
    }

    // ── Medium: 20 enemies (20 object-returning calls, 60 leaf calls) ─────────
    {
        let src = generate_func_heavy(20);
        let (ast, st, _) = parse_analyze_and_enhance(&src);
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_function("enemy_20calls", |b| {
            b.iter_batched(
                || ValueResolver::new(black_box(ast.clone()), black_box(&st), DebugMode::Off),
                |r| black_box(r.resolve()),
                BatchSize::SmallInput,
            );
        });
    }

    // ── Large: 60 enemies ────────────────────────────────────────────────────
    {
        let src = generate_func_heavy(60);
        let (ast, st, _) = parse_analyze_and_enhance(&src);
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_function("enemy_60calls", |b| {
            b.iter_batched(
                || ValueResolver::new(black_box(ast.clone()), black_box(&st), DebugMode::Off),
                |r| black_box(r.resolve()),
                BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 3 — full end-to-end pipeline including resolution
//
// Measures total wall time a caller sees:
//   ConfigSectionHandler → Tokenizer → GeneralParser
//   → GeneralSemanticAnalyzer → GeneralAstEnhancer → ValueResolver
//
// Resolution-only baselines allow deriving:
//   resolution % = resolution_only / full_pipeline * 100
//
// Compare against `full_pipeline_with_semantics` (semantics_benchmark) to
// understand the marginal cost of adding AST enhancement and value resolution.
// =============================================================================

fn bench_full_pipeline_with_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline_with_resolution");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(80);

    // ── Resolution-only baselines (enhanced AST pre-built, only resolve) ──────
    {
        let (ast_no, st_no, _) = parse_analyze_and_enhance(NO_FUNCS_INPUT);
        group.throughput(Throughput::Bytes(NO_FUNCS_INPUT.len() as u64));
        group.bench_function("resolution_only_no_funcs", |b| {
            b.iter_batched(
                || ValueResolver::new(ast_no.clone(), &st_no, DebugMode::Off),
                |r| black_box(r.resolve()),
                BatchSize::SmallInput,
            );
        });
    }
    {
        let (ast_cx, st_cx, _) = parse_analyze_and_enhance(COMPLEX_FUNCS_INPUT);
        group.throughput(Throughput::Bytes(COMPLEX_FUNCS_INPUT.len() as u64));
        group.bench_function("resolution_only_complex_funcs", |b| {
            b.iter_batched(
                || ValueResolver::new(ast_cx.clone(), &st_cx, DebugMode::Off),
                |r| black_box(r.resolve()),
                BatchSize::SmallInput,
            );
        });
    }

    // ── Full pipeline: no function calls ─────────────────────────────────────
    group.throughput(Throughput::Bytes(NO_FUNCS_INPUT.len() as u64));
    group.bench_function("full_pipeline_no_funcs", |b| {
        b.iter(|| black_box(run_full_pipeline(black_box(NO_FUNCS_INPUT))));
    });

    // ── Full pipeline: simple function calls ─────────────────────────────────
    group.throughput(Throughput::Bytes(SIMPLE_FUNCS_INPUT.len() as u64));
    group.bench_function("full_pipeline_simple_funcs", |b| {
        b.iter(|| black_box(run_full_pipeline(black_box(SIMPLE_FUNCS_INPUT))));
    });

    // ── Full pipeline: complex function calls ────────────────────────────────
    group.throughput(Throughput::Bytes(COMPLEX_FUNCS_INPUT.len() as u64));
    group.bench_function("full_pipeline_complex_funcs", |b| {
        b.iter(|| black_box(run_full_pipeline(black_box(COMPLEX_FUNCS_INPUT))));
    });

    // ── Full pipeline: 20-enemy generated input ───────────────────────────────
    {
        let medium_src = generate_func_heavy(20);
        group.throughput(Throughput::Bytes(medium_src.len() as u64));
        group.bench_function("full_pipeline_20enemies", |b| {
            b.iter(|| black_box(run_full_pipeline(black_box(&medium_src))));
        });
    }

    // ── Real .mdix file ───────────────────────────────────────────────────────
    if let Ok(real_src) =
        std::fs::read_to_string("mdix_files/advanced/all_datatypes_test.mdix")
    {
        group.throughput(Throughput::Bytes(real_src.len() as u64));

        {
            let (real_ast, real_st, _) = parse_analyze_and_enhance(&real_src);
            group.bench_function("real_file_resolution_only", |b| {
                b.iter_batched(
                    || ValueResolver::new(real_ast.clone(), &real_st, DebugMode::Off),
                    |r| black_box(r.resolve()),
                    BatchSize::SmallInput,
                );
            });
        }

        group.bench_function("real_file_full_pipeline", |b| {
            b.iter(|| black_box(run_full_pipeline(black_box(&real_src))));
        });
    }

    group.finish();
}

// =============================================================================
// Registration
// =============================================================================

criterion_group!(
    benches,
    bench_resolver_by_complexity,
    bench_combined_resolution,
    bench_full_pipeline_with_resolution,
);
criterion_main!(benches);
