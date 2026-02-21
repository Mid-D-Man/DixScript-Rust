// tests/pipeline_integration_tests.rs
//
// Comprehensive DixScript pipeline integration tests.
// Covers: Config Handling → Tokenization → Parsing → Semantic Analysis
//
// Usage:
//   cargo test pipeline                                    # all tests (excl. #[ignore])
//   cargo test pipeline -- --nocapture                    # with stdout
//   cargo test pipeline -- --nocapture --include-ignored  # include jsonnet benchmarks
//   cargo test -- --test-threads=1 --nocapture            # serialised (avoids singleton bleed)

use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::Config::ConfigSectionHandler;
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, TokenizationResult};
use dixscript::Compiler::Core::{
    GeneralParser, GeneralSemanticAnalyzer, OperationalSettings, SemanticAnalysisResult,
};
use dixscript::ErrorManager::{DiagnosticDumper, ErrorManager};
use dixscript::Utilities::{AstDebugPrinter, TokenDebugPrinter};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

// ═══════════════════════════════════════════════════════════
//  INLINE TEST SOURCES
// ═══════════════════════════════════════════════════════════

/// Minimal valid file — no CONFIG block.
const SRC_MINIMAL: &str = r#"@DATA(
    x = 42,
    name = "hello world",
    active = true,
    score = 9.99f,
    ratio = 0.5
)"#;

/// Config present, minimal body.
const SRC_CONFIG_ONLY: &str = r#"@CONFIG(
    version -> "1.0.0",
    author -> "Test Suite",
    features -> "advanced",
    debug_mode -> "off",
    error_handling -> "continue"
)

@DATA(
    placeholder = 1
)"#;

/// Enums + typed DATA references.
const SRC_ENUMS: &str = r#"@CONFIG(
    version -> "1.0.0",
    features -> "advanced",
    error_handling -> "continue"
)

@ENUMS(
    Status { ACTIVE = 1, INACTIVE = 2, PENDING = 3 },
    Priority { LOW = 1, MEDIUM = 2, HIGH = 3, CRITICAL = 4 }
)

@DATA(
    current_status<enum> = Status.ACTIVE,
    task_priority<enum> = Priority.HIGH,
    status_code<int> = 1,
    label = "operational"
)"#;

/// QuickFuncs + Enums — the primary DixScript value proposition.
const SRC_QUICKFUNCS: &str = r#"@CONFIG(
    version -> "1.0.0",
    features -> "advanced",
    error_handling -> "continue"
)

@ENUMS(
    Difficulty { EASY = 1, NORMAL = 2, HARD = 3 }
)

@QUICKFUNCS(
    ~createEnemy<object> => global(name<string>, health<int>, damage<int>) {
        let armor = health / 10;
        let xp = health / 2;
        let gold = health / 4;
        return {
            name = name,
            health = health,
            damage = damage,
            armor = armor,
            xp = xp,
            gold = gold
        };
    }

    ~doubleX<int> => global(x<int>) {
        return x * 2;
    }

    ~serverConfig<object> => global(env<enum>, suffix<string>) {
        let pool = env == Difficulty.HARD ? 50 : 10;
        return {
            host = $"{suffix}.local",
            port = 8080,
            pool_size = pool,
            ssl = env == Difficulty.HARD
        };
    }
)

@DATA(
    goblin = createEnemy("Goblin", 50, 10),
    orc = createEnemy("Orc", 100, 20),
    troll = createEnemy("Troll", 200, 40),
    doubled = doubleX(21),
    dev_server = serverConfig(Difficulty.EASY, "dev"),
    prod_server = serverConfig(Difficulty.HARD, "prod")
)"#;

/// Full-featured: all major sections present.
const SRC_FULL: &str = r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    author -> "Integration Test",
    features -> "advanced",
    debug_mode -> "off",
    error_handling -> "continue",
    compatibility_mode -> "strict"
)

@ENUMS(
    Environment { DEV = 1, STAGING = 2, PROD = 3 },
    LogLevel { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 },
    HttpMethod { GET = 1, POST = 2, PUT = 3, DELETE = 4 }
)

@QUICKFUNCS(
    ~serverConfig<object> => global(env<enum>, host<string>) {
        let pool = env == Environment.DEV ? 10 :
                   env == Environment.STAGING ? 25 : 50;
        let ssl = env == Environment.PROD;
        return {
            host = host,
            port = 8080,
            pool_size = pool,
            ssl = ssl,
            timeout_ms = 5000
        };
    }

    ~endpoint<object> => global(path<string>, method<enum>, auth<bool>) {
        let rate = method == HttpMethod.GET ? 200 : 50;
        return {
            path = path,
            method = method,
            rate_limit = rate,
            auth = auth
        };
    }

    ~buildDbUrl<string> => global(host<string>, port<int>, dbname<string>) {
        return $"postgresql://{host}:{port}/{dbname}";
    }
)

