# mdix-zig — DixScript Zig Bindings

Zig bindings for loading `.mdix` files, built on the same `mdix_ffi`
native library used by the Rust, Python, C#, WASM/npm, Go, and Odin
wrappers.

## Documentation Site https://dixscript-docs.pages.dev

---

## Status

🚧 **In progress — raw FFI layer only, not yet code-complete.** This is
the newest wrapper in the ecosystem; it isn't in the root README's
publish table yet. Tracking against `mdix-odin` as the closest sibling
(same "no built-in FFI convenience layer" situation Go/Python/C#/WASM
don't have to deal with).

| Layer | Status |
|---|---|
| `mdix_ffi/mdix_ffi.zig` — raw `extern` bindings | ✅ Done — all 72 exported `mdix_*` symbols, verified 1:1 against `mdix-c/include/mdix.h` |
| `build.zig` / `build.zig.zon` | ✅ Done — targets Zig 0.16 |
| `examples/hello.zig` | ✅ Done — raw-layer only (manual `mdix_free`/`mdix_free_string`) |
| `mdix/` — idiomatic wrapper (`Database`, `Builder`, error-union getters) | ⏳ Not started |
| `mdix/watch.zig` — hot reload | ⏳ Not started |
| `mdix/merge.zig` — weighted AST merge | ⏳ Not started |
| `mdix/query.zig` — `Query(T)` over JSON-decoded structs | ⏳ Not started |
| `mdix/schema.zig` — client-side schema validation | ⏳ Not started |
| `mdix/tests/` | ⏳ Not started |

---

## Directory layout

```
mdix-zig/
├── build.zig
├── build.zig.zon
├── mdix_ffi/
│   └── mdix_ffi.zig      # raw extern bindings (module "mdix_ffi")
├── mdix/                 # idiomatic wrapper (module "mdix") — not yet started
│   └── ...
└── examples/
    └── hello.zig
```

Mirrors `mdix-odin`'s two-package split — `mdix_ffi` (raw, hand-maintained
against `mdix-ffi/src/lib.rs`) and `mdix` (idiomatic, built on top) — as
two separate Zig modules rather than Odin packages.

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
zig build -Dmdix-lib-path=/path/to/DixScript-Rust/target/release
zig build test        -Dmdix-lib-path=/path/to/target/release
zig build run-hello   -Dmdix-lib-path=/path/to/target/release
```

On Linux/macOS this also sets an rpath on the produced binary, so it
finds the library at run time without an `LD_LIBRARY_PATH`/
`DYLD_LIBRARY_PATH` export. On Windows, `mdix_ffi.dll` still needs to be
on `PATH` or next to the executable at run time — import libraries don't
carry rpath-equivalent metadata; see `mdix-c/README.md`'s platform notes,
which this mirrors.

Omit `-Dmdix-lib-path` if `libmdix_ffi` is already installed to a
standard system library path.

## Using `mdix_ffi` as a dependency

Once this package is registered the normal way (`zig fetch --save` /
hand-written `build.zig.zon` dependency entry — pending a publishing
pass, same "code-complete, not yet published" status the Go/Java/Lua/PHP/
Odin wrappers are in per the root README), consumers add:

```zig
const mdix_dep = b.dependency("mdix_zig", .{ .target = target, .optimize = optimize });
exe.root_module.addImport("mdix_ffi", mdix_dep.module("mdix_ffi"));
```

For now, within this repo, `@import("mdix_ffi")` after the module import
shown in `build.zig` is enough — see `examples/hello.zig`.

## Zig usage (raw layer)

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

This is the same ownership discipline as the C API — every
`mdix_get_*`/`mdix_to_*`/`mdix_format_*` string return is caller-owned
and must be freed with `mdix_free_string`; every handle is freed with
`mdix_free`, `mdix_builder_free`, or `mdix_watcher_free`. The forthcoming
`mdix` idiomatic layer wraps this in Zig error unions/optionals the way
`mdix-odin/mdix`'s `(value, ok)` pattern wraps the raw `mdix_ffi` package
— see the status table above.

## Running tests

```bash
zig build test -Dmdix-lib-path=/path/to/target/release
```

Currently three link-level smoke tests in `mdix_ffi/mdix_ffi.zig` itself
(version string, load/valid/free round-trip, null-source failure path).
Real behavioral coverage lands in `mdix/tests/` alongside the idiomatic
layer, matching `mdix-odin/mdix/tests/`.

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
