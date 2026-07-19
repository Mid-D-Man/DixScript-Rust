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
//! The second fixture also pins down a *follow-up* fix (Option B from the
//! design discussion): `DixData::from_ast` only ever reads `ast.enums` --
//! this file's own local `@ENUMS` -- and has no `symbol_table` access, so
//! an imported enum used with no local re-declaration used to get the right
//! `field_name` but a `value` that silently fell back to 0. Rather than
//! threading `symbol_table` into `from_ast` (a public API with ~25 call
//! sites across 4 crates), `resolve_all_enum_values` (Phase 1's driver) now
//! collects every imported enum actually referenced during the walk and
//! merges a synthesized declaration for it into `self.ast.enums`, keyed by
//! its full qualified name ("Types.Priority") so it can never collide with
//! a local enum's bare name. `from_ast` and `value_encoder.rs`'s
//! `local_enums` table both just read `ast.enums` either way, so this fixes
//! both the plain-load and binary paths without touching either of them.
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
/// has_local_functions and has_imported_functions are BOTH false (only
/// has_imported_enums is true). Before the loader.rs fix, Stage 7 never ran
/// for this file at all.
///
/// This is also the regression guard for the Option B follow-up (see the
/// module doc comment above): all seven assertions check the resolved
/// *int*, not just field_name identity -- a regression back to "field name
/// right, value silently 0" (the state before the resolve_all_enum_values
/// merge existed) would fail loudly here, not just look slightly off.
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
    // path as top-level scalar fields, not just SimpleProperty.
    assert_enum_field(&result, "incidents[0].priority", "CRITICAL", 3);
    assert_enum_field(&result, "incidents[0].region", "APAC", 3);
    assert_enum_field(&result, "incidents[1].priority", "MEDIUM", 1);
    assert_enum_field(&result, "incidents[1].region", "NA", 1);
}

/// Directly exercises the Option B merge itself: two *different* imported
/// enums referenced by two *different* fields, from a file with a local
/// @QUICKFUNCS unrelated to either of them (so Stage 7 would have run
/// regardless of the enum-presence gate -- this isolates the merge fix from
/// the gating fix, which is already covered by the two tests above).
#[test]
fn multiple_distinct_imported_enums_all_merge_correctly() {
    let loader = DixLoader::new();
    let source = format!(
        r#"
@IMPORTS(
  Types from "{}"
)

@QUICKFUNCS(
  ~identity<int>(x) {{
    return x
  }}
)

@DATA(
  a<enum> = Types.Priority.MEDIUM
  b<enum> = Types.Region.LATAM
  touch = identity(0)
)
"#,
        concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/tests/imports/enum_only_types.mdix")
    );

    let result = loader
        .load_from_str(&source, &DixLoadOptions::new())
        .expect("should compile: two distinct imported enums alongside a local QuickFunc");

    assert_enum_field(&result, "a", "MEDIUM", 1);
    assert_enum_field(&result, "b", "LATAM", 4);
  }
