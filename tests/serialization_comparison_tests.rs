// tests/serialization_comparison_tests.rs
//
// DixScript Binary Format vs Industry Alternatives — Comparison Suite
//
// IMPORTANT: All DixScript test sources use PRE-COMPUTED plain literal values only.
// No @QUICKFUNCS or @ENUMS sections are included. BinaryPacker expects a fully
// resolved AST where every DATA value is a plain literal (int, float, string,
// bool, array, object) — no function call nodes, no enum references.
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

use std::io::{Read, Write};
use std::time::Instant;

use bincode;
use bzip2::Compression as Bz2Level;
use bzip2::read::BzDecoder;
use bzip2::write::BzEncoder;
use ciborium;
use flate2::{Compression as FlateLevel, read::GzDecoder, write::GzEncoder};
use postcard;
use rmp_serde;
use serde::{Deserialize, Serialize};
use xz2::read::XzDecoder;
use xz2::write::XzEncoder;

use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::BinarySerialization::{BinaryPacker, BinaryUnpacker};
use dixscript::Compiler::Core::Config::ConfigSectionHandler;
use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::GeneralParser;
use dixscript::ErrorManager::ErrorManager;

// ══════════════════════════════════════════════════════════════════════════════
// EQUIVALENT DATA STRUCTURES
// Each struct mirrors a DixScript DATA section so all formats carry identical
// logical payloads. serde derives allow encoding in every format under test.
// ══════════════════════════════════════════════════════════════════════════════

/// Flat key-value config record.
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

/// Single server entry.
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

/// Multi-server config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MultiServerConfig {
    environment: String,
    version:     String,
    servers:     Vec<ServerEntry>,
}

/// Game enemy.
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

/// Full game data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct GameData {
    game_title: String,
    version:    String,
    difficulty: String,
    enemies:    Vec<EnemyStats>,
}

/// API endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Endpoint {
    path:       String,
    method:     String,
    rate_limit: i32,
    auth:       bool,
    version:    String,
}

/// Full API config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ApiConfig {
    base_url:    String,
    api_version: i32,
    endpoints:   Vec<Endpoint>,
}

// ══════════════════════════════════════════════════════════════════════════════
// DIXSCRIPT SOURCE STRINGS
//
// All values are PRE-COMPUTED plain literals. No function calls, no enum
// references. This mirrors what BinaryPacker receives after value resolution
// in a real compilation pipeline.
//
// NOTE: Compared to the QuickFuncs version these sources are larger, because
// deduplication has been expanded. The binary OUTPUT sizes are what matter for
// the format comparison — the source is the input, not the deliverable.
// ══════════════════════════════════════════════════════════════════════════════

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

/// Pre-computed server objects — timeout=5000 explicit, no function call.
const SRC_OBJECTS: &str = r#"
@CONFIG(version -> "1.0.0", features -> "advanced", error_handling -> "continue")
@DATA(
    environment = "production",
    version     = "1.0.0",
    servers::
        { id = 1, host = "10.0.0.1", port = 8080, ssl = false, pool_size = 10, timeout = 5000, region = "us-east",    weight = 1.0f },
        { id = 2, host = "10.0.0.2", port = 8080, ssl = false, pool_size = 10, timeout = 5000, region = "us-east",    weight = 1.0f },
        { id = 3, host = "10.0.0.3", port = 8080, ssl = false, pool_size = 10, timeout = 5000, region = "us-east",    weight = 1.0f },
        { id = 4, host = "10.0.0.4", port = 8443, ssl = true,  pool_size = 20, timeout = 5000, region = "us-west",    weight = 0.8f },
        { id = 5, host = "10.0.0.5", port = 8443, ssl = true,  pool_size = 20, timeout = 5000, region = "us-west",    weight = 0.8f },
        { id = 6, host = "10.0.1.1", port = 8080, ssl = false, pool_size = 15, timeout = 5000, region = "eu-central", weight = 0.9f },
        { id = 7, host = "10.0.1.2", port = 8080, ssl = false, pool_size = 15, timeout = 5000, region = "eu-central", weight = 0.9f },
        { id = 8, host = "10.0.1.3", port = 8443, ssl = true,  pool_size = 25, timeout = 5000, region = "eu-west",    weight = 0.7f }
)
"#;

