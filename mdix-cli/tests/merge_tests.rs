mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

fn out(name: &str) -> String {
    helpers::results_file("merge", name)
}

// ── Success cases ───────────────────────────────────────────────────────────

#[test]
fn merge_two_files_exits_zero() {
    let output = out("basic_plus_enums.merged.mdix");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            "-o", &output,
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn merge_writes_output_file() {
    let output = out("writes_output.merged.mdix");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            "-o", &output,
        ])
        .assert()
        .success();

    assert!(
        std::path::Path::new(&output).exists(),
        "merge must write the merged file to the given --output path"
    );
}

#[test]
fn merge_three_files_exits_zero() {
    let output = out("three_way.merged.mdix");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            &helpers::fixture("with_functions.mdix"),
            "-o", &output,
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn merge_requires_at_least_two_files() {
    // clap's num_args = 2.. should reject a single file before any merge
    // logic runs -- this is a usage error, not a runtime CliError, so it's
    // clap's own exit code (2), not CliError::InvalidArgument's (3).
    mdix()
        .args(["merge", &helpers::fixture("basic.mdix")])
        .assert()
        .failure();
}

// ── Strategy flag ────────────────────────────────────────────────────────────

#[test]
fn merge_strategy_primary_exits_zero() {
    let output = out("strategy_primary.merged.mdix");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            "-o", &output,
            "--strategy", "primary",
        ])
        .assert()
        .success();
}

#[test]
fn merge_strategy_secondary_exits_zero() {
    let output = out("strategy_secondary.merged.mdix");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            "-o", &output,
            "--strategy", "secondary",
        ])
        .assert()
        .success();
}

#[test]
fn merge_strategy_weighted_with_explicit_weights_exits_zero() {
    let output = out("strategy_weighted.merged.mdix");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            "-o", &output,
            "--strategy", "weighted",
            "--weights", "1.0,0.5",
        ])
        .assert()
        .success();
}

#[test]
fn merge_strategy_throw_on_genuinely_conflicting_files_fails() {
    // basic.mdix and with_enums.mdix both declare `app_name` and `port`
    // with different values -- a real conflict, which "throw" should
    // refuse to resolve.
    let output = out("strategy_throw.merged.mdix");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            "-o", &output,
            "--strategy", "throw",
        ])
        .assert()
        .failure();
}

#[test]
fn merge_unknown_strategy_is_rejected() {
    let output = out("strategy_unknown.merged.mdix");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            "-o", &output,
            "--strategy", "not-a-real-strategy",
        ])
        .assert()
        .failure();
}

// ── array-strategy flag ──────────────────────────────────────────────────────

#[test]
fn merge_array_strategy_concat_exits_zero() {
    let output = out("array_concat.merged.mdix");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_functions.mdix"),
            "-o", &output,
            "--array-strategy", "concat",
        ])
        .assert()
        .success();
}

#[test]
fn merge_array_strategy_replace_exits_zero() {
    let output = out("array_replace.merged.mdix");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_functions.mdix"),
            "-o", &output,
            "--array-strategy", "replace",
        ])
        .assert()
        .success();
}

// ── Output format ────────────────────────────────────────────────────────────

#[test]
fn merge_to_json_produces_parseable_json_file() {
    let output = out("to_format.merged.json");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            "-o", &output,
            "--to", "json",
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&output).unwrap();
    let _: serde_json::Value = serde_json::from_str(&content)
        .expect("--to json output must be parseable JSON");
}

#[test]
fn merge_output_extension_infers_format_without_to_flag() {
    let output = out("infer_from_ext.merged.json");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            "-o", &output,
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&output).unwrap();
    let _: serde_json::Value = serde_json::from_str(&content)
        .expect(".json output extension should infer JSON format even without --to");
}

// ── Failure cases ─────────────────────────────────────────────────────────────

#[test]
fn merge_missing_file_exits_two() {
    let output = out("missing_file.merged.mdix");
    mdix()
        .args([
            "merge",
            &helpers::fixture("basic.mdix"),
            "does_not_exist.mdix",
            "-o", &output,
        ])
        .assert()
        .failure()
        .code(2);
}

// ── JSON output ───────────────────────────────────────────────────────────────

#[test]
fn merge_json_flag_produces_valid_json() {
    let output = out("json_flag.merged.mdix");
    let cmd_output = mdix()
        .args([
            "merge", "--json",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_functions.mdix"),
            "-o", &output,
        ])
        .output()
        .unwrap();

    assert!(cmd_output.status.success());
    let stdout = String::from_utf8(cmd_output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");
    assert_eq!(parsed["success"], true);

    let result_file = helpers::results_file("merge", "basic_plus_functions.json");
    std::fs::write(result_file, &stdout).ok();
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn merge_quiet_suppresses_stdout() {
    let output = out("quiet.merged.mdix");
    mdix()
        .args([
            "merge", "--quiet",
            &helpers::fixture("basic.mdix"),
            &helpers::fixture("with_enums.mdix"),
            "-o", &output,
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}
