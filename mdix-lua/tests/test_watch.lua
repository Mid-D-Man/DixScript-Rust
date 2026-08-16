-- Tests: mdix.watch() -- MdixWatcher:check/force_reload/has_changed/has_loaded/path.

return function(fw, mdix)

    local function write_file(path, content)
        local f = assert(io.open(path, "w"))
        f:write(content)
        f:close()
    end

    local function temp_path()
        return os.tmpname() .. ".mdix"
    end

    -- Sleeps past a full second boundary so a rewritten file's mtime is
    -- unambiguously different -- mtime resolution is coarse (~1s) on some
    -- filesystems. os.execute("sleep") is Unix-only, matching lua-ci.yml
    -- (ubuntu-latest only for now); revisit if this suite ever needs to
    -- run on a Windows runner.
    local function sleep_past_mtime_boundary()
        os.execute("sleep 1.1")
    end

    -- ── Construction / path() ────────────────────────────────────────────

    fw.suite("watch_construction")

    fw.test("path_returns_watched_file", function()
        local p = temp_path()
        write_file(p, "@DATA( x = 1 )")
        local watcher = mdix.watch(p)
        fw.assert_eq(watcher:path(), p)
        os.remove(p)
    end)

    fw.test("empty_path_raises", function()
        fw.assert_raises(function() mdix.watch("") end)
    end)

    fw.test("has_loaded_false_before_first_check", function()
        local p = temp_path()
        write_file(p, "@DATA( x = 1 )")
        local watcher = mdix.watch(p)
        fw.assert_false(watcher:has_loaded())
        os.remove(p)
    end)

    fw.test("tostring_shows_path", function()
        local p = temp_path()
        write_file(p, "@DATA( x = 1 )")
        local watcher = mdix.watch(p)
        fw.assert_contains(tostring(watcher), "MdixWatcher")
        os.remove(p)
    end)

    -- ── check() ──────────────────────────────────────────────────────────

    fw.suite("watch_check")

    fw.test("first_check_loads_and_reports_changed", function()
        local p = temp_path()
        write_file(p, "@DATA( value = 1 )")
        local watcher = mdix.watch(p)

        local db, changed = watcher:check()
        fw.assert_true(changed)
        fw.assert_not_nil(db)
        fw.assert_eq(db:get_int("value"), 1)
        fw.assert_true(watcher:has_loaded())

        db:close()
        os.remove(p)
    end)

    fw.test("second_check_without_changes_reports_unchanged", function()
        local p = temp_path()
        write_file(p, "@DATA( value = 1 )")
        local watcher = mdix.watch(p)

        local db1 = watcher:check()
        db1:close()

        local db2, changed = watcher:check()
        fw.assert_false(changed)
        fw.assert_nil(db2)

        os.remove(p)
    end)

    fw.test("check_picks_up_a_real_file_change", function()
        local p = temp_path()
        write_file(p, "@DATA( value = 1 )")
        local watcher = mdix.watch(p)

        local db1 = watcher:check()
        fw.assert_eq(db1:get_int("value"), 1)
        db1:close()

        sleep_past_mtime_boundary()
        write_file(p, "@DATA( value = 2 )")

        local db2, changed = watcher:check()
        fw.assert_true(changed)
        fw.assert_eq(db2:get_int("value"), 2)
        db2:close()

        os.remove(p)
    end)

    fw.test("check_on_missing_file_raises", function()
        local p = temp_path() -- deliberately never written
        local watcher = mdix.watch(p)
        fw.assert_raises(function() watcher:check() end)
    end)

    -- ── has_changed() (peek without reloading) ──────────────────────────

    fw.suite("watch_has_changed")

    fw.test("has_changed_true_before_first_check", function()
        local p = temp_path()
        write_file(p, "@DATA( x = 1 )")
        local watcher = mdix.watch(p)
        fw.assert_true(watcher:has_changed()) -- never loaded yet -> counts as changed
        os.remove(p)
    end)

    fw.test("has_changed_does_not_itself_reload", function()
        local p = temp_path()
        write_file(p, "@DATA( x = 1 )")
        local watcher = mdix.watch(p)

        watcher:check() -- establishes baseline
        fw.assert_false(watcher:has_changed()) -- peeking again: nothing changed
        fw.assert_false(watcher:has_changed()) -- calling it twice must not itself count as a change

        os.remove(p)
    end)

    fw.test("has_changed_true_after_real_change", function()
        local p = temp_path()
        write_file(p, "@DATA( x = 1 )")
        local watcher = mdix.watch(p)
        local db1 = watcher:check()
        db1:close()

        sleep_past_mtime_boundary()
        write_file(p, "@DATA( x = 2 )")

        fw.assert_true(watcher:has_changed())
        os.remove(p)
    end)

    -- ── force_reload() ───────────────────────────────────────────────────

    fw.suite("watch_force_reload")

    fw.test("force_reload_reloads_even_without_a_change", function()
        local p = temp_path()
        write_file(p, "@DATA( value = 1 )")
        local watcher = mdix.watch(p)
        watcher:check()

        -- No file change at all -- force_reload must still succeed and
        -- return a usable database, unlike check().
        local db = watcher:force_reload()
        fw.assert_not_nil(db)
        fw.assert_eq(db:get_int("value"), 1)

        db:close()
        os.remove(p)
    end)

end
