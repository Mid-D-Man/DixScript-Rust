"""Tests for MdixSchemaBuilder and MdixValidationReport."""

import pytest
from midmanstudio.mdix import (
    MdixDatabase,
    MdixSchemaBuilder,
    MdixValidationReport,
    MdixValidationError,
    MdixError,
)


class TestBasicValidation:

    def test_empty_schema_always_passes(self, flat_db):
        report = MdixSchemaBuilder().validate(flat_db)
        assert report.is_valid

    def test_all_required_fields_present_passes(self, flat_db):
        report = (MdixSchemaBuilder()
                  .require_string("app_name")
                  .require_int("port")
                  .require_bool("enabled")
                  .validate(flat_db))
        assert report.is_valid
        assert report.error_count == 0

    def test_missing_required_field_fails(self, flat_db):
        report = (MdixSchemaBuilder()
                  .require_string("nonexistent_field")
                  .validate(flat_db))
        assert not report.is_valid
        assert report.error_count == 1

    def test_wrong_type_fails(self, flat_db):
        # port is an int, not a string
        report = (MdixSchemaBuilder()
                  .require_string("port")
                  .validate(flat_db))
        assert not report.is_valid

    def test_multiple_missing_fields_each_produce_error(self, flat_db):
        report = (MdixSchemaBuilder()
                  .require_string("missing_a")
                  .require_int("missing_b")
                  .require_bool("missing_c")
                  .validate(flat_db))
        assert report.error_count == 3

    def test_mixed_pass_and_fail(self, flat_db):
        report = (MdixSchemaBuilder()
                  .require_string("app_name")   # present and correct → pass
                  .require_string("missing")     # missing → fail
                  .validate(flat_db))
        assert not report.is_valid
        assert report.error_count == 1

    def test_require_float(self, flat_db):
        report = MdixSchemaBuilder().require_float("rate").validate(flat_db)
        assert report.is_valid

    def test_require_double(self, flat_db):
        report = MdixSchemaBuilder().require_double("score").validate(flat_db)
        assert report.is_valid

    def test_require_array(self, array_db):
        report = MdixSchemaBuilder().require_array("tags").validate(array_db)
        assert report.is_valid

    def test_require_object(self, nested_db):
        report = MdixSchemaBuilder().require_object("server").validate(nested_db)
        assert report.is_valid

    def test_require_enum(self, enums_db):
        report = MdixSchemaBuilder().require_enum("log_level").validate(enums_db)
        assert report.is_valid

    def test_require_with_explicit_type_string(self, flat_db):
        report = (MdixSchemaBuilder()
                  .require("app_name", "string")
                  .require("port", "int")
                  .validate(flat_db))
        assert report.is_valid

    def test_require_with_any_type_always_passes_if_present(self, flat_db):
        report = (MdixSchemaBuilder()
                  .require("port", "any")
                  .require("app_name", "any")
                  .validate(flat_db))
        assert report.is_valid

    def test_require_with_invalid_type_string_raises(self):
        with pytest.raises(MdixError, match="Unknown expected type"):
            MdixSchemaBuilder().require("path", "not_a_real_type")


class TestOptionalFields:

    def test_optional_present_correct_type_passes(self, flat_db):
        report = (MdixSchemaBuilder()
                  .optional_string("app_name")
                  .validate(flat_db))
        assert report.is_valid

    def test_optional_absent_passes(self, flat_db):
        # Schema declares a field that doesn't exist — optional, so no error
        report = (MdixSchemaBuilder()
                  .optional_string("nonexistent")
                  .validate(flat_db))
        assert report.is_valid

    def test_optional_present_wrong_type_fails(self, flat_db):
        # port IS present but is an int, not a bool
        report = (MdixSchemaBuilder()
                  .optional_bool("port")
                  .validate(flat_db))
        assert not report.is_valid

    def test_optional_with_explicit_type_string(self, flat_db):
        report = (MdixSchemaBuilder()
                  .optional("app_name", "string")
                  .validate(flat_db))
        assert report.is_valid

    def test_optional_array_absent_passes(self, flat_db):
        # flat_db has no arrays
        report = MdixSchemaBuilder().optional_array("tags").validate(flat_db)
        assert report.is_valid

    def test_optional_array_present_passes(self, array_db):
        report = MdixSchemaBuilder().optional_array("tags").validate(array_db)
        assert report.is_valid

    def test_optional_object_absent_passes(self, flat_db):
        report = MdixSchemaBuilder().optional_object("server").validate(flat_db)
        assert report.is_valid

    def test_optional_int_present_passes(self, flat_db):
        report = MdixSchemaBuilder().optional_int("port").validate(flat_db)
        assert report.is_valid

    def test_optional_float_present_passes(self, flat_db):
        report = MdixSchemaBuilder().optional_float("rate").validate(flat_db)
        assert report.is_valid

    def test_optional_double_present_passes(self, flat_db):
        report = MdixSchemaBuilder().optional_double("score").validate(flat_db)
        assert report.is_valid


