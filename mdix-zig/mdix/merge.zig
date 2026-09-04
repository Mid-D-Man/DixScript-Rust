//! merge.zig — merging multiple .mdix sources into one Database. Mirrors
//! mdix-odin/mdix/merge.odin.
//!
//! Wraps mdix_merge_sources / mdix_merge_sources_weighted — the real
//! AST-level DixScript merger, not a JSON round-trip, so every DixScript
//! type survives exactly (Long/Float/Double/HexColor/Blob/Regex/Date/
//! Timestamp/Enum), and conflicts are reported per key instead of
//! silently resolved.

const std = @import("std");
const mdix_ffi = @import("mdix_ffi");
const root = @import("mdix.zig");

pub const MergeStrategy = mdix_ffi.MdixMergeStrategy;
pub const ArrayMergeStrategy = mdix_ffi.MdixArrayMergeStrategy;

/// One path that more than one source defined, and which source won.
pub const MergeConflict = struct {
    path: []u8, // owned
    winning_source: i64,
    winning_label: []u8, // owned
};

pub const MergeResult = struct {
    db: root.Database,
    /// Empty (not an error) when there were no conflicts. Owned by the
    /// caller — free each entry's `.path`/`.winning_label` then the
    /// slice itself, or use freeMergeConflicts below.
    conflicts: []MergeConflict,
};

/// Frees a MergeResult.conflicts slice — each entry's owned strings,
/// then the slice itself.
pub fn freeMergeConflicts(allocator: std.mem.Allocator, conflicts: []MergeConflict) void {
    for (conflicts) |c| {
        allocator.free(c.path);
        allocator.free(c.winning_label);
    }
    allocator.free(conflicts);
}

/// Merges two or more .mdix source strings into a new Database. Sources
/// are weighted in descending order — sources[0] gets the highest
/// weight, the last source the lowest — which only matters under
/// .weighted_priority; use mergeSourcesWeighted for explicit weights.
///
/// Fails (error.MdixFailed) only if the merge itself failed (a source
/// failed to parse, or .throw_on_conflict hit a conflicting key) — check
/// mdix.lastError(). The caller must deinit() the returned Database on
/// success.
pub fn mergeSources(
    allocator: std.mem.Allocator,
    sources: []const []const u8,
    strategy: MergeStrategy,
    array_strategy: ArrayMergeStrategy,
) !MergeResult {
    if (sources.len == 0) return error.MdixFailed;

    const csources = try allocator.alloc(?[*:0]const u8, sources.len);
    defer allocator.free(csources);
    const owned = try allocator.alloc([:0]u8, sources.len);
    defer {
        for (owned) |s| allocator.free(s);
        allocator.free(owned);
    }
    for (sources, 0..) |s, i| {
        owned[i] = try allocator.dupeZ(u8, s);
        csources[i] = owned[i].ptr;
    }

    var out_conflicts: ?[*:0]u8 = null;
    const h = mdix_ffi.mdix_merge_sources(
        csources.ptr,
        @intCast(sources.len),
        strategy,
        array_strategy,
        &out_conflicts,
    );
    if (h == null) return error.MdixFailed;

    const conflicts = try parseMergeConflicts(allocator, out_conflicts);
    return .{ .db = .{ .handle = h }, .conflicts = conflicts };
}

/// mergeSources with explicit per-source weights. `weights` must be the
/// same length as `sources`; a higher weight wins under
/// .weighted_priority.
pub fn mergeSourcesWeighted(
    allocator: std.mem.Allocator,
    sources: []const []const u8,
    weights: []const f64,
    strategy: MergeStrategy,
    array_strategy: ArrayMergeStrategy,
) !MergeResult {
    if (sources.len == 0 or sources.len != weights.len) return error.MdixFailed;

    const csources = try allocator.alloc(?[*:0]const u8, sources.len);
    defer allocator.free(csources);
    const owned = try allocator.alloc([:0]u8, sources.len);
    defer {
        for (owned) |s| allocator.free(s);
        allocator.free(owned);
    }
    for (sources, 0..) |s, i| {
        owned[i] = try allocator.dupeZ(u8, s);
        csources[i] = owned[i].ptr;
    }

    var out_conflicts: ?[*:0]u8 = null;
    const h = mdix_ffi.mdix_merge_sources_weighted(
        csources.ptr,
        weights.ptr,
        @intCast(sources.len),
        strategy,
        array_strategy,
        &out_conflicts,
    );
    if (h == null) return error.MdixFailed;

    const conflicts = try parseMergeConflicts(allocator, out_conflicts);
    return .{ .db = .{ .handle = h }, .conflicts = conflicts };
}

