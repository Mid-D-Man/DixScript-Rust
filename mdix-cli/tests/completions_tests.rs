mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;

fn mdix() -> Command {
    Command::cargo_bin("mdix").unwrap()
}

// ── Each supported shell ─────────────────────────────────────────────────────

#[test]
fn completions_bash_exits_zero() {
    mdix().args(["completions", "bash"]).assert().success().code(0);
}

#[test]
fn completions_zsh_exits_zero() {
    mdix().args(["completions", "zsh"]).assert().success().code(0);
}

#[test]
fn completions_fish_exits_zero() {
    mdix().args(["completions", "fish"]).assert().success().code(0);
}

#[test]
fn completions_powershell_exits_zero() {
    mdix().args(["completions", "powershell"]).assert().success().code(0);
}

#[test]
fn completions_elvish_exits_zero() {
    mdix().args(["completions", "elvish"]).assert().success().code(0);
}

// ── Content sanity ───────────────────────────────────────────────────────────

#[test]
fn completions_bash_output_is_nonempty() {
    let output = mdix().args(["completions", "bash"]).output().unwrap();
    assert!(!output.stdout.is_empty(), "bash completion script should not be empty");
}

#[test]
fn completions_bash_mentions_the_binary_name() {
    mdix()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mdix"));
}

#[test]
fn completions_bash_mentions_top_level_subcommands() {
    // Spot-check a couple of real subcommand names show up in the
    // generated completion script.
    mdix()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("compile"))
        .stdout(predicate::str::contains("validate"));
}

#[test]
fn completions_mentions_the_new_diff_and_audit_commands() {
    // Regression guard: completions are generated from the live Cli struct,
    // so newly-added subcommands should appear automatically. This is
    // really a check that we didn't forget to register them in the
    // Commands enum, not a completions-specific behavior.
    mdix()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("diff"))
        .stdout(predicate::str::contains("audit"))
        .stdout(predicate::str::contains("completions"));
}

// ── Failure cases ─────────────────────────────────────────────────────────────

#[test]
fn completions_unknown_shell_is_rejected() {
    // "powersh" isn't a valid clap_complete::Shell variant -- clap's own
    // value_enum validation should reject it before run() is ever called.
    mdix()
        .args(["completions", "powersh"])
        .assert()
        .failure();
}

#[test]
fn completions_missing_shell_argument_is_rejected() {
    mdix().args(["completions"]).assert().failure();
}

// ── Quiet flag ────────────────────────────────────────────────────────────────

#[test]
fn completions_quiet_does_not_suppress_the_script_itself() {
    // Unlike most commands, the completion script IS the primary output,
    // not an informational side message -- --quiet suppressing it would
    // make the command useless. This pins down that expectation
    // explicitly rather than leaving it to guesswork.
    let output = mdix().args(["completions", "--quiet", "bash"]).output().unwrap();
    assert!(
        !output.stdout.is_empty(),
        "--quiet must not swallow the completion script itself, only informational messages"
    );
}
