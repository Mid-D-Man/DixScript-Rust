// mdix-cli/tests/config_tests.rs
//
// Each test runs against an isolated config directory: a fresh TempDir is
// injected via the MDIX_CONFIG_DIR environment variable that ConfigManager
// checks before falling back to ~/.dixscript. Previously every test shared
// the real ~/.dixscript/config.toml, so parallel test threads racing on the
// same keys (e.g. default_indent_size) produced flaky failures — most
// visibly in config_reset_single_key_exits_zero, which could observe a
// value written by a concurrently-running test like config_set_and_get_integer
// instead of the freshly-reset default.

mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn mdix(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("mdix").unwrap();
    cmd.env("MDIX_CONFIG_DIR", dir.path());
    cmd
}

fn reset_key(dir: &TempDir, key: &str) {
    mdix(dir)
        .args(["config", "reset", key])
        .assert()
        .success();
}

// ── List ──────────────────────────────────────────────────────────────────────

#[test]
fn config_list_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    mdix(&dir)
        .args(["config", "list"])
        .assert()
        .success()
        .code(0);
}

#[test]
fn config_list_shows_known_keys() {
    let dir = tempfile::tempdir().unwrap();
    mdix(&dir)
        .args(["config", "list"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("default_indent_size")
                .and(predicate::str::contains("color_output"))
                .and(predicate::str::contains("show_warnings")),
        );
}

#[test]
fn config_list_json_produces_object() {
    let dir = tempfile::tempdir().unwrap();
    let output = mdix(&dir)
        .args(["config", "--json", "list"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"].is_object());

    let result = helpers::results_file("config", "list.json");
    std::fs::write(result, &stdout).ok();
}

// ── Get ───────────────────────────────────────────────────────────────────────

#[test]
fn config_get_known_key_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    mdix(&dir)
        .args(["config", "get", "default_indent_size"])
        .assert()
        .success()
        .code(0);
}

#[test]
fn config_get_unknown_key_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    mdix(&dir)
        .args(["config", "get", "nonexistent_key_xyz"])
        .assert()
        .failure();
}

// ── Set + Get round-trip ──────────────────────────────────────────────────────

#[test]
fn config_set_and_get_integer() {
    let dir = tempfile::tempdir().unwrap();
    reset_key(&dir, "default_indent_size");

    mdix(&dir)
        .args(["config", "set", "default_indent_size", "4"])
        .assert()
        .success();

    mdix(&dir)
        .args(["config", "get", "default_indent_size"])
        .assert()
        .success()
        .stdout(predicate::str::contains("4"));

    reset_key(&dir, "default_indent_size");
}

#[test]
fn config_set_and_get_bool() {
    let dir = tempfile::tempdir().unwrap();
    reset_key(&dir, "use_tabs");

    mdix(&dir)
        .args(["config", "set", "use_tabs", "true"])
        .assert()
        .success();

    mdix(&dir)
        .args(["config", "get", "use_tabs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("true"));

    reset_key(&dir, "use_tabs");
}

#[test]
fn config_set_and_get_string() {
    let dir = tempfile::tempdir().unwrap();
    reset_key(&dir, "default_output_directory");

    mdix(&dir)
        .args(["config", "set", "default_output_directory", "/tmp/mdix_test"])
        .assert()
        .success();

    mdix(&dir)
        .args(["config", "get", "default_output_directory"])
        .assert()
        .success()
        .stdout(predicate::str::contains("/tmp/mdix_test"));

    reset_key(&dir, "default_output_directory");
}

#[test]
fn config_set_invalid_bool_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    mdix(&dir)
        .args(["config", "set", "use_tabs", "notabool"])
        .assert()
        .failure();
}

#[test]
fn config_set_invalid_integer_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    mdix(&dir)
        .args(["config", "set", "default_indent_size", "notanumber"])
        .assert()
        .failure();
}

// ── Reset ─────────────────────────────────────────────────────────────────────

#[test]
fn config_reset_single_key_exits_zero() {
    let dir = tempfile::tempdir().unwrap();

    mdix(&dir)
        .args(["config", "set", "default_indent_size", "8"])
        .assert()
        .success();

    mdix(&dir)
        .args(["config", "reset", "default_indent_size"])
        .assert()
        .success()
        .code(0);

    mdix(&dir)
        .args(["config", "get", "default_indent_size"])
        .assert()
        .success()
        .stdout(predicate::str::contains("2"));
}

#[test]
fn config_reset_all_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    mdix(&dir)
        .args(["config", "reset"])
        .assert()
        .success()
        .code(0);
    }
