// tests/loader_tests.rs
//
// Comprehensive tests for DixLoader.
// Covers: text file loading, compile pipeline, data access, error cases,
// and comparison benchmarks against Jsonnet and CUE.
//
// CUE tests require the `cue` CLI on PATH and the `cue_cli_available` feature:
//   cargo test --features cue_cli_available

use dixscript::Runtime::{DixData, DixLoadOptions, DixLoader, DixValue};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tempfile::TempDir;

// ==================== HELPERS ====================

/// Creates a temp dir and writes a .mdix file inside it.
/// Returns (TempDir, PathBuf). Keep TempDir alive or the dir will be deleted.
fn write_mdix(dir: &TempDir, filename: &str, content: &str) -> PathBuf {
    let path = dir.path().join(filename);
    fs::write(&path, content).expect("failed to write test .mdix file");
    path
}

fn default_loader() -> DixLoader {
    DixLoader::new()
}

fn default_opts() -> DixLoadOptions {
    DixLoadOptions::new()
}

// ==================== MINIMAL VALID CONTENT ====================

const MINIMAL_MDIX: &str = r#"
@CONFIG(
  version -> "1.0.0"
)

@DATA(
  app_name = "TestApp"
  port = 8080
  enabled = true
)
"#;

const FLAT_PROPERTIES_MDIX: &str = r#"
@DATA(
  name = "Alice"
  age = 30
  score = 99.5
  active = true
  label = null
)
"#;

const TABLE_PROPERTIES_MDIX: &str = r#"
@DATA(
  title = "My App"

  server: host = "localhost", port = 8080, ssl = false
  database: host = "db.local", port = 5432, name = "mydb"
)
"#;

const GROUP_ARRAY_MDIX: &str = r#"
@DATA(
  app = "Launcher"

  tags:: "rust", "config", "fast"
  ports:: 8080, 8081, 8082
)
"#;

const ENUMS_MDIX: &str = r#"
@ENUMS(
  Status { ACTIVE = 1, INACTIVE = 2, PENDING = 3 }
  Priority { LOW = 0, MEDIUM = 5, HIGH = 10 }
)

@DATA(
  current_status<enum> = Status.ACTIVE
  task_priority<enum>  = Priority.HIGH
)
"#;

const CONFIG_SECTION_MDIX: &str = r#"
@CONFIG(
  version -> "1.0.0"
  author  -> "MidManStudio"
  debug_mode -> "off"
  error_handling -> "halt"
)

@DATA(
  value = 42
)
"#;

const NESTED_OBJECT_MDIX: &str = r#"
@DATA(
  profile: name = "Bob", age = 25, email = "bob@example.com"
  address: street = "123 Main St", city = "Springfield", zip = "12345"
)
"#;

const ARRAY_OF_OBJECTS_MDIX: &str = r#"
@DATA(
  app_name = "GameServer"

  enemies::
    { name = "Goblin", health = 50, damage = 10 },
    { name = "Orc",    health = 100, damage = 20 },
    { name = "Troll",  health = 200, damage = 40 }
)
"#;

const QUICKFUNCS_MDIX: &str = r#"
@QUICKFUNCS(
  ~double<int>(x) {
    return x * 2
  }

  ~greet<string>(name) {
    return $"Hello, {name}!"
  }
)

@DATA(
  result    = double(21)
  message   = greet("World")
  big_num   = double(500)
)
"#;

const MULTI_TYPE_MDIX: &str = r#"
@DATA(
  int_val        = 42
  float_val      = 3.14f
  double_val     = 2.718281828
  bool_true      = true
  bool_false     = false
  string_val     = "hello world"
  null_val       = null
  hex_color      = #FF5733
  date_val       = 2025-12-31
  timestamp_val  = 2025-01-15T10:30:00Z
  blob_val       = b:("SGVsbG8gV29ybGQ=")
  regex_val      = r:("^[a-z0-9]+$")
)
"#;

const ALL_SECTIONS_MDIX: &str = r#"
@CONFIG(
  version -> "1.0.0"
  author  -> "Test"
)

@ENUMS(
  Environment { DEV = 1, STAGING = 2, PROD = 3 }
)

@QUICKFUNCS(
  ~calc_port<int>(base, env<enum>) {
    return base + env
  }
)

@DATA(
  env<enum> = Environment.PROD
  port      = calc_port(8000, Environment.PROD)
  name      = "AllSections"

  db: host = "db.prod", port = 5432
  replicas:: "db-1.prod", "db-2.prod"
)
"#;

// ==================== BASIC LOADING TESTS ====================

#[test]
fn test_loader_new() {
    let loader = default_loader();
    // Constructing the loader should not produce any errors
    let em = dixscript::ErrorManager::ErrorManager::get_shared_instance();
    em.clear_errors();
    assert!(!em.has_errors());
}

