//! Format Comparison Benchmark — DixScript vs JSON vs TOML
//!
//! Measures parse speed, full-pipeline throughput, and data-access latency
//! across three payload sizes (small / medium / large).
//!
//! Run:  cargo bench -p dixscript --bench format_comparison_benchmark
//!       cargo bench -p dixscript --bench format_comparison_benchmark -- --save-baseline base

use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::{GeneralAstEnhancer, GeneralParser, GeneralSemanticAnalyzer};
use std::time::Duration;

// ── Fixture sources loaded at compile time ────────────────────────────────────

const SMALL_MDIX: &str = include_str!("fixtures/comparison/small_config.mdix");
const SMALL_JSON: &str = include_str!("fixtures/comparison/small_config.json");
const SMALL_TOML: &str = include_str!("fixtures/comparison/small_config.toml");

const MEDIUM_MDIX: &str = include_str!("fixtures/comparison/medium_game_config.mdix");
const MEDIUM_JSON: &str = include_str!("fixtures/comparison/medium_game_config.json");
const MEDIUM_TOML: &str = include_str!("fixtures/comparison/medium_game_config.toml");

const LARGE_MDIX: &str = include_str!("fixtures/comparison/large_schema.mdix");
const LARGE_JSON: &str = include_str!("fixtures/comparison/large_schema.json");

// ── Size report printed once at startup ──────────────────────────────────────

fn print_size_report() {
    println!("\n╔══════════════════════════════════════════════════════╗");
    println!("║          FORMAT COMPARISON — SOURCE SIZES            ║");
    println!("╠══════════════════════════════════════════════════════╣");
    println!(
        "║  Small   │ mdix {:>6}B │ json {:>6}B │ toml {:>6}B ║",
        SMALL_MDIX.len(),
        SMALL_JSON.len(),
        SMALL_TOML.len()
    );
    println!(
        "║  Medium  │ mdix {:>6}B │ json {:>6}B │ toml {:>6}B ║",
        MEDIUM_MDIX.len(),
        MEDIUM_JSON.len(),
        MEDIUM_TOML.len()
    );
    println!(
        "║  Large   │ mdix {:>6}B │ json {:>6}B │                ║",
        LARGE_MDIX.len(),
        LARGE_JSON.len()
    );
    println!("╠══════════════════════════════════════════════════════╣");

    let small_ratio = SMALL_JSON.len() as f64 / SMALL_MDIX.len() as f64;
    let med_ratio = MEDIUM_JSON.len() as f64 / MEDIUM_MDIX.len() as f64;
    let large_ratio = LARGE_JSON.len() as f64 / LARGE_MDIX.len() as f64;

    println!(
        "║  JSON/DixScript size ratio: small={:.2}x  med={:.2}x  large={:.2}x ║",
        small_ratio, med_ratio, large_ratio
    );
    println!("╚══════════════════════════════════════════════════════╝\n");
}

// ── DixScript pipeline helpers ────────────────────────────────────────────────

/// Stage 1 only — tokenize full source (no splitting, no parsing).
#[inline]
fn mdix_tokenize_only(source: &str) -> usize {
    let settings = OperationalSettings::default();
    let tokenizer = Tokenizer::new(source, &settings);
    let result = tokenizer.tokenize();
    result.tokens.len()
}

/// Stages 1-2 — tokenize + split @CONFIG + process config tokens.
#[inline]
fn mdix_tokenize_and_split(source: &str) -> OperationalSettings {
    let settings = OperationalSettings::default();
    let tokenizer = Tokenizer::new(source, &settings);
    let tok = tokenizer.tokenize();
    let split = split_config_tokens(tok.tokens);
    let mut handler = ConfigSectionHandler::new(None);
    handler.process_config_tokens(&split.config_tokens).operational_settings
}

