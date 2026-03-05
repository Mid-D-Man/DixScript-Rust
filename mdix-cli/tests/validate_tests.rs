// mdix-cli/tests/validate_tests.rs

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

// ── Success cases ─────────────────────────────────────────────────────────────

#[test]
fn validate_basic_exits_zero() {
    mdix()
        .args(["validate", &fixture("basic.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn validate_with_enums_exits_zero() {
    mdix()
        .args(["validate", &fixture("with_enums.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn validate_with_functions_exits_zero() {
    mdix()
        .args(["validate", &fixture("with_functions.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn validate_prints_token_count() {
    mdix()
        .args(["validate", &fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("tokens"));
}

// ── Failure cases ─────────────────────────────────────────────────────────────

#[test]
fn validate_invalid_syntax_exits_nonzero() {
    mdix()
        .args(["validate", &fixture("invalid_syntax.mdix")])
        .assert()
        .failure();
}

#[test]
fn validate_missing_file_exits_two() {
    mdix()
        .args(["validate", "nonexistent.mdix"])
        .assert()
        .failure()
        .code(2);
}

// ── JSON output ───────────────────────────────────────────────────────────────

#[test]
fn validate_json_flag_produces_valid_json_on_success() {
    let output = mdix()
        .args(["validate", "--json", &fixture("basic.mdix")])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["token_count"].is_number());
}

#[test]
fn validate_json_flag_produces_valid_json_on_failure() {
    let output = mdix()
        .args(["validate", "--json", &fixture("invalid_syntax.mdix")])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stderr)
        .expect("stderr should be valid JSON");
    assert_eq!(parsed["success"], false);
    assert!(parsed["error"].is_string());
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn validate_quiet_produces_no_stdout_on_success() {
    mdix()
        .args(["validate", "--quiet", &fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}
