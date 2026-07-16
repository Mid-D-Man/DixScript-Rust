//! mdix-wasm has no enum coverage at all yet -- tests/web.rs never
//! touches get_enum_name/get_enum_field/get_int-on-an-enum-path. This
//! file closes that gap directly, mirroring the equivalent Python
//! coverage added in mdix-python/tests/test_enum_mixed_data.py so the
//! same fixture/assertions exist on both sides of the binding.
//!
//! Runs in a real headless browser via `wasm-pack test --headless
//! --chrome` (see .github/workflows/wasm-test.yml), same as web.rs.
//!
//! Why PENDING = 2 and EDITOR = 1 specifically: dixscript's AST resolver
//! (Runtime/dix_value.rs, ast_value_to_dix_value) falls back to 0 on an
//! enum-table lookup miss. Picking non-zero, non-default variants means
//! that failure mode shows up as an obvious wrong number instead of
//! hiding behind a coincidentally-correct 0.

use wasm_bindgen_test::*;
use mdix_wasm::MdixDatabase;

wasm_bindgen_test_configure!(run_in_browser);

const SOURCE: &str = r#"
@ENUMS(
  Status { ACTIVE = 1, INACTIVE = 0, PENDING = 2, ARCHIVED = 3 }
  Role   { ADMIN = 0, EDITOR = 1, VIEWER = 2 }
)
@DATA(
  app = "enum-mixed-data-fixture"

  user:
    name = "Alice",
    age<int> = 30,
    score<double> = 98.5,
    active<bool> = true,
    tags = ["admin", "verified"],
    status<enum> = Status.PENDING

  user.permissions::
    { role<enum> = Role.EDITOR, scope = "team" },
    { role<enum> = Role.ADMIN,  scope = "global" }
)
"#;

// ── Enum alongside sibling fields on the same table property ──────────────

#[wasm_bindgen_test]
fn sibling_fields_are_unaffected_by_the_enum_field() {
    let db = MdixDatabase::load_str(SOURCE).expect("fixture should parse");

    assert_eq!(db.get_string("user.name").unwrap(), "Alice");
    assert_eq!(db.get_int("user.age").unwrap(), 30);
    assert!((db.get_double("user.score").unwrap() - 98.5).abs() < 1e-9);
    assert_eq!(db.get_bool("user.active").unwrap(), true);
    assert_eq!(db.get_array_length("user.tags").unwrap(), 2);
}

#[wasm_bindgen_test]
fn enum_field_resolves_name_field_and_value_together() {
    let db = MdixDatabase::load_str(SOURCE).expect("fixture should parse");

    assert_eq!(db.get_enum_name("user.status").unwrap(), "Status");
    assert_eq!(db.get_enum_field("user.status").unwrap(), "PENDING");
    // PENDING is declared as 2. A silent enum-table lookup-miss falls
    // back to 0, which happens to be a different, valid-looking variant
    // (ACTIVE) -- this assertion is what would actually catch that.
    assert_eq!(db.get_int("user.status").unwrap(), 2);
}

// ── Enum nested inside a permissions:: group array element ────────────────

#[wasm_bindgen_test]
fn first_group_array_element_resolves_independently() {
    let db = MdixDatabase::load_str(SOURCE).expect("fixture should parse");

    assert_eq!(db.get_enum_name("user.permissions[0].role").unwrap(), "Role");
    assert_eq!(db.get_enum_field("user.permissions[0].role").unwrap(), "EDITOR");
    assert_eq!(db.get_int("user.permissions[0].role").unwrap(), 1);
    assert_eq!(db.get_string("user.permissions[0].scope").unwrap(), "team");
}

#[wasm_bindgen_test]
fn second_group_array_element_resolves_independently() {
    let db = MdixDatabase::load_str(SOURCE).expect("fixture should parse");

    assert_eq!(db.get_enum_field("user.permissions[1].role").unwrap(), "ADMIN");
    assert_eq!(db.get_int("user.permissions[1].role").unwrap(), 0);
    assert_eq!(db.get_string("user.permissions[1].scope").unwrap(), "global");
}

#[wasm_bindgen_test]
fn top_level_and_nested_enum_fields_do_not_cross_contaminate() {
    let db = MdixDatabase::load_str(SOURCE).expect("fixture should parse");

    let top_level_field = db.get_enum_field("user.status").unwrap();
    let nested_field = db.get_enum_field("user.permissions[0].role").unwrap();
    assert_ne!(top_level_field, nested_field);

    let top_level_name = db.get_enum_name("user.status").unwrap();
    let nested_name = db.get_enum_name("user.permissions[0].role").unwrap();
    assert_ne!(top_level_name, nested_name);
}
