"""Tests for MdixDatabase.query() / query_many() / MdixQuery.

Fixture is deliberately the exact same `.mdix` source as the core crate's
`dixscript/tests/query_tests.rs` — same task list, same server statuses,
same expected counts/values — so these tests double as a direct
cross-check that the Python binding's independent (pure-Python-object,
callback-driven) query implementation agrees with the core's own
`DixData::query` / `DixQuery` behavior, not just that it runs.
"""

import pytest
from midmanstudio.mdix import MdixDatabase, MdixQuery, MdixError

SRC = """
@DATA(
  app_name = "QueryTestApp"

  tasks::
  {
    name = "Backup",
    priority = 3
  },
  {
    name = "Docs",
    priority = 1
  },
  {
    name = "Audit",
    priority = 3
  },
  {
    name = "Deploy",
    priority = 2
  }

  servers.web1:
  status = "up"

  servers.db1:
  status = "down"

  servers.web2:
  status = "up"
)
"""


@pytest.fixture
def db():
    d = MdixDatabase.load_str(SRC)
    yield d
    d.close()


class TestQueryEntryPoint:

    def test_query_returns_none_for_missing_path(self, db):
        assert db.query("does_not_exist") is None

    def test_query_returns_none_for_non_array_path(self, db):
        # app_name is a String, not an Array -- query() should refuse it,
        # not raise.
        assert db.query("app_name") is None

    def test_query_covers_group_array_items_via_base_path(self, db):
        tasks = db.query("tasks")
        assert tasks is not None
        assert tasks.count() == 4

    def test_query_empty_path_raises(self, db):
        with pytest.raises(MdixError):
            db.query("")


class TestWhere:

    def test_where_filters_group_array_items_by_field(self, db):
        high_priority = db.query("tasks").where_(lambda t: t["priority"] == 3)
        assert high_priority.count() == 2

    def test_where_field_eq_matches_the_plain_where_equivalent(self, db):
        a = db.query("tasks").where_(lambda t: t["priority"] == 3).count()
        b = db.query("tasks").where_field_eq("priority", 3).count()
        assert a == b == 2


class TestSelect:

    def test_select_projects_a_field_from_each_element(self, db):
        filtered = db.query("tasks").where_(lambda t: t["priority"] == 3)
        names = filtered.select(lambda t: t["name"])
        assert names == ["Backup", "Audit"]

    def test_select_field_is_a_shorthand_for_select_with_field(self, db):
        filtered = db.query("tasks").where_(lambda t: t["priority"] == 3)
        assert filtered.select_field("name") == ["Backup", "Audit"]


class TestOrdering:

    def test_order_by_desc_then_take_gets_the_top_result(self, db):
        # Stable sort -- Backup (index 0) and Audit (index 2) are tied at
        # priority 3, so Backup wins the tie by appearing first.
        top = db.query("tasks").order_by_desc(lambda t: t["priority"]).take(1)
        assert top.select(lambda t: t["name"]) == ["Backup"]

    def test_order_by_ascending_sorts_the_other_direction(self, db):
        bottom = db.query("tasks").order_by(lambda t: t["priority"]).take(1)
        assert bottom.select(lambda t: t["name"]) == ["Docs"]


class TestSkipTake:

    def test_skip_drops_the_leading_n_elements(self, db):
        assert db.query("tasks").skip(3).count() == 1

    def test_take_keeps_only_the_first_n(self, db):
        assert db.query("tasks").take(2).count() == 2


class TestGroupBy:

    def test_group_by_priority_groups_correctly_in_first_seen_order(self, db):
        groups = db.query("tasks").group_by(lambda t: t["priority"])

        # First-seen distinct priorities in fixture order: 3, 1, 2.
        assert len(groups) == 3
        assert groups[0][0] == 3
        assert groups[1][0] == 1
        assert groups[2][0] == 2
        assert len(groups[0][1]) == 2


class TestAnyAll:

    def test_any_and_all_over_a_query(self, db):
        tasks = db.query("tasks")
        assert tasks.any(lambda t: t["priority"] == 1)
        assert not tasks.all(lambda t: t["priority"] == 3)

    def test_empty_query_all_is_vacuously_true(self):
        empty = MdixQuery([])
        assert empty.all(lambda _: False)
        assert not empty.any(lambda _: True)


class TestFirstLastNth:

    def test_first_last_and_nth(self, db):
        tasks = db.query("tasks")
        assert tasks.first()["name"] == "Backup"
        assert tasks.last()["name"] == "Deploy"
        assert tasks.nth(1)["name"] == "Docs"

    def test_first_or_returns_default_when_empty(self):
        empty = MdixQuery([])
        assert empty.first() is None
        assert empty.first_or("fallback") == "fallback"


class TestQueryMany:

    def test_query_many_matches_sibling_wildcarded_paths(self, db):
        up_count = db.query_many("servers.*.status").where_(lambda s: s == "up").count()
        assert up_count == 2
        assert db.query_many("servers.*.status").count() == 3

    def test_query_many_empty_pattern_raises(self, db):
        with pytest.raises(MdixError):
            db.query_many("")


class TestDistinct:

    def test_distinct_removes_duplicates_preserving_first_seen_order(self):
        q = MdixQuery([1, 2, 2, 3, 1])
        deduped = q.distinct()
        assert deduped.to_list() == [1, 2, 3]


class TestAggregates:

    def test_sum_int_sum_float_and_avg_float(self, db):
        priorities = db.query("tasks").select(lambda t: t["priority"])
        assert sum(priorities) == 9  # 3 + 1 + 3 + 2

        raw = MdixQuery([2, 4, 6])
        assert raw.sum_int() == 12
        assert raw.sum_float() == 12.0
        assert raw.avg_float() == 4.0

    def test_avg_float_on_empty_is_none(self):
        assert MdixQuery([]).avg_float() is None


class TestMinMaxByKey:

    def test_min_by_key_and_max_by_key(self, db):
        tasks = db.query("tasks")
        cheapest = tasks.min_by_key(lambda t: t["priority"])
        priciest = tasks.max_by_key(lambda t: t["priority"])

        assert cheapest["name"] == "Docs"
        # Backup and Audit are tied at 3 -- max_by_key keeps the *last*
        # maximum (matches Rust's Iterator::max_by_key tie-breaking),
        # so Audit wins here, not Backup.
        assert priciest["name"] == "Audit"


class TestEmptyQueryTerminalOps:

    def test_empty_query_terminal_ops_dont_raise(self):
        empty = MdixQuery([])
        assert empty.is_empty
        assert empty.count() == 0
        assert empty.first() is None
        assert empty.sum_int() == 0
        assert empty.avg_float() is None
        assert not bool(empty)


class TestSequenceProtocol:

    def test_len_matches_count(self, db):
        tasks = db.query("tasks")
        assert len(tasks) == tasks.count() == 4

    def test_indexing_and_iteration(self, db):
        tasks = db.query("tasks")
        assert tasks[0]["name"] == "Backup"
        names = [t["name"] for t in tasks]
        assert names == ["Backup", "Docs", "Audit", "Deploy"]

    def test_out_of_range_index_raises(self, db):
        tasks = db.query("tasks")
        with pytest.raises(IndexError):
            _ = tasks[999]

    def test_bool_true_when_non_empty(self, db):
        assert bool(db.query("tasks"))
