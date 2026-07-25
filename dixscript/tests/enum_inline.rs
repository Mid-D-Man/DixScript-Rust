//! Tests for `DixConverter::inline_enum_values` (the shared core utility
//! that resolves every `Value::EnumValue` to its literal `Value::Integer`)
//! and the `DixFormatOptions::inline_enum_values` flag that makes `to_mdix`
//! use it instead of the default identity-preserving flattened-declaration
//! behavior (`enum_mdix_roundtrip_regression.rs` covers that default path).
//!
//! Run with:
//!   cargo test --test enum_inline_literal_regression -- --nocapture

use dixscript::Runtime::{DixConverter, DixFormatOptions, DixLoader, DixLoadOptions};

const IMPORTS_ENUM_ONLY_TYPES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/tests/imports/enum_only_types.mdix");

/// `DixConverter::inline_enum_values` directly: local enum, top-level
/// property, table property, and group array all in the same DataSection.
#[test]
fn inline_enum_values_resolves_local_enums_everywhere_they_can_appear() {
    let loader = DixLoader::new();
    let source = r#"
@ENUMS(
  Status { ACTIVE = 1, INACTIVE = 0, PENDING = 2 }
)

@DATA(
  top<enum> = Status.ACTIVE
  timer:
    nested<enum> = Status.PENDING
  flags:: Status.ACTIVE, Status.INACTIVE
)
"#;
    let ast = loader
        .compile_to_resolved_ast_from_str(source, "inline-local-test")
        .expect("should compile");

    let converter = DixConverter::new();
    let data = ast.data.as_ref().expect("data section present");
    let inlined = converter.inline_enum_values(data, ast.enums.as_ref());

    let rendered = format!("{}", inlined);
    // Every enum reference should now be a bare integer literal, not
    // "Status.ACTIVE" / "Status.PENDING" / "Status.INACTIVE" anywhere.
    assert!(!rendered.contains("Status."), "enum references were not inlined:\n{rendered}");
    assert!(rendered.contains('1'), "expected the ACTIVE=1 literal somewhere:\n{rendered}");
    assert!(rendered.contains('2'), "expected the PENDING=2 literal somewhere:\n{rendered}");
    assert!(rendered.contains('0'), "expected the INACTIVE=0 literal somewhere:\n{rendered}");
}

/// Same, but for an IMPORTED enum — the case that actually motivated this.
#[test]
fn inline_enum_values_resolves_imported_enums_too() {
    let loader = DixLoader::new();
    let source = format!(
        r#"
@IMPORTS(
  Types from "{}"
)

@DATA(
  a<enum> = Types.Priority.HIGH
)
"#,
        IMPORTS_ENUM_ONLY_TYPES
    );
    let ast = loader
        .compile_to_resolved_ast_from_str(&source, "inline-imported-test")
        .expect("should compile");

    let converter = DixConverter::new();
    let data = ast.data.as_ref().expect("data section present");
    let inlined = converter.inline_enum_values(data, ast.enums.as_ref());

    let rendered = format!("{}", inlined);
    assert!(!rendered.contains("Types.Priority"), "imported enum reference not inlined:\n{rendered}");
    assert!(rendered.contains("a<int> = 2"), "expected literal 2 with the <enum> annotation downgraded to <int>:\n{rendered}");
    assert!(!rendered.contains("<enum>"), "the now-meaningless <enum> annotation should have been downgraded:\n{rendered}");
}

/// `to_mdix` with `inline_enum_values: true`: no @ENUMS section at all, and
/// the output must still be valid, re-parseable, self-contained `.mdix`.
#[test]
fn to_mdix_with_inline_enum_values_omits_enums_section_and_still_round_trips() {
    let loader = DixLoader::new();
    let source = format!(
        r#"
@IMPORTS(
  Types from "{}"
)

@DATA(
  a<enum> = Types.Priority.HIGH
)
"#,
        IMPORTS_ENUM_ONLY_TYPES
    );
    let ast = loader
        .compile_to_resolved_ast_from_str(&source, "inline-to-mdix-test")
        .expect("should compile");

    let converter = DixConverter::new();
    let opts = DixFormatOptions { inline_enum_values: true, ..DixFormatOptions::new() };
    let mdix_text = converter.to_mdix(&ast, Some(&opts)).expect("to_mdix should succeed");

    assert!(!mdix_text.contains("@ENUMS"), "expected no @ENUMS section:\n{mdix_text}");
    assert!(!mdix_text.contains("Types.Priority"), "expected no symbolic reference left:\n{mdix_text}");
    assert!(!mdix_text.contains("<enum>"), "the now-meaningless <enum> annotation should have been downgraded:\n{mdix_text}");
    assert!(mdix_text.contains('2'), "expected the literal 2 in the output:\n{mdix_text}");

    // Must still compile completely standalone.
    let roundtrip_loader = DixLoader::new();
    let roundtrip = roundtrip_loader
        .load_from_str(&mdix_text, &DixLoadOptions::new())
        .unwrap_or_else(|e| panic!("inline_enum_values output failed to re-compile:\n{mdix_text}\n\nerror: {e}"));

    match roundtrip.get_value("a") {
        Some(dixscript::Runtime::DixValue::Int(v)) => assert_eq!(*v, 2),
        other => panic!("expected a plain Int(2) after round-trip, got {:?}", other),
    }
}

/// The default (`inline_enum_values: false`) behavior must be unchanged:
/// still a valid flattened local declaration, still an @ENUMS section.
#[test]
fn to_mdix_default_still_preserves_enum_identity() {
    let loader = DixLoader::new();
    let source = format!(
        r#"
@IMPORTS(
  Types from "{}"
)

@DATA(
  a<enum> = Types.Priority.HIGH
)
"#,
        IMPORTS_ENUM_ONLY_TYPES
    );
    let ast = loader
        .compile_to_resolved_ast_from_str(&source, "inline-default-test")
        .expect("should compile");

    let converter = DixConverter::new();
    let mdix_text = converter.to_mdix(&ast, None).expect("to_mdix should succeed");

    assert!(mdix_text.contains("@ENUMS"), "expected @ENUMS section to still be present by default:\n{mdix_text}");
    assert!(mdix_text.contains("Types_Priority"), "expected the flattened symbolic form by default:\n{mdix_text}");
}
