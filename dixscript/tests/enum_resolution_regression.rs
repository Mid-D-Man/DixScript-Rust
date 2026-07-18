//! Regression test pinning a real, currently-unfixed bug: enum identity
//! (enum type name + field name) does not survive the compiled binary
//! round trip -- or in fact ANY path through `ValueResolver` Phase 1 once
//! a DixScript enum reaches `@DATA`. It silently degrades to a bare
//! `DixValue::Int`, indistinguishable from a hand-written integer literal.
//! No error, no panic -- just a quiet type downgrade.
//!
//! ## Root cause
//!
//! `ValueResolver::resolve_all_enum_values()` (Phase 1, in
//! `Compiler/Core/ValueResolution/value_resolver.rs::resolve_enums_in_value`)
//! unconditionally rewrites every `Value::EnumValue { enum_name, value:
//! field_name, .. }` node into a bare `Value::Integer { value: int_val,
//! .. }`, discarding `enum_name`/`field_name` with no trace left behind.
//! This runs for BOTH the plain source-load path and the binary-compile
//! path (`Runtime/loader.rs` Stage 7), because `has_local_enums ||
//! has_imported_enums` was added to Stage 7's gate as part of fixing the
//! "enum-only file with no QuickFuncs never resolves" bug (see
//! `enum_resolution_regression.rs`).
//!
//! Before that gating fix, an enum-only file with zero QuickFuncs in
//! scope skipped Stage 7 entirely, so `Value::EnumValue` nodes reached
//! `DixData::from_ast` -> `ast_value_to_dix_value` (`Runtime/dix_value.rs`)
//! untouched, and its `Value::EnumValue` branch correctly produced a real
//! `DixValue::Enum { enum_name, field_name, value }`. That was an
//! accidental side effect of the bug, not a designed code path -- but it
//! was the ONLY way `DixValue::Enum` ever got constructed in practice.
//! Closing the QuickFuncs gating hole (correctly) closed that accidental
//! path too, so `DixValue::Enum` construction is now dead in every real
//! scenario, text-load and binary-load alike.
//!
//! The binary format compounds this independently of the above:
//! `ValueTypeTag` (`Compiler/Core/BinarySerialization/binary_format.rs`)
//! has no `Enum` variant at all, and `value_encoder.rs` hard-errors if an
//! unresolved `EnumValue` ever reaches it (dead code today, since Phase 1
//! always empties the AST of them first) -- so even a correct Phase 1
//! wouldn't be sufficient on its own; the wire format needs a tag too.
//!
//! ## What's already correct and just needs live input again
//!
//! (confirmed by reading every file in this list -- none of them need to
//! change)
//!
//! - `ast_value_to_dix_value`'s `Value::EnumValue` branch
//!   (`Runtime/dix_value.rs:235-246`)
//! - `DixData::extract_enums_section` / `DixData.enums`
//!   (`Runtime/dix_data.rs`)
//! - `mdix_get_enum_name` / `mdix_get_enum_field` FFI exports
//!   (`mdix-ffi/src/lib.rs:575-605`) -- shipped, and currently always
//!   answers "not an enum" for real data.
//! - `SchemaBuilder::require_enum` / `ExpectedValueType::Enum`
//!   (`Runtime/schema.rs:274-275,481`) -- shipped, currently unusable:
//!   see `schema_validation_require_enum_accepts_a_real_enum_field` below.
//! - `DixConverter`'s `DixValue::Enum <-> Value::EnumValue` round trip
//!   (`Runtime/converter.rs:992,1048,1057`) and `@ENUMS` reconstruction
//!   from live enum usages (`collect_enum_usages`).
//! - `MdixMerger`'s AST-level `EnumValue` equality arm
//!   (`Runtime/merge.rs:5,1291-1292`) -- merges enums correctly because it
//!   operates pre-resolution.
//! - `data_builder.rs` (`Value::EnumValue` construction at lines 460, 660,
//!   740) -- already builds correct nodes for anything going through the
//!   builder API.
//! - `array_homogenizer.rs` -- audited, confirmed SAFE with no change
//!   needed: `numeric_rank()` returns `None` for any non-numeric value
//!   (including a surviving `EnumValue`), which makes
//!   `homogenize_numeric_siblings` bail on the whole array untouched --
//!   exactly the documented, intended behavior for enums today.
//! - `compactor.rs` -- audited, confirmed UNRELATED: it's a lexer-token
//!   minifier for raw source text, never touches resolved `Value`/
//!   `DixValue` at all.
//! - `type_inference_visitor.rs`'s `DixType::Enum` <-> `DataType::Enum`
//!   conversion table -- compile-time static type tags for QuickFunc
//!   bodies, independent of runtime value fidelity. Unaffected either way.
//!
//! ## What actually needs new code (three places, in this order)
//!
//! 1. `binary_format.rs`: new `ValueTypeTag::Enum` (e.g. `0x10` --
//!    15 tags used today, room up to `0xFE` before `Invalid = 0xFF`).
//! 2. `value_encoder.rs` / `value_decoder.rs`: `encode_enum` / `decode_enum`,
//!    self-contained `[tag][enum_name][field_name][resolved i32]`.
//!    `data_section_reader.rs` has no access to the `@ENUMS` table today
//!    (checked -- it reads its section in isolation), so an
//!    index-into-that-table encoding would need cross-section context
//!    threaded through it, which is more invasive than duplicating the
//!    (typically short, DLM-compressed-away) name strings. `decode_enum`
//!    only needs to reconstruct a plain `Value::EnumValue { enum_name,
//!    value: field_name, position }` node -- the exact shape the parser
//!    already produces -- so `ast_value_to_dix_value` picks it back up
//!    for free with zero changes on that side.
//! 3. `ValueResolver::resolve_enums_in_value` (Phase 1): stop collapsing
//!    `EnumValue` to `Integer` at *leaf/data positions* (direct entry
//!    value, object property value, array item -- the top-level match arm
//!    in `resolve_enums_in_value`). Keep collapsing inside genuine
//!    computation contexts (`resolve_enums_in_expr`'s binary ops,
//!    conditionals, QuickFunc call arguments, interpolated strings) --
//!    those need a concrete number to execute, and identity doesn't
//!    survive arithmetic anyway. Still validate the field exists (keep
//!    the error path), just don't discard the node when nothing is
//!    computing with it.
//!
//! Once (3) lands, arrays/objects/group-array items containing
//! `EnumValue` will survive further into the post-resolution AST than
//! they ever have before -- re-check `array_homogenizer.rs` (already
//! confirmed safe above) and anything else that walks that AST assuming
//! "no `EnumValue` past this point" before shipping.
//!
//! ## Test status
//!
//! Every test below is written against the CORRECT/desired behavior and
//! is expected to FAIL until the three changes above land. Run with:
//!   cargo test --test enum_metadata_binary_regression -- --nocapture
//!
//! (Companion to `enum_resolution_regression.rs`, which pins the earlier,
//! already-fixed Stage 7 gating bug -- that file's own two tests are
//! ALSO expected to start failing once you read them closely: both of
//! its fixtures are exactly "local/imported enum, zero QuickFuncs
//! anywhere", which is precisely the case Phase 1 now eagerly resolves
//! unconditionally. It was written to pin the gating fix, and in doing
//! so accidentally pinned the pre-fix side effect of metadata survival
//! along with it. Worth a look before touching Phase 1 -- you may want
//! to update its assertions in the same pass as this fix, not before.)

