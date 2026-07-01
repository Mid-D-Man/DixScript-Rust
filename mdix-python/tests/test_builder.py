"""Tests for MdixBuilder — two-tier ordering, all value types, and finalization."""

import pytest
from midmanstudio.mdix import MdixBuilder, MdixDatabase, MdixError


class TestTwoTierOrdering:

    def test_flat_before_grouped_is_valid(self):
        db = (MdixBuilder()
              .set_string("name", "App")
              .set_int("port", 8080)
              .with_table_properties("server", host="localhost")
              .to_database())
        assert db.get_string("name") == "App"
        db.close()

    def test_flat_after_grouped_raises(self):
        with pytest.raises(MdixError, match="two-tier"):
            (MdixBuilder()
             .with_table_properties("server", host="x")
             .set_string("name", "App"))

    def test_multiple_table_properties_valid(self):
        db = (MdixBuilder()
              .set_string("app", "Test")
              .with_table_properties("server", host="localhost", port=8080)
              .with_table_properties("db", host="db.local", port=5432)
              .to_database())
        assert db.get_string("server.host") == "localhost"
        assert db.get_string("db.host") == "db.local"
        db.close()

    def test_flat_then_group_array_then_no_more_flat(self):
        with pytest.raises(MdixError, match="two-tier"):
            (MdixBuilder()
             .with_group_array("tags", ["a", "b"])
             .set_int("port", 8080))

    def test_reset_grouped_allows_new_flat(self):
        b = MdixBuilder().with_table_properties("server", host="x")
        b.reset_grouped()
        b.set_string("name", "App")  # must not raise after reset

    def test_reset_clears_all(self):
        b = (MdixBuilder()
             .set_string("name", "App")
             .with_table_properties("server", host="x"))
        b.reset()
        src = b.serialize()
        assert "@DATA" not in src


class TestFlatValueTypes:

    def test_set_string(self):
        db = MdixBuilder().set_string("name", "TestApp").to_database()
        assert db.get_string("name") == "TestApp"
        db.close()

    def test_set_int(self):
        db = MdixBuilder().set_int("port", 9000).to_database()
        assert db.get_int("port") == 9000
        db.close()

    def test_set_bool_true(self):
        db = MdixBuilder().set_bool("flag", True).to_database()
        assert db.get_bool("flag") is True
        db.close()

    def test_set_bool_false(self):
        db = MdixBuilder().set_bool("flag", False).to_database()
        assert db.get_bool("flag") is False
        db.close()

    def test_set_float(self):
        db = MdixBuilder().set_float("rate", 1.5).to_database()
        assert abs(db.get_float("rate") - 1.5) < 0.001
        db.close()

    def test_set_double(self):
        db = MdixBuilder().set_double("score", 99.9).to_database()
        assert abs(db.get_double("score") - 99.9) < 0.0001
        db.close()

    def test_set_hex_color_valid(self):
        b = MdixBuilder().set_hex_color("color", "#FF5733")
        src = b.serialize()
        assert "#FF5733" in src

    def test_set_hex_color_no_hash_raises(self):
        with pytest.raises(Exception):
            MdixBuilder().set_hex_color("color", "FF5733")

    def test_set_array_ints(self):
        db = MdixBuilder().set_array("ids", [1, 2, 3]).to_database()
        assert db.get_array_length("ids") == 3
        db.close()

    def test_set_tuple_max_six(self):
        src = MdixBuilder().set_tuple("t", [1, 2, 3, 4, 5, 6]).serialize()
        assert "t:(" in src

    def test_set_tuple_seven_raises(self):
        with pytest.raises(Exception):
            MdixBuilder().set_tuple("t", [1, 2, 3, 4, 5, 6, 7])

    def test_set_enum(self):
        src = MdixBuilder().set_enum("level", "LogLevel", "INFO").serialize()
        assert "LogLevel.INFO" in src

    def test_set_object(self):
        src = MdixBuilder().set_object("cfg", {"host": "x", "port": 80}).serialize()
        assert "host" in src
        assert "port" in src


