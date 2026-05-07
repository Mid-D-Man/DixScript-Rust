// mdix-cli/tests/inspect_tests.rs

mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

// ── Basic inspection ──────────────────────────────────────────────────────────

#[test]
fn inspect_basic_exits_zero() {
    mdix()
        .args(["inspect", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn inspect_with_enums_exits_zero() {
    mdix()
        .args(["inspect", &helpers::fixture("with_enums.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn inspect_with_functions_exits_zero() {
    mdix()
        .args(["inspect", &helpers::fixture("with_functions.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn inspect_shows_data_section() {
    mdix()
        .args(["inspect", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("@DATA"));
}

#[test]
fn inspect_shows_version() {
    mdix()
        .args(["inspect", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("1.0.0"));
}

// ── --sections flag ───────────────────────────────────────────────────────────

#[test]
fn inspect_sections_flag_exits_zero() {
    mdix()
        .args(["inspect", "--sections", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn inspect_sections_shows_at_config() {
    mdix()
        .args(["inspect", "--sections", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("@CONFIG"));
}

#[test]
fn inspect_sections_shows_enums_when_present() {
    mdix()
        .args(["inspect", "--sections", &helpers::fixture("with_enums.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("@ENUMS"));
}

// ── --keys flag ───────────────────────────────────────────────────────────────

#[test]
fn inspect_keys_flag_exits_zero() {
    mdix()
        .args(["inspect", "--keys", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn inspect_keys_shows_app_name() {
    mdix()
        .args(["inspect", "--keys", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("app_name"));
}

#[test]
fn inspect_keys_shows_port() {
    mdix()
        .args(["inspect", "--keys", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("port"));
}

#[test]
fn inspect_keys_shows_type_column() {
    // The keys table has a type column showing "string", "int", "bool", etc.
    let output = mdix()
        .args(["inspect", "--keys", &helpers::fixture("basic.mdix")])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("string") || stdout.contains("int") || stdout.contains("bool"),
        "keys output should include type annotations: {}", stdout
    );
}

// ── Error cases ───────────────────────────────────────────────────────────────

#[test]
fn inspect_missing_file_exits_two() {
    mdix()
        .args(["inspect", "nonexistent.mdix"])
        .assert()
        .failure()
        .code(2);
}

// ── JSON output ───────────────────────────────────────────────────────────────

#[test]
fn inspect_json_flag_produces_valid_json() {
    let output = mdix()
        .args(["inspect", "--json", &helpers::fixture("basic.mdix")])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");

    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["key_count"].is_number());
    assert!(parsed["data"]["sections"].is_array());
    assert!(parsed["data"]["version"].is_string());

    let result = helpers::results_file("inspect", "inspect_basic.json");
    std::fs::write(result, &stdout).ok();
}

#[test]
fn inspect_json_sections_contains_data() {
    let output = mdix()
        .args(["inspect", "--json", &helpers::fixture("basic.mdix")])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let sections = parsed["data"]["sections"].as_array().unwrap();
    let section_strings: Vec<&str> = sections
        .iter()
        .filter_map(|s| s.as_str())
        .collect();

    assert!(
        section_strings.contains(&"@DATA"),
        "@DATA must be in sections list: {:?}",
        section_strings
    );
}

#[test]
fn inspect_json_key_count_positive() {
    let output = mdix()
        .args(["inspect", "--json", &helpers::fixture("basic.mdix")])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let key_count = parsed["data"]["key_count"].as_u64().unwrap_or(0);
    assert!(key_count > 0, "basic.mdix must have at least one key");
}

#[test]
fn inspect_json_with_keys_flag_includes_keys_array() {
    let output = mdix()
        .args(["inspect", "--json", "--keys", &helpers::fixture("basic.mdix")])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(
        parsed["data"]["keys"].is_array(),
        "with --keys flag, JSON output must include 'keys' array"
    );

    let keys = parsed["data"]["keys"].as_array().unwrap();
    assert!(!keys.is_empty(), "keys array must not be empty for basic.mdix");
    assert!(keys[0]["path"].is_string(), "each key entry must have a 'path' field");
    assert!(keys[0]["value_type"].is_string(), "each key entry must have a 'value_type' field");

    let result = helpers::results_file("inspect", "inspect_with_keys.json");
    std::fs::write(result, &stdout).ok();
}

#[test]
fn inspect_json_enum_count_for_with_enums_file() {
    let output = mdix()
        .args(["inspect", "--json", &helpers::fixture("with_enums.mdix")])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let enum_count = parsed["data"]["enum_count"].as_u64().unwrap_or(0);
    assert!(
        enum_count > 0,
        "with_enums.mdix must report at least one enum"
    );
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn inspect_quiet_suppresses_stdout() {
    mdix()
        .args(["inspect", "--quiet", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}
