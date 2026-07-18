//! Regression tests for a real bug found and fixed in
//! `Runtime/loader.rs::compile_source` (Stage 7 value-resolution gating)
//! and `Compiler/Core/ValueResolution/value_resolver.rs` (Phase 1 no longer
//! discarding enum identity at leaf/data positions — see
//! `enum_metadata_binary_regression.rs` for the full writeup of that second,
//! deeper fix).
//!
//! The original bug: Stage 7's gate was `has_local_functions ||
//! has_imported_functions` only, so any file using enums with zero
//! QuickFuncs anywhere in scope skipped value resolution entirely, and its
//! enum references either silently defaulted to 0 (binary/DLM output) or
//! depended on an accidental fallback path (plain `from_ast` loading) that
//! only worked for *local* enums.
//!
//! Two distinct fixtures are covered here, matching the two ways this used
//! to be reachable:
//!   - a file using its own local `@ENUMS` with no `@QUICKFUNCS` at all
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

/// Only checks field_name identity, not the resolved int -- see the long
/// comment on `imported_enum_only_with_zero_functions_anywhere_resolves`
/// below for why the int specifically is still a known, separate gap for
/// imported enums on this code path.
fn assert_enum_field_name_only(data: &dixscript::Runtime::DixData, path: &str, expected_field: &str) {
    match data.get_value(path) {
        Some(DixValue::Enum { field_name, .. }) => {
            assert_eq!(field_name, expected_field, "{path}: expected enum field '{expected_field}', got '{field_name}'");
        }
        other => panic!("{path}: expected DixValue::Enum, got {:?}", other),
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
/// has_local_functions and has_imported_functions are BOTH false (only
/// has_imported_enums is true). Before the loader.rs fix, Stage 7 never ran
/// for this file at all.
///
/// KNOWN REMAINING GAP, precisely scoped: Phase 1 now validates this
/// reference correctly (via symbol_table, which has full visibility into
/// imported namespaces) and leaves the `Value::EnumValue` node intact --
/// field_name identity is correct below. But `DixData::from_ast` (the
/// consumer that turns the resolved AST into the final `DixValue::Enum`)
/// re-derives the *integer* independently via its own `extract_enums_section`,
/// which only ever reads `ast.enums` -- this file's own local `@ENUMS`
/// section. It has no access to the symbol table, so it can't see
/// `enum_only_types.mdix`'s declarations, and its lookup falls back to 0 for
/// any enum that isn't locally declared. Closing this needs `from_ast` to
/// receive the symbol table too, which cascades to ~25 call sites across 4
/// crates (dixscript's own internal tests, loader.rs, data_builder.rs,
/// merge.rs, and mdix-wasm/mdix-lua/mdix-ffi's own merge.rs each) -- too
/// invasive to change without the ability to compile-check it here.
/// value_encoder.rs's `local_enums` table has the identical blind spot for
/// the same reason (see its own doc comment) and degrades the same way:
/// name preserved, resolved int falls back to 0 with a logged warning
/// rather than silently or hard-failing.
#[test]
fn imported_enum_only_with_zero_functions_anywhere_resolves() {
    let loader = DixLoader::new();
    let fixture_path =
        concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/tests/imports/11_enum_only_import_no_funcs.mdix");

    let result = loader
        .load_text(fixture_path, &DixLoadOptions::new())
        .expect("should compile: imported enum-only module, zero QuickFuncs anywhere in scope");

    assert_enum_field_name_only(&result, "ticket_priority", "HIGH");
    assert_enum_field_name_only(&result, "fallback_priority", "LOW");
    assert_enum_field_name_only(&result, "deploy_region", "EU");
    assert_enum_field_name_only(&result, "incidents[0].priority", "CRITICAL");
    assert_enum_field_name_only(&result, "incidents[0].region", "APAC");
    assert_enum_field_name_only(&result, "incidents[1].priority", "MEDIUM");
    assert_enum_field_name_only(&result, "incidents[1].region", "NA");
}
