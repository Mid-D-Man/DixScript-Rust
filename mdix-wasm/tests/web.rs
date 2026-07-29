//! Runs in a real headless browser via `wasm-pack test --headless
//! --chrome` (see .github/workflows/wasm-tests.yml) — not just a compile
//! check. wasm_bindgen_test_configure!(run_in_browser) below is what makes
//! that distinction matter: without it these would try to run under
//! Node.js instead, where `web_sys::window()` (used by the localStorage
//! cache backend) would be `None`.

use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use mdix_wasm::{MdixDatabase, merge_sources, merge_sources_weighted};

wasm_bindgen_test_configure!(run_in_browser);

const SAMPLE: &str = r#"
@DATA(
  app_name = "DixScript WASM Test",
  version  = 1,
  ready    = true
)
"#;

// ── Core load / read ─────────────────────────────────────────────────────

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

// ── Cloud import (prefetch_import + localStorage cache) ─────────────────
//
// This only exercises resolution succeeding/failing, not the imported
// content's actual effect (e.g. calling an imported quickfunc) — that
// would need a fuller fixture and I haven't traced the namespace/quickfunc
// resolution path closely enough to assert on it with confidence. Flag it
// back if this doesn't behave as expected; the assertion here is
// deliberately conservative (does resolution succeed at all) rather than
// deeply verifying import semantics I can't run locally to confirm.

#[wasm_bindgen_test]
fn prefetch_import_does_not_panic() {
    // Doesn't assert cache behavior here (that's exercised properly by the
    // native cloud_file_cache tests in the dixscript crate) — this is
    // purely confirming the wasm-bindgen binding itself is callable in a
    // real browser without panicking, since it touches
    // web_sys::window().local_storage() which only exists in this
    // run_in_browser configuration, not under Node.
    mdix_wasm::prefetch_import("https://example.com/fixture.mdix", "@DATA(x = 1)");
}

#[wasm_bindgen_test]
fn prefetch_import_then_cloud_import_resolves() {
    let cloud_url     = "https://example.com/shared-fixture.mdix";
    let cloud_content = r#"
@QUICKFUNCS(
  ~double<int>(_x) {
    return _x * 2
  }
)
"#;
    mdix_wasm::prefetch_import(cloud_url, cloud_content);

    let importing_source = format!(
        r#"
@IMPORTS(
  shared from_cloud "{}"
)
@DATA(
  x = 1
)
"#,
        cloud_url
    );

    let result = MdixDatabase::load_str(&importing_source);
    assert!(
        result.is_ok(),
        "a source importing a URL that was already prefetch_import()'d should resolve from \
         the localStorage cache instead of erroring — got: {:?}",
        result.err()
    );
}

#[wasm_bindgen_test]
fn cloud_import_without_prefetch_errors_cleanly() {
    // No prefetch_import() call for this URL — the cache is empty, so this
    // should fail with a clear resolution error rather than panicking or
    // hanging on a network call wasm32 can't make from inside the module.
    let importing_source = r#"
@IMPORTS(
  shared from_cloud "https://example.com/never-prefetched.mdix"
)
@DATA(
  x = 1
)
"#;
    let result = MdixDatabase::load_str(importing_source);
    assert!(
        result.is_err(),
        "an unresolvable cloud import should error cleanly, not silently drop the import"
    );
}

// ── Merging ───────────────────────────────────────────────────────────────

#[wasm_bindgen_test]
fn merge_sources_rejects_empty_list() {
    let result = merge_sources(vec![], None, None);
    assert!(result.is_err(), "merging an empty source list should error, not panic");
}

#[wasm_bindgen_test]
fn merge_sources_combines_disjoint_data() {
    let a = "@DATA(x = 1)".to_string();
    let b = "@DATA(y = 2)".to_string();
    let mut outcome = merge_sources(vec![a, b], None, None)
        .expect("merging two disjoint sources should succeed");
    let db = outcome.database().expect("database() should be consumable exactly once");
    assert_eq!(db.get_int("x").unwrap(), 1);
    assert_eq!(db.get_int("y").unwrap(), 2);
}

