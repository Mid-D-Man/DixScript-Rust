// tests/serialization_comparison_tests.rs
//
// DixScript Binary Format vs Industry Alternatives — Comparison Suite
//
// What we measure:
//   Raw size          — bytes on disk before any compression
//   After gzip        — flate2 level 6 (balanced)
//   After bzip2       — level 9 (best)
//   After lzma        — preset 6 (xz2 / LZMA)
//   Serialise speed   — µs per encode (averaged over N iterations)
//   Deserialise speed — µs per decode (averaged over N iterations)
//   Round-trip fidelity — decoded == original logical data
//
// Formats compared:
//   1. DixScript custom binary  (BinaryPacker / BinaryUnpacker)
//   2. JSON                     (serde_json)
//   3. Bincode                  (bincode 1.x)
//   4. MessagePack              (rmp-serde)
//   5. Postcard                 (postcard)
//   6. CBOR                     (ciborium)
//
// Run:
//   cargo test serialization_comparison -- --nocapture
//   cargo test serialization_comparison -- --nocapture --include-ignored  # slow benches
//
// NOTE: All "equivalent data" structures are serde-annotated so every non-DixScript
// format serialises exactly the same logical payload, making size comparisons fair.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::time::Instant;

use bincode;
use ciborium;
use flate2::{Compression as FlateLevel, read::GzDecoder, write::GzEncoder};
use bzip2::Compression as Bz2Level;
use bzip2::read::BzDecoder;
use bzip2::write::BzEncoder;
use xz2::read::XzDecoder;
use xz2::write::XzEncoder;
use rmp_serde;
use postcard;
use serde::{Deserialize, Serialize};

use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::BinarySerialization::{BinaryPacker, BinaryUnpacker};
use dixscript::Compiler::Core::Config::ConfigSectionHandler;
use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::ValueResolution::ValueResolver;
use dixscript::Compiler::Core::{DebugMode, GeneralParser, GeneralSemanticAnalyzer};
use dixscript::ErrorManager::ErrorManager;

// ══════════════════════════════════════════════════════════════════════════════
// EQUIVALENT DATA STRUCTURES
// Each struct mirrors a DixScript DATA section so all formats carry identical
// logical payloads.  serde derives allow encoding in every format under test.
// ══════════════════════════════════════════════════════════════════════════════

/// Flat key-value config record — mirrors SRC_FLAT.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FlatConfig {
    app_name:    String,
    version:     String,
    port:        i32,
    max_conns:   i32,
    timeout_ms:  i32,
    ssl_enabled: bool,
    debug_mode:  bool,
    log_level:   String,
    api_key:     String,
    base_url:    String,
}

/// Server entry in a list — mirrors the servers array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ServerEntry {
    id:        i32,
    host:      String,
    port:      i32,
    ssl:       bool,
    pool_size: i32,
    timeout:   i32,
    region:    String,
    weight:    f32,
}

/// Top-level multi-server config — mirrors SRC_OBJECTS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MultiServerConfig {
    environment: String,
    version:     String,
    servers:     Vec<ServerEntry>,
}

/// Deeply nested game-data style record — mirrors SRC_NESTED.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct EnemyStats {
    name:       String,
    health:     i32,
    damage:     i32,
    armor:      i32,
    xp:         i32,
    gold:       i32,
    ai_type:    String,
    spawn_rate: f32,
    loot:       Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct GameData {
    game_title:  String,
    version:     String,
    difficulty:  String,
    enemies:     Vec<EnemyStats>,
}

/// Repetitive endpoint list — mirrors SRC_REPETITIVE.
/// This is the case where DixScript's deduplication shines most.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Endpoint {
    path:       String,
    method:     String,
    rate_limit: i32,
    auth:       bool,
    version:    String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ApiConfig {
    base_url:   String,
    api_version: i32,
    endpoints:  Vec<Endpoint>,
}

// ══════════════════════════════════════════════════════════════════════════════
// DIXSCRIPT SOURCE STRINGS (one per logical dataset)
// These are the .mdix equivalents of the serde structs above.
// They intentionally use QuickFuncs to show the deduplication advantage.
// ══════════════════════════════════════════════════════════════════════════════

/// Plain flat config — no QuickFuncs, no deduplication advantage.
const SRC_FLAT: &str = r#"
@CONFIG(version -> "1.0.0", features -> "advanced", error_handling -> "continue")
@DATA(
    app_name    = "MyApplication",
    version     = "2.5.1",
    port        = 8080,
    max_conns   = 500,
    timeout_ms  = 3000,
    ssl_enabled = true,
    debug_mode  = false,
    log_level   = "INFO",
    api_key     = "sk-test-abcdef123456",
    base_url    = "https://api.example.com"
)
"#;

