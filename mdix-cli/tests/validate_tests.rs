
mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

// ── Success cases ─────────────────────────────────────────────────────────────

#[test]
fn validate_basic_exits_zero() {
    mdix()
        .args(["validate", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn validate_with_enums_exits_zero() {
    mdix()
        .args(["validate", &helpers::fixture("with_enums.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn validate_with_functions_exits_zero() {
    mdix()
        .args(["validate", &helpers::fixture("with_functions.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn validate_prints_token_count() {
    mdix()
        .args(["validate", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("tokens"));
}

// ── Invalid syntax — Approach-B lenient behaviour ─────────────────────────────
//
// With Approach B the tokenizer runs before @CONFIG is parsed, so no
// error-handling strategy from the file can influence tokenization. The
// pipeline is lenient by design: it collects diagnostics without
// hard-stopping and always exits 0. Tests here verify that the command
// completes without crashing and produces well-formed output.

#[test]
fn validate_invalid_syntax_exits_zero() {
    // The Approach-B pipeline runs the tokenizer before config is processed.
    // Errors are collected without stopping the pipeline, so exit code is 0.
    mdix()
        .args(["validate", &helpers::fixture("invalid_syntax.mdix")])
        .assert()
        .success()
        .code(0);
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
        .args(["validate", "--json", &helpers::fixture("basic.mdix")])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["token_count"].is_number());

    let out = helpers::results_file("validate", "basic_success.json");
    std::fs::write(out, &stdout).ok();
}

#[test]
fn validate_json_flag_produces_valid_json_on_invalid_file() {
    // The pipeline exits 0 for invalid syntax (Approach-B lenient mode).
    // The JSON envelope appears on stdout with success=true.
    let output = mdix()
        .args(["validate", "--json", &helpers::fixture("invalid_syntax.mdix")])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["token_count"].is_number());

    let out = helpers::results_file("validate", "invalid_syntax_json.json");
    std::fs::write(out, &stdout).ok();
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn validate_quiet_produces_no_stdout_on_success() {
    mdix()
        .args(["validate", "--quiet", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

// ── Strict flag ───────────────────────────────────────────────────────────────

#[test]
fn validate_strict_on_valid_file_exits_zero() {
    mdix()
        .args(["validate", "--strict", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .code(0);
        }