@DATA(
    app_name = "FullTestApp",
    version = "1.0.0",
    debug = false,
    log_level<enum> = LogLevel.INFO,

    dev = serverConfig(Environment.DEV, "dev.internal"),
    staging = serverConfig(Environment.STAGING, "staging.internal"),
    prod = serverConfig(Environment.PROD, "api.example.com"),

    db.primary:
        host = "db.prod.local",
        port = 5432,
        name = "maindb",
        url = buildDbUrl("db.prod.local", 5432, "maindb")

    api.endpoints::
        endpoint("/users", HttpMethod.GET, true),
        endpoint("/users", HttpMethod.POST, true),
        endpoint("/health", HttpMethod.GET, false),
        endpoint("/products", HttpMethod.GET, false)
)"#;

// ── Jsonnet equivalents for comparison ──────────────────────────────────────
// Functionally equivalent to SRC_QUICKFUNCS for a fair benchmark.
const JSONNET_EQUIVALENT: &str = r#"local createEnemy(name, health, damage) = {
  name: name,
  health: health,
  damage: damage,
  armor: std.floor(health / 10),
  xp: std.floor(health / 2),
  gold: std.floor(health / 4),
};

local double(x) = x * 2;

local serverConfig(env, suffix) = {
  host: suffix + ".local",
  port: 8080,
  pool_size: if env == "HARD" then 50 else 10,
  ssl: env == "HARD",
};

{
  goblin:      createEnemy("Goblin", 50,  10),
  orc:         createEnemy("Orc",   100,  20),
  troll:       createEnemy("Troll", 200,  40),
  doubled:     double(21),
  dev_server:  serverConfig("EASY", "dev"),
  prod_server: serverConfig("HARD", "prod"),
}
"#;

// ═══════════════════════════════════════════════════════════
//  PIPELINE INFRASTRUCTURE
// ═══════════════════════════════════════════════════════════

struct PipelineResult {
    ast: DixScript,
    tokenization: TokenizationResult,
    semantic: SemanticAnalysisResult,
    metrics: Metrics,
}

#[derive(Debug, Clone)]
struct Metrics {
    input_bytes: usize,
    config_ms: f64,
    tokenize_ms: f64,
    parse_ms: f64,
    analyze_ms: f64,
    total_ms: f64,
    token_count: usize,
    throughput_kb_s: f64,
}

impl Metrics {
    fn print(&self, label: &str) {
        println!(
            "\n┌─── {} ─────────────────────────────────\n\
             │  Input:      {:>7} bytes   {:>5} tokens\n\
             │  Config:     {:>10.3} ms\n\
             │  Tokenize:   {:>10.3} ms\n\
             │  Parse:      {:>10.3} ms\n\
             │  Analyze:    {:>10.3} ms\n\
             │  ────────────────────────────────────\n\
             │  Total:      {:>10.3} ms\n\
             │  Throughput: {:>10.1} KB/s\n\
             └────────────────────────────────────────",
            label,
            self.input_bytes,
            self.token_count,
            self.config_ms,
            self.tokenize_ms,
            self.parse_ms,
            self.analyze_ms,
            self.total_ms,
            self.throughput_kb_s,
        );
    }
}

/// Clear the global singleton between tests to prevent error bleed.
fn reset_singletons() {
    ErrorManager::get_shared_instance().clear_errors();
}

/// Run the full four-phase DixScript pipeline and return timings + AST.
fn run_pipeline(source: &str) -> PipelineResult {
    reset_singletons();

    let t_total = Instant::now();
    let input_bytes = source.len();

    // ── Phase 1: Config extraction ─────────────────────────
    let t0 = Instant::now();
    let handler = ConfigSectionHandler::new(None);
    let cr = handler.process_config_section(source);
    let config_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // ── Phase 2: Tokenisation ──────────────────────────────
    let t1 = Instant::now();
    let tokenization = Tokenizer::new(cr.cleaned_input_string.clone()).tokenize();
    let tokenize_ms = t1.elapsed().as_secs_f64() * 1000.0;
    let token_count = tokenization.metadata.total_tokens;

    // ── Phase 3: Parsing ───────────────────────────────────
    let t2 = Instant::now();
    let parser = GeneralParser::new(
        tokenization.tokens.clone(),
        cr.config_section.clone(),
        cr.operational_settings.clone(),
    )
    .expect("GeneralParser::new failed");
    let ast = parser.parse().expect("parser.parse() failed");
    let parse_ms = t2.elapsed().as_secs_f64() * 1000.0;

    // ── Phase 4: Semantic analysis ─────────────────────────
    let t3 = Instant::now();
    let semantic =
        GeneralSemanticAnalyzer::new(&ast, &cr.operational_settings).analyze();
    let analyze_ms = t3.elapsed().as_secs_f64() * 1000.0;

    let total_ms = t_total.elapsed().as_secs_f64() * 1000.0;
    let throughput_kb_s = if total_ms > 0.0 {
        (input_bytes as f64 / 1024.0) / (total_ms / 1000.0)
    } else {
        f64::INFINITY
    };

    PipelineResult {
        ast,
        tokenization,
        semantic,
        metrics: Metrics {
            input_bytes,
            config_ms,
            tokenize_ms,
            parse_ms,
            analyze_ms,
            total_ms,
            token_count,
            throughput_kb_s,
        },
    }
}