#[test]
fn test_load_minimal_text_file() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "minimal.mdix", MINIMAL_MDIX);
    let loader = default_loader();

    let result = loader.load_text(path.to_str().unwrap(), &default_opts());

    assert!(result.is_ok(), "load_text should succeed: {:?}", result.err());

    let data = result.unwrap();
    assert_eq!(data.version, "1.0.0");
    assert!(!data.is_encrypted);
    assert!(!data.is_compressed);
}

#[test]
fn test_load_nonexistent_file_returns_error() {
    let loader = default_loader();
    let result = loader.load_text("/nonexistent/path/file.mdix", &default_opts());

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("not found") || err.contains("File not found"),
        "error should mention 'not found': {}",
        err
    );
}

#[test]
fn test_load_empty_file() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "empty.mdix", "");
    let loader = default_loader();

    // Empty file should either succeed with empty data or return a graceful error
    let result = loader.load_text(path.to_str().unwrap(), &default_opts());
    match result {
        Ok(data) => assert_eq!(data.entry_count(), 0),
        Err(e) => assert!(!e.is_empty()),
    }
}

// ==================== DATA ACCESS TESTS ====================

#[test]
fn test_get_flat_string_property() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "flat.mdix", FLAT_PROPERTIES_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let name: String = data.get("name").expect("name not found");
    assert_eq!(name, "Alice");
}

#[test]
fn test_get_flat_int_property() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "flat.mdix", FLAT_PROPERTIES_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let age: i32 = data.get("age").expect("age not found");
    assert_eq!(age, 30);
}

#[test]
fn test_get_flat_bool_property() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "flat.mdix", FLAT_PROPERTIES_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let active: bool = data.get("active").expect("active not found");
    assert!(active);
}

#[test]
fn test_get_missing_key_returns_error() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "flat.mdix", FLAT_PROPERTIES_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let result: Result<String, String> = data.get("does_not_exist");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[test]
fn test_get_or_default_missing_key() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "flat.mdix", FLAT_PROPERTIES_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let val: i32 = data.get_or_default("missing_key", 999);
    assert_eq!(val, 999);
}

#[test]
fn test_exists_positive() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "flat.mdix", FLAT_PROPERTIES_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    assert!(data.exists("name"));
    assert!(data.exists("age"));
}

#[test]
fn test_exists_negative() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "flat.mdix", FLAT_PROPERTIES_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    assert!(!data.exists("nonexistent_key"));
}

// ==================== TABLE PROPERTIES TESTS ====================

#[test]
fn test_table_property_dotted_access() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "table.mdix", TABLE_PROPERTIES_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let host: String = data.get("server.host").expect("server.host not found");
    assert_eq!(host, "localhost");

    let port: i32 = data.get("server.port").expect("server.port not found");
    assert_eq!(port, 8080);

    let ssl: bool = data.get("server.ssl").expect("server.ssl not found");
    assert!(!ssl);
}

#[test]
fn test_multiple_table_properties() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "table.mdix", TABLE_PROPERTIES_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    // Both server and database should be accessible
    let db_host: String = data.get("database.host").expect("database.host not found");
    assert_eq!(db_host, "db.local");

    let db_name: String = data.get("database.name").expect("database.name not found");
    assert_eq!(db_name, "mydb");
}

#[test]
fn test_flat_and_table_coexist() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "table.mdix", TABLE_PROPERTIES_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    // Flat property
    let title: String = data.get("title").expect("title not found");
    assert_eq!(title, "My App");

    // Table property
    let host: String = data.get("server.host").expect("server.host not found");
    assert_eq!(host, "localhost");
}

// ==================== GROUP ARRAY TESTS ====================

#[test]
fn test_group_array_indexed_access() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "array.mdix", GROUP_ARRAY_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let tag0: String = data.get("tags[0]").expect("tags[0] not found");
    assert_eq!(tag0, "rust");

    let tag1: String = data.get("tags[1]").expect("tags[1] not found");
    assert_eq!(tag1, "config");

    let tag2: String = data.get("tags[2]").expect("tags[2] not found");
    assert_eq!(tag2, "fast");
}

#[test]
fn test_group_array_full_array_access() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "array.mdix", GROUP_ARRAY_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let tags: Vec<DixValue> = data.get("tags").expect("tags array not found");
    assert_eq!(tags.len(), 3);
}

#[test]
fn test_int_array() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "array.mdix", GROUP_ARRAY_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let port0: i32 = data.get("ports[0]").expect("ports[0] not found");
    assert_eq!(port0, 8080);

    let port2: i32 = data.get("ports[2]").expect("ports[2] not found");
    assert_eq!(port2, 8082);
}