#[wasm_bindgen_test]
fn merge_sources_throw_on_conflict_succeeds_for_disjoint_data() {
    // Regression test: every parsed source gets identical auto-populated
    // minimal @CONFIG defaults even when the user writes no @CONFIG block
    // at all, which used to make merge_config flag a "conflict" on every
    // multi-source merge regardless of @DATA content — ThrowOnConflict
    // raised unconditionally even for these genuinely disjoint sources.
    let a = "@DATA(x = 1)".to_string();
    let b = "@DATA(y = 2)".to_string();
    let result = merge_sources(vec![a, b], Some("throw_on_conflict".to_string()), None);
    assert!(
        result.is_ok(),
        "disjoint @DATA keys with no explicit @CONFIG should not conflict under \
         throw_on_conflict — got: {:?}",
        result.err()
    );
}

#[wasm_bindgen_test]
fn merge_sources_throw_on_conflict_raises_for_real_conflict() {
    let a = "@DATA(x = 1)".to_string();
    let b = "@DATA(x = 2)".to_string();
    let result = merge_sources(vec![a, b], Some("throw_on_conflict".to_string()), None);
    assert!(
        result.is_err(),
        "a genuinely conflicting key (same key, different value) should still raise \
         under throw_on_conflict"
    );
}

#[wasm_bindgen_test]
fn merge_sources_primary_wins_on_conflict() {
    let a = "@DATA(x = 1)".to_string();
    let b = "@DATA(x = 2)".to_string();
    let mut outcome = merge_sources(vec![a, b], Some("primary_wins".to_string()), None)
        .expect("primary_wins should never raise on conflict");
    let db = outcome.database().unwrap();
    assert_eq!(db.get_int("x").unwrap(), 1, "primary_wins should keep the first source's value");
}

#[wasm_bindgen_test]
fn merge_sources_secondary_wins_on_conflict() {
    let a = "@DATA(x = 1)".to_string();
    let b = "@DATA(x = 2)".to_string();
    let mut outcome = merge_sources(vec![a, b], Some("secondary_wins".to_string()), None)
        .expect("secondary_wins should never raise on conflict");
    let db = outcome.database().unwrap();
    assert_eq!(db.get_int("x").unwrap(), 2, "secondary_wins should keep the second source's value");
}

#[wasm_bindgen_test]
fn merge_sources_conflicts_report_has_the_expected_shape() {
    let a = "@DATA(x = 1)".to_string();
    let b = "@DATA(x = 2)".to_string();
    let outcome = merge_sources(vec![a, b], Some("primary_wins".to_string()), None).unwrap();
    let conflicts_value = outcome.conflicts().expect("conflicts() should return valid JSON");
    let conflicts_array = js_sys::Array::from(&conflicts_value);
    assert!(
        conflicts_array.length() > 0,
        "a genuine key conflict should show up in the conflicts report"
    );
    let first = conflicts_array.get(0);
    assert!(
        js_sys::Reflect::has(&first, &JsValue::from_str("path")).unwrap_or(false),
        "each conflict entry should have a 'path' field"
    );
    assert!(
        js_sys::Reflect::has(&first, &JsValue::from_str("winningSource")).unwrap_or(false),
        "each conflict entry should have a 'winningSource' field"
    );
}

#[wasm_bindgen_test]
fn merge_sources_weighted_respects_explicit_weights() {
    let pair_a = js_sys::Array::new();
    pair_a.push(&JsValue::from_str("@DATA(x = 1)"));
    pair_a.push(&JsValue::from_f64(0.9));

    let pair_b = js_sys::Array::new();
    pair_b.push(&JsValue::from_str("@DATA(x = 2)"));
    pair_b.push(&JsValue::from_f64(0.1));

    let entries: Vec<JsValue> = vec![pair_a.into(), pair_b.into()];

    let mut outcome = merge_sources_weighted(entries, Some("weighted".to_string()), None)
        .expect("weighted merge with explicit weights should succeed");
    let db = outcome.database().unwrap();
    assert_eq!(
        db.get_int("x").unwrap(),
        1,
        "the higher-weighted source (0.9 vs 0.1) should win under the weighted strategy"
    );
}

#[wasm_bindgen_test]
fn merge_sources_weighted_rejects_empty_list() {
    let result = merge_sources_weighted(vec![], None, None);
    assert!(result.is_err(), "merging an empty weighted list should error, not panic");
}