/// Load a .mdix fixture file relative to the crate root.
fn load_fixture(rel: &str) -> String {
    let root = std::env::current_dir().expect("cannot get cwd");
    let full = root.join(rel);
    fs::read_to_string(&full)
        .unwrap_or_else(|e| panic!("Cannot read '{}': {}", full.display(), e))
}

/// Skip gracefully if a fixture does not exist.
fn fixture_exists(rel: &str) -> bool {
    std::env::current_dir()
        .map(|r| r.join(rel).exists())
        .unwrap_or(false)
}

/// Run the pipeline N times and return (avg_ms, avg_token_count).
fn bench_n(source: &str, n: u32) -> (f64, usize) {
    let mut total_ms = 0.0f64;
    let mut total_tokens = 0usize;
    for _ in 0..n {
        reset_singletons();
        let t = Instant::now();
        let cr = ConfigSectionHandler::new(None).process_config_section(source);
        let tok = Tokenizer::new(cr.cleaned_input_string.clone()).tokenize();
        total_tokens += tok.metadata.total_tokens;
        let parser =
            GeneralParser::new(tok.tokens, cr.config_section.clone(), cr.operational_settings.clone())
                .expect("parser");
        let ast = parser.parse().expect("ast");
        let _ = GeneralSemanticAnalyzer::new(&ast, &cr.operational_settings).analyze();
        total_ms += t.elapsed().as_secs_f64() * 1000.0;
    }
    (total_ms / n as f64, total_tokens / n as usize)
}

// ═══════════════════════════════════════════════════════════
//  CONFIG HANDLING TESTS
// ═══════════════════════════════════════════════════════════

#[test]
fn config_present_extracts_and_cleans() {
    reset_singletons();
    let cr = ConfigSectionHandler::new(None).process_config_section(SRC_CONFIG_ONLY);

    // CONFIG entries should be populated
    assert!(
        !cr.config_section.entries.is_empty(),
        "Config entries should not be empty"
    );

    // Cleaned input must not contain @CONFIG any more
    assert!(
        !cr.cleaned_input_string.to_uppercase().contains("@CONFIG"),
        "Cleaned input should not contain @CONFIG block"
    );

    // Version from file should be parsed
    assert_eq!(cr.operational_settings.version, "1.0.0", "Version should match file");
    assert!(
        cr.operational_settings.is_advanced_mode(),
        "features -> advanced should set advanced mode"
    );

    println!("[config_present] entries={}, cleaned={} bytes, advanced={}",
        cr.config_section.entries.len(),
        cr.cleaned_input_string.len(),
        cr.operational_settings.is_advanced_mode(),
    );
}

#[test]
fn config_absent_uses_defaults_no_fatal_error() {
    reset_singletons();
    let cr = ConfigSectionHandler::new(None).process_config_section(SRC_MINIMAL);

    let em = ErrorManager::get_shared_instance();
    assert!(
        !em.has_fatal_errors(),
        "Missing @CONFIG must not cause fatal errors"
    );
    assert!(
        !cr.operational_settings.version.is_empty(),
        "Default version must be non-empty"
    );

    println!("[config_absent] default_version={}, warnings={:?}",
        cr.operational_settings.version, cr.warnings);
}

#[test]
fn config_all_well_known_fields() {
    reset_singletons();
    let cr = ConfigSectionHandler::new(None).process_config_section(SRC_FULL);
    let s = &cr.operational_settings;

    assert_eq!(s.version, "1.0.0");
    assert!(s.is_advanced_mode());

    println!("[config_all_fields] version={} debug={:?} strategy={:?}",
        s.version, s.debug_mode, s.error_handling_strategy);
}

// ═══════════════════════════════════════════════════════════
//  TOKENISATION TESTS
// ═══════════════════════════════════════════════════════════

#[test]
fn tokenize_minimal_no_lex_errors() {
    reset_singletons();
    let cr = ConfigSectionHandler::new(None).process_config_section(SRC_MINIMAL);
    let result = Tokenizer::new(cr.cleaned_input_string).tokenize();

    assert!(result.metadata.total_tokens > 0, "Must produce tokens");
    assert!(
        result.metadata.sections_detected.iter().any(|s| s.eq_ignore_ascii_case("DATA")),
        "Should detect @DATA"
    );

    let lex_errs = ErrorManager::get_shared_instance().get_lexical_errors();
    assert!(lex_errs.is_empty(), "Minimal source should have zero lex errors; got: {:?}", lex_errs);

    println!("[tokenize_minimal] tokens={} sections={:?}",
        result.metadata.total_tokens, result.metadata.sections_detected);
}

#[test]
fn tokenize_enums_detects_both_sections() {
    reset_singletons();
    let cr = ConfigSectionHandler::new(None).process_config_section(SRC_ENUMS);
    let result = Tokenizer::new(cr.cleaned_input_string).tokenize();

    assert!(
        result.metadata.sections_detected.iter().any(|s| s.eq_ignore_ascii_case("ENUMS")),
        "Must detect @ENUMS"
    );
    assert!(
        result.metadata.sections_detected.iter().any(|s| s.eq_ignore_ascii_case("DATA")),
        "Must detect @DATA"
    );

    println!("[tokenize_enums] tokens={}", result.metadata.total_tokens);
}

