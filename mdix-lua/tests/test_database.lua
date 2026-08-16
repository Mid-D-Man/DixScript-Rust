-- Tests: Database methods test_basic.lua doesn't already cover --
-- get_long, get_float/get_double, array_length, keys.

return function(fw, mdix)

    local SRC = [[
@DATA(
  small_int = 42
  big_id    = 9_000_000_000L
  ratio     = 3.14f
  pi        = 3.14159265358979
  name      = "Widget"
  tags::    "a", "b", "c"
  server: host = "localhost", port = 8080
)
]]

    -- ── get_long ─────────────────────────────────────────────────────────

    fw.suite("get_long")

    fw.test("reads_a_genuine_long_value", function()
        -- 9_000_000_000 overflows i32 (max ~2.1 billion) -- this is
        -- specifically checking database.rs's get_long is its own
        -- distinct binding, not a widened get_int (the class of bug
        -- found and fixed in the Go binding's original GetInt64).
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get_long("big_id"), 9000000000)
        db:close()
    end)

    fw.test("get_long_also_accepts_int_values", function()
        -- Widening: an i32-range value is still a valid Long read.
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get_long("small_int"), 42)
        db:close()
    end)

    -- ── get_float / get_double ───────────────────────────────────────────
    -- These two are NOT symmetric in this binding: get_float only
    -- matches a genuine Float value; get_double widens from
    -- Float/Int/Long as well as matching a genuine Double (see its own
    -- doc comment in database.rs). Documenting the actual behavior here,
    -- not asserting what "should" happen -- this is worth a second look
    -- against the FFI-based bindings (Go/Odin/C# all route get_float and
    -- get_double through the same core get::<f64>() call and are fully
    -- symmetric as a result), since this binding hand-implements both
    -- against DixValue directly instead and ended up with different
    -- rules. Not changed here -- flagging via this test, not fixing it
    -- unasked.

    fw.suite("get_float_get_double")

    fw.test("get_float_reads_a_genuine_float", function()
        local db = mdix.load_str(SRC)
        fw.assert_near(db:get_float("ratio"), 3.14, 0.001)
        db:close()
    end)

    fw.test("get_double_reads_a_genuine_double", function()
        local db = mdix.load_str(SRC)
        fw.assert_near(db:get_double("pi"), 3.14159265358979, 0.0000001)
        db:close()
    end)

    fw.test("get_double_widens_a_genuine_float", function()
        local db = mdix.load_str(SRC)
        fw.assert_near(db:get_double("ratio"), 3.14, 0.001)
        db:close()
    end)

    fw.test("get_double_widens_a_genuine_int", function()
        local db = mdix.load_str(SRC)
        fw.assert_near(db:get_double("small_int"), 42.0, 0.001)
        db:close()
    end)

    fw.test("get_double_widens_a_genuine_long", function()
        local db = mdix.load_str(SRC)
        fw.assert_near(db:get_double("big_id"), 9000000000.0, 0.001)
        db:close()
    end)

    fw.test("get_float_on_a_double_value_uses_default_not_error", function()
        -- pi is a genuine Double, not a Float -- get_float with a
        -- default should fall back to it rather than raising, per the
        -- Some(other) => match default branch in database.rs.
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:get_float("pi", -1.0), -1.0)
        db:close()
    end)

    fw.test("get_float_on_a_double_value_without_default_raises", function()
        local db = mdix.load_str(SRC)
        fw.assert_raises(function() db:get_float("pi") end)
        db:close()
    end)

    -- ── array_length ─────────────────────────────────────────────────────

    fw.suite("array_length")

    fw.test("counts_array_elements", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:array_length("tags"), 3)
        db:close()
    end)

    fw.test("returns_minus_one_for_non_array_path", function()
        -- Documented, deliberate behavior (see array_length's own doc
        -- comment) -- not an error the way a missing path is.
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:array_length("name"), -1)
        db:close()
    end)

    fw.test("returns_minus_one_for_missing_path", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(db:array_length("does.not.exist"), -1)
        db:close()
    end)

    -- ── keys ─────────────────────────────────────────────────────────────

    fw.suite("keys")

    fw.test("top_level_keys_with_no_argument", function()
        local db = mdix.load_str(SRC)
        local top = db:keys()
        fw.assert_true(#top > 0)
        local found_name = false
        for _, k in ipairs(top) do
            if k == "name" then found_name = true end
        end
        fw.assert_true(found_name)
        db:close()
    end)

    fw.test("top_level_keys_with_empty_string", function()
        local db = mdix.load_str(SRC)
        fw.assert_eq(#db:keys(), #db:keys(""))
        db:close()
    end)

    fw.test("nested_keys_under_prefix", function()
        local db = mdix.load_str(SRC)
        local server_keys = db:keys("server")
        fw.assert_eq(#server_keys, 2) -- host, port
        db:close()
    end)

end
