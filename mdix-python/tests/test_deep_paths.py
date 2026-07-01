"""
Regression tests for deep / sibling dotted @DATA group paths.

These mirror the exact failure pattern found in mdix-scaffold's
generate_structure.py (2026-06-29): a template declaring a shallow group
("crates.midn-ecs") followed by a deeper group sharing the same prefix
("crates.midn-ecs.src"). The CLI's old `DixConverter::to_json` reconstructed
nested JSON objects from dotted paths and silently dropped the deeper group
whenever a shallower one already occupied that key (an Array can't become
an Object to hold a child, so the `if let Value::Object` match silently
failed).

`MdixDatabase` never goes through that reconstruction step. `DixData`'s
`flattened_data` is built once, directly, as a flat `HashMap<String,
DixValue>` (see `flatten_data_section` / `flatten_entry` in
`dixscript::Runtime::dix_data`). Every dotted path is its own independent
hashmap key — nothing is ever nested into a tree, so nothing can collide.
These tests exist to prove that property holds and to catch any future
change that reintroduces the tree-building step here.
"""

import json

import pytest
from midmanstudio.mdix import MdixDatabase


class TestSiblingAndDeepGroupsCoexist:
    """crates.alpha / crates.beta / crates.beta.src — the exact shape that
    broke the old `to_json` path."""

    def test_all_three_group_paths_exist(self, scaffold_like_db):
        assert scaffold_like_db.exists("crates.alpha")
        assert scaffold_like_db.exists("crates.beta")
        assert scaffold_like_db.exists("crates.beta.src")

    def test_shallow_sibling_unaffected_by_deep_group(self, scaffold_like_db):
        # crates.beta existing as an Array must not be disturbed by
        # crates.beta.src being declared afterwards in the same @DATA block.
        assert scaffold_like_db.get_type("crates.beta") == "array"
        assert scaffold_like_db.get_array_length("crates.beta") == 1

    def test_deep_group_is_not_dropped(self, scaffold_like_db):
        assert scaffold_like_db.get_type("crates.beta.src") == "array"
        assert scaffold_like_db.get_array_length("crates.beta.src") == 2

    def test_unrelated_sibling_group_unaffected(self, scaffold_like_db):
        assert scaffold_like_db.get_type("crates.alpha") == "array"
        assert scaffold_like_db.get_array_length("crates.alpha") == 2

    def test_top_level_scalar_unaffected_by_nested_groups(self, scaffold_like_db):
        assert scaffold_like_db.get_string("project_name") == "demo-core"

    def test_deep_group_items_have_correct_fields(self, scaffold_like_db):
        assert scaffold_like_db.get_string("crates.beta.src[0].name") == "lib"
        assert scaffold_like_db.get_string("crates.beta.src[0].ext")  == "rs"
        assert scaffold_like_db.get_string("crates.beta.src[1].name") == "main"

    def test_shallow_group_items_have_correct_fields(self, scaffold_like_db):
        assert scaffold_like_db.get_string("crates.beta[0].name") == "Cargo"
        assert scaffold_like_db.get_string("crates.beta[0].ext")  == "toml"

    def test_get_json_on_deep_group_round_trips(self, scaffold_like_db):
        raw = scaffold_like_db.get_json("crates.beta.src")
        parsed = json.loads(raw)
        assert len(parsed) == 2
        names = {item["name"] for item in parsed}
        assert names == {"lib", "main"}

    def test_get_json_on_shallow_group_unaffected_by_deep_sibling(self, scaffold_like_db):
        raw = scaffold_like_db.get_json("crates.beta")
        parsed = json.loads(raw)
        assert len(parsed) == 1
        assert parsed[0]["name"] == "Cargo"


class TestArrayItemFieldAccess:
    """Object-shaped items inside a GroupArray (the fc()/{} scaffold
    file-entry pattern) must be individually addressable by index + field,
    not just retrievable as an opaque blob."""

    def test_object_item_type_is_object(self, array_db):
        assert array_db.get_type("enemies[0]") == "object"

    def test_first_item_field(self, array_db):
        assert array_db.get_string("enemies[0].name") == "Goblin"
        assert array_db.get_int("enemies[0].hp") == 50

    def test_second_item_field(self, array_db):
        assert array_db.get_string("enemies[1].name") == "Orc"
        assert array_db.get_int("enemies[1].hp") == 100

    def test_third_item_field(self, array_db):
        assert array_db.get_string("enemies[2].name") == "Dragon"
        assert array_db.get_string("enemies[2].ai")   == "BOSS"

    def test_out_of_range_index_does_not_exist(self, array_db):
        assert array_db.exists("enemies[99]") is False
        assert array_db.get_type("enemies[99]") == "unknown"


# Three levels deep (a.b.c) — one level deeper than the original bug report,
# to make sure the fix isn't accidentally depth-limited to exactly two.
THREE_LEVEL_SOURCE = """
@DATA(
  a::
    { name = "top" }

  a.b::
    { name = "mid" }

  a.b.c::
    { name = "bottom" },
    { name = "bottom2" }
)
"""


@pytest.fixture
def three_level_db():
    db = MdixDatabase.load_str(THREE_LEVEL_SOURCE)
    yield db
    db.close()


class TestThreeLevelsDeep:

    def test_all_three_levels_survive(self, three_level_db):
        assert three_level_db.get_array_length("a")     == 1
        assert three_level_db.get_array_length("a.b")   == 1
        assert three_level_db.get_array_length("a.b.c") == 2

    def test_deepest_level_field_access(self, three_level_db):
        assert three_level_db.get_string("a.b.c[1].name") == "bottom2"