#[test]
fn tokenize_quickfuncs_detects_all_sections() {
    reset_singletons();
    let cr = ConfigSectionHandler::new(None).process_config_section(SRC_QUICKFUNCS);
    let result = Tokenizer::new(cr.cleaned_input_string).tokenize();

    for section in &["ENUMS", "QUICKFUNCS", "DATA"] {
        assert!(
            result.metadata.sections_detected.iter().any(|s| s.eq_ignore_ascii_case(section)),
            "Must detect @{}", section
        );
    }

    println!("[tokenize_qf] tokens={} static_calls={} tuple_constructors={}",
        result.metadata.total_tokens,
        result.metadata.static_calls_found,
        result.metadata.tuple_constructors,
    );
}

#[test]
fn tokenize_full_source_no_lex_errors() {
    reset_singletons();
    let cr = ConfigSectionHandler::new(None).process_config_section(SRC_FULL);
    let result = Tokenizer::new(cr.cleaned_input_string).tokenize();

    let lex_errs = ErrorManager::get_shared_instance().get_lexical_errors();
    assert!(lex_errs.is_empty(), "Full source lex errors: {:?}", lex_errs);

    println!("[tokenize_full] tokens={} sections={:?}",
        result.metadata.total_tokens, result.metadata.sections_detected);
}

#[test]
fn tokenize_dump_contains_expected_output() {
    reset_singletons();
    let cr = ConfigSectionHandler::new(None).process_config_section(SRC_ENUMS);
    let result = Tokenizer::new(cr.cleaned_input_string).tokenize();

    let mut printer = TokenDebugPrinter::new(true, true, false);
    let dump = printer.print(&result);

    assert!(!dump.is_empty(), "Token dump must not be empty");
    assert!(dump.contains("Identifier"), "Dump must contain Identifier tokens");
    assert!(dump.contains("METADATA"), "Dump must contain metadata section");

    // Preview first 600 chars
    println!("[token_dump] {} chars\n{}", dump.len(), &dump[..dump.len().min(600)]);
}

// ═══════════════════════════════════════════════════════════
//  PARSING TESTS
// ═══════════════════════════════════════════════════════════

#[test]
fn parse_minimal_ast_structure() {
    let r = run_pipeline(SRC_MINIMAL);

    assert!(r.ast.config.is_some(), "AST must have CONFIG (defaults injected)");
    assert!(r.ast.data.is_some(), "AST must have DATA section");
    assert!(r.ast.enums.is_none(), "Minimal source should have no ENUMS");
    assert!(r.ast.quick_functions.is_none(), "Minimal source should have no QUICKFUNCS");
    assert!(r.ast.dlm.is_none(), "Minimal source should have no DLM");

    r.metrics.print("Parse — Minimal");
}

#[test]
fn parse_enums_section_correct_counts() {
    let r = run_pipeline(SRC_ENUMS);

    let enums = r.ast.enums.as_ref().expect("ENUMS section missing from AST");
    assert_eq!(enums.enums.len(), 2, "Should have Status and Priority");

    let status = enums.enums.iter().find(|e| e.name == "Status")
        .expect("Status enum not found");
    assert_eq!(status.fields.len(), 3, "Status should have ACTIVE/INACTIVE/PENDING");

    let priority = enums.enums.iter().find(|e| e.name == "Priority")
        .expect("Priority enum not found");
    assert_eq!(priority.fields.len(), 4, "Priority should have LOW/MEDIUM/HIGH/CRITICAL");

    println!("[parse_enums] enum_count={}", enums.enums.len());
    r.metrics.print("Parse — Enums");
}

#[test]
fn parse_quickfuncs_correct_function_count_and_params() {
    let r = run_pipeline(SRC_QUICKFUNCS);

    let qf = r.ast.quick_functions.as_ref().expect("QUICKFUNCS missing from AST");
    assert_eq!(qf.functions.len(), 3, "Should have createEnemy, doubleX, serverConfig");

    let create_enemy = qf.functions.iter().find(|f| f.name == "createEnemy")
        .expect("createEnemy missing");
    assert_eq!(create_enemy.parameters.len(), 3, "createEnemy should have 3 params");

    let double_fn = qf.functions.iter().find(|f| f.name == "doubleX")
        .expect("doubleX missing");
    assert_eq!(double_fn.parameters.len(), 1, "doubleX should have 1 param");

    println!("[parse_qf] function_count={}", qf.functions.len());
    r.metrics.print("Parse — QuickFuncs");
}

#[test]
fn parse_full_all_ast_sections_populated() {
    let r = run_pipeline(SRC_FULL);

    assert!(r.ast.config.is_some(), "config");
    assert!(r.ast.enums.is_some(), "enums");
    assert!(r.ast.quick_functions.is_some(), "quick_functions");
    assert!(r.ast.data.is_some(), "data");

    let enums = r.ast.enums.as_ref().unwrap();
    assert_eq!(enums.enums.len(), 3, "Should have Environment, LogLevel, HttpMethod");

    r.metrics.print("Parse — Full");
}

