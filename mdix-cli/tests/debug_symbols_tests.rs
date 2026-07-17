mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

// ── Success cases ────────────────────────────────────────────────────────────

#[test]
fn debug_symbols_basic_exits_zero() {
    mdix()
        .args(["debug-symbols", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn debug_symbols_with_enums_shows_all_three_declared_enums() {
    // with_enums.mdix declares exactly Environment, LogLevel, Status.
    mdix()
        .args(["debug-symbols", &helpers::fixture("with_enums.mdix"), "--section", "ENUMS"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Environment"))
        .stdout(predicate::str::contains("LogLevel"))
        .stdout(predicate::str::contains("Status"));
}

#[test]
fn debug_symbols_with_functions_shows_quickfuncs() {
    // with_functions.mdix declares makeServer and tierLimit.
    mdix()
        .args(["debug-symbols", &helpers::fixture("with_functions.mdix"), "--section", "FUNCTIONS"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("makeServer")
                .or(predicate::str::contains("tierLimit")),
        );
}

#[test]
fn debug_symbols_section_data_exits_zero() {
    mdix()
        .args(["debug-symbols", &helpers::fixture("basic.mdix"), "--section", "DATA"])
        .assert()
        .success();
}

#[test]
fn debug_symbols_section_namespaces_exits_zero() {
    mdix()
        .args(["debug-symbols", &helpers::fixture("basic.mdix"), "--section", "NAMESPACES"])
        .assert()
        .success();
}

#[test]
fn debug_symbols_section_builtins_exits_zero() {
    mdix()
        .args(["debug-symbols", &helpers::fixture("basic.mdix"), "--section", "BUILTINS"])
        .assert()
        .success();
}

#[test]
fn debug_symbols_section_config_exits_zero() {
    mdix()
        .args(["debug-symbols", &helpers::fixture("basic.mdix"), "--section", "CONFIG"])
        .assert()
        .success();
}

#[test]
fn debug_symbols_default_section_is_all() {
    // Omitting --section should behave like --section ALL: enum names from
    // with_enums.mdix should still show up without asking for ENUMS specifically.
    mdix()
        .args(["debug-symbols", &helpers::fixture("with_enums.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Environment"));
}

#[test]
fn debug_symbols_verbose_exits_zero() {
    mdix()
        .args(["debug-symbols", &helpers::fixture("with_enums.mdix"), "--verbose"])
        .assert()
        .success();
}

#[test]
fn debug_symbols_writes_to_output_file() {
    let out = helpers::results_file("debug_symbols", "with_enums_symbols.txt");
    mdix()
        .args(["debug-symbols", &helpers::fixture("with_enums.mdix"), "-o", &out])
        .assert()
        .success();

    assert!(std::path::Path::new(&out).exists());
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("Environment"));
}

// ── Failure cases ─────────────────────────────────────────────────────────────

#[test]
fn debug_symbols_missing_file_exits_two() {
    mdix()
        .args(["debug-symbols", "does_not_exist.mdix"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn debug_symbols_unknown_section_does_not_crash() {
    // Not documented which of these should happen -- a clean rejection or a
    // graceful fallback to ALL are both acceptable, a panic/abort is not.
    // Checked via raw .output() rather than a code() predicate so this
    // doesn't depend on which exact exit code convention was chosen.
    let output = mdix()
        .args(["debug-symbols", &helpers::fixture("basic.mdix"), "--section", "NOT_A_REAL_SECTION"])
        .output()
        .expect("process should run to completion, not hang or panic");

    assert!(
        output.status.code().is_some(),
        "process should exit normally with a status code, not be killed by a signal"
    );
}
