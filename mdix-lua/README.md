<!-- mdix-lua/README.md -->
# mdix-lua — DixScript Lua Bindings

Lua 5.4 bindings for the DixScript (`.mdix`) runtime.
Built on the same core Rust library as the C#, Go, Odin, Python,
WASM/npm, and Java wrappers.

## Documentation Site https://dixscript-docs.pages.dev

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

### Cross-platform notes (mlua's `module` feature)

This crate builds with mlua's `module` feature, not `vendored` — it's
meant to be `require()`'d into a host process that already embeds its own
Lua interpreter (a game's modding layer, an editor's scripting console),
so it deliberately leaves every `lua_*`/`luaL_*` symbol unresolved at
build time, to be resolved dynamically against whatever process loads
it. See `Cargo.toml`'s own comment for the full "why not vendored"
reasoning.

That has different, real implications per platform — checked directly
against `mlua-sys`'s build script rather than assumed:

- **Linux** — no special handling needed. This is the standard, decades-old
  pattern for loadable Lua C modules; the default linker already permits
  a shared object with unresolved symbols like that. `lua-ci.yml`
  validates this for real on every run — system `lua5.4` loading a
  `module`-built `mdix.so`.
- **macOS** — needs the linker told explicitly that's intentional
  (`-undefined dynamic_lookup`), which `mlua-sys` does not add
  automatically. `mdix-lua/build.rs` supplies it for `target_os =
  "macos"` — without that file, `cargo build --target
  x86_64-apple-darwin` / `aarch64-apple-darwin` fails at the link step.
- **Windows** — `mlua-sys` links via Rust's `raw-dylib` feature
  automatically (no Lua headers/`.lib` needed to build), but the
  resulting `mdix.dll` hard-requires an actual DLL named `lua54.dll` to
  be loaded in the host process at runtime — that's the literal name
  baked into every FFI declaration
  (`#[link(name = "lua54", kind = "raw-dylib")]`). A host that statically
  links Lua straight into its own `.exe` with no separate `lua54.dll`
  will fail to load this module on Windows, even though the identical
  setup works fine on Linux/macOS.

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

## Query

DixScript's core `DixQuery` closures can't cross into Lua the same way
they can't cross into any FFI boundary — but this crate links directly
against the `dixscript` crate, so `db:query(path)` builds off
`DixData::query()`'s real `Vec<DixValue>` result directly (via the same
`dix_to_lua` conversion `db:get()` already uses), no JSON round trip
needed. Every predicate/key/selector is a plain Lua function:

```lua
local heavies = db:query("enemies"):where(function(e) return e.hp > 500 end)
local names = heavies:select(function(e) return e.name end)
local sorted = db:query("enemies"):order_by_desc(function(e) return e.hp end)
local groups = db:query("enemies"):group_by(function(e) return e.name end)
for _, group in ipairs(groups) do
    print(group.key, #group.items)
end

-- Sibling paths sharing shape, wildcarding one segment:
local statuses = db:query_many("servers.*.status")

-- Query arbitrary Lua data too, not just a loaded Database's fields:
local total = mdix.query({1, 5, 3, 2, 4}):sum_int()
```

Also available: `where_field_eq`, `select_field`, `skip`, `take`,
`distinct`, `any`, `all`, `count`, `is_empty`, `first(_or)`, `last`,
`nth` (1-indexed), `sum_int`/`sum_float`, `avg_float`, `min_by_key`,
`max_by_key`, `to_table`, `#query` (via `__len`).

---

## Merge

Exposed at module level, not as a `Database` method, since merging
operates over files/ASTs rather than an already-resolved `DixData` — see
`merge.rs`'s own header comment for why this is the real AST-level
merger and not a hand-written deep-merge:

```lua
local db, conflicts = mdix.merge_files({"base.mdix", "patch.mdix"})
local db, conflicts = mdix.merge_files({"base.mdix", "patch.mdix"}, "primary_wins")
local db, conflicts = mdix.merge_files_weighted(
    {{"base.mdix", 1.0}, {"patch.mdix", 0.8}}, "weighted")

for _, c in ipairs(conflicts) do
    print(c.path, c.winning_source, c.winning_label)
end

-- Two already-loaded databases:
local merged, conflicts = db1:merge_with(db2, "primary_wins", "concat")
```

`strategy`: `"weighted"` (default) | `"primary_wins"` | `"secondary_wins"` | `"throw_on_conflict"`
`array_strategy`: `"concat_dedup"` (default) | `"replace"` | `"concat"`

---

## Schema

No schema-validation C ABI exists for this to bind to even if it wanted
to, so — same as every other binding — this validates client-side. All
`require_*`/`optional_*`/`with_description` calls chain and return the
same schema, so this reads the way it looks:

```lua
local schema = mdix.schema()
    :require_string("app_name")
    :require_int("port")
    :require_long("created_at_ms")   -- also accepts Int values (widened)
    :optional_bool("debug")
    :with_description("app configuration")

local report = db:validate_schema(schema)
if not report:is_valid() then
    for _, e in ipairs(report:errors()) do
        print(e.path, e.expected, e.actual, e.kind)
    end
end
```

A schema is reusable — `validate_schema` only reads from the `Database`
you pass it, so the same schema can validate any number of databases.

Field types: `string`, `int`, `long`, `float`, `double`, `bool`, `array`, `object`, `enum`.

---

## Hot reload

```lua
local watcher = mdix.watch("config.mdix")

-- in your game loop / tick / update:
local db, changed = watcher:check()
if changed then
    apply_new_config(db)
end

-- Force a reload regardless of whether the file changed:
local db = watcher:force_reload()

-- Peek without reloading:
if watcher:has_changed() then ... end
```

`db` is `nil` when nothing changed (`changed == false`) — keep using the
previously loaded database instance in that case. Poll-based (checks
mtime), same reasoning as every other binding's hot reload — see
`dixscript/src/Runtime/hot_reload.rs`.

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

## Testing

```bash
cargo build -p mdix-lua --release
ln -sf ../../target/release/libmdix.so mdix-lua/tests/mdix.so   # or .dylib
cd mdix-lua/tests
lua5.4 run_tests.lua
```

`run_tests.lua` runs every `test_*.lua` module listed in its own
`test_modules` table (not auto-discovered by filename — add new test
files to that list explicitly) against the framework in `framework.lua`.
`lua-ci.yml` does exactly this against a real build on every run and
publishes results to the `Lua Tests` card on the landing page.

---

## Metadata
```lua
print(mdix.version)  -- "1.0.0"
```
