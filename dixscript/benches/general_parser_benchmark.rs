//! General Parser Benchmark — DixScript v1.0.0 (token-based pipeline)
//!
//! Pipeline order (mirroring DixLoader::compile_source):
//!   Stage 1: Tokenizer::new(source).tokenize()
//!   Stage 2: split_config_tokens(tokens)
//!   Stage 3: ConfigSectionHandler::process_config_tokens(&config_tokens)
//!   Stage 4: GeneralParser::new(rest_tokens, config_section, settings).parse()
//!
//! Three benchmark groups:
//!   1. section_parsers   — each section parser in isolation.
//!   2. combined_sections — all sections together via GeneralParser.
//!   3. pipeline_e2e      — full front-end with incremental stage breakdown.

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use dixscript::Compiler::AST::ConfigSection;
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::GeneralParser;
use dixscript::Compiler::Core::SectionParsers::{
    DataSectionParser, DlmSectionParser, EnumsSectionParser, ImportsSectionParser,
    QuickFuncsSectionParser, SecuritySectionParser,
};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Token, TokenType, Tokenizer};
use std::time::Duration;

// =============================================================================
// Static section inputs
// =============================================================================

const CONFIG_FULL: &str = r#"@CONFIG(
    version            -> "1.0.0",
    encoding           -> "utf-8",
    author             -> "BenchSuite",
    features           -> "advanced",
    debug_mode         -> "off",
    error_handling     -> "halt",
    compatibility_mode -> "strict"
)
@DATA( x = 1 )"#;

const CONFIG_PARTIAL: &str = r#"@CONFIG(
    author     -> "BenchSuite",
    debug_mode -> "verbose"
)
@DATA( x = 1 )"#;

const ENUMS_SMALL: &str = r#"@ENUMS(
    Status   { ACTIVE = 1, INACTIVE = 2, PENDING = 3 }
    LogLevel { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
)
@DATA( x = 1 )"#;

const ENUMS_LARGE: &str = r#"@ENUMS(
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

const QUICKFUNCS_SIMPLE: &str = r#"@QUICKFUNCS(
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

const QUICKFUNCS_COMPLEX: &str = r#"@QUICKFUNCS(
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

const DLM_INPUT: &str = r#"@DLM(
    DAuditor.enhanced,
    DCompressor.gzip,
    DEncryptor.aes256
)
@DATA( x = 1 )"#;

const IMPORTS_INPUT: &str = r#"@IMPORTS(
    Utils    from "common/utils.dixscript",
    Shared   from "shared/config.dixscript",
    Helpers  from "utils/helpers.dixscript",
    Enums    from "shared/enums.dixscript"
)
@DATA( x = 1 )"#;

const SECURITY_INPUT: &str = r#"@SECURITY(
    encryption -> { mode = "password", algorithm = "aes256-gcm" },
    keystore   -> { auto_generate = true, backup_count = 3 },
    validation -> { strict = true }
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
// Input generators
// =============================================================================

fn generate_data_section(properties: usize) -> String {
    let mut s = String::with_capacity(properties * 38);
    s.push_str("@DATA(\n");
    for i in 0..properties {
        match i % 8 {
            0 => s.push_str(&format!("    prop_{i}<int> = {i},\n")),
            1 => s.push_str(&format!("    str_{i} = \"value_{i}\",\n")),
            2 => s.push_str(&format!("    flag_{i} = {},\n", i % 2 == 0)),
            3 => s.push_str(&format!("    rate_{i}<float> = {i}.5f,\n")),
            4 => s.push_str(&format!("    arr_{i} = [{i}, {}, {}],\n", i + 1, i + 2)),
            5 => s.push_str(&format!(
                "    date_{i}<date> = 2025-01-{:02},\n",
                (i % 28) + 1
            )),
            6 => s.push_str(&format!(
                "    obj_{i} = {{ id = {i}, name = \"item_{i}\" }},\n"
            )),
            _ => s.push_str(&format!(
                "    ts_{i}<timestamp> = 2025-01-15T{:02}:00:00Z,\n",
                i % 24
            )),
        }
    }
    s.push_str(")\n");
    s
}

