"""Tests for MdixDatabase — loading, reading, type inspection, and export."""

import pytest
from midmanstudio.mdix import MdixDatabase, MdixError


class TestLoading:

    def test_load_str_valid_source_succeeds(self):
        db = MdixDatabase.load_str("@DATA( x = 1 )")
        assert db.is_valid
        db.close()

    def test_load_str_empty_raises(self):
        with pytest.raises(MdixError):
            MdixDatabase.load_str("")

    def test_load_str_malformed_raises(self):
        with pytest.raises(MdixError):
            MdixDatabase.load_str("@@@INVALID###")

    def test_context_manager_closes_on_exit(self):
        with MdixDatabase.load_str("@DATA( x = 1 )") as db:
            assert db.is_valid
        assert not db.is_valid

    def test_close_is_idempotent(self):
        db = MdixDatabase.load_str("@DATA( x = 1 )")
        db.close()
        db.close()  # must not raise

    def test_entry_count_positive(self):
        db = MdixDatabase.load_str("@DATA( a = 1, b = 2, c = 3 )")
        assert db.entry_count > 0
        db.close()

    def test_from_json_valid_object(self):
        import json
        payload = json.dumps({"port": 8080, "host": "localhost"})
        with MdixDatabase.from_json(payload) as db:
            assert db.get_int("port") == 8080
            assert db.get_string("host") == "localhost"

    def test_from_json_empty_raises(self):
        with pytest.raises(MdixError):
            MdixDatabase.from_json("")

    def test_from_json_array_toplevel_raises(self):
        with pytest.raises(MdixError):
            MdixDatabase.from_json("[1, 2, 3]")

    def test_from_toml_valid(self):
        with MdixDatabase.from_toml('port = 8080\nhost = "localhost"\n') as db:
            assert db.get_int("port") == 8080
            assert db.get_string("host") == "localhost"

    def test_from_toml_empty_raises(self):
        with pytest.raises(MdixError):
            MdixDatabase.from_toml("")


class TestTypedGetters:

    def test_get_string_known_path(self, flat_db):
        assert flat_db.get_string("app_name") == "TestApp"

    def test_get_string_with_default(self, flat_db):
        assert flat_db.get_string("missing", "fallback") == "fallback"

    def test_get_string_missing_raises(self, flat_db):
        with pytest.raises(MdixError):
            flat_db.get_string("missing")

    def test_get_int_known_path(self, flat_db):
        assert flat_db.get_int("port") == 8080

    def test_get_int_with_default(self, flat_db):
        assert flat_db.get_int("missing", 42) == 42

    def test_get_bool_true(self, flat_db):
        assert flat_db.get_bool("enabled") is True

    def test_get_float_known(self, flat_db):
        assert abs(flat_db.get_float("rate") - 1.5) < 0.001

    def test_get_double_known(self, flat_db):
        assert abs(flat_db.get_double("score") - 99.9) < 0.001

    def test_get_string_empty_path_raises(self, flat_db):
        with pytest.raises(MdixError):
            flat_db.get_string("")

    def test_nested_dotted_path(self, nested_db):
        assert nested_db.get_string("server.host") == "localhost"
        assert nested_db.get_int("server.port") == 9000
        assert nested_db.get_bool("server.ssl") is True

    def test_get_json_returns_valid_json(self, flat_db):
        import json
        raw = flat_db.get_json("port")
        parsed = json.loads(raw)
        assert parsed == 8080


class TestTypeInspection:

    def test_exists_present(self, flat_db):
        assert flat_db.exists("port") is True

    def test_exists_absent(self, flat_db):
        assert flat_db.exists("nonexistent") is False

    def test_get_type_int(self, flat_db):
        assert flat_db.get_type("port") == "int"

    def test_get_type_string(self, flat_db):
        assert flat_db.get_type("app_name") == "string"

    def test_get_type_bool(self, flat_db):
        assert flat_db.get_type("enabled") == "bool"

    def test_get_type_unknown(self, flat_db):
        assert flat_db.get_type("nonexistent") == "unknown"

    def test_get_keys_top_level(self, flat_db):
        keys = flat_db.get_keys()
        assert "app_name" in keys
        assert "port" in keys

    def test_get_array_length_array(self, array_db):
        assert array_db.get_array_length("tags") == 3

    def test_get_array_length_non_array(self, flat_db):
        assert flat_db.get_array_length("port") == -1


class TestExport:

    def test_to_json_contains_values(self, flat_db):
        import json
        result = flat_db.to_json(indented=False)
        parsed = json.loads(result)
        assert parsed["port"] == 8080
        assert parsed["app_name"] == "TestApp"

    def test_to_json_indented_has_newlines(self, flat_db):
        assert "\n" in flat_db.to_json(indented=True)

    def test_to_toml_contains_values(self, flat_db):
        result = flat_db.to_toml()
        assert "8080" in result
        assert "TestApp" in result

    def test_to_json_then_from_json_roundtrip(self, flat_db):
        json_str = flat_db.to_json(indented=False)
        with MdixDatabase.from_json(json_str) as restored:
            assert restored.get_int("port") == flat_db.get_int("port")
            assert restored.get_string("app_name") == flat_db.get_string("app_name")

    def test_to_toml_then_from_toml_roundtrip(self, flat_db):
        toml_str = flat_db.to_toml()
        with MdixDatabase.from_toml(toml_str) as restored:
            assert restored.get_int("port") == flat_db.get_int("port")
