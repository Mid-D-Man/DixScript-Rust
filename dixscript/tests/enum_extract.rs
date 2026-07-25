//! Byte-for-byte reproduction of `mdix_files/advanced/EnumsWithStuffImported.mdix`
//! importing `mdix_files/advanced/EnumWithCompressionAndOrEnc.mdix`, the exact
//! pair reported as still producing 0 for `mdix convert --to json/toml` even
//! after the enum-registry (Bug A) and `to_mdix` flattening fixes.
//!
//! This differs from `enum_converter_json_toml_regression.rs` in every way
//! that file *doesn't*, simultaneously:
//!   - the enum reference sits inside a `TableProperty` (`timer:\n uop = ...`),
//!     not a top-level `SimpleProperty`
//!   - it has no `<enum>` type annotation
//!   - the outer file has a `@DLM(DCompressor.gzip)` section
//!   - the imported file has its own `@DATA` (using its own LOCAL enum) and
//!     its own `@SECURITY` section with no matching `@DLM`
//!   - the outer file has no local `@ENUMS`, no `@QUICKFUNCS`, and no
//!     `@CONFIG` at all -- nothing but the import triggers the resolution
//!     gate
//!
//! The only text change from the real files is the `@IMPORTS` path, which
//! pointed at an absolute path on the reporter's machine
//! (`/Users/midman/Desktop/...`) -- swapped here for the portable
//! `CARGO_MANIFEST_DIR`-relative equivalent. Everything else, byte for byte,
//! matches what's committed in the repo.
//!
//! Run with:
//!   cargo test --test enum_exact_repro_regression -- --nocapture

use dixscript::Runtime::{DixConverter, DixLoader};

const IMPORTED_FILE: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/advanced/EnumWithCompressionAndOrEnc.mdix");

#[test]
fn exact_repro_to_json_flat() {
    let loader = DixLoader::new();
    let source = format!(
        "@IMPORTS(\n  EnumMan from \"{}\"\n)\n@DLM(\n  DCompressor.gzip\n)\n@DATA(\n seee = \"lk\"\n timer:\n  uop = EnumMan.Suka.Crack\n\n)",
        IMPORTED_FILE
    );

    let ast = loader
        .compile_to_resolved_ast_from_str(&source, "exact-repro-json")
        .expect("should compile: exact repro of EnumsWithStuffImported.mdix");

    let converter = DixConverter::new();
    let json = converter
        .to_json_flat(&ast, true)
        .expect("to_json_flat should succeed");

    println!("=== exact_repro_to_json_flat output ===\n{json}\n=======================================");

    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("converter output should be valid JSON");

    // Suka.Crack = 9 in EnumWithCompressionAndOrEnc.mdix. Checking every
    // plausible key shape (flat dotted vs actually-nested object) so this
    // fails on the VALUE being wrong, not on a wrong guess about the key.
    let found = parsed.get("timer.uop").and_then(|v| v.as_i64())
        .or_else(|| parsed.get("timer").and_then(|t| t.get("uop")).and_then(|v| v.as_i64()));

    assert_eq!(
        found,
        Some(9),
        "EnumMan.Suka.Crack should be 9, not 0 -- full JSON output:\n{json}"
    );
}

#[test]
fn exact_repro_to_toml() {
    let loader = DixLoader::new();
    let source = format!(
        "@IMPORTS(\n  EnumMan from \"{}\"\n)\n@DLM(\n  DCompressor.gzip\n)\n@DATA(\n seee = \"lk\"\n timer:\n  uop = EnumMan.Suka.Crack\n\n)",
        IMPORTED_FILE
    );

    let ast = loader
        .compile_to_resolved_ast_from_str(&source, "exact-repro-toml")
        .expect("should compile: exact repro of EnumsWithStuffImported.mdix");

    let converter = DixConverter::new();
    let toml_str = converter.to_toml(&ast).expect("to_toml should succeed");

    println!("=== exact_repro_to_toml output ===\n{toml_str}\n===================================");

    let parsed: toml::Value = toml_str.parse().expect("converter output should be valid TOML");

    let found = parsed.get("timer.uop").and_then(|v| v.as_integer())
        .or_else(|| parsed.get("timer").and_then(|t| t.get("uop")).and_then(|v| v.as_integer()));

    assert_eq!(
        found,
        Some(9),
        "EnumMan.Suka.Crack should be 9, not 0 -- full TOML output:\n{toml_str}"
    );
}

/// Same exact repro, but going through `DixData` (the `load_from_str` path)
/// instead of `DixConverter` directly -- to check whether this is specific
/// to the converter or affects the whole compile pipeline for this shape.
#[test]
fn exact_repro_via_dix_data() {
    let loader = DixLoader::new();
    let source = format!(
        "@IMPORTS(\n  EnumMan from \"{}\"\n)\n@DLM(\n  DCompressor.gzip\n)\n@DATA(\n seee = \"lk\"\n timer:\n  uop = EnumMan.Suka.Crack\n\n)",
        IMPORTED_FILE
    );

    let result = loader
        .load_from_str(&source, &dixscript::Runtime::DixLoadOptions::new())
        .expect("should compile via load_from_str: exact repro");

    match result.get_value("timer.uop") {
        Some(dixscript::Runtime::DixValue::Enum { value, .. }) => {
            assert_eq!(*value, 9, "EnumMan.Suka.Crack should be 9 via DixData too");
        }
        Some(dixscript::Runtime::DixValue::Int(v)) => {
            assert_eq!(*v, 9, "EnumMan.Suka.Crack should be 9 via DixData too");
        }
        other => panic!(
            "expected int/enum 9 for 'timer.uop' via DixData, got {:?} -- \
             if this ALSO fails, the bug is upstream of DixConverter (shared \
             with DixData); if this PASSES while the converter tests fail, \
             the bug is specific to DixConverter's own code",
            other
        ),
    }
  }
