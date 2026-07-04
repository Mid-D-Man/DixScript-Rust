//! mdix-wasm/tests/web.rs
//!
//! Runs in a real headless browser via `wasm-pack test --headless
//! --chrome` (see .github/workflows/wasm-tests.yml) — not just a compile
//! check. wasm_bindgen_test_configure!(run_in_browser) below is what makes
//! that distinction matter: without it these would try to run under
//! Node.js instead, where `web_sys::window()` (used by the localStorage
//! cache backend) would be `None`.

use wasm_bindgen_test::*;
use mdix_wasm::MdixDatabase;

wasm_bindgen_test_configure!(run_in_browser);

const SAMPLE: &str = r#"
@DATA(
  app_name -> "DixScript WASM Test",
  version  -> 1,
  ready    -> true
)
"#;

#[wasm_bindgen_test]
fn load_str_parses_valid_source() {
    let db = MdixDatabase::load_str(SAMPLE).expect("load_str should succeed on valid source");
    assert!(db.is_valid());
}

#[wasm_bindgen_test]
fn load_str_rejects_empty_source() {
    let result = MdixDatabase::load_str("");
    assert!(result.is_err(), "empty source should be rejected, not silently accepted");
}

#[wasm_bindgen_test]
fn get_string_returns_the_right_value() {
    let db = MdixDatabase::load_str(SAMPLE).unwrap();
    let name = db.get_string("app_name").expect("app_name should exist and be a string");
    assert_eq!(name, "DixScript WASM Test");
}

#[wasm_bindgen_test]
fn get_int_returns_the_right_value() {
    let db = MdixDatabase::load_str(SAMPLE).unwrap();
    let version = db.get_int("version").expect("version should exist and be an int");
    assert_eq!(version, 1);
}

#[wasm_bindgen_test]
fn get_bool_returns_the_right_value() {
    let db = MdixDatabase::load_str(SAMPLE).unwrap();
    let ready = db.get_bool("ready").expect("ready should exist and be a bool");
    assert!(ready);
}

#[wasm_bindgen_test]
fn get_string_on_missing_path_errors_cleanly() {
    let db = MdixDatabase::load_str(SAMPLE).unwrap();
    let result = db.get_string("does_not_exist");
    assert!(result.is_err(), "a missing path should error, not panic or silently return empty");
}

#[wasm_bindgen_test]
fn from_json_round_trips_a_simple_object() {
    let json = r#"{"app_name": "FromJSON", "version": 2}"#;
    let db = MdixDatabase::from_json(json).expect("from_json should succeed on valid JSON");
    assert_eq!(db.get_string("app_name").unwrap(), "FromJSON");
    assert_eq!(db.get_int("version").unwrap(), 2);
}

#[wasm_bindgen_test]
fn from_toml_round_trips_a_simple_table() {
    let toml = "app_name = \"FromTOML\"\nversion = 3\n";
    let db = MdixDatabase::from_toml(toml).expect("from_toml should succeed on valid TOML");
    assert_eq!(db.get_string("app_name").unwrap(), "FromTOML");
    assert_eq!(db.get_int("version").unwrap(), 3);
}

#[wasm_bindgen_test]
fn entry_count_is_nonzero_for_valid_data() {
    let db = MdixDatabase::load_str(SAMPLE).unwrap();
    let count = db.entry_count().expect("entry_count should succeed on a valid database");
    assert!(count > 0, "a database with three fields should report a nonzero entry count");
}

#[wasm_bindgen_test]
fn prefetch_import_does_not_panic() {
    // Doesn't assert cache behavior here (that's exercised properly by the
    // native cloud_file_cache tests in the dixscript crate) — this is
    // purely confirming the wasm-bindgen binding itself is callable in a
    // real browser without panicking, since it touches
    // web_sys::window().local_storage() which only exists in this
    // run_in_browser configuration, not under Node.
    mdix_wasm::prefetch_import("https://example.com/fixture.mdix", "@DATA(x -> 1)");
      }
