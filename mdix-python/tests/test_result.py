"""Tests for MdixResult — railway-oriented programming."""

import pytest
from midmanstudio.mdix import MdixDatabase, MdixResult, MdixError


class TestConstruction:

    def test_ok_is_success(self):
        r = MdixResult.ok(42)
        assert r.is_success
        assert not r.is_failure

    def test_err_is_failure(self):
        r = MdixResult.err("something went wrong")
        assert r.is_failure
        assert not r.is_success

    def test_ok_bool_true(self):
        assert bool(MdixResult.ok(42))

    def test_err_bool_false(self):
        assert not bool(MdixResult.err("oops"))

    def test_ok_value_accessible(self):
        r = MdixResult.ok("hello")
        assert r.value == "hello"

    def test_err_message_accessible(self):
        r = MdixResult.err("not found")
        assert "not found" in r.error

    def test_value_on_failure_raises(self):
        r = MdixResult.err("fail")
        with pytest.raises(Exception):
            _ = r.value

    def test_error_on_success_raises(self):
        r = MdixResult.ok(1)
        with pytest.raises(Exception):
            _ = r.error


class TestUnwrapping:

    def test_or_raise_on_success_returns_value(self):
        assert MdixResult.ok(99).or_raise() == 99

    def test_or_raise_on_failure_raises_mdix_error(self):
        with pytest.raises(MdixError):
            MdixResult.err("gone").or_raise()

    def test_unwrap_alias_of_or_raise(self):
        assert MdixResult.ok("x").unwrap() == "x"

    def test_unwrap_or_success(self):
        assert MdixResult.ok(5).unwrap_or(-1) == 5

    def test_unwrap_or_failure(self):
        assert MdixResult.err("e").unwrap_or(-1) == -1

    def test_unwrap_or_else_factory_called_on_failure(self):
        calls = []
        MdixResult.err("msg").unwrap_or_else(lambda e: calls.append(e) or 0)
        assert calls == ["[mdix:get_int] msg"] or len(calls) == 1

    def test_unwrap_or_else_factory_not_called_on_success(self):
        calls = []
        MdixResult.ok(1).unwrap_or_else(lambda _: calls.append(1) or 0)
        assert calls == []


class TestTransformation:

    def test_map_transforms_success(self):
        result = MdixResult.ok(4).map(lambda v: v * 2)
        assert result.is_success
        assert result.value == 8

    def test_map_forwards_failure(self):
        result = MdixResult.err("fail").map(lambda v: v * 2)
        assert result.is_failure

    def test_and_then_chains_success(self):
        result = (MdixResult.ok(10)
                  .and_then(lambda v: MdixResult.ok(v * 3)))
        assert result.value == 30

    def test_and_then_short_circuits_failure(self):
        called = []
        result = (MdixResult.err("fail")
                  .and_then(lambda v: called.append(v) or MdixResult.ok(v)))
        assert result.is_failure
        assert called == []

    def test_ensure_passing_predicate(self):
        result = MdixResult.ok(10).ensure(lambda v: v > 0, "must be positive")
        assert result.is_success

    def test_ensure_failing_predicate(self):
        result = MdixResult.ok(-1).ensure(lambda v: v > 0, "must be positive")
        assert result.is_failure
        assert "positive" in result.error

    def test_ensure_on_failure_passthrough(self):
        result = MdixResult.err("original").ensure(lambda v: True, "irrelevant")
        assert "original" in result.error

    def test_or_returns_self_on_success(self):
        r1 = MdixResult.ok(1)
        r2 = MdixResult.ok(99)
        assert r1.or_(r2).value == 1

    def test_or_returns_fallback_on_failure(self):
        r1 = MdixResult.err("gone")
        r2 = MdixResult.ok(99)
        assert r1.or_(r2).value == 99


class TestBranching:

    def test_fold_success_branch(self):
        msg = MdixResult.ok(42).fold(
            on_success=lambda v: f"got {v}",
            on_failure=lambda e: f"err {e}",
        )
        assert msg == "got 42"

    def test_fold_failure_branch(self):
        msg = MdixResult.err("oops").fold(
            on_success=lambda v: "ok",
            on_failure=lambda e: f"failed: {e}",
        )
        assert "failed" in msg


class TestSideEffects:

    def test_tap_called_on_success(self):
        seen = []
        MdixResult.ok(7).tap(seen.append)
        assert seen == [7]

    def test_tap_not_called_on_failure(self):
        seen = []
        MdixResult.err("e").tap(seen.append)
        assert seen == []

    def test_tap_error_called_on_failure(self):
        seen = []
        MdixResult.err("boom").tap_error(seen.append)
        assert len(seen) == 1

    def test_tap_error_not_called_on_success(self):
        seen = []
        MdixResult.ok(1).tap_error(seen.append)
        assert seen == []


class TestDatabaseRailway:
    """Integration tests — try_* methods on real databases."""

    def test_try_load_str_success(self):
        result = MdixDatabase.try_load_str("@DATA( port = 8080 )")
        assert result.is_success

    def test_try_load_str_failure(self):
        result = MdixDatabase.try_load_str("")
        assert result.is_failure

    def test_chain_load_and_get(self):
        port = (MdixDatabase.try_load_str("@DATA( port = 8080 )")
                .and_then(lambda db: db.try_get_int("port"))
                .unwrap_or(0))
        assert port == 8080

    def test_chain_with_map(self):
        port_x2 = (MdixDatabase.try_load_str("@DATA( port = 4040 )")
                   .and_then(lambda db: db.try_get_int("port"))
                   .map(lambda p: p * 2)
                   .unwrap_or(0))
        assert port_x2 == 8080

    def test_chain_with_ensure(self):
        result = (MdixDatabase.try_load_str("@DATA( port = 80 )")
                  .and_then(lambda db: db.try_get_int("port"))
                  .ensure(lambda p: p > 1024, "port must be > 1024"))
        assert result.is_failure
        assert "1024" in result.error

    def test_repr_ok(self):
        r = MdixResult.ok(42)
        assert "ok" in repr(r).lower() or "42" in repr(r)

    def test_repr_err(self):
        r = MdixResult.err("gone")
        assert "err" in repr(r).lower() or "gone" in repr(r)
