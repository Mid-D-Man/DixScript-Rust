
mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

// ── Success cases ─────────────────────────────────────────────────────────────

#[test]
fn compile_basic_exits_zero() {
    let out = helpers::results_dir("compile");
    mdix()
        .args([
            "compile",
            &helpers::fixture("basic.mdix"),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn compile_with_enums_exits_zero() {
    let out = helpers::results_dir("compile");
    mdix()
        .args([
            "compile",
            &helpers::fixture("with_enums.mdix"),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn compile_with_functions_exits_zero() {
    let out = helpers::results_dir("compile");
    mdix()
        .args([
            "compile",
            &helpers::fixture("with_functions.mdix"),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn compile_prints_source_path() {
    let out = helpers::results_dir("compile");
    mdix()
        .args([
            "compile",
            &helpers::fixture("basic.mdix"),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("basic.mdix")
                .or(predicate::str::contains("Compiled")),
        );
}

// ── Invalid syntax — Approach-B lenient behaviour ─────────────────────────────
//
// With Approach B the tokenizer runs before @CONFIG is parsed, so the
// pipeline operates in lenient mode: errors are collected without stopping
// and the command exits 0 even for files with syntax problems.

#[test]
fn compile_invalid_syntax_exits_zero() {
    // Approach-B lenient mode: the pipeline collects errors without
    // hard-stopping. The compile command exits 0 for invalid syntax.
    let out = helpers::results_dir("compile");
    mdix()
        .args([
            "compile",
            &helpers::fixture("invalid_syntax.mdix"),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success()
        .code(0);
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

// ── JSON output ───────────────────────────────────────────────────────────────

#[test]
fn compile_json_flag_produces_valid_json() {
    let out = helpers::results_dir("compile");
    let output = mdix()
        .args([
            "compile",
            "--json",
            &helpers::fixture("basic.mdix"),
            "-o",
            out.to_str().unwrap(),
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

    let result_file = helpers::results_file("compile", "basic_compile.json");
    std::fs::write(result_file, &stdout).ok();
}

// ── Inspect after compile ─────────────────────────────────────────────────────

#[test]
fn inspect_after_compile_shows_data_section() {
    mdix()
        .args(["inspect", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("@DATA"));
}