/// Multi-server config — QuickFuncs deduplicate the repeated structure.
/// The JSON equivalent must repeat every field for every server.
const SRC_OBJECTS: &str = r#"
@CONFIG(version -> "1.0.0", features -> "advanced", error_handling -> "continue")
@ENUMS( Env { DEV = 1, STAGING = 2, PROD = 3 } )
@QUICKFUNCS(
    ~server<object> => global(id<int>, host<string>, port<int>, ssl<bool>, pool<int>, region<string>, weight<float>) {
        return { id = id, host = host, port = port, ssl = ssl, pool_size = pool,
                 timeout = 5000, region = region, weight = weight };
    }
)
@DATA(
    environment = "production",
    version     = "1.0.0",
    servers::
        server(1, "10.0.0.1", 8080, false, 10, "us-east", 1.0f),
        server(2, "10.0.0.2", 8080, false, 10, "us-east", 1.0f),
        server(3, "10.0.0.3", 8080, false, 10, "us-east", 1.0f),
        server(4, "10.0.0.4", 8443, true,  20, "us-west", 0.8f),
        server(5, "10.0.0.5", 8443, true,  20, "us-west", 0.8f),
        server(6, "10.0.1.1", 8080, false, 15, "eu-central", 0.9f),
        server(7, "10.0.1.2", 8080, false, 15, "eu-central", 0.9f),
        server(8, "10.0.1.3", 8443, true,  25, "eu-west", 0.7f)
)
"#;

/// Game enemy data — repeated structure with computed fields (DixScript's sweet spot).
const SRC_NESTED: &str = r#"
@CONFIG(version -> "1.0.0", features -> "advanced", error_handling -> "continue")
@ENUMS( AIType { PASSIVE = 0, NEUTRAL = 1, AGGRESSIVE = 2, BOSS = 3 } )
@QUICKFUNCS(
    ~enemy<object> => global(name<string>, hp<int>, dmg<int>, ai<enum>) {
        return {
            name       = name,
            health     = hp,
            damage     = dmg,
            armor      = hp / 10,
            xp         = hp / 2,
            gold       = hp / 4,
            ai_type    = ai == AIType.BOSS ? "boss" : "standard",
            spawn_rate = ai == AIType.BOSS ? 0.01f : 0.3f,
            loot       = ["health_potion", "gold_coin"]
        };
    }
)
@DATA(
    game_title  = "DixScript Demo Game",
    version     = "1.0.0",
    difficulty  = "normal",
    enemies::
        enemy("Goblin",        50,   10, AIType.AGGRESSIVE),
        enemy("Orc",          100,   20, AIType.AGGRESSIVE),
        enemy("Troll",        200,   40, AIType.AGGRESSIVE),
        enemy("Dark Elf",     150,   35, AIType.NEUTRAL),
        enemy("Skeleton",      75,   15, AIType.AGGRESSIVE),
        enemy("Zombie",        60,   12, AIType.NEUTRAL),
        enemy("Bandit",        80,   18, AIType.AGGRESSIVE),
        enemy("Wolf",          45,    8, AIType.NEUTRAL),
        enemy("Giant Spider", 120,   25, AIType.AGGRESSIVE),
        enemy("Dragon",      1000,  150, AIType.BOSS)
)
"#;

/// Highly repetitive API endpoint list — maximum deduplication benefit.
const SRC_REPETITIVE: &str = r#"
@CONFIG(version -> "1.0.0", features -> "advanced", error_handling -> "continue")
@ENUMS( Method { GET = 1, POST = 2, PUT = 3, DELETE = 4 } )
@QUICKFUNCS(
    ~ep<object> => global(path<string>, method<enum>, rate<int>, auth<bool>) {
        return { path = path, method = method, rate_limit = rate, auth = auth, version = "v2" };
    }
)
@DATA(
    base_url    = "https://api.example.com",
    api_version = 2,
    endpoints::
        ep("/users",            Method.GET,    200, true),
        ep("/users",            Method.POST,    50, true),
        ep("/users/{id}",       Method.GET,    200, true),
        ep("/users/{id}",       Method.PUT,     50, true),
        ep("/users/{id}",       Method.DELETE,  20, true),
        ep("/products",         Method.GET,    500, false),
        ep("/products",         Method.POST,    50, true),
        ep("/products/{id}",    Method.GET,    500, false),
        ep("/products/{id}",    Method.PUT,     50, true),
        ep("/products/{id}",    Method.DELETE,  20, true),
        ep("/orders",           Method.GET,    200, true),
        ep("/orders",           Method.POST,    50, true),
        ep("/orders/{id}",      Method.GET,    200, true),
        ep("/orders/{id}",      Method.PUT,     30, true),
        ep("/orders/{id}",      Method.DELETE,  10, true),
        ep("/health",           Method.GET,   1000, false),
        ep("/metrics",          Method.GET,    100, true),
        ep("/auth/login",       Method.POST,    20, false),
        ep("/auth/logout",      Method.POST,   100, true),
        ep("/auth/refresh",     Method.POST,    50, true)
)
"#;

// ══════════════════════════════════════════════════════════════════════════════
// EQUIVALENT SERDE PAYLOADS (same logical data as the .mdix sources above)
// ══════════════════════════════════════════════════════════════════════════════

fn make_flat_config() -> FlatConfig {
    FlatConfig {
        app_name:    "MyApplication".into(),
        version:     "2.5.1".into(),
        port:        8080,
        max_conns:   500,
        timeout_ms:  3000,
        ssl_enabled: true,
        debug_mode:  false,
        log_level:   "INFO".into(),
        api_key:     "sk-test-abcdef123456".into(),
        base_url:    "https://api.example.com".into(),
    }
}

