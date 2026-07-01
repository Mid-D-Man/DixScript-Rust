"""Tests for MdixMerger — source-text merging, file merging,
conflict strategies, array strategies, and railway variants."""

import pytest
from midmanstudio.mdix import MdixDatabase, MdixMerger, MdixError

# ── Shared source constants ────────────────────────────────────────────────────

_BASE = '@DATA( app_name = "App", port = 8080, version = "1.0" )'
_PATCH = '@DATA( app_name = "Override", port = 9000, debug = true )'

# Non-conflicting sources — safe to merge with any strategy
_LEFT  = '@DATA( left_only = "yes", shared = "from-left" )'
_RIGHT = '@DATA( right_only = "yes", shared = "from-right" )'

# Array sources
_ARRAY_A = '@DATA( tags:: "alpha", "beta" )'
_ARRAY_B = '@DATA( tags:: "beta", "gamma" )'


class TestMergeStrings:
    """merge_strings — no disk I/O, takes (label, source, weight) triples."""

    def test_non_conflicting_keys_from_both_sources_survive(self):
        with MdixMerger().merge_strings([
            ("left",  _LEFT,  1.0),
            ("right", _RIGHT, 0.5),
        ]) as db:
            assert db.exists("left_only")
            assert db.exists("right_only")

    def test_primary_wins_on_conflict(self):
        with MdixMerger().with_strategy("primary_wins").merge_strings([
            ("base",    _BASE,  1.0),
            ("overlay", _PATCH, 0.5),
        ]) as db:
            assert db.get_string("app_name") == "App"
            assert db.get_int("port") == 8080

    def test_secondary_wins_on_conflict(self):
        with MdixMerger().with_strategy("secondary_wins").merge_strings([
            ("base",    _BASE,  1.0),
            ("overlay", _PATCH, 0.5),
        ]) as db:
            assert db.get_string("app_name") == "Override"
            assert db.get_int("port") == 9000

    def test_primary_wins_preserves_secondary_unique_keys(self):
        # Keys not in the primary source should always come through regardless
        # of conflict strategy.
        with MdixMerger().with_strategy("primary_wins").merge_strings([
            ("base",    _BASE,  1.0),
            ("overlay", _PATCH, 0.5),
        ]) as db:
            assert db.get_bool("debug") is True
            assert db.get_string("version") == "1.0"

    def test_weighted_higher_weight_wins_on_conflict(self):
        with MdixMerger().with_strategy("weighted_priority").merge_strings([
            ("high", '@DATA( port = 8080 )', 1.0),
            ("low",  '@DATA( port = 9000 )', 0.1),
        ]) as db:
            assert db.get_int("port") == 8080

    def test_weighted_reversed_weights(self):
        with MdixMerger().with_strategy("weighted_priority").merge_strings([
            ("low",  '@DATA( port = 8080 )', 0.1),
            ("high", '@DATA( port = 9000 )', 1.0),
        ]) as db:
            assert db.get_int("port") == 9000

    def test_throw_on_conflict_raises(self):
        with pytest.raises(MdixError):
            MdixMerger().with_strategy("throw_on_conflict").merge_strings([
                ("a", '@DATA( x = 1 )', 1.0),
                ("b", '@DATA( x = 2 )', 0.5),
            ])

    def test_throw_on_conflict_non_conflicting_succeeds(self):
        with MdixMerger().with_strategy("throw_on_conflict").merge_strings([
            ("a", '@DATA( x = 1 )', 1.0),
            ("b", '@DATA( y = 2 )', 0.5),
        ]) as db:
            assert db.get_int("x") == 1
            assert db.get_int("y") == 2

    def test_invalid_strategy_raises(self):
        with pytest.raises(MdixError, match="Unknown merge strategy"):
            MdixMerger().with_strategy("not_a_strategy")

    def test_invalid_array_strategy_raises(self):
        with pytest.raises(MdixError, match="Unknown array merge strategy"):
            MdixMerger().with_array_strategy("not_a_strategy")

    def test_empty_sources_raises(self):
        with pytest.raises(MdixError):
            MdixMerger().merge_strings([])

    def test_single_source_is_identity(self):
        with MdixMerger().merge_strings([
            ("only", _BASE, 1.0),
        ]) as db:
            assert db.get_string("app_name") == "App"
            assert db.get_int("port") == 8080

    def test_three_sources_all_keys_present(self):
        with MdixMerger().with_strategy("primary_wins").merge_strings([
            ("a", '@DATA( x = 1 )', 1.0),
            ("b", '@DATA( y = 2 )', 0.7),
            ("c", '@DATA( z = 3 )', 0.3),
        ]) as db:
            assert db.get_int("x") == 1
            assert db.get_int("y") == 2
            assert db.get_int("z") == 3

    def test_result_is_valid_database(self):
        db = MdixMerger().merge_strings([("a", _BASE, 1.0)])
        assert isinstance(db, MdixDatabase)
        assert db.is_valid
        db.close()

    def test_merge_strings_wrong_tuple_size_raises(self):
        with pytest.raises(MdixError):
            MdixMerger().merge_strings([("a", "source_only")])  # missing weight