#[wasm_bindgen_test]
fn merge_with_merges_two_loaded_databases() {
    // Regression test: MdixDatabase.mergeWith previously had no real
    // wasm-bindgen binding at all — merge.rs's merge_with() was a plain
    // Rust free function, never attached to the #[wasm_bindgen] impl block
    // or re-exported from lib.rs, so this was unreachable from JS despite
    // being documented at the top of merge.rs. This confirms it's wired up.
    let primary   = MdixDatabase::load_str("@DATA(x = 1)").unwrap();
    let secondary = MdixDatabase::load_str("@DATA(y = 2)").unwrap();

    let mut outcome = primary
        .merge_with(&secondary, None, None)
        .expect("mergeWith should succeed for two valid, disjoint databases");
    let merged = outcome.database().unwrap();
    assert_eq!(merged.get_int("x").unwrap(), 1);
    assert_eq!(merged.get_int("y").unwrap(), 2);
}

#[wasm_bindgen_test]
fn merge_with_primary_wins_by_default_weighting() {
    let primary   = MdixDatabase::load_str("@DATA(x = 1)").unwrap();
    let secondary = MdixDatabase::load_str("@DATA(x = 2)").unwrap();

    // mergeWith weights primary at 1.0 and secondary at 0.5, so under the
    // default "weighted" strategy primary should win a genuine conflict.
    let mut outcome = primary.merge_with(&secondary, None, None).unwrap();
    let merged = outcome.database().unwrap();
    assert_eq!(merged.get_int("x").unwrap(), 1);
}

// ── GroupArray combine-by-source (the "camo configs" scenario) ──────────
//
// Three sources, each declaring ONE item under the SAME top-level GroupArray
// path, each item identified by its own EquipableItemCamoClassId — same
// shape as combining separate per-weapon-class camo config files into one
// array. Confirms: (1) as long as the *source strings themselves* don't
// collide on a SimpleProperty/TableProperty key, non-throwing strategies
// combine the GroupArray items rather than replacing; (2) throw_on_conflict
// specifically does NOT work for this, since any same-path GroupArray
// collision routes through pick_winner regardless of array_strategy.

const SMG_CAMOS: &str = r#"
@DATA(
  CamoConfigs:: {
    EquipableItemCamoClassId = "ALL_LEGENDARY_SMG_CAMOS_CONFIG",
    MainItemId = "1C_X39XX_HADES"
  }
)
"#;

const SNIPER_CAMOS: &str = r#"
@DATA(
  CamoConfigs:: {
    EquipableItemCamoClassId = "ALL_LEGENDARY_SNIPER_CAMOS_CONFIG",
    MainItemId = "1C_DRAGONBOLT_REDROSE"
  }
)
"#;

const SHOTGUN_CAMOS: &str = r#"
@DATA(
  CamoConfigs:: {
    EquipableItemCamoClassId = "ALL_LEGENDARY_SHOTGUN_CAMOS_CONFIG",
    MainItemId = "1C_WINFAUST88_DISCHARGE"
  }
)
"#;

#[wasm_bindgen_test]
fn merge_sources_combines_non_conflicting_group_array_items() {
    let sources = vec![
        SMG_CAMOS.to_string(),
        SNIPER_CAMOS.to_string(),
        SHOTGUN_CAMOS.to_string(),
    ];

    // "weighted" (default strategy) + "concat_dedup" (default array
    // strategy) is the "proper mode" for this: no key in any of the three
    // sources collides with another (each has its own unique
    // EquipableItemCamoClassId), so nothing here is a *real* conflict.
    let mut outcome = merge_sources(sources, None, None)
        .expect("three sources with non-conflicting GroupArray items should merge cleanly");
    let db = outcome.database().unwrap();

    let len = db.get_array_length("CamoConfigs")
        .expect("CamoConfigs should exist as an array after merging");
    assert_eq!(len, 3, "all three per-weapon-class configs should survive as separate array items");

    // Order follows source order (SMG, Sniper, Shotgun) since ConcatDedup
    // appends the loser's items after the winner's, and with default
    // descending weights source[0] is always the winner here.
    assert_eq!(
        db.get_string("CamoConfigs[0].EquipableItemCamoClassId").unwrap(),
        "ALL_LEGENDARY_SMG_CAMOS_CONFIG"
    );
    assert_eq!(
        db.get_string("CamoConfigs[1].EquipableItemCamoClassId").unwrap(),
        "ALL_LEGENDARY_SNIPER_CAMOS_CONFIG"
    );
    assert_eq!(
        db.get_string("CamoConfigs[2].EquipableItemCamoClassId").unwrap(),
        "ALL_LEGENDARY_SHOTGUN_CAMOS_CONFIG"
    );
}

