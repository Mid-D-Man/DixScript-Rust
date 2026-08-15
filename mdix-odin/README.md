# mdix-odin — DixScript Odin Bindings

Odin bindings for loading `.mdix` files, built on the same `mdix_ffi`
native library used by the Rust, Python, C#, WASM/npm, and Go wrappers.

## Documentation Site https://dixscript-docs.pages.dev

---

## Directory layout

```
mdix-odin/
├── mdix_ffi/
│   └── mdix_ffi.odin      # raw foreign-import bindings (package mdix_ffi)
├── mdix/                  # idiomatic wrapper (package mdix)
│   ├── mdix.odin            # Database, Builder, (value, ok) getters, Convert
│   ├── query.odin            # Query(T) — where/select/order_by/group_by/...
│   ├── merge.odin             # merge_sources / merge_sources_weighted
│   ├── schema.odin             # Schema_Builder — client-side validation
│   ├── watch.odin                # Hot_Reload — explicit poll-check, not a thread
│   ├── types.odin                # Hex_Color, Blob, Mdix_Regex, Mdix_Date, Mdix_Timestamp
│   └── tests/                    # package mdix_tests — odin test mdix/tests
└── examples/
    └── hello.odin
```

---

## Build the native library

```bash
cargo build --release -p mdix-ffi
# Linux:   target/release/libmdix_ffi.so
# macOS:   target/release/libmdix_ffi.dylib
# Windows: target/release/mdix_ffi.dll + mdix_ffi.lib
```

## Linking from Odin

`mdix_ffi/mdix_ffi.odin` uses `foreign import "system:mdix_ffi"`
(`"system:mdix_ffi.lib"` on Windows). Point the linker at the directory
containing the built library:

```bash
odin build . -extra-linker-flags:"-L/path/to/lib -Wl,-rpath,/path/to/lib"
odin build . -extra-linker-flags:"/LIBPATH:C:\path\to\lib"   # Windows/MSVC
```

Or drop the library next to your executable, or install it to a standard
system path — same platform notes as `mdix-c/README.md`, identical artifact.

Running the test suite the same way:

```bash
odin test mdix/tests -extra-linker-flags:"-L/path/to/lib -Wl,-rpath,/path/to/lib"
```

---

## API shape

Every read returns `(value, ok)` instead of the C API's null-sentinel +
`mdix_get_last_error()` pattern. String-returning procs accept an optional
`allocator` (defaults to `context.allocator`); the returned string is
caller-owned — `delete()` it. Path/value arguments going *in* are
converted via `context.temp_allocator`; call `free_all(context.temp_allocator)`
yourself in long-running loops with no surrounding temp scope.

This toolchain doesn't support method-call chaining (`x->f()` was tried
and rejected by the compiler — see schema.odin's header comment), so
unlike the fluent `.RequireString(...).RequireInt(...)` style in the Go
and C# bindings, builder-style APIs here (`Builder`, `Schema_Builder`) are
called as a plain sequence of procs against a pointer, matching how
`builder_set_*` already worked before this pass added `Schema_Builder`:

```odin
b := mdix.builder_new()
defer mdix.builder_destroy(&b)
mdix.builder_set_string(b, "name", "player1")
mdix.builder_set_int(b, "level", 42)
```

### Load / Build / Convert

```odin
db, ok := mdix.load("config.mdix")           // from file
db, ok := mdix.load_str(src)                   // from string
defer mdix.destroy(&db)

port, ok := mdix.get_int(db, "server.port")
name, ok := mdix.get_string(db, "app_name")    // caller-owned, delete() it
long_val, ok := mdix.get_long(db, "big_id")    // genuine 64-bit — distinct from get_int

b := mdix.builder_new()
defer mdix.builder_destroy(&b)
mdix.builder_set_string(b, "app_name", "Widget")
db2, ok := mdix.builder_to_database(b)
```

### Query

Odin's parametric polymorphism (`Query($T)`) stands in for DixScript's
core `DixQuery`, whose closures can't cross the FFI boundary — same
reasoning as the Go/Python/C# bindings: fetch the array natively, decode
with `core:encoding/json`'s reflection-based `unmarshal`, query the
resulting Odin slice with Odin's own tools.

```odin
Enemy :: struct {
    name: string `json:"name"`,
    hp:   int    `json:"hp"`,
}

q, ok := mdix.query_load(Enemy, db, "enemies")
defer mdix.query_delete(q)
heavies := mdix.query_where(q, proc(e: Enemy) -> bool { return e.hp > 500 })
defer mdix.query_delete(heavies)
names := mdix.query_select(q, proc(e: Enemy) -> string { return e.name })
defer delete(names)

// Sibling paths sharing shape, wildcarding one segment:
statuses, ok := mdix.query_many(string, db, "servers.*.status")
```

Also available: `query_skip`, `query_take`, `query_any`, `query_all`,
`query_count`, `query_is_empty`, `query_first(_or)`, `query_last`,
`query_nth`, `query_order_by(_desc)`, `query_distinct`, `query_group_by`,
`query_min/max_by_key`, `query_sum_int/float`, `query_avg_float`.