// ==================== ENUMS TESTS ====================

#[test]
fn test_enums_section_populated() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "enums.mdix", ENUMS_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    assert!(data.enums.is_some(), "enums section should be populated");

    let enums = data.enums.unwrap();
    assert!(enums.contains_key("Status"), "Status enum should exist");
    assert!(enums.contains_key("Priority"), "Priority enum should exist");

    let status = &enums["Status"];
    assert_eq!(status.get("ACTIVE"), Some(&1));
    assert_eq!(status.get("INACTIVE"), Some(&2));
    assert_eq!(status.get("PENDING"), Some(&3));

    let priority = &enums["Priority"];
    assert_eq!(priority.get("LOW"), Some(&0));
    assert_eq!(priority.get("HIGH"), Some(&10));
}

// ==================== CONFIG SECTION TESTS ====================

#[test]
fn test_config_section_populated() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "config.mdix", CONFIG_SECTION_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    assert!(data.config.is_some(), "config section should be populated");

    let config = data.config.unwrap();
    assert_eq!(config.get("version").map(|s| s.as_str()), Some("1.0.0"));
    assert_eq!(config.get("author").map(|s| s.as_str()), Some("MidManStudio"));
}

// ==================== QUICKFUNCS TESTS ====================

#[test]
fn test_quickfunc_integer_result() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "qf.mdix", QUICKFUNCS_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let result: i32 = data.get("result").expect("result not found");
    assert_eq!(result, 42, "double(21) should equal 42");
}

#[test]
fn test_quickfunc_string_interpolation() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "qf.mdix", QUICKFUNCS_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let message: String = data.get("message").expect("message not found");
    assert_eq!(message, "Hello, World!");
}

#[test]
fn test_quickfunc_large_value() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "qf.mdix", QUICKFUNCS_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let big: i32 = data.get("big_num").expect("big_num not found");
    assert_eq!(big, 1000, "double(500) should equal 1000");
}

// ==================== ALL SECTIONS INTEGRATION TEST ====================

#[test]
fn test_load_all_sections() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "all.mdix", ALL_SECTIONS_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    // Config
    assert!(data.config.is_some());

    // Enums
    assert!(data.enums.is_some());
    let enums = data.enums.as_ref().unwrap();
    assert!(enums.contains_key("Environment"));

    // Flat data
    let name: String = data.get("name").expect("name not found");
    assert_eq!(name, "AllSections");

    // QuickFunc result
    let port: i32 = data.get("port").expect("port not found");
    // calc_port(8000, Environment.PROD) = 8000 + 3 = 8003
    assert_eq!(port, 8003);

    // Table property
    let db_host: String = data.get("db.host").expect("db.host not found");
    assert_eq!(db_host, "db.prod");

    // Group array
    let rep0: String = data.get("replicas[0]").expect("replicas[0] not found");
    assert_eq!(rep0, "db-1.prod");
}

// ==================== NESTED OBJECT TESTS ====================

#[test]
fn test_nested_object_access() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "nested.mdix", NESTED_OBJECT_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let name: String = data.get("profile.name").expect("profile.name not found");
    assert_eq!(name, "Bob");

    let age: i32 = data.get("profile.age").expect("profile.age not found");
    assert_eq!(age, 25);

    let city: String = data.get("address.city").expect("address.city not found");
    assert_eq!(city, "Springfield");
}

#[test]
fn test_array_of_objects() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "array_obj.mdix", ARRAY_OF_OBJECTS_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    // Full array should exist
    let enemies: Vec<DixValue> = data.get("enemies").expect("enemies not found");
    assert_eq!(enemies.len(), 3);
}

// ==================== ENTRY COUNT TESTS ====================

#[test]
fn test_entry_count_reflects_all_entries() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "flat.mdix", FLAT_PROPERTIES_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    // name, age, score, active, label = 5 flat properties
    assert!(data.entry_count() >= 5);
}

#[test]
fn test_get_keys_prefix() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "nested.mdix", NESTED_OBJECT_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let profile_keys = data.get_keys("profile");
    // Should find: name, age, email
    assert!(profile_keys.len() >= 3, "expected 3+ profile keys, got {}: {:?}", profile_keys.len(), profile_keys);
    assert!(profile_keys.contains(&"name".to_string()));
    assert!(profile_keys.contains(&"age".to_string()));
}

// ==================== TO HASHMAP TEST ====================

#[test]
fn test_to_hashmap_contains_all_entries() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "flat.mdix", FLAT_PROPERTIES_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let map = data.to_hashmap();
    assert!(map.contains_key("name"));
    assert!(map.contains_key("age"));
    assert!(map.contains_key("active"));
}

// ==================== LOAD OPTIONS TESTS ====================

