mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

// ── Success cases ────────────────────────────────────────────────────────────

#[test]
fn diff_two_files_exits_zero() {
    mdix()
        .args(["diff", &helpers::fixture("basic.mdix"), &helpers::fixture("with_enums.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn diff_identical_file_against_itself_reports_no_conflicts() {
    let f = helpers::fixture("basic.mdix");
    mdix()
        .args(["diff", &f, &f])
        .assert()
        .success()
        .stdout(predicate::str::contains("No conflicts"));
}

#[test]
fn diff_genuinely_conflicting_files_reports_conflicts() {
    // basic.mdix and with_enums.mdix both declare app_name/port with
    // different values -- real, detectable conflicts (basic.mdix also has
    // an explicit CONFIG.author with no counterpart in with_enums.mdix,
    // which resolves to a third, equally real conflict).
    //
    // The summary header/count/trailer print via printer::section/info
    // (stdout), but each individual conflict line prints via
    // printer::warning, which is stderr by design
    // (mdix-cli/src/output/printer.rs's `eprintln!`) -- so the actual
    // conflicting key only shows up in stderr, not stdout.
    mdix()
        .args(["diff", &helpers::fixture("basic.mdix"), &helpers::fixture("with_enums.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("potential conflict"))
        .stderr(predicate::str::contains("app_name"));
}

#[test]
fn diff_three_files_exits_zero() {
    mdix()
        .args([
            "diff",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            &helpers::fixture("with_functions.mdix"),
        ])
        .assert()
        .success();
}

#[test]
fn diff_with_labels_exits_zero() {
    mdix()
        .args([
            "diff",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            "--labels", "base,enums-variant",
        ])
        .assert()
        .success();
}

#[test]
fn diff_does_not_write_any_output_file() {
    // The whole point of diff vs merge: preview only, nothing written to disk.
    let dir = helpers::results_dir("diff_no_write_check");
    let before: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();

    mdix()
        .args(["diff", &helpers::fixture("basic.mdix"), &helpers::fixture("with_enums.mdix")])
        .current_dir(&dir)
        .assert()
        .success();

    let after: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
    assert_eq!(before.len(), after.len(), "diff must not write any files to the working directory");
}

// ── --fail-on-conflict ───────────────────────────────────────────────────────

#[test]
fn diff_fail_on_conflict_exits_nonzero_when_conflicts_exist() {
    mdix()
        .args([
            "diff",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            "--fail-on-conflict",
        ])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn diff_fail_on_conflict_exits_zero_when_clean() {
    let f = helpers::fixture("basic.mdix");
    mdix()
        .args(["diff", &f, &f, "--fail-on-conflict"])
        .assert()
        .success()
        .code(0);
}

// ── Failure cases ─────────────────────────────────────────────────────────────

#[test]
fn diff_requires_at_least_two_files() {
    mdix()
        .args(["diff", &helpers::fixture("basic.mdix")])
        .assert()
        .failure();
}

#[test]
fn diff_missing_file_exits_two() {
    mdix()
        .args(["diff", &helpers::fixture("basic.mdix"), "does_not_exist.mdix"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn diff_labels_count_mismatch_fails() {
    mdix()
        .args([
            "diff",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            &helpers::fixture("with_functions.mdix"),
            "--labels", "only,two",
        ])
        .assert()
        .failure();
}

// ── JSON output ───────────────────────────────────────────────────────────────

#[test]
fn diff_json_flag_produces_valid_json() {
    let output = mdix()
        .args(["diff", "--json", &helpers::fixture("basic.mdix"), &helpers::fixture("with_enums.mdix")])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["conflict_count"].is_number());
    assert!(parsed["data"]["conflicts"].is_array());
    assert!(parsed["data"]["input_paths"].is_array());

    let result_file = helpers::results_file("diff", "basic_vs_enums.json");
    std::fs::write(result_file, &stdout).ok();
}

#[test]
fn diff_json_conflict_count_matches_conflicts_array_length() {
    let output = mdix()
        .args(["diff", "--json", &helpers::fixture("basic.mdix"), &helpers::fixture("with_enums.mdix")])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let count = parsed["data"]["conflict_count"].as_u64().unwrap();
    let array_len = parsed["data"]["conflicts"].as_array().unwrap().len() as u64;
    assert_eq!(count, array_len);
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn diff_quiet_suppresses_stdout() {
    mdix()
        .args(["diff", "--quiet", &helpers::fixture("basic.mdix"), &helpers::fixture("with_enums.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
        }
