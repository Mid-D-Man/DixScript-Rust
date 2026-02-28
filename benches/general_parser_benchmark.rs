// benches/general_parser_benchmark.rs
//! General Parser Benchmark — DixScript v1.0.0
//!
//! Three benchmark groups:
//!
//! 1. `section_parsers`   — each section in isolation (speed + throughput).
//! 2. `combined_sections` — all sections together via GeneralParser (small / medium / large).
//! 3. `pipeline_e2e`      — full front-end: ConfigHandler → Tokenizer → GeneralParser.

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
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType, Tokenizer};
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
    Utils    from "common/utils.mdix",
    Shared   from "shared/config.mdix",
    Helpers  from "utils/helpers.mdix",
    Enums    from "shared/enums.mdix"
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
{ENUMS_LARGE}
{QUICKFUNCS_COMPLEX}
{data_section}
"#,
        ENUMS_LARGE = &ENUMS_LARGE[..ENUMS_LARGE.rfind("@DATA").unwrap_or(ENUMS_LARGE.len())],
        QUICKFUNCS_COMPLEX = &QUICKFUNCS_COMPLEX
            [..QUICKFUNCS_COMPLEX.rfind("@DATA").unwrap_or(QUICKFUNCS_COMPLEX.len())],
        data_section = data_section,
    )
}

// =============================================================================
// Helpers
// =============================================================================

fn tokenize_input(input: &str, settings: &OperationalSettings) -> Vec<Token> {
    Tokenizer::new(input, settings).tokenize().tokens
}

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
        .min(all_tokens.len() - 1);

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
    let last_col = section_tokens.last().map(|t| t.column + 1).unwrap_or(1);
    section_tokens.push(Token::eof(last_line, last_col));
    section_tokens
}

fn run_config_handler(input: &str) -> (ConfigSection, String, OperationalSettings) {
    let mut handler = ConfigSectionHandler::new(None);
    let r = handler.process_config_section(input);
    (r.config_section, r.cleaned_input_string, r.operational_settings)
}

// =============================================================================
// Benchmark 1 — individual section parsers
//
// Tokens are pre-built once outside the timed loop.
// Section parsers receive &[Token] slices so zero allocation in the hot path.
// =============================================================================

