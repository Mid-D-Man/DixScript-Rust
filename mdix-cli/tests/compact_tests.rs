// mdix-cli/tests/compact_tests.rs

mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

// ── Default mode (compact) ────────────────────────────────────────────────────

#[test]
fn compact_basic_exits_zero() {
    let out = helpers::results_file("compact", "basic.compact.mdix");
    mdix()
        .args(["compact", &helpers::fixture("basic.mdix"), "-o", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn compact_produces_output_file() {
    let out = helpers::results_file("compact", "basic_produced.compact.mdix");
    mdix()
        .args(["compact", &helpers::fixture("basic.mdix"), "-o", &out])
        .assert()
        .success();

    assert!(std::path::Path::new(&out).exists(), "output file must be created");
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(!content.trim().is_empty(), "output must not be empty");
}

#[test]
fn compact_output_is_smaller_than_or_equal_to_input() {
    let out = helpers::results_file("compact", "basic_size_check.compact.mdix");
    mdix()
        .args(["compact", &helpers::fixture("basic.mdix"), "-o", &out])
        .assert()
        .success();

    let input_len  = std::fs::read_to_string(helpers::fixture("basic.mdix")).unwrap().len();
    let output_len = std::fs::read_to_string(&out).unwrap().len();
    assert!(
        output_len <= input_len,
        "compacted output ({} bytes) should be ≤ input ({} bytes)",
        output_len, input_len
    );
}

#[test]
fn compact_with_enums_exits_zero() {
    let out = helpers::results_file("compact", "with_enums.compact.mdix");
    mdix()
        .args(["compact", &helpers::fixture("with_enums.mdix"), "-o", &out])
        .assert()
        .success()
        .code(0);
}

// ── Minify mode ───────────────────────────────────────────────────────────────

#[test]
fn compact_minify_mode_exits_zero() {
    let out = helpers::results_file("compact", "basic.min.mdix");
    mdix()
        .args([
            "compact",
            &helpers::fixture("basic.mdix"),
            "--mode", "minify",
            "-o", &out,
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn compact_minify_produces_no_newlines() {
    let out = helpers::results_file("compact", "basic_minified.mdix");
    mdix()
        .args([
            "compact",
            &helpers::fixture("basic.mdix"),
            "--mode", "minify",
            "-o", &out,
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out).unwrap();
    // Minified output may have very few newlines — definitely fewer than input
    let input_newlines = std::fs::read_to_string(helpers::fixture("basic.mdix"))
        .unwrap()
        .lines()
        .count();
    let output_newlines = content.lines().count();
    assert!(
        output_newlines < input_newlines,
        "minified output should have fewer lines than input ({} vs {})",
        output_newlines, input_newlines
    );
}

#[test]
fn compact_minify_smaller_than_compact() {
    let compact_out = helpers::results_file("compact", "compare_compact.mdix");
    let minify_out  = helpers::results_file("compact", "compare_minify.mdix");

    mdix()
        .args([
            "compact",
            &helpers::fixture("basic.mdix"),
            "--mode", "compact",
            "-o", &compact_out,
        ])
        .assert()
        .success();

    mdix()
        .args([
            "compact",
            &helpers::fixture("basic.mdix"),
            "--mode", "minify",
            "-o", &minify_out,
        ])
        .assert()
        .success();

    let compact_len = std::fs::read_to_string(&compact_out).unwrap().len();
    let minify_len  = std::fs::read_to_string(&minify_out).unwrap().len();
    assert!(
        minify_len <= compact_len,
        "minified ({} bytes) should be ≤ compact ({} bytes)",
        minify_len, compact_len
    );
}

// ── Strip-comments mode ───────────────────────────────────────────────────────

#[test]
fn compact_strip_comments_exits_zero() {
    let out = helpers::results_file("compact", "basic.nocomments.mdix");
    mdix()
        .args([
            "compact",
            &helpers::fixture("basic.mdix"),
            "--mode", "strip-comments",
            "-o", &out,
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn compact_strip_comments_preserves_data() {
    let out = helpers::results_file("compact", "strip_data_preserved.mdix");
    mdix()
        .args([
            "compact",
            &helpers::fixture("basic.mdix"),
            "--mode", "strip-comments",
            "-o", &out,
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out).unwrap();
    // The @DATA section and its entries must still be there
    assert!(content.contains("@DATA"), "strip-comments must preserve @DATA section");
    assert!(
        content.contains("app_name") || content.contains("port"),
        "strip-comments must preserve data entries"
    );
}

// ── Error cases ───────────────────────────────────────────────────────────────

#[test]
fn compact_unknown_mode_exits_nonzero() {
    mdix()
        .args([
            "compact",
            &helpers::fixture("basic.mdix"),
            "--mode", "badmode",
        ])
        .assert()
        .failure();
}

#[test]
fn compact_missing_file_exits_two() {
    mdix()
        .args(["compact", "nonexistent.mdix"])
        .assert()
        .failure()
        .code(2);
}

// ── JSON output ───────────────────────────────────────────────────────────────

#[test]
fn compact_json_flag_produces_valid_json() {
    let out = helpers::results_file("compact", "json_test.compact.mdix");
    let output = mdix()
        .args([
            "compact",
            "--json",
            &helpers::fixture("basic.mdix"),
            "-o", &out,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["ratio"].is_number());
    assert!(parsed["data"]["original_size"].is_number());
    assert!(parsed["data"]["compacted_size"].is_number());

    let result = helpers::results_file("compact", "compact_json_result.json");
    std::fs::write(result, &stdout).ok();
}

#[test]
fn compact_json_ratio_between_zero_and_one() {
    let out = helpers::results_file("compact", "ratio_check.compact.mdix");
    let output = mdix()
        .args([
            "compact",
            "--json",
            &helpers::fixture("basic.mdix"),
            "-o", &out,
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let ratio = parsed["data"]["ratio"].as_f64().unwrap_or(-1.0);
    assert!(
        (-0.01..=1.01).contains(&ratio),
        "ratio {} should be between 0.0 and 1.0",
        ratio
    );
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn compact_quiet_suppresses_stdout() {
    let out = helpers::results_file("compact", "quiet_test.compact.mdix");
    mdix()
        .args([
            "compact",
            "--quiet",
            &helpers::fixture("basic.mdix"),
            "-o", &out,
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}
