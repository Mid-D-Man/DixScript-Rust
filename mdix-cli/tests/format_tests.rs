
mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

// ── Basic formatting ──────────────────────────────────────────────────────────

#[test]
fn format_basic_exits_zero() {
    let out = helpers::results_file("format", "basic_formatted.mdix");
    mdix()
        .args(["format", &helpers::fixture("basic.mdix"), "-o", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn format_with_enums_exits_zero() {
    let out = helpers::results_file("format", "with_enums_formatted.mdix");
    mdix()
        .args(["format", &helpers::fixture("with_enums.mdix"), "-o", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn format_with_functions_exits_zero() {
    let out = helpers::results_file("format", "with_functions_formatted.mdix");
    mdix()
        .args(["format", &helpers::fixture("with_functions.mdix"), "-o", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn format_output_file_is_created() {
    let out = helpers::results_file("format", "created_check.mdix");
    mdix()
        .args(["format", &helpers::fixture("basic.mdix"), "-o", &out])
        .assert()
        .success();

    assert!(
        std::path::Path::new(&out).exists(),
        "format must write the output file"
    );
}

#[test]
fn format_output_is_nonempty() {
    let out = helpers::results_file("format", "nonempty_check.mdix");
    mdix()
        .args(["format", &helpers::fixture("basic.mdix"), "-o", &out])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out).unwrap();
    assert!(!content.trim().is_empty(), "formatted output must not be empty");
}

#[test]
fn format_output_contains_data_section() {
    let out = helpers::results_file("format", "has_data.mdix");
    mdix()
        .args(["format", &helpers::fixture("basic.mdix"), "-o", &out])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("@DATA"), "formatted output must retain @DATA section");
}

#[test]
fn format_output_is_valid_dixscript() {
    // Format then validate — the round-trip must produce a parseable file.
    let out = helpers::results_file("format", "valid_after_format.mdix");
    mdix()
        .args(["format", &helpers::fixture("basic.mdix"), "-o", &out])
        .assert()
        .success();

    mdix()
        .args(["validate", &out])
        .assert()
        .success()
        .code(0);
}

// ── --indent flag ─────────────────────────────────────────────────────────────

#[test]
fn format_indent_4_exits_zero() {
    let out = helpers::results_file("format", "indent4.mdix");
    mdix()
        .args([
            "format",
            &helpers::fixture("basic.mdix"),
            "--indent", "4",
            "-o", &out,
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn format_indent_4_produces_4_space_indents() {
    let out = helpers::results_file("format", "indent4_check.mdix");
    mdix()
        .args([
            "format",
            &helpers::fixture("basic.mdix"),
            "--indent", "4",
            "-o", &out,
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out).unwrap();
    // At least one indented line should have 4-space indent
    assert!(
        content.lines().any(|l| l.starts_with("    ") && !l.starts_with("     ")),
        "4-space indented output should contain lines starting with exactly 4 spaces"
    );
}

// ── --tabs flag ───────────────────────────────────────────────────────────────

#[test]
fn format_tabs_exits_zero() {
    let out = helpers::results_file("format", "tabs.mdix");
    mdix()
        .args([
            "format",
            &helpers::fixture("basic.mdix"),
            "--tabs",
            "-o", &out,
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn format_tabs_produces_tab_indents() {
    let out = helpers::results_file("format", "tabs_check.mdix");
    mdix()
        .args([
            "format",
            &helpers::fixture("basic.mdix"),
            "--tabs",
            "-o", &out,
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out).unwrap();
    assert!(
        content.contains('\t'),
        "tab-indented output must contain at least one tab character"
    );
}

// ── --check flag ──────────────────────────────────────────────────────────────

#[test]
fn format_check_on_just_formatted_file_exits_zero() {
    // Format first, then --check. A file that was just formatted by
    // the formatter should pass its own check.
    let formatted = helpers::results_file("format", "for_check_test.mdix");

    mdix()
        .args([
            "format",
            &helpers::fixture("basic.mdix"),
            "-o", &formatted,
        ])
        .assert()
        .success();

    // --check on the already-formatted version should exit 0.
    mdix()
        .args(["format", "--check", &formatted])
        .assert()
        .success()
        .code(0);
}

#[test]
fn format_check_does_not_modify_file() {
    let path = helpers::results_file("format", "check_no_modify.mdix");

    // Set up a formatted file.
    mdix()
        .args([
            "format",
            &helpers::fixture("basic.mdix"),
            "-o", &path,
        ])
        .assert()
        .success();

    let before = std::fs::read_to_string(&path).unwrap();

    // Run --check.
    mdix()
        .args(["format", "--check", &path])
        .assert()
        .success();

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        before, after,
        "--check must never modify the file contents"
    );
}

// ── Error cases ───────────────────────────────────────────────────────────────

#[test]
fn format_missing_file_exits_two() {
    mdix()
        .args(["format", "nonexistent.mdix"])
        .assert()
        .failure()
        .code(2);
}

// ── JSON output ───────────────────────────────────────────────────────────────

#[test]
fn format_json_flag_produces_valid_json() {
    let out = helpers::results_file("format", "json_out.mdix");
    let output = mdix()
        .args([
            "format",
            "--json",
            &helpers::fixture("basic.mdix"),
            "-o", &out,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout must be valid JSON");

    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["file_path"].is_string());

    let result = helpers::results_file("format", "format_json_result.json");
    std::fs::write(result, &stdout).ok();
}

#[test]
fn format_check_json_has_already_formatted_field() {
    let formatted = helpers::results_file("format", "check_json_test.mdix");

    mdix()
        .args([
            "format",
            &helpers::fixture("basic.mdix"),
            "-o", &formatted,
        ])
        .assert()
        .success();

    let output = mdix()
        .args(["format", "--json", "--check", &formatted])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout must be valid JSON");

    assert!(
        parsed["data"]["already_formatted"].is_boolean(),
        "JSON output for --check must include 'already_formatted' field"
    );
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn format_quiet_suppresses_stdout_on_success() {
    let out = helpers::results_file("format", "quiet_format.mdix");
    mdix()
        .args([
            "format",
            "--quiet",
            &helpers::fixture("basic.mdix"),
            "-o", &out,
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
      }