/// Pre-computed enemy stats — armor=hp/10, xp=hp/2, gold=hp/4 already evaluated.
/// Dragon: ai_type="boss", spawn_rate=0.01. All others: ai_type="standard", spawn_rate=0.3.
const SRC_NESTED: &str = r#"
@CONFIG(version -> "1.0.0", features -> "advanced", error_handling -> "continue")
@DATA(
    game_title = "DixScript Demo Game",
    version    = "1.0.0",
    difficulty = "normal",
    enemies::
        { name = "Goblin",       health =   50, damage =  10, armor =   5, xp =  25, gold =  12, ai_type = "standard", spawn_rate = 0.3f,  loot = ["health_potion", "gold_coin"] },
        { name = "Orc",          health =  100, damage =  20, armor =  10, xp =  50, gold =  25, ai_type = "standard", spawn_rate = 0.3f,  loot = ["health_potion", "gold_coin"] },
        { name = "Troll",        health =  200, damage =  40, armor =  20, xp = 100, gold =  50, ai_type = "standard", spawn_rate = 0.3f,  loot = ["health_potion", "gold_coin"] },
        { name = "Dark Elf",     health =  150, damage =  35, armor =  15, xp =  75, gold =  37, ai_type = "standard", spawn_rate = 0.3f,  loot = ["health_potion", "gold_coin"] },
        { name = "Skeleton",     health =   75, damage =  15, armor =   7, xp =  37, gold =  18, ai_type = "standard", spawn_rate = 0.3f,  loot = ["health_potion", "gold_coin"] },
        { name = "Zombie",       health =   60, damage =  12, armor =   6, xp =  30, gold =  15, ai_type = "standard", spawn_rate = 0.3f,  loot = ["health_potion", "gold_coin"] },
        { name = "Bandit",       health =   80, damage =  18, armor =   8, xp =  40, gold =  20, ai_type = "standard", spawn_rate = 0.3f,  loot = ["health_potion", "gold_coin"] },
        { name = "Wolf",         health =   45, damage =   8, armor =   4, xp =  22, gold =  11, ai_type = "standard", spawn_rate = 0.3f,  loot = ["health_potion", "gold_coin"] },
        { name = "Giant Spider", health =  120, damage =  25, armor =  12, xp =  60, gold =  30, ai_type = "standard", spawn_rate = 0.3f,  loot = ["health_potion", "gold_coin"] },
        { name = "Dragon",       health = 1000, damage = 150, armor = 100, xp = 500, gold = 250, ai_type = "boss",     spawn_rate = 0.01f, loot = ["health_potion", "gold_coin"] }
)
"#;