fn build_combined_input(data_props: usize) -> String {
    let data_section = generate_data_section(data_props);
    format!(
        r#"@CONFIG(
    version        -> "1.0.0",
    encoding       -> "utf-8",
    features       -> "advanced",
    error_handling -> "halt"
)
{ENUMS_BODY}
{QF_BODY}
{data_section}
"#,
        ENUMS_BODY = &ENUMS_LARGE[..ENUMS_LARGE.rfind("@DATA").unwrap_or(ENUMS_LARGE.len())],
        QF_BODY    = &QUICKFUNCS_COMPLEX
            [..QUICKFUNCS_COMPLEX.rfind("@DATA").unwrap_or(QUICKFUNCS_COMPLEX.len())],
        data_section = data_section,
    )
}

// =============================================================================
// Pipeline helpers
// =============================================================================

/// Tokenise `input` with default settings.
/// Used to build token slices for section-parser isolation benchmarks.
fn tokenize_input(input: &str, settings: &OperationalSettings) -> Vec<Token> {
    Tokenizer::new(input, settings).tokenize().tokens
}

/// Extract tokens belonging to one named section from a full token stream.
fn extract_section_tokens(all_tokens: &[Token], section_name: &str) -> Vec<Token> {
    let kw_pos = all_tokens
        .iter()
        .position(|t| {
            t.token_type
                .get_section_context()
                .map(|ctx| ctx.eq_ignore_ascii_case(section_name))
                .unwrap_or(false)
        })
        .unwrap_or(0);

    let open_pos = all_tokens[kw_pos..]
        .iter()
        .position(|t| matches!(t.token_type, TokenType::Symbol('(')))
        .map(|rel| kw_pos + rel)
        .unwrap_or(kw_pos + 1)
        .min(all_tokens.len().saturating_sub(1));

    let mut depth = 0i32;
    let mut close_pos = all_tokens.len().saturating_sub(1);
    for (i, tok) in all_tokens[open_pos..].iter().enumerate() {
        match &tok.token_type {
            TokenType::Symbol('(') => depth += 1,
            TokenType::Symbol(')') => {
                depth -= 1;
                if depth == 0 {
                    close_pos = open_pos + i;
                    break;
                }
            }
            _ => {}
        }
    }

    let mut section_tokens = all_tokens[open_pos..=close_pos].to_vec();
    let last_line = section_tokens.last().map(|t| t.line).unwrap_or(1);
    let last_col  = section_tokens.last().map(|t| t.column + 1).unwrap_or(1);
    section_tokens.push(Token::eof(last_line, last_col));
    section_tokens
}

/// Run the full config-extraction pipeline (stages 1-3) and return:
///   (ConfigSection, rest_tokens ready for GeneralParser, OperationalSettings)
///
/// Mirrors the flow in DixLoader::compile_source.
fn run_config_handler(input: &str) -> (ConfigSection, Vec<Token>, OperationalSettings) {
    let initial   = OperationalSettings::default();
    let tok_result = Tokenizer::new(input, &initial).tokenize();
    let split     = split_config_tokens(tok_result.tokens);
    let mut handler = ConfigSectionHandler::new(None);
    let cfg = handler.process_config_tokens(&split.config_tokens);
    (cfg.config_section, split.rest_tokens, cfg.operational_settings)
}

// =============================================================================
// Benchmark 1 — individual section parsers
//
// Tokens are pre-built once outside the timed loop.
// Section parsers receive &[Token] slices; zero allocation in hot path.
// =============================================================================

