<!-- mdix-lua/README.md -->
# mdix-lua — DixScript Lua Bindings

Lua 5.4 bindings for the DixScript (`.mdix`) runtime.
Built on the same core Rust library as the C#, Go, Python, and Java wrappers.

> **NOT PRODUCTION READY** — runtime incomplete, API may change.
> Add `"mdix-lua"` to the workspace `members` array in the root `Cargo.toml` before building.

---

## Building
```bash
cargo build -p mdix-lua --release
```

The output is `target/release/libmdix.so` (Linux), `libmdix.dylib` (macOS),
or `mdix.dll` (Windows).

Lua's `require("mdix")` looks for `mdix.so` — rename or symlink the file:
```bash
# Linux
ln -s target/release/libmdix.so mdix.so

# macOS
ln -s target/release/libmdix.dylib mdix.so

# Windows — already named mdix.dll, no rename needed
```

Then place `mdix.so` / `mdix.dll` somewhere on `package.cpath`, or run Lua
from the directory that contains it.

---

## Quick start
```lua
local mdix = require("mdix")

-- Load from file
local db = mdix.load("config.mdix")

-- Load from string
local db = mdix.load_str([[
  @DATA(
    app_name = "AirStrike"
    port     = 7777
    debug    = false
    version  = "1.0.0"
  )
]])

-- Read values
local name = db:get_string("app_name")   -- "AirStrike"
local port = db:get_int("port")          -- 7777
local flag = db:get_bool("debug")        -- false

-- get() auto-converts to the best Lua type
local ver  = db:get("version")           -- "1.0.0"
local port = db:get("port")              -- 7777 (integer)

-- Optional default values (no error if path is missing)
local host = db:get_string("server.host", "localhost")
local cap  = db:get_int("max_players",   100)

-- Check existence
if db:exists("server.host") then ... end

-- Nested / dotted paths
local host = db:get_string("server.host")
local port = db:get_int("server.port")

-- Arrays
local len   = db:array_length("enemies")    -- number of items
local first = db:get("enemies[0]")          -- first item (table)
local name  = db:get_string("enemies[0].name")

-- Child key listing
local top_keys    = db:keys()           -- top-level keys
local server_keys = db:keys("server")  -- children of "server"

-- Type inspection
local t = db:get_type("port")   -- "int", "string", "bool", "array", ...

-- Export
local json = db:to_json()       -- pretty JSON string
local json = db:to_json(false)  -- compact JSON
local toml = db:to_toml()
local mdix_src = db:to_mdix()

-- Cleanup (optional — GC handles it automatically)
db:close()

print(tostring(db))  -- MdixDatabase(entries=4)
```

---

## Encrypted files
```lua
-- Key-file mode (auto-detects .mdix.key next to the .enc file)
local db = mdix.load_encrypted("secrets.mdix.enc")

-- Explicit key file path
local db = mdix.load_encrypted("secrets.mdix.enc", "/secure/keys/secrets.mdix.key")

-- Password mode
local db = mdix.load_encrypted_password("secrets.mdix.enc", "my-password")
```

---

## Foreign format import
```lua
local db = mdix.from_json('{"port": 7777, "host": "localhost"}')
local db = mdix.from_toml('port = 7777\nhost = "localhost"\n')
```

---

## Builder

The builder creates `.mdix` data programmatically.

### Two-tier ordering rule

DixScript's `@DATA` section has two tiers:
1. **Flat properties** — simple `key = value` pairs
2. **Grouped data** — table properties (`:`) and group arrays (`::`)

**All flat properties must come before any grouped data.**
Calling `set_*` after `with_table()` or `with_array()` raises an error immediately.
```lua
local b = mdix.builder()

-- @CONFIG (optional)
b:set_config("version", "1.0.0")
b:set_config("author", "MidManStudio")

-- @ENUMS (optional)
b:add_enum("LogLevel", {"DEBUG", "INFO", "WARN", "ERROR"})   -- auto-increment
b:add_enum("Status",   {{"ACTIVE", 1}, {"INACTIVE", 0}})     -- explicit values

-- @DATA tier 1 — flat properties (MUST come first)
b:set_string("app_name", "AirStrike")
b:set_int("port",         7777)
b:set_bool("debug",       false)
b:set_number("gravity",   9.81)
b:set_date("release",     "2025-12-31")
b:set_hex_color("sky",    "#87CEEB")
b:set_enum("log_level",  "LogLevel", "INFO")  -- references enum above

-- @DATA tier 2 — grouped (MUST come after all flat properties)
b:with_table("server", {host = "localhost", port = 7777, ssl = false})

b:with_array("enemies", {
    {name = "Goblin", hp = 50,  damage = 10},
    {name = "Orc",    hp = 100, damage = 20},
    {name = "Dragon", hp = 500, damage = 80},
})

b:with_array("tags", {"alpha", "beta", "release-candidate"})

-- Build into a readable database
local db = b:build()
local name = db:get_string("app_name")      -- "AirStrike"
local port = db:get_int("server.port")      -- 7777
local len  = db:array_length("enemies")     -- 3

-- Or just get the .mdix source string
local src = b:serialize()
print(src)

-- Reset grouped data only (keep flat properties and re-use)
b:reset_grouped()

-- Full reset
b:reset()

print(tostring(b))  -- MdixBuilder(flat=0, tables=0, arrays=0)
```

### Builder method reference

| Method | Description |
|--------|-------------|
| `set_config(key, value)` | Add a `@CONFIG` entry |
| `add_enum(name, fields)` | Add a `@ENUMS` declaration |
| `set_string(path, value)` | Flat string property |
| `set_int(path, value)` | Flat integer property |
| `set_number(path, value)` | Flat float/double property |
| `set_bool(path, value)` | Flat boolean property |
| `set_date(path, "YYYY-MM-DD")` | Flat date property |
| `set_hex_color(path, "#RRGGBB")` | Flat hex color property |
| `set_blob(path, base64)` | Flat base64 blob property |
| `set_regex(path, pattern)` | Flat regex property |
| `set_enum(path, enum, field)` | Flat enum reference |
| `set(path, value)` | Flat property, auto-detects type |
| `with_table(path, table)` | Tier-2 table property block |
| `with_array(path, table)` | Tier-2 group array |
| `build()` | Returns a loaded `MdixDatabase` |
| `serialize()` | Returns the `.mdix` source string |
| `reset_grouped()` | Clears tier-2 data only |
| `reset()` | Clears everything |

---

## Error handling

All errors are raised as Lua errors. Use `pcall` to catch them:
```lua
local ok, result = pcall(function()
    return mdix.load("missing.mdix")
end)
if not ok then
    print("Load failed: " .. result)
end

-- Getters with defaults never raise:
local host = db:get_string("server.host", "localhost")

-- Getters without defaults raise on missing path:
local ok, val = pcall(function() return db:get_string("missing.key") end)
```

---

## Enum values

Enum values are returned as a Lua table:
```lua
local val = db:get("log_level")
print(val.enum_name)  -- "LogLevel"
print(val.field)      -- "INFO"
print(val.value)      -- 1  (the resolved integer)
```

---

## Metadata
```lua
print(mdix.version)  -- "1.0.0"
```