#[wasm_bindgen_test]
fn merge_sources_group_array_combine_fails_under_throw_on_conflict() {
    // Same three genuinely-non-conflicting sources as above, but with
    // throw_on_conflict — documents the caveat from our discussion: ANY
    // same-path GroupArray collision across sources calls pick_winner
    // before array_strategy ever runs, so throw_on_conflict raises here
    // even though nothing actually clashes. Use "weighted" /
    // "primary_wins" / "secondary_wins" instead when you want combine
    // behavior for a shared GroupArray key.
    let sources = vec![SMG_CAMOS.to_string(), SNIPER_CAMOS.to_string()];
    let result = merge_sources(sources, Some("throw_on_conflict".to_string()), None);
    assert!(
        result.is_err(),
        "throw_on_conflict raises on ANY shared GroupArray path, even non-conflicting items — \
         this is the caveat, not a bug in this test"
    );
}

#[wasm_bindgen_test]
fn merge_sources_group_array_same_id_twice_is_not_key_merged() {
    // Documents the other caveat: array_strategy has no concept of
    // EquipableItemCamoClassId being "the key" — ConcatDedup only skips an
    // incoming item if it's a byte-for-byte duplicate of one already
    // present. Two items sharing the same ID but different content both
    // survive as SEPARATE entries, not deep-merged into one.
    let a = r#"
@DATA(
  CamoConfigs:: {
    EquipableItemCamoClassId = "ALL_LEGENDARY_SMG_CAMOS_CONFIG",
    MainItemId = "1C_X39XX_HADES"
  }
)
"#.to_string();
    let b = r#"
@DATA(
  CamoConfigs:: {
    EquipableItemCamoClassId = "ALL_LEGENDARY_SMG_CAMOS_CONFIG",
    MainItemId = "1C_ALIYAHOO419_CRIMSONVORTEX"
  }
)
"#.to_string();

    let mut outcome = merge_sources(vec![a, b], None, None).unwrap();
    let db = outcome.database().unwrap();
    let len = db.get_array_length("CamoConfigs").unwrap();
    assert_eq!(
        len, 2,
        "two items sharing an EquipableItemCamoClassId but different MainItemId are NOT \
         key-merged into one — they survive as two separate array entries, which is the \
         current (dumb-concat) behavior, not a bug"
    );
}


// ── DLM (compress / encrypt / audit) ─────────────────────────────────────

use mdix_wasm::{compile_with_dlm, decompile_with_dlm};

#[wasm_bindgen_test]
fn compile_with_dlm_round_trips_with_compression_and_encryption() {
    let source = r#"
@DLM(DCompressor.gzip, DEncryptor.aes256)
@DATA(
  secret = "shh, this should survive compression and encryption",
  count  = 42
)
"#;

    let outcome = compile_with_dlm(source, "dlm-roundtrip-test")
        .expect("compileWithDlm should succeed with gzip + aes256");

    assert!(outcome.isSuccess(), "DLM pipeline should report success");
    assert!(!outcome.processedData().is_empty(), "processedData should be non-empty");
    assert!(
        outcome.keyFileContent().is_some(),
        "keyFileContent should be populated when DEncryptor ran"
    );
    let modules = outcome.executedModules();
    assert!(
        modules.iter().any(|m| m.to_lowercase().contains("compressor")),
        "executedModules should mention the compressor: {:?}", modules
    );
    assert!(
        modules.iter().any(|m| m.to_lowercase().contains("encryptor")),
        "executedModules should mention the encryptor: {:?}", modules
    );

    let key_content = outcome.keyFileContent().unwrap();
    let db = decompile_with_dlm(outcome.processedData(), &key_content, "dlm-roundtrip-test")
        .expect("decompileWithDlm should reverse compileWithDlm's output");

    assert_eq!(
        db.get_string("secret").unwrap(),
        "shh, this should survive compression and encryption"
    );
    assert_eq!(db.get_int("count").unwrap(), 42);
}

