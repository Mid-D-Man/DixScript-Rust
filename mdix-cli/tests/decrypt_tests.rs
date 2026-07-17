mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

/// Compiles `security_01_keyfile_aes256.mdix` (real DLM fixture: keyfile-mode
/// AES-256, no password) into `results_dir("decrypt")` and returns the
/// resulting (.mdix.enc, .mdix.key) paths.
///
/// NOTE: `mdix compile`'s JSON `generated_files` field currently echoes back
/// `modules_applied` (e.g. "DEncryptor.aes256") instead of real output
/// paths -- see services/compilation.rs line ~56, `generated_files:
/// dix_data.applied_modules.clone()`. So this helper builds the expected
/// paths by the naming convention documented in the fixture's own header
/// comment (`<stem>.mdix.enc` / `<stem>.mdix.key` next to the output dir)
/// rather than trusting that JSON field. Once that's fixed, this can read
/// the real paths straight out of `generated_files` instead.
fn compile_encrypted_fixture() -> (String, String) {
    let dir = helpers::results_dir("decrypt");
    mdix()
        .args([
            "compile",
            &helpers::fixture("security_01_keyfile_aes256.mdix"),
            "-o", dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let enc = dir.join("security_01_keyfile_aes256.mdix.enc");
    let key = dir.join("security_01_keyfile_aes256.mdix.key");
    (enc.to_string_lossy().to_string(), key.to_string_lossy().to_string())
}

// ── Setup sanity ─────────────────────────────────────────────────────────────

#[test]
fn compiling_the_keyfile_fixture_produces_enc_and_key_files() {
    let (enc, key) = compile_encrypted_fixture();
    assert!(Path::new(&enc).exists(), "compile should produce a .mdix.enc file: {enc}");
    assert!(Path::new(&key).exists(), "compile should produce a .mdix.key file: {key}");
}

// ── Success cases ────────────────────────────────────────────────────────────

#[test]
fn decrypt_with_explicit_key_exits_zero() {
    let (enc, key) = compile_encrypted_fixture();
    mdix()
        .args(["decrypt", &enc, "--key", &key])
        .assert()
        .success()
        .code(0);
}

#[test]
fn decrypt_auto_detects_key_next_to_enc_file() {
    // No --key given -- compilation.rs's auto-detect strips ".mdix.enc"
    // and looks for "<stem>.mdix.key" in the same directory, which is
    // exactly what compile_encrypted_fixture() just produced.
    let (enc, _key) = compile_encrypted_fixture();
    mdix()
        .args(["decrypt", &enc])
        .assert()
        .success()
        .code(0);
}

#[test]
fn decrypt_writes_plaintext_mdix_output() {
    let (enc, key) = compile_encrypted_fixture();
    let out_dir = helpers::results_dir("decrypt");
    mdix()
        .args(["decrypt", &enc, "--key", &key, "-o", out_dir.to_str().unwrap()])
        .assert()
        .success();

    let plaintext = out_dir.join("security_01_keyfile_aes256.mdix");
    assert!(
        plaintext.exists(),
        "decrypt should write the plaintext .mdix (stripped of .enc) to the output dir: {}",
        plaintext.display()
    );
}

#[test]
fn decrypted_output_preserves_data() {
    let (enc, key) = compile_encrypted_fixture();
    let out_dir = helpers::results_dir("decrypt");
    mdix()
        .args(["decrypt", &enc, "--key", &key, "-o", out_dir.to_str().unwrap()])
        .assert()
        .success();

    let plaintext_path = out_dir.join("security_01_keyfile_aes256.mdix");
    let content = std::fs::read_to_string(&plaintext_path).unwrap();

    // Spot-check a handful of the fixture's actual @DATA values survived
    // the encrypt -> decrypt round trip.
    assert!(content.contains("SecureVault"), "app_name should survive decryption");
    assert!(content.contains("db.prod.internal"), "nested database.host should survive decryption");

    // The fixture's real enums, per @ENUMS -- confirms enum identity
    // (not just the raw resolved int) survives the DLM round trip too.
    assert!(content.contains("Environment"), "enum declarations should survive decryption");
}

// ── Failure cases ─────────────────────────────────────────────────────────────

#[test]
fn decrypt_missing_enc_file_exits_two() {
    mdix()
        .args(["decrypt", "does_not_exist.mdix.enc"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn decrypt_without_key_and_none_discoverable_fails() {
    // Copy just the .enc file into an otherwise-empty dir so auto-detect
    // has nothing to find.
    let (enc, _key) = compile_encrypted_fixture();
    let isolated_dir = helpers::results_dir("decrypt_isolated");
    let isolated_enc = isolated_dir.join("orphaned.mdix.enc");
    std::fs::copy(&enc, &isolated_enc).unwrap();

    mdix()
        .args(["decrypt", isolated_enc.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn decrypt_with_wrong_key_fails() {
    let (enc, _key) = compile_encrypted_fixture();

    // Generate an unrelated key of the same algorithm -- wrong key
    // material, should not decrypt data encrypted under a different key.
    let wrong_key = helpers::results_file("decrypt", "wrong.mdix.key");
    mdix()
        .args(["key", "generate", "--output", &wrong_key, "--algorithm", "aes256"])
        .assert()
        .success();

    mdix()
        .args(["decrypt", &enc, "--key", &wrong_key])
        .assert()
        .failure();
}

// ── JSON output ───────────────────────────────────────────────────────────────

#[test]
fn decrypt_json_flag_produces_valid_json() {
    let (enc, key) = compile_encrypted_fixture();
    let output = mdix()
        .args(["decrypt", "--json", &enc, "--key", &key])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["output_path"].is_string());

    let result_file = helpers::results_file("decrypt", "decrypt_result.json");
    std::fs::write(result_file, &stdout).ok();
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn decrypt_quiet_suppresses_stdout() {
    let (enc, key) = compile_encrypted_fixture();
    mdix()
        .args(["decrypt", "--quiet", &enc, "--key", &key])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}