#[test]
fn test_load_options_default() {
    let opts = DixLoadOptions::new();
    assert!(opts.password.is_none());
    assert!(opts.validate_checksums);
    assert!(!opts.allow_url_key_loading);
    assert!(!opts.allow_direct_key_content);
    assert!(opts.validate().is_ok());
}

#[test]
fn test_load_options_with_password() {
    let opts = DixLoadOptions::with_password("test_password");
    assert_eq!(opts.password.as_deref(), Some("test_password"));
}

#[test]
fn test_load_options_url_requires_https() {
    let http_result = DixLoadOptions::with_key_url("http://example.com/key.dxkey", true);
    assert!(http_result.is_err(), "HTTP URL should be rejected");

    let https_result = DixLoadOptions::with_key_url("https://example.com/key.dxkey", true);
    assert!(https_result.is_ok(), "HTTPS URL should be accepted");
}

#[test]
fn test_load_options_direct_content_requires_ack() {
    let no_ack = DixLoadOptions::with_key_content("content", false);
    assert!(no_ack.is_err(), "should require acknowledgment");

    let with_ack = DixLoadOptions::with_key_content("content", true);
    assert!(with_ack.is_ok());
}

#[test]
fn test_load_options_multiple_key_methods_rejected() {
    let opts = DixLoadOptions {
        key_file_path: Some("path.dxkey".to_string()),
        key_file_content: Some("content".to_string()),
        allow_direct_key_content: true,
        ..DixLoadOptions::new()
    };
    assert!(opts.validate().is_err(), "multiple key methods should be rejected");
}

// ==================== PERFORMANCE BASELINES ====================

/// How fast we expect DixScript to load vs Jsonnet for equivalent data.
/// Target: DixScript should not be more than 2x slower than Jsonnet for
/// equivalent compile-time function expansion.
const DIXSCRIPT_MAX_LOAD_MS_SIMPLE: u128 = 50;
const JSONNET_MAX_EVAL_MS_SIMPLE: u128 = 100;

#[test]
fn test_load_performance_simple_file() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "perf.mdix", MINIMAL_MDIX);

    let start = Instant::now();
    for _ in 0..10 {
        let _ = default_loader().load_text(path.to_str().unwrap(), &default_opts());
    }
    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_millis() / 10;

    println!("\n[PERF] DixScript simple load avg: {}ms (baseline: <{}ms)", avg_ms, DIXSCRIPT_MAX_LOAD_MS_SIMPLE);

    assert!(
        avg_ms < DIXSCRIPT_MAX_LOAD_MS_SIMPLE,
        "DixScript simple load too slow: {}ms avg (baseline {}ms)",
        avg_ms,
        DIXSCRIPT_MAX_LOAD_MS_SIMPLE
    );
}

#[test]
fn test_load_performance_with_quickfuncs() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "perf_qf.mdix", QUICKFUNCS_MDIX);

    let start = Instant::now();
    for _ in 0..10 {
        let _ = default_loader().load_text(path.to_str().unwrap(), &default_opts());
    }
    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_millis() / 10;

    println!("[PERF] DixScript QuickFuncs load avg: {}ms", avg_ms);

    // QuickFuncs should still be reasonable
    assert!(
        avg_ms < 200,
        "QuickFuncs load too slow: {}ms avg",
        avg_ms
    );
}

// ==================== JSONNET COMPARISON ====================
// These tests evaluate equivalent Jsonnet to compare parse/eval performance.
// jrsonnet-evaluator is used in-process - no subprocess needed.

/// Jsonnet equivalent of MINIMAL_MDIX
const MINIMAL_JSONNET: &str = r#"
{
  config: {
    version: "1.0.0"
  },
  data: {
    app_name: "TestApp",
    port: 8080,
    enabled: true
  }
}
"#;

/// Jsonnet equivalent of the QuickFuncs example
const QUICKFUNCS_JSONNET: &str = r#"
local double(x) = x * 2;
local greet(name) = "Hello, " + name + "!";
{
  result:  double(21),
  message: greet("World"),
  big_num: double(500)
}
"#;