fn bench_section_parsers(c: &mut Criterion) {
    let mut group = c.benchmark_group("section_parsers");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(100);

    let settings = OperationalSettings::default();

    // ── @CONFIG ──────────────────────────────────────────────────────────────
    // These also include the tokenize + split stage to be realistic.
    for (label, input) in &[("full_7keys", CONFIG_FULL), ("partial_2keys", CONFIG_PARTIAL)] {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::new("config", label), input, |b, s| {
            b.iter(|| {
                let initial = OperationalSettings::default();
                let tok     = Tokenizer::new(black_box(s), &initial).tokenize();
                let split   = split_config_tokens(tok.tokens);
                let mut h   = ConfigSectionHandler::new(None);
                black_box(h.process_config_tokens(black_box(&split.config_tokens)))
            });
        });
    }

    // ── @ENUMS ───────────────────────────────────────────────────────────────
    let enums_small_toks =
        extract_section_tokens(&tokenize_input(ENUMS_SMALL, &settings), "ENUMS");
    let enums_large_toks =
        extract_section_tokens(&tokenize_input(ENUMS_LARGE, &settings), "ENUMS");

    group.throughput(Throughput::Bytes(ENUMS_SMALL.len() as u64));
    group.bench_function("enums_small_2decls", |b| {
        b.iter(|| {
            let mut p = EnumsSectionParser::new(black_box(&enums_small_toks), &settings);
            black_box(p.parse_section())
        });
    });

    group.throughput(Throughput::Bytes(ENUMS_LARGE.len() as u64));
    group.bench_function("enums_large_8decls", |b| {
        b.iter(|| {
            let mut p = EnumsSectionParser::new(black_box(&enums_large_toks), &settings);
            black_box(p.parse_section())
        });
    });

    // ── @QUICKFUNCS ──────────────────────────────────────────────────────────
    let qf_simple_toks =
        extract_section_tokens(&tokenize_input(QUICKFUNCS_SIMPLE, &settings), "QUICKFUNCS");
    let qf_complex_toks =
        extract_section_tokens(&tokenize_input(QUICKFUNCS_COMPLEX, &settings), "QUICKFUNCS");

    group.throughput(Throughput::Bytes(QUICKFUNCS_SIMPLE.len() as u64));
    group.bench_function("quickfuncs_simple_3fns", |b| {
        b.iter(|| {
            let mut p = QuickFuncsSectionParser::new(black_box(&qf_simple_toks), &settings);
            black_box(p.parse_section())
        });
    });

    group.throughput(Throughput::Bytes(QUICKFUNCS_COMPLEX.len() as u64));
    group.bench_function("quickfuncs_complex_8fns", |b| {
        b.iter(|| {
            let mut p = QuickFuncsSectionParser::new(black_box(&qf_complex_toks), &settings);
            black_box(p.parse_section())
        });
    });

    // ── @DATA (small / medium / large) ───────────────────────────────────────
    for (label, n_props) in &[("small_30", 30usize), ("medium_150", 150), ("large_500", 500)] {
        let data_src  = generate_data_section(*n_props);
        let data_toks = extract_section_tokens(&tokenize_input(&data_src, &settings), "DATA");
        let byte_count = data_src.len() as u64;

        group.throughput(Throughput::Bytes(byte_count));
        group.bench_with_input(
            BenchmarkId::new("data", label),
            &data_toks,
            |b, toks| {
                b.iter(|| {
                    let mut p = DataSectionParser::new(black_box(toks), &settings);
                    black_box(p.parse_section())
                });
            },
        );
    }

    // ── @DLM ─────────────────────────────────────────────────────────────────
    let dlm_toks = extract_section_tokens(&tokenize_input(DLM_INPUT, &settings), "DLM");
    group.throughput(Throughput::Bytes(DLM_INPUT.len() as u64));
    group.bench_function("dlm_3modules", |b| {
        b.iter(|| {
            let mut p = DlmSectionParser::new(black_box(&dlm_toks), &settings);
            black_box(p.parse_section())
        });
    });

    // ── @IMPORTS ─────────────────────────────────────────────────────────────
    let imports_toks =
        extract_section_tokens(&tokenize_input(IMPORTS_INPUT, &settings), "IMPORTS");
    group.throughput(Throughput::Bytes(IMPORTS_INPUT.len() as u64));
    group.bench_function("imports_4paths", |b| {
        b.iter(|| {
            let mut p = ImportsSectionParser::new(black_box(&imports_toks), &settings);
            black_box(p.parse_section())
        });
    });

    // ── @SECURITY ────────────────────────────────────────────────────────────
    let sec_toks =
        extract_section_tokens(&tokenize_input(SECURITY_INPUT, &settings), "SECURITY");
    group.throughput(Throughput::Bytes(SECURITY_INPUT.len() as u64));
    group.bench_function("security_3blocks", |b| {
        b.iter(|| {
            let mut p = SecuritySectionParser::new(black_box(&sec_toks), &settings);
            black_box(p.parse_section())
        });
    });

    group.finish();
}

