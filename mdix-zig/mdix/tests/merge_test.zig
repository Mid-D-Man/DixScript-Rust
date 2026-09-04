const std = @import("std");
const mdix = @import("mdix");

test "mergeSourcesWeighted: higher weight wins under .weighted_priority" {
    const allocator = std.testing.allocator;
    var result = try mdix.mergeSourcesWeighted(
        allocator,
        &.{ "@DATA( env = \"dev\" )", "@DATA( env = \"prod\" )" },
        &.{ 1.0, 10.0 }, // second source outweighs the first
        .weighted_priority,
        .replace,
    );
    defer result.db.deinit();
    defer mdix.freeMergeConflicts(allocator, result.conflicts);

    const env = try result.db.getString(allocator, "env");
    defer allocator.free(env);
    try std.testing.expectEqualStrings("prod", env);
    try std.testing.expectEqual(@as(usize, 1), result.conflicts.len);
}

test "mergeSourcesWeighted: mismatched sources/weights length fails" {
    const allocator = std.testing.allocator;
    try std.testing.expectError(error.MdixFailed, mdix.mergeSourcesWeighted(
        allocator,
        &.{ "@DATA( a = 1 )", "@DATA( b = 2 )" },
        &.{1.0}, // only one weight for two sources
        .weighted_priority,
        .replace,
    ));
}

test "mergeSources: array_strategy .concat combines rather than replaces" {
    const allocator = std.testing.allocator;
    var result = try mdix.mergeSources(
        allocator,
        &.{
            "@DATA( tags = [\"a\", \"b\"] )",
            "@DATA( tags = [\"c\"] )",
        },
        .primary_wins,
        .concat,
    );
    defer result.db.deinit();
    defer mdix.freeMergeConflicts(allocator, result.conflicts);

    try std.testing.expectEqual(@as(i32, 3), result.db.arrayLength("tags"));
}