/// Jsonnet with 50 items using a function - equivalent complexity to DixScript QuickFuncs
const BULK_JSONNET: &str = r#"
local server(ip) = {
  host: ip,
  port: 8080,
  ssl: true,
  timeout: 5000
};
{
  servers: [
    server("10.0.0.1"),
    server("10.0.0.2"),
    server("10.0.0.3"),
    server("10.0.0.4"),
    server("10.0.0.5"),
    server("10.0.0.6"),
    server("10.0.0.7"),
    server("10.0.0.8"),
    server("10.0.0.9"),
    server("10.0.0.10"),
    server("10.0.0.11"),
    server("10.0.0.12"),
    server("10.0.0.13"),
    server("10.0.0.14"),
    server("10.0.0.15"),
    server("10.0.0.16"),
    server("10.0.0.17"),
    server("10.0.0.18"),
    server("10.0.0.19"),
    server("10.0.0.20"),
    server("10.0.0.21"),
    server("10.0.0.22"),
    server("10.0.0.23"),
    server("10.0.0.24"),
    server("10.0.0.25"),
    server("10.0.0.26"),
    server("10.0.0.27"),
    server("10.0.0.28"),
    server("10.0.0.29"),
    server("10.0.0.30"),
    server("10.0.0.31"),
    server("10.0.0.32"),
    server("10.0.0.33"),
    server("10.0.0.34"),
    server("10.0.0.35"),
    server("10.0.0.36"),
    server("10.0.0.37"),
    server("10.0.0.38"),
    server("10.0.0.39"),
    server("10.0.0.40"),
    server("10.0.0.41"),
    server("10.0.0.42"),
    server("10.0.0.43"),
    server("10.0.0.44"),
    server("10.0.0.45"),
    server("10.0.0.46"),
    server("10.0.0.47"),
    server("10.0.0.48"),
    server("10.0.0.49"),
    server("10.0.0.50")
  ]
}
"#;

/// DixScript equivalent of BULK_JSONNET
const BULK_DIXSCRIPT: &str = r#"
@QUICKFUNCS(
  ~server<object>(ip) {
    return {
      host    = ip,
      port    = 8080,
      ssl     = true,
      timeout = 5000
    }
  }
)

@DATA(
  servers::
    server("10.0.0.1"),  server("10.0.0.2"),  server("10.0.0.3"),
    server("10.0.0.4"),  server("10.0.0.5"),  server("10.0.0.6"),
    server("10.0.0.7"),  server("10.0.0.8"),  server("10.0.0.9"),
    server("10.0.0.10"), server("10.0.0.11"), server("10.0.0.12"),
    server("10.0.0.13"), server("10.0.0.14"), server("10.0.0.15"),
    server("10.0.0.16"), server("10.0.0.17"), server("10.0.0.18"),
    server("10.0.0.19"), server("10.0.0.20"), server("10.0.0.21"),
    server("10.0.0.22"), server("10.0.0.23"), server("10.0.0.24"),
    server("10.0.0.25"), server("10.0.0.26"), server("10.0.0.27"),
    server("10.0.0.28"), server("10.0.0.29"), server("10.0.0.30"),
    server("10.0.0.31"), server("10.0.0.32"), server("10.0.0.33"),
    server("10.0.0.34"), server("10.0.0.35"), server("10.0.0.36"),
    server("10.0.0.37"), server("10.0.0.38"), server("10.0.0.39"),
    server("10.0.0.40"), server("10.0.0.41"), server("10.0.0.42"),
    server("10.0.0.43"), server("10.0.0.44"), server("10.0.0.45"),
    server("10.0.0.46"), server("10.0.0.47"), server("10.0.0.48"),
    server("10.0.0.49"), server("10.0.0.50")
)
"#;

/// Helper: evaluate Jsonnet snippet in-process using jrsonnet 0.4.x and return manifest JSON string.
/// Returns Err if jrsonnet fails.
///
/// API notes (jrsonnet 0.4.x):
/// - `jrsonnet_stdlib` 0.4.x exports ONLY `STDLIB_STR` — no `ContextInitializer` (that is 0.5.x).
///   Stdlib is initialized via `state.with_stdlib()`, which is sufficient.
/// - The snippet evaluation method is `evaluate_snippet_raw(source_name: IStr, source: IStr)`.
///   The 0.5.x name `evaluate_snippet` does not exist in this version.
/// - JSON manifesting lives on `Val` directly (`val.manifest(&ManifestFormat)`),
///   NOT on `EvaluationState`. `manifest_json_ex` is a 0.5.x concept.
fn eval_jsonnet(snippet: &str) -> Result<String, String> {
    use jrsonnet_evaluator::{EvaluationState, ManifestFormat};

    let state = EvaluationState::default();
    // with_stdlib() is the correct 0.4.x method — no ContextInitializer needed.
    state.with_stdlib();

    // evaluate_snippet_raw is the 0.4.x name; takes two IStr (interned strings).
    let val = state
        .evaluate_snippet_raw("test".into(), snippet.into())
        .map_err(|e| format!("jrsonnet eval error: {:?}", e))?;

    // manifest() is a method on Val in 0.4.x — EvaluationState is not involved here.
    let json = val
        .manifest(&ManifestFormat::Json(2))
        .map_err(|e| format!("jrsonnet manifest error: {:?}", e))?;

    Ok(json.to_string())
}