fn bench_section_parsers(c: &mut Criterion) {
    let mut group = c.benchmark_group("section_parsers");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(100);

    let settings = OperationalSettings::default();

    // ── @CONFIG ──────────────────────────────────────────────────────────────
    for (label, input) in &[("full_7keys", CONFIG_FULL), ("partial_2keys", CONFIG_PARTIAL)] {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::new("config", label), input, |b, s| {
            b.iter(|| {
                let mut h = ConfigSectionHandler::new(None);
                black_box(h.process_config_section(black_box(s)))
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
        let data_src = generate_data_section(*n_props);
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
// GeneralParser owns Vec<Token> and comment-filters it in new().
// Use iter_batched so the clone happens in the (unmeasured) setup phase,
// not inside the timed routine.  This isolates pure parse cost.
// =============================================================================

fn bench_combined_sections(c: &mut Criterion) {
    let mut group = c.benchmark_group("combined_sections");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(80);

    let settings = OperationalSettings::default();

    // ── Small (CONFIG + 2 ENUMS + 2 QF + small DATA) ─────────────────────────
    {
        let (cfg, cleaned, _) = run_config_handler(COMBINED_SMALL);
        let toks = tokenize_input(&cleaned, &settings);
        group.throughput(Throughput::Bytes(COMBINED_SMALL.len() as u64));
        group.bench_function("all_sections_small", |b| {
            b.iter_batched(
                || toks.clone(),
                |t| {
                    let p = GeneralParser::new(black_box(t), black_box(&cfg), &settings)
                        .expect("parser init");
                    black_box(p.parse())
                },
                BatchSize::SmallInput,
            );
        });
    }

    // ── Medium (all sections, 150-prop DATA) ──────────────────────────────────
    {
        let medium_src = build_combined_input(150);
        let (cfg, cleaned, _) = run_config_handler(&medium_src);
        let toks = tokenize_input(&cleaned, &settings);
        group.throughput(Throughput::Bytes(medium_src.len() as u64));
        group.bench_function("all_sections_medium_150props", |b| {
            b.iter_batched(
                || toks.clone(),
                |t| {
                    let p = GeneralParser::new(black_box(t), black_box(&cfg), &settings)
                        .expect("parser init");
                    black_box(p.parse())
                },
                BatchSize::SmallInput,
            );
        });
    }

    // ── Large (all sections, 500-prop DATA) ───────────────────────────────────
    {
        let large_src = build_combined_input(500);
        let (cfg, cleaned, _) = run_config_handler(&large_src);
        let toks = tokenize_input(&cleaned, &settings);
        group.throughput(Throughput::Bytes(large_src.len() as u64));
        group.bench_function("all_sections_large_500props", |b| {
            b.iter_batched(
                || toks.clone(),
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
// Benchmark 3 — full end-to-end pipeline
//
// Measures the complete front-end cost a caller sees:
//   ConfigSectionHandler  (extract + validate @CONFIG)
//   → Tokenizer           (lex remaining source)
//   → GeneralParser       (parse all sections)
//
// The pipeline allocates fresh strings and vecs on every call, so b.iter
// is correct here — there is nothing to hoist out of the loop.
// The tokenize-only sub-benchmarks use b.iter for the same reason.
// =============================================================================

fn bench_pipeline_e2e(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_e2e");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(80);

    let settings = OperationalSettings::default();

    // ── Tokenize-only baselines ───────────────────────────────────────────────
    for (label, src) in &[("small", COMBINED_SMALL), ("medium", QUICKFUNCS_COMPLEX)] {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("phase_tokenize_only", label),
            src,
            |b, s| {
                b.iter(|| black_box(Tokenizer::new(black_box(s), &settings).tokenize()));
            },
        );
    }

    // ── Full pipeline: small source ───────────────────────────────────────────
    group.throughput(Throughput::Bytes(COMBINED_SMALL.len() as u64));
    group.bench_function("full_pipeline_small", |b| {
        b.iter(|| {
            let mut handler = ConfigSectionHandler::new(None);
            let cfg_result = handler.process_config_section(black_box(COMBINED_SMALL));

            let s_local = cfg_result.operational_settings.clone();
            let tok_result =
                Tokenizer::new(&cfg_result.cleaned_input_string, &s_local).tokenize();

            let parser = GeneralParser::new(
                tok_result.tokens,
                &cfg_result.config_section,
                &s_local,
            )
            .expect("parser init");
            black_box(parser.parse())
        });
    });

    // ── Full pipeline: medium source (150-prop DATA) ───────────────────────────
    {
        let medium_src = build_combined_input(150);
        group.throughput(Throughput::Bytes(medium_src.len() as u64));
        group.bench_function("full_pipeline_medium", |b| {
            b.iter(|| {
                let mut handler = ConfigSectionHandler::new(None);
                let cfg_result = handler.process_config_section(black_box(&medium_src));

                let s_local = cfg_result.operational_settings.clone();
                let tok_result =
                    Tokenizer::new(&cfg_result.cleaned_input_string, &s_local).tokenize();

                let parser = GeneralParser::new(
                    tok_result.tokens,
                    &cfg_result.config_section,
                    &s_local,
                )
                .expect("parser init");
                black_box(parser.parse())
            });
        });
    }

    // ── Real .mdix file (from disk) ───────────────────────────────────────────
    if let Ok(real_src) =
        std::fs::read_to_string("mdix_files/advanced/all_datatypes_test.mdix")
    {
        group.throughput(Throughput::Bytes(real_src.len() as u64));

        group.bench_function("real_file_tokenize_only", |b| {
            b.iter(|| black_box(Tokenizer::new(black_box(&real_src), &settings).tokenize()));
        });

        group.bench_function("real_file_full_pipeline", |b| {
            b.iter(|| {
                let mut handler = ConfigSectionHandler::new(None);
                let cfg_result = handler.process_config_section(black_box(&real_src));

                let s_local = cfg_result.operational_settings.clone();
                let tok_result =
                    Tokenizer::new(&cfg_result.cleaned_input_string, &s_local).tokenize();

                let parser = GeneralParser::new(
                    tok_result.tokens,
                    &cfg_result.config_section,
                    &s_local,
                )
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
