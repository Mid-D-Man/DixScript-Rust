# mdix-odin — DixScript Odin Bindings

Odin bindings for loading `.mdix` files, built on the same `mdix_ffi`
native library used by the C/C++ and Go wrappers.mdix-odin/
mdix_ffi/ raw foreign-import bindings (package mdix_ffi)
mdix/ idiomatic wrapper: Database, Builder, (value, ok) getters
examples/
hello.odin## Build the native library

cargo build --release -p mdix-ffi
# Linux:   target/release/libmdix_ffi.so
# macOS:   target/release/libmdix_ffi.dylib
# Windows: target/release/mdix_ffi.dll + mdix_ffi.lib

## Linking from Odin

`mdix_ffi/mdix_ffi.odin` uses `foreign import "system:mdix_ffi"`
(`"system:mdix_ffi.lib"` on Windows). Point the linker at the directory
containing the built library:

    odin build . -extra-linker-flags:"-L/path/to/lib -Wl,-rpath,/path/to/lib"
    odin build . -extra-linker-flags:"/LIBPATH:C:\path\to\lib"   (Windows/MSVC)

Or drop the library next to your executable, or install to a standard
system path — same platform notes as mdix-c/README.md, identical artifact.

## API shape

Every read returns `(value, ok)` instead of the C API's null-sentinel +
`mdix_get_last_error()` pattern. String-returning procs accept an optional
`allocator` (defaults to `context.allocator`); the returned string is
caller-owned — `delete()` it. Path/value arguments going *in* are
converted via `context.temp_allocator`; call `free_all(context.temp_allocator)`
yourself in long-running loops with no surrounding temp scope.

## License

MIT — see repository root LICENSE.