/// Pre-computed API endpoints — method as string, version="v2" explicit.
const SRC_REPETITIVE: &str = r#"
@CONFIG(version -> "1.0.0", features -> "advanced", error_handling -> "continue")
@DATA(
    base_url    = "https://api.example.com",
    api_version = 2,
    endpoints::
        { path = "/users",            method = "GET",    rate_limit = 200, auth = true,  version = "v2" },
        { path = "/users",            method = "POST",   rate_limit =  50, auth = true,  version = "v2" },
        { path = "/users/{id}",       method = "GET",    rate_limit = 200, auth = true,  version = "v2" },
        { path = "/users/{id}",       method = "PUT",    rate_limit =  50, auth = true,  version = "v2" },
        { path = "/users/{id}",       method = "DELETE", rate_limit =  20, auth = true,  version = "v2" },
        { path = "/products",         method = "GET",    rate_limit = 500, auth = false, version = "v2" },
        { path = "/products",         method = "POST",   rate_limit =  50, auth = true,  version = "v2" },
        { path = "/products/{id}",    method = "GET",    rate_limit = 500, auth = false, version = "v2" },
        { path = "/products/{id}",    method = "PUT",    rate_limit =  50, auth = true,  version = "v2" },
        { path = "/products/{id}",    method = "DELETE", rate_limit =  20, auth = true,  version = "v2" },
        { path = "/orders",           method = "GET",    rate_limit = 200, auth = true,  version = "v2" },
        { path = "/orders",           method = "POST",   rate_limit =  50, auth = true,  version = "v2" },
        { path = "/orders/{id}",      method = "GET",    rate_limit = 200, auth = true,  version = "v2" },
        { path = "/orders/{id}",      method = "PUT",    rate_limit =  30, auth = true,  version = "v2" },
        { path = "/orders/{id}",      method = "DELETE", rate_limit =  10, auth = true,  version = "v2" },
        { path = "/health",           method = "GET",    rate_limit = 1000,auth = false, version = "v2" },
        { path = "/metrics",          method = "GET",    rate_limit = 100, auth = true,  version = "v2" },
        { path = "/auth/login",       method = "POST",   rate_limit =  20, auth = false, version = "v2" },
        { path = "/auth/logout",      method = "POST",   rate_limit = 100, auth = true,  version = "v2" },
        { path = "/auth/refresh",     method = "POST",   rate_limit =  50, auth = true,  version = "v2" }
)
"#;

// ══════════════════════════════════════════════════════════════════════════════
// EQUIVALENT SERDE PAYLOADS
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
    let raw = [
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
        servers: raw.iter().map(|&(id, host, port, ssl, pool, region, weight)| {
            ServerEntry { id, host: host.into(), port, ssl, pool_size: pool,
                          timeout: 5000, region: region.into(), weight }
        }).collect(),
    }
}

fn make_game_data() -> GameData {
    // Values match SRC_NESTED exactly: armor=hp/10, xp=hp/2, gold=hp/4 (integer division)
    let raw: &[(&str, i32, i32, i32, i32, i32, &str, f32)] = &[
        ("Goblin",        50,   10,   5,  25,  12, "standard", 0.30),
        ("Orc",          100,   20,  10,  50,  25, "standard", 0.30),
        ("Troll",        200,   40,  20, 100,  50, "standard", 0.30),
        ("Dark Elf",     150,   35,  15,  75,  37, "standard", 0.30),
        ("Skeleton",      75,   15,   7,  37,  18, "standard", 0.30),
        ("Zombie",        60,   12,   6,  30,  15, "standard", 0.30),
        ("Bandit",        80,   18,   8,  40,  20, "standard", 0.30),
        ("Wolf",          45,    8,   4,  22,  11, "standard", 0.30),
        ("Giant Spider", 120,   25,  12,  60,  30, "standard", 0.30),
        ("Dragon",      1000,  150, 100, 500, 250, "boss",     0.01),
    ];
    GameData {
        game_title: "DixScript Demo Game".into(),
        version:    "1.0.0".into(),
        difficulty: "normal".into(),
        enemies: raw.iter().map(|&(name, hp, dmg, armor, xp, gold, ai, spawn)| EnemyStats {
            name:       name.into(),
            health:     hp,
            damage:     dmg,
            armor,
            xp,
            gold,
            ai_type:    ai.into(),
            spawn_rate: spawn,
            loot:       vec!["health_potion".into(), "gold_coin".into()],
        }).collect(),
    }
}