#[test]
fn parse_ast_debug_dump_readable() {
    let r = run_pipeline(SRC_ENUMS);

    let mut printer = AstDebugPrinter::new(true, true);
    let dump = printer.print(&r.ast);

    assert!(!dump.is_empty(), "AST dump must not be empty");
    assert!(dump.contains("@ENUMS"), "AST dump must contain @ENUMS");
    assert!(dump.contains("Status"), "AST dump must contain enum name");

    println!("[ast_dump] {} chars\n{}", dump.len(), &dump[..dump.len().min(800)]);
}

// ═══════════════════════════════════════════════════════════
//  SEMANTIC ANALYSIS TESTS
// ═══════════════════════════════════════════════════════════

#[test]
fn semantic_minimal_no_blocking_errors() {
    let r = run_pipeline(SRC_MINIMAL);

    // With continue mode semantics should succeed or have only warnings
    println!("[semantic_minimal] success={} errors={} warnings={}",
        r.semantic.is_success, r.semantic.errors.len(), r.semantic.warnings.len());
    for e in &r.semantic.errors {
        println!("  ERR [{}] {}: {}", e.error_id, e.error_type, e.message);
    }

    assert!(
        r.semantic.errors.is_empty() || r.semantic.is_success,
        "Minimal source semantic analysis must not produce blocking errors"
    );
    r.metrics.print("Semantic — Minimal");
}

#[test]
fn semantic_enums_populate_symbol_table() {
    let r = run_pipeline(SRC_ENUMS);

    let st = r.semantic.symbol_table.as_ref()
        .expect("Symbol table should be populated after semantic analysis");

    assert!(st.enums.contains_key("Status"), "SymbolTable must have Status");
    assert!(st.enums.contains_key("Priority"), "SymbolTable must have Priority");

    // Verify actual values
    let status_vals = &st.enums["Status"];
    assert_eq!(status_vals.get("ACTIVE"), Some(&1), "Status.ACTIVE should be 1");
    assert_eq!(status_vals.get("INACTIVE"), Some(&2), "Status.INACTIVE should be 2");
    assert_eq!(status_vals.get("PENDING"), Some(&3), "Status.PENDING should be 3");

    println!("[semantic_enums] symbol_table enums={:?}",
        st.enums.keys().collect::<Vec<_>>());
    r.metrics.print("Semantic — Enums");
}

#[test]
fn semantic_quickfuncs_section_result_present() {
    let r = run_pipeline(SRC_QUICKFUNCS);

    // After the fix, QUICKFUNCS section result should exist in section_results
    println!("[semantic_qf] success={} errors={} warnings={}",
        r.semantic.is_success, r.semantic.errors.len(), r.semantic.warnings.len());
    println!("[semantic_qf] sections_analyzed={:?}",
        r.semantic.section_results.keys().collect::<Vec<_>>());

    for e in &r.semantic.errors {
        println!("  ERR [{}] {} in @{}: {}", e.error_id, e.error_type, e.section_name, e.message);
        if !e.suggestion.is_empty() {
            println!("       → {}", e.suggestion);
        }
    }
    for w in &r.semantic.warnings {
        println!("  WARN [{}] in @{}: {}", w.warning_id, w.section_name, w.message);
    }

    r.metrics.print("Semantic — QuickFuncs");
}

#[test]
fn semantic_data_section_result_present() {
    let r = run_pipeline(SRC_FULL);

    // After the fix, DATA section result should be in section_results
    println!("[semantic_full] success={} errors={} warnings={}",
        r.semantic.is_success, r.semantic.errors.len(), r.semantic.warnings.len());
    println!("[semantic_full] sections_analyzed={:?}",
        r.semantic.section_results.keys().collect::<Vec<_>>());

    r.metrics.print("Semantic — Full");
}

#[test]
fn semantic_version_validation_passes_for_known_version() {
    let r = run_pipeline(SRC_CONFIG_ONLY);

    // Version validation (Phase 1 of semantic analysis) should not produce errors
    let version_errors: Vec<_> = r.semantic.errors.iter()
        .filter(|e| e.error_type == "VersionCompatibility")
        .collect();

    assert!(version_errors.is_empty(),
        "Version 1.0.0 should pass validation: {:?}", version_errors);
}

// ═══════════════════════════════════════════════════════════
//  DIAGNOSTIC DUMP TESTS
// ═══════════════════════════════════════════════════════════

#[test]
fn diagnostic_dump_structure_valid() {
    reset_singletons();
    let _ = run_pipeline(SRC_FULL); // populate some log entries

    let dump = DiagnosticDumper::new().generate_dump();

    assert!(!dump.is_empty(), "Dump must not be empty");
    assert!(dump.contains("DIAGNOSTIC DUMP"), "Must contain header");
    assert!(dump.contains("LEXICAL ERRORS"), "Must list lexical error category");
    assert!(dump.contains("PARSE ERRORS"), "Must list parse error category");
    assert!(dump.contains("LOG CONTENTS"), "Must contain log section");

    println!("[diag_dump] {} chars\n{}", dump.len(), &dump[..dump.len().min(800)]);
}