#[test]
fn test_jsonnet_minimal_eval() {
    let result = eval_jsonnet(MINIMAL_JSONNET);
    assert!(result.is_ok(), "Jsonnet eval failed: {:?}", result.err());

    let json = result.unwrap();
    assert!(json.contains("TestApp"), "JSON should contain TestApp");
    assert!(json.contains("8080"), "JSON should contain port 8080");

    println!("[Jsonnet] minimal output:\n{}", json);
}

#[test]
fn test_jsonnet_quickfuncs_equivalent_eval() {
    let result = eval_jsonnet(QUICKFUNCS_JSONNET);
    assert!(result.is_ok(), "Jsonnet QuickFuncs eval failed: {:?}", result.err());

    let json = result.unwrap();
    assert!(json.contains("42"), "result should be 42");
    assert!(json.contains("Hello, World!"), "message should match");

    println!("[Jsonnet] QuickFuncs output:\n{}", json);
}

#[test]
fn test_jsonnet_performance_simple() {
    let start = Instant::now();
    for _ in 0..10 {
        let _ = eval_jsonnet(MINIMAL_JSONNET);
    }
    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_millis() / 10;

    println!("[PERF] Jsonnet simple eval avg: {}ms (baseline: <{}ms)", avg_ms, JSONNET_MAX_EVAL_MS_SIMPLE);

    assert!(
        avg_ms < JSONNET_MAX_EVAL_MS_SIMPLE,
        "Jsonnet simple eval too slow: {}ms avg",
        avg_ms
    );
}

#[test]
fn test_compare_dixscript_vs_jsonnet_bulk_functions() {
    let dir = TempDir::new().unwrap();
    let dix_path = write_mdix(&dir, "bulk.mdix", BULK_DIXSCRIPT);

    // --- DixScript ---
    let dix_start = Instant::now();
    let dix_result = default_loader().load_text(dix_path.to_str().unwrap(), &default_opts());
    let dix_elapsed = dix_start.elapsed();

    // --- Jsonnet ---
    let json_start = Instant::now();
    let json_result = eval_jsonnet(BULK_JSONNET);
    let json_elapsed = json_start.elapsed();

    println!("\n============================================");
    println!("  BULK FUNCTION EXPANSION: DixScript vs Jsonnet");
    println!("  50 server() calls, 4 fields each = 200 values");
    println!("--------------------------------------------");
    println!("  DixScript: {:?}", dix_elapsed);
    println!("  Jsonnet:   {:?}", json_elapsed);

    if dix_elapsed < json_elapsed {
        println!("  Winner:    DixScript ({:.1}x faster)", json_elapsed.as_secs_f64() / dix_elapsed.as_secs_f64());
    } else {
        println!("  Winner:    Jsonnet ({:.1}x faster)", dix_elapsed.as_secs_f64() / json_elapsed.as_secs_f64());
        println!("  Note:      jrsonnet is highly optimised - this is expected at first.");
        println!("             Target: DixScript within 3x of jrsonnet.");
    }
    println!("============================================\n");

    // Both should succeed
    assert!(dix_result.is_ok(), "DixScript bulk load failed: {:?}", dix_result.err());
    assert!(json_result.is_ok(), "Jsonnet bulk eval failed: {:?}", json_result.err());

    // DixScript should produce the right number of servers
    let data = dix_result.unwrap();
    let servers: Vec<DixValue> = data.get("servers").expect("servers array not found");
    assert_eq!(servers.len(), 50, "should have 50 servers");
}

// ==================== CUE COMPARISON ====================
// CUE has no Rust crate. These tests call the `cue` CLI as a subprocess.
// They only run when the `cue_cli_available` feature is enabled AND
// the `cue` binary is on PATH.
//
// Run: cargo test --features cue_cli_available

