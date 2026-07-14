//! Binary Serialization Benchmark — DixScript v1.0.0
//!
//! Compares DixScript's custom binary format against bincode, postcard, and
//! MessagePack (rmp-serde) across serialization speed and compressed output
//! size (gzip / bzip2 / lzma).  Results are printed once as a formatted table
//! before the timed measurements begin.
//!
//! Groups:
//! 1. `serialization_speed`    — raw encode time per format, small + medium payloads
//! 2. `compression_pipeline`   — encode + compress time per format x algorithm
//!
//! NOTE: DixScript custom-format benchmarks are skipped gracefully if the
//! BinaryPacker or parser are not yet functional, so the serde-format
//! comparisons always run regardless of port completeness.

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use bzip2::{write::BzEncoder, Compression as BzCompression};
use flate2::{write::GzEncoder, Compression as GzCompression};
use lzma_rust2::{XzWriter, XzOptions};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::Once;
use std::time::Duration;

use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::{
    BinarySerialization::BinaryPacker,
    Config::{ConfigSectionHandler, OperationalSettings},
    GeneralParser,
    Tokenizer::{split_config_tokens, Tokenizer},
};

// =============================================================================
// Representative serde-serializable data
// =============================================================================

#[derive(Serialize, Deserialize, Clone)]
struct Enemy {
    name: String,
    health: i32,
    damage: i32,
    armor: i32,
    xp: i32,
    gold: i32,
    spawn_rate: f32,
}

#[derive(Serialize, Deserialize, Clone)]
struct ServerCfg {
    host: String,
    port: i32,
    pool: i32,
    timeout: i32,
    ssl: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct SmallPayload {
    app_version: String,
    enemies: Vec<Enemy>,
    servers: Vec<ServerCfg>,
}

fn make_enemy(name: &str, health: i32, damage: i32) -> Enemy {
    Enemy {
        name: name.to_string(),
        health,
        damage,
        armor: health / 10,
        xp: health / 2,
        gold: health / 4,
        spawn_rate: if health > 500 { 0.01 } else { 0.30 },
    }
}

fn make_server(suffix: &str, pool: i32, ssl: bool) -> ServerCfg {
    ServerCfg {
        host: format!("{}-server.local", suffix),
        port: 8080,
        pool,
        timeout: 5000,
        ssl,
    }
}

fn small_payload() -> SmallPayload {
    SmallPayload {
        app_version: "1.0.0".to_string(),
        enemies: vec![
            make_enemy("Goblin", 50, 10),
            make_enemy("Orc", 100, 20),
            make_enemy("Troll", 200, 40),
            make_enemy("Dragon", 1000, 150),
        ],
        servers: vec![
            make_server("dev", 10, false),
            make_server("staging", 25, false),
            make_server("prod", 50, true),
        ],
    }
}

fn medium_payload() -> SmallPayload {
    let names = ["Goblin", "Orc", "Troll", "Ogre", "Wraith", "Vampire", "Dragon", "Golem"];
    let enemies = (0..20)
        .map(|i| {
            make_enemy(
                &format!("{}_{}", names[i % names.len()], i),
                50 * ((i % 8) + 1) as i32,
                (50 * ((i % 8) + 1) / 5) as i32,
            )
        })
        .collect();
    SmallPayload {
        app_version: "1.0.0".to_string(),
        enemies,
        servers: vec![
            make_server("dev", 10, false),
            make_server("staging", 25, false),
            make_server("prod", 50, true),
        ],
    }
}

// =============================================================================
// DixScript AST source for custom-format tests
// =============================================================================

const SMALL_MDIX: &str = r#"@CONFIG(
    version -> "1.0.0", features -> "advanced", error_handling -> "halt"
)
@ENUMS(
    AIType { PASSIVE = 0, AGGRESSIVE = 1, BOSS = 2 }
)
@QUICKFUNCS(
    ~calcXp<int>(health<int>) { return health / 2 }
    ~calcGold<int>(health<int>) { return health / 4 }
    ~createEnemy<object>(name<string>, health<int>, damage<int>) {
        return { name = name, health = health, damage = damage,
                 armor = health / 10, xp = calcXp(health), gold = calcGold(health) }
    }
)
@DATA(
    app_version = "1.0.0"
    enemies::
        createEnemy("Goblin", 50, 10),
        createEnemy("Orc", 100, 20),
        createEnemy("Troll", 200, 40),
        createEnemy("Dragon", 1000, 150)
    servers::
        { host = "dev-server.local", port = 8080, pool = 10, timeout = 5000, ssl = false },
        { host = "staging-server.local", port = 8080, pool = 25, timeout = 5000, ssl = false },
        { host = "prod-server.local", port = 8080, pool = 50, timeout = 5000, ssl = true }
)"#;

