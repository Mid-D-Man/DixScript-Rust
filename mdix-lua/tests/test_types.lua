-- Tests: DixValue types, arrays, enums, nested structures, get_json.

return function(fw, mdix)

    -- ── Array access ──────────────────────────────────────────────────────

    fw.suite("arrays")

    local ARR_SRC = [[
@DATA(
  tags:: "alpha", "beta", "gamma"
  ids::  1, 2, 3
  enemies::
    { name = "Goblin", hp = 50  },
    { name = "Orc",    hp = 100 }
)
]]

    fw.test("array_length_string_array", function()
        local db = mdix.load_str(ARR_SRC)
        fw.assert_eq(db:array_length("tags"), 3)
        db:close()
    end)

    fw.test("array_length_int_array", function()
        local db = mdix.load_str(ARR_SRC)
        fw.assert_eq(db:array_length("ids"), 3)
        db:close()
    end)

    fw.test("array_length_object_array", function()
        local db = mdix.load_str(ARR_SRC)
        fw.assert_eq(db:array_length("enemies"), 2)
        db:close()
    end)

    fw.test("array_element_by_index_string", function()
        local db = mdix.load_str(ARR_SRC)
        fw.assert_eq(db:get_string("tags[0]"), "alpha")
        db:close()
    end)

    fw.test("array_element_by_index_int", function()
        local db = mdix.load_str(ARR_SRC)
        fw.assert_eq(db:get_int("ids[0]"), 1)
        db:close()
    end)

    fw.test("array_get_returns_lua_table", function()
        local db  = mdix.load_str(ARR_SRC)
        local arr = db:get("tags")
        fw.assert_type(arr, "table")
        fw.assert_eq(#arr, 3)
        db:close()
    end)

    fw.test("array_length_non_array_returns_minus_one", function()
        local db = mdix.load_str('@DATA( port = 8080 )')
        fw.assert_eq(db:array_length("port"), -1)
        db:close()
    end)

    fw.test("object_array_element_field", function()
        local db = mdix.load_str(ARR_SRC)
        fw.assert_eq(db:get_string("enemies[0].name"), "Goblin")
        fw.assert_eq(db:get_int("enemies[0].hp"),     50)
        db:close()
    end)

    -- ── Enum values ───────────────────────────────────────────────────────

    fw.suite("enums")

    local ENUM_SRC = [[
@ENUMS(
  LogLevel { DEBUG, INFO, WARN, ERROR }
  Status   { ACTIVE = 1, INACTIVE = 0 }
)
@DATA(
  log_level<enum> = LogLevel.INFO
  status<enum>    = Status.ACTIVE
)
]]

    fw.test("enum_get_returns_table", function()
        local db  = mdix.load_str(ENUM_SRC)
        local val = db:get("log_level")
        fw.assert_type(val, "table")
        db:close()
    end)

    fw.test("enum_table_has_enum_name", function()
        local db  = mdix.load_str(ENUM_SRC)
        local val = db:get("log_level")
        fw.assert_eq(val.enum_name, "LogLevel")
        db:close()
    end)

    fw.test("enum_table_has_field", function()
        local db  = mdix.load_str(ENUM_SRC)
        local val = db:get("log_level")
        fw.assert_eq(val.field, "INFO")
        db:close()
    end)

    fw.test("enum_table_has_integer_value", function()
        local db  = mdix.load_str(ENUM_SRC)
        local val = db:get("log_level")
        fw.assert_type(val.value, "number")
        -- INFO is index 1 (auto-increment from 0)
        fw.assert_eq(val.value, 1)
        db:close()
    end)

    fw.test("enum_explicit_value", function()
        local db  = mdix.load_str(ENUM_SRC)
        local val = db:get("status")
        fw.assert_eq(val.field, "ACTIVE")
        fw.assert_eq(val.value, 1)
        db:close()
    end)

    fw.test("get_type_reports_enum", function()
        local db = mdix.load_str(ENUM_SRC)
        fw.assert_eq(db:get_type("log_level"), "enum")
        db:close()
    end)

    -- ── Special literal types ─────────────────────────────────────────────

    fw.suite("special_types")

    fw.test("hex_color_accessible_as_string", function()
        local db = mdix.load_str('@DATA( sky<hex> = #87CEEB )')
        fw.assert_eq(db:get_type("sky"), "hex_color")
        local val = db:get_string("sky")
        fw.assert_contains(val, "#")
        db:close()
    end)

    fw.test("date_accessible_as_string", function()
        local db = mdix.load_str('@DATA( release = 2025-12-31 )')
        fw.assert_eq(db:get_type("release"), "date")
        fw.assert_contains(db:get_string("release"), "2025")
        db:close()
    end)

    fw.test("blob_type", function()
        local db = mdix.load_str('@DATA( icon = b:("SGVsbG8=") )')
        fw.assert_eq(db:get_type("icon"), "blob")
        db:close()
    end)

    fw.test("null_get_returns_nil", function()
        local db  = mdix.load_str('@DATA( nothing = null )')
        local val = db:get("nothing")
        fw.assert_nil(val)
        db:close()
    end)

    -- ── get_json escape hatch ─────────────────────────────────────────────

    fw.suite("get_json")

    fw.test("get_json_int_produces_json_number", function()
        local db  = mdix.load_str('@DATA( port = 8080 )')
        local raw = db:get_json("port")
        fw.assert_contains(raw, "8080")
        db:close()
    end)

    fw.test("get_json_string_produces_quoted", function()
        local db  = mdix.load_str('@DATA( name = "App" )')
        local raw = db:get_json("name")
        fw.assert_contains(raw, '"App"')
        db:close()
    end)

    fw.test("get_json_missing_path_raises", function()
        local db = mdix.load_str('@DATA( port = 8080 )')
        fw.assert_raises(function() db:get_json("nonexistent") end)
        db:close()
    end)

    -- ── Complex source with all section types ─────────────────────────────

    fw.suite("full_source")

    local FULL_SRC = [[
@CONFIG(
  version -> "1.0.0"
  author  -> "MidManStudio"
)

@ENUMS(
  Rarity { Common, Rare, Epic, Legendary }
)

@QUICKFUNCS(
  ~item<object>(id, rarity<enum>) {
    return { item_id = id, rarity = rarity }
  }
)

@DATA(
  game_name = "AirStrike"
  version   = "0.1.0"
  build     = 42
  beta      = true

  server: host = "localhost", port = 7777, ssl = false

  tags:: "alpha", "beta", "test"
)
]]

    fw.test("full_source_flat_props", function()
        local db = mdix.load_str(FULL_SRC)
        fw.assert_eq(db:get_string("game_name"), "AirStrike")
        fw.assert_eq(db:get_int("build"),        42)
        fw.assert_eq(db:get_bool("beta"),        true)
        db:close()
    end)

    fw.test("full_source_nested_props", function()
        local db = mdix.load_str(FULL_SRC)
        fw.assert_eq(db:get_string("server.host"), "localhost")
        fw.assert_eq(db:get_int("server.port"),    7777)
        db:close()
    end)

    fw.test("full_source_array", function()
        local db = mdix.load_str(FULL_SRC)
        fw.assert_eq(db:array_length("tags"), 3)
        fw.assert_eq(db:get_string("tags[0]"), "alpha")
        db:close()
    end)

end
