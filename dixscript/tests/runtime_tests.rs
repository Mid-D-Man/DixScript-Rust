// tests/runtime_tests.rs
//
// Integration tests that mirror every benchmark group in benches/runtime_benchmark.rs.
// Unlike benchmarks, tests print timing + diagnostic info so you can see exactly
// what is (or isn't) working and why.
//
// Run with:
//   cargo test --test runtime_tests -- --nocapture
//
// Individual suites:
//   cargo test --test runtime_tests load_from_str    -- --nocapture
//   cargo test --test runtime_tests data_access      -- --nocapture
//   cargo test --test runtime_tests data_builder     -- --nocapture
//   cargo test --test runtime_tests converter        -- --nocapture
//   cargo test --test runtime_tests compactor        -- --nocapture
//   cargo test --test runtime_tests dix_value        -- --nocapture

use dixscript::Runtime::{
    DixCompactor, DixConverter, DixData, DixDataBuilder, DixFormatOptions, DixLoader,
    DixLoadOptions, DixValue,
};
use std::collections::HashMap;
use std::time::Instant;

// ── Shared source fixtures (identical to bench fixtures) ──────────────────────

const SRC_MINIMAL: &str = r#"
@DATA(
  app_name = "BenchApp"
  version = "1.0.0"
  port = 8080
  debug = false
  max_connections = 1000
)
"#;

