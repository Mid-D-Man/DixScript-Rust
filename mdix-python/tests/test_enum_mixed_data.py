"""
Enum-plus-mixed-data coverage for MdixDatabase.

`TestEnumGetters` in test_database.py already covers get_enum_name() /
get_enum_field() in isolation -- positive, non-enum, and missing-path
cases. What it never does is call get_int() against an enum path to check
the *resolved* integer. That matters because dixscript's AST->DixValue
resolver (Runtime/dix_value.rs, ast_value_to_dix_value) silently falls
back to 0 on an enum-table lookup miss:

    let resolved = enums
        .and_then(|e| e.get(enum_name.as_str()))
        .and_then(|fields| fields.get(field_name.as_str()))
        .copied()
        .unwrap_or(0);

The existing enums_db fixture in conftest.py happens to use Status.ACTIVE
(= 1, not the conventional 0) and LogLevel.INFO -- decent -- but nothing
in this repo currently exercises get_int() on either path, so a silent
fallback-to-0 bug would ship undetected. This file deliberately builds a
fixture where the "wrong" (fallback) answer and the "right" answer are
different small integers, so a mismatch is impossible to miss:

  - Status.PENDING = 2   (not 0, not 1 -- three ways to be wrong)
  - Role.EDITOR    = 1   (nested inside a permissions:: group array,
                          since that's the exact spot nested-path
                          resolution broke before, per the
                          mdix-scaffold GroupArray regression)

Adjust the import path below if `midmanstudio.mdix` differs from what's
actually installed in your environment.
"""
import pytest
from midmanstudio.mdix import MdixDatabase


SOURCE = """
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
"""


@pytest.fixture
def db():
    database = MdixDatabase.load_str(SOURCE)
    yield database
    database.close()


class TestEnumAlongsideMixedData:
    """An enum field sitting next to ordinary sibling fields on the same
    table property -- the exact "enum with the other data it holds"
    scenario."""

    def test_sibling_fields_are_unaffected_by_the_enum_field(self, db):
        assert db.get_string("user.name") == "Alice"
        assert db.get_int("user.age") == 30
        assert db.get_double("user.score") == pytest.approx(98.5)
        assert db.get_bool("user.active") is True
        assert db.get_array_length("user.tags") == 2

    def test_enum_field_resolves_name_field_and_value_together(self, db):
        assert db.get_enum_name("user.status") == "Status"
        assert db.get_enum_field("user.status") == "PENDING"
        # The important one: PENDING is declared as 2. If the enum table
        # lookup ever silently misses, this comes back 0 instead -- and
        # 0 is ACTIVE, a different, valid-looking variant. That's the
        # bug this assertion exists to catch.
        assert db.get_int("user.status") == 2


class TestEnumInsideGroupArray:
    """Enum field nested inside a permissions:: group array element --
    combines the enum-resolution path with the nested-path resolution
    that had a real regression in mdix-scaffold."""

    def test_first_element_resolves_independently(self, db):
        assert db.get_enum_name("user.permissions[0].role") == "Role"
        assert db.get_enum_field("user.permissions[0].role") == "EDITOR"
        assert db.get_int("user.permissions[0].role") == 1
        assert db.get_string("user.permissions[0].scope") == "team"

    def test_second_element_resolves_independently(self, db):
        assert db.get_enum_field("user.permissions[1].role") == "ADMIN"
        assert db.get_int("user.permissions[1].role") == 0
        assert db.get_string("user.permissions[1].scope") == "global"

    def test_top_level_and_nested_enum_fields_do_not_cross_contaminate(self, db):
        top_level_field = db.get_enum_field("user.status")
        nested_field    = db.get_enum_field("user.permissions[0].role")
        assert top_level_field != nested_field
        assert db.get_enum_name("user.status") != db.get_enum_name("user.permissions[0].role")
