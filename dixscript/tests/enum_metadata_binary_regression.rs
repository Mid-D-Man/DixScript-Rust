//! Regression tests for enum metadata loss through the binary pack/unpack
//! path, and for a *combined* case none of the earlier regression fixtures
//! covered: a file with enums *and* QuickFuncs together.
//!
//! Three separate fixes made these possible, in this order:
//!   1. `binary_format.rs` — added `ValueTypeTag::Enum`. The wire format had
//!      15 type tags and none of them meant "enum"; an enum field could only
//!      ever round-trip as a bare, untagged int.
//!   2. `value_encoder.rs` / `value_decoder.rs` — `encode_enum` / `decode_enum`,
//!      writing/reading `[enum_name][field_name][resolved i32]` self-contained
//!      (no cross-section @ENUMS lookup needed at decode time).
//!   3. `Compiler/Core/ValueResolution/value_resolver.rs` — Phase 1's
//!      `resolve_enums_in_value` no longer collapses `Value::EnumValue` into
//!      a bare `Value::Integer` at leaf/data positions (a bare `Enum.FIELD`
//!      sitting as a field's value). It still validates the reference (same
//!      error paths for a bad enum/field), just doesn't discard the node.
//!      Computation contexts (QuickFunc call arguments, arithmetic) are a
//!      *different* code path (`resolve_enums_in_expr` / `Expression::
//!      EnumAccess`) and are untouched by this change -- they still
//!      correctly collapse to a concrete int, which they genuinely need.
//!
//! Before all three: `Runtime/loader.rs` Stage 7 ran for *any* file with a
//! QuickFunc anywhere in scope, regardless of whether that QuickFunc had
//! anything to do with the enum field in question -- so any enum sharing a
//! file with a QuickFunc silently lost its identity, forever, with no
//! fixture anywhere pinning that down. `enum_resolution_regression.rs`
//! covers the enum-alone (no QuickFuncs) cases; this file covers the
//! enum-plus-QuickFunc combination and the binary wire format specifically.
//!
//! Consumers that were already correctly built against `DixValue::Enum` and
//! were simply starved of real input until now (no changes needed to any of
//! these): `DixData::from_ast`'s `ast_value_to_dix_value` (Runtime/dix_value.rs),
//! the `mdix_get_enum_name`/`mdix_get_enum_field` FFI exports (mdix-ffi/src/lib.rs),
//! `SchemaBuilder::require_enum` (Runtime/schema.rs, exercised below),
//! `Runtime/converter.rs`'s `@ENUMS` reconstruction from live usages, and
//! `Runtime/merge.rs`'s AST-level `EnumValue` equality.

use dixscript::Runtime::{DixLoader, DixValue, SchemaBuilder};

fn assert_enum_field(data: &dixscript::Runtime::DixData, path: &str, expected_field: &str, expected_value: i32) {
    match data.get_value(path) {
        Some(DixValue::Enum { field_name, value, .. }) => {
            assert_eq!(field_name, expected_field, "{path}: expected enum field '{expected_field}', got '{field_name}'");
            assert_eq!(*value, expected_value, "{path}: expected resolved int {expected_value}, got {value}");
        }
        other => panic!("{path}: expected DixValue::Enum, got {:?}", other),
    }
}

/// Round-trips `source` through the real production binary path: compile to
/// packed bytes (`compile_with_dlm_from_str`, which calls `BinaryPacker::pack`
/// unconditionally regardless of whether a `@DLM` section exists), then back
/// (`decompile_with_dlm_from_bytes` with an empty key, which takes the
/// plain-pack `BinaryUnpacker::unpack` branch -- no compression/encryption
/// involved either way).
fn pack_and_unpack(source: &str, label: &str) -> dixscript::Runtime::DixData {
    let loader = DixLoader::new();
    let packed = loader
        .compile_with_dlm_from_str(source, label)
        .unwrap_or_else(|e| panic!("compile_with_dlm_from_str failed for {label}: {e}"));
    loader
        .decompile_with_dlm_from_bytes(packed.processed_data, "", label)
        .unwrap_or_else(|e| panic!("decompile_with_dlm_from_bytes failed for {label}: {e}"))
}

