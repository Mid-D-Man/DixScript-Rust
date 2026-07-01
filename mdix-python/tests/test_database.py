"""Tests for MdixDatabase — loading, reading, type inspection, and export."""

import json

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


class TestFileBasedLoading:
    """`load`/`try_load` take a filesystem path rather than a source string —
    a separate code path from `load_str` that was previously untested."""

    def test_load_from_file(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080, name = "FileApp" )')
        with MdixDatabase.load(str(p)) as db:
            assert db.get_int("port") == 8080
            assert db.get_string("name") == "FileApp"

    def test_load_empty_path_raises(self):
        with pytest.raises(MdixError):
            MdixDatabase.load("")

    def test_load_nonexistent_file_raises(self):
        with pytest.raises(MdixError):
            MdixDatabase.load("/nonexistent/path/does-not-exist.mdix")

    def test_try_load_from_file_success(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text("@DATA( port = 9090 )")
        result = MdixDatabase.try_load(str(p))
        assert result.is_success

    def test_try_load_nonexistent_file_failure(self):
        result = MdixDatabase.try_load("/nonexistent/path/does-not-exist.mdix")
        assert result.is_failure


class TestEncryptedLoadGuards:
    """A real encrypted round-trip needs a writer this binding doesn't
    expose (encryption happens via the CLI's `encrypt` command, not through
    mdix-python). These cover the boundary conditions the binding itself
    owns: empty-argument guards and missing-file handling."""

    def test_load_encrypted_empty_path_raises(self):
        with pytest.raises(MdixError):
            MdixDatabase.load_encrypted("")

    def test_load_encrypted_nonexistent_file_raises(self):
        with pytest.raises(MdixError):
            MdixDatabase.load_encrypted("/nonexistent/secret.mdix.enc")

    def test_load_encrypted_password_empty_path_raises(self):
        with pytest.raises(MdixError):
            MdixDatabase.load_encrypted_password("", "somepassword")

    def test_load_encrypted_password_empty_password_raises(self, tmp_path):
        p = tmp_path / "secret.mdix.enc"
        p.write_bytes(b"irrelevant")
        with pytest.raises(MdixError):
            MdixDatabase.load_encrypted_password(str(p), "")

    def test_load_encrypted_password_nonexistent_file_raises(self):
        with pytest.raises(MdixError):
            MdixDatabase.load_encrypted_password("/nonexistent/secret.mdix.enc", "pw")


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
        raw = flat_db.get_json("port")
        parsed = json.loads(raw)
        assert parsed == 8080


class TestTryGettersFullSet:
    """try_get_int was exercised indirectly via railway-chain tests in
    test_result.py; the rest of the typed try_get_* family had no direct
    coverage."""

    def test_try_get_string_success(self, flat_db):
        result = flat_db.try_get_string("app_name")
        assert result.is_success
        assert result.value == "TestApp"

    def test_try_get_string_missing_failure(self, flat_db):
        result = flat_db.try_get_string("missing")
        assert result.is_failure

    def test_try_get_float_success(self, flat_db):
        result = flat_db.try_get_float("rate")
        assert result.is_success
        assert abs(result.value - 1.5) < 0.001

    def test_try_get_double_success(self, flat_db):
        result = flat_db.try_get_double("score")
        assert result.is_success
        assert abs(result.value - 99.9) < 0.001

    def test_try_get_bool_success(self, flat_db):
        result = flat_db.try_get_bool("enabled")
        assert result.is_success
        assert result.value is True

    def test_try_get_json_success(self, flat_db):
        result = flat_db.try_get_json("port")
        assert result.is_success
        assert json.loads(result.value) == 8080

    def test_try_get_json_missing_failure(self, flat_db):
        result = flat_db.try_get_json("missing")
        assert result.is_failure


class TestEnumGetters:
    """get_enum_name / get_enum_field — and the enums_db fixture that
    defines them — had zero coverage anywhere in the suite."""

    def test_get_enum_name(self, enums_db):
        assert enums_db.get_enum_name("log_level") == "LogLevel"

    def test_get_enum_field(self, enums_db):
        assert enums_db.get_enum_field("log_level") == "INFO"

    def test_get_enum_name_second_enum(self, enums_db):
        assert enums_db.get_enum_name("status") == "Status"

    def test_get_enum_field_second_enum(self, enums_db):
        assert enums_db.get_enum_field("status") == "ACTIVE"

    def test_get_type_for_enum(self, enums_db):
        assert enums_db.get_type("log_level") == "enum"

    def test_get_enum_name_on_non_enum_raises(self, flat_db):
        with pytest.raises(MdixError):
            flat_db.get_enum_name("port")

    def test_get_enum_field_on_non_enum_raises(self, flat_db):
        with pytest.raises(MdixError):
            flat_db.get_enum_field("port")

    def test_get_enum_name_missing_path_raises(self, enums_db):
        with pytest.raises(MdixError):
            enums_db.get_enum_name("nonexistent")


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

    def test_get_type_array(self, array_db):
        assert array_db.get_type("tags") == "array"

    def test_get_type_object(self, nested_db):
        assert nested_db.get_type("server") == "object"

    def test_get_type_object_array_item(self, array_db):
        assert array_db.get_type("enemies[0]") == "object"

    def test_get_keys_top_level(self, flat_db):
        keys = flat_db.get_keys()
        assert "app_name" in keys
        assert "port" in keys

    def test_get_keys_nested_prefix(self, nested_db):
        keys = nested_db.get_keys("server")
        assert "host" in keys
        assert "port" in keys
        assert "ssl" in keys

    def test_get_array_length_array(self, array_db):
        assert array_db.get_array_length("tags") == 3

    def test_get_array_length_non_array(self, flat_db):
        assert flat_db.get_array_length("port") == -1

    def test_get_json_array(self, array_db):
        parsed = json.loads(array_db.get_json("tags"))
        assert parsed == ["alpha", "beta", "gamma"]

    def test_get_json_object(self, nested_db):
        parsed = json.loads(nested_db.get_json("server"))
        assert parsed["host"] == "localhost"
        assert parsed["port"] == 9000


class TestExport:

    def test_to_json_contains_values(self, flat_db):
        result = flat_db.to_json(indented=False)
        parsed = json.loads(result)
        assert parsed["port"] == 8080
        assert parsed["app_name"] == "TestApp"

    def test_to_json_indented_has_newlines(self, flat_db):
        assert "\n" in flat_db.to_json(indented=True)

    def test_to_json_preserves_nested_table(self, nested_db):
        result = json.loads(nested_db.to_json(indented=False))
        assert result["server"]["host"] == "localhost"
        assert result["server"]["port"] == 9000

    def test_to_json_preserves_group_array(self, array_db):
        result = json.loads(array_db.to_json(indented=False))
        assert result["tags"] == ["alpha", "beta", "gamma"]
        assert len(result["enemies"]) == 3

    def test_to_toml_contains_values(self, flat_db):
        result = flat_db.to_toml()
        assert "8080" in result
        assert "TestApp" in result

    def test_to_toml_preserves_nested_table(self, nested_db):
        result = nested_db.to_toml()
        assert "localhost" in result
        assert "9000" in result

    def test_to_json_then_from_json_roundtrip(self, flat_db):
        json_str = flat_db.to_json(indented=False)
        with MdixDatabase.from_json(json_str) as restored:
            assert restored.get_int("port") == flat_db.get_int("port")
            assert restored.get_string("app_name") == flat_db.get_string("app_name")

    def test_to_toml_then_from_toml_roundtrip(self, flat_db):
        toml_str = flat_db.to_toml()
        with MdixDatabase.from_toml(toml_str) as restored:
            assert restored.get_int("port") == flat_db.get_int("port")


class TestToMdixExport:
    """to_mdix had no coverage at all."""

    def test_to_mdix_contains_data_section(self, flat_db):
        result = flat_db.to_mdix()
        assert "@DATA(" in result
        assert "TestApp" in result

    def test_to_mdix_preserves_nested_table(self, nested_db):
        result = nested_db.to_mdix()
        assert "localhost" in result
        assert "9000" in result

    def test_to_mdix_preserves_group_array(self, array_db):
        result = array_db.to_mdix()
        assert "Goblin" in result
        assert "Dragon" in result

    def test_to_mdix_then_reload_roundtrip(self, flat_db):
        src = flat_db.to_mdix()
        with MdixDatabase.load_str(src) as restored:
            assert restored.get_int("port") == 8080
            assert restored.get_string("app_name") == "TestApp"