/// Stages 1-3 — full parse (tokenize + split + config + parse).
#[inline]
fn mdix_full_parse(source: &str) -> dixscript::Compiler::AST::DixScript {
    let initial = OperationalSettings::default();
    let tokenizer = Tokenizer::new(source, &initial);
    let tok = tokenizer.tokenize();
    let split = split_config_tokens(tok.tokens);
    let mut handler = ConfigSectionHandler::new(None);
    let cfg = handler.process_config_tokens(&split.config_tokens);
    let mut settings = cfg.operational_settings.clone();
    settings.skip_imports_resolution = true;

    let parser =
        GeneralParser::new(split.rest_tokens, &cfg.config_section, &settings).unwrap();
    parser.parse().unwrap_or_else(|_| dixscript::Compiler::AST::DixScript::new())
}

/// Stages 1-4 — full parse + semantic analysis.
#[inline]
fn mdix_parse_and_analyze(source: &str) -> bool {
    let ast = mdix_full_parse(source);
    let settings = OperationalSettings {
        skip_imports_resolution: true,
        ..OperationalSettings::default()
    };
    let analyzer = GeneralSemanticAnalyzer::new(&ast, &settings);
    analyzer.analyze().is_success
}

/// Stages 1-5 — full pipeline including AST enhancement.
#[inline]
fn mdix_full_pipeline(source: &str) -> bool {
    let ast = mdix_full_parse(source);
    let settings = OperationalSettings {
        skip_imports_resolution: true,
        ..OperationalSettings::default()
    };
    let analyzer = GeneralSemanticAnalyzer::new(&ast, &settings);
    let sem = analyzer.analyze();
    if !sem.is_success {
        return false;
    }
    let enhancer = GeneralAstEnhancer::new(&settings);
    enhancer.enhance(&ast, Some(&sem)).is_success
}

// ── Group 1: Tokenization-only throughput ─────────────────────────────────────

fn bench_tokenize_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("format/tokenize");
    group.measurement_time(Duration::from_secs(5));

    for (label, src) in [
        ("small",  SMALL_MDIX),
        ("medium", MEDIUM_MDIX),
        ("large",  LARGE_MDIX),
    ] {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("mdix_tokenize", label),
            src,
            |b, s| b.iter(|| mdix_tokenize_only(black_box(s))),
        );
    }
    group.finish();
}

// ── Group 2: Full parse comparison ───────────────────────────────────────────

fn bench_parse_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("format/parse");
    group.measurement_time(Duration::from_secs(8));

    // Small payload
    group.throughput(Throughput::Bytes(SMALL_MDIX.len() as u64));
    group.bench_function("mdix_small_full_parse", |b| {
        b.iter(|| mdix_full_parse(black_box(SMALL_MDIX)))
    });

    group.throughput(Throughput::Bytes(SMALL_JSON.len() as u64));
    group.bench_function("json_small_parse", |b| {
        b.iter(|| {
            let v: serde_json::Value = serde_json::from_str(black_box(SMALL_JSON)).unwrap();
            black_box(v)
        })
    });

    group.throughput(Throughput::Bytes(SMALL_TOML.len() as u64));
    group.bench_function("toml_small_parse", |b| {
        b.iter(|| {
            let v: toml::Value = toml::from_str(black_box(SMALL_TOML)).unwrap();
            black_box(v)
        })
    });

    // Medium payload
    group.throughput(Throughput::Bytes(MEDIUM_MDIX.len() as u64));
    group.bench_function("mdix_medium_full_parse", |b| {
        b.iter(|| mdix_full_parse(black_box(MEDIUM_MDIX)))
    });

    group.throughput(Throughput::Bytes(MEDIUM_JSON.len() as u64));
    group.bench_function("json_medium_parse", |b| {
        b.iter(|| {
            let v: serde_json::Value = serde_json::from_str(black_box(MEDIUM_JSON)).unwrap();
            black_box(v)
        })
    });

    group.throughput(Throughput::Bytes(MEDIUM_TOML.len() as u64));
    group.bench_function("toml_medium_parse", |b| {
        b.iter(|| {
            let v: toml::Value = toml::from_str(black_box(MEDIUM_TOML)).unwrap();
            black_box(v)
        })
    });

    // Large payload
    group.throughput(Throughput::Bytes(LARGE_MDIX.len() as u64));
    group.bench_function("mdix_large_full_parse", |b| {
        b.iter(|| mdix_full_parse(black_box(LARGE_MDIX)))
    });

    group.throughput(Throughput::Bytes(LARGE_JSON.len() as u64));
    group.bench_function("json_large_parse", |b| {
        b.iter(|| {
            let v: serde_json::Value = serde_json::from_str(black_box(LARGE_JSON)).unwrap();
            black_box(v)
        })
    });

    group.finish();
}