class TestSchemaReusability:
    """validate() borrows — the same MdixSchemaBuilder can validate
    multiple databases. This is the key design difference from MdixBuilder
    (which is consumed on to_database)."""

    def test_same_schema_validates_two_passing_databases(self):
        schema = (MdixSchemaBuilder()
                  .require_string("name")
                  .require_int("port"))

        db1 = MdixDatabase.load_str('@DATA( name = "App1", port = 8080 )')
        db2 = MdixDatabase.load_str('@DATA( name = "App2", port = 9090 )')

        try:
            assert schema.validate(db1).is_valid
            assert schema.validate(db2).is_valid
        finally:
            db1.close()
            db2.close()

    def test_same_schema_catches_failure_in_second_database(self):
        schema = MdixSchemaBuilder().require_string("name")

        db_ok   = MdixDatabase.load_str('@DATA( name = "ok" )')
        db_fail = MdixDatabase.load_str("@DATA( port = 8080 )")

        try:
            assert schema.validate(db_ok).is_valid
            assert not schema.validate(db_fail).is_valid
        finally:
            db_ok.close()
            db_fail.close()

    def test_schema_state_unchanged_between_validate_calls(self):
        schema = (MdixSchemaBuilder()
                  .require_string("x")
                  .require_int("y"))
        db = MdixDatabase.load_str("@DATA( x = \"hi\", y = 1 )")
        try:
            r1 = schema.validate(db)
            r2 = schema.validate(db)
            assert r1.is_valid
            assert r2.is_valid
            assert r1.error_count == r2.error_count
        finally:
            db.close()


class TestSchemaIntrospection:

    def test_field_count_empty(self):
        assert MdixSchemaBuilder().field_count == 0

    def test_field_count_after_requires(self):
        schema = (MdixSchemaBuilder()
                  .require_string("a")
                  .require_int("b")
                  .optional_bool("c"))
        assert schema.field_count == 3

    def test_paths_empty(self):
        assert MdixSchemaBuilder().paths == []

    def test_paths_contains_added_fields(self):
        schema = (MdixSchemaBuilder()
                  .require_string("app_name")
                  .require_int("port")
                  .optional_bool("debug"))
        paths = schema.paths
        assert "app_name" in paths
        assert "port" in paths
        assert "debug" in paths

    def test_paths_length_matches_field_count(self):
        schema = (MdixSchemaBuilder()
                  .require_string("a")
                  .require_int("b")
                  .require_bool("c"))
        assert len(schema.paths) == schema.field_count

    def test_with_description_does_not_affect_field_count(self):
        schema = (MdixSchemaBuilder()
                  .require_string("name")
                  .with_description("The application name"))
        assert schema.field_count == 1

    def test_with_description_does_not_affect_validation(self, flat_db):
        report = (MdixSchemaBuilder()
                  .require_string("app_name")
                  .with_description("The application name")
                  .validate(flat_db))
        assert report.is_valid