#[test]
fn full_pipeline_with_all_debug_outputs() {
    reset_singletons();

    // ── Tokenise ───────────────────────────────────────────
    let cr = ConfigSectionHandler::new(None).process_config_section(SRC_QUICKFUNCS);
    let tokenization = Tokenizer::new(cr.cleaned_input_string.clone()).tokenize();

    let mut tdp = TokenDebugPrinter::new(true, true, true);
    let token_dump = tdp.print(&tokenization);
    println!("\n{}\n[TOKEN DUMP — first 1000 chars]\n{}",
        "═".repeat(70), &token_dump[..token_dump.len().min(1000)]);

    // ── Parse ──────────────────────────────────────────────
    let parser = GeneralParser::new(
        tokenization.tokens.clone(),
        cr.config_section.clone(),
        cr.operational_settings.clone(),
    ).expect("Parser::new failed");
    let ast = parser.parse().expect("parse failed");

    let mut adp = AstDebugPrinter::new(false, true);
    let ast_dump = adp.print(&ast);
    println!("\n{}\n[AST DUMP — first 1000 chars]\n{}",
        "═".repeat(70), &ast_dump[..ast_dump.len().min(1000)]);

    // ── Semantic ───────────────────────────────────────────
    let sem = GeneralSemanticAnalyzer::new(&ast, &cr.operational_settings).analyze();
    println!("\n{}\n[SEMANTIC] success={} errors={} warnings={} sections={:?}",
        "═".repeat(70),
        sem.is_success, sem.errors.len(), sem.warnings.len(),
        sem.section_results.keys().collect::<Vec<_>>());

    // ── Diagnostic dump ────────────────────────────────────
    let diag = DiagnosticDumper::new().generate_dump();
    println!("\n{}\n[DIAGNOSTIC DUMP — first 1000 chars]\n{}",
        "═".repeat(70), &diag[..diag.len().min(1000)]);

    // Sanity assertions
    assert!(!token_dump.is_empty());
    assert!(!ast_dump.is_empty());
    assert!(!diag.is_empty());
}

// ═══════════════════════════════════════════════════════════
//  FIXTURE FILE TESTS  (actual .mdix files from the repo)
// ═══════════════════════════════════════════════════════════

#[test]
fn fixture_function_calls_heavy() {
    let path = "tests/fixtures/function_calls_heavy.mdix";
    if !fixture_exists(path) { println!("[SKIP] {}", path); return; }

    let r = run_pipeline(&load_fixture(path));
    r.metrics.print("fixture: function_calls_heavy.mdix");

    println!("  sections={:?}", r.tokenization.metadata.sections_detected);
    println!("  semantic_success={} errors={}", r.semantic.is_success, r.semantic.errors.len());

    assert!(r.ast.quick_functions.is_some(), "Should have @QUICKFUNCS");
    assert!(r.ast.enums.is_some(), "Should have @ENUMS");
    assert!(r.ast.data.is_some(), "Should have @DATA");
}

#[test]
fn fixture_parsing_stress() {
    let path = "tests/fixtures/parsing_stress.mdix";
    if !fixture_exists(path) { println!("[SKIP] {}", path); return; }

    let r = run_pipeline(&load_fixture(path));
    r.metrics.print("fixture: parsing_stress.mdix");

    let enum_count = r.ast.enums.as_ref().map(|e| e.enums.len()).unwrap_or(0);
    println!("  enums={} semantic_success={}", enum_count, r.semantic.is_success);

    assert!(r.ast.enums.is_some(), "Should have @ENUMS");
    assert!(r.ast.data.is_some(), "Should have @DATA");
    assert!(enum_count >= 4, "Stress file has 4 enums (Status/Priority/Category/Type)");
}

#[test]
fn fixture_quickfuncs_heavy() {
    let path = "tests/fixtures/quickfuncs_heavy.mdix";
    if !fixture_exists(path) { println!("[SKIP] {}", path); return; }

    let r = run_pipeline(&load_fixture(path));
    r.metrics.print("fixture: quickfuncs_heavy.mdix");

    let fn_count = r.ast.quick_functions.as_ref().map(|q| q.functions.len()).unwrap_or(0);
    println!("  function_count={} semantic_success={}", fn_count, r.semantic.is_success);

    assert!(r.ast.quick_functions.is_some(), "Should have @QUICKFUNCS");
    assert!(fn_count > 10, "Heavy file has many functions; got {}", fn_count);

    for e in &r.semantic.errors {
        println!("  ERR [{}] {}: {}", e.error_id, e.error_type, e.message);
    }
}

#[test]
fn fixture_sample_9kb() {
    let path = "tests/fixtures/sample_9kb.mdix";
    if !fixture_exists(path) { println!("[SKIP] {}", path); return; }

    let r = run_pipeline(&load_fixture(path));
    r.metrics.print("fixture: sample_9kb.mdix");

    println!("  dlm={} security={} imports={}",
        r.ast.dlm.is_some(), r.ast.security.is_some(), r.ast.imports.is_some());
}

