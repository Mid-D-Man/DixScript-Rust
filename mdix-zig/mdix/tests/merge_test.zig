const std = @import("std");
const mdix = @import("mdix");

test "mergeSourcesWeighted: higher weight wins under .weighted_priority" {
    const allocator = std.testing.allocator;
    // Weights are clamped to [0.0, 1.0] on the Rust side
    // (MdixMergeInput::with_weight) — 0.1/0.9 stays comfortably inside
    // that range and unambiguously orders the two sources. An earlier
    // draft used 1.0/10.0: 10.0 silently clamps down to 1.0, tying with
    // the first source's 1.0, and a tie falls back to the primary
    // (lower-indexed) source — which looked like "weights don't work"
    // but was really "this weight never got applied in the first place."
    var result = try mdix.mergeSourcesWeighted(
        allocator,
        &.{ "@DATA( env = \"dev\" )", "@DATA( env = \"prod\" )" },
        &.{ 0.1, 0.9 }, // second source outweighs the first
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
    // array_strategy only applies to a GroupArray entry — DixScript's
    // `path:: item, item` syntax (confirmed against the parser: a
    // GroupArray is only ever produced by the `::` token). A plain
    // bracket-literal array *value* (`tags = ["a", "b"]`) is a regular
    // property whose conflicts resolve winner-takes-all under the
    // merge strategy, same as any other scalar — array_strategy never
    // enters into it. An earlier draft used bracket literals here and
    // got 2 (source 0's whole array replacing source 1's, exactly the
    // winner-takes-all behavior a plain property gets) instead of the
    // 3 a real GroupArray concat would produce.
    var result = try mdix.mergeSources(
        allocator,
        &.{
            "@DATA( tags:: \"a\", \"b\" )",
            "@DATA( tags:: \"c\" )",
        },
        .primary_wins,
        .concat,
    );
    defer result.db.deinit();
    defer mdix.freeMergeConflicts(allocator, result.conflicts);

    try std.testing.expectEqual(@as(i32, 3), result.db.arrayLength("tags"));
}