// =============================================================================
// Pipeline helpers — updated for token-based flow
// =============================================================================

/// Attempt to parse MDIX source into an AST using the current token-based pipeline.
/// Returns None and prints a warning if the parser is not yet functional.
fn try_parse_ast(source: &str) -> Option<DixScript> {
    let result = std::panic::catch_unwind(|| {
        let initial    = OperationalSettings::default();
        let tok_result = Tokenizer::new(source, &initial).tokenize();
        let split      = split_config_tokens(tok_result.tokens);
        let mut handler = ConfigSectionHandler::new(None);
        let cfg        = handler.process_config_tokens(&split.config_tokens);
        let s          = cfg.operational_settings.clone();
        let parser     = GeneralParser::new(split.rest_tokens, &cfg.config_section, &s).ok()?;
        parser.parse().ok()
    });

    match result {
        Ok(Some(ast)) => Some(ast),
        Ok(None) => {
            eprintln!(
                "[binary_serialization_benchmark] Parser returned None for MDIX input — \
                 DixScript custom-format benchmarks will be skipped."
            );
            None
        }
        Err(_) => {
            eprintln!(
                "[binary_serialization_benchmark] Parser panicked on MDIX input — \
                 DixScript custom-format benchmarks will be skipped."
            );
            None
        }
    }
}

/// Attempt to pack a parsed AST into binary bytes.
/// Returns None and prints a diagnostic if the packer fails or panics.
fn try_pack_dixscript(ast: &DixScript) -> Option<Vec<u8>> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        BinaryPacker::new().pack(ast)
    }));

    match result {
        Ok(pack_result) if pack_result.is_success => Some(pack_result.binary_data),
        Ok(pack_result) => {
            eprintln!(
                "[binary_serialization_benchmark] BinaryPacker failed: {:?} — \
                 DixScript custom-format benchmarks will be skipped.",
                pack_result.errors
            );
            None
        }
        Err(_) => {
            eprintln!(
                "[binary_serialization_benchmark] BinaryPacker panicked — \
                 DixScript custom-format benchmarks will be skipped."
            );
            None
        }
    }
}

// =============================================================================
// Compression helpers
// =============================================================================

fn compress_gzip(data: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::with_capacity(data.len()), GzCompression::default());
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

