//! Coverage test: makes sure `DixConverter::to_json_flat` and
//! `DixConverter::to_toml` -- the two functions behind `mdix convert --to
//! json|toml` -- correctly resolve an IMPORTED enum literal (`Namespace.Enum.FIELD`
//! written directly in `@DATA`) to its real integer value, not 0.
//!
//! `enum_resolution_regression.rs` already covers this exact scenario for
//! `DixData::from_ast` (the `DixLoader::load_from_str`/`load_text` path).
//! Both paths share the same `resolved_ast` -- produced once by
//! `DixLoader::compile_to_resolved_ast[_from_str]`, which runs
//! `ValueResolver`'s Phase 1 merge of imported-enum usages into `ast.enums`
//! -- but until now nothing exercised `DixConverter`'s own
//! `extract_enums`/`ast_value_to_dix_value` call sites directly, so a
//! regression specific to the CLI's `mdix convert` path could slip through
//! unnoticed even with the DixData-side test green. This test closes that
//! gap by going through `DixConverter` directly, the same way
//! `mdix-cli/src/services/conversion.rs`'s `mdix_to_json`/`mdix_to_toml` do.
//!
//! Run with:
//!   cargo test --test enum_converter_json_toml_regression -- --nocapture

use dixscript::Runtime::{DixConverter, DixLoader};

const IMPORTS_ENUM_ONLY_TYPES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/tests/imports/enum_only_types.mdix");

#[test]
fn to_json_flat_resolves_imported_enum_to_real_value_not_zero() {
    let loader = DixLoader::new();
    let source = format!(
        r#"
@IMPORTS(
  Types from "{}"
)

@DATA(
  a<enum> = Types.Priority.HIGH
  b<enum> = Types.Region.LATAM
)
"#,
        IMPORTS_ENUM_ONLY_TYPES
    );

    let ast = loader
        .compile_to_resolved_ast_from_str(&source, "converter-enum-import-json-test")
        .expect("should compile: imported enum literals for JSON conversion");

    let converter = DixConverter::new();
    let json = converter
        .to_json_flat(&ast, false)
        .expect("to_json_flat should succeed");

    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("converter output should be valid JSON");

    assert_eq!(
        parsed.get("a").and_then(|v| v.as_i64()),
        Some(2),
        "a (Types.Priority.HIGH) should serialize to 2, not 0 -- got JSON: {json}"
    );
    assert_eq!(
        parsed.get("b").and_then(|v| v.as_i64()),
        Some(4),
        "b (Types.Region.LATAM) should serialize to 4, not 0 -- got JSON: {json}"
    );
}

#[test]
fn to_toml_resolves_imported_enum_to_real_value_not_zero() {
    let loader = DixLoader::new();
    let source = format!(
        r#"
@IMPORTS(
  Types from "{}"
)

@DATA(
  a<enum> = Types.Priority.HIGH
  b<enum> = Types.Region.LATAM
)
"#,
        IMPORTS_ENUM_ONLY_TYPES
    );

    let ast = loader
        .compile_to_resolved_ast_from_str(&source, "converter-enum-import-toml-test")
        .expect("should compile: imported enum literals for TOML conversion");

    let converter = DixConverter::new();
    let toml_str = converter.to_toml(&ast).expect("to_toml should succeed");

    let parsed: toml::Value = toml_str
        .parse()
        .expect("converter output should be valid TOML");

    assert_eq!(
        parsed.get("a").and_then(|v| v.as_integer()),
        Some(2),
        "a (Types.Priority.HIGH) should serialize to 2, not 0 -- got TOML:\n{toml_str}"
    );
    assert_eq!(
        parsed.get("b").and_then(|v| v.as_integer()),
        Some(4),
        "b (Types.Region.LATAM) should serialize to 4, not 0 -- got TOML:\n{toml_str}"
    );
}
