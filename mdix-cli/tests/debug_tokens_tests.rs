mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

// ── Success cases ────────────────────────────────────────────────────────────

#[test]
fn debug_tokens_basic_exits_zero() {
    mdix()
        .args(["debug-tokens", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn debug_tokens_with_enums_exits_zero() {
    mdix()
        .args(["debug-tokens", &helpers::fixture("with_enums.mdix")])
        .assert()
        .success()
        .code(0);
}

#[test]
fn debug_tokens_shows_config_section_token() {
    // debug-tokens' whole design intent (see its own module doc comment)
    // is Approach B: tokenize the *full* source exactly once, so @CONFIG
    // tokens ARE in the stream with a real SectionId::Config stamp --
    // unlike debug-ast/the parser proper, which splits @CONFIG out and
    // processes it separately. That's what makes debug-tokens useful for
    // diagnosing tokenizer-level bugs (e.g. hover/folding) independent of
    // anything downstream -- so basic.mdix's explicit @CONFIG block must
    // show up as a real token, not be silently stripped.
    mdix()
        .args(["debug-tokens", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("SectionConfig"));
}

#[test]
fn debug_tokens_shows_data_section_tag() {
    mdix()
        .args(["debug-tokens", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::contains("DATA"));
}

#[test]
fn debug_tokens_by_line_exits_zero() {
    mdix()
        .args(["debug-tokens", &helpers::fixture("basic.mdix"), "--by-line"])
        .assert()
        .success();
}

#[test]
fn debug_tokens_sections_false_still_exits_zero() {
    mdix()
        .args(["debug-tokens", &helpers::fixture("basic.mdix"), "--sections", "false"])
        .assert()
        .success();
}

#[test]
fn debug_tokens_writes_to_output_file() {
    let out = helpers::results_file("debug_tokens", "basic_tokens.txt");
    mdix()
        .args(["debug-tokens", &helpers::fixture("basic.mdix"), "-o", &out])
        .assert()
        .success();

    assert!(std::path::Path::new(&out).exists(), "-o should write the token dump to a file");
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(!content.is_empty(), "written token dump should be non-empty");
}

#[test]
fn debug_tokens_invalid_syntax_still_exits_zero() {
    // Same Approach-B lenient philosophy as compile: tokenizing runs before
    // @CONFIG-driven strict parsing, so malformed syntax still produces a
    // token stream rather than hard-failing the command.
    mdix()
        .args(["debug-tokens", &helpers::fixture("invalid_syntax.mdix")])
        .assert()
        .success()
        .code(0);
}

// ── Failure cases ─────────────────────────────────────────────────────────────

#[test]
fn debug_tokens_missing_file_exits_two() {
    mdix()
        .args(["debug-tokens", "does_not_exist.mdix"])
        .assert()
        .failure()
        .code(2);
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn debug_tokens_quiet_suppresses_stdout() {
    mdix()
        .args(["debug-tokens", "--quiet", &helpers::fixture("basic.mdix")])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    }
