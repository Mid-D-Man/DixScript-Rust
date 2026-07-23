//! Regression tests for a real bug found and fixed in
//! `Compiler/Core/general_semantics_analyzer.rs::register_enums_with_builtin_system`
//! (and its call site in `analyze()`).
//!
//! `enum_object::DIXSCRIPT_ENUMS` (in `Builtins/Static/enum_object.rs`) is a
//! process-global registry that backs the `Enum.*` builtin static object
//! (`Enum.getValue`, `Enum.getValues`, `Enum.getName`, `Enum.exists`, etc.).
//! `GeneralSemanticAnalyzer::analyze()` used to call
//! `register_enums_with_builtin_system()` -- which unconditionally does
//! `enum_object::clear_enums()` then re-registers only `self.symbol_table.enums`
//! (this file's own *local* enums) -- on EVERY `analyze()` call, including
//! the fully-recursive nested `analyze()` that `ImportsResolver` runs on
//! every imported file (see `GeneralSemanticAnalyzer::new_with_seed_namespaces`,
//! called from `ImportsResolution/imports_resolver.rs`).
//!
//! For any file that imports another file, the sequence used to be:
//!   1. Outer file's Phase 3 (imports resolution) recursively compiles the
//!      imported file, which -- as part of its own `analyze()` -- clears the
//!      registry and registers only ITS OWN local enums.
//!   2. Outer file continues to its own Phase 4 + registration step, which
//!      clears the registry AGAIN and registers only the OUTER file's own
//!      local enums (zero, for an "imports only, no local @ENUMS" file).
//!
//! Net effect: by the time compilation finished, every enum registered by
//! an imported file was gone. Any `Enum.*` builtin call referencing an
//! imported enum would fail to find it in the registry and error out --
//! this is the "enum builtins produce null" symptom. The fix: only the
//! outermost (non-nested) `analyze()` call touches the registry, and it now
//! registers BOTH its own local enums (bare name, e.g. `"Status"`) AND every
//! imported namespace's enums (qualified, e.g. `"Types.Priority"` --
//! matching the exact qualification `Value::EnumValue.enum_name` and
//! `ValueResolver`'s import merge already use elsewhere in the compiler).
//!
//! Run with:
//!   cargo test --test enum_builtin_registry_regression -- --nocapture

use dixscript::Runtime::{DixData, DixLoader, DixLoadOptions, DixValue};

const IMPORTS_ENUM_ONLY_TYPES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/tests/imports/enum_only_types.mdix");

fn assert_int_field(data: &DixData, path: &str, expected: i32) {
    match data.get_value(path) {
        Some(DixValue::Int(v)) => assert_eq!(
            *v, expected,
            "{path}: expected {expected}, got {v}"
        ),
        other => panic!(
            "{path}: expected DixValue::Int({expected}), got {:?} -- this is the \
             enum-registry-clobbering bug if this is an error/None instead of a value",
            other
        ),
    }
}

fn assert_bool_field(data: &DixData, path: &str, expected: bool) {
    match data.get_value(path) {
        Some(DixValue::Bool(v)) => assert_eq!(*v, expected, "{path}: expected {expected}, got {v}"),
        other => panic!("{path}: expected DixValue::Bool({expected}), got {:?}", other),
    }
}

/// A file with ZERO local enums of its own that calls `Enum.getValue(...)`
/// and `Enum.exists(...)` on an IMPORTED enum by its qualified name. Before
/// the fix this always failed to compile: the nested import analysis
/// registered "Priority"/"Region" bare-named, then the outer file's own
/// registration step wiped the registry and re-registered its own (zero)
/// local enums, leaving `Enum.getValue("Types.Priority", "HIGH")` unable to
/// find anything and erroring the whole compile out (error_handling
/// defaults to halt).
#[test]
fn enum_builtin_resolves_imported_enum_by_qualified_name() {
    let loader = DixLoader::new();
    let source = format!(
        r#"
@IMPORTS(
  Types from "{}"
)

@QUICKFUNCS(
  ~priorityValue<int>() {{
    return Enum.getValue("Types.Priority", "HIGH")
  }}
  ~regionExists<bool>() {{
    return Enum.exists("Types.Region")
  }}
)

@DATA(
  resolved_priority<int> = priorityValue()
  region_known<bool> = regionExists()
)
"#,
        IMPORTS_ENUM_ONLY_TYPES
    );

    let result = loader
        .load_from_str(&source, &DixLoadOptions::new())
        .expect("should compile: Enum.* builtin call against an imported enum's qualified name");

    assert_int_field(&result, "resolved_priority", 2);
    assert_bool_field(&result, "region_known", true);
}

/// Local AND imported enums used together via `Enum.*` builtins in the same
/// file -- makes sure adding qualified-name registration for imports didn't
/// break plain bare-name registration for this file's own local enums.
#[test]
fn enum_builtin_resolves_both_local_and_imported_enums_together() {
    let loader = DixLoader::new();
    let source = format!(
        r#"
@ENUMS(
  Severity {{ LOW = 1, HIGH = 9 }}
)

@IMPORTS(
  Types from "{}"
)

@QUICKFUNCS(
  ~localValue<int>() {{
    return Enum.getValue("Severity", "HIGH")
  }}
  ~importedValue<int>() {{
    return Enum.getValue("Types.Priority", "CRITICAL")
  }}
)

@DATA(
  local_result<int> = localValue()
  imported_result<int> = importedValue()
)
"#,
        IMPORTS_ENUM_ONLY_TYPES
    );

    let result = loader
        .load_from_str(&source, &DixLoadOptions::new())
        .expect("should compile: local + imported enums both used via Enum.* builtins");

    assert_int_field(&result, "local_result", 9);
    assert_int_field(&result, "imported_result", 3);
}

/// Guards the ORIGINAL fix this bug's fix must not regress: a stale
/// registry from a PREVIOUS, unrelated compile must never leak into a new
/// one (this is the exact property that made fuzzing safe -- see the
/// `clear_enums()` call at the top of `register_enums_with_builtin_system`).
/// Compiles an import-heavy file first (which registers a qualified
/// "Types.Priority" entry), then compiles a second, totally unrelated file
/// with only its own local enum in the SAME process, and checks the second
/// file does NOT see the first file's leftover imported-enum entry.
#[test]
fn later_unrelated_compile_does_not_see_previous_compiles_imported_enums() {
    let loader = DixLoader::new();

    let first_source = format!(
        r#"
@IMPORTS(
  Types from "{}"
)

@QUICKFUNCS(
  ~touch<int>() {{
    return Enum.getValue("Types.Priority", "LOW")
  }}
)

@DATA(
  touched<int> = touch()
)
"#,
        IMPORTS_ENUM_ONLY_TYPES
    );
    loader
        .load_from_str(&first_source, &DixLoadOptions::new())
        .expect("first (import-heavy) compile should succeed");

    let second_source = r#"
@ENUMS(
  Mood { HAPPY = 1, SAD = 2 }
)

@QUICKFUNCS(
  ~knowsPriority<bool>() {
    return Enum.exists("Types.Priority")
  }
  ~moodValue<int>() {
    return Enum.getValue("Mood", "HAPPY")
  }
)

@DATA(
  saw_stale_import<bool> = knowsPriority()
  own_enum<int> = moodValue()
)
"#;

    let result = loader
        .load_from_str(second_source, &DixLoadOptions::new())
        .expect("second (local-only) compile should succeed and not see the first compile's imports");

    assert_bool_field(
        &result,
        "saw_stale_import",
        false, // a prior compile's imported enum must NOT leak across compiles
    );
    assert_int_field(&result, "own_enum", 1);
}