fn parseMergeConflicts(allocator: std.mem.Allocator, raw: ?[*:0]u8) ![]MergeConflict {
    if (raw == null) return &.{};
    defer mdix_ffi.mdix_free_string(raw);

    // mdix_merge_sources reports "[]" (not null) when there were no
    // conflicts — std.json handles that fine as an empty array, but skip
    // the round trip for the common case.
    const raw_str = std.mem.span(raw.?);
    if (raw_str.len == 0 or std.mem.eql(u8, raw_str, "[]")) return &.{};

    var parsed = std.json.parseFromSlice(std.json.Value, allocator, raw_str, .{}) catch return &.{};
    defer parsed.deinit();

    const arr = switch (parsed.value) {
        .array => |a| a,
        else => return &.{},
    };
    if (arr.items.len == 0) return &.{};

    const result = try allocator.alloc(MergeConflict, arr.items.len);
    var filled: usize = 0;
    errdefer {
        for (result[0..filled]) |c| {
            allocator.free(c.path);
            allocator.free(c.winning_label);
        }
        allocator.free(result);
    }
    for (arr.items, 0..) |entry, i| {
        var path: []u8 = try allocator.dupe(u8, "");
        var winning_source: i64 = 0;
        var winning_label: []u8 = try allocator.dupe(u8, "");

        if (entry == .object) {
            const obj = entry.object;
            if (obj.get("path")) |v| switch (v) {
                .string => |s| {
                    allocator.free(path);
                    path = try allocator.dupe(u8, s);
                },
                else => {},
            };
            if (obj.get("winningSource")) |v| {
                winning_source = switch (v) {
                    .integer => |n| n,
                    .float => |f| @intFromFloat(f),
                    else => 0,
                };
            }
            if (obj.get("winningLabel")) |v| switch (v) {
                .string => |s| {
                    allocator.free(winning_label);
                    winning_label = try allocator.dupe(u8, s);
                },
                else => {},
            };
        }

        result[i] = .{ .path = path, .winning_source = winning_source, .winning_label = winning_label };
        filled += 1;
    }
    return result;
}

// ── Sanity tests ────────────────────────────────────────────────────────

test "mergeSources — primary_wins, no conflicts reported" {
    const allocator = std.testing.allocator;
    var result = try mergeSources(
        allocator,
        &.{ "@DATA( a = 1 )", "@DATA( b = 2 )" },
        .primary_wins,
        .replace,
    );
    defer result.db.deinit();
    defer freeMergeConflicts(allocator, result.conflicts);

    try std.testing.expectEqual(@as(i32, 1), try result.db.getInt("a"));
    try std.testing.expectEqual(@as(i32, 2), try result.db.getInt("b"));
    try std.testing.expectEqual(@as(usize, 0), result.conflicts.len);
}

test "mergeSources — throw_on_conflict fails on an actual conflict" {
    const allocator = std.testing.allocator;
    try std.testing.expectError(error.MdixFailed, mergeSources(
        allocator,
        &.{ "@DATA( a = 1 )", "@DATA( a = 2 )" },
        .throw_on_conflict,
        .replace,
    ));
}

test "mergeSources — empty sources list fails" {
    const allocator = std.testing.allocator;
    try std.testing.expectError(error.MdixFailed, mergeSources(
        allocator,
        &.{},
        .primary_wins,
        .replace,
    ));
}