### Merge

```odin
db, conflicts, ok := mdix.merge_sources(
    {base_src, override_src},
    .Primary_Wins,
    .Concat,
)
defer mdix.destroy(&db)
defer delete(conflicts)

for c in conflicts {
    fmt.printf("%s: source %d won\n", c.path, c.winning_source)
}

// Or with explicit per-source weights:
db, conflicts, ok := mdix.merge_sources_weighted(sources, weights, .Weighted_Priority, .Replace)
```

### Schema

`mdix-ffi` has no schema-validation C ABI, so — same as the Go and C#
bindings — this validates client-side, purely with `exists`/`get_type`:

```odin
s := mdix.schema_new()
defer mdix.schema_destroy(&s)
mdix.schema_require_string(&s, "app_name")
mdix.schema_require_int(&s, "port")
mdix.schema_optional_bool(&s, "debug")

report := mdix.schema_validate(s, db)
defer mdix.validation_report_destroy(&report)
if !mdix.validation_report_is_valid(report) {
    for e in report.errors {
        fmt.println(mdix.validation_error_to_string(e))
    }
}
```

### Hot reload

Deliberately **not** a background thread, unlike Go's `EnableHotReload`
(goroutine + ticker). This package's usual consumer already has its own
per-frame loop (a game, an editor, a renderer) — an explicit poll call
fits that better than a second thread with a mutex around the handle
would:

```odin
hr: mdix.Hot_Reload
mdix.hot_reload_init(&hr, "config.mdix")
defer mdix.hot_reload_destroy(&hr)

for /* your main loop */ {
    if mdix.hot_reload_check(&hr, &db) {
        fmt.println("config reloaded")
    }
    // ... rest of frame
}
```

On a successful reload, `db`'s handle is swapped in place — every other
reference to the same `Database` sees fresh data immediately.

### Typed convenience getters

`Hex_Color`, `Blob`, `Mdix_Regex`, `Mdix_Date`, `Mdix_Timestamp` all
parse from `get_string`'s output client-side (mdix-ffi's canonical form
for each of these *is* a string) — no separate FFI calls needed:

```odin
color, ok := mdix.get_hex_color(db, "primary_color")   // Hex_Color{r,g,b,a: f32}
blob, ok  := mdix.get_blob(db, "icon_data")              // Blob; blob_bytes(blob) to decode
re, ok    := mdix.get_regex(db, "validation_pattern")    // Mdix_Regex; regex_compile(re) via core:text/regex
date, ok  := mdix.get_date(db, "release_date")            // Mdix_Date{value: time.Time}
ts, ok    := mdix.get_timestamp(db, "created_at")          // Mdix_Timestamp{value: time.Time}
val, ok   := mdix.get_enum_value(db, "ai_type")              // resolved int — same as get_int
```

---

## Testing

```bash
odin test mdix/tests -extra-linker-flags:"-L/path/to/lib -Wl,-rpath,/path/to/lib"
```

`mdix/tests` is its own package (`package mdix_tests`) importing `mdix`
as a dependency, rather than test files living inside `mdix/` itself —
keeps `core:testing` and every `@(test)` proc out of what a consumer of
this package actually builds. Odin's native test runner also has a
built-in structured report: `-define:ODIN_TEST_JSON_REPORT=results.json`
(used by `odin-ci.yml`).

`types_test.odin` and `query_test.odin` need no native library at all —
`Hex_Color`/`Mdix_Date`/`Mdix_Timestamp` parsing and `Query(T)` built via
`query_new` over a plain Odin slice are pure logic. The rest
(`database_test.odin`, `builder_test.odin`, `merge_test.odin`,
`schema_test.odin`, most of `watch_test.odin`) call into the real FFI and
need `libmdix_ffi` built and linked per the flags above.

---

## vs Go package

| Concept | Go | Odin |
|---|---|---|
| Load | `dixscript.Load(path)` | `mdix.load(path)` |
| Read | `db.GetInt("path")` → `(int, error)` | `mdix.get_int(db, "path")` → `(int, bool)` |
| Build | `dixscript.NewBuilder()` | `mdix.builder_new()` |
| Chaining | fluent `.Where(...).OrderBy(...)` | sequential calls (no `x->f()` in this toolchain) |
| Query | `dixscript.LoadQuery[Enemy](db, path)` | `mdix.query_load(Enemy, db, path)` |
| Merge | `dixscript.MergeSources(...)` | `mdix.merge_sources(...)` |
| Schema | `dixscript.NewSchema()` (client-side) | `mdix.schema_new()` (client-side, same reason) |
| Hot reload | goroutine + ticker, event callbacks | explicit `hot_reload_check()` from your own loop |
| Memory | GC — no manual free | explicit `allocator` params + `delete()`/`defer` |
| FFI glue | cgo (`internal/ffi.go`) | `foreign import "system:mdix_ffi"` |
| Errors | `(T, error)` with `*MdixError` | `(T, bool)` + `last_error()` |

---

## License

MIT — see repository root LICENSE.
