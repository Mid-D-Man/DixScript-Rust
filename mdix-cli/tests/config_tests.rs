// mdix-cli/tests/config_tests.rs
//
// These tests mutate ~/.mdix/config.toml so each test resets the key it
// touches before and after to avoid cross-test pollution.

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

fn reset_key(key: &str) {
    mdix()
        .args(["config", "reset", key])
        .assert()
        .success();
}

// ── List ──────────────────────────────────────────────────────────────────────

#[test]
fn config_list_exits_zero() {
    mdix()
        .args(["config", "list"])
        .assert()
        .success()
        .code(0);
}

#[test]
fn config_list_shows_known_keys() {
    mdix()
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
    let output = mdix()
        .args(["config", "--json", "list"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"].is_object());
}

// ── Get ───────────────────────────────────────────────────────────────────────

#[test]
fn config_get_known_key_exits_zero() {
    mdix()
        .args(["config", "get", "default_indent_size"])
        .assert()
        .success()
        .code(0);
}

#[test]
fn config_get_unknown_key_exits_nonzero() {
    mdix()
        .args(["config", "get", "nonexistent_key_xyz"])
        .assert()
        .failure();
}

// ── Set + Get round-trip ──────────────────────────────────────────────────────

#[test]
fn config_set_and_get_integer() {
    reset_key("default_indent_size");

    mdix()
        .args(["config", "set", "default_indent_size", "4"])
        .assert()
        .success();

    mdix()
        .args(["config", "get", "default_indent_size"])
        .assert()
        .success()
        .stdout(predicate::str::contains("4"));

    reset_key("default_indent_size");
}

#[test]
fn config_set_and_get_bool() {
    reset_key("use_tabs");

    mdix()
        .args(["config", "set", "use_tabs", "true"])
        .assert()
        .success();

    mdix()
        .args(["config", "get", "use_tabs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("true"));

    reset_key("use_tabs");
}

#[test]
fn config_set_and_get_string() {
    reset_key("default_output_directory");

    mdix()
        .args(["config", "set", "default_output_directory", "/tmp/mdix_test"])
        .assert()
        .success();

    mdix()
        .args(["config", "get", "default_output_directory"])
        .assert()
        .success()
        .stdout(predicate::str::contains("/tmp/mdix_test"));

    reset_key("default_output_directory");
}

#[test]
fn config_set_invalid_bool_exits_nonzero() {
    mdix()
        .args(["config", "set", "use_tabs", "notabool"])
        .assert()
        .failure();
}

#[test]
fn config_set_invalid_integer_exits_nonzero() {
    mdix()
        .args(["config", "set", "default_indent_size", "notanumber"])
        .assert()
        .failure();
}

// ── Reset ─────────────────────────────────────────────────────────────────────

#[test]
fn config_reset_single_key_exits_zero() {
    mdix()
        .args(["config", "set", "default_indent_size", "8"])
        .assert()
        .success();

    mdix()
        .args(["config", "reset", "default_indent_size"])
        .assert()
        .success()
        .code(0);

    mdix()
        .args(["config", "get", "default_indent_size"])
        .assert()
        .success()
        .stdout(predicate::str::contains("2"));
}

#[test]
fn config_reset_all_exits_zero() {
    mdix()
        .args(["config", "reset"])
        .assert()
        .success()
        .code(0);
  }