// ── Group 3: Full pipeline (DixScript compile vs JSON parse) ──────────────────

fn bench_full_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("format/full_pipeline");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);

    for (label, mdix_src, json_src, json_expanded_len) in [
        (
            "small",
            SMALL_MDIX,
            SMALL_JSON,
            SMALL_JSON.len(),
        ),
        (
            "medium",
            MEDIUM_MDIX,
            MEDIUM_JSON,
            // JSON here is the EXPANDED output equivalent; DixScript source is smaller
            MEDIUM_JSON.len(),
        ),
        (
            "large",
            LARGE_MDIX,
            LARGE_JSON,
            LARGE_JSON.len(),
        ),
    ] {
        // DixScript: compile from compact source
        group.throughput(Throughput::Bytes(mdix_src.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("mdix_compile_from_source", label),
            mdix_src,
            |b, s| b.iter(|| mdix_full_pipeline(black_box(s))),
        );

        // DixScript: tokenize + parse + semantic (no enhancement)
        group.bench_with_input(
            BenchmarkId::new("mdix_parse_and_analyze", label),
            mdix_src,
            |b, s| b.iter(|| mdix_parse_and_analyze(black_box(s))),
        );

        // JSON: parse equivalent expanded output
        group.throughput(Throughput::Bytes(json_expanded_len as u64));
        group.bench_with_input(
            BenchmarkId::new("json_parse_expanded", label),
            json_src,
            |b, s| {
                b.iter(|| {
                    let v: serde_json::Value = serde_json::from_str(black_box(s)).unwrap();
                    black_box(v)
                })
            },
        );
    }

    group.finish();
}

// ── Group 4: Pipeline stage breakdown ────────────────────────────────────────