/// Top-level enum identity survives a full binary pack/unpack round trip —
/// this file has *both* enums and a QuickFunc, the combination that was
/// always broken (Stage 7 ran regardless, Phase 1 always collapsed).
#[test]
fn top_level_enum_survives_binary_round_trip_alongside_quickfuncs() {
    let source = r#"
@ENUMS(
  Difficulty { EASY = 0, NORMAL = 1, HARD = 2, NIGHTMARE = 3 }
)

@QUICKFUNCS(
  ~doubled<int>(x) {
    return x * 2
  }
)

@DATA(
  selected_difficulty<enum> = Difficulty.HARD
  unrelated_computed = doubled(21)
)
"#;
    let data = pack_and_unpack(source, "enum-plus-quickfunc-toplevel");

    assert_enum_field(&data, "selected_difficulty", "HARD", 2);
    assert_eq!(
        data.get::<i32>("unrelated_computed").unwrap(), 42,
        "the QuickFunc call itself must still resolve correctly — this change must not touch computation contexts"
    );
}

/// Same, but the enum is inside a `GroupArray` object's field rather than a
/// top-level scalar — exercises the array/object-item recursion branch of
/// `resolve_enums_in_value`, not just the `SimpleProperty` branch.
#[test]
fn nested_enum_in_group_array_survives_binary_round_trip() {
    let source = r#"
@ENUMS(
  Role { ADMIN = 0, EDITOR = 1, VIEWER = 2 }
)

@QUICKFUNCS(
  ~greet<string>(name) {
    return name
  }
)

@DATA(
  team::
    { username = "alice", role<enum> = Role.ADMIN },
    { username = "bob",   role<enum> = Role.VIEWER }

  greeting = greet("hi")
)
"#;
    let data = pack_and_unpack(source, "enum-in-group-array");

    assert_enum_field(&data, "team[0].role", "ADMIN", 0);
    assert_enum_field(&data, "team[1].role", "VIEWER", 2);
    assert_eq!(data.get::<String>("team[0].username").unwrap(), "alice");
}

/// The raw integer payload is provably intact independent of the enum tag —
/// so if identity were ever silently lost again, this reads as a visible
/// downgrade (Enum -> Int with the *same* number) rather than a crash,
/// which is exactly the failure mode that went unnoticed before.
#[test]
fn resolved_integer_matches_declared_enum_value_exactly() {
    let source = r#"
@ENUMS(
  Priority { LOW = 5, MEDIUM = 10, HIGH = 20 }
)
@QUICKFUNCS(
  ~noop<int>(x) {
    return x
  }
)
@DATA(
  level<enum> = Priority.HIGH
  touch = noop(1)
)
"#;
    let data = pack_and_unpack(source, "enum-value-fidelity");

    // Two different accessors, same underlying int — get_int reads the
    // resolved value directly regardless of the Enum wrapper.
    assert_eq!(data.get::<i32>("level").unwrap(), 20);
    assert_enum_field(&data, "level", "HIGH", 20);
}

/// `SchemaBuilder::require_enum` — an already-shipped, previously-starved
/// consumer. Right now, before this fix, this would fail for *any* real
/// input, because `DixValue::Enum` was never actually constructed once
/// Stage 7 ran. No changes needed to schema.rs itself.
#[test]
fn schema_require_enum_accepts_a_real_enum_field() {
    let source = r#"
@ENUMS(
  Status { ACTIVE = 1, INACTIVE = 0 }
)
@QUICKFUNCS(
  ~identity<int>(x) {
    return x
  }
)
@DATA(
  account_status<enum> = Status.ACTIVE
  padding = identity(0)
)
"#;
    let data = pack_and_unpack(source, "schema-enum-validation");

    let report = SchemaBuilder::new()
        .require_enum("account_status")
        .validate(&data);

    assert!(
        report.is_valid(),
        "require_enum should accept a real DixValue::Enum field, got errors: {:?}",
        report.errors
    );
}
