-- mdix-lua/tests/run_tests.lua
-- Master test runner.  Outputs a single JSON blob to stdout.
-- All other output goes to stderr so the CI runner can capture JSON cleanly.
--
-- Usage (from repo root):
--   LUA_CPATH="mdix-lua/tests/?.so;;" lua5.4 mdix-lua/tests/run_tests.lua
--
-- Or copy libmdix.so → mdix-lua/tests/mdix.so and run from that directory.

-- ── Path setup ────────────────────────────────────────────────────────────────

local script_dir = (arg and arg[0] or ""):match("^(.*[/\\])") or "./"
package.path  = script_dir .. "?.lua;" .. package.path
package.cpath = script_dir .. "?.so;"
             .. script_dir .. "?.dll;"
             .. script_dir .. "?.dylib;"
             .. package.cpath

-- ── Load framework ────────────────────────────────────────────────────────────

local ok_fw, fw = pcall(require, "framework")
if not ok_fw then
    -- Can't even load the framework — emit minimal failure JSON and exit.
    io.write(string.format(
        '{"build":"%s","branch":"%s","commit":"%s","date":"%s",'
     .. '"lua_version":"%s","tests":{"total":1,"passed":0,"failed":1,"duration_s":0},'
     .. '"suites":[{"name":"bootstrap","passed":0,"failed":1,"duration_s":0,'
     .. '"tests":[{"name":"load_framework","status":"failed","duration_ms":0,'
     .. '"output":%q}]}]}\n',
        os.getenv("BUILD_NUM")  or "0",
        os.getenv("BRANCH")     or "unknown",
        (os.getenv("COMMIT") or "unknown"):sub(1,8),
        os.getenv("BUILD_DATE") or "",
        _VERSION,
        tostring(fw)
    ))
    os.exit(1)
end

-- ── Load mdix module ──────────────────────────────────────────────────────────

local ok_mdix, mdix = pcall(require, "mdix")

if not ok_mdix then
    io.stderr:write("[run_tests] WARNING: cannot load mdix module: " .. tostring(mdix) .. "\n")
    io.stderr:write("[run_tests] Tests will be registered as failures.\n")

    fw.suite("mdix_module")
    fw.test("require('mdix')", function()
        error("Cannot load mdix: " .. tostring(mdix))
    end)

    fw.summary()
    return
end

io.stderr:write("[run_tests] mdix loaded — Lua " .. _VERSION .. "\n")

-- ── Load and run test files ───────────────────────────────────────────────────

local test_modules = {
    "test_basic",
    "test_builder",
    "test_types",
}

for _, name in ipairs(test_modules) do
    io.stderr:write("[run_tests] Loading " .. name .. "\n")
    local ok, result = pcall(function()
        local mod = require(name)
        if type(mod) == "function" then
            mod(fw, mdix)
        else
            error(name .. " did not return a function")
        end
    end)
    if not ok then
        io.stderr:write("[run_tests] ERROR loading " .. name .. ": " .. tostring(result) .. "\n")
        fw.suite(name .. "_load_error")
        fw.test("load_" .. name, function() error(tostring(result)) end)
    end
end

-- ── Emit results ──────────────────────────────────────────────────────────────

fw.summary()