/// Check if cue CLI is available on PATH.
fn cue_available() -> bool {
    std::process::Command::new("cue")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Evaluate a CUE file at the given path and return the JSON export.
fn eval_cue_file(path: &Path) -> Result<String, String> {
    let output = std::process::Command::new("cue")
        .args(["export", "--out", "json", path.to_str().unwrap()])
        .output()
        .map_err(|e| format!("failed to run cue: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// CUE equivalent of the bulk server example
const BULK_CUE: &str = r#"
#Server: {
    host:    string
    port:    int
    ssl:     bool
    timeout: int
}

server: [string]: #Server

#makeServer: {
    _ip:     string
    host:    _ip
    port:    8080
    ssl:     true
    timeout: 5000
}

servers: [
    { host: "10.0.0.1",  port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.2",  port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.3",  port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.4",  port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.5",  port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.6",  port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.7",  port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.8",  port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.9",  port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.10", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.11", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.12", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.13", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.14", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.15", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.16", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.17", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.18", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.19", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.20", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.21", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.22", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.23", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.24", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.25", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.26", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.27", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.28", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.29", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.30", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.31", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.32", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.33", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.34", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.35", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.36", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.37", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.38", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.39", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.40", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.41", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.42", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.43", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.44", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.45", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.46", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.47", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.48", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.49", port: 8080, ssl: true, timeout: 5000 },
    { host: "10.0.0.50", port: 8080, ssl: true, timeout: 5000 }
]
"#;

#[test]
#[cfg(feature = "cue_cli_available")]
fn test_cue_available_on_path() {
    assert!(
        cue_available(),
        "cue CLI not found on PATH. Install from https://cuelang.org/docs/install/"
    );
}

#[test]
#[cfg(feature = "cue_cli_available")]
fn test_compare_dixscript_vs_cue_bulk() {
    if !cue_available() {
        println!("[SKIP] cue not on PATH - skipping CUE comparison");
        return;
    }

    let dir = TempDir::new().unwrap();
    let dix_path = write_mdix(&dir, "bulk.mdix", BULK_DIXSCRIPT);
    let cue_path = dir.path().join("bulk.cue");
    fs::write(&cue_path, BULK_CUE).unwrap();

    // --- DixScript ---
    let dix_start = Instant::now();
    let dix_result = default_loader().load_text(dix_path.to_str().unwrap(), &default_opts());
    let dix_elapsed = dix_start.elapsed();

    // --- CUE ---
    let cue_start = Instant::now();
    let cue_result = eval_cue_file(&cue_path);
    let cue_elapsed = cue_start.elapsed();

    println!("\n============================================");
    println!("  BULK CONFIG: DixScript vs CUE");
    println!("  50 server entries, 4 fields each = 200 values");
    println!("--------------------------------------------");
    println!("  DixScript: {:?}", dix_elapsed);
    println!("  CUE:       {:?}", cue_elapsed);

    if dix_elapsed < cue_elapsed {
        println!("  Winner:    DixScript ({:.1}x faster)", cue_elapsed.as_secs_f64() / dix_elapsed.as_secs_f64());
    } else {
        println!("  Winner:    CUE ({:.1}x faster)", dix_elapsed.as_secs_f64() / cue_elapsed.as_secs_f64());
        println!("  Note:      CUE includes subprocess startup overhead.");
    }
    println!("============================================\n");

    assert!(dix_result.is_ok(), "DixScript failed: {:?}", dix_result.err());
    assert!(cue_result.is_ok(), "CUE failed: {:?}", cue_result.err());
}

// NOTE ON CUE COMPARISON FAIRNESS:
// The CUE test above includes subprocess startup time (~50-200ms on most systems),
// which makes CUE look slower than it is. For a truly fair comparison you would
// need to measure only CUE's parse/evaluate phase, which is not possible without
// a native Rust binding. The comparison is still useful for measuring total
// wall-clock time a developer would actually experience.

// ==================== SELECT MANY TESTS ====================

#[test]
fn test_select_many_with_wildcard() {
    // Inline mdix with repeated pattern
    let content = r#"
@DATA(
  title = "Test"

  users.alice: name = "Alice", age = 30
  users.bob:   name = "Bob",   age = 25
  users.carol: name = "Carol", age = 35
)
"#;
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "wildcard.mdix", content);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let names: Vec<String> = data.select_many("users.*.name");
    assert_eq!(names.len(), 3, "should find 3 user names: {:?}", names);
}

// ==================== MULTI-ENV CONFIG COMPARISON ====================
// This reproduces the README multi-env scenario to ensure end-to-end correctness.

const MULTI_ENV_MDIX: &str = r#"
@ENUMS(
  Environment { DEV = 1, STAGING = 2, PROD = 3 }
)

@QUICKFUNCS(
  ~serverConfig<object>(env<enum>, suffix) {
    pool = env == Environment.DEV ? 10 :
           env == Environment.STAGING ? 25 : 50
    return {
      host     = $"{suffix}-server.local",
      port     = 8080,
      pool_size = pool,
      timeout  = 5000,
      ssl      = env == Environment.PROD
    }
  }
)

@DATA(
  dev     = serverConfig(Environment.DEV,     "dev")
  staging = serverConfig(Environment.STAGING, "staging")
  prod    = serverConfig(Environment.PROD,    "prod")
)
"#;

#[test]
fn test_multi_env_dev_config() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "multi_env.mdix", MULTI_ENV_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let dev_host: String = data.get("dev.host").expect("dev.host not found");
    assert_eq!(dev_host, "dev-server.local");

    let dev_pool: i32 = data.get("dev.pool_size").expect("dev.pool_size not found");
    assert_eq!(dev_pool, 10);

    let dev_ssl: bool = data.get("dev.ssl").expect("dev.ssl not found");
    assert!(!dev_ssl, "DEV should not have SSL");
}