fn compress_bzip2(data: &[u8]) -> Vec<u8> {
    let mut enc = BzEncoder::new(Vec::with_capacity(data.len()), BzCompression::default());
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

fn compress_lzma(data: &[u8]) -> Vec<u8> {
    let mut enc = XzWriter::new(Vec::with_capacity(data.len()), XzOptions::default()).unwrap();
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

// =============================================================================
// Comparison report — printed once before Criterion runs its measurements
// =============================================================================

fn print_comparison_report() {
    let payload_small  = small_payload();
    let payload_medium = medium_payload();

    let bincode_small  = bincode::serialize(&payload_small).unwrap();
    let postcard_small = postcard::to_allocvec(&payload_small).unwrap();
    let msgpack_small  = rmp_serde::to_vec(&payload_small).unwrap();

    let bincode_medium  = bincode::serialize(&payload_medium).unwrap();
    let postcard_medium = postcard::to_allocvec(&payload_medium).unwrap();
    let msgpack_medium  = rmp_serde::to_vec(&payload_medium).unwrap();

    let custom_small_opt = try_parse_ast(SMALL_MDIX).and_then(|ast| try_pack_dixscript(&ast));

    println!();
    println!("╔══════════════════════════════════════════════════════════════════════════════╗");
    println!("║       DixScript Binary Format — Size & Compression Comparison               ║");
    println!("╚══════════════════════════════════════════════════════════════════════════════╝");

    if let Some(ref custom_small) = custom_small_opt {
        print_format_row("DixScript Custom", custom_small, custom_small);
    } else {
        println!();
        println!("  ┌─ DixScript Custom ──────────────────────────────────────────────");
        println!("  │  (skipped — packer not yet functional in this port)");
        println!("  └─────────────────────────────────────────────────────────────────");
    }

    print_format_row("Bincode",     &bincode_small,  &bincode_medium);
    print_format_row("Postcard",    &postcard_small,  &postcard_medium);
    print_format_row("MessagePack", &msgpack_small,   &msgpack_medium);

    println!();
    println!("  ── Small-dataset summary (raw bytes) ──────────────────────────────────────");
    println!(
        "  {:<20} {:>9} {:>10} {:>10} {:>10}",
        "Format", "Raw", "Gzip", "Bzip2", "LZMA"
    );
    println!("  {}", "─".repeat(63));

    if let Some(ref custom_small) = custom_small_opt {
        let gz = compress_gzip(custom_small);
        let bz = compress_bzip2(custom_small);
        let xz = compress_lzma(custom_small);
        println!(
            "  {:<20} {:>9} {:>10} {:>10} {:>10}",
            "DixScript Custom", custom_small.len(), gz.len(), bz.len(), xz.len()
        );
    } else {
        println!("  {:<20} {:>9}", "DixScript Custom", "(skipped)");
    }

    for (label, small) in &[
        ("Bincode",     &bincode_small),
        ("Postcard",    &postcard_small),
        ("MessagePack", &msgpack_small),
    ] {
        let gz = compress_gzip(small);
        let bz = compress_bzip2(small);
        let xz = compress_lzma(small);
        println!(
            "  {:<20} {:>9} {:>10} {:>10} {:>10}",
            label, small.len(), gz.len(), bz.len(), xz.len()
        );
    }

    println!();
}

fn print_format_row(label: &str, small: &[u8], medium: &[u8]) {
    let ratio = |raw: usize, comp: usize| -> f64 {
        if raw == 0 { 0.0 } else { (1.0 - comp as f64 / raw as f64) * 100.0 }
    };

    let gz = compress_gzip(small);
    let bz = compress_bzip2(small);
    let xz = compress_lzma(small);

    println!();
    println!("  ┌─ {} ─────────────────────────────────────────────────", label);
    println!("  │  [Small dataset — 4 enemies / 3 servers]");
    println!("  │    Raw        : {:>7} bytes", small.len());
    println!("  │    + Gzip     : {:>7} bytes  ({:.1}% reduction)", gz.len(), ratio(small.len(), gz.len()));
    println!("  │    + Bzip2    : {:>7} bytes  ({:.1}% reduction)", bz.len(), ratio(small.len(), bz.len()));
    println!("  │    + LZMA     : {:>7} bytes  ({:.1}% reduction)", xz.len(), ratio(small.len(), xz.len()));

    if !std::ptr::eq(small, medium) {
        let gz_m = compress_gzip(medium);
        let bz_m = compress_bzip2(medium);
        let xz_m = compress_lzma(medium);
        println!("  │  [Medium dataset — 20 enemies / 3 servers]");
        println!("  │    Raw        : {:>7} bytes", medium.len());
        println!("  │    + Gzip     : {:>7} bytes  ({:.1}% reduction)", gz_m.len(), ratio(medium.len(), gz_m.len()));
        println!("  │    + Bzip2    : {:>7} bytes  ({:.1}% reduction)", bz_m.len(), ratio(medium.len(), bz_m.len()));
        println!("  │    + LZMA     : {:>7} bytes  ({:.1}% reduction)", xz_m.len(), ratio(medium.len(), xz_m.len()));
    }

    println!("  └─────────────────────────────────────────────────────────────");
}

static REPORT_ONCE: Once = Once::new();

// =============================================================================
// Benchmark 1 — Serialization speed
// =============================================================================

fn bench_serialization_speed(c: &mut Criterion) {
    REPORT_ONCE.call_once(print_comparison_report);

    let mut group = c.benchmark_group("serialization_speed");
    group.measurement_time(Duration::from_secs(6));
    group.sample_size(100);

    let payload_small  = small_payload();
    let payload_medium = medium_payload();

    let ast_small_opt = try_parse_ast(SMALL_MDIX);
    if let Some(ref ast_small) = ast_small_opt {
        if try_pack_dixscript(ast_small).is_some() {
            group.throughput(Throughput::Bytes(SMALL_MDIX.len() as u64));
            group.bench_function("dixscript_custom_small", |b| {
                b.iter_batched(
                    BinaryPacker::new,
                    |mut packer| black_box(packer.pack(black_box(ast_small))),
                    BatchSize::SmallInput,
                );
            });
        }
    }

    group.throughput(Throughput::Bytes(bincode::serialize(&payload_small).unwrap().len() as u64));
    group.bench_function("bincode_small", |b| {
        b.iter(|| black_box(bincode::serialize(black_box(&payload_small)).unwrap()));
    });
    group.throughput(Throughput::Bytes(bincode::serialize(&payload_medium).unwrap().len() as u64));
    group.bench_function("bincode_medium", |b| {
        b.iter(|| black_box(bincode::serialize(black_box(&payload_medium)).unwrap()));
    });

    group.throughput(Throughput::Bytes(postcard::to_allocvec(&payload_small).unwrap().len() as u64));
    group.bench_function("postcard_small", |b| {
        b.iter(|| black_box(postcard::to_allocvec(black_box(&payload_small)).unwrap()));
    });
    group.throughput(Throughput::Bytes(postcard::to_allocvec(&payload_medium).unwrap().len() as u64));
    group.bench_function("postcard_medium", |b| {
        b.iter(|| black_box(postcard::to_allocvec(black_box(&payload_medium)).unwrap()));
    });

    group.throughput(Throughput::Bytes(rmp_serde::to_vec(&payload_small).unwrap().len() as u64));
    group.bench_function("msgpack_small", |b| {
        b.iter(|| black_box(rmp_serde::to_vec(black_box(&payload_small)).unwrap()));
    });
    group.throughput(Throughput::Bytes(rmp_serde::to_vec(&payload_medium).unwrap().len() as u64));
    group.bench_function("msgpack_medium", |b| {
        b.iter(|| black_box(rmp_serde::to_vec(black_box(&payload_medium)).unwrap()));
    });

    group.finish();
}

// =============================================================================
// Benchmark 2 — Serialize + compress (DLM pipeline simulation)
// =============================================================================

fn bench_compression_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_pipeline");
    group.measurement_time(Duration::from_secs(6));
    group.sample_size(60);

    let payload_medium = medium_payload();

    let bincode_bytes  = bincode::serialize(&payload_medium).unwrap();
    let postcard_bytes = postcard::to_allocvec(&payload_medium).unwrap();
    let msgpack_bytes  = rmp_serde::to_vec(&payload_medium).unwrap();

    let custom_bytes_opt = try_parse_ast(SMALL_MDIX).and_then(|ast| try_pack_dixscript(&ast));

    let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();
    if let Some(cb) = custom_bytes_opt { cases.push(("dixscript_custom", cb)); }
    cases.push(("bincode",  bincode_bytes));
    cases.push(("postcard", postcard_bytes));
    cases.push(("msgpack",  msgpack_bytes));

    for (label, data) in &cases {
        group.throughput(Throughput::Bytes(data.len() as u64));

        group.bench_with_input(BenchmarkId::new("gzip",  label), data.as_slice(),
            |b, d| b.iter(|| black_box(compress_gzip(black_box(d)))));
        group.bench_with_input(BenchmarkId::new("bzip2", label), data.as_slice(),
            |b, d| b.iter(|| black_box(compress_bzip2(black_box(d)))));
        group.bench_with_input(BenchmarkId::new("lzma",  label), data.as_slice(),
            |b, d| b.iter(|| black_box(compress_lzma(black_box(d)))));
    }

    group.finish();
}

// =============================================================================
// Registration
// =============================================================================

criterion_group!(benches, bench_serialization_speed, bench_compression_pipeline);
criterion_main!(benches);