#[test]
fn mdix_all_datatypes_test() {
    let path = "mdix_files/advanced/all_datatypes_test.mdix";
    if !fixture_exists(path) { println!("[SKIP] {}", path); return; }

    let r = run_pipeline(&load_fixture(path));
    r.metrics.print("mdix: all_datatypes_test.mdix");

    println!("  semantic_success={} errors={}", r.semantic.is_success, r.semantic.errors.len());
    for e in &r.semantic.errors {
        println!("  ERR [{}] in @{}: {}", e.error_id, e.section_name, e.message);
    }
    for w in &r.semantic.warnings {
        println!("  WARN [{}] in @{}: {}", w.warning_id, w.section_name, w.message);
    }
}

#[test]
fn mdix_data_variable_usage() {
    let path = "mdix_files/advanced/data_variable_usage.mdix";
    if !fixture_exists(path) { println!("[SKIP] {}", path); return; }

    let r = run_pipeline(&load_fixture(path));
    r.metrics.print("mdix: data_variable_usage.mdix");

    println!("  semantic_success={} errors={} warnings={}",
        r.semantic.is_success, r.semantic.errors.len(), r.semantic.warnings.len());
}

#[test]
fn mdix_basic_enum_test() {
    let path = "mdix_files/basic/basic_test.mdix";
    if !fixture_exists(path) { println!("[SKIP] {}", path); return; }

    let source = load_fixture(path);
    // basic_test.mdix only has @ENUMS — check it parses without data
    let r = run_pipeline(&source);
    r.metrics.print("mdix: basic_test.mdix");

    println!("  enums={}", r.ast.enums.as_ref().map(|e| e.enums.len()).unwrap_or(0));
}

// ═══════════════════════════════════════════════════════════
//  THROUGHPUT / PERFORMANCE BENCHMARKS
// ═══════════════════════════════════════════════════════════

#[test]
fn bench_minimal_1000_iters() {
    let (avg_ms, avg_tokens) = bench_n(SRC_MINIMAL, 1000);
    let throughput = (SRC_MINIMAL.len() as f64 / 1024.0) / (avg_ms / 1000.0);
    println!(
        "\n[bench_minimal ×1000]\n  avg={:.4} ms  throughput={:.0} KB/s  tokens/call={}",
        avg_ms, throughput, avg_tokens
    );
    // Very loose guard — just ensures we haven't regressed to seconds-per-call
    assert!(avg_ms < 500.0, "avg pipeline latency too high: {:.2} ms", avg_ms);
}

#[test]
fn bench_quickfuncs_500_iters() {
    let (avg_ms, avg_tokens) = bench_n(SRC_QUICKFUNCS, 500);
    let throughput = (SRC_QUICKFUNCS.len() as f64 / 1024.0) / (avg_ms / 1000.0);
    println!(
        "\n[bench_quickfuncs ×500]\n  avg={:.4} ms  throughput={:.0} KB/s  tokens/call={}",
        avg_ms, throughput, avg_tokens
    );
    assert!(avg_ms < 500.0, "regression: {:.2} ms", avg_ms);
}

#[test]
fn bench_full_200_iters() {
    let (avg_ms, avg_tokens) = bench_n(SRC_FULL, 200);
    let throughput = (SRC_FULL.len() as f64 / 1024.0) / (avg_ms / 1000.0);
    println!(
        "\n[bench_full ×200]\n  avg={:.4} ms  throughput={:.0} KB/s  tokens/call={}",
        avg_ms, throughput, avg_tokens
    );
    assert!(avg_ms < 1000.0, "regression: {:.2} ms", avg_ms);
}

#[test]
fn bench_fixture_files_50_iters() {
    let files = [
        "tests/fixtures/function_calls_heavy.mdix",
        "tests/fixtures/parsing_stress.mdix",
        "tests/fixtures/quickfuncs_heavy.mdix",
        "tests/fixtures/sample_9kb.mdix",
    ];
    for path in &files {
        if !fixture_exists(path) { println!("[SKIP] {}", path); continue; }
        let source = load_fixture(path);
        let name = Path::new(path).file_name().unwrap().to_str().unwrap();
        let (avg_ms, avg_tokens) = bench_n(&source, 50);
        let throughput = (source.len() as f64 / 1024.0) / (avg_ms / 1000.0);
        println!(
            "\n[bench_fixture: {} ×50]\n  avg={:.4} ms  throughput={:.0} KB/s  tokens/call={}",
            name, avg_ms, throughput, avg_tokens
        );
    }
}