class TestDateTimestampBlobRegex:
    """set_date / set_timestamp / set_blob / set_regex had zero coverage.

    Note: unlike set_string, the date/timestamp setters push the raw value
    string unquoted — DixScript's lexer treats a bare digit-dash sequence
    as a Date literal (and digit-dash...T... as Timestamp), so the value
    must already be a valid bare literal, not a quoted string.
    """

    def test_set_date_appears_in_output(self):
        src = MdixBuilder().set_date("created", "2024-01-15").serialize()
        assert "2024-01-15" in src
        assert "created" in src

    def test_set_date_readable_back(self):
        db = MdixBuilder().set_date("created", "2024-01-15").to_database()
        assert db.get_type("created") == "date"
        db.close()

    def test_set_timestamp_appears_in_output(self):
        src = MdixBuilder().set_timestamp("seen_at", "2024-01-15T10:30:00Z").serialize()
        assert "2024-01-15T10:30:00Z" in src

    def test_set_timestamp_readable_back(self):
        db = MdixBuilder().set_timestamp("seen_at", "2024-01-15T10:30:00Z").to_database()
        assert db.get_type("seen_at") == "timestamp"
        db.close()

    def test_set_blob_wraps_constructor_syntax(self):
        src = MdixBuilder().set_blob("payload", "aGVsbG8=").serialize()
        assert 'b:("aGVsbG8=")' in src

    def test_set_blob_readable_back(self):
        db = MdixBuilder().set_blob("payload", "aGVsbG8=").to_database()
        assert db.get_type("payload") == "blob"
        db.close()

    def test_set_regex_wraps_constructor_syntax(self):
        src = MdixBuilder().set_regex("pattern", "^[a-z]+$").serialize()
        assert 'r:("^[a-z]+$")' in src

    def test_set_regex_readable_back(self):
        db = MdixBuilder().set_regex("pattern", "^[a-z]+$").to_database()
        assert db.get_type("pattern") == "regex"
        db.close()


class TestTier2GroupedData:

    def test_with_table_properties_dict(self):
        db = (MdixBuilder()
              .with_table_properties("server", {"host": "localhost", "port": 8080})
              .to_database())
        assert db.get_string("server.host") == "localhost"
        assert db.get_int("server.port") == 8080
        db.close()

    def test_with_table_properties_kwargs(self):
        db = (MdixBuilder()
              .with_table_properties("server", host="localhost", port=8080)
              .to_database())
        assert db.get_string("server.host") == "localhost"
        db.close()

    def test_with_table_properties_mixed(self):
        db = (MdixBuilder()
              .with_table_properties("server", {"host": "localhost"}, port=8080)
              .to_database())
        assert db.get_string("server.host") == "localhost"
        assert db.get_int("server.port") == 8080
        db.close()

    def test_with_table_properties_empty_raises(self):
        with pytest.raises(MdixError):
            MdixBuilder().with_table_properties("server")

    def test_with_group_array_scalars(self):
        db = (MdixBuilder()
              .with_group_array("tags", ["alpha", "beta", "gamma"])
              .to_database())
        assert db.get_array_length("tags") == 3
        db.close()

    def test_with_group_array_objects(self):
        db = (MdixBuilder()
              .with_group_array("enemies", [
                  {"name": "Goblin", "hp": 50},
                  {"name": "Orc",    "hp": 100},
              ])
              .to_database())
        assert db.get_array_length("enemies") == 2
        db.close()

    def test_with_group_array_empty_path_raises(self):
        with pytest.raises(MdixError):
            MdixBuilder().with_group_array("", ["x"])


class TestConfigAndEnums:

    def test_set_config_appears_in_output(self):
        src = MdixBuilder().set_config("version", "1.0.0").serialize()
        assert "@CONFIG(" in src
        assert "1.0.0" in src

    def test_add_enum_auto_increment(self):
        src = (MdixBuilder()
               .add_enum("LogLevel", ["DEBUG", "INFO", "WARN"])
               .serialize())
        assert "@ENUMS(" in src
        assert "LogLevel" in src

    def test_add_enum_explicit_values(self):
        src = (MdixBuilder()
               .add_enum("Status", [("ACTIVE", 1), ("INACTIVE", 0)])
               .serialize())
        assert "ACTIVE = 1" in src
        assert "INACTIVE = 0" in src

    def test_add_enum_empty_raises(self):
        with pytest.raises(Exception):
            MdixBuilder().add_enum("Empty", [])

    def test_section_order_config_enums_data(self):
        src = (MdixBuilder()
               .set_config("version", "1.0.0")
               .add_enum("E", ["A", "B"])
               .set_int("x", 1)
               .serialize())
        assert src.index("@CONFIG") < src.index("@ENUMS") < src.index("@DATA")


class TestFinalization:

    def test_serialize_produces_data_section(self):
        src = MdixBuilder().set_int("port", 8080).serialize()
        assert "@DATA(" in src
        assert "port = 8080" in src

    def test_to_database_produces_readable_db(self):
        db = (MdixBuilder()
              .set_string("name", "MyApp")
              .set_int("port", 8080)
              .to_database())
        assert db.get_string("name") == "MyApp"
        assert db.get_int("port") == 8080
        db.close()

    def test_try_to_database_success(self):
        result = MdixBuilder().set_int("port", 8080).try_to_database()
        assert result.is_success

    def test_empty_builder_serialize_empty(self):
        src = MdixBuilder().serialize().strip()
        assert src == ""
