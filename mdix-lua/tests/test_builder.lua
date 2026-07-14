-- Tests: MdixBuilder — two-tier ordering, all value types, finalization.

return function(fw, mdix)

    -- ── Two-tier ordering ─────────────────────────────────────────────────

    fw.suite("builder_two_tier")

    fw.test("flat_before_grouped_is_valid", function()
        local db = mdix.builder()
            :set_string("name", "App")
            :set_int("port", 8080)
            :with_table("server", {host = "localhost"})
            :build()
        fw.assert_eq(db:get_string("name"), "App")
        db:close()
    end)

    fw.test("flat_after_table_raises", function()
        fw.assert_raises(function()
            mdix.builder()
                :with_table("server", {host = "x"})
                :set_string("name", "App")  -- must raise
        end, "two%-tier")
    end)

    fw.test("flat_after_array_raises", function()
        fw.assert_raises(function()
            mdix.builder()
                :with_array("tags", {"a", "b"})
                :set_int("port", 80)  -- must raise
        end, "two%-tier")
    end)

    fw.test("reset_grouped_allows_new_flat", function()
        local b = mdix.builder():with_table("server", {host = "x"})
        b:reset_grouped()
        -- must not raise
        b:set_string("name", "App")
    end)

    fw.test("reset_clears_all", function()
        local b = mdix.builder()
            :set_string("name", "App")
            :with_table("server", {host = "x"})
        b:reset()
        local src = b:serialize()
        fw.assert_false(src:find("@DATA") ~= nil,
            "expected no @DATA after full reset")
    end)

    fw.test("reset_grouped_keeps_flat", function()
        local b = mdix.builder()
            :set_string("name", "App")
            :with_table("server", {host = "x"})
        b:reset_grouped()
        local src = b:serialize()
        fw.assert_contains(src, "name")
    end)

    -- ── Flat property setters ─────────────────────────────────────────────

    fw.suite("builder_flat_setters")

    fw.test("set_string_roundtrip", function()
        local db = mdix.builder():set_string("n", "Hello"):build()
        fw.assert_eq(db:get_string("n"), "Hello")
        db:close()
    end)

    fw.test("set_string_escapes_quotes", function()
        local src = mdix.builder():set_string("q", 'say "hi"'):serialize()
        fw.assert_contains(src, '\\"hi\\"')
    end)

    fw.test("set_int_roundtrip", function()
        local db = mdix.builder():set_int("port", 9000):build()
        fw.assert_eq(db:get_int("port"), 9000)
        db:close()
    end)

    fw.test("set_bool_true_roundtrip", function()
        local db = mdix.builder():set_bool("flag", true):build()
        fw.assert_eq(db:get_bool("flag"), true)
        db:close()
    end)

    fw.test("set_bool_false_roundtrip", function()
        local db = mdix.builder():set_bool("flag", false):build()
        fw.assert_eq(db:get_bool("flag"), false)
        db:close()
    end)

    fw.test("set_number_float_roundtrip", function()
        local db = mdix.builder():set_number("rate", 1.5):build()
        fw.assert_near(db:get_number("rate"), 1.5, 0.01)
        db:close()
    end)

    fw.test("set_date_appears_in_source", function()
        local src = mdix.builder():set_date("release", "2025-12-31"):serialize()
        fw.assert_contains(src, "2025-12-31")
    end)

    fw.test("set_hex_color_valid", function()
        local src = mdix.builder():set_hex_color("sky", "#87CEEB"):serialize()
        fw.assert_contains(src, "#87CEEB")
    end)

    fw.test("set_hex_color_no_hash_raises", function()
        fw.assert_raises(function()
            mdix.builder():set_hex_color("sky", "87CEEB")
        end)
    end)

    fw.test("set_blob_appears_in_source", function()
        local src = mdix.builder():set_blob("icon", "SGVsbG8="):serialize()
        fw.assert_contains(src, "b:(")
        fw.assert_contains(src, "SGVsbG8=")
    end)

    fw.test("set_regex_appears_in_source", function()
        local src = mdix.builder():set_regex("pat", "^[a-z]+$"):serialize()
        fw.assert_contains(src, "r:(")
    end)

    fw.test("set_enum_produces_dot_notation", function()
        local src = mdix.builder():set_enum("level", "LogLevel", "INFO"):serialize()
        fw.assert_contains(src, "LogLevel.INFO")
    end)

    fw.test("set_auto_detects_int", function()
        local db = mdix.builder():set("port", 7777):build()
        fw.assert_eq(db:get_int("port"), 7777)
        db:close()
    end)

    fw.test("set_auto_detects_string", function()
        local db = mdix.builder():set("name", "Foo"):build()
        fw.assert_eq(db:get_string("name"), "Foo")
        db:close()
    end)

    fw.test("set_auto_detects_bool", function()
        local db = mdix.builder():set("flag", true):build()
        fw.assert_eq(db:get_bool("flag"), true)
        db:close()
    end)

    fw.test("set_auto_array", function()
        local src = mdix.builder():set("ids", {1, 2, 3}):serialize()
        fw.assert_contains(src, "[1")
    end)

    fw.test("set_auto_object", function()
        local src = mdix.builder():set("cfg", {host = "localhost"}):serialize()
        fw.assert_contains(src, "host")
    end)

    -- ── Tier-2 grouped ────────────────────────────────────────────────────

    fw.suite("builder_grouped")

    fw.test("with_table_roundtrip", function()
        local db = mdix.builder()
            :with_table("server", {host = "localhost", port = 8080})
            :build()
        fw.assert_eq(db:get_string("server.host"), "localhost")
        fw.assert_eq(db:get_int("server.port"), 8080)
        db:close()
    end)

    fw.test("with_table_empty_raises", function()
        fw.assert_raises(function()
            mdix.builder():with_table("server", {})
        end)
    end)

    fw.test("with_table_empty_path_raises", function()
        fw.assert_raises(function()
            mdix.builder():with_table("", {host = "x"})
        end)
    end)

    fw.test("with_array_scalars_roundtrip", function()
        local db = mdix.builder()
            :with_array("tags", {"alpha", "beta", "gamma"})
            :build()
        fw.assert_eq(db:array_length("tags"), 3)
        db:close()
    end)

    fw.test("with_array_objects", function()
        local db = mdix.builder()
            :with_array("enemies", {
                {name = "Goblin", hp = 50},
                {name = "Orc",    hp = 100},
            })
            :build()
        fw.assert_eq(db:array_length("enemies"), 2)
        db:close()
    end)

    fw.test("with_array_empty_path_raises", function()
        fw.assert_raises(function()
            mdix.builder():with_array("", {"a"})
        end)
    end)

    fw.test("multiple_tables_independent", function()
        local db = mdix.builder()
            :with_table("server", {host = "localhost", port = 8080})
            :with_table("db",     {host = "db.local",  port = 5432})
            :build()
        fw.assert_eq(db:get_string("server.host"), "localhost")
        fw.assert_eq(db:get_string("db.host"),     "db.local")
        db:close()
    end)

    -- ── @CONFIG and @ENUMS ────────────────────────────────────────────────

    fw.suite("builder_config_enums")

    fw.test("set_config_appears_in_source", function()
        local src = mdix.builder():set_config("version", "1.0.0"):serialize()
        fw.assert_contains(src, "@CONFIG")
        fw.assert_contains(src, "1.0.0")
    end)

    fw.test("set_config_empty_key_raises", function()
        fw.assert_raises(function()
            mdix.builder():set_config("", "value")
        end)
    end)

    fw.test("add_enum_auto_increment", function()
        local src = mdix.builder()
            :add_enum("LogLevel", {"DEBUG", "INFO", "WARN", "ERROR"})
            :serialize()
        fw.assert_contains(src, "@ENUMS")
        fw.assert_contains(src, "LogLevel")
    end)

    fw.test("add_enum_explicit_values", function()
        local src = mdix.builder()
            :add_enum("Status", {{"ACTIVE", 1}, {"INACTIVE", 0}})
            :serialize()
        fw.assert_contains(src, "ACTIVE = 1")
        fw.assert_contains(src, "INACTIVE = 0")
    end)

    fw.test("add_enum_empty_raises", function()
        fw.assert_raises(function()
            mdix.builder():add_enum("Empty", {})
        end)
    end)

    fw.test("section_order_config_enums_data", function()
        local src = mdix.builder()
            :set_config("version", "1.0.0")
            :add_enum("E", {"A", "B"})
            :set_int("x", 1)
            :serialize()
        fw.assert_true(src:find("@CONFIG") < src:find("@ENUMS"))
        fw.assert_true(src:find("@ENUMS")  < src:find("@DATA"))
    end)

    -- ── Finalization ──────────────────────────────────────────────────────

    fw.suite("builder_finalize")

    fw.test("serialize_produces_data_section", function()
        local src = mdix.builder():set_int("port", 8080):serialize()
        fw.assert_contains(src, "@DATA(")
        fw.assert_contains(src, "port = 8080")
    end)

    fw.test("build_produces_readable_db", function()
        local db = mdix.builder()
            :set_string("name", "MyApp")
            :set_int("port", 8080)
            :build()
        fw.assert_eq(db:get_string("name"), "MyApp")
        fw.assert_eq(db:get_int("port"), 8080)
        db:close()
    end)

    fw.test("build_empty_raises", function()
        fw.assert_raises(function()
            mdix.builder():build()
        end)
    end)

    fw.test("tostring_shows_counts", function()
        local b = mdix.builder()
            :set_string("x", "1")
            :with_table("s", {a = 1})
        fw.assert_contains(tostring(b), "MdixBuilder")
    end)

    fw.test("flat_and_grouped_in_correct_tier_order", function()
        local src = mdix.builder()
            :set_string("name", "App")
            :set_int("port", 8080)
            :with_table("server", {host = "localhost"})
            :with_array("tags", {"a", "b"})
            :serialize()
        -- flat props must come before :: and : in the DATA section
        local data_start = src:find("@DATA")
        local flat_pos   = src:find("name", data_start)
        local table_pos  = src:find("server:", data_start)
        local arr_pos    = src:find("tags::", data_start)
        fw.assert_not_nil(flat_pos)
        fw.assert_not_nil(table_pos)
        fw.assert_not_nil(arr_pos)
        fw.assert_true(flat_pos < table_pos)
        fw.assert_true(flat_pos < arr_pos)
    end)

end