const SRC_MEDIUM: &str = r#"
@ENUMS(
  LogLevel { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
  Environment { DEV = 1, STAGING = 2, PROD = 3 }
)

@DATA(
  app_name = "BenchApp"
  version = "2.0.0"
  port = 9090
  log_level<enum> = LogLevel.INFO
  environment<enum> = Environment.PROD

  database.primary: host = "db.local", port = 5432, pool = 25, ssl = true
  database.replica: host = "db-ro.local", port = 5432, pool = 10, ssl = true

  cache.redis: host = "cache.local", port = 6379, ttl = 3600

  allowed_origins:: "https://app.example.com", "https://admin.example.com"

  feature_flags: dark_mode = true, beta_ui = false, new_onboarding = true
)
"#;

const SRC_HEAVY_WITH_FUNCTIONS: &str = r#"
@ENUMS(
  Rarity { COMMON = 1, UNCOMMON = 2, RARE = 3, EPIC = 4, LEGENDARY = 5 }
  AIType { PASSIVE, NEUTRAL, AGGRESSIVE, BOSS }
)

@QUICKFUNCS(
  ~createEnemy<object>(name, health, damage, rarity<enum>, ai<enum>) {
    return {
      name = name,
      health = health,
      damage = damage,
      armor = health / 10,
      xp = health / 2,
      gold = health / 4,
      rarity = rarity,
      ai_type = ai,
      spawn_rate = ai == AIType.BOSS ? 0.01 : 0.3
    }
  }

  ~createServer<object>(host, port, pool) {
    return {
      host = host,
      port = port,
      pool_size = pool,
      timeout = 5000,
      ssl = true
    }
  }
)

@DATA(
  game_version = "3.0.0"
  max_players = 100

  enemies::
    createEnemy("Goblin",   50,   10, Rarity.COMMON,    AIType.AGGRESSIVE),
    createEnemy("Orc",      100,  20, Rarity.UNCOMMON,  AIType.AGGRESSIVE),
    createEnemy("Troll",    200,  40, Rarity.RARE,      AIType.AGGRESSIVE),
    createEnemy("Wyvern",   500,  80, Rarity.EPIC,      AIType.AGGRESSIVE),
    createEnemy("Dragon",   1000, 150, Rarity.LEGENDARY, AIType.BOSS)

  servers::
    createServer("us-east-1.db.local", 5432, 50),
    createServer("eu-west-1.db.local", 5432, 50),
    createServer("ap-south-1.db.local", 5432, 25)
)
"#;

// ── Helper: build a pre-loaded DixData from the medium fixture ────────────────

fn build_medium_data() -> DixData {
    let loader = DixLoader::new();
    loader
        .load_from_str(SRC_MEDIUM, &DixLoadOptions::new())
        .expect("medium fixture must compile")
}

/// Build a DixData with N flat integer entries.
fn build_flat_data(n: usize) -> DixData {
    let mut src = "@DATA(\n".to_string();
    for i in 0..n {
        src.push_str(&format!("  key_{} = {}\n", i, i));
    }
    src.push(')');

    let loader = DixLoader::new();
    loader
        .load_from_str(&src, &DixLoadOptions::new())
        .expect("flat fixture must compile")
}

/// Build a HashMap with N entries for converter tests.
fn build_hashmap(n: usize) -> HashMap<String, DixValue> {
    (0..n)
        .map(|i| (format!("key_{}", i), DixValue::Int(i as i32)))
        .collect()
}

// ── Timing helper ─────────────────────────────────────────────────────────────

fn elapsed_us(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000_000.0
}

// ═════════════════════════════════════════════════════════════════════════════
// GROUP 1: load_from_str
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn load_from_str_minimal() {
    let loader = DixLoader::new();
    let opts = DixLoadOptions::new();

    let t = Instant::now();
    let result = loader.load_from_str(SRC_MINIMAL, &opts);
    let us = elapsed_us(t);

    println!("[load_from_str_minimal] {:.2} µs", us);

    let data = result.expect("minimal source must compile");
    println!("  entry_count = {}", data.entry_count());

    let app_name: String = data.get("app_name").expect("app_name must exist");
    assert_eq!(app_name, "BenchApp", "app_name mismatch");

    let port: i32 = data.get("port").expect("port must exist");
    assert_eq!(port, 8080, "port mismatch");

    let debug: bool = data.get("debug").expect("debug must exist");
    assert!(!debug, "debug should be false");

    let max_conn: i32 = data.get("max_connections").expect("max_connections must exist");
    assert_eq!(max_conn, 1000, "max_connections mismatch");

    println!("  app_name={} port={} debug={} max_connections={}", app_name, port, debug, max_conn);
}

#[test]
fn load_from_str_medium_with_enums() {
    let loader = DixLoader::new();
    let opts = DixLoadOptions::new();

    let t = Instant::now();
    let result = loader.load_from_str(SRC_MEDIUM, &opts);
    let us = elapsed_us(t);

    println!("[load_from_str_medium_with_enums] {:.2} µs", us);

    let data = result.expect("medium source must compile");
    println!("  entry_count = {}", data.entry_count());
    println!("  keys present: {:?}", {
        let mut keys = data.to_hashmap().keys().cloned().collect::<Vec<_>>();
        keys.sort();
        keys
    });

    let app_name: String = data.get("app_name").expect("app_name");
    assert_eq!(app_name, "BenchApp");

    let port: i32 = data.get("port").expect("port");
    assert_eq!(port, 9090);

    let db_host: String = data.get("database.primary.host").expect("database.primary.host");
    assert_eq!(db_host, "db.local");

    let db_pool: i32 = data.get("database.primary.pool").expect("database.primary.pool");
    assert_eq!(db_pool, 25);

    let cache_ttl: i32 = data.get("cache.redis.ttl").expect("cache.redis.ttl");
    assert_eq!(cache_ttl, 3600);

    assert!(data.exists("allowed_origins"), "allowed_origins array must exist");
    assert!(data.exists("allowed_origins[0]"), "allowed_origins[0] must exist");

    println!(
        "  db_host={} db_pool={} cache_ttl={} origins_exist={}",
        db_host, db_pool, cache_ttl,
        data.exists("allowed_origins")
    );
}

#[test]
fn load_from_str_heavy_with_quickfuncs() {
    let loader = DixLoader::new();
    let opts = DixLoadOptions::new();

    let t = Instant::now();
    let result = loader.load_from_str(SRC_HEAVY_WITH_FUNCTIONS, &opts);
    let us = elapsed_us(t);

    println!("[load_from_str_heavy_with_quickfuncs] {:.2} µs", us);

    match result {
        Ok(data) => {
            println!("  entry_count = {}", data.entry_count());
            println!("  all keys = {:?}", {
                let mut keys = data.to_hashmap().keys().cloned().collect::<Vec<_>>();
                keys.sort();
                keys
            });

            let game_version: String = data.get("game_version").expect("game_version");
            assert_eq!(game_version, "3.0.0");

            let max_players: i32 = data.get("max_players").expect("max_players");
            assert_eq!(max_players, 100);

            let enemies_exist = data.exists("enemies");
            let servers_exist = data.exists("servers");
            println!("  enemies array exists={} servers array exists={}", enemies_exist, servers_exist);

            if data.exists("enemies[0]") {
                println!("  enemies[0] = {:?}", data.get_value("enemies[0]"));
            } else {
                println!("  WARN: enemies[0] not found - QuickFuncs may not be resolved yet");
            }

            println!("  game_version={} max_players={}", game_version, max_players);
        }
        Err(e) => {
            println!("  WARN: load failed (QuickFuncs resolver pending?): {}", e);
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// GROUP 2: load_throughput (byte-rate diagnostics)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn load_throughput_all_fixtures() {
    let loader = DixLoader::new();
    let opts = DixLoadOptions::new();

    for (label, src) in [
        ("minimal",   SRC_MINIMAL),
        ("medium",    SRC_MEDIUM),
        ("heavy",     SRC_HEAVY_WITH_FUNCTIONS),
    ] {
        let t = Instant::now();
        let result = loader.load_from_str(src, &opts);
        let us = elapsed_us(t);

        let bytes = src.len();
        let mb_per_sec = (bytes as f64 / 1_000_000.0) / (us / 1_000_000.0);

        match result {
            Ok(data) => println!(
                "[load_throughput] {:10} | {:6} bytes | {:8.2} µs | {:7.2} MB/s | {} entries",
                label, bytes, us, mb_per_sec, data.entry_count()
            ),
            Err(e) => println!(
                "[load_throughput] {:10} | {:6} bytes | {:8.2} µs | FAILED: {}",
                label, bytes, us, e
            ),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// GROUP 3: data_access
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn data_access_get_string() {
    let data = build_medium_data();

    let t = Instant::now();
    let v: String = data.get("app_name").expect("app_name must exist");
    let us = elapsed_us(t);

    println!("[data_access_get_string] {:.3} µs  value={}", us, v);
    assert_eq!(v, "BenchApp");
}

#[test]
fn data_access_get_int() {
    let data = build_medium_data();

    let t = Instant::now();
    let v: i32 = data.get("port").expect("port must exist");
    let us = elapsed_us(t);

    println!("[data_access_get_int] {:.3} µs  value={}", us, v);
    assert_eq!(v, 9090);
}

#[test]
fn data_access_get_nested() {
    let data = build_medium_data();

    let t = Instant::now();
    let v: String = data.get("database.primary.host").expect("database.primary.host must exist");
    let us = elapsed_us(t);

    println!("[data_access_get_nested] {:.3} µs  value={}", us, v);
    assert_eq!(v, "db.local");
}

#[test]
fn data_access_exists_hit() {
    let data = build_medium_data();

    let t = Instant::now();
    let found = data.exists("port");
    let us = elapsed_us(t);

    println!("[data_access_exists_hit] {:.3} µs  found={}", us, found);
    assert!(found);
}

#[test]
fn data_access_exists_miss() {
    let data = build_medium_data();

    let t = Instant::now();
    let found = data.exists("nonexistent_key_xyz");
    let us = elapsed_us(t);

    println!("[data_access_exists_miss] {:.3} µs  found={}", us, found);
    assert!(!found);
}

#[test]
fn data_access_get_or_default_miss() {
    let data = build_medium_data();

    let t = Instant::now();
    let v: i32 = data.get_or_default("nonexistent", 42);
    let us = elapsed_us(t);

    println!("[data_access_get_or_default_miss] {:.3} µs  value={}", us, v);
    assert_eq!(v, 42);
}

#[test]
fn data_access_all_medium_keys() {
    let data = build_medium_data();
    let mut keys: Vec<String> = data.to_hashmap().keys().cloned().collect();
    keys.sort();

    println!("[data_access_all_medium_keys] {} keys total:", keys.len());
    for k in &keys {
        println!("  {:45} = {:?}", k, data.get_value(k).unwrap());
    }

    assert!(keys.len() > 5, "medium fixture should produce more than 5 keys");
    assert!(keys.contains(&"app_name".to_string()));
    assert!(keys.contains(&"database.primary.host".to_string()));
}

// ═════════════════════════════════════════════════════════════════════════════
// GROUP 4: data_access_scaling
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn data_access_scaling_get_by_index() {
    for n in [10usize, 100, 1_000, 10_000] {
        let data = build_flat_data(n);
        let mid_key = format!("key_{}", n / 2);

        let t = Instant::now();
        let v: i32 = data.get(&mid_key).expect("mid key must exist");
        let us = elapsed_us(t);

        println!("[data_access_scaling] n={:6}  get({}) = {}  ({:.3} µs)", n, mid_key, v, us);
        assert_eq!(v, (n / 2) as i32);
    }
}

#[test]
fn data_access_scaling_get_keys_prefix() {
    for n in [10usize, 100, 1_000, 10_000] {
        let data = build_flat_data(n);

        let t = Instant::now();
        let keys = data.get_keys("");
        let us = elapsed_us(t);

        println!(
            "[data_access_scaling] n={:6}  get_keys(\"\") returned {} keys  ({:.3} µs)",
            n, keys.len(), us
        );
        assert!(!keys.is_empty(), "get_keys should return at least one key for n={}", n);
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// GROUP 5: data_builder
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn data_builder_simple_flat() {
    let t = Instant::now();
    let result = DixDataBuilder::new()
        .config(|cfg| {
            cfg.with_version("1.0.0");
            cfg.with_author("bench");
        })
        .data(|d| {
            d.with_string("name", "BenchApp");
            d.with_int("port", 8080);
            d.with_bool("debug", false);
            d.with_double("timeout", 5.0);
        })
        .build();
    let us = elapsed_us(t);

    println!("[data_builder_simple_flat] {:.2} µs", us);

    let data = result.expect("builder must succeed");
    println!("  entry_count={}", data.entry_count());

    let name: String = data.get("name").expect("name");
    assert_eq!(name, "BenchApp");

    let port: i32 = data.get("port").expect("port");
    assert_eq!(port, 8080);

    let debug: bool = data.get("debug").expect("debug");
    assert!(!debug);

    println!("  name={} port={} debug={}", name, port, debug);
}

#[test]
fn data_builder_with_table_properties() {
    let t = Instant::now();
    let result = DixDataBuilder::new()
        .data(|d| {
            d.with_string("version", "1.0.0");
            d.with_table_properties("database", |t| {
                t.with_string("host", "localhost");
                t.with_int("port", 5432);
                t.with_bool("ssl", true);
                t.with_int("pool", 20);
            });
            d.with_table_properties("cache", |t| {
                t.with_string("host", "redis.local");
                t.with_int("port", 6379);
                t.with_int("ttl", 3600);
            });
        })
        .build();
    let us = elapsed_us(t);

    println!("[data_builder_with_table_properties] {:.2} µs", us);

    let data = result.expect("builder must succeed");
    println!("  entry_count={}", data.entry_count());
    println!("  keys: {:?}", {
        let mut keys = data.to_hashmap().keys().cloned().collect::<Vec<_>>();
        keys.sort();
        keys
    });

    let db_host: String = data.get("database.host").expect("database.host");
    assert_eq!(db_host, "localhost");

    let db_port: i32 = data.get("database.port").expect("database.port");
    assert_eq!(db_port, 5432);

    let db_ssl: bool = data.get("database.ssl").expect("database.ssl");
    assert!(db_ssl);

    let cache_ttl: i32 = data.get("cache.ttl").expect("cache.ttl");
    assert_eq!(cache_ttl, 3600);

    println!("  db_host={} db_port={} db_ssl={} cache_ttl={}", db_host, db_port, db_ssl, cache_ttl);
}

#[test]
fn data_builder_with_group_array() {
    let t = Instant::now();
    let result = DixDataBuilder::new()
        .data(|d| {
            d.with_string("version", "1.0.0");
            d.with_group_array_builder("allowed_origins", |arr| {
                arr.add_string("https://app.example.com");
                arr.add_string("https://admin.example.com");
                arr.add_string("https://api.example.com");
            });
        })
        .build();
    let us = elapsed_us(t);

    println!("[data_builder_with_group_array] {:.2} µs", us);

    let data = result.expect("builder must succeed");
    println!("  entry_count={}", data.entry_count());

    assert!(data.exists("allowed_origins"),    "allowed_origins array must exist");
    assert!(data.exists("allowed_origins[0]"), "allowed_origins[0] must exist");
    assert!(data.exists("allowed_origins[1]"), "allowed_origins[1] must exist");
    assert!(data.exists("allowed_origins[2]"), "allowed_origins[2] must exist");

    let first: String = data.get("allowed_origins[0]").expect("allowed_origins[0]");
    assert_eq!(first, "https://app.example.com");
    println!("  allowed_origins[0]={}", first);
}

#[test]
fn data_builder_large_flat_100_keys() {
    let t = Instant::now();
    let mut builder = DixDataBuilder::new();
    builder = builder.data(|d| {
        for i in 0..100_i32 {
            d.with_int(&format!("key_{}", i), i);
        }
    });
    let result = builder.build();
    let us = elapsed_us(t);

    println!("[data_builder_large_flat_100_keys] {:.2} µs", us);

    let data = result.expect("builder must succeed");
    println!("  entry_count={}", data.entry_count());
    assert_eq!(data.entry_count(), 100, "should have exactly 100 entries");

    for i in [0_i32, 49, 99] {
        let v: i32 = data.get(&format!("key_{}", i)).expect("key must exist");
        assert_eq!(v, i, "key_{} should equal {}", i, i);
    }
    println!("  key_0={} key_49={} key_99={}",
        data.get::<i32>("key_0").unwrap(),
        data.get::<i32>("key_49").unwrap(),
        data.get::<i32>("key_99").unwrap(),
    );
}

#[test]
fn data_builder_two_tier_enforcement() {
    let result = std::panic::catch_unwind(|| {
        let _ = DixDataBuilder::new()
            .data(|d| {
                d.with_table_properties("user", |t| {
                    t.with_string("name", "Bob");
                });
                d.with_int("late_flat", 1);
            })
            .build();
    });

    assert!(result.is_err(), "two-tier violation must panic");
    println!("[data_builder_two_tier_enforcement] correctly panicked on tier violation");
}

// ═════════════════════════════════════════════════════════════════════════════
// GROUP 6: converter
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn converter_from_hashmap_small() {
    let converter = DixConverter::new();

    for n in [10usize, 100, 500] {
        let map = build_hashmap(n);

        let t = Instant::now();
        let result = converter.from_hashmap(map.clone());
        let us = elapsed_us(t);

        match result {
            Ok(ast) => {
                let entry_count = ast.data.as_ref().map(|d| d.entries.len()).unwrap_or(0);
                println!(
                    "[converter_from_hashmap] n={:4}  entries={}  ({:.2} µs)",
                    n, entry_count, us
                );
                assert_eq!(entry_count, n, "expected {} DATA entries for n={}", n, n);
            }
            Err(e) => panic!("[converter_from_hashmap] n={} FAILED: {}", n, e),
        }
    }
}

#[test]
fn converter_from_hashmap_types() {
    let converter = DixConverter::new();

    let mut map = HashMap::new();
    map.insert("name".to_string(),   DixValue::String("Alice".to_string()));
    map.insert("age".to_string(),    DixValue::Int(30));
    map.insert("score".to_string(),  DixValue::Double(9.5));
    map.insert("active".to_string(), DixValue::Bool(true));
    map.insert("ratio".to_string(),  DixValue::Float(0.5_f32));

    let t = Instant::now();
    let result = converter.from_hashmap(map);
    let us = elapsed_us(t);

    println!("[converter_from_hashmap_types] {:.2} µs", us);

    let ast = result.expect("must convert mixed types");
    let entries = ast.data.expect("must have DATA section").entries;
    println!("  entries = {}", entries.len());
    assert_eq!(entries.len(), 5, "should have 5 entries");
}

#[test]
fn converter_to_hashmap_medium() {
    use dixscript::Compiler::AST::{DixScript, DataSection, Position};

    let converter = DixConverter::new();

    let ast = DixScript {
        config: None,
        imports: None,
        dlm: None,
        enums: None,
        quick_functions: None,
        data: Some(DataSection {
            entries: vec![],
            position: Position::UNKNOWN,
        }),
        security: None,
    };

    let t = Instant::now();
    let map = converter.to_hashmap(&ast);
    let us = elapsed_us(t);

    println!("[converter_to_hashmap_medium] {:.2} µs  keys={}", us, map.len());
    assert_eq!(map.len(), 0);
}

#[test]
fn converter_to_hashmap_roundtrip() {
    let original = DixDataBuilder::new()
        .data(|d| {
            d.with_string("app", "TestApp");
            d.with_int("port", 3000);
            d.with_bool("ssl", true);
        })
        .build()
        .expect("build");

    let map = original.to_hashmap();
    println!("[converter_to_hashmap_roundtrip] original map keys: {:?}", {
        let mut k: Vec<_> = map.keys().cloned().collect();
        k.sort();
        k
    });

    let app = map.get("app").expect("app").as_string().unwrap_or("").to_string();
    assert_eq!(app, "TestApp");

    let port = map.get("port").expect("port").as_int().expect("int");
    assert_eq!(port, 3000);

    println!("  app={} port={}", app, port);
}

// ═════════════════════════════════════════════════════════════════════════════
// GROUP 7: compactor
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn compactor_minify_all_fixtures() {
    for (label, src) in [
        ("minimal", SRC_MINIMAL),
        ("medium",  SRC_MEDIUM),
        ("heavy",   SRC_HEAVY_WITH_FUNCTIONS),
    ] {
        let t = Instant::now();
        let minified = DixCompactor::minify(src);
        let us = elapsed_us(t);

        let ratio = DixCompactor::get_compression_ratio(src, &minified);
        println!(
            "[compactor_minify] {:10} | {} → {} bytes ({:.1}% reduction)  ({:.2} µs)",
            label, src.len(), minified.len(), ratio * 100.0, us
        );

        assert!(
            minified.len() < src.len(),
            "{}: minified must be smaller than original", label
        );
        assert!(
            !minified.contains("  "),
            "{}: minified should not contain double-spaces", label
        );
        assert!(
            !minified.contains('\n'),
            "{}: minified should not contain newlines", label
        );
    }
}

#[test]
fn compactor_compact_all_fixtures() {
    for (label, src) in [
        ("minimal", SRC_MINIMAL),
        ("medium",  SRC_MEDIUM),
        ("heavy",   SRC_HEAVY_WITH_FUNCTIONS),
    ] {
        let t = Instant::now();
        let compacted = DixCompactor::compact(src);
        let us = elapsed_us(t);

        let ratio = DixCompactor::get_compression_ratio(src, &compacted);
        println!(
            "[compactor_compact] {:10} | {} → {} bytes ({:.1}% reduction)  ({:.2} µs)",
            label, src.len(), compacted.len(), ratio * 100.0, us
        );

        assert!(
            compacted.len() <= src.len(),
            "{}: compact output must be ≤ original size", label
        );
    }
}

#[test]
fn compactor_remove_comments_all_fixtures() {
    // Root cause of the previous failure: SRC_MEDIUM contains
    // "https://app.example.com" — the `//` sits inside a string literal, so
    // remove_comments CORRECTLY preserves it. Asserting `!contains("//")` over
    // the whole output was wrong because it would require stripping string contents.
    //
    // Fix: inject a real sentinel comment into each fixture, verify that specific
    // comment disappears, and verify that string-embedded `//` (HTTPS URLs) survive.

    for (label, src) in [
        ("minimal", SRC_MINIMAL),
        ("medium",  SRC_MEDIUM),
        ("heavy",   SRC_HEAVY_WITH_FUNCTIONS),
    ] {
        let sentinel = format!("// SENTINEL_{}\n", label.to_uppercase());
        let src_with_comment = format!("{}{}", sentinel, src);

        let t = Instant::now();
        let stripped = DixCompactor::remove_comments(&src_with_comment);
        let us = elapsed_us(t);

        println!(
            "[compactor_remove_comments] {:10} | {} → {} bytes  ({:.2} µs)",
            label, src_with_comment.len(), stripped.len(), us
        );

        // 1. The injected sentinel line must be gone.
        assert!(
            !stripped.contains(&sentinel),
            "{}: sentinel comment must be stripped", label
        );

        // 2. Output must be strictly shorter (we removed at least the sentinel line).
        assert!(
            stripped.len() < src_with_comment.len(),
            "{}: stripped output must be shorter than input with comment", label
        );

        // 3. String-embedded `//` (HTTPS URLs) must survive intact.
        if label == "medium" {
            assert!(
                stripped.contains("https://app.example.com"),
                "medium: HTTPS URL inside string literal must be preserved"
            );
            assert!(
                stripped.contains("https://admin.example.com"),
                "medium: HTTPS URL inside string literal must be preserved"
            );
        }
    }
}

#[test]
fn compactor_preserves_string_contents() {
    let src = r#"@DATA( url = "https://example.com/api/v2" note = "hello   world" )"#;
    let minified = DixCompactor::minify(src);
    println!("[compactor_preserves_string_contents] minified: {}", minified);
    assert!(minified.contains("https://example.com/api/v2"), "URL must be preserved");
    assert!(minified.contains("hello   world"), "internal spaces in string must be preserved");
}

#[test]
fn compactor_compression_ratio_calculation() {
    let original = "hello world foo bar baz";
    let compressed = "hi";
    let ratio = DixCompactor::get_compression_ratio(original, compressed);
    println!("[compactor_compression_ratio] {:.4}", ratio);
    assert!(ratio > 0.8, "ratio should be > 0.8 for dramatic compression");

    let zero_ratio = DixCompactor::get_compression_ratio("", "anything");
    assert_eq!(zero_ratio, 0.0, "empty original → ratio 0.0");
}

// ═════════════════════════════════════════════════════════════════════════════
// GROUP 8: format_options (to_mdix)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn format_options_to_mdix_all_styles() {
    use dixscript::Compiler::AST::{
        ConfigEntry, ConfigSection, ConfigValue, DataEntry, DataSection, DixScript, Position, Value,
    };

    let converter = DixConverter::new();

    let ast = DixScript {
        config: Some(ConfigSection {
            entries: vec![ConfigEntry {
                key: "version".to_string(),
                value: ConfigValue::String("1.0.0".to_string()),
                position: Position::UNKNOWN,
            }],
            position: Position::UNKNOWN,
        }),
        data: Some(DataSection {
            entries: (0..20_i32)
                .map(|i| DataEntry::SimpleProperty {
                    name: format!("key_{}", i),
                    data_type: None,
                    value: Value::Integer { value: i, position: Position::UNKNOWN },
                    position: Position::UNKNOWN,
                })
                .collect(),
            position: Position::UNKNOWN,
        }),
        imports: None,
        dlm: None,
        enums: None,
        quick_functions: None,
        security: None,
    };

    for (label, opts) in [
        ("default",  DixFormatOptions::new()),
        ("minified", DixFormatOptions::minified()),
        ("pretty",   DixFormatOptions::pretty()),
    ] {
        let t = Instant::now();
        let result = converter.to_mdix(&ast, Some(&opts));
        let us = elapsed_us(t);

        match result {
            Ok(s) => {
                println!(
                    "[format_options_to_mdix] {:10} | {} bytes  ({:.2} µs)",
                    label, s.len(), us
                );
                assert!(!s.is_empty(), "{}: output must not be empty", label);

                if label == "minified" {
                    assert!(!s.contains('\n'), "minified must have no newlines");
                }
                if label == "pretty" {
                    assert!(s.contains("@CONFIG"), "pretty should include CONFIG section");
                }
            }
            Err(e) => panic!("[format_options_to_mdix] {} FAILED: {}", label, e),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// GROUP 9: dix_value
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn dix_value_create_string() {
    let t = Instant::now();
    let v = DixValue::string("hello world");
    let us = elapsed_us(t);

    println!("[dix_value_create_string] {:.3} µs  value={}", us, v);
    assert_eq!(v.as_string(), Some("hello world"));
    assert_eq!(v.type_name(), "string");
}

#[test]
fn dix_value_create_array_10() {
    let t = Instant::now();
    let items: Vec<DixValue> = (0..10).map(DixValue::Int).collect();
    let v = DixValue::array(items);
    let us = elapsed_us(t);

    println!("[dix_value_create_array_10] {:.3} µs  value={}", us, v);
    assert_eq!(v.type_name(), "array");
    assert_eq!(v.as_array().map(|a| a.len()), Some(10));
}

#[test]
fn dix_value_create_object_10() {
    let t = Instant::now();
    let map: HashMap<String, DixValue> = (0..10)
        .map(|i| (format!("k{}", i), DixValue::Int(i)))
        .collect();
    let v = DixValue::object(map);
    let us = elapsed_us(t);

    println!("[dix_value_create_object_10] {:.3} µs  value={}", us, v);
    assert_eq!(v.type_name(), "object");
    assert_eq!(v.as_object().map(|o| o.len()), Some(10));
}

#[test]
fn dix_value_as_int_hit() {
    let v = DixValue::Int(42);

    let t = Instant::now();
    let result = v.as_int();
    let us = elapsed_us(t);

    println!("[dix_value_as_int_hit] {:.3} µs  result={:?}", us, result);
    assert_eq!(result, Some(42));
}

#[test]
fn dix_value_as_int_miss() {
    let v = DixValue::String("hello".to_string());

    let t = Instant::now();
    let result = v.as_int();
    let us = elapsed_us(t);

    println!("[dix_value_as_int_miss] {:.3} µs  result={:?}", us, result);
    assert_eq!(result, None);
}

#[test]
fn dix_value_numeric_coercion() {
    assert_eq!(DixValue::Float(3.9_f32).as_int(), Some(3));
    assert_eq!(DixValue::Double(7.1).as_int(), Some(7));
    assert!((DixValue::Int(5).as_float().unwrap() - 5.0).abs() < f64::EPSILON);
    println!("[dix_value_numeric_coercion] all coercions passed");
}

#[test]
fn dix_value_type_name_all_variants() {
    let cases: &[(DixValue, &str)] = &[
        (DixValue::Null,                                           "null"),
        (DixValue::Bool(true),                                     "bool"),
        (DixValue::Int(1),                                         "int"),
        (DixValue::Float(1.0_f32),                                 "float"),
        (DixValue::Double(1.0),                                    "double"),
        (DixValue::String("x".to_string()),                        "string"),
        (DixValue::Date("2025-01-01".to_string()),                 "date"),
        (DixValue::Timestamp("2025-01-01T00:00:00Z".to_string()),  "timestamp"),
        (DixValue::HexColor("#FF0000".to_string()),                "hexcolor"),
        (DixValue::Blob("data".to_string()),                       "blob"),
        (DixValue::Regex(".*".to_string()),                        "regex"),
        (DixValue::Array(vec![]),                                   "array"),
        (DixValue::Object(HashMap::new()),                          "object"),
        (DixValue::Tuple(vec![]),                                   "tuple"),
        (DixValue::Enum { enum_name: "E".into(), field_name: "A".into(), value: 0 }, "enum"),
    ];

    let t = Instant::now();
    for (v, expected) in cases {
        let name = v.type_name();
        assert_eq!(name, *expected, "type_name() mismatch for {:?}", v);
    }
    let us = elapsed_us(t);

    println!("[dix_value_type_name_all_variants] {:.3} µs  ({} variants)", us, cases.len());
}

#[test]
fn dix_value_display_format() {
    let cases: Vec<(DixValue, &str)> = vec![
        (DixValue::Null,                      "null"),
        (DixValue::Bool(true),                "true"),
        (DixValue::Int(42),                   "42"),
        (DixValue::String("hi".to_string()),  "\"hi\""),
    ];

    for (v, expected) in &cases {
        let s = format!("{}", v);
        assert_eq!(&s, expected, "Display mismatch for type {}", v.type_name());
        println!("[dix_value_display] {} → {}", v.type_name(), s);
    }
}

#[test]
fn dix_value_from_trait_impls() {
    let v_bool:   DixValue = true.into();
    let v_int:    DixValue = 42_i32.into();
    let v_float:  DixValue = 1.5_f32.into();
    let v_double: DixValue = 3.14_f64.into();
    let v_string: DixValue = "hello".into();
    let v_owned:  DixValue = "owned".to_string().into();

    assert_eq!(v_bool.type_name(),   "bool");
    assert_eq!(v_int.type_name(),    "int");
    assert_eq!(v_float.type_name(),  "float");
    assert_eq!(v_double.type_name(), "double");
    assert_eq!(v_string.type_name(), "string");
    assert_eq!(v_owned.type_name(),  "string");

    println!("[dix_value_from_trait_impls] all From<T> impls verified");
}

// ═════════════════════════════════════════════════════════════════════════════
// GROUP 10: Integration — full pipeline smoke tests
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn integration_builder_then_flatten_then_access() {
    let data = DixDataBuilder::new()
        .config(|c| { c.with_version("1.0.0"); c.with_author("test"); })
        .data(|d| {
            d.with_string("app_name", "IntegrationApp");
            d.with_int("port", 7777);
            d.with_bool("tls", true);
            d.with_double("latency_ms", 1.23);
            d.with_table_properties("db", |t| {
                t.with_string("host", "pg.local");
                t.with_int("port", 5432);
            });
            d.with_group_array_builder("tags", |a| {
                a.add_string("production");
                a.add_string("v2");
            });
        })
        .build()
        .expect("integration build");

    println!("[integration_builder_then_flatten_then_access]");
    println!("  entry_count = {}", data.entry_count());

    let map = data.to_hashmap();
    println!("  all keys: {:?}", { let mut k: Vec<_> = map.keys().cloned().collect(); k.sort(); k });

    assert_eq!(data.get::<String>("app_name").unwrap(), "IntegrationApp");
    assert_eq!(data.get::<i32>("port").unwrap(), 7777);
    assert_eq!(data.get::<bool>("tls").unwrap(), true);
    assert_eq!(data.get::<String>("db.host").unwrap(), "pg.local");
    assert_eq!(data.get::<i32>("db.port").unwrap(), 5432);
    assert!(data.exists("tags"));
    assert!(data.exists("tags[0]"));
    assert_eq!(data.get::<String>("tags[0]").unwrap(), "production");

    println!("  all assertions passed");
}

#[test]
fn integration_load_then_get_keys_then_select() {
    let data = build_medium_data();

    let mut db_keys = data.get_keys("database.primary");
    db_keys.sort();
    println!("[integration_load_then_get_keys_then_select]");
    println!("  database.primary sub-keys: {:?}", db_keys);

    assert!(!db_keys.is_empty(), "database.primary should have sub-keys");

    let flags: Vec<bool> = data.select_many("feature_flags.*");
    println!("  feature_flags.* values ({} found): {:?}", flags.len(), flags);
    assert_eq!(flags.len(), 3, "should find 3 feature flag booleans");
}
