# mdix-c — DixScript C/C++ Package

Pre-built native library + headers for loading `.mdix` files from C and C++.

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
