-- Tests: loading, reading, type inspection, export, error handling.

return function(fw, mdix)

    -- ── Loading ───────────────────────────────────────────────────────────

    fw.suite("loading")

    fw.test("load_str_valid_returns_db", function()
        local db = mdix.load_str('@DATA( port = 8080, host = "localhost" )')
        fw.assert_not_nil(db)
        db:close()
    end)

    fw.test("load_str_empty_raises", function()
        fw.assert_raises(function() mdix.load_str("") end)
    end)

    fw.test("load_str_whitespace_only_raises", function()
        fw.assert_raises(function() mdix.load_str("   \n\t  ") end)
    end)

    fw.test("load_str_malformed_raises", function()
        fw.assert_raises(function() mdix.load_str("@@@BROKEN###") end)
    end)

    fw.test("load_nonexistent_file_raises", function()
        fw.assert_raises(function() mdix.load("/nonexistent/path/config.mdix") end)
    end)

    fw.test("close_is_idempotent", function()
        local db = mdix.load_str("@DATA( x = 1 )")
        db:close()
        db:close()  -- must not crash
    end)

    fw.test("entry_count_positive", function()
        local db = mdix.load_str("@DATA( a = 1, b = 2, c = 3 )")
        fw.assert_true(db:entry_count() > 0)
        db:close()
    end)

    fw.test("tostring_shows_entry_count", function()
        local db = mdix.load_str("@DATA( x = 1 )")
        local s = tostring(db)
        fw.assert_contains(s, "MdixDatabase")
        db:close()
    end)

    fw.test("tostring_after_close_shows_closed", function()
        local db = mdix.load_str("@DATA( x = 1 )")
        db:close()
        fw.assert_contains(tostring(db), "closed")
    end)

    -- ── Existence and type inspection ─────────────────────────────────────

    fw.suite("inspection")

    local SRC = '@DATA( name = "TestApp", port = 8080, enabled = true, rate = 1.5f, score = 99.9 )'

    fw.test("exists_present_returns_true", function()
        local db = mdix.load_str(SRC)
        fw.assert_true(db:exists("port"))
        db:close()
    end)

    fw.test("exists_absent_returns_false", function()
        local db = mdix.load_str(SRC)
        fw.assert_false(db:exists("nonexistent"))
        db:close()
    end)

    fw.test("get_type_int", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get_type("port"), "int")
        db:close()
    end)

    fw.test("get_type_string", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get_type("name"), "string")
        db:close()
    end)

    fw.test("get_type_bool", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get_type("enabled"), "bool")
        db:close()
    end)

    fw.test("get_type_unknown_for_missing", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get_type("missing"), "unknown")
        db:close()
    end)

    fw.test("get_keys_top_level_contains_name", function()
        local db = mdix.load_str(SRC)
        local keys = db:keys()
        local found = false
        for _, k in ipairs(keys) do
            if k == "name" then found = true; break end
        end
        fw.assert_true(found)
        db:close()
    end)

    -- ── Typed getters ─────────────────────────────────────────────────────

    fw.suite("typed_getters")

    fw.test("get_string_known_path", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get_string("name"), "TestApp")
        db:close()
    end)

    fw.test("get_string_with_default_for_missing", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get_string("missing", "fallback"), "fallback")
        db:close()
    end)

    fw.test("get_string_missing_no_default_raises", function()
        local db = mdix.load_str(SRC)
        fw.assert_raises(function() db:get_string("missing") end)
        db:close()
    end)

    fw.test("get_int_known_path", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get_int("port"), 8080)
        db:close()
    end)

    fw.test("get_int_with_default", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get_int("missing", 42), 42)
        db:close()
    end)

    fw.test("get_bool_true", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get_bool("enabled"), true)
        db:close()
    end)

    fw.test("get_number_float", function()
        local db = mdix.load_str(SRC)
        fw.assert_near(db:get_number("rate"), 1.5, 0.01)
        db:close()
    end)

    fw.test("get_number_double", function()
        local db = mdix.load_str(SRC)
        fw.assert_near(db:get_number("score"), 99.9, 0.01)
        db:close()
    end)

    fw.test("get_auto_converts_int", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get("port"), 8080)
        fw.assert_eq(type(db:get("port")), "number")
        db:close()
    end)

    fw.test("get_auto_converts_string", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get("name"), "TestApp")
        db:close()
    end)

    fw.test("get_auto_returns_nil_for_missing", function()
        local db = mdix.load_str(SRC)
        fw.assert_nil(db:get("nonexistent"))
        db:close()
    end)

    -- ── Nested dotted paths ────────────────────────────────────────────────

    fw.suite("nested_paths")

    local NESTED = [[
@DATA(
  app  = "Nested"
  server: host = "localhost", port = 9000, ssl = true
  db: host = "db.local", port = 5432
)
]]

    fw.test("nested_get_string", function()
        local db = mdix.load_str(NESTED)
        fw.assert_eq(db:get_string("server.host"), "localhost")
        db:close()
    end)

    fw.test("nested_get_int", function()
        local db = mdix.load_str(NESTED)
        fw.assert_eq(db:get_int("server.port"), 9000)
        db:close()
    end)

    fw.test("nested_get_bool", function()
        local db = mdix.load_str(NESTED)
        fw.assert_eq(db:get_bool("server.ssl"), true)
        db:close()
    end)

    fw.test("nested_keys_for_prefix", function()
        local db = mdix.load_str(NESTED)
        local keys = db:keys("server")
        fw.assert_true(#keys >= 1)
        db:close()
    end)

    -- ── Foreign format import ──────────────────────────────────────────────

    fw.suite("foreign_formats")

    fw.test("from_json_valid_object", function()
        local db = mdix.from_json('{"port": 7777, "host": "localhost"}')
        fw.assert_not_nil(db)
        fw.assert_eq(db:get_int("port"), 7777)
        db:close()
    end)

    fw.test("from_json_empty_raises", function()
        fw.assert_raises(function() mdix.from_json("") end)
    end)

    fw.test("from_json_array_top_level_raises", function()
        fw.assert_raises(function() mdix.from_json("[1, 2, 3]") end)
    end)

    fw.test("from_toml_valid", function()
        local db = mdix.from_toml('port = 7777\nhost = "localhost"\n')
        fw.assert_not_nil(db)
        fw.assert_eq(db:get_int("port"), 7777)
        db:close()
    end)

    fw.test("from_toml_empty_raises", function()
        fw.assert_raises(function() mdix.from_toml("") end)
    end)

    -- ── Export ────────────────────────────────────────────────────────────

    fw.suite("export")

    fw.test("to_json_contains_values", function()
        local db  = mdix.load_str(SRC)
        local raw = db:to_json(false)
        fw.assert_contains(raw, "8080")
        fw.assert_contains(raw, "TestApp")
        db:close()
    end)

    fw.test("to_json_indented_has_newlines", function()
        local db = mdix.load_str(SRC)
        fw.assert_contains(db:to_json(true), "\n")
        db:close()
    end)

    fw.test("to_toml_contains_values", function()
        local db = mdix.load_str(SRC)
        local t  = db:to_toml()
        fw.assert_contains(t, "8080")
        db:close()
    end)

    fw.test("to_mdix_contains_data_section", function()
        local db = mdix.load_str(SRC)
        fw.assert_contains(db:to_mdix(), "@DATA")
        db:close()
    end)

    fw.test("roundtrip_json", function()
        local db   = mdix.load_str(SRC)
        local json = db:to_json(false)
        db:close()
        local db2 = mdix.from_json(json)
        fw.assert_eq(db2:get_int("port"), 8080)
        db2:close()
    end)

    fw.test("roundtrip_toml", function()
        local db   = mdix.load_str(SRC)
        local toml = db:to_toml()
        db:close()
        local db2 = mdix.from_toml(toml)
        fw.assert_eq(db2:get_int("port"), 8080)
        db2:close()
    end)

    -- ── Source utilities ───────────────────────────────────────────────────

    fw.suite("source_utils")

    fw.test("minify_source_reduces_size", function()
        local src     = "@CONFIG(\n  version -> \"1.0.0\"\n)"
        local minified = mdix.minify_source(src)
        fw.assert_true(#minified < #src)
    end)

    fw.test("format_source_preserves_content", function()
        local src      = "@DATA(\n\n\n  port = 8080\n\n)"
        local formatted = mdix.format_source(src)
        fw.assert_contains(formatted, "8080")
    end)

end