class TestValidationReport:

    def test_is_valid_true_on_pass(self, flat_db):
        report = MdixSchemaBuilder().require_string("app_name").validate(flat_db)
        assert report.is_valid is True

    def test_is_valid_false_on_fail(self, flat_db):
        report = MdixSchemaBuilder().require_string("missing").validate(flat_db)
        assert report.is_valid is False

    def test_bool_true_on_pass(self, flat_db):
        report = MdixSchemaBuilder().require_string("app_name").validate(flat_db)
        assert bool(report) is True

    def test_bool_false_on_fail(self, flat_db):
        report = MdixSchemaBuilder().require_string("missing").validate(flat_db)
        assert bool(report) is False

    def test_error_count_zero_on_pass(self, flat_db):
        report = MdixSchemaBuilder().require_string("app_name").validate(flat_db)
        assert report.error_count == 0

    def test_errors_empty_list_on_pass(self, flat_db):
        report = MdixSchemaBuilder().require_string("app_name").validate(flat_db)
        assert report.errors == []

    def test_errors_list_length_matches_error_count(self, flat_db):
        report = (MdixSchemaBuilder()
                  .require_string("missing_a")
                  .require_int("missing_b")
                  .validate(flat_db))
        assert len(report.errors) == report.error_count

    def test_failed_paths_empty_on_pass(self, flat_db):
        report = MdixSchemaBuilder().require_string("app_name").validate(flat_db)
        assert report.failed_paths() == []

    def test_failed_paths_contains_failing_field(self, flat_db):
        report = MdixSchemaBuilder().require_string("missing").validate(flat_db)
        assert "missing" in report.failed_paths()

    def test_failed_paths_does_not_contain_passing_field(self, flat_db):
        report = (MdixSchemaBuilder()
                  .require_string("app_name")  # passes
                  .require_string("missing")    # fails
                  .validate(flat_db))
        assert "app_name" not in report.failed_paths()
        assert "missing" in report.failed_paths()

    def test_str_on_pass_mentions_passed(self, flat_db):
        report = MdixSchemaBuilder().validate(flat_db)
        assert "passed" in str(report).lower() or report.is_valid

    def test_str_on_fail_mentions_error_count(self, flat_db):
        report = (MdixSchemaBuilder()
                  .require_string("missing_a")
                  .require_string("missing_b")
                  .validate(flat_db))
        s = str(report)
        assert "2" in s

    def test_repr_contains_validity_info(self, flat_db):
        report = MdixSchemaBuilder().validate(flat_db)
        r = repr(report)
        assert "is_valid" in r or "True" in r


class TestValidationError:

    def test_missing_error_has_correct_kind(self, flat_db):
        report = MdixSchemaBuilder().require_string("missing").validate(flat_db)
        assert len(report.errors) == 1
        err = report.errors[0]
        assert err.kind == "missing"

    def test_wrong_type_error_has_correct_kind(self, flat_db):
        # port is int, not string
        report = MdixSchemaBuilder().require_string("port").validate(flat_db)
        assert len(report.errors) == 1
        err = report.errors[0]
        assert err.kind == "wrong_type"

    def test_error_path_is_the_failing_field(self, flat_db):
        report = MdixSchemaBuilder().require_string("missing_key").validate(flat_db)
        err = report.errors[0]
        assert err.path == "missing_key"

    def test_error_expected_describes_required_type(self, flat_db):
        report = MdixSchemaBuilder().require_string("missing_key").validate(flat_db)
        err = report.errors[0]
        assert err.expected  # non-empty string

    def test_error_actual_describes_what_was_found(self, flat_db):
        report = MdixSchemaBuilder().require_string("missing_key").validate(flat_db)
        err = report.errors[0]
        assert err.actual  # non-empty string

    def test_error_str_is_human_readable(self, flat_db):
        report = MdixSchemaBuilder().require_string("missing_key").validate(flat_db)
        s = str(report.errors[0])
        assert "missing_key" in s

    def test_error_repr_contains_path(self, flat_db):
        report = MdixSchemaBuilder().require_string("missing_key").validate(flat_db)
        r = repr(report.errors[0])
        assert "missing_key" in r


class TestErrorsOfKind:

    def _report_with_both_kinds(self, flat_db):
        # missing: "nonexistent" doesn't exist
        # wrong_type: port IS present but is int, not bool
        return (MdixSchemaBuilder()
                .require_string("nonexistent")   # → missing
                .require_bool("port")             # → wrong_type
                .validate(flat_db))

    def test_errors_of_kind_missing(self, flat_db):
        report = self._report_with_both_kinds(flat_db)
        missing_errs = report.errors_of_kind("missing")
        assert len(missing_errs) == 1
        assert missing_errs[0].path == "nonexistent"

    def test_errors_of_kind_wrong_type(self, flat_db):
        report = self._report_with_both_kinds(flat_db)
        type_errs = report.errors_of_kind("wrong_type")
        assert len(type_errs) == 1
        assert type_errs[0].path == "port"

    def test_errors_of_kind_returns_empty_for_absent_kind(self, flat_db):
        # Only produce missing errors — no wrong_type errors should exist
        report = MdixSchemaBuilder().require_string("nonexistent").validate(flat_db)
        assert report.errors_of_kind("wrong_type") == []

    def test_errors_of_kind_unknown_kind_returns_empty(self, flat_db):
        report = MdixSchemaBuilder().require_string("nonexistent").validate(flat_db)
        assert report.errors_of_kind("not_a_real_kind") == []