use dixscript::Runtime::{DixLoader, DixValue, ExpectedValueType, SchemaBuilder};

const SRC: &str = r#"
@ENUMS(
  Status { ACTIVE = 1, INACTIVE = 0, PENDING = 2 }
)

@DATA(
  current_status<enum> = Status.PENDING
  app_name = "EnumBinaryApp"

  events::
  {
    kind = Status.ACTIVE,
    note = "first"
  },
  {
    kind = Status.INACTIVE,
    note = "second"
  }
)
"#;

/// Compiles `SRC` to plain binary-packed bytes (no `@DLM` compressor or
/// encryptor configured, so `key_file_content` is empty on the way back)
/// and round-trips it through `decompile_with_dlm_from_bytes` -- the
/// exact path a compiled `.mdixb` consumer (C#, Python, WASM, ...) goes
/// through.
fn roundtrip_through_binary() -> dixscript::Runtime::DixData {
    let loader = DixLoader::new();
    let packed = loader
        .compile_with_dlm_from_str(SRC, "enum_metadata_test")
        .expect("compile_with_dlm_from_str should succeed");
    assert!(packed.is_success, "pack stage failed: {:?}", packed.errors);

    loader
        .decompile_with_dlm_from_bytes(packed.processed_data, "", "enum_metadata_test")
        .expect("decompile_with_dlm_from_bytes should succeed")
}

