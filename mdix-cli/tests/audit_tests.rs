mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

/// Stage a private copy of `fixture_name` inside a directory unique to
/// `test_name` and return that directory.
///
/// `Compiler/DLM/Auditor/auditor_utilities.rs`'s
/// `AuditorPathUtils::resolve_audit_file_path` intentionally keeps
/// `.mdix.au` next to the *source* file being compiled, not in whatever
/// `-o` directory a given run happens to pass -- that's what lets the
/// audit trail for one source file stay continuous across multiple
/// compiles to different output locations. That means every test here
/// needs its own private copy of the fixture (not the one shared from
/// `tests/fixtures/`) so its audit file can never collide with another
/// test's, whether or not `cargo test` happens to run them concurrently.
fn staged_test_dir(test_name: &str, fixture_name: &str) -> PathBuf {
    let dir = helpers::results_dir(&format!("audit/{test_name}"));
    let source = dir.join(fixture_name);
    std::fs::copy(helpers::fixture(fixture_name), &source).unwrap_or_else(|e| {
        panic!("failed to stage fixture '{fixture_name}' for test '{test_name}': {e}")
    });
    dir
}

/// Compiles the given DLM-auditor fixture (staged into its own per-test
/// directory first -- see `staged_test_dir`) and returns the resulting
/// `.mdix.au` path.
///
/// `test_name` must be unique per test -- pass the test function's own
/// name -- so each test gets a private staged source and therefore a
/// private, collision-free audit file. `-o` is pointed at the same
/// directory as the staged source, so "source dir" (where
/// `AuditorPathUtils` actually writes `.mdix.au`) and "`-o` dir" line up,
/// which is also why the naming-convention path below
/// (`dir.join(fixture_name + ".au")`) matches reality -- see
/// `staged_test_dir`.
///
/// Same caveat as decrypt_tests.rs's compile_encrypted_fixture(): compile's
/// JSON `generated_files` currently echoes `modules_applied` rather than
/// real paths (services/compilation.rs ~line 56), so this builds the
/// expected `.au` path by naming convention instead of trusting that
/// field.
fn compile_audited_fixture(test_name: &str, fixture_name: &str) -> String {
    let dir = staged_test_dir(test_name, fixture_name);
    let source = dir.join(fixture_name);

    mdix()
        .args(["compile", source.to_str().unwrap(), "-o", dir.to_str().unwrap()])
        .assert()
        .success();

    dir.join(format!("{fixture_name}.au")).to_string_lossy().to_string()
}

// ── Setup sanity ─────────────────────────────────────────────────────────────

#[test]
fn compiling_the_diy_audit_fixture_produces_an_au_file() {
    let au = compile_audited_fixture(
        "compiling_the_diy_audit_fixture_produces_an_au_file",
        "07_diy_audit.mdix",
    );
    assert!(Path::new(&au).exists(), "compile should produce a .mdix.au file: {au}");
}

#[test]
fn compiling_the_enhanced_audit_fixture_produces_an_au_file() {
    let au = compile_audited_fixture(
        "compiling_the_enhanced_audit_fixture_produces_an_au_file",
        "08_enhanced_audit.mdix",
    );
    assert!(Path::new(&au).exists(), "compile should produce a .mdix.au file: {au}");
}

#[test]
fn compiling_twice_appends_a_second_entry_not_a_second_file() {
    // AuditFileManager::append_entry -- recompiling the same source should
    // grow the entry count in the same .mdix.au, not silently overwrite it
    // or spawn a second file.
    let test_name = "compiling_twice_appends_a_second_entry_not_a_second_file";
    let au = compile_audited_fixture(test_name, "07_diy_audit.mdix");
    let dir = helpers::results_dir(&format!("audit/{test_name}"));
    let source = dir.join("07_diy_audit.mdix");
    mdix()
        .args(["compile", source.to_str().unwrap(), "-o", dir.to_str().unwrap()])
        .assert()
        .success();

    let output = mdix().args(["audit", "info", "--json", &au]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let entry_count = parsed["data"]["entry_count"].as_u64().unwrap();
    assert!(entry_count >= 2, "recompiling should append, not overwrite -- got {entry_count} entr(y/ies)");
}

// ── mdix audit info ──────────────────────────────────────────────────────────

#[test]
fn audit_info_exits_zero() {
    let au = compile_audited_fixture("audit_info_exits_zero", "07_diy_audit.mdix");
    mdix().args(["audit", "info", &au]).assert().success().code(0);
}

#[test]
fn audit_info_shows_source_file() {
    let au = compile_audited_fixture("audit_info_shows_source_file", "07_diy_audit.mdix");
    mdix()
        .args(["audit", "info", &au])
        .assert()
        .success()
        .stdout(predicate::str::contains("07_diy_audit"));
}

#[test]
fn audit_info_json_has_expected_fields() {
    let au = compile_audited_fixture("audit_info_json_has_expected_fields", "08_enhanced_audit.mdix");
    let output = mdix().args(["audit", "info", "--json", &au]).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");

    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["source_file"].is_string());
    assert!(parsed["data"]["format"].is_string());
    assert!(parsed["data"]["entry_count"].is_number());
    assert!(parsed["data"]["max_entries"].is_number());
    assert!(parsed["data"]["created"].is_string());

    let result_file = helpers::results_file("audit", "enhanced_info.json");
    std::fs::write(result_file, &stdout).ok();
}

