
mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

fn key_path(name: &str) -> String {
    helpers::results_file("key", name)
}

/// Generate a key at `path` with the given algorithm and return that path.
fn generate_key(path: &str, algorithm: &str) -> String {
    mdix()
        .args(["key", "generate", "--output", path, "--algorithm", algorithm])
        .assert()
        .success();
    path.to_string()
}

// ── key generate ──────────────────────────────────────────────────────────────

#[test]
fn key_generate_aes256_exits_zero() {
    let out = key_path("gen_aes256.mdix.key");
    mdix()
        .args(["key", "generate", "--output", &out, "--algorithm", "aes256"])
        .assert()
        .success()
        .code(0);
}

#[test]
fn key_generate_aes128_exits_zero() {
    let out = key_path("gen_aes128.mdix.key");
    mdix()
        .args(["key", "generate", "--output", &out, "--algorithm", "aes128"])
        .assert()
        .success()
        .code(0);
}

#[test]
fn key_generate_chacha20_exits_zero() {
    let out = key_path("gen_chacha20.mdix.key");
    mdix()
        .args(["key", "generate", "--output", &out, "--algorithm", "chacha20"])
        .assert()
        .success()
        .code(0);
}

#[test]
fn key_generate_creates_file() {
    let out = key_path("gen_creates_file.mdix.key");
    mdix()
        .args(["key", "generate", "--output", &out])
        .assert()
        .success();

    assert!(
        std::path::Path::new(&out).exists(),
        "key generate must create the key file at the specified path"
    );
}

#[test]
fn key_generate_file_is_nonempty() {
    let out = key_path("gen_nonempty.mdix.key");
    mdix()
        .args(["key", "generate", "--output", &out])
        .assert()
        .success();

    let size = std::fs::metadata(&out).unwrap().len();
    assert!(size > 0, "generated key file must not be empty");
}

#[test]
fn key_generate_file_contains_algorithm_marker() {
    let out = key_path("gen_has_algo.mdix.key");
    mdix()
        .args(["key", "generate", "--output", &out, "--algorithm", "aes256"])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out).unwrap();
    // The key file format should reference the algorithm somewhere
    assert!(
        content.to_lowercase().contains("aes") || content.to_lowercase().contains("256"),
        "key file must reference the chosen algorithm: {}", &content[..200.min(content.len())]
    );
}

#[test]
fn key_generate_password_mode_exits_zero() {
    let out = key_path("gen_password_mode.mdix.key");
    mdix()
        .args(["key", "generate", "--output", &out, "--password"])
        .assert()
        .success()
        .code(0);
}

#[test]
fn key_generate_default_algorithm_is_aes256() {
    let out = key_path("gen_default_algo.mdix.key");
    let output = mdix()
        .args(["key", "generate", "--json", "--output", &out])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let algorithm = parsed["data"]["algorithm"].as_str().unwrap_or("");
    assert!(
        algorithm.to_lowercase().contains("aes256") || algorithm.to_lowercase().contains("aes"),
        "default algorithm should be aes256, got: {}", algorithm
    );
}

// ── key validate ──────────────────────────────────────────────────────────────

#[test]
fn key_validate_valid_key_exits_zero() {
    let out = key_path("validate_valid.mdix.key");
    generate_key(&out, "aes256");

    mdix()
        .args(["key", "validate", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn key_validate_aes128_key_exits_zero() {
    let out = key_path("validate_aes128.mdix.key");
    generate_key(&out, "aes128");

    mdix()
        .args(["key", "validate", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn key_validate_chacha20_key_exits_zero() {
    let out = key_path("validate_chacha20.mdix.key");
    generate_key(&out, "chacha20");

    mdix()
        .args(["key", "validate", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn key_validate_missing_file_exits_two() {
    mdix()
        .args(["key", "validate", "nonexistent.mdix.key"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn key_validate_json_produces_valid_json() {
    let out = key_path("validate_json.mdix.key");
    generate_key(&out, "aes256");

    let output = mdix()
        .args(["key", "--json", "validate", &out])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout must be valid JSON");
    assert_eq!(parsed["success"], true);
}

// ── key info ──────────────────────────────────────────────────────────────────

#[test]
fn key_info_exits_zero() {
    let out = key_path("info_test.mdix.key");
    generate_key(&out, "aes256");

    mdix()
        .args(["key", "info", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn key_info_shows_algorithm() {
    let out = key_path("info_shows_algo.mdix.key");
    generate_key(&out, "aes256");

    mdix()
        .args(["key", "info", &out])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("aes256")
                .or(predicate::str::contains("aes-256"))
                .or(predicate::str::contains("AES")),
        );
}

#[test]
fn key_info_shows_key_length() {
    let out = key_path("info_shows_length.mdix.key");
    generate_key(&out, "aes256");

    mdix()
        .args(["key", "info", &out])
        .assert()
        .success()
        .stdout(predicate::str::contains("32")); // 32 bytes for aes256
}

#[test]
fn key_info_missing_file_exits_two() {
    mdix()
        .args(["key", "info", "nonexistent.mdix.key"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn key_info_json_produces_valid_json() {
    let out = key_path("info_json.mdix.key");
    generate_key(&out, "aes256");

    let output = mdix()
        .args(["key", "info", "--json", &out])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout must be valid JSON");

    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["algorithm"].is_string(), "info JSON must have 'algorithm'");
    assert!(parsed["data"]["key_length"].is_number(),  "info JSON must have 'key_length'");
    assert!(parsed["data"]["mode"].is_string(),         "info JSON must have 'mode'");
    assert!(parsed["data"]["created"].is_string(),      "info JSON must have 'created'");

    let result = helpers::results_file("key", "key_info.json");
    std::fs::write(result, &stdout).ok();
}

#[test]
fn key_info_json_key_length_matches_algorithm() {
    let out = key_path("info_json_aes128_len.mdix.key");
    generate_key(&out, "aes128");

    let output = mdix()
        .args(["key", "info", "--json", &out])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let key_length = parsed["data"]["key_length"].as_u64().unwrap_or(0);
    assert_eq!(key_length, 16, "aes128 key must be 16 bytes, got {}", key_length);
}

#[test]
fn key_info_json_password_mode_shows_correct_mode() {
    let out = key_path("info_password_mode.mdix.key");
    mdix()
        .args(["key", "generate", "--output", &out, "--password"])
        .assert()
        .success();

    let output = mdix()
        .args(["key", "info", "--json", &out])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let mode = parsed["data"]["mode"].as_str().unwrap_or("");
    assert_eq!(mode, "password", "password-mode key must report mode = 'password'");
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn key_generate_quiet_suppresses_stdout() {
    let out = key_path("quiet_gen.mdix.key");
    mdix()
        .args(["key", "generate", "--quiet", "--output", &out])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn key_validate_quiet_suppresses_stdout() {
    let out = key_path("quiet_validate.mdix.key");
    generate_key(&out, "aes256");

    mdix()
        .args(["key", "--quiet", "validate", &out])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}
