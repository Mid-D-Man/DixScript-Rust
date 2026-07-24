//! Regression test for a real bug found and fixed in
//! `Runtime/converter.rs::to_mdix` / `format_value_for_mdix`.
//!
//! `ValueResolver::resolve_all_enum_values` synthesizes an `EnumDeclaration`
//! for every imported enum a file actually uses, named with the full
//! qualified form (`"EnumMan.Suka"`) so it can never collide with a real
//! local declaration -- see `enum_resolution_regression.rs` and
//! `enum_converter_json_toml_regression.rs`, which both confirm that's the
//! right key for in-memory lookups. But `to_mdix` -- the serializer behind
//! `mdix format`, `mdix decrypt`, and any `DixFormatOptions::minify` output
//! -- used to write `decl.name` straight into `@ENUMS(...)` and
//! `Value::EnumValue.enum_name` straight into `@DATA`, producing text like:
//!
//! ```text
//! @ENUMS(
//!   EnumMan.Suka { Booliat = 0, Zabania = 1, Crack = 9 }
//! )
//! @DATA(
//!   uop = EnumMan.Suka.Crack
//! )
//! ```
//!
//! `.mdix` enum declaration names are plain identifiers -- the grammar has
//! no syntax for a dot inside one -- so `EnumMan.Suka { ... }` doesn't
//! re-parse, and a round-tripped file never carries the original
//! `@IMPORTS` forward anyway, so `EnumMan.Suka.Crack` has no namespace left
//! to resolve against even if it did. The fix flattens any dotted enum
//! declaration name to a valid local identifier (`.` -> `_`, deduped
//! against every other name in the file) and rewrites the matching `@DATA`
//! references to match, turning the imported enum into a real self-
//! contained local one in the output file.
//!
//! Run with:
//!   cargo test --test enum_mdix_roundtrip_regression -- --nocapture

use dixscript::Runtime::{DixConverter, DixLoader};

const IMPORTS_ENUM_ONLY_TYPES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/tests/imports/enum_only_types.mdix");

/// The core regression: an imported enum used as a plain `@DATA` literal
/// must round-trip through `to_mdix` as valid, re-parseable `.mdix` text
/// that resolves to the same value with no `@IMPORTS` left in the picture.
#[test]
fn to_mdix_flattens_imported_enum_into_a_valid_local_declaration() {
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
        .compile_to_resolved_ast_from_str(&source, "mdix-roundtrip-enum-test")
        .expect("should compile: imported enum literal");

    let converter = DixConverter::new();
    let mdix_text = converter
        .to_mdix(&ast, None)
        .expect("to_mdix should succeed");

    // The invalid, unparseable form must never appear in the output.
    assert!(
        !mdix_text.contains("Types.Priority {"),
        "to_mdix wrote an unparseable dotted enum declaration name -- got:\n{mdix_text}"
    );

    // A valid, flattened local declaration should be there instead.
    assert!(
        mdix_text.contains("Types_Priority"),
        "expected a flattened 'Types_Priority' local enum declaration -- got:\n{mdix_text}"
    );

    // The @DATA reference must be rewritten to match (2-part local form),
    // not left as the unparseable 3-part imported form.
    assert!(
        !mdix_text.contains("Types.Priority.HIGH"),
        "the @DATA value still uses the unparseable 3-part imported form -- got:\n{mdix_text}"
    );
    assert!(
        mdix_text.contains("Types_Priority.HIGH"),
        "expected the @DATA value rewritten to 'Types_Priority.HIGH' -- got:\n{mdix_text}"
    );

    // The real test: the output must actually re-parse and re-resolve to
    // the same correct value, entirely on its own, no @IMPORTS needed.
    let roundtrip_loader = DixLoader::new();
    let roundtrip = roundtrip_loader
        .load_from_str(&mdix_text, &dixscript::Runtime::DixLoadOptions::new())
        .unwrap_or_else(|e| {
            panic!("to_mdix output failed to re-compile on its own -- this means the flattened output is STILL invalid .mdix:\n{mdix_text}\n\nerror: {e}")
        });

    match roundtrip.get_value("a") {
        Some(dixscript::Runtime::DixValue::Enum { value, .. }) => {
            assert_eq!(*value, 2, "Types.Priority.HIGH should round-trip to 2");
        }
        Some(dixscript::Runtime::DixValue::Int(v)) => {
            assert_eq!(*v, 2, "Types.Priority.HIGH should round-trip to 2");
        }
        other => panic!("expected an enum/int value of 2 after round-trip, got {:?}", other),
    }
}

const IMPORTS_BASE_TYPES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/tests/imports/base_types.mdix");

/// Two imported enums from DIFFERENT files used together must each flatten
/// independently and correctly -- neither should interfere with the other,
/// and both must still be present (nothing silently dropped) in the output.
#[test]
fn to_mdix_flattens_multiple_distinct_imported_enums_independently() {
    let loader = DixLoader::new();
    let source = format!(
        r#"
@IMPORTS(
  X from "{base_types}"
  Y from "{enum_only_types}"
)

@DATA(
  first<enum>  = X.Rarity.RARE
  second<enum> = Y.Priority.CRITICAL
)
"#,
        base_types = IMPORTS_BASE_TYPES,
        enum_only_types = IMPORTS_ENUM_ONLY_TYPES,
    );

    let ast = loader
        .compile_to_resolved_ast_from_str(&source, "mdix-roundtrip-dedup-test")
        .expect("should compile: two different files imported under two different aliases");

    let converter = DixConverter::new();
    let mdix_text = converter
        .to_mdix(&ast, None)
        .expect("to_mdix should succeed");

    assert!(mdix_text.contains("X_Rarity"), "missing flattened 'X_Rarity':\n{mdix_text}");
    assert!(mdix_text.contains("Y_Priority"), "missing flattened 'Y_Priority':\n{mdix_text}");
    assert!(!mdix_text.contains("X.Rarity {"), "unparseable dotted declaration leaked through:\n{mdix_text}");
    assert!(!mdix_text.contains("Y.Priority {"), "unparseable dotted declaration leaked through:\n{mdix_text}");

    let roundtrip_loader = DixLoader::new();
    let roundtrip = roundtrip_loader
        .load_from_str(&mdix_text, &dixscript::Runtime::DixLoadOptions::new())
        .unwrap_or_else(|e| {
            panic!("to_mdix output with two distinct imports failed to re-compile:\n{mdix_text}\n\nerror: {e}")
        });

    let first_val = match roundtrip.get_value("first") {
        Some(dixscript::Runtime::DixValue::Enum { value, .. }) => *value,
        Some(dixscript::Runtime::DixValue::Int(v)) => *v,
        other => panic!("expected int/enum for 'first', got {:?}", other),
    };
    let second_val = match roundtrip.get_value("second") {
        Some(dixscript::Runtime::DixValue::Enum { value, .. }) => *value,
        Some(dixscript::Runtime::DixValue::Int(v)) => *v,
        other => panic!("expected int/enum for 'second', got {:?}", other),
    };

    assert_eq!(first_val, 2, "X.Rarity.RARE should round-trip to 2");
    assert_eq!(second_val, 3, "Y.Priority.CRITICAL should round-trip to 3");
}
