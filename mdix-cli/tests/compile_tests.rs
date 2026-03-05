// mdix-cli/tests/compile_tests.rs

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;
use tempfile::TempDir;

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
fn compile_basic_exits_zero() {
    let tmp = TempDir::new().unwrap();
    mdix()
        .args([
            "compile",
            &fixture("basic.mdix"),
            "-o",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn compile_with_enums_exits_zero() {
    let tmp = TempDir::new().unwrap();
    mdix()
        .args([
            "compile",
            &fixture("with_enums.mdix"),
            "-o",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn compile_with_functions_exits_zero() {
    let tmp = TempDir::new().unwrap();
    mdix()
        .args([
            "compile",
            &fixture("with_functions.mdix"),
            "-o",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn compile_prints_source_path() {
    let tmp = TempDir::new().unwrap();
    mdix()
        .args([
            "compile",
            &fixture("basic.mdix"),
            "-o",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("basic.mdix").or(predicate::str::contains("Compiled")));
}

// ── Failure cases ─────────────────────────────────────────────────────────────

#[test]
fn compile_missing_file_exits_two() {
    mdix()
        .args(["compile", "does_not_exist.mdix"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn compile_invalid_syntax_exits_nonzero() {
    let tmp = TempDir::new().unwrap();
    mdix()
        .args([
            "compile",
            &fixture("invalid_syntax.mdix"),
            "-o",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .failure();
}

// ── JSON output ───────────────────────────────────────────────────────────────

#[test]
fn compile_json_flag_produces_valid_json() {
    let tmp = TempDir::new().unwrap();
    let output = mdix()
        .args([
            "compile",
            "--json",
            &fixture("basic.mdix"),
            "-o",
            tmp.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["source_path"].is_string());
    assert!(parsed["data"]["elapsed_ms"].is_number());
}

// ── Inspect after compile ─────────────────────────────────────────────────────

#[test]
fn inspect_after_compile_shows_data_section() {
    mdix()
        .args(["inspect", &fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("@DATA"));
          }