fn bench_pipeline_stages(c: &mut Criterion) {
    let mut group = c.benchmark_group("format/pipeline_stages");
    group.measurement_time(Duration::from_secs(6));

    let payloads = [
        ("small",  SMALL_MDIX),
        ("medium", MEDIUM_MDIX),
        ("large",  LARGE_MDIX),
    ];

    for (label, src) in payloads {
        group.throughput(Throughput::Bytes(src.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("stage1_tokenize",       label), src,
            |b, s| b.iter(|| mdix_tokenize_only(black_box(s))),
        );
        group.bench_with_input(
            BenchmarkId::new("stage2_tokenize_split", label), src,
            |b, s| b.iter(|| mdix_tokenize_and_split(black_box(s))),
        );
        group.bench_with_input(
            BenchmarkId::new("stage3_full_parse",     label), src,
            |b, s| b.iter(|| mdix_full_parse(black_box(s))),
        );
        group.bench_with_input(
            BenchmarkId::new("stage4_parse_semantic", label), src,
            |b, s| b.iter(|| mdix_parse_and_analyze(black_box(s))),
        );
        group.bench_with_input(
            BenchmarkId::new("stage5_full_pipeline",  label), src,
            |b, s| b.iter(|| mdix_full_pipeline(black_box(s))),
        );
    }

    group.finish();
}

// ── Group 5: Repeated-parse throughput (hot path) ────────────────────────────

fn bench_repeated_parse_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("format/hot_path_throughput");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(200);

    // Simulates a server repeatedly reloading the same config file
    for (label, mdix_src, json_src) in [
        ("small",  SMALL_MDIX,  SMALL_JSON),
        ("medium", MEDIUM_MDIX, MEDIUM_JSON),
    ] {
        group.throughput(Throughput::Bytes(mdix_src.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("mdix_hot_tokenize", label),
            mdix_src,
            |b, s| b.iter(|| black_box(mdix_tokenize_only(black_box(s)))),
        );

        group.throughput(Throughput::Bytes(json_src.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("json_hot_parse", label),
            json_src,
            |b, s| {
                b.iter(|| {
                    let v: serde_json::Value = serde_json::from_str(black_box(s)).unwrap();
                    black_box(v)
                })
            },
        );
    }

    group.finish();
}

// ── Group 6: Scaled payloads (token count vs parse time) ─────────────────────

fn bench_scaled_payloads(c: &mut Criterion) {
    // Build scaled versions of the small config by repeating its DATA block
    let base_data_block = r#"
  app_name = "ScaledApp"
  version  = "1.0.0"
  port     = 8080
  debug    = false
  timeout  = 30
"#;

    let base_json_object = r#"
  "app_name": "ScaledApp",
  "version": "1.0.0",
  "port": 8080,
  "debug": false,
  "timeout": 30
"#;

    let scales = [10usize, 50, 100, 500];

    let mut group = c.benchmark_group("format/scaled");
    group.measurement_time(Duration::from_secs(5));

    for &n in &scales {
        // Build scaled DixScript
        let mut mdix_src = "@CONFIG(version -> \"1.0.0\")\n@DATA(\n".to_string();
        for i in 0..n {
            mdix_src.push_str(&base_data_block.replace("ScaledApp", &format!("App{}", i)));
        }
        mdix_src.push(')');

        // Build scaled JSON
        let entries: Vec<String> = (0..n)
            .map(|i| {
                base_json_object
                    .replace("ScaledApp", &format!("App{}", i))
                    .split('\n')
                    .enumerate()
                    .map(|(j, line)| {
                        if j == 0 { line.to_string() }
                        else { format!("  \"field_{i}_{j}\": {}", line.trim()) }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect();
        let json_src = format!("{{{}}}", entries.join(",\n"));

        let mdix_bytes = mdix_src.len() as u64;
        let json_bytes = json_src.len() as u64;

        group.throughput(Throughput::Bytes(mdix_bytes));
        group.bench_with_input(
            BenchmarkId::new("mdix_parse", n),
            &mdix_src,
            |b, s| b.iter(|| mdix_full_parse(black_box(s))),
        );

        group.throughput(Throughput::Bytes(json_bytes));
        group.bench_with_input(
            BenchmarkId::new("json_parse", n),
            &json_src,
            |b, s| {
                b.iter(|| {
                    let v: serde_json::Value = serde_json::from_str(black_box(s)).unwrap();
                    black_box(v)
                })
            },
        );
    }

    group.finish();
}

// ── Setup ─────────────────────────────────────────────────────────────────────

fn setup(_c: &mut Criterion) {
    print_size_report();
}

criterion_group!(
    setup_group,
    setup
);

criterion_group!(
    name    = tokenize_benches;
    config  = Criterion::default().measurement_time(Duration::from_secs(5));
    targets = bench_tokenize_throughput
);

criterion_group!(
    name    = parse_benches;
    config  = Criterion::default().measurement_time(Duration::from_secs(8));
    targets = bench_parse_comparison
);

criterion_group!(
    name    = pipeline_benches;
    config  = Criterion::default()
                .measurement_time(Duration::from_secs(10))
                .sample_size(50);
    targets = bench_full_pipeline, bench_pipeline_stages
);

criterion_group!(
    name    = throughput_benches;
    config  = Criterion::default()
                .measurement_time(Duration::from_secs(8))
                .sample_size(200);
    targets = bench_repeated_parse_throughput
);

criterion_group!(
    name    = scaled_benches;
    config  = Criterion::default().measurement_time(Duration::from_secs(5));
    targets = bench_scaled_payloads
);

criterion_main!(
    setup_group,
    tokenize_benches,
    parse_benches,
    pipeline_benches,
    throughput_benches,
    scaled_benches
);