// =============================================================================
// Benchmark 2 — all sections together via GeneralParser
//
// run_config_handler returns (ConfigSection, rest_tokens, settings).
// rest_tokens are fed directly into GeneralParser — no re-tokenisation needed.
// iter_batched clones rest_tokens in the (unmeasured) setup phase.
// =============================================================================

fn bench_combined_sections(c: &mut Criterion) {
    let mut group = c.benchmark_group("combined_sections");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(80);

    // ── Small: CONFIG + 2 ENUMS + 2 QF + tiny DATA ───────────────────────────
    {
        let (cfg, rest_toks, settings) = run_config_handler(COMBINED_SMALL);
        group.throughput(Throughput::Bytes(COMBINED_SMALL.len() as u64));
        group.bench_function("all_sections_small", |b| {
            b.iter_batched(
                || rest_toks.clone(),
                |t| {
                    let p = GeneralParser::new(black_box(t), black_box(&cfg), &settings)
                        .expect("parser init");
                    black_box(p.parse())
                },
                BatchSize::SmallInput,
            );
        });
    }

    // ── Medium: all sections, 150-prop DATA ───────────────────────────────────
    {
        let medium_src = build_combined_input(150);
        let (cfg, rest_toks, settings) = run_config_handler(&medium_src);
        group.throughput(Throughput::Bytes(medium_src.len() as u64));
        group.bench_function("all_sections_medium_150props", |b| {
            b.iter_batched(
                || rest_toks.clone(),
                |t| {
                    let p = GeneralParser::new(black_box(t), black_box(&cfg), &settings)
                        .expect("parser init");
                    black_box(p.parse())
                },
                BatchSize::SmallInput,
            );
        });
    }

    // ── Large: all sections, 500-prop DATA ────────────────────────────────────
    {
        let large_src = build_combined_input(500);
        let (cfg, rest_toks, settings) = run_config_handler(&large_src);
        group.throughput(Throughput::Bytes(large_src.len() as u64));
        group.bench_function("all_sections_large_500props", |b| {
            b.iter_batched(
                || rest_toks.clone(),
                |t| {
                    let p = GeneralParser::new(black_box(t), black_box(&cfg), &settings)
                        .expect("parser init");
                    black_box(p.parse())
                },
                BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 3 — full end-to-end pipeline with stage breakdown
//
// Incremental stages let you derive:
//   split_cost          = stage1_2 - stage1
//   config_cost         = stage1_2_3 - stage1_2
//   parse_cost          = full_pipeline - stage1_2_3
// =============================================================================

fn bench_pipeline_e2e(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_e2e");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(80);

    let initial = OperationalSettings::default();

    // ── Stage breakdown on COMBINED_SMALL ────────────────────────────────────

    group.throughput(Throughput::Bytes(COMBINED_SMALL.len() as u64));

    group.bench_function("stage1_tokenize_only/small", |b| {
        b.iter(|| {
            black_box(Tokenizer::new(black_box(COMBINED_SMALL), &initial).tokenize())
        });
    });

    group.bench_function("stage1_2_tokenize_and_split/small", |b| {
        b.iter(|| {
            let tok = Tokenizer::new(black_box(COMBINED_SMALL), &initial).tokenize();
            black_box(split_config_tokens(tok.tokens))
        });
    });

    group.bench_function("stage1_2_3_plus_config/small", |b| {
        b.iter(|| {
            let tok   = Tokenizer::new(black_box(COMBINED_SMALL), &initial).tokenize();
            let split = split_config_tokens(tok.tokens);
            let mut h = ConfigSectionHandler::new(None);
            black_box(h.process_config_tokens(&split.config_tokens))
        });
    });

    // ── Full pipeline: small source ───────────────────────────────────────────

    group.bench_function("full_pipeline_small", |b| {
        b.iter(|| {
            let tok   = Tokenizer::new(black_box(COMBINED_SMALL), &initial).tokenize();
            let split = split_config_tokens(tok.tokens);
            let mut h = ConfigSectionHandler::new(None);
            let cfg   = h.process_config_tokens(&split.config_tokens);
            let s     = cfg.operational_settings.clone();
            let parser = GeneralParser::new(split.rest_tokens, &cfg.config_section, &s)
                .expect("parser init");
            black_box(parser.parse())
        });
    });

    // ── Full pipeline: medium source (150-prop DATA) ──────────────────────────

    {
        let medium_src = build_combined_input(150);
        group.throughput(Throughput::Bytes(medium_src.len() as u64));

        group.bench_function("stage1_tokenize_only/medium", |b| {
            b.iter(|| {
                black_box(Tokenizer::new(black_box(&medium_src), &initial).tokenize())
            });
        });

        group.bench_function("stage1_2_tokenize_and_split/medium", |b| {
            b.iter(|| {
                let tok = Tokenizer::new(black_box(&medium_src), &initial).tokenize();
                black_box(split_config_tokens(tok.tokens))
            });
        });

        group.bench_function("full_pipeline_medium", |b| {
            b.iter(|| {
                let tok   = Tokenizer::new(black_box(&medium_src), &initial).tokenize();
                let split = split_config_tokens(tok.tokens);
                let mut h = ConfigSectionHandler::new(None);
                let cfg   = h.process_config_tokens(&split.config_tokens);
                let s     = cfg.operational_settings.clone();
                let parser = GeneralParser::new(split.rest_tokens, &cfg.config_section, &s)
                    .expect("parser init");
                black_box(parser.parse())
            });
        });
    }

    // ── Real .dixscript file (from disk, optional) ────────────────────────────

    if let Ok(real_src) =
        std::fs::read_to_string("../../mdix_files/advanced/all_datatypes_test.dixscript")
    {
        group.throughput(Throughput::Bytes(real_src.len() as u64));

        group.bench_function("real_file_tokenize_only", |b| {
            b.iter(|| {
                black_box(Tokenizer::new(black_box(&real_src), &initial).tokenize())
            });
        });

        group.bench_function("real_file_tokenize_and_split", |b| {
            b.iter(|| {
                let tok = Tokenizer::new(black_box(&real_src), &initial).tokenize();
                black_box(split_config_tokens(tok.tokens))
            });
        });

        group.bench_function("real_file_full_pipeline", |b| {
            b.iter(|| {
                let tok   = Tokenizer::new(black_box(&real_src), &initial).tokenize();
                let split = split_config_tokens(tok.tokens);
                let mut h = ConfigSectionHandler::new(None);
                let cfg   = h.process_config_tokens(&split.config_tokens);
                let s     = cfg.operational_settings.clone();
                let parser = GeneralParser::new(split.rest_tokens, &cfg.config_section, &s)
                    .expect("parser init");
                black_box(parser.parse())
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
    bench_section_parsers,
    bench_combined_sections,
    bench_pipeline_e2e,
);
criterion_main!(benches);