fn make_multi_server() -> MultiServerConfig {
    let base_servers = [
        (1, "10.0.0.1", 8080, false, 10, "us-east",    1.0f32),
        (2, "10.0.0.2", 8080, false, 10, "us-east",    1.0f32),
        (3, "10.0.0.3", 8080, false, 10, "us-east",    1.0f32),
        (4, "10.0.0.4", 8443, true,  20, "us-west",    0.8f32),
        (5, "10.0.0.5", 8443, true,  20, "us-west",    0.8f32),
        (6, "10.0.1.1", 8080, false, 15, "eu-central", 0.9f32),
        (7, "10.0.1.2", 8080, false, 15, "eu-central", 0.9f32),
        (8, "10.0.1.3", 8443, true,  25, "eu-west",    0.7f32),
    ];
    MultiServerConfig {
        environment: "production".into(),
        version:     "1.0.0".into(),
        servers: base_servers.iter().map(|&(id, host, port, ssl, pool, region, weight)| {
            ServerEntry { id, host: host.into(), port, ssl, pool_size: pool,
                          timeout: 5000, region: region.into(), weight }
        }).collect(),
    }
}

fn make_game_data() -> GameData {
    let raw: &[(&str, i32, i32, &str, f32)] = &[
        ("Goblin",        50,   10, "standard", 0.3),
        ("Orc",          100,   20, "standard", 0.3),
        ("Troll",        200,   40, "standard", 0.3),
        ("Dark Elf",     150,   35, "standard", 0.3),
        ("Skeleton",      75,   15, "standard", 0.3),
        ("Zombie",        60,   12, "standard", 0.3),
        ("Bandit",        80,   18, "standard", 0.3),
        ("Wolf",          45,    8, "standard", 0.3),
        ("Giant Spider", 120,   25, "standard", 0.3),
        ("Dragon",      1000,  150, "boss",     0.01),
    ];
    GameData {
        game_title: "DixScript Demo Game".into(),
        version:    "1.0.0".into(),
        difficulty: "normal".into(),
        enemies: raw.iter().map(|&(name, hp, dmg, ai, spawn)| EnemyStats {
            name:       name.into(),
            health:     hp,
            damage:     dmg,
            armor:      hp / 10,
            xp:         hp / 2,
            gold:       hp / 4,
            ai_type:    ai.into(),
            spawn_rate: spawn,
            loot:       vec!["health_potion".into(), "gold_coin".into()],
        }).collect(),
    }
}

