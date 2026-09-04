const std = @import("std");
const mdix = @import("mdix");

const Item = struct { name: []const u8, tier: i32 };

fn tierOf(i: Item) i32 {
    return i.tier;
}
fn nameOf(i: Item) []const u8 {
    return i.name;
}

test "Query: skip/take share backing storage without copying" {
    var items = [_]Item{
        .{ .name = "a", .tier = 1 },
        .{ .name = "b", .tier = 2 },
        .{ .name = "c", .tier = 3 },
        .{ .name = "d", .tier = 4 },
    };
    const q = mdix.Query(Item).new(&items);

    const skipped = q.skip(1);
    try std.testing.expectEqual(@as(usize, 3), skipped.count());
    try std.testing.expectEqualStrings("b", skipped.first().?.name);

    const taken = skipped.take(2);
    try std.testing.expectEqual(@as(usize, 2), taken.count());
    try std.testing.expectEqualStrings("c", taken.last().?.name);
}

test "Query: select projects into a different type" {
    const allocator = std.testing.allocator;
    var items = [_]Item{
        .{ .name = "a", .tier = 1 },
        .{ .name = "b", .tier = 2 },
    };
    const q = mdix.Query(Item).new(&items);

    const tiers = try q.select(i32, allocator, tierOf);
    defer allocator.free(tiers);
    try std.testing.expectEqualSlices(i32, &.{ 1, 2 }, tiers);
}

test "Query: distinct on []const u8 items hashes by content" {
    const allocator = std.testing.allocator;
    var names = [_][]const u8{ "a", "b", "a", "c", "b" };
    const q = mdix.Query([]const u8).new(&names);

    const uniq = try q.distinct(allocator);
    defer uniq.deinit(allocator);
    try std.testing.expectEqual(@as(usize, 3), uniq.count());
}

test "Query: where -> orderByDesc chain" {
    const allocator = std.testing.allocator;
    var items = [_]Item{
        .{ .name = "a", .tier = 1 },
        .{ .name = "b", .tier = 5 },
        .{ .name = "c", .tier = 3 },
        .{ .name = "d", .tier = 4 },
    };
    const q = mdix.Query(Item).new(&items);

    const highTier = struct {
        fn f(i: Item) bool {
            return i.tier >= 3;
        }
    }.f;

    const filtered = try q.where(allocator, highTier);
    defer filtered.deinit(allocator);

    const sorted = try filtered.orderByDesc(i32, allocator, tierOf);
    defer sorted.deinit(allocator);

    try std.testing.expectEqualStrings("b", sorted.items[0].name); // tier 5
    try std.testing.expectEqualStrings("c", sorted.items[2].name); // tier 3
    _ = nameOf;
}