#[wasm_bindgen_test]
fn compile_with_dlm_passthrough_when_no_dlm_section() {
    // No @DLM section at all — should still succeed, just with nothing
    // compressed or encrypted (mirrors the native
    // determine_dlm_behavior's own has_compressor/has_encryptor guard).
    let source = r#"@DATA(plain = "just plain data, no @DLM section")"#;

    let outcome = compile_with_dlm(source, "dlm-passthrough-test")
        .expect("compileWithDlm should succeed even with no @DLM section");

    assert!(outcome.isSuccess());
    assert!(!outcome.processedData().is_empty());
    assert!(
        outcome.keyFileContent().is_none(),
        "keyFileContent should be absent when no DEncryptor ran"
    );
    assert!(outcome.executedModules().is_empty());

    // Empty string key content signals decompileWithDlm to unpack
    // directly instead of attempting decryption — the mirror image of
    // the no-modules case above.
    let db = decompile_with_dlm(outcome.processedData(), "", "dlm-passthrough-test")
        .expect("decompileWithDlm should unpack plain (non-DLM) data directly");

    assert_eq!(db.get_string("plain").unwrap(), "just plain data, no @DLM section");
}

#[wasm_bindgen_test]
fn compile_with_dlm_rejects_empty_source() {
    let result = compile_with_dlm("", "empty-test");
    assert!(result.is_err(), "empty source should error, not panic");
}

#[wasm_bindgen_test]
fn decompile_with_dlm_rejects_empty_data() {
    let result = decompile_with_dlm(vec![], "", "empty-test");
    assert!(result.is_err(), "empty data should error, not panic");
}

// ── Schema validation ────────────────────────────────────────────────────
//
// Regression test: MdixDatabase.validateSchema had no real wasm-bindgen
// binding at all — MdixSchema and MdixValidationReport both existed and
// schema.rs's own module doc comment demonstrated `db.validateSchema(schema)`,
// but nothing in database.rs's #[wasm_bindgen] impl block ever called
// SchemaBuilder::validate(&self, data), so this was unreachable from JS
// entirely — the same shape of miss as the mergeWith regression above.
// This confirms it's wired up.

use mdix_wasm::MdixSchema;

#[wasm_bindgen_test]
fn validate_schema_passes_for_matching_data() {
    let db = MdixDatabase::load_str(r#"@DATA(
        app_name = "MyApp",
        port     = 8080
    )"#).unwrap();

    let schema = MdixSchema::new()
        .require_string("app_name")
        .require_int("port");

    let report = db.validate_schema(&schema).expect("validateSchema should succeed");
    assert!(report.is_valid());
    assert_eq!(report.error_count(), 0);
}

#[wasm_bindgen_test]
fn validate_schema_reports_missing_field() {
    let db = MdixDatabase::load_str(r#"@DATA(app_name = "MyApp")"#).unwrap();

    let schema = MdixSchema::new()
        .require_string("app_name")
        .require_int("port");

    let report = db.validate_schema(&schema).unwrap();
    assert!(!report.is_valid());
    assert_eq!(report.error_count(), 1);
    assert_eq!(report.failed_paths(), vec!["port".to_string()]);
}

#[wasm_bindgen_test]
fn validate_schema_reports_wrong_type() {
    let db = MdixDatabase::load_str(r#"@DATA(port = "not-a-number")"#).unwrap();

    let schema = MdixSchema::new().require_int("port");

    let report = db.validate_schema(&schema).unwrap();
    assert!(!report.is_valid());
}

#[wasm_bindgen_test]
fn validate_schema_can_be_reused_across_databases() {
    // MdixSchema is borrowed (not consumed) by validateSchema, so the same
    // schema instance must be able to validate more than one database.
    let schema = MdixSchema::new().require_string("name");

    let db_a = MdixDatabase::load_str(r#"@DATA(name = "Alice")"#).unwrap();
    let db_b = MdixDatabase::load_str(r#"@DATA(name = "Bob")"#).unwrap();

    assert!(db_a.validate_schema(&schema).unwrap().is_valid());
    assert!(db_b.validate_schema(&schema).unwrap().is_valid());
}
