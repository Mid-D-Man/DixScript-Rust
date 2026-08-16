-- Tests: mdix.merge_files, mdix.merge_files_weighted, MdixDatabase:merge_with.

return function(fw, mdix)

    local PRIMARY_SRC = [[
@DATA(
  app_name = "PrimaryApp"
  port     = 8080
  tags::   "primary", "base"
)
]]

    local SECONDARY_SRC = [[
@DATA(
  app_name = "SecondaryApp"
  debug    = true
  tags::   "secondary"
)
]]

    local function write_temp(content)
        local path = os.tmpname() .. ".mdix"
        local f = assert(io.open(path, "w"))
        f:write(content)
        f:close()
        return path
    end

    -- ── merge_with (in-memory databases, no disk juggling needed) ────────

    fw.suite("merge_with")

    fw.test("primary_wins", function()
        local primary   = mdix.load_str(PRIMARY_SRC)
        local secondary = mdix.load_str(SECONDARY_SRC)
        local merged, conflicts = primary:merge_with(secondary, "primary_wins")
        fw.assert_eq(merged:get_string("app_name"), "PrimaryApp")
        -- debug only exists in secondary -- no conflict, still merges in.
        fw.assert_eq(merged:get_bool("debug"), true)
        fw.assert_true(#conflicts > 0)
        primary:close()
        secondary:close()
        merged:close()
    end)

    fw.test("secondary_wins", function()
        local primary   = mdix.load_str(PRIMARY_SRC)
        local secondary = mdix.load_str(SECONDARY_SRC)
        local merged = primary:merge_with(secondary, "secondary_wins")
        fw.assert_eq(merged:get_string("app_name"), "SecondaryApp")
        primary:close()
        secondary:close()
        merged:close()
    end)

    fw.test("throw_on_conflict_raises_on_genuine_conflict", function()
        local primary   = mdix.load_str(PRIMARY_SRC)
        local secondary = mdix.load_str(SECONDARY_SRC)
        fw.assert_raises(function()
            primary:merge_with(secondary, "throw_on_conflict")
        end)
        primary:close()
        secondary:close()
    end)

    fw.test("throw_on_conflict_does_not_raise_without_conflicts", function()
        local a = mdix.load_str("@DATA( only_a = 1 )")
        local b = mdix.load_str("@DATA( only_b = 2 )")
        local merged, conflicts = a:merge_with(b, "throw_on_conflict")
        fw.assert_not_nil(merged)
        fw.assert_eq(#conflicts, 0)
        a:close()
        b:close()
        merged:close()
    end)

    fw.test("array_concat_strategy", function()
        local primary   = mdix.load_str(PRIMARY_SRC)
        local secondary = mdix.load_str(SECONDARY_SRC)
        local merged = primary:merge_with(secondary, "primary_wins", "concat")
        -- primary has 2 tags, secondary has 1 -- concat = 3
        fw.assert_eq(merged:array_length("tags"), 3)
        primary:close()
        secondary:close()
        merged:close()
    end)

    fw.test("array_replace_strategy", function()
        local primary   = mdix.load_str(PRIMARY_SRC)
        local secondary = mdix.load_str(SECONDARY_SRC)
        local merged = primary:merge_with(secondary, "primary_wins", "replace")
        fw.assert_eq(merged:array_length("tags"), 2) -- primary's array wins whole
        primary:close()
        secondary:close()
        merged:close()
    end)

    fw.test("conflicts_have_expected_shape", function()
        local primary   = mdix.load_str(PRIMARY_SRC)
        local secondary = mdix.load_str(SECONDARY_SRC)
        local merged, conflicts = primary:merge_with(secondary, "primary_wins")
        local c = conflicts[1]
        fw.assert_not_nil(c.path)
        fw.assert_not_nil(c.winning_source)
        primary:close()
        secondary:close()
        merged:close()
    end)

    fw.test("unknown_strategy_raises", function()
        local primary   = mdix.load_str(PRIMARY_SRC)
        local secondary = mdix.load_str(SECONDARY_SRC)
        fw.assert_raises(function()
            primary:merge_with(secondary, "not_a_real_strategy")
        end, "strategy")
        primary:close()
        secondary:close()
    end)

    -- ── merge_files (real disk files — exercises the AST-from-path path) ──

    fw.suite("merge_files")

    fw.test("merges_two_files", function()
        local p1 = write_temp(PRIMARY_SRC)
        local p2 = write_temp(SECONDARY_SRC)
        local merged, conflicts = mdix.merge_files({p1, p2})
        fw.assert_not_nil(merged)
        fw.assert_true(#conflicts > 0)
        merged:close()
        os.remove(p1)
        os.remove(p2)
    end)

    fw.test("empty_paths_table_raises", function()
        fw.assert_raises(function() mdix.merge_files({}) end)
    end)

    fw.test("nonexistent_file_raises", function()
        fw.assert_raises(function() mdix.merge_files({"/nonexistent/path.mdix"}) end)
    end)

    fw.test("merge_files_weighted_higher_weight_wins", function()
        local p1 = write_temp(PRIMARY_SRC)
        local p2 = write_temp(SECONDARY_SRC)
        -- Secondary weighted higher despite being listed second.
        local merged = mdix.merge_files_weighted({{p1, 0.1}, {p2, 0.9}}, "weighted")
        fw.assert_eq(merged:get_string("app_name"), "SecondaryApp")
        merged:close()
        os.remove(p1)
        os.remove(p2)
    end)

end
