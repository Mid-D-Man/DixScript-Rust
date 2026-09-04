const std = @import("std");
const mdix = @import("mdix");

test "mdix.HotReload is reachable from the public module surface" {
    const allocator = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();

    try tmp.dir.writeFile(.{ .sub_path = "config.mdix", .data = "@DATA( port = 8080 )" });
    const path = try tmp.dir.realpathAlloc(allocator, "config.mdix");
    defer allocator.free(path);

    var hr = try mdix.HotReload.init(allocator, path);
    defer hr.deinit();

    var db = try mdix.Database.load(path);
    defer db.deinit();

    try std.testing.expectEqual(@as(i32, 8080), try db.getInt("port"));
    // No change since init — a second check right away should report
    // false (already covered in depth by watch.zig's own inline tests;
    // this just confirms the re-export wires up end to end).
    try std.testing.expect(!hr.check(&db));
}