#[test]
fn audit_info_missing_file_exits_two() {
    mdix()
        .args(["audit", "info", "does_not_exist.mdix.au"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn audit_info_on_non_au_file_fails_with_parse_error_not_a_panic() {
    // Point it at a real, existing .mdix file (not a .au file) -- should
    // fail gracefully as a malformed-audit-file error, not crash.
    let bad_path = helpers::fixture("basic.mdix");
    mdix()
        .args(["audit", "info", &bad_path])
        .assert()
        .failure()
        .code(1); // CliError::ParseError falls into the generic `_ => 1` arm
}

// ── mdix audit view ──────────────────────────────────────────────────────────

#[test]
fn audit_view_exits_zero() {
    let au = compile_audited_fixture("audit_view_exits_zero", "07_diy_audit.mdix");
    mdix().args(["audit", "view", &au]).assert().success().code(0);
}

#[test]
fn audit_view_shows_at_least_one_entry() {
    let au = compile_audited_fixture("audit_view_shows_at_least_one_entry", "07_diy_audit.mdix");
    mdix()
        .args(["audit", "view", &au])
        .assert()
        .success()
        .stdout(predicate::str::contains("#0"));
}

#[test]
fn audit_view_enhanced_shows_success_status() {
    let au = compile_audited_fixture("audit_view_enhanced_shows_success_status", "08_enhanced_audit.mdix");
    mdix()
        .args(["audit", "view", &au])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("SUCCESS")
                .or(predicate::str::contains("success")),
        );
}

#[test]
fn audit_view_json_entries_have_expected_shape() {
    let au = compile_audited_fixture("audit_view_json_entries_have_expected_shape", "07_diy_audit.mdix");
    let output = mdix().args(["audit", "view", "--json", &au]).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");

    let entries = parsed["data"].as_array().expect("data should be a JSON array of entries");
    assert!(!entries.is_empty());

    let first = &entries[0];
    assert!(first["index"].is_number());
    assert!(first["compilation_id"].is_string());
    assert!(first["timestamp"].is_string());
    assert!(first["status"].is_string());
    assert!(first["modules_executed"].is_array());
    assert!(first["execution_time_ms"].is_number());
    assert!(first["source_checksum"].is_string());
}

#[test]
fn audit_view_tail_limits_entries() {
    // Compile three times, then --tail 2 should return exactly 2 entries
    // even though 3 were recorded.
    let dir = staged_test_dir("audit_view_tail_limits_entries", "07_diy_audit.mdix");
    let source = dir.join("07_diy_audit.mdix");
    for _ in 0..3 {
        mdix()
            .args(["compile", source.to_str().unwrap(), "-o", dir.to_str().unwrap()])
            .assert()
            .success();
    }
    let au = dir.join("07_diy_audit.mdix.au");

    let output = mdix()
        .args(["audit", "view", "--json", au.to_str().unwrap(), "--tail", "2"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let entries = parsed["data"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "--tail 2 should return exactly 2 entries");
}

#[test]
fn audit_view_missing_file_exits_two() {
    mdix()
        .args(["audit", "view", "does_not_exist.mdix.au"])
        .assert()
        .failure()
        .code(2);
}

// ── mdix audit archives ──────────────────────────────────────────────────────

#[test]
fn audit_archives_on_a_fresh_file_reports_none() {
    let au = compile_audited_fixture("audit_archives_on_a_fresh_file_reports_none", "07_diy_audit.mdix");
    mdix()
        .args(["audit", "archives", &au])
        .assert()
        .success()
        .stdout(predicate::str::contains("No rotated archive"));
}

#[test]
fn audit_archives_json_is_an_array() {
    let au = compile_audited_fixture("audit_archives_json_is_an_array", "07_diy_audit.mdix");
    let output = mdix().args(["audit", "archives", "--json", &au]).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed["data"]["archives"].is_array());
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn audit_info_quiet_suppresses_stdout() {
    let au = compile_audited_fixture("audit_info_quiet_suppresses_stdout", "07_diy_audit.mdix");
    mdix()
        .args(["audit", "--quiet", "info", &au])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}
