//! Regression tests for a real bug found and fixed in
//! `Runtime/loader.rs::compile_source` (Stage 7 value-resolution gating).
//!
//! The gate used to be `has_local_functions || has_imported_functions`,
//! on the assumption that a file with no QuickFuncs anywhere in scope has
//! nothing for `ValueResolver` to do. That's wrong: `ValueResolver::resolve()`
//! always runs Phase 1 (`resolve_all_enum_values`) first, which resolves
//! every `Value::EnumValue` node -- local AND imported -- to a plain
//! `Value::Integer`, completely independent of whether any QuickFunc call
//! exists. A file that used enums (local or imported) but had zero
//! QuickFuncs anywhere in scope skipped this stage entirely, leaving raw
//! `Value::EnumValue` nodes in the resolved AST.
//!
//! Two distinct fixtures are covered here, matching the two ways this used
//! to be reachable:
//!   - a file using its own local `@ENUMS` with no `@QUICKFUNCS` at all
//!     (already had some incidental coverage elsewhere, included here for
//!     completeness and to pin the exact mechanism down in one place)
//!   - a file that imports *another* file's enums -- and only its enums,
//!     the imported module has no `@QUICKFUNCS` either -- into its own
//!     `@DATA` section, while having no local `@QUICKFUNCS` of its own.
//!     Every existing importable fixture (`base_types.mdix`,
//!     `game_helpers.mdix`) pairs `@ENUMS` with `@QUICKFUNCS`, so
//!     `has_imported_functions` was always incidentally true for anything
//!     importing them -- this second case was previously untested and is
//!     the exact gap reported: "enums never get resolved if used alone and
//!     no QuickFuncs... in the case where we have a file that imports
//!     another file and uses its enums... within the file data section but
//!     it itself lacks those sections".
//!
//! Run with:
//!   cargo test --test enum_resolution_regression -- --nocapture

use dixscript::Runtime::{DixLoader, DixLoadOptions, DixValue};

/// Local-enum-only, zero QuickFuncs anywhere -- no imports at all.
const SRC_LOCAL_ENUM_NO_FUNCS: &str = r#"
@ENUMS(
  Status { ACTIVE = 1, INACTIVE = 0, PENDING = 2 }
)

@DATA(
  current_status<enum> = Status.PENDING
  app_name = "EnumOnlyApp"
)
"#;

fn assert_enum_field(data: &dixscript::Runtime::DixData, path: &str, expected_field: &str, expected_value: i32) {
    match data.get_value(path) {
        Some(DixValue::Enum { field_name, value, .. }) => {
            assert_eq!(
                field_name, expected_field,
                "{path}: expected enum field '{expected_field}', got '{field_name}'"
            );
            assert_eq!(
                *value, expected_value,
                "{path}: expected resolved int {expected_value}, got {value} \
                 -- a silent lookup-miss/skip would show up here as 0"
            );
        }
        other => panic!(
            "{path}: expected DixValue::Enum, got {:?} -- enum resolution did not run \
             (this is exactly the Stage 7 gating bug if this fails)",
            other
        ),
    }
}

#[test]
fn local_enum_alone_with_no_quickfuncs_resolves() {
    let loader = DixLoader::new();
    let result = loader
        .load_from_str(SRC_LOCAL_ENUM_NO_FUNCS, &DixLoadOptions::new())
        .expect("should compile: local enum, no QuickFuncs anywhere");

    assert_enum_field(&result, "current_status", "PENDING", 2);
    assert_eq!(
        result.get::<String>("app_name").unwrap(),
        "EnumOnlyApp",
        "sibling non-enum field should be unaffected"
    );
}

/// Imports `enum_only_types.mdix` (an @ENUMS-only module, zero @QUICKFUNCS)
/// and uses its enums in @DATA, with no local @QUICKFUNCS either -- so
/// has_local_functions and has_imported_functions are BOTH false. Before
/// the loader.rs fix, Stage 7 never ran for this file at all.
#[test]
fn imported_enum_only_with_zero_functions_anywhere_resolves() {
    let loader = DixLoader::new();
    let fixture_path =
        concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/tests/imports/11_enum_only_import_no_funcs.mdix");

    let result = loader
        .load_text(fixture_path, &DixLoadOptions::new())
        .expect("should compile: imported enum-only module, zero QuickFuncs anywhere in scope");

    assert_enum_field(&result, "ticket_priority", "HIGH", 2);
    assert_enum_field(&result, "fallback_priority", "LOW", 0);
    assert_enum_field(&result, "deploy_region", "EU", 2);

    // Nested inside a group array -- exercises the same recursive resolution
    // path ValueResolver uses for object/array literals, not just top-level
    // scalar fields.
    assert_enum_field(&result, "incidents[0].priority", "CRITICAL", 3);
    assert_enum_field(&result, "incidents[0].region", "APAC", 3);
    assert_enum_field(&result, "incidents[1].priority", "MEDIUM", 1);
    assert_enum_field(&result, "incidents[1].region", "NA", 1);
  }
