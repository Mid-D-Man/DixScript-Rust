//! Tests for the dynamic-style `Index` access added to `DixValue` and
//! `DixData` (`Runtime/dix_value.rs`, `Runtime/dix_data.rs`) -- the
//! closest stable-Rust equivalent to chaining through a C# `dynamic`:
//! `data["timer"]["uop"].as_int()`.
//!
//! Key behavior under test: a missing key/index at any point in the chain
//! must return `DixValue::Null`, never panic -- `Index::index` can't return
//! `Option`, so this is the same "reference to a shared Null" trick
//! `serde_json::Value` uses.
//!
//! Run with:
//!   cargo test --test dynamic_value_access_regression -- --nocapture

use dixscript::Runtime::{DixData, DixLoader, DixLoadOptions, DixValue};

fn load(source: &str) -> DixData {
    let loader = DixLoader::new();
    loader
        .load_from_str(source, &DixLoadOptions::new())
        .expect("test source should compile")
}

#[test]
fn dixvalue_index_chains_through_nested_object() {
    let data = load(
        r#"
@DATA(
  timer:
    uop = 9
    label = "crack"
)
"#,
    );

    // "timer" is the aggregate key -> DixValue::Object
    let timer = &data["timer"];
    assert_eq!(timer["uop"].as_int(), Some(9));
    assert_eq!(timer["label"].as_str(), Some("crack"));

    // Chained straight off DixData: data["timer"]["uop"]
    assert_eq!(data["timer"]["uop"].as_int(), Some(9));
}

#[test]
fn dixvalue_index_on_array() {
    let data = load(
        r#"
@DATA(
  scores:: 10, 20, 30
)
"#,
    );

    let scores = &data["scores"];
    assert_eq!(scores[0].as_int(), Some(10));
    assert_eq!(scores[1].as_int(), Some(20));
    assert_eq!(scores[2].as_int(), Some(30));
}

#[test]
fn missing_keys_return_null_not_panic() {
    let data = load(
        r#"
@DATA(
  timer:
    uop = 9
)
"#,
    );

    // Missing key at every link in the chain -- none of this should panic.
    assert!(data["does_not_exist"].is_null());
    assert!(data["timer"]["also_missing"].is_null());
    assert!(data["timer"]["uop"]["cant_index_an_int"].is_null());
    assert!(data["timer"]["uop"][0].is_null()); // int isn't an array either

    // And the terminal extractor just comes back None, not a crash.
    assert_eq!(data["nope"]["still_nope"][99].as_int(), None);
}

#[test]
fn index_on_dixvalue_directly_not_just_dixdata() {
    let data = load(
        r#"
@DATA(
  outer:
    inner:: 1, 2, 3
)
"#,
    );

    // "outer" is a DixValue::Object whose "inner" entry is itself indexable.
    let outer: &DixValue = &data["outer"];
    assert_eq!(outer["inner"][1].as_int(), Some(2));
}

#[test]
fn as_int_unwraps_enum_values_directly() {
    let data = load(
        r#"
@ENUMS(
  Status { ACTIVE = 1, INACTIVE = 0 }
)
@DATA(
  current<enum> = Status.ACTIVE
)
"#,
    );

    // Ergonomic win: as_int() doesn't require knowing ahead of time whether
    // a field is a plain int or an enum wrapping one.
    assert_eq!(data["current"].as_int(), Some(1));

    // The richer accessor is still there for anyone who wants the name too.
    let (enum_name, field_name, value) = data["current"].as_enum().expect("should be an enum");
    assert_eq!(enum_name, "Status");
    assert_eq!(field_name, "ACTIVE");
    assert_eq!(value, 1);
}

#[test]
fn full_path_string_still_works_alongside_indexing() {
    let data = load(
        r#"
@DATA(
  timer:
    uop = 9
)
"#,
    );

    // The existing dotted-path API and the new Index sugar must agree.
    assert_eq!(data.get_value("timer.uop"), Some(&DixValue::Int(9)));
    assert_eq!(data["timer.uop"].as_int(), Some(9));
    assert_eq!(data["timer"]["uop"].as_int(), Some(9));
}
