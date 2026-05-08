// mdix-cli/tests/create_tests.rs

mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

/// Unique scratch path that won't collide between parallel tests.
fn scratch(name: &str) -> String {
    helpers::results_file("create", name)
}

// ── Basic template ────────────────────────────────────────────────────────────

#[test]
fn create_basic_template_exits_zero() {
    let out = scratch("basic_template.mdix");
    mdix()
        .args(["create", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn create_basic_template_file_exists() {
    let out = scratch("basic_exists.mdix");
    mdix().args(["create", &out]).assert().success();
    assert!(
        std::path::Path::new(&out).exists(),
        "create must produce a file at the given path"
    );
}

#[test]
fn create_basic_template_is_nonempty() {
    let out = scratch("basic_nonempty.mdix");
    mdix().args(["create", &out]).assert().success();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(!content.trim().is_empty(), "created file must not be empty");
}

#[test]
fn create_basic_template_contains_data_section() {
    let out = scratch("basic_has_data.mdix");
    mdix().args(["create", &out]).assert().success();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("@DATA"), "created file must contain @DATA section");
}

#[test]
fn create_basic_template_is_valid() {
    // After creating, run validate on the output — must pass.
    let out = scratch("basic_valid.mdix");
    mdix().args(["create", &out]).assert().success();
    mdix().args(["validate", &out]).assert().success().code(0);
}

// ── Advanced template ─────────────────────────────────────────────────────────

#[test]
fn create_advanced_template_exits_zero() {
    let out = scratch("advanced_template.mdix");
    mdix()
        .args(["create", "--template", "advanced", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn create_advanced_template_is_valid() {
    let out = scratch("advanced_valid.mdix");
    mdix()
        .args(["create", "--template", "advanced", &out])
        .assert()
        .success();
    mdix().args(["validate", &out]).assert().success().code(0);
}

#[test]
fn create_advanced_template_contains_enums() {
    let out = scratch("advanced_has_enums.mdix");
    mdix()
        .args(["create", "--template", "advanced", &out])
        .assert()
        .success();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(
        content.contains("@ENUMS"),
        "advanced template must contain @ENUMS section"
    );
}

#[test]
fn create_advanced_template_contains_quickfuncs() {
    let out = scratch("advanced_has_qf.mdix");
    mdix()
        .args(["create", "--template", "advanced", &out])
        .assert()
        .success();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(
        content.contains("@QUICKFUNCS"),
        "advanced template must contain @QUICKFUNCS section"
    );
}

// ── Security template ─────────────────────────────────────────────────────────

#[test]
fn create_security_template_exits_zero() {
    let out = scratch("security_template.mdix");
    mdix()
        .args(["create", "--template", "security", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn create_security_template_contains_dlm_and_security() {
    let out = scratch("security_has_sections.mdix");
    mdix()
        .args(["create", "--template", "security", &out])
        .assert()
        .success();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("@DLM"),      "security template must contain @DLM");
    assert!(content.contains("@SECURITY"), "security template must contain @SECURITY");
}

// ── DLM template ──────────────────────────────────────────────────────────────

#[test]
fn create_dlm_template_exits_zero() {
    let out = scratch("dlm_template.mdix");
    mdix()
        .args(["create", "--template", "dlm", &out])
        .assert()
        .success()
        .code(0);
}

#[test]
fn create_dlm_template_contains_compressor_and_encryptor() {
    let out = scratch("dlm_has_modules.mdix");
    mdix()
        .args(["create", "--template", "dlm", &out])
        .assert()
        .success();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(
        content.contains("DCompressor") || content.contains("DEncryptor"),
        "dlm template must reference at least one DLM module"
    );
}

// ── --force flag ──────────────────────────────────────────────────────────────

#[test]
fn create_existing_file_without_force_fails() {
    let out = scratch("no_force_fail.mdix");

    // Create it the first time — succeeds.
    mdix().args(["create", &out]).assert().success();

    // Try again without --force — must fail (exit 3: InvalidArgument).
    mdix()
        .args(["create", &out])
        .assert()
        .failure()
        .code(3);
}

#[test]
fn create_existing_file_without_force_shows_hint() {
    let out = scratch("no_force_hint.mdix");
    mdix().args(["create", &out]).assert().success();

    let output = mdix().args(["create", &out]).output().unwrap();
    let stderr  = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("--force") || stderr.contains("force"),
        "error output should mention --force: {}", stderr
    );
}

#[test]
fn create_with_force_overwrites_existing() {
    let out = scratch("force_overwrite.mdix");
    mdix().args(["create", &out]).assert().success();

    // Write garbage into the file to ensure it really is overwritten.
    std::fs::write(&out, "not a valid mdix file").unwrap();

    mdix()
        .args(["create", "--force", &out])
        .assert()
        .success()
        .code(0);

    // The file should now be a valid template again.
    mdix().args(["validate", &out]).assert().success().code(0);
}

// ── Unknown template ──────────────────────────────────────────────────────────

#[test]
fn create_unknown_template_exits_nonzero() {
    let out = scratch("unknown_template.mdix");
    mdix()
        .args(["create", "--template", "notexist", &out])
        .assert()
        .failure();
}

#[test]
fn create_unknown_template_does_not_create_file() {
    let out = scratch("unknown_no_file.mdix");
    // Make sure it doesn't exist before the test.
    let _ = std::fs::remove_file(&out);

    mdix()
        .args(["create", "--template", "notexist", &out])
        .assert()
        .failure();

    assert!(
        !std::path::Path::new(&out).exists(),
        "no file must be created for an unknown template"
    );
}

// ── JSON output ───────────────────────────────────────────────────────────────

#[test]
fn create_json_flag_produces_valid_json() {
    let out = scratch("json_output.mdix");
    let output = mdix()
        .args(["create", "--json", &out])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout must be valid JSON");

    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["file_path"].is_string());
    assert!(parsed["data"]["template"].is_string());
    assert_eq!(parsed["data"]["template"], "basic");

    let result = helpers::results_file("create", "create_json_result.json");
    std::fs::write(result, &stdout).ok();
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn create_quiet_suppresses_stdout_on_success() {
    let out = scratch("quiet_create.mdix");
    mdix()
        .args(["create", "--quiet", &out])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
  }
