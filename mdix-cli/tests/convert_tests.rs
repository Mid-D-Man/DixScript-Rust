
mod helpers;

use assert_cmd::Command;

fn mdix() -> Command {
    #[allow(deprecated)]
    Command::cargo_bin("mdix").unwrap()
}

// ── mdix → json ───────────────────────────────────────────────────────────────

#[test]
fn convert_mdix_to_json_exits_zero() {
    let out = helpers::results_file("convert", "basic_to_json.json");
    mdix()
        .args([
            "convert",
            &helpers::fixture("basic.mdix"),
            "--to", "json",
            "-o", &out,
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn convert_mdix_to_json_produces_valid_json() {
    let out_path = helpers::results_file("convert", "basic_valid_json.json");

    mdix()
        .args([
            "convert",
            &helpers::fixture("basic.mdix"),
            "--to", "json",
            "-o", &out_path,
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content)
        .expect("output should be valid JSON");
    assert!(parsed.is_object());
}

#[test]
fn convert_mdix_to_json_contains_expected_keys() {
    let out_path = helpers::results_file("convert", "basic_keys.json");

    mdix()
        .args([
            "convert",
            &helpers::fixture("basic.mdix"),
            "--to", "json",
            "-o", &out_path,
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(
        content.contains("app_name") || content.contains("port"),
        "converted JSON should contain data keys from basic.mdix"
    );
}

#[test]
fn convert_with_enums_to_json_exits_zero() {
    let out = helpers::results_file("convert", "with_enums.json");
    mdix()
        .args([
            "convert",
            &helpers::fixture("with_enums.mdix"),
            "--to", "json",
            "-o", &out,
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn convert_with_functions_to_json_exits_zero() {
    let out = helpers::results_file("convert", "with_functions.json");
    mdix()
        .args([
            "convert",
            &helpers::fixture("with_functions.mdix"),
            "--to", "json",
            "-o", &out,
        ])
        .assert()
        .success()
        .code(0);
}

// ── mdix → toml ───────────────────────────────────────────────────────────────

#[test]
fn convert_mdix_to_toml_exits_zero() {
    let out = helpers::results_file("convert", "basic.toml");
    mdix()
        .args([
            "convert",
            &helpers::fixture("basic.mdix"),
            "--to", "toml",
            "-o", &out,
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn convert_mdix_to_toml_produces_valid_toml() {
    let out_path = helpers::results_file("convert", "basic_valid.toml");

    mdix()
        .args([
            "convert",
            &helpers::fixture("basic.mdix"),
            "--to", "toml",
            "-o", &out_path,
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(!content.trim().is_empty(), "toml output should not be empty");
    assert!(
        !content.trim_start().starts_with('{'),
        "toml output should not start with '{{'"
    );
}

// ── json → mdix ───────────────────────────────────────────────────────────────

#[test]
fn convert_json_to_mdix_exits_zero() {
    let json_path = helpers::results_file("convert", "roundtrip_input.json");
    mdix()
        .args([
            "convert",
            &helpers::fixture("basic.mdix"),
            "--to", "json",
            "-o", &json_path,
        ])
        .assert()
        .success();

    let mdix_out = helpers::results_file("convert", "roundtrip_recovered.mdix");
    mdix()
        .args([
            "convert",
            &json_path,
            "--to", "mdix",
            "-o", &mdix_out,
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn convert_json_to_mdix_via_dixscript_alias_exits_zero() {
    let json_path = helpers::results_file("convert", "alias_input.json");
    mdix()
        .args([
            "convert",
            &helpers::fixture("basic.mdix"),
            "--to", "json",
            "-o", &json_path,
        ])
        .assert()
        .success();

    let out = helpers::results_file("convert", "alias_recovered.mdix");
    mdix()
        .args([
            "convert",
            &json_path,
            "--to", "dixscript",
            "-o", &out,
        ])
        .assert()
        .success()
        .code(0);
}

#[test]
fn convert_json_to_mdix_produces_nonempty_file() {
    let json_path = helpers::results_file("convert", "nonempty_input.json");
    mdix()
        .args([
            "convert",
            &helpers::fixture("basic.mdix"),
            "--to", "json",
            "-o", &json_path,
        ])
        .assert()
        .success();

    let out_path = helpers::results_file("convert", "nonempty_recovered.mdix");
    mdix()
        .args([
            "convert",
            &json_path,
            "--to", "mdix",
            "-o", &out_path,
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(!content.trim().is_empty(), "recovered mdix should not be empty");
    assert!(content.contains("@DATA"), "recovered mdix should contain @DATA section");
}

// ── toml → mdix ───────────────────────────────────────────────────────────────

#[test]
fn convert_toml_to_mdix_exits_zero() {
    let toml_path = helpers::results_file("convert", "toml_rt_input.toml");
    mdix()
        .args([
            "convert",
            &helpers::fixture("basic.mdix"),
            "--to", "toml",
            "-o", &toml_path,
        ])
        .assert()
        .success();

    let out = helpers::results_file("convert", "toml_rt_recovered.mdix");
    mdix()
        .args([
            "convert",
            &toml_path,
            "--to", "mdix",
            "-o", &out,
        ])
        .assert()
        .success()
        .code(0);
}

// ── Unsupported / error cases ─────────────────────────────────────────────────

#[test]
fn convert_unknown_format_exits_four() {
    let out = helpers::results_file("convert", "out.xyz");
    mdix()
        .args([
            "convert",
            &helpers::fixture("basic.mdix"),
            "--to", "xyz",
            "-o", &out,
        ])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn convert_missing_file_exits_two() {
    mdix()
        .args(["convert", "ghost.mdix", "--to", "json"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn convert_same_format_exits_nonzero() {
    let out = helpers::results_file("convert", "same_format.mdix");
    mdix()
        .args([
            "convert",
            &helpers::fixture("basic.mdix"),
            "--to", "mdix",
            "-o", &out,
        ])
        .assert()
        .failure();
}

// ── JSON envelope flag ────────────────────────────────────────────────────────

#[test]
fn convert_json_flag_produces_envelope() {
    let out = helpers::results_file("convert", "envelope_output.json");

    let output = mdix()
        .args([
            "convert",
            "--json",
            &helpers::fixture("basic.mdix"),
            "--to", "json",
            "-o", &out,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["input_path"].is_string());
    assert!(parsed["data"]["output_path"].is_string());
    assert!(parsed["data"]["elapsed_ms"].is_number());

    let result = helpers::results_file("convert", "envelope_result.json");
    std::fs::write(result, &stdout).ok();
    }
