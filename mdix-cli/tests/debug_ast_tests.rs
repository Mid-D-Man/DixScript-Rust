mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

// ── Success cases ────────────────────────────────────────────────────────────

#[test]
fn debug_ast_basic_exits_zero() {
    mdix()
        .args(["debug-ast", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn debug_ast_with_enums_exits_zero() {
    mdix()
        .args(["debug-ast", &helpers::fixture("with_enums.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn debug_ast_with_functions_exits_zero() {
    mdix()
        .args(["debug-ast", &helpers::fixture("with_functions.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn debug_ast_with_enums_shows_enum_declarations() {
    // with_enums.mdix declares Environment/LogLevel/Status -- the AST dump
    // should surface at least the enum type names somewhere.
    mdix()
        .args(["debug-ast", &helpers::fixture("with_enums.mdix")])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Environment")
                .or(predicate::str::contains("Enum")),
        );
}

#[test]
fn debug_ast_positions_false_exits_zero() {
    mdix()
        .args(["debug-ast", &helpers::fixture("basic.mdix"), "--positions", "false"])
        .assert()
        .success();
}

#[test]
fn debug_ast_enhanced_false_exits_zero() {
    // Skips semantic analysis/enhancement -- should still print a raw AST.
    mdix()
        .args(["debug-ast", &helpers::fixture("basic.mdix"), "--enhanced", "false"])
        .assert()
        .success();
}

#[test]
fn debug_ast_writes_to_output_file() {
    let out = helpers::results_file("debug_ast", "basic_ast.txt");
    mdix()
        .args(["debug-ast", &helpers::fixture("basic.mdix"), "-o", &out])
        .assert()
        .success();

    assert!(std::path::Path::new(&out).exists());
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(!content.is_empty());
}

#[test]
fn debug_ast_invalid_syntax_still_exits_zero() {
    // Approach-B lenient mode again -- errors get collected, not hard-stopped.
    mdix()
        .args(["debug-ast", &helpers::fixture("invalid_syntax.mdix")])
        .assert()
        .success()
        .code(0);
}

// ── Failure cases ─────────────────────────────────────────────────────────────

#[test]
fn debug_ast_missing_file_exits_two() {
    mdix()
        .args(["debug-ast", "does_not_exist.mdix"])
        .assert()
        .failure()
        .code(2);
}
