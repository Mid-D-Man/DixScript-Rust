# mdix-c — DixScript C/C++ Package

Pre-built native library + headers for loading `.mdix` files from C and C++.

Full language reference, `.mdix` syntax, and the DLM/schema/query semantics
this package surfaces: **https://dixscript-docs.pages.dev**

## Download

Download `mdix-c-package-build-N.zip` from the [Actions artifacts](https://github.com/Mid-D-Man/DixScript-Rust/actions)
or the [Releases page](https://github.com/Mid-D-Man/DixScript-Rust/releases).

Unzip next to your project. The layout expected by `CMakeLists.txt`:
```
mdix-c/
  include/
    mdix.h
    mdix.hpp
  lib/
    linux-x64/   libmdix_ffi.so
    windows-x64/ mdix_ffi.dll  mdix_ffi.lib
    macos/       libmdix_ffi.dylib
  CMakeLists.txt
```

## CMake integration
```cmake
add_subdirectory(mdix-c)        # or wherever you placed the package

target_link_libraries(my_app PRIVATE mdix::mdix)
```

## C usage
```c
#include <mdix.h>
#include <stdio.h>

int main(void) {
    void* db = mdix_load_str("@DATA( port = 8080, host = \"localhost\" )");
    if (!db) { fprintf(stderr, "%s\n", mdix_get_last_error()); return 1; }

    char* host = mdix_get_string(db, "host");
    int   port = mdix_get_int   (db, "port");
    printf("%s:%d\n", host, port);   /* localhost:8080 */

    mdix_free_string(host);
    mdix_free(db);
    return 0;
}
```

## C++ usage
```cpp
#include <mdix.hpp>
#include <iostream>

int main() {
    auto db = mdix::Database::load_str(
        "@DATA( port = 8080, host = \"localhost\", ssl = true )");
    if (!db) { std::cerr << db.error().message() << '\n'; return 1; }

    std::cout << db->get_string("host").value_or("?")
              << ':'
              << db->get_int("port").value_or(0)
              << '\n';                              // localhost:8080

    // Builder
    auto result = mdix::Builder{}
        .set_string("app",  "MyGame")
        .set_int   ("port", 9000)
        .set_bool  ("ssl",  true)
        .to_database();

    if (result) std::cout << result->to_json().value_or("{}") << '\n';
}
```

## Merge — weighted AST-level merge of multiple sources
```cpp
auto result = mdix::merge_sources_weighted(
    {base_config_src, override_config_src},
    {1.0, 0.5},
    MDIX_MERGE_WEIGHTED_PRIORITY,
    MDIX_ARRAY_MERGE_CONCAT_DEDUP);

if (result) {
    if (result->has_conflicts())
        for (auto& c : result->conflicts) std::cout << c.path << " -> source[" << c.winning_source << "]\n";
    int port = result->database.get_int("server.port").value_or(0);
}
```
Real AST-level merge (`dixscript::Runtime::MdixMerger`) — weighted-priority
conflict resolution, per-source conflict reporting, and full type fidelity
for every DixScript value type, not a shallow JSON-object merge. In C:
`mdix_merge_sources()` / `mdix_merge_sources_weighted()`, both writing a
JSON conflict report to an `out_conflicts_json` out-parameter.

## Query — sibling-path glob matching
```cpp
auto matched = db->query_many("levels.*.enemies");   // JSON array of every match
```
For a single fixed array/value, `get_json(path)` already covers it —
`query_many` is specifically for gathering values across sibling paths that
share structure via a whole-segment `*` wildcard. In C: `mdix_select_many_as_json()`.

## Validate — syntax check without loading
```cpp
if (!mdix::validate(source)) { /* not valid DixScript */ }
```
Parses `source` and reports only whether it's syntactically valid — not
schema validation against expected fields/types, just "does it parse". In
C: `mdix_validate()`.

## Hot reload — poll-based file watching
```cpp
mdix::Watcher watcher("config.mdix");
while (running) {
    if (auto fresh = watcher.check_and_reload()) {
        apply_new_config(*fresh);
    }
    tick();
}
```
Poll-based, not OS-event-based — a single `stat()` call per check, cheap
enough to run every frame and consistent across every platform. The first
check always reports a change. Use `force_reload()` to reload
unconditionally, or `has_changed()` to check without reloading.
**Encrypted `.mdix` files are not supported by hot reload** — this is a
limitation of the core Runtime feature itself. In C:
`mdix_watcher_new()` / `_free()` / `_path()` / `_has_loaded()` /
`_has_changed()` / `_check_and_reload()` / `_force_reload()`.

## Builder round-trip editing
```cpp
auto db = mdix::Database::load_str(existing_source);
auto builder = mdix::Builder::from_handle(*db);   // pre-populated with db's root values
builder->set_int("port", 9090);
builder->save("config.mdix");
```
`mdix_builder_from_handle()` in C — starts a builder pre-populated from an
already-loaded file's root-level values, for load-modify-save instead of
rebuilding a file from scratch. Synthetic indexed children (`tags[0]`,
`server.host`, ...) are already stripped; only aggregate/root values that
map back to valid `.mdix` identifiers carry over.

## Other additions

`is_compressed()` / `is_encrypted()` (DLM flags), `get_loaded_version()`
(version string recorded in the loaded data itself), `get_all_keys()`
(every key in the flattened data set, recursive — vs. `get_keys(prefix)`'s
direct children only), `get_config_value(key)` (reads `@CONFIG` section
values), `compact_source()` / `strip_comments()` (source-text transforms
alongside the existing `format_source()` / `minify_source()`).

## Running tests
```bash
cmake -B build -DMDIX_BUILD_TESTS=ON
cmake --build build
ctest --test-dir build --output-on-failure
```
Two self-contained binaries (no GoogleTest/Catch2 dependency, matching this
package's own header-only, dependency-minimal design): `test_mdix_c`
exercises the plain C API directly, `test_mdix_cpp` exercises the
`mdix.hpp` RAII wrapper — a bug in the header-only layer itself (RAII
lifetime, `Result<T>` plumbing, the conflict-JSON scanner behind
`merge_sources()`) wouldn't show up testing the C API alone.

## Compiler requirements

| Language | Minimum standard |
|----------|-----------------|
| C        | C99             |
| C++      | C++17           |

## Platform notes

**Windows (MSVC):** link against `mdix_ffi.lib` (import library); `mdix_ffi.dll`
must be on `PATH` or next to the executable at runtime.

**Linux:** set `LD_LIBRARY_PATH` or install `libmdix_ffi.so` to a system lib path,
or pass `-Wl,-rpath,'$$ORIGIN'` to embed the search path in the binary.

**macOS:** `@rpath` is set in the dylib. Pass `-Wl,-rpath,@loader_path` or
install to `/usr/local/lib`.
