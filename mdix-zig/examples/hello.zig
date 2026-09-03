//! examples/hello.zig — build & run: `zig build run-hello` from mdix-zig/
//! (needs libmdix_ffi discoverable — see ../README.md and the
//! `-Dmdix-lib-path=` build option).
//!
//! This talks to the raw `mdix_ffi` bindings directly, with manual
//! `mdix_free`/`mdix_free_string` calls — matching
//! mdix-c/tests/test_mdix_c.c's style, not the RAII-ish
//! mdix-odin/examples/hello.odin one. The idiomatic `mdix` package
//! (Database/Builder wrappers with Zig error-union/optional getters)
//! hasn't landed yet — see ../README.md's status table — once it does,
//! this example gets a `mdix`-based sibling the way mdix-odin's
//! `examples/hello.odin` uses its `mdix` package rather than raw
//! `mdix_ffi`.

const std = @import("std");
const mdix_ffi = @import("mdix_ffi");

pub fn main() !void {
    const handle = mdix_ffi.mdix_load_str(
        "@DATA( port = 8080, host = \"localhost\", ssl = true )",
    );
    if (handle == null) {
        std.debug.print("load failed: {s}\n", .{
            std.mem.span(mdix_ffi.mdix_get_last_error() orelse "unknown error"),
        });
        return;
    }
    defer mdix_ffi.mdix_free(handle);

    const host = mdix_ffi.mdix_get_string(handle, "host");
    defer if (host) |h| mdix_ffi.mdix_free_string(h);
    const port = mdix_ffi.mdix_get_int(handle, "port");
    const ssl = mdix_ffi.mdix_get_bool(handle, "ssl");

    std.debug.print("{s}:{d} (ssl={})\n", .{
        if (host) |h| std.mem.span(h) else "?",
        port,
        ssl,
    });

    // Builder round-trip.
    const builder = mdix_ffi.mdix_builder_new();
    if (builder == null) {
        std.debug.print("builder_new failed: {s}\n", .{
            std.mem.span(mdix_ffi.mdix_get_last_error() orelse "unknown error"),
        });
        return;
    }
    defer mdix_ffi.mdix_builder_free(builder);

    _ = mdix_ffi.mdix_builder_set_string(builder, "app", "MyGame");
    _ = mdix_ffi.mdix_builder_set_int(builder, "port", 9000);
    _ = mdix_ffi.mdix_builder_set_bool(builder, "ssl", true);

    if (mdix_ffi.mdix_builder_to_string(builder)) |out| {
        defer mdix_ffi.mdix_free_string(out);
        std.debug.print("{s}\n", .{std.mem.span(out)});
    }
}