#[test]
fn binary_roundtrip_preserves_top_level_enum_identity() {
    let data = roundtrip_through_binary();

    match data.get_value("current_status") {
        Some(DixValue::Enum { enum_name, field_name, value }) => {
            assert_eq!(enum_name, "Status");
            assert_eq!(field_name, "PENDING");
            assert_eq!(*value, 2);
        }
        other => panic!(
            "current_status: expected DixValue::Enum{{Status, PENDING, 2}} to survive \
             the binary round trip, got {:?} instead -- enum identity was silently \
             flattened to a bare integer during compile/pack (see module doc for root cause)",
            other
        ),
    }

    // Sanity: the sibling non-enum field is unaffected either way.
    assert_eq!(data.get::<String>("app_name").unwrap(), "EnumBinaryApp");
}

#[test]
fn binary_roundtrip_preserves_enum_identity_inside_group_array_objects() {
    let data = roundtrip_through_binary();

    match data.get_value("events[0].kind") {
        Some(DixValue::Enum { enum_name, field_name, value }) => {
            assert_eq!(enum_name, "Status");
            assert_eq!(field_name, "ACTIVE");
            assert_eq!(*value, 1);
        }
        other => panic!("events[0].kind: expected DixValue::Enum{{Status, ACTIVE, 1}}, got {:?}", other),
    }

    match data.get_value("events[1].kind") {
        Some(DixValue::Enum { enum_name, field_name, value }) => {
            assert_eq!(enum_name, "Status");
            assert_eq!(field_name, "INACTIVE");
            assert_eq!(*value, 0);
        }
        other => panic!("events[1].kind: expected DixValue::Enum{{Status, INACTIVE, 0}}, got {:?}", other),
    }
}

#[test]
fn the_underlying_integer_still_survives_even_though_identity_is_lost() {
    // This one PASSES today -- included to make the failure mode
    // explicit: this isn't a crash or a missing value, it's a silent
    // type downgrade. A consumer reading `current_status` as a plain int
    // gets the right number (2) with zero indication it was ever
    // `Status.PENDING`.
    let data = roundtrip_through_binary();
    let raw: i32 = data.get("current_status").unwrap();
    assert_eq!(raw, 2, "the numeric payload is intact -- only the type identity is lost");
}

#[test]
fn schema_validation_require_enum_accepts_a_real_enum_field() {
    // SchemaBuilder::require_enum() / ExpectedValueType::Enum only
    // matches DixValue::Enum{..} -- so this fails validation today for a
    // field the .mdix source correctly declared `<enum>` and assigned
    // from a real @ENUMS member. Any consumer using schema validation to
    // guard config loading is currently unable to require an enum field
    // at all -- it always reports WrongType.
    let data = roundtrip_through_binary();
    let report = data.validate_schema(SchemaBuilder::new().require_enum("current_status"));

    assert!(
        report.is_valid(),
        "expected current_status to satisfy ExpectedValueType::Enum, got: {}",
        report
    );
}

#[test]
fn schema_validation_reports_the_actual_wrong_type_today() {
    // Mirror of the test above, written the other direction: documents
    // exactly what WrongType error the schema validator produces right
    // now, so this test's own failure message stays informative even if
    // someone runs it before reading the rest of this file.
    let data = roundtrip_through_binary();
    let report = data.validate_schema(SchemaBuilder::new().require_enum("current_status"));

    if !report.is_valid() {
        let errors = report.errors_of_kind(&dixscript::Runtime::ValidationErrorKind::WrongType);
        assert!(
            errors.iter().any(|e| e.path == "current_status" && e.expected == ExpectedValueType::Enum.to_string()),
            "expected a WrongType error on 'current_status' explaining the enum requirement failed, got: {}",
            report
        );
    }
    // No assertion failure on its own if `report.is_valid()` -- once the
    // fix lands this branch is simply never taken, which is fine; the
    // point of this test is documentation, not additional coverage.
}