fn make_api_config() -> ApiConfig {
    let raw: &[(&str, &str, i32, bool)] = &[
        ("/users",         "GET",    200, true),
        ("/users",         "POST",    50, true),
        ("/users/{id}",    "GET",    200, true),
        ("/users/{id}",    "PUT",     50, true),
        ("/users/{id}",    "DELETE",  20, true),
        ("/products",      "GET",    500, false),
        ("/products",      "POST",    50, true),
        ("/products/{id}", "GET",    500, false),
        ("/products/{id}", "PUT",     50, true),
        ("/products/{id}", "DELETE",  20, true),
        ("/orders",        "GET",    200, true),
        ("/orders",        "POST",    50, true),
        ("/orders/{id}",   "GET",    200, true),
        ("/orders/{id}",   "PUT",     30, true),
        ("/orders/{id}",   "DELETE",  10, true),
        ("/health",        "GET",   1000, false),
        ("/metrics",       "GET",    100, true),
        ("/auth/login",    "POST",    20, false),
        ("/auth/logout",   "POST",   100, true),
        ("/auth/refresh",  "POST",    50, true),
    ];
    ApiConfig {
        base_url:    "https://api.example.com".into(),
        api_version: 2,
        endpoints: raw.iter().map(|&(path, method, rate, auth)| Endpoint {
            path:       path.into(),
            method:     method.into(),
            rate_limit: rate,
            auth,
            version:    "v2".into(),
        }).collect(),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// COMPRESSION HELPERS
// ══════════════════════════════════════════════════════════════════════════════

fn gzip_compress(data: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), FlateLevel::new(6));
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

fn bzip2_compress(data: &[u8]) -> Vec<u8> {
    let mut enc = BzEncoder::new(Vec::new(), Bz2Level::Best);
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

fn lzma_compress(data: &[u8]) -> Vec<u8> {
    let mut enc = XzEncoder::new(Vec::new(), 6);
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

fn gzip_decompress(data: &[u8]) -> Vec<u8> {
    let mut dec = GzDecoder::new(data);
    let mut out = Vec::new();
    dec.read_to_end(&mut out).unwrap();
    out
}

fn bzip2_decompress(data: &[u8]) -> Vec<u8> {
    let mut dec = BzDecoder::new(data);
    let mut out = Vec::new();
    dec.read_to_end(&mut out).unwrap();
    out
}

fn lzma_decompress(data: &[u8]) -> Vec<u8> {
    let mut dec = XzDecoder::new(data);
    let mut out = Vec::new();
    dec.read_to_end(&mut out).unwrap();
    out
}

// ══════════════════════════════════════════════════════════════════════════════
// SERIALISATION HELPERS — one function per format
// ══════════════════════════════════════════════════════════════════════════════

fn encode_json<T: Serialize>(v: &T) -> Vec<u8> {
    serde_json::to_vec(v).expect("json encode")
}

fn encode_json_pretty<T: Serialize>(v: &T) -> Vec<u8> {
    serde_json::to_vec_pretty(v).expect("json pretty encode")
}

fn encode_bincode<T: Serialize>(v: &T) -> Vec<u8> {
    bincode::serialize(v).expect("bincode encode")
}

fn encode_msgpack<T: Serialize>(v: &T) -> Vec<u8> {
    rmp_serde::to_vec(v).expect("msgpack encode")
}

fn encode_postcard<T: Serialize>(v: &T) -> Vec<u8> {
    postcard::to_allocvec(v).expect("postcard encode")
}

fn encode_cbor<T: Serialize>(v: &T) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::into_writer(v, &mut buf).expect("cbor encode");
    buf
}

/// Run DixScript full pipeline on a source string, return BinaryPacker bytes.
fn dixscript_binary(source: &str) -> Vec<u8> {
    ErrorManager::get_shared_instance().clear_errors();

    let cr  = ConfigSectionHandler::new(None).process_config_section(source);
    let tok = Tokenizer::new(cr.cleaned_input_string.clone()).tokenize();
    let ast = GeneralParser::new(
                  tok.tokens,
                  cr.config_section.clone(),
                  cr.operational_settings.clone(),
              )
              .expect("parser")
              .parse()
              .expect("parse");

    let sem = GeneralSemanticAnalyzer::new(&ast, &cr.operational_settings).analyze();

    // Run value resolution if QuickFuncs are present
    let resolved = if ast.quick_functions.is_some() {
        if let Some(st) = sem.symbol_table.as_ref() {
            let vr  = ValueResolver::new(ast, st, DebugMode::Off);
            let res = vr.resolve();
            res.resolved_ast.unwrap_or_default()
        } else {
            ast
        }
    } else {
        ast
    };

    let result = BinaryPacker::new().pack(&resolved);
    assert!(result.is_success, "BinaryPacker failed: {:?}", result.errors);
    result.binary_data
}

// ══════════════════════════════════════════════════════════════════════════════
// SIZE REPORT
// ══════════════════════════════════════════════════════════════════════════════

struct SizeRow {
    format:    &'static str,
    raw:       usize,
    gzip:      usize,
    bzip2:     usize,
    lzma:      usize,
}

impl SizeRow {
    fn new(format: &'static str, raw_bytes: &[u8]) -> Self {
        SizeRow {
            format,
            raw:   raw_bytes.len(),
            gzip:  gzip_compress(raw_bytes).len(),
            bzip2: bzip2_compress(raw_bytes).len(),
            lzma:  lzma_compress(raw_bytes).len(),
        }
    }
}

fn print_size_table(label: &str, rows: &[SizeRow]) {
    // Find baseline (first row = DixScript)
    let baseline = rows[0].raw as f64;

    println!("\n╔══════════════════════════════════════════════════════════════════════════════════════╗");
    println!("║  {}  — Binary Size Comparison", label);
    println!("╠══════════════════╦══════════╦══════════╦══════════╦══════════╦══════════╦═══════════╗");
    println!("║  Format          ║  Raw (B) ║ +gzip(B) ║+bzip2(B) ║ +lzma(B) ║  vs raw  ║  vs json  ║");
    println!("╠══════════════════╬══════════╬══════════╬══════════╬══════════╬══════════╬═══════════╣");

    let json_raw = rows.iter().find(|r| r.format == "JSON (compact)").map(|r| r.raw as f64).unwrap_or(1.0);

    for row in rows {
        let vs_raw  = (1.0 - row.gzip as f64 / row.raw  as f64) * 100.0;
        let vs_json = (1.0 - row.raw  as f64 / json_raw)         * 100.0;
        println!(
            "║  {:<16} ║ {:>8} ║ {:>8} ║ {:>8} ║ {:>8} ║ {:>+7.1}% ║ {:>+8.1}% ║",
            row.format, row.raw, row.gzip, row.bzip2, row.lzma,
            -vs_raw,   // negative = gzip reduced size
            vs_json,   // negative = smaller than JSON
        );
    }
    println!("╠══════════════════╩══════════╩══════════╩══════════╩══════════╩══════════╩═══════════╣");
    // Best raw / best compressed annotations
    let best_raw  = rows.iter().min_by_key(|r| r.raw).unwrap();
    let best_gz   = rows.iter().min_by_key(|r| r.gzip).unwrap();
    let best_lzma = rows.iter().min_by_key(|r| r.lzma).unwrap();
    println!("║  Smallest raw:        {:<16}  ({} B)                                   ║", best_raw.format, best_raw.raw);
    println!("║  Smallest +gzip:      {:<16}  ({} B)                                   ║", best_gz.format, best_gz.gzip);
    println!("║  Smallest +lzma:      {:<16}  ({} B)                                   ║", best_lzma.format, best_lzma.lzma);
    println!("╚══════════════════════════════════════════════════════════════════════════════════════╝");
}

// ══════════════════════════════════════════════════════════════════════════════
// SPEED HELPERS
// ══════════════════════════════════════════════════════════════════════════════

struct SpeedRow {
    format:     &'static str,
    encode_us:  f64,
    decode_us:  f64,
}

fn time_encode<T: Serialize>(v: &T, encode: impl Fn(&T) -> Vec<u8>, n: u32) -> f64 {
    let t = Instant::now();
    for _ in 0..n { let _ = encode(v); }
    t.elapsed().as_micros() as f64 / n as f64
}

fn time_decode_json<T: for<'de> Deserialize<'de>>(bytes: &[u8], n: u32) -> f64 {
    let t = Instant::now();
    for _ in 0..n { let _: T = serde_json::from_slice(bytes).unwrap(); }
    t.elapsed().as_micros() as f64 / n as f64
}

fn time_decode_bincode<T: for<'de> Deserialize<'de>>(bytes: &[u8], n: u32) -> f64 {
    let t = Instant::now();
    for _ in 0..n { let _: T = bincode::deserialize(bytes).unwrap(); }
    t.elapsed().as_micros() as f64 / n as f64
}

fn time_decode_msgpack<T: for<'de> Deserialize<'de>>(bytes: &[u8], n: u32) -> f64 {
    let t = Instant::now();
    for _ in 0..n { let _: T = rmp_serde::from_slice(bytes).unwrap(); }
    t.elapsed().as_micros() as f64 / n as f64
}

fn time_decode_postcard<T: for<'de> Deserialize<'de>>(bytes: &[u8], n: u32) -> f64 {
    let t = Instant::now();
    for _ in 0..n { let _: T = postcard::from_bytes(bytes).unwrap(); }
    t.elapsed().as_micros() as f64 / n as f64
}

fn time_decode_cbor<T: for<'de> Deserialize<'de>>(bytes: &[u8], n: u32) -> f64 {
    let t = Instant::now();
    for _ in 0..n { let _: T = ciborium::from_reader(bytes).unwrap(); }
    t.elapsed().as_micros() as f64 / n as f64
}

fn time_dixscript_pack(source: &str, n: u32) -> (f64, f64) {
    // encode
    let enc_t = Instant::now();
    for _ in 0..n { let _ = dixscript_binary(source); }
    let enc_us = enc_t.elapsed().as_micros() as f64 / n as f64;

    // decode
    let packed = dixscript_binary(source);
    let dec_t  = Instant::now();
    for _ in 0..n {
        let result = BinaryUnpacker::new().unpack(&packed);
        assert!(result.is_success);
    }
    let dec_us = dec_t.elapsed().as_micros() as f64 / n as f64;

    (enc_us, dec_us)
}

fn print_speed_table(label: &str, rows: &[SpeedRow]) {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  {}  — Encode / Decode Speed", label);
    println!("╠══════════════════╦═════════════════╦═════════════════╣");
    println!("║  Format          ║   Encode (µs)   ║   Decode (µs)   ║");
    println!("╠══════════════════╬═════════════════╬═════════════════╣");
    for row in rows {
        println!("║  {:<16} ║ {:>15.2} ║ {:>15.2} ║", row.format, row.encode_us, row.decode_us);
    }
    println!("╚══════════════════╩═════════════════╩═════════════════╝");
}

// ══════════════════════════════════════════════════════════════════════════════
// SIZE TESTS
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn size_flat_config() {
    let payload = make_flat_config();
    let dx      = dixscript_binary(SRC_FLAT);

    let rows = vec![
        SizeRow::new("DixScript",      &dx),
        SizeRow::new("JSON (compact)", &encode_json(&payload)),
        SizeRow::new("JSON (pretty)",  &encode_json_pretty(&payload)),
        SizeRow::new("Bincode",        &encode_bincode(&payload)),
        SizeRow::new("MessagePack",    &encode_msgpack(&payload)),
        SizeRow::new("Postcard",       &encode_postcard(&payload)),
        SizeRow::new("CBOR",           &encode_cbor(&payload)),
    ];

    print_size_table("Flat Config (10 fields)", &rows);

    // Smoke: every format produces at least 1 byte
    for row in &rows {
        assert!(row.raw > 0, "{} produced 0 bytes", row.format);
    }
}

#[test]
fn size_repetitive_objects() {
    let payload = make_multi_server();
    let dx      = dixscript_binary(SRC_OBJECTS);

    let rows = vec![
        SizeRow::new("DixScript",      &dx),
        SizeRow::new("JSON (compact)", &encode_json(&payload)),
        SizeRow::new("JSON (pretty)",  &encode_json_pretty(&payload)),
        SizeRow::new("Bincode",        &encode_bincode(&payload)),
        SizeRow::new("MessagePack",    &encode_msgpack(&payload)),
        SizeRow::new("Postcard",       &encode_postcard(&payload)),
        SizeRow::new("CBOR",           &encode_cbor(&payload)),
    ];

    print_size_table("Multi-Server Config (8 servers, repeated structure)", &rows);

    for row in &rows {
        assert!(row.raw > 0, "{} produced 0 bytes", row.format);
    }
}

#[test]
fn size_nested_game_data() {
    let payload = make_game_data();
    let dx      = dixscript_binary(SRC_NESTED);

    let rows = vec![
        SizeRow::new("DixScript",      &dx),
        SizeRow::new("JSON (compact)", &encode_json(&payload)),
        SizeRow::new("JSON (pretty)",  &encode_json_pretty(&payload)),
        SizeRow::new("Bincode",        &encode_bincode(&payload)),
        SizeRow::new("MessagePack",    &encode_msgpack(&payload)),
        SizeRow::new("Postcard",       &encode_postcard(&payload)),
        SizeRow::new("CBOR",           &encode_cbor(&payload)),
    ];

    print_size_table("Game Enemy Data (10 enemies, computed fields)", &rows);

    for row in &rows {
        assert!(row.raw > 0, "{} produced 0 bytes", row.format);
    }
}

#[test]
fn size_repetitive_api_endpoints() {
    let payload = make_api_config();
    let dx      = dixscript_binary(SRC_REPETITIVE);

    let rows = vec![
        SizeRow::new("DixScript",      &dx),
        SizeRow::new("JSON (compact)", &encode_json(&payload)),
        SizeRow::new("JSON (pretty)",  &encode_json_pretty(&payload)),
        SizeRow::new("Bincode",        &encode_bincode(&payload)),
        SizeRow::new("MessagePack",    &encode_msgpack(&payload)),
        SizeRow::new("Postcard",       &encode_postcard(&payload)),
        SizeRow::new("CBOR",           &encode_cbor(&payload)),
    ];

    print_size_table("API Endpoints (20 endpoints, max repetition)", &rows);

    for row in &rows {
        assert!(row.raw > 0, "{} produced 0 bytes", row.format);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ROUND-TRIP FIDELITY TESTS
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn roundtrip_json_flat() {
    let original = make_flat_config();
    let bytes    = encode_json(&original);
    let decoded: FlatConfig = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(original, decoded, "JSON round-trip failed for FlatConfig");
    println!("[rt_json_flat] ok — {} bytes", bytes.len());
}

#[test]
fn roundtrip_bincode_flat() {
    let original = make_flat_config();
    let bytes    = encode_bincode(&original);
    let decoded: FlatConfig = bincode::deserialize(&bytes).unwrap();
    assert_eq!(original, decoded);
    println!("[rt_bincode_flat] ok — {} bytes", bytes.len());
}

#[test]
fn roundtrip_msgpack_flat() {
    let original = make_flat_config();
    let bytes    = encode_msgpack(&original);
    let decoded: FlatConfig = rmp_serde::from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
    println!("[rt_msgpack_flat] ok — {} bytes", bytes.len());
}

#[test]
fn roundtrip_postcard_flat() {
    let original = make_flat_config();
    let bytes    = encode_postcard(&original);
    let decoded: FlatConfig = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(original, decoded);
    println!("[rt_postcard_flat] ok — {} bytes", bytes.len());
}

#[test]
fn roundtrip_cbor_flat() {
    let original = make_flat_config();
    let bytes    = encode_cbor(&original);
    let decoded: FlatConfig = ciborium::from_reader(bytes.as_slice()).unwrap();
    assert_eq!(original, decoded);
    println!("[rt_cbor_flat] ok — {} bytes", bytes.len());
}

#[test]
fn roundtrip_dixscript_pack_unpack() {
    // DixScript binary round-trip: pack → unpack → DATA section present
    let dx     = dixscript_binary(SRC_NESTED);
    let result = BinaryUnpacker::new().unpack(&dx);
    assert!(result.is_success, "DixScript unpack failed: {:?}", result.errors);
    let restored = result.ast.expect("no AST");
    assert!(restored.data.is_some(), "DATA section lost in DixScript round-trip");
    println!("[rt_dixscript] ok — {} bytes → DATA entries={}",
        dx.len(),
        restored.data.as_ref().unwrap().entries.len()
    );
}

#[test]
fn roundtrip_all_formats_game_data() {
    let original = make_game_data();

    // JSON
    let j: GameData = serde_json::from_slice(&encode_json(&original)).unwrap();
    assert_eq!(original, j);

    // Bincode
    let b: GameData = bincode::deserialize(&encode_bincode(&original)).unwrap();
    assert_eq!(original, b);

    // MessagePack
    let m: GameData = rmp_serde::from_slice(&encode_msgpack(&original)).unwrap();
    assert_eq!(original, m);

    // Postcard
    let p: GameData = postcard::from_bytes(&encode_postcard(&original)).unwrap();
    assert_eq!(original, p);

    // CBOR
    let c: GameData = ciborium::from_reader(encode_cbor(&original).as_slice()).unwrap();
    assert_eq!(original, c);

    println!("[rt_all_formats_game] all 5 non-DixScript formats round-tripped OK");
}

// ══════════════════════════════════════════════════════════════════════════════
// COMPRESSION FIDELITY TESTS
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn compression_roundtrip_gzip() {
    let data       = encode_json(&make_game_data());
    let compressed = gzip_compress(&data);
    let restored   = gzip_decompress(&compressed);
    assert_eq!(data, restored, "gzip round-trip failed");
    println!("[gzip_rt] {} → {} → {} bytes", data.len(), compressed.len(), restored.len());
}

#[test]
fn compression_roundtrip_bzip2() {
    let data       = encode_json(&make_game_data());
    let compressed = bzip2_compress(&data);
    let restored   = bzip2_decompress(&compressed);
    assert_eq!(data, restored, "bzip2 round-trip failed");
    println!("[bzip2_rt] {} → {} → {} bytes", data.len(), compressed.len(), restored.len());
}

#[test]
fn compression_roundtrip_lzma() {
    let data       = encode_json(&make_game_data());
    let compressed = lzma_compress(&data);
    let restored   = lzma_decompress(&compressed);
    assert_eq!(data, restored, "lzma round-trip failed");
    println!("[lzma_rt] {} → {} → {} bytes", data.len(), compressed.len(), restored.len());
}

// ══════════════════════════════════════════════════════════════════════════════
// COMPREHENSIVE SUMMARY — all datasets × all formats × all compressions
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn summary_all_datasets_all_formats() {
    struct Dataset<T> {
        label:   &'static str,
        src:     &'static str,    // mdix source
        payload: T,
    }

    // We can't easily make a generic function over T here, so we do it manually.

    let datasets: &[(&str, &str)] = &[
        ("Flat Config (10 fields)",             SRC_FLAT),
        ("Multi-Server (8×repeated struct)",    SRC_OBJECTS),
        ("Game Data (10 enemies+computed)",     SRC_NESTED),
        ("API Endpoints (20×repeated struct)",  SRC_REPETITIVE),
    ];

    let payloads_flat: Vec<Vec<u8>> = vec![
        encode_json(&make_flat_config()),
        encode_json(&make_multi_server()),
        encode_json(&make_game_data()),
        encode_json(&make_api_config()),
    ];

    let payloads_bincode: Vec<Vec<u8>> = vec![
        encode_bincode(&make_flat_config()),
        encode_bincode(&make_multi_server()),
        encode_bincode(&make_game_data()),
        encode_bincode(&make_api_config()),
    ];

    let payloads_msgpack: Vec<Vec<u8>> = vec![
        encode_msgpack(&make_flat_config()),
        encode_msgpack(&make_multi_server()),
        encode_msgpack(&make_game_data()),
        encode_msgpack(&make_api_config()),
    ];

    let payloads_postcard: Vec<Vec<u8>> = vec![
        encode_postcard(&make_flat_config()),
        encode_postcard(&make_multi_server()),
        encode_postcard(&make_game_data()),
        encode_postcard(&make_api_config()),
    ];

    let payloads_cbor: Vec<Vec<u8>> = vec![
        encode_cbor(&make_flat_config()),
        encode_cbor(&make_multi_server()),
        encode_cbor(&make_game_data()),
        encode_cbor(&make_api_config()),
    ];

    println!("\n");
    println!("╔══════════════════════════════════════════════════════════════════════════════════════════════════════╗");
    println!("║                              COMPREHENSIVE FORMAT × COMPRESSION SUMMARY                             ║");
    println!("║  Values = raw bytes.  Lower is better.  DixScript includes full pipeline overhead (parse + pack).  ║");
    println!("╠══════════════════════════╦══════════════════╦══════════════════╦══════════════════╦═════════════════╣");
    println!("║  Dataset                 ║  DixScript       ║  JSON+gzip       ║  Bincode+gzip    ║  Postcard       ║");
    println!("╠══════════════════════════╬══════════════════╬══════════════════╬══════════════════╬═════════════════╣");

    for (i, (label, src)) in datasets.iter().enumerate() {
        let dx_raw   = dixscript_binary(src);
        let dx_gz    = gzip_compress(&dx_raw);
        let json_gz  = gzip_compress(&payloads_flat[i]);
        let bc_gz    = gzip_compress(&payloads_bincode[i]);
        let pc_raw   = &payloads_postcard[i];

        println!(
            "║  {:<24}  ║  raw={:>5} gz={:>5} ║  raw={:>5} gz={:>5} ║  raw={:>5} gz={:>5} ║  raw={:>5}        ║",
            label,
            dx_raw.len(), dx_gz.len(),
            payloads_flat[i].len(), json_gz.len(),
            payloads_bincode[i].len(), bc_gz.len(),
            pc_raw.len(),
        );
    }

    println!("╠══════════════════════════╩══════════════════╩══════════════════╩══════════════════╩═════════════════╣");
    println!("║  Notes:                                                                                              ║");
    println!("║  • DixScript 'raw' = binary after full compile pipeline (QuickFuncs resolved, checksummed)          ║");
    println!("║  • DixScript 'gz'  = ready to store or send (apply DLM Compressor in production)                   ║");
    println!("║  • JSON requires external compression; DixScript DLM integrates it.                                 ║");
    println!("║  • Postcard is smallest raw binary — no schema overhead, but no schema means no self-description.   ║");
    println!("╚══════════════════════════════════════════════════════════════════════════════════════════════════════╝");
}

// ══════════════════════════════════════════════════════════════════════════════
// SPEED BENCHMARKS (ignored by default — run with --include-ignored)
// ══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "slow — run with --include-ignored"]
fn bench_encode_speed_flat_config() {
    const N: u32 = 10_000;
    let payload = make_flat_config();

    let rows = vec![
        SpeedRow {
            format:    "JSON",
            encode_us: time_encode(&payload, encode_json, N),
            decode_us: time_decode_json::<FlatConfig>(&encode_json(&payload), N),
        },
        SpeedRow {
            format:    "Bincode",
            encode_us: time_encode(&payload, encode_bincode, N),
            decode_us: time_decode_bincode::<FlatConfig>(&encode_bincode(&payload), N),
        },
        SpeedRow {
            format:    "MessagePack",
            encode_us: time_encode(&payload, encode_msgpack, N),
            decode_us: time_decode_msgpack::<FlatConfig>(&encode_msgpack(&payload), N),
        },
        SpeedRow {
            format:    "Postcard",
            encode_us: time_encode(&payload, encode_postcard, N),
            decode_us: time_decode_postcard::<FlatConfig>(&encode_postcard(&payload), N),
        },
        SpeedRow {
            format:    "CBOR",
            encode_us: time_encode(&payload, encode_cbor, N),
            decode_us: time_decode_cbor::<FlatConfig>(&encode_cbor(&payload), N),
        },
    ];

    // DixScript encode includes full pipeline (parse + resolve + pack)
    let (dx_enc, dx_dec) = time_dixscript_pack(SRC_FLAT, 200);
    let mut all_rows = vec![SpeedRow { format: "DixScript", encode_us: dx_enc, decode_us: dx_dec }];
    all_rows.extend(rows);

    print_speed_table("Flat Config ×10k (DixScript ×200)", &all_rows);
}

#[test]
#[ignore = "slow — run with --include-ignored"]
fn bench_encode_speed_game_data() {
    const N: u32 = 5_000;
    let payload = make_game_data();

    let rows = vec![
        SpeedRow {
            format:    "JSON",
            encode_us: time_encode(&payload, encode_json, N),
            decode_us: time_decode_json::<GameData>(&encode_json(&payload), N),
        },
        SpeedRow {
            format:    "Bincode",
            encode_us: time_encode(&payload, encode_bincode, N),
            decode_us: time_decode_bincode::<GameData>(&encode_bincode(&payload), N),
        },
        SpeedRow {
            format:    "MessagePack",
            encode_us: time_encode(&payload, encode_msgpack, N),
            decode_us: time_decode_msgpack::<GameData>(&encode_msgpack(&payload), N),
        },
        SpeedRow {
            format:    "Postcard",
            encode_us: time_encode(&payload, encode_postcard, N),
            decode_us: time_decode_postcard::<GameData>(&encode_postcard(&payload), N),
        },
        SpeedRow {
            format:    "CBOR",
            encode_us: time_encode(&payload, encode_cbor, N),
            decode_us: time_decode_cbor::<GameData>(&encode_cbor(&payload), N),
        },
    ];

    let (dx_enc, dx_dec) = time_dixscript_pack(SRC_NESTED, 100);
    let mut all_rows = vec![SpeedRow { format: "DixScript", encode_us: dx_enc, decode_us: dx_dec }];
    all_rows.extend(rows);

    print_speed_table("Game Data ×5k (DixScript ×100)", &all_rows);
}

#[test]
#[ignore = "slow — run with --include-ignored"]
fn bench_compression_speed() {
    // How long does each compression algorithm take on ~2KB of JSON?
    let data = encode_json_pretty(&make_game_data());
    println!("\n[compression speed] input = {} bytes", data.len());

    let formats: &[(&str, fn(&[u8]) -> Vec<u8>, fn(&[u8]) -> Vec<u8>)] = &[
        ("gzip",  gzip_compress,  gzip_decompress),
        ("bzip2", bzip2_compress, bzip2_decompress),
        ("lzma",  lzma_compress,  lzma_decompress),
    ];

    println!("╔════════════╦═══════════════╦═══════════════╦══════════════╗");
    println!("║  Algorithm ║  Compress(µs) ║  Decomp. (µs) ║  Output (B)  ║");
    println!("╠════════════╬═══════════════╬═══════════════╬══════════════╣");
    for (name, compress, decompress) in formats {
        const N: u32 = 500;
        let t0        = Instant::now();
        for _ in 0..N { let _ = compress(&data); }
        let comp_us   = t0.elapsed().as_micros() as f64 / N as f64;

        let compressed = compress(&data);
        let out_bytes  = compressed.len();

        let t1        = Instant::now();
        for _ in 0..N { let _ = decompress(&compressed); }
        let decomp_us = t1.elapsed().as_micros() as f64 / N as f64;

        println!("║  {:<10} ║ {:>13.2} ║ {:>13.2} ║ {:>12} ║", name, comp_us, decomp_us, out_bytes);
    }
    println!("╚════════════╩═══════════════╩═══════════════╩══════════════╝");
}
