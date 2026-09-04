const std = @import("std");
const mdix = @import("mdix");

test "Builder: hasKey / remove" {
    var b = mdix.Builder.new();
    defer b.deinit();

    try std.testing.expect(b.setInt("port", 8080));
    try std.testing.expect(b.hasKey("port"));
    try std.testing.expect(!b.hasKey("nonexistent"));

    try std.testing.expect(b.remove("port"));
    try std.testing.expect(!b.hasKey("port"));
}

test "Builder: clear resets entryCount to zero" {
    var b = mdix.Builder.new();
    defer b.deinit();

    try std.testing.expect(b.setInt("a", 1));
    try std.testing.expect(b.setInt("b", 2));
    try std.testing.expect(b.entryCount() >= 2);

    try std.testing.expect(b.clear());
    try std.testing.expectEqual(@as(i32, 0), b.entryCount());
}

test "Builder.fromDatabase forks an independent copy" {
    const allocator = std.testing.allocator;
    var db = try mdix.Database.loadStr(allocator, "@DATA( port = 8080 )");
    defer db.deinit();

    var b = try mdix.Builder.fromDatabase(db);
    defer b.deinit();

    try std.testing.expect(b.hasKey("port"));
    try std.testing.expect(b.setInt("port", 9090));

    // db is untouched by the fork's mutation.
    try std.testing.expectEqual(@as(i32, 8080), try db.getInt("port"));
    try std.testing.expectEqual(@as(i32, 9090), try b.getInt("port"));
}

test "Builder: getInt on a key never set fails rather than returning 0" {
    var b = mdix.Builder.new();
    defer b.deinit();
    try std.testing.expectError(error.MdixFailed, b.getInt("never_set"));
}
