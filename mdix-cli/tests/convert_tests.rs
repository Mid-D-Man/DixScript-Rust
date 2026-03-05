// mdix-cli/tests/convert_tests.rs

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

// ── mdix → json ───────────────────────────────────────────────────────────────

#[test]
fn convert_mdix_to_json_exits_zero() {
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("output.json").to_string_lossy().to_string();
    mdix()
        .args(["convert", &fixture("basic.mdix"), "--to", "json", "-o", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn convert_mdix_to_json_produces_valid_json() {
    let tmp = TempDir::new().unwrap();
    let out_path = tmp.path().join("output.json");
    let out = out_path.to_string_lossy().to_string();

    mdix()
        .args(["convert", &fixture("basic.mdix"), "--to", "json", "-o", &out])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content)
        .expect("output should be valid JSON");
    assert!(parsed.is_object());
}

#[test]
fn convert_mdix_to_json_contains_expected_keys() {
    let tmp = TempDir::new().unwrap();
    let out_path = tmp.path().join("output.json");
    let out = out_path.to_string_lossy().to_string();

    mdix()
        .args(["convert", &fixture("basic.mdix"), "--to", "json", "-o", &out])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(content.contains("app_name") || content.contains("port"));
}

// ── json → mdix ───────────────────────────────────────────────────────────────

#[test]
fn convert_json_to_mdix_exits_zero() {
    let tmp = TempDir::new().unwrap();

    // First produce a JSON file from basic.mdix
    let json_path = tmp.path().join("basic.json");
    mdix()
        .args([
            "convert",
            &fixture("basic.mdix"),
            "--to", "json",
            "-o", json_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Then convert it back to mdix
    let mdix_out = tmp.path().join("recovered.mdix").to_string_lossy().to_string();
    mdix()
        .args([
            "convert",
            json_path.to_str().unwrap(),
            "--to", "mdix",
            "-o", &mdix_out,
        ])
        .assert()
        .success()
        .code(0);
}

// ── Unsupported format ────────────────────────────────────────────────────────

#[test]
fn convert_unknown_format_exits_four() {
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("out.xyz").to_string_lossy().to_string();
    mdix()
        .args(["convert", &fixture("basic.mdix"), "--to", "xyz", "-o", &out])
        .assert()
        .failure()
        .code(4);
}

// ── Missing input file ────────────────────────────────────────────────────────

#[test]
fn convert_missing_file_exits_two() {
    mdix()
        .args(["convert", "ghost.mdix", "--to", "json"])
        .assert()
        .failure()
        .code(2);
}

// ── Same format error ─────────────────────────────────────────────────────────

#[test]
fn convert_same_format_exits_nonzero() {
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("out.mdix").to_string_lossy().to_string();
    mdix()
        .args([
            "convert",
            &fixture("basic.mdix"),
            "--to", "mdix",
            "-o", &out,
        ])
        .assert()
        .failure();
}

// ── JSON flag ─────────────────────────────────────────────────────────────────

#[test]
fn convert_json_flag_produces_envelope() {
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("output.json").to_string_lossy().to_string();

    let output = mdix()
        .args([
            "convert",
            "--json",
            &fixture("basic.mdix"),
            "--to", "json",
            "-o", &out,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["input_path"].is_string());
    assert!(parsed["data"]["output_path"].is_string());
}