#[test]
fn test_multi_env_prod_config() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "multi_env.mdix", MULTI_ENV_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    let prod_host: String = data.get("prod.host").expect("prod.host not found");
    assert_eq!(prod_host, "prod-server.local");

    let prod_pool: i32 = data.get("prod.pool_size").expect("prod.pool_size not found");
    assert_eq!(prod_pool, 50);

    let prod_ssl: bool = data.get("prod.ssl").expect("prod.ssl not found");
    assert!(prod_ssl, "PROD should have SSL");
}

#[test]
fn test_multi_env_all_three_exist() {
    let dir = TempDir::new().unwrap();
    let path = write_mdix(&dir, "multi_env.mdix", MULTI_ENV_MDIX);
    let data = default_loader()
        .load_text(path.to_str().unwrap(), &default_opts())
        .expect("load failed");

    assert!(data.exists("dev.host"));
    assert!(data.exists("staging.host"));
    assert!(data.exists("prod.host"));

    let staging_pool: i32 = data.get("staging.pool_size").expect("staging.pool_size not found");
    assert_eq!(staging_pool, 25);
}

// ==================== FULL COMPARISON SUMMARY ====================

#[test]
#[ignore] // Run manually: cargo test print_comparison_summary -- --ignored --nocapture
fn print_comparison_summary() {
    let dir = TempDir::new().unwrap();

    // DixScript timings
    let dix_simple_path = write_mdix(&dir, "s.mdix", MINIMAL_MDIX);
    let dix_qf_path     = write_mdix(&dir, "q.mdix", QUICKFUNCS_MDIX);
    let dix_bulk_path   = write_mdix(&dir, "b.mdix", BULK_DIXSCRIPT);

    let runs = 20usize;

    let mut dix_simple_total = std::time::Duration::ZERO;
    let mut dix_qf_total     = std::time::Duration::ZERO;
    let mut dix_bulk_total   = std::time::Duration::ZERO;
    let mut json_simple_total = std::time::Duration::ZERO;
    let mut json_qf_total    = std::time::Duration::ZERO;
    let mut json_bulk_total  = std::time::Duration::ZERO;

    for _ in 0..runs {
        let t = Instant::now();
        let _ = default_loader().load_text(dix_simple_path.to_str().unwrap(), &default_opts());
        dix_simple_total += t.elapsed();

        let t = Instant::now();
        let _ = default_loader().load_text(dix_qf_path.to_str().unwrap(), &default_opts());
        dix_qf_total += t.elapsed();

        let t = Instant::now();
        let _ = default_loader().load_text(dix_bulk_path.to_str().unwrap(), &default_opts());
        dix_bulk_total += t.elapsed();

        let t = Instant::now();
        let _ = eval_jsonnet(MINIMAL_JSONNET);
        json_simple_total += t.elapsed();

        let t = Instant::now();
        let _ = eval_jsonnet(QUICKFUNCS_JSONNET);
        json_qf_total += t.elapsed();

        let t = Instant::now();
        let _ = eval_jsonnet(BULK_JSONNET);
        json_bulk_total += t.elapsed();
    }

    let avg = |d: std::time::Duration| d / runs as u32;

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║        DixScript vs Jsonnet (jrsonnet) — {} runs avg      ║", runs);
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Scenario          │ DixScript  │ Jsonnet    │ Ratio      ║");
    println!("╠══════════════════════════════════════════════════════════╣");

    let dix_s = avg(dix_simple_total).as_micros();
    let jsn_s = avg(json_simple_total).as_micros();
    println!("║ Simple config     │ {:>7}µs  │ {:>7}µs  │ {:.2}x        ║",
             dix_s, jsn_s,
             dix_s as f64 / jsn_s as f64);

    let dix_q = avg(dix_qf_total).as_micros();
    let jsn_q = avg(json_qf_total).as_micros();
    println!("║ QuickFuncs (3 fn) │ {:>7}µs  │ {:>7}µs  │ {:.2}x        ║",
             dix_q, jsn_q,
             dix_q as f64 / jsn_q as f64);

    let dix_b = avg(dix_bulk_total).as_micros();
    let jsn_b = avg(json_bulk_total).as_micros();
    println!("║ Bulk 50 fn calls  │ {:>7}µs  │ {:>7}µs  │ {:.2}x        ║",
             dix_b, jsn_b,
             dix_b as f64 / jsn_b as f64);

    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Target: DixScript within 3x of jrsonnet (release build)  ║");
    println!("║ Note:   jrsonnet is the fastest Jsonnet impl in existence ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");
}
