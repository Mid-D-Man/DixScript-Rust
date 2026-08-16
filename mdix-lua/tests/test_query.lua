-- Tests: MdixQuery — db:query(path), db:query_many(pattern), mdix.query(table).

return function(fw, mdix)

    local ENEMIES_SRC = [[
@DATA(
  enemies::
    { name = "Goblin",   hp = 50  },
    { name = "Orc",      hp = 120 },
    { name = "Orc",      hp = 120 },
    { name = "Dragon",   hp = 900 },
    { name = "Skeleton", hp = 40  }
)
]]

    -- ── Construction ─────────────────────────────────────────────────────

    fw.suite("query_construction")

    fw.test("db_query_returns_queryable", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local q = db:query("enemies")
        fw.assert_not_nil(q)
        fw.assert_eq(q:count(), 5)
        db:close()
    end)

    fw.test("db_query_missing_path_raises", function()
        local db = mdix.load_str(ENEMIES_SRC)
        fw.assert_raises(function() db:query("does.not.exist") end)
        db:close()
    end)

    fw.test("db_query_non_array_path_raises", function()
        local db = mdix.load_str('@DATA( port = 8080 )')
        fw.assert_raises(function() db:query("port") end)
        db:close()
    end)

    fw.test("mdix_query_wraps_plain_table", function()
        local q = mdix.query({1, 5, 3, 2, 4})
        fw.assert_eq(q:count(), 5)
    end)

    fw.test("tostring_shows_count", function()
        local q = mdix.query({1, 2, 3})
        fw.assert_contains(tostring(q), "MdixQuery")
        fw.assert_contains(tostring(q), "3")
    end)

    fw.test("len_metamethod", function()
        local q = mdix.query({1, 2, 3, 4})
        fw.assert_eq(#q, 4)
    end)

    -- ── Filtering / projection ───────────────────────────────────────────

    fw.suite("query_filter_project")

    fw.test("where_filters_by_predicate", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local heavies = db:query("enemies"):where(function(e) return e.hp > 100 end)
        fw.assert_eq(heavies:count(), 3) -- Orc, Orc, Dragon
        db:close()
    end)

    fw.test("where_field_eq_shorthand", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local orcs = db:query("enemies"):where_field_eq("name", "Orc")
        fw.assert_eq(orcs:count(), 2)
        db:close()
    end)

    fw.test("select_projects_field", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local names = db:query("enemies"):select(function(e) return e.name end)
        fw.assert_eq(#names, 5)
        fw.assert_eq(names[1], "Goblin")
        fw.assert_eq(names[5], "Skeleton")
        db:close()
    end)

    fw.test("select_field_shorthand", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local names = db:query("enemies"):select_field("name")
        fw.assert_eq(names[1], "Goblin")
        db:close()
    end)

    fw.test("skip_and_take", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local q = db:query("enemies")
        fw.assert_eq(q:skip(3):count(), 2)
        fw.assert_eq(q:take(2):count(), 2)
        db:close()
    end)

    fw.test("distinct_by_name", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local names = db:query("enemies"):select(function(e) return e.name end)
        local distinct_names = mdix.query(names):distinct()
        -- first-seen order, one Orc: Goblin, Orc, Dragon, Skeleton
        fw.assert_eq(distinct_names:count(), 4)
        db:close()
    end)

    -- ── Ordering ─────────────────────────────────────────────────────────

    fw.suite("query_ordering")

    fw.test("order_by_ascending", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local asc = db:query("enemies"):order_by(function(e) return e.hp end)
        local hps = asc:select(function(e) return e.hp end)
        fw.assert_eq(hps[1], 40)
        fw.assert_eq(hps[5], 900)
        db:close()
    end)

    fw.test("order_by_desc", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local desc = db:query("enemies"):order_by_desc(function(e) return e.hp end)
        local hps = desc:select(function(e) return e.hp end)
        fw.assert_eq(hps[1], 900)
        fw.assert_eq(hps[5], 40)
        db:close()
    end)

    fw.test("order_by_is_stable", function()
        -- Both Orcs have hp=120 -- their relative order must survive the sort.
        local db = mdix.load_str(ENEMIES_SRC)
        local sorted = db:query("enemies"):order_by(function(e) return e.hp end)
        local names = sorted:select(function(e) return e.name end)
        -- 40 (Skeleton), 50 (Goblin), 120 (Orc), 120 (Orc), 900 (Dragon)
        fw.assert_eq(names[3], "Orc")
        fw.assert_eq(names[4], "Orc")
        db:close()
    end)

    -- ── Grouping ─────────────────────────────────────────────────────────

    fw.suite("query_grouping")

    fw.test("group_by_name", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local groups = db:query("enemies"):group_by(function(e) return e.name end)
        fw.assert_eq(#groups, 4)
        -- "Orc" is the second distinct name to appear, with 2 members.
        fw.assert_eq(groups[2].key, "Orc")
        fw.assert_eq(#groups[2].items, 2)
        db:close()
    end)

    -- ── Terminal predicates / accessors ─────────────────────────────────

    fw.suite("query_terminal")

    fw.test("any_and_all", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local q = db:query("enemies")
        fw.assert_true(q:any(function(e) return e.hp > 800 end))
        fw.assert_false(q:all(function(e) return e.hp > 800 end))
        db:close()
    end)

    fw.test("is_empty", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local q = db:query("enemies")
        fw.assert_false(q:is_empty())
        local empty = q:where(function(e) return e.hp > 100000 end)
        fw.assert_true(empty:is_empty())
        db:close()
    end)

    fw.test("first_last_nth", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local q = db:query("enemies")
        fw.assert_eq(q:first().name, "Goblin")
        fw.assert_eq(q:last().name, "Skeleton")
        fw.assert_eq(q:nth(3).name, "Orc") -- 1-indexed, Lua convention
        db:close()
    end)

    fw.test("nth_out_of_range_is_nil", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local q = db:query("enemies")
        fw.assert_nil(q:nth(0))
        fw.assert_nil(q:nth(999))
        db:close()
    end)

    fw.test("first_or_default_on_empty", function()
        local q = mdix.query({})
        fw.assert_eq(q:first_or("fallback"), "fallback")
    end)

    fw.test("first_on_empty_is_nil", function()
        local q = mdix.query({})
        fw.assert_nil(q:first())
    end)

    -- ── Aggregation ──────────────────────────────────────────────────────

    fw.suite("query_aggregation")

    fw.test("sum_int_of_hp", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local hps = db:query("enemies"):select(function(e) return e.hp end)
        local total = mdix.query(hps):sum_int()
        fw.assert_eq(total, 50 + 120 + 120 + 900 + 40)
        db:close()
    end)

    fw.test("avg_float_of_hp", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local hps = db:query("enemies"):select(function(e) return e.hp end)
        local avg = mdix.query(hps):avg_float()
        fw.assert_near(avg, (50 + 120 + 120 + 900 + 40) / 5, 0.01)
        db:close()
    end)

    fw.test("avg_float_on_empty_is_nil", function()
        fw.assert_nil(mdix.query({}):avg_float())
    end)

    fw.test("min_max_by_key", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local q = db:query("enemies")
        local weakest = q:min_by_key(function(e) return e.hp end)
        local strongest = q:max_by_key(function(e) return e.hp end)
        fw.assert_eq(weakest.name, "Skeleton")
        fw.assert_eq(strongest.name, "Dragon")
        db:close()
    end)

    -- ── to_table ─────────────────────────────────────────────────────────

    fw.suite("query_to_table")

    fw.test("to_table_returns_plain_sequence", function()
        local db = mdix.load_str(ENEMIES_SRC)
        local t = db:query("enemies"):where(function(e) return e.hp > 100 end):to_table()
        fw.assert_type(t, "table")
        fw.assert_eq(#t, 3)
        db:close()
    end)

    -- ── query_many (wildcard) ────────────────────────────────────────────

    fw.suite("query_many")

    fw.test("query_many_matches_sibling_paths", function()
        local db = mdix.load_str([[
@DATA(
  servers: web1 = "up", web2 = "down", web3 = "up"
)
]])
        local statuses = db:query_many("servers.*")
        fw.assert_eq(statuses:count(), 3)
        db:close()
    end)

    fw.test("query_many_no_match_is_empty_not_error", function()
        local db = mdix.load_str('@DATA( port = 8080 )')
        local q = db:query_many("nonexistent.*")
        fw.assert_true(q:is_empty())
        db:close()
    end)

end