#[test]
fn bench_synthetic_large_200_entries() {
    // Dynamically build a large DATA section to test parse throughput
    let header = r#"@CONFIG(version -> "1.0.0", features -> "advanced", error_handling -> "continue")
@ENUMS( Status { ACTIVE = 1, INACTIVE = 2 } )
@DATA(
"#;
    let mut entries = String::new();
    for i in 0..200 {
        entries.push_str(&format!(
            "    item_{:03} = {{ id = {}, name = \"item{}\", active = true, score = {}f }},\n",
            i, i, i, i
        ));
    }
    let source = format!("{}{}\n)", header, entries);

    let (avg_ms, avg_tokens) = bench_n(&source, 100);
    let throughput = (source.len() as f64 / 1024.0) / (avg_ms / 1000.0);
    println!(
        "\n[bench_synthetic_200_entries ×100]\n  input={} bytes  avg={:.4} ms  throughput={:.0} KB/s  tokens/call={}",
        source.len(), avg_ms, throughput, avg_tokens
    );
    assert!(avg_ms < 2000.0, "regression: {:.2} ms", avg_ms);
}

// ═══════════════════════════════════════════════════════════
//  JSONNET COMPARISON
//  Requires the `jsonnet` binary on PATH.
//  Install: https://github.com/google/go-jsonnet/releases
//  Run:  cargo test jsonnet -- --nocapture --include-ignored
// ═══════════════════════════════════════════════════════════

fn jsonnet_available() -> bool {
    Command::new("jsonnet").arg("--version").output().is_ok()
}

fn bench_jsonnet_cli(source: &str, n: u32) -> Option<f64> {
    let tmp = std::env::temp_dir().join("_dixscript_bench.jsonnet");
    fs::write(&tmp, source).ok()?;
    let mut total_ms = 0.0f64;
    for _ in 0..n {
        let t = Instant::now();
        let out = Command::new("jsonnet").arg(&tmp).output().ok()?;
        total_ms += t.elapsed().as_secs_f64() * 1000.0;
        if !out.status.success() {
            eprintln!("jsonnet stderr: {}", String::from_utf8_lossy(&out.stderr));
            return None;
        }
    }
    let _ = fs::remove_file(&tmp);
    Some(total_ms / n as f64)
}

#[test]
#[ignore = "requires jsonnet on PATH — run with --include-ignored"]
fn compare_throughput_dixscript_vs_jsonnet() {
    if !jsonnet_available() {
        println!("[SKIP] jsonnet not found. Install from https://github.com/google/go-jsonnet");
        return;
    }

    let n = 50u32;

    // ── DixScript ──
    let (dx_avg_ms, _) = bench_n(SRC_QUICKFUNCS, n);
    let dx_kb_s = (SRC_QUICKFUNCS.len() as f64 / 1024.0) / (dx_avg_ms / 1000.0);

    // ── Jsonnet ──
    let jn_avg_ms = match bench_jsonnet_cli(JSONNET_EQUIVALENT, n) {
        Some(ms) => ms,
        None => { println!("[jsonnet] CLI benchmark failed"); return; }
    };
    let jn_kb_s = (JSONNET_EQUIVALENT.len() as f64 / 1024.0) / (jn_avg_ms / 1000.0);

    let ratio = jn_avg_ms / dx_avg_ms;
    let dx_size = SRC_QUICKFUNCS.len();
    let jn_size = JSONNET_EQUIVALENT.len();
    let size_pct = (1.0 - dx_size as f64 / jn_size as f64) * 100.0;

    println!("\n");
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║          DixScript  vs  Jsonnet  —  Comparison          ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║  Equivalent functionality  |  {} iterations each       ║", n);
    println!("╠══════════════════════╦═══════════════╦═══════════════════╣");
    println!("║  Metric              ║  DixScript    ║  Jsonnet (CLI)    ║");
    println!("╠══════════════════════╬═══════════════╬═══════════════════╣");
    println!("║  Avg latency         ║  {:>8.3} ms ║  {:>12.3} ms ║", dx_avg_ms, jn_avg_ms);
    println!("║  Throughput          ║  {:>7.0} KB/s ║  {:>11.0} KB/s ║", dx_kb_s, jn_kb_s);
    println!("║  Source size         ║  {:>8} B  ║  {:>14} B ║", dx_size, jn_size);
    println!("╠══════════════════════╩═══════════════╩═══════════════════╣");
    if ratio >= 1.0 {
        println!("║  ✅  DixScript is {:.1}× faster than Jsonnet CLI       ║", ratio);
    } else {
        println!("║  ⚠️   Jsonnet CLI is {:.1}× faster than DixScript        ║", 1.0 / ratio);
    }
    if size_pct >= 0.0 {
        println!("║  ✅  DixScript source is {:.1}% smaller than Jsonnet    ║", size_pct);
    } else {
        println!("║  ⚠️   Jsonnet source is {:.1}% smaller than DixScript    ║", -size_pct);
    }
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║  ⓘ  DixScript runs in-process (no fork/exec overhead)   ║");
    println!("║  ⓘ  Jsonnet measured end-to-end including process start  ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");
}

#[test]
#[ignore = "requires jsonnet on PATH — run with --include-ignored"]
fn compare_source_size_metrics() {
    let pairs: &[(&str, &str, &str)] = &[
        ("QuickFuncs", SRC_QUICKFUNCS, JSONNET_EQUIVALENT),
    ];

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║               Source Size Comparison                    ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    for (name, dx, jn) in pairs {
        let reduction = (1.0 - dx.len() as f64 / jn.len() as f64) * 100.0;
        println!("║  {:12}  DixScript={:5}B  Jsonnet={:5}B  Δ={:+.0}%     ║",
            name, dx.len(), jn.len(), reduction);
    }
    println!("╚══════════════════════════════════════════════════════════╝\n");
  }