class TestArrayMergeStrategies:

    def test_concat_keeps_duplicates(self):
        # Result should contain items from both arrays, including the shared "beta"
        with MdixMerger().with_array_strategy("concat").merge_strings([
            ("a", _ARRAY_A, 1.0),
            ("b", _ARRAY_B, 0.5),
        ]) as db:
            assert db.get_array_length("tags") == 4  # alpha, beta, beta, gamma

    def test_concat_dedup_removes_duplicates(self):
        with MdixMerger().with_array_strategy("concat_dedup").merge_strings([
            ("a", _ARRAY_A, 1.0),
            ("b", _ARRAY_B, 0.5),
        ]) as db:
            # alpha, beta, gamma — "beta" deduplicated
            assert db.get_array_length("tags") == 3

    def test_replace_keeps_only_one_sources_array(self):
        with MdixMerger().with_array_strategy("replace").merge_strings([
            ("a", _ARRAY_A, 1.0),
            ("b", _ARRAY_B, 0.5),
        ]) as db:
            # exactly one source's array (2 items), not concatenated
            assert db.get_array_length("tags") == 2

    def test_concat_dedup_with_no_overlap_equals_concat(self):
        a = '@DATA( nums:: 1, 2 )'
        b = '@DATA( nums:: 3, 4 )'
        with MdixMerger().with_array_strategy("concat_dedup").merge_strings([
            ("a", a, 1.0),
            ("b", b, 0.5),
        ]) as db:
            assert db.get_array_length("nums") == 4


class TestMergeFilesFromDisk:
    """merge_files / merge_files_weighted — actual filesystem paths."""

    def test_merge_files_two_paths(self, tmp_path):
        f1 = tmp_path / "a.mdix"
        f2 = tmp_path / "b.mdix"
        f1.write_text('@DATA( x = 10, shared = "from-a" )')
        f2.write_text('@DATA( y = 20, shared = "from-b" )')

        with MdixMerger().with_strategy("primary_wins").merge_files(
            [str(f1), str(f2)]
        ) as db:
            assert db.get_int("x") == 10
            assert db.get_int("y") == 20

    def test_merge_files_empty_list_raises(self):
        with pytest.raises(MdixError):
            MdixMerger().merge_files([])

    def test_merge_files_nonexistent_path_raises(self, tmp_path):
        with pytest.raises(MdixError):
            MdixMerger().merge_files(["/nonexistent/path.mdix"])

    def test_merge_files_weighted(self, tmp_path):
        high = tmp_path / "high.mdix"
        low  = tmp_path / "low.mdix"
        high.write_text('@DATA( port = 8080 )')
        low.write_text('@DATA( port = 9000, extra = true )')

        with MdixMerger().with_strategy("weighted_priority").merge_files_weighted([
            (str(high), 1.0),
            (str(low),  0.1),
        ]) as db:
            assert db.get_int("port") == 8080
            assert db.get_bool("extra") is True

    def test_merge_files_weighted_empty_list_raises(self):
        with pytest.raises(MdixError):
            MdixMerger().merge_files_weighted([])

    def test_merge_files_weighted_wrong_tuple_raises(self, tmp_path):
        p = tmp_path / "x.mdix"
        p.write_text("@DATA( x = 1 )")
        with pytest.raises(MdixError):
            MdixMerger().merge_files_weighted([(str(p),)])  # missing weight


class TestRailwayVariants:

    def test_try_merge_files_success(self, tmp_path):
        f = tmp_path / "ok.mdix"
        f.write_text("@DATA( x = 1 )")
        result = MdixMerger().try_merge_files([str(f)])
        assert result.is_success

    def test_try_merge_files_failure_bad_path(self):
        result = MdixMerger().try_merge_files(["/bad/path.mdix"])
        assert result.is_failure

    def test_try_merge_files_weighted_success(self, tmp_path):
        f = tmp_path / "ok.mdix"
        f.write_text("@DATA( x = 1 )")
        result = MdixMerger().try_merge_files_weighted([(str(f), 1.0)])
        assert result.is_success

    def test_try_merge_files_weighted_failure_bad_path(self):
        result = MdixMerger().try_merge_files_weighted([("/bad/path.mdix", 1.0)])
        assert result.is_failure

    def test_try_merge_result_value_is_database(self, tmp_path):
        f = tmp_path / "ok.mdix"
        f.write_text('@DATA( name = "Test" )')
        result = MdixMerger().try_merge_files([str(f)])
        db = result.unwrap()
        assert isinstance(db, MdixDatabase)
        assert db.get_string("name") == "Test"
        db.close()

    def test_try_railway_chain_on_success(self, tmp_path):
        f = tmp_path / "ok.mdix"
        f.write_text("@DATA( port = 8080 )")
        port = (MdixMerger()
                .try_merge_files([str(f)])
                .and_then(lambda db: db.try_get_int("port"))
                .unwrap_or(0))
        assert port == 8080


class TestMergeRepr:

    def test_repr_contains_strategy(self):
        r = repr(MdixMerger().with_strategy("primary_wins"))
        assert "primary_wins" in r.lower() or "PrimaryWins" in r

    def test_repr_contains_array_strategy(self):
        r = repr(MdixMerger().with_array_strategy("concat"))
        assert "concat" in r.lower() or "Concat" in r
