-- Tests: mdix.schema(), MdixSchema require_*/optional_*, MdixValidationReport.

return function(fw, mdix)

    local SRC = [[
@DATA(
  app_name = "TestApp"
  port     = 8080
  big_id   = 9_000_000_000L
  ratio    = 3.14f
  pi       = 3.14159265358979
  debug    = true
)
]]

    -- ── Chaining — the point of the add_function_mut fix ────────────────

    fw.suite("schema_chaining")

    fw.test("require_calls_chain_and_return_the_same_schema", function()
        -- Before the add_function_mut fix, require_string returned Ok(())
        -- (no Lua value), so this second :require_int(...) call would have
        -- raised "attempt to index a nil value" -- this test exists
        -- specifically to catch that regressing.
        local schema = mdix.schema():require_string("app_name"):require_int("port")
        fw.assert_not_nil(schema)
        fw.assert_eq(schema:field_count(), 2)
    end)

    fw.test("long_chain_of_required_and_optional", function()
        local schema = mdix.schema()
            :require_string("app_name")
            :require_int("port")
            :require_long("big_id")
            :optional_bool("debug")
            :optional_string("does_not_exist")
        fw.assert_eq(schema:field_count(), 5)
    end)

    fw.test("with_description_chains_too", function()
        local schema = mdix.schema():require_string("app_name"):with_description("the app's display name")
        fw.assert_eq(schema:field_count(), 1)
    end)

    -- ── Validation ─────────────────────────────────────────────────────

    fw.suite("schema_validation")

    fw.test("all_required_present_is_valid", function()
        local db = mdix.load_str(SRC)
        local schema = mdix.schema()
            :require_string("app_name")
            :require_int("port")
            :require_bool("debug")
        local report = db:validate_schema(schema)
        fw.assert_true(report:is_valid())
        fw.assert_eq(report:error_count(), 0)
        db:close()
    end)

    fw.test("missing_required_field_is_invalid", function()
        local db = mdix.load_str(SRC)
        local schema = mdix.schema():require_string("does_not_exist")
        local report = db:validate_schema(schema)
        fw.assert_false(report:is_valid())
        fw.assert_eq(report:error_count(), 1)
        db:close()
    end)

    fw.test("missing_optional_field_is_still_valid", function()
        local db = mdix.load_str(SRC)
        local schema = mdix.schema():optional_string("does_not_exist")
        local report = db:validate_schema(schema)
        fw.assert_true(report:is_valid())
        db:close()
    end)

    fw.test("wrong_type_is_invalid", function()
        local db = mdix.load_str(SRC)
        -- app_name is a String, not a Bool.
        local schema = mdix.schema():require_bool("app_name")
        local report = db:validate_schema(schema)
        fw.assert_false(report:is_valid())
        db:close()
    end)

    fw.test("failed_paths_lists_missing_and_wrong_type", function()
        local db = mdix.load_str(SRC)
        local schema = mdix.schema()
            :require_string("app_name")   -- present, correct type: not failed
            :require_string("missing_a")  -- missing: failed
            :require_bool("port")          -- present, wrong type: failed
        local report = db:validate_schema(schema)
        local failed = report:failed_paths()
        fw.assert_eq(#failed, 2)
        db:close()
    end)

    fw.test("errors_have_expected_shape", function()
        local db = mdix.load_str(SRC)
        local schema = mdix.schema():require_string("missing_field")
        local report = db:validate_schema(schema)
        local errs = report:errors()
        fw.assert_eq(#errs, 1)
        fw.assert_eq(errs[1].path, "missing_field")
        fw.assert_not_nil(errs[1].expected)
        fw.assert_not_nil(errs[1].kind)
        db:close()
    end)

    fw.test("to_string_and_tostring_agree", function()
        local db = mdix.load_str(SRC)
        local schema = mdix.schema():require_string("missing_field")
        local report = db:validate_schema(schema)
        fw.assert_eq(report:to_string(), tostring(report))
        db:close()
    end)

    -- ── Reusability ──────────────────────────────────────────────────────

    fw.suite("schema_reuse")

    fw.test("same_schema_validates_multiple_databases", function()
        local schema = mdix.schema():require_string("app_name")

        local db_good = mdix.load_str(SRC)
        local db_bad = mdix.load_str("@DATA( other_field = 1 )")

        fw.assert_true(db_good:validate_schema(schema):is_valid())
        fw.assert_false(db_bad:validate_schema(schema):is_valid())

        db_good:close()
        db_bad:close()
    end)

    -- ── paths() introspection ────────────────────────────────────────────

    fw.suite("schema_introspection")

    fw.test("paths_lists_declared_fields_in_order", function()
        local schema = mdix.schema():require_string("a"):require_int("b"):optional_bool("c")
        local paths = schema:paths()
        fw.assert_eq(#paths, 3)
        fw.assert_eq(paths[1], "a")
        fw.assert_eq(paths[2], "b")
        fw.assert_eq(paths[3], "c")
    end)

end