fn make_api_config() -> ApiConfig {
    let raw: &[(&str, &str, i32, bool)] = &[
        ("/users",         "GET",     200, true),
        ("/users",         "POST",     50, true),
        ("/users/{id}",    "GET",     200, true),
        ("/users/{id}",    "PUT",      50, true),
        ("/users/{id}",    "DELETE",   20, true),
        ("/products",      "GET",     500, false),
        ("/products",      "POST",     50, true),
        ("/products/{id}", "GET",     500, false),
        ("/products/{id}", "PUT",      50, true),
        ("/products/{id}", "DELETE",   20, true),
        ("/orders",        "GET",     200, true),
        ("/orders",        "POST",     50, true),
        ("/orders/{id}",   "GET",     200, true),
        ("/orders/{id}",   "PUT",      30, true),
        ("/orders/{id}",   "DELETE",   10, true),
        ("/health",        "GET",    1000, false),
        ("/metrics",       "GET",     100, true),
        ("/auth/login",    "POST",     20, false),
        ("/auth/logout",   "POST",    100, true),
        ("/auth/refresh",  "POST",     50, true),
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
// DIXSCRIPT PIPELINE HELPER
//
// Runs: ConfigSectionHandler → Tokenizer → GeneralParser → BinaryPacker
// No QuickFuncs / value resolution step — all DATA values are plain literals.
// ══════════════════════════════════════════════════════════════════════════════

fn dixscript_binary(source: &str) -> Vec<u8> {
    ErrorManager::get_shared_instance().clear_errors();
    let cr  = ConfigSectionHandler::new(None).process_config_section(source);
    let tok = Tokenizer::new(cr.cleaned_input_string.clone()).tokenize();
    let ast = GeneralParser::new(
        tok.tokens,
        cr.config_section.clone(),
        cr.operational_settings.clone(),
    )
    .expect("GeneralParser::new failed")
    .parse()
    .expect("GeneralParser::parse failed");

    let result = BinaryPacker::new().pack(&ast);
    assert!(
        result.is_success,
        "BinaryPacker failed — errors: {:?}",
        result.errors
    );
    result.binary_data
}

fn dixscript_unpack(bytes: &[u8]) -> DixScript {
    let result = BinaryUnpacker::new().unpack(bytes);
    assert!(
        result.is_success,
        "BinaryUnpacker failed — errors: {:?}",
        result.errors
    );
    result.ast.expect("BinaryUnpacker returned no AST")
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
    let mut enc = BzEncoder::new(Vec::new(), Bz2Level::best());
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
// SERIALISATION HELPERS
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

// ══════════════════════════════════════════════════════════════════════════════
// SIZE REPORT HELPERS
// ══════════════════════════════════════════════════════════════════════════════

struct SizeRow {
    format: &'static str,
    raw:    usize,
    gzip:   usize,
    bzip2:  usize,
    lzma:   usize,
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
    let json_raw = rows
        .iter()
        .find(|r| r.format == "JSON (compact)")
        .map(|r| r.raw as f64)
        .unwrap_or(1.0);

    println!("\n╔══════════════════════════════════════════════════════════════════════════════════════╗");
    println!("║  {}  — Binary Size Comparison", label);
    println!("╠══════════════════╦══════════╦══════════╦══════════╦══════════╦══════════╦═══════════╗");
    println!("║  Format          ║  Raw (B) ║ +gzip(B) ║+bzip2(B) ║ +lzma(B) ║  gz-red  ║  vs json  ║");
    println!("╠══════════════════╬══════════╬══════════╬══════════╬══════════╬══════════╬═══════════╣");

    for row in rows {
        let gz_red  = (1.0 - row.gzip as f64 / row.raw  as f64) * 100.0;
        let vs_json = (1.0 - row.raw  as f64 / json_raw)         * 100.0;
        println!(
            "║  {:<16} ║ {:>8} ║ {:>8} ║ {:>8} ║ {:>8} ║ {:>+7.1}% ║ {:>+8.1}% ║",
            row.format, row.raw, row.gzip, row.bzip2, row.lzma,
            -gz_red, vs_json,
        );
    }
    println!("╠══════════════════╩══════════╩══════════╩══════════╩══════════╩══════════╩═══════════╣");
    let best_raw  = rows.iter().min_by_key(|r| r.raw).unwrap();
    let best_gz   = rows.iter().min_by_key(|r| r.gzip).unwrap();
    let best_lzma = rows.iter().min_by_key(|r| r.lzma).unwrap();
    println!("║  Smallest raw:    {:<18} ({} B)", best_raw.format,  best_raw.raw);
    println!("║  Smallest +gzip:  {:<18} ({} B)", best_gz.format,   best_gz.gzip);
    println!("║  Smallest +lzma:  {:<18} ({} B)", best_lzma.format, best_lzma.lzma);
    println!("╚══════════════════════════════════════════════════════════════════════════════════════╝");
}

// ══════════════════════════════════════════════════════════════════════════════
// SPEED HELPERS
// ══════════════════════════════════════════════════════════════════════════════

struct SpeedRow {
    format:    &'static str,
    encode_us: f64,
    decode_us: f64,
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
    let enc_t  = Instant::now();
    for _ in 0..n { let _ = dixscript_binary(source); }
    let enc_us = enc_t.elapsed().as_micros() as f64 / n as f64;

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
    for row in &rows { assert!(row.raw > 0, "{} produced 0 bytes", row.format); }
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

    print_size_table("Multi-Server Config (8 servers)", &rows);
    for row in &rows { assert!(row.raw > 0, "{} produced 0 bytes", row.format); }
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

    print_size_table("Game Enemy Data (10 enemies, pre-computed fields)", &rows);
    for row in &rows { assert!(row.raw > 0, "{} produced 0 bytes", row.format); }
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

    print_size_table("API Endpoints (20 endpoints)", &rows);
    for row in &rows { assert!(row.raw > 0, "{} produced 0 bytes", row.format); }
}

// ══════════════════════════════════════════════════════════════════════════════
// ROUND-TRIP FIDELITY TESTS
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn roundtrip_json_flat() {
    let original = make_flat_config();
    let decoded: FlatConfig = serde_json::from_slice(&encode_json(&original)).unwrap();
    assert_eq!(original, decoded);
    println!("[rt_json_flat] ok — {} bytes", encode_json(&original).len());
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
    // Verifies BinaryPacker → BinaryUnpacker preserves the DATA section.
    let dx       = dixscript_binary(SRC_NESTED);
    let restored = dixscript_unpack(&dx);
    assert!(restored.data.is_some(), "DATA section lost in DixScript round-trip");
    let entry_count = restored.data.as_ref().unwrap().entries.len();
    assert!(entry_count > 0, "DATA section has no entries after round-trip");
    println!(
        "[rt_dixscript] ok — {} bytes → {} DATA entries",
        dx.len(), entry_count
    );
}

#[test]
fn roundtrip_dixscript_all_sources() {
    for (label, src) in [
        ("flat",       SRC_FLAT),
        ("objects",    SRC_OBJECTS),
        ("nested",     SRC_NESTED),
        ("repetitive", SRC_REPETITIVE),
    ] {
        let bytes    = dixscript_binary(src);
        let restored = dixscript_unpack(&bytes);
        assert!(restored.data.is_some(), "[{}] DATA section missing after round-trip", label);
        println!("[rt_dixscript/{}] ok — {} bytes", label, bytes.len());
    }
}

#[test]
fn roundtrip_all_formats_game_data() {
    let original = make_game_data();
    let j: GameData = serde_json::from_slice(&encode_json(&original)).unwrap();
    assert_eq!(original, j, "JSON round-trip failed");
    let b: GameData = bincode::deserialize(&encode_bincode(&original)).unwrap();
    assert_eq!(original, b, "Bincode round-trip failed");
    let m: GameData = rmp_serde::from_slice(&encode_msgpack(&original)).unwrap();
    assert_eq!(original, m, "MessagePack round-trip failed");
    let p: GameData = postcard::from_bytes(&encode_postcard(&original)).unwrap();
    assert_eq!(original, p, "Postcard round-trip failed");
    let c: GameData = ciborium::from_reader(encode_cbor(&original).as_slice()).unwrap();
    assert_eq!(original, c, "CBOR round-trip failed");
    println!("[rt_all_formats_game] all 5 non-DixScript formats OK");
}

// ══════════════════════════════════════════════════════════════════════════════
// COMPRESSION FIDELITY TESTS
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn compression_roundtrip_gzip() {
    let data       = encode_json(&make_game_data());
    let compressed = gzip_compress(&data);
    let restored   = gzip_decompress(&compressed);
    assert_eq!(data, restored);
    println!("[gzip_rt] {} → {} → {} B", data.len(), compressed.len(), restored.len());
}

#[test]
fn compression_roundtrip_bzip2() {
    let data       = encode_json(&make_game_data());
    let compressed = bzip2_compress(&data);
    let restored   = bzip2_decompress(&compressed);
    assert_eq!(data, restored);
    println!("[bzip2_rt] {} → {} → {} B", data.len(), compressed.len(), restored.len());
}

#[test]
fn compression_roundtrip_lzma() {
    let data       = encode_json(&make_game_data());
    let compressed = lzma_compress(&data);
    let restored   = lzma_decompress(&compressed);
    assert_eq!(data, restored);
    println!("[lzma_rt] {} → {} → {} B", data.len(), compressed.len(), restored.len());
}

// ══════════════════════════════════════════════════════════════════════════════
// COMPREHENSIVE SUMMARY
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn summary_all_datasets_all_formats() {
    let datasets = [
        ("Flat Config (10 fields)",            SRC_FLAT),
        ("Multi-Server (8 repeated structs)",  SRC_OBJECTS),
        ("Game Data (10 enemies+fields)",      SRC_NESTED),
        ("API Endpoints (20 repeated structs)",SRC_REPETITIVE),
    ];

    let json_payloads: Vec<Vec<u8>> = vec![
        encode_json(&make_flat_config()),
        encode_json(&make_multi_server()),
        encode_json(&make_game_data()),
        encode_json(&make_api_config()),
    ];
    let bc_payloads: Vec<Vec<u8>> = vec![
        encode_bincode(&make_flat_config()),
        encode_bincode(&make_multi_server()),
        encode_bincode(&make_game_data()),
        encode_bincode(&make_api_config()),
    ];
    let pc_payloads: Vec<Vec<u8>> = vec![
        encode_postcard(&make_flat_config()),
        encode_postcard(&make_multi_server()),
        encode_postcard(&make_game_data()),
        encode_postcard(&make_api_config()),
    ];

    println!("\n");
    println!("╔════════════════════════════════════════════════════════════════════════════════════════════╗");
    println!("║                     COMPREHENSIVE FORMAT × COMPRESSION SUMMARY                            ║");
    println!("╠═══════════════════════════╦══════════════════╦══════════════════╦══════════════╦══════════╣");
    println!("║  Dataset                  ║  DixScript       ║  JSON+gzip       ║  Bincode+gz  ║ Postcard ║");
    println!("╠═══════════════════════════╬══════════════════╬══════════════════╬══════════════╬══════════╣");

    for (i, (label, src)) in datasets.iter().enumerate() {
        let dx_raw  = dixscript_binary(src);
        let dx_gz   = gzip_compress(&dx_raw);
        let json_gz = gzip_compress(&json_payloads[i]);
        let bc_gz   = gzip_compress(&bc_payloads[i]);
        let pc_raw  = &pc_payloads[i];
        println!(
            "║  {:<25}  ║ raw={:>5} gz={:>5} ║ raw={:>5} gz={:>5} ║ raw={:>5} gz={:>4} ║ raw={:>5} ║",
            label,
            dx_raw.len(), dx_gz.len(),
            json_payloads[i].len(), json_gz.len(),
            bc_payloads[i].len(), bc_gz.len(),
            pc_raw.len(),
        );
    }
    println!("╚════════════════════════════════════════════════════════════════════════════════════════════╝");
}

// ══════════════════════════════════════════════════════════════════════════════
// SPEED BENCHMARKS (ignored by default — run with --include-ignored)
// ══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "slow — run with: cargo test bench_encode_speed_flat -- --ignored --nocapture"]
fn bench_encode_speed_flat_config() {
    const N: u32 = 10_000;
    let payload = make_flat_config();
    let mut rows = vec![{
        let (enc, dec) = time_dixscript_pack(SRC_FLAT, 200);
        SpeedRow { format: "DixScript", encode_us: enc, decode_us: dec }
    }];
    rows.push(SpeedRow {
        format:    "JSON",
        encode_us: time_encode(&payload, encode_json, N),
        decode_us: time_decode_json::<FlatConfig>(&encode_json(&payload), N),
    });
    rows.push(SpeedRow {
        format:    "Bincode",
        encode_us: time_encode(&payload, encode_bincode, N),
        decode_us: time_decode_bincode::<FlatConfig>(&encode_bincode(&payload), N),
    });
    rows.push(SpeedRow {
        format:    "MessagePack",
        encode_us: time_encode(&payload, encode_msgpack, N),
        decode_us: time_decode_msgpack::<FlatConfig>(&encode_msgpack(&payload), N),
    });
    rows.push(SpeedRow {
        format:    "Postcard",
        encode_us: time_encode(&payload, encode_postcard, N),
        decode_us: time_decode_postcard::<FlatConfig>(&encode_postcard(&payload), N),
    });
    rows.push(SpeedRow {
        format:    "CBOR",
        encode_us: time_encode(&payload, encode_cbor, N),
        decode_us: time_decode_cbor::<FlatConfig>(&encode_cbor(&payload), N),
    });
    print_speed_table("Flat Config ×10k (DixScript ×200)", &rows);
}

#[test]
#[ignore = "slow — run with: cargo test bench_encode_speed_game -- --ignored --nocapture"]
fn bench_encode_speed_game_data() {
    const N: u32 = 5_000;
    let payload = make_game_data();
    let mut rows = vec![{
        let (enc, dec) = time_dixscript_pack(SRC_NESTED, 100);
        SpeedRow { format: "DixScript", encode_us: enc, decode_us: dec }
    }];
    rows.push(SpeedRow {
        format:    "JSON",
        encode_us: time_encode(&payload, encode_json, N),
        decode_us: time_decode_json::<GameData>(&encode_json(&payload), N),
    });
    rows.push(SpeedRow {
        format:    "Bincode",
        encode_us: time_encode(&payload, encode_bincode, N),
        decode_us: time_decode_bincode::<GameData>(&encode_bincode(&payload), N),
    });
    rows.push(SpeedRow {
        format:    "MessagePack",
        encode_us: time_encode(&payload, encode_msgpack, N),
        decode_us: time_decode_msgpack::<GameData>(&encode_msgpack(&payload), N),
    });
    rows.push(SpeedRow {
        format:    "Postcard",
        encode_us: time_encode(&payload, encode_postcard, N),
        decode_us: time_decode_postcard::<GameData>(&encode_postcard(&payload), N),
    });
    rows.push(SpeedRow {
        format:    "CBOR",
        encode_us: time_encode(&payload, encode_cbor, N),
        decode_us: time_decode_cbor::<GameData>(&encode_cbor(&payload), N),
    });
    print_speed_table("Game Data ×5k (DixScript ×100)", &rows);
}

#[test]
#[ignore = "slow — run with: cargo test bench_compression_speed -- --ignored --nocapture"]
fn bench_compression_speed() {
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
        let t1         = Instant::now();
        for _ in 0..N { let _ = decompress(&compressed); }
        let decomp_us = t1.elapsed().as_micros() as f64 / N as f64;
        println!("║  {:<10} ║ {:>13.2} ║ {:>13.2} ║ {:>12} ║", name, comp_us, decomp_us, out_bytes);
    }
    println!("╚════════════╩═══════════════╩═══════════════╩══════════════╝");
    }
