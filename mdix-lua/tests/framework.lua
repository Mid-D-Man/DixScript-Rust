-- mdix-lua/tests/framework.lua
-- Minimal test framework for mdix-lua CI.
-- Collects pass/fail per suite and outputs a single JSON blob to stdout.

local M = {}

M._suites     = {}
M._current    = nil

local function new_suite(name)
    return { name = name, passed = 0, failed = 0, tests = {}, _start = os.clock() }
end

-- Create / switch active suite.  Always call before fw.test().
function M.suite(name)
    local s = new_suite(name)
    table.insert(M._suites, s)
    M._current = s
end

M.suite("default")   -- ensure there is always a current suite

-- Register and immediately run one test.
function M.test(name, fn)
    local s     = M._current
    local t0    = os.clock()
    local ok, err = pcall(fn)
    local dur_ms = (os.clock() - t0) * 1000

    local entry = { name = name, duration_ms = dur_ms }
    if ok then
        entry.status = "passed"
        entry.output = ""
        s.passed = s.passed + 1
    else
        entry.status = "failed"
        entry.output = tostring(err)
        s.failed = s.failed + 1
    end
    table.insert(s.tests, entry)
end

-- ── Assertions ─────────────────────────────────────────────────────────────

function M.assert_eq(got, expected, msg)
    if got ~= expected then
        error(string.format("expected %s, got %s%s",
            tostring(expected), tostring(got),
            msg and ("\n  " .. msg) or ""), 2)
    end
end

function M.assert_ne(got, unexpected, msg)
    if got == unexpected then
        error(string.format("expected not %s%s",
            tostring(unexpected), msg and ("\n  " .. msg) or ""), 2)
    end
end

function M.assert_true(v, msg)
    if not v then error(msg or "expected true, got falsy", 2) end
end

function M.assert_false(v, msg)
    if v then error(msg or "expected false, got truthy", 2) end
end

function M.assert_nil(v, msg)
    if v ~= nil then
        error(string.format("expected nil, got %s%s",
            tostring(v), msg and ("\n  " .. msg) or ""), 2)
    end
end

function M.assert_not_nil(v, msg)
    if v == nil then error(msg or "expected non-nil, got nil", 2) end
end

function M.assert_near(got, expected, delta, msg)
    delta = delta or 0.001
    if math.abs(got - expected) > delta then
        error(string.format("expected ~%s (±%s), got %s%s",
            expected, delta, got, msg and ("\n  " .. msg) or ""), 2)
    end
end

function M.assert_contains(str, pattern, msg)
    if not tostring(str):find(pattern, 1, true) then
        error(string.format("expected string to contain %q, got %q%s",
            pattern, tostring(str), msg and ("\n  " .. msg) or ""), 2)
    end
end

function M.assert_type(v, expected_type, msg)
    local got = type(v)
    if got ~= expected_type then
        error(string.format("expected type %s, got %s%s",
            expected_type, got, msg and ("\n  " .. msg) or ""), 2)
    end
end

--- Assert that calling fn() raises an error matching optional pattern.
function M.assert_raises(fn, pattern, msg)
    local ok, err = pcall(fn)
    if ok then
        error(msg or "expected an error but none was raised", 2)
    end
    if pattern then
        local s = tostring(err)
        if not s:find(pattern) then
            error(string.format("expected error matching %q, got %q%s",
                pattern, s, msg and ("\n  " .. msg) or ""), 2)
        end
    end
end

-- ── JSON serialiser (no external deps) ────────────────────────────────────

local function json_str(s)
    s = tostring(s)
    s = s:gsub('\\', '\\\\'):gsub('"', '\\"')
         :gsub('\n','\\n'):gsub('\r','\\r'):gsub('\t','\\t')
    return '"' .. s .. '"'
end

local function to_json(v)
    local t = type(v)
    if t == "nil"     then return "null"
    elseif t == "boolean" then return v and "true" or "false"
    elseif t == "number"  then
        if v ~= v then return "null" end
        return string.format("%.10g", v)
    elseif t == "string" then return json_str(v)
    elseif t == "table"  then
        -- sequence check
        local n = #v
        local is_seq = n > 0
        if is_seq then
            for k in pairs(v) do
                if type(k) ~= "number" or k < 1 or k > n or math.floor(k) ~= k then
                    is_seq = false; break
                end
            end
        end
        if is_seq then
            local parts = {}
            for _, item in ipairs(v) do parts[#parts+1] = to_json(item) end
            return "[" .. table.concat(parts, ",") .. "]"
        else
            local parts = {}
            for k, item in pairs(v) do
                parts[#parts+1] = json_str(k) .. ":" .. to_json(item)
            end
            return "{" .. table.concat(parts, ",") .. "}"
        end
    end
    return "null"
end

-- ── summary — outputs JSON and exits ──────────────────────────────────────

function M.summary()
    local total_p, total_f, total_dur = 0, 0, 0.0

    for _, s in ipairs(M._suites) do
        local dur = 0
        for _, t in ipairs(s.tests) do dur = dur + t.duration_ms end
        s.duration_s = dur / 1000.0
        total_p = total_p + s.passed
        total_f = total_f + s.failed
        total_dur = total_dur + s.duration_s
    end

    local result = {
        build       = os.getenv("BUILD_NUM")  or "0",
        branch      = os.getenv("BRANCH")     or "unknown",
        commit      = (os.getenv("COMMIT") or "unknown"):sub(1, 8),
        date        = os.getenv("BUILD_DATE") or "",
        lua_version = _VERSION,
        tests = {
            total      = total_p + total_f,
            passed     = total_p,
            failed     = total_f,
            duration_s = total_dur,
        },
        suites = M._suites,
    }

    io.write(to_json(result))
    io.write("\n")
    io.flush()

    os.exit(total_f > 0 and 1 or 0)
end

return M
