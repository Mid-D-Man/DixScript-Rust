# mdix-zig — DixScript Zig Bindings

Zig bindings for loading `.mdix` files, built on the same `mdix_ffi`
native library used by the Rust, Python, C#, WASM/npm, Go, and Odin
wrappers.

## Documentation Site https://dixscript-docs.pages.dev

---

## Status

🚧 **Code-complete, not yet published.** Every module `mdix-odin` has is
now ported. This is the newest wrapper in the ecosystem; it isn't in the
root README's publish table yet. Tracking against `mdix-odin` as the
closest sibling (same "no built-in FFI convenience layer" situation
Go/Python/C#/WASM don't have to deal with).

| Layer | Status |
|---|---|
| `mdix_ffi/mdix_ffi.zig` — raw `extern` bindings | ✅ Done — all 72 exported `mdix_*` symbols, verified 1:1 against `mdix-c/include/mdix.h` |
| `build.zig` / `build.zig.zon` | ✅ Done — targets Zig 0.16, fingerprint filled in from a real `zig build` run (CI #1) |
| `examples/hello.zig` | ✅ Done — raw-layer only (manual `mdix_free`/`mdix_free_string`) |
| `mdix/mdix.zig` — `Database`, `Builder`, source-text utilities | ✅ Done — error-union getters (`!T`), full surface of `mdix-odin/mdix/mdix.odin` |
| `mdix/watch.zig` — hot reload (`HotReload`, re-exported as `mdix.HotReload`) | ✅ Done — native `std.fs` stat polling, not the C `MdixWatcher` handle, same design choice as `watch.odin` |
| `mdix/types.zig` — `HexColor`/`Blob`/`MdixRegex`/`MdixDate`/`MdixTimestamp` | ✅ Done — two deliberate gaps vs. Odin: no calendar-instant conversion, no regex `compile()` (see the file's doc comment) |
| `mdix/merge.zig` — weighted AST merge | ✅ Done — `mergeSources`/`mergeSourcesWeighted`, JSON conflict-report decoding via `std.json` |
| `mdix/schema.zig` — client-side schema validation | ✅ Done — `SchemaBuilder`, full report (doesn't stop at the first error) |
| `mdix/query.zig` — `Query(T)` over JSON-decoded structs | ✅ Done — comptime-generic `Query(T)` / `GroupResult(K, T)`; `[]const u8` keys special-cased to `std.StringHashMap` for `distinct()`/`groupBy()` |
| `examples/hello_mdix.zig` | ✅ Done — idiomatic-layer sibling to `hello.zig` |
| `mdix/tests/` — dedicated external-API behavioral suite | ✅ Done — 7 files mirroring `mdix-odin/mdix/tests/`'s structure, `zig build test-behavioral` |
| CI (`zig-ci.yml`, `parse_zig_test_results.py`, `zig-test-template.html`) | ✅ Done — mirrors `odin-ci.yml`'s structure, captures all three suites separately |

**Compile status: unverified beyond CI run #1's `build.zig.zon`
fingerprint error.** No Zig toolchain is available in the environment
this was authored in (see `mdix-c` execution-constraints notes) — every
file is hand-written against Zig 0.16's documented APIs, cross-checked
against real 0.16 code and real DixScript `.mdix` fixtures in this repo
where possible, but the bulk of it hasn't been through an actual
`zig build test` yet. Run it locally / let the next CI run tell you what
doesn't compile as-is.

---

## Directory layout

```
mdix-zig/
├── build.zig
├── build.zig.zon
├── mdix_ffi/
│   └── mdix_ffi.zig       # raw extern bindings (module "mdix_ffi")
├── mdix/                  # idiomatic wrapper (module "mdix")
│   ├── mdix.zig            # Database, Builder, source-text utilities; re-exports every sibling below
│   ├── watch.zig            # HotReload
│   ├── types.zig            # HexColor/Blob/MdixRegex/MdixDate/MdixTimestamp
│   ├── merge.zig             # mergeSources / mergeSourcesWeighted
│   ├── schema.zig             # SchemaBuilder / ValidationReport
│   ├── query.zig               # Query(T) / GroupResult(K, T) / queryLoad / queryMany
│   └── tests/                   # external-API behavioral suite (module "mdix" as any consumer would import it)
│       ├── tests.zig              # aggregator — root of the test-behavioral binary
│       ├── database_test.zig
│       ├── builder_test.zig
│       ├── merge_test.zig
│       ├── query_test.zig
│       ├── schema_test.zig
│       ├── types_test.zig
│       └── watch_test.zig
└── examples/
    ├── hello.zig           # raw-layer demo
    └── hello_mdix.zig      # idiomatic-layer demo
```

Mirrors `mdix-odin`'s two-package split — `mdix_ffi` (raw, hand-maintained
against `mdix-ffi/src/lib.rs`) and `mdix` (idiomatic, built on top) — as
two separate Zig modules rather than Odin packages. Within the `mdix`
module, sibling files (`watch.zig`/`types.zig`/`merge.zig`/`schema.zig`/
`query.zig`) reach shared internals via `@import("mdix.zig")` and get
re-exported at `mdix.zig`'s top level — the explicit-per-file equivalent
of Odin's automatic flat per-directory package namespace. `mdix/tests/`
is its own thing: a separate test binary that imports "mdix" as a module
(like any external consumer would), rather than reaching in via relative
file imports the way the sibling files above do.

---

## Build the native library

```bash
cargo build --release -p mdix-ffi
# Linux:   target/release/libmdix_ffi.so
# macOS:   target/release/libmdix_ffi.dylib
# Windows: target/release/mdix_ffi.dll + mdix_ffi.lib
```

## Linking from Zig

`build.zig` links every module/executable it defines against the system
`mdix_ffi` library via `linkSystemLibrary`. Point it at the directory
holding the library you just built with `-Dmdix-lib-path`:

```bash
zig build                          -Dmdix-lib-path=/path/to/DixScript-Rust/target/release
zig build test                      -Dmdix-lib-path=/path/to/target/release  # all three suites
zig build test-ffi                   -Dmdix-lib-path=/path/to/target/release  # mdix_ffi only
zig build test-mdix                   -Dmdix-lib-path=/path/to/target/release  # mdix inline tests only
zig build test-behavioral              -Dmdix-lib-path=/path/to/target/release  # mdix/tests/ external-API suite only
zig build run-hello                     -Dmdix-lib-path=/path/to/target/release  # raw-layer example
zig build run-hello-mdix                 -Dmdix-lib-path=/path/to/target/release  # idiomatic-layer example
```

On Linux/macOS this also sets an rpath on the produced binary, so it
finds the library at run time without an `LD_LIBRARY_PATH`/
`DYLD_LIBRARY_PATH` export. On Windows, `mdix_ffi.dll` still needs to be
on `PATH` or next to the executable at run time — import libraries don't
carry rpath-equivalent metadata; see `mdix-c/README.md`'s platform notes,
which this mirrors.

Omit `-Dmdix-lib-path` if `libmdix_ffi` is already installed to a
standard system library path.

## Using this package as a dependency

Once this package is registered the normal way (`zig fetch --save` /
hand-written `build.zig.zon` dependency entry — pending a publishing
pass, same "code-complete, not yet published" status the Go/Java/Lua/PHP/
Odin wrappers are in per the root README), consumers add:

```zig
const mdix_dep = b.dependency("mdix_zig", .{ .target = target, .optimize = optimize });
exe.root_module.addImport("mdix", mdix_dep.module("mdix"));           // idiomatic layer
exe.root_module.addImport("mdix_ffi", mdix_dep.module("mdix_ffi"));   // raw layer, if needed directly
```

For now, within this repo, `@import("mdix")` / `@import("mdix_ffi")`
after the module imports shown in `build.zig` are enough — see
`examples/hello_mdix.zig` / `examples/hello.zig`.

## Zig usage (idiomatic layer — start here)

```zig
const std = @import("std");
const mdix = @import("mdix");

pub fn main() !void {
    const allocator = std.heap.page_allocator;

    var db = try mdix.Database.loadStr(allocator,
        \\@DATA( port = 8080, host = "localhost" )
    );
    defer db.deinit();

    const host = try db.getString(allocator, "host");
    defer allocator.free(host);
    const port = try db.getInt("port");

    std.debug.print("{s}:{d}\n", .{ host, port });
}
```

Every fallible call returns a Zig error union (`!T`) instead of the C
API's null-sentinel + `mdix_get_last_error()` pattern — call
`mdix.lastError()` for the human-readable reason on any error. See
`mdix/mdix.zig`'s file-level doc comment for the full allocator
convention (short path/key arguments use an internal stack buffer, no
allocator needed to call in; arbitrary-length text and every owned
return value take an explicit `std.mem.Allocator`).

Beyond the core `Database`/`Builder` shown above:
- **Hot reload** — `mdix.HotReload` (`mdix/watch.zig`), deliberately not
  a background thread, same reasoning as `mdix-odin/mdix/watch.odin`.
- **Typed convenience getters** — `mdix.getHexColor`/`getBlob`/
  `getRegex`/`getDate`/`getTimestamp`/`getEnumValue` (`mdix/types.zig`)
  for values whose canonical mdix-ffi representation is a plain string.
- **Merging multiple sources** — `mdix.mergeSources`/
  `mergeSourcesWeighted` (`mdix/merge.zig`) for AST-level (not JSON
  round-trip) merges with a per-key conflict report.
- **Schema validation** — `mdix.SchemaBuilder` (`mdix/schema.zig`) for
  checking a loaded `Database` against a declared shape before you start
  reading from it.
- **Querying decoded arrays** — `mdix.queryLoad`/`queryMany` +
  `mdix.Query(T)` (`mdix/query.zig`) for filter/sort/group/aggregate over
  a JSON-decoded array — see that file's doc comment for the ownership
  model (`std.json.Parsed([]T)`, not a `Query(T)`, is what you `deinit()`).

## Zig usage (raw layer)

Still available directly if you want it — same ownership discipline as
the C API throughout (every `mdix_get_*`/`mdix_to_*`/`mdix_format_*`
string return is caller-owned, free with `mdix_free_string`; every
handle is freed with `mdix_free` / `mdix_builder_free` /
`mdix_watcher_free`):

```zig
const std = @import("std");
const mdix_ffi = @import("mdix_ffi");

pub fn main() !void {
    const db = mdix_ffi.mdix_load_str("@DATA( port = 8080, host = \"localhost\" )");
    if (db == null) {
        std.debug.print("{s}\n", .{std.mem.span(mdix_ffi.mdix_get_last_error() orelse "?")});
        return;
    }
    defer mdix_ffi.mdix_free(db);

    const host = mdix_ffi.mdix_get_string(db, "host");
    defer if (host) |h| mdix_ffi.mdix_free_string(h);
    const port = mdix_ffi.mdix_get_int(db, "port");

    std.debug.print("{s}:{d}\n", .{ if (host) |h| std.mem.span(h) else "?", port });
}
```

## Running tests

```bash
zig build test -Dmdix-lib-path=/path/to/target/release
```

Three suites, matching `build.zig`'s three `addTest` targets (`zig build
test` runs all of them; `test-ffi` / `test-mdix` / `test-behavioral` run
one at a time — see "Linking from Zig" above):

- **`mdix_ffi`** (`mdix_ffi/mdix_ffi.zig`) — 3 link-level smoke tests.
- **`mdix`** (`mdix/mdix.zig` + everything it re-exports, pulled in
  transitively) — inline `test` blocks scattered through `mdix.zig`,
  `watch.zig`, `types.zig`, `merge.zig`, `schema.zig`, `query.zig`;
  exercises internals from inside their own file.
- **`mdix_tests`** (`mdix/tests/`) — the dedicated external-API suite;
  every test goes through `@import("mdix")`, the same surface an actual
  consumer gets, complementing rather than duplicating the inline tests.

CI (`.github/workflows/zig-ci.yml`) runs all three suites separately,
parses each raw `zig test`-style output with
`scripts/parse_zig_test_results.py` (a standalone script rather than the
inline `python3 -&nbsp;<<&nbsp;PYEOF` block `odin-ci.yml`/`go-ci.yml`
use, since this covers multiple build stages), and publishes a report to
`gh-pages` under `/zig/` using `.github/zig-test-template.html` — same
pipeline shape as every other wrapper's CI, see `odin-ci.yml` for the
closest reference.

## Compiler requirements

Targets **Zig 0.16** (current stable as of this writing) — uses the
`root_module`-based `addExecutable`/`addTest` API and the
`build.zig.zon` `.fingerprint`/enum-literal `.name` manifest schema, both
specific to 0.14+. `minimum_zig_version` in `build.zig.zon` is set
accordingly; bump it if you backport to an older release.

## Platform notes

Same artifact, same notes as `mdix-c/README.md`: **Linux** needs
`libmdix_ffi.so` on the library search path or an rpath (handled for you
via `-Dmdix-lib-path`, see above); **macOS** ditto for `libmdix_ffi.dylib`;
**Windows** needs `mdix_ffi.dll` on `PATH` or next to the executable at
run time.
