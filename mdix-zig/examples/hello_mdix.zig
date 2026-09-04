//! examples/hello_mdix.zig — the idiomatic-layer sibling to hello.zig,
//! same shape as mdix-odin/examples/hello.odin. Build & run:
//! `zig build run-hello-mdix` from mdix-zig/ (needs libmdix_ffi
//! discoverable — see ../README.md and the `-Dmdix-lib-path=` option).

const std = @import("std");
const mdix = @import("mdix");

pub fn main() !void {
    const allocator = std.heap.page_allocator;

    var db = try mdix.Database.loadStr(allocator,
        \\@DATA( port = 8080, host = "localhost", ssl = true )
    );
    defer db.deinit();

    const host = try db.getString(allocator, "host");
    defer allocator.free(host);
    const port = try db.getInt("port");
    const ssl = try db.getBool("ssl");

    std.debug.print("{s}:{d} (ssl={})\n", .{ host, port, ssl });

    var b = mdix.Builder.new();
    defer b.deinit();

    _ = try b.setString(allocator, "app", "MyGame");
    _ = b.setInt("port", 9000);
    _ = b.setBool("ssl", true);

    const out = try b.toStringOwned(allocator);
    defer allocator.free(out);
    std.debug.print("{s}\n", .{out});
}
