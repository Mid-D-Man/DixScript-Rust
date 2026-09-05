//! query.zig — querying decoded Zig data. The Zig counterpart to
//! dixscript::Runtime::query::DixQuery (dixscript/src/Runtime/query.rs).
//! Mirrors mdix-odin/mdix/query.odin.
//!
//! Deliberately doesn't bind DixQuery itself: every predicate/key/
//! selector it takes is a Rust closure, which can't cross the FFI
//! boundary. Same choice the Go, Python, C#, and Odin bindings already
//! made — fetch the array natively via getJson/selectManyAsJson, decode
//! it with std.json's reflection-based parsing, and query the resulting
//! Zig slice with Zig's own tools (comptime generics + function
//! pointers), not a transliteration of Rust's Iterator API.
//!
//!   const Enemy = struct { name: []const u8, hp: i32 };
//!
//!   var parsed = try mdix.queryLoad(Enemy, allocator, db, "enemies");
//!   defer parsed.deinit(); // owns every Enemy's .name content too
//!   const q = mdix.Query(Enemy).new(parsed.value);
//!
//!   const heavies = try q.where(allocator, struct {
//!       fn f(e: Enemy) bool { return e.hp > 500; }
//!   }.f);
//!   defer allocator.free(heavies.items); // shares parsed's string data — only the outer slice is heavies' own
//!
//!   // Sibling paths sharing shape, wildcarding one segment:
//!   var statuses = try mdix.queryMany([]const u8, allocator, db, "servers.*.status");
//!   defer statuses.deinit();
//!
//! std.json has no per-field JSON-key-rename mechanism the way Odin's
//! `json:"..."` struct tags do — your struct's field names must match
//! the JSON keys exactly (implement a custom jsonParse on T if they
//! don't).
//!
//! Ownership: queryLoad/queryMany return the standard
//! std.json.Parsed([]T) — NOT a Query(T) — because it's the arena
//! `parsed.deinit()` owns that actually needs freeing (it deep-frees
//! every nested string/slice field inside each T too, not just the
//! outer array). Wrap `.value` in Query(T).new(...) yourself once
//! parsed, as shown above. Every Query(T) method that allocates a new
//! result (where/select/orderBy/distinct/groupBy) allocates plain
//! memory via the `allocator` you pass it — free those with
//! `allocator.free(...)`/`Query(T).deinit(allocator)` as documented per
//! method; they share q's string *content* (still owned by `parsed`)
//! but own their own outer slice.

const std = @import("std");
const root = @import("mdix.zig");

pub fn Query(comptime T: type) type {
    return struct {
        items: []T,

        const Self = @This();

        pub fn new(items: []T) Self {
            return .{ .items = items };
        }

        /// Frees the slice backing self. Only call this on a Query whose
        /// `items` this package itself allocated (where/select-as-Query/
        /// orderBy/orderByDesc/distinct — see each method) — a Query
        /// wrapping `parsed.value` directly (queryLoad/queryMany) is
        /// freed via `parsed.deinit()` instead, and skip()/take()
        /// results share their source's backing array and must not be
        /// deinit()'d independently of it.
        pub fn deinit(self: Self, allocator: std.mem.Allocator) void {
            allocator.free(self.items);
        }

        pub fn count(self: Self) usize {
            return self.items.len;
        }
        pub fn isEmpty(self: Self) bool {
            return self.items.len == 0;
        }

        pub fn first(self: Self) ?T {
            if (self.items.len == 0) return null;
            return self.items[0];
        }
        pub fn firstOr(self: Self, fallback: T) T {
            if (self.items.len == 0) return fallback;
            return self.items[0];
        }
        pub fn last(self: Self) ?T {
            if (self.items.len == 0) return null;
            return self.items[self.items.len - 1];
        }
        pub fn nth(self: Self, index: usize) ?T {
            if (index >= self.items.len) return null;
            return self.items[index];
        }

        pub fn any(self: Self, predicate: *const fn (T) bool) bool {
            for (self.items) |item| {
                if (predicate(item)) return true;
            }
            return false;
        }
        pub fn all(self: Self, predicate: *const fn (T) bool) bool {
            for (self.items) |item| {
                if (!predicate(item)) return false;
            }
            return true;
        }

        // ── filtering / slicing ──────────────────────────────────────────
        // where() allocates a new backing slice — self is never mutated,
        // and the result owns its own outer array (deinit it
        // independently of self). skip()/take() share self's backing
        // array instead (a sub-slice, not a copy) — do not deinit() a
        // skip/take result independently of the Query it was sliced
        // from.

        pub fn where(self: Self, allocator: std.mem.Allocator, predicate: *const fn (T) bool) !Self {
            var out: std.ArrayListUnmanaged(T) = .empty;
            errdefer out.deinit(allocator);
            for (self.items) |item| {
                if (predicate(item)) try out.append(allocator, item);
            }
            return .{ .items = try out.toOwnedSlice(allocator) };
        }

        pub fn skip(self: Self, n: usize) Self {
            if (n >= self.items.len) return .{ .items = &.{} };
            return .{ .items = self.items[n..] };
        }

        pub fn take(self: Self, n: usize) Self {
            if (n >= self.items.len) return self;
            return .{ .items = self.items[0..n] };
        }

        // ── projection / ordering / grouping ────────────────────────────

        /// Projects every element of self through mapper into a freshly
        /// allocated slice.
        pub fn select(self: Self, comptime R: type, allocator: std.mem.Allocator, mapper: *const fn (T) R) ![]R {
            const out = try allocator.alloc(R, self.items.len);
            for (self.items, 0..) |item, i| out[i] = mapper(item);
            return out;
        }

        /// Stable-sorted ascending by key(item). Owns its own backing
        /// storage (a clone of self.items) — deinit it independently of
        /// self.
        pub fn orderBy(self: Self, comptime K: type, allocator: std.mem.Allocator, key: *const fn (T) K) !Self {
            const out = try allocator.dupe(T, self.items);
            const Ctx = struct {
                key_fn: *const fn (T) K,
                fn lessThan(ctx: @This(), a: T, b: T) bool {
                    return ctx.key_fn(a) < ctx.key_fn(b);
                }
            };
            std.mem.sort(T, out, Ctx{ .key_fn = key }, Ctx.lessThan);
            return .{ .items = out };
        }

        /// orderBy, descending.
        pub fn orderByDesc(self: Self, comptime K: type, allocator: std.mem.Allocator, key: *const fn (T) K) !Self {
            const out = try allocator.dupe(T, self.items);
            const Ctx = struct {
                key_fn: *const fn (T) K,
                fn greaterThan(ctx: @This(), a: T, b: T) bool {
                    return ctx.key_fn(a) > ctx.key_fn(b);
                }
            };
            std.mem.sort(T, out, Ctx{ .key_fn = key }, Ctx.greaterThan);
            return .{ .items = out };
        }

        /// Removes duplicate elements (by value equality), preserving
        /// first-seen order. `[]const u8` is special-cased to hash by
        /// content (std.StringHashMap) — every other T uses
        /// std.AutoHashMap, so T must be a type AutoHashMap accepts as a
        /// key (no slice/pointer fields inside a struct T, notably) — for
        /// a T that isn't, filter with where() and your own equality
        /// logic instead. (Odin's built-in map type hashes strings by
        /// content automatically, so query_distinct.odin didn't need
        /// this special case — std.AutoHashMap does not.)
        pub fn distinct(self: Self, allocator: std.mem.Allocator) !Self {
            const HashMapT = if (T == []const u8) std.StringHashMap(void) else std.AutoHashMap(T, void);
            var seen = HashMapT.init(allocator);
            defer seen.deinit();
            var out: std.ArrayListUnmanaged(T) = .empty;
            errdefer out.deinit(allocator);
            for (self.items) |item| {
                const res = try seen.getOrPut(item);
                if (!res.found_existing) try out.append(allocator, item);
            }
            return .{ .items = try out.toOwnedSlice(allocator) };
        }

        /// Groups elements by key(item), preserving first-seen key order
        /// and first-seen element order within each group. Returns a
        /// slice of groups rather than a map specifically to preserve
        /// that order. Caller owns the result — free with freeGroups
        /// below, or manually: each group's `.items`, then the returned
        /// slice itself.
        pub fn groupBy(self: Self, comptime K: type, allocator: std.mem.Allocator, key: *const fn (T) K) ![]GroupResult(K, T) {
            const HashMapT = if (K == []const u8) std.StringHashMap(usize) else std.AutoHashMap(K, usize);
            var index = HashMapT.init(allocator);
            defer index.deinit();

            var groups: std.ArrayListUnmanaged(std.ArrayListUnmanaged(T)) = .empty;
            var keys: std.ArrayListUnmanaged(K) = .empty;
            defer {
                for (groups.items) |*g| g.deinit(allocator);
                groups.deinit(allocator);
                keys.deinit(allocator);
            }

            for (self.items) |item| {
                const k = key(item);
                const res = try index.getOrPut(k);
                if (res.found_existing) {
                    try groups.items[res.value_ptr.*].append(allocator, item);
                } else {
                    res.value_ptr.* = groups.items.len;
                    try keys.append(allocator, k);
                    var new_group: std.ArrayListUnmanaged(T) = .empty;
                    try new_group.append(allocator, item);
                    try groups.append(allocator, new_group);
                }
            }

            const result = try allocator.alloc(GroupResult(K, T), groups.items.len);
            var filled: usize = 0;
            errdefer {
                for (result[0..filled]) |r| allocator.free(r.items);
                allocator.free(result);
            }
            for (groups.items, keys.items, 0..) |*g, k, i| {
                result[i] = .{ .key = k, .items = try g.toOwnedSlice(allocator) };
                filled += 1;
            }
            return result;
        }

        // ── aggregation ────────────────────────────────────────────────

        pub fn minByKey(self: Self, comptime K: type, key: *const fn (T) K) ?T {
            var best: ?T = null;
            var best_key: K = undefined;
            for (self.items) |item| {
                const k = key(item);
                if (best == null or k < best_key) {
                    best = item;
                    best_key = k;
                }
            }
            return best;
        }

        pub fn maxByKey(self: Self, comptime K: type, key: *const fn (T) K) ?T {
            var best: ?T = null;
            var best_key: K = undefined;
            for (self.items) |item| {
                const k = key(item);
                if (best == null or k >= best_key) {
                    best = item;
                    best_key = k;
                }
            }
            return best;
        }

        pub fn sumInt(self: Self, key: *const fn (T) i64) i64 {
            var sum: i64 = 0;
            for (self.items) |item| sum += key(item);
            return sum;
        }

        pub fn sumFloat(self: Self, key: *const fn (T) f64) f64 {
            var sum: f64 = 0;
            for (self.items) |item| sum += key(item);
            return sum;
        }

        pub fn avgFloat(self: Self, key: *const fn (T) f64) ?f64 {
            if (self.items.len == 0) return null;
            return self.sumFloat(key) / @as(f64, @floatFromInt(self.items.len));
        }
    };
}

/// One group produced by Query(T).groupBy: a key and the elements that
/// share it, in first-seen order.
pub fn GroupResult(comptime K: type, comptime T: type) type {
    return struct {
        key: K,
        items: []T, // owned
    };
}

/// Frees a []GroupResult(K, T) returned by Query(T).groupBy — each
/// group's `.items`, then the slice itself.
pub fn freeGroups(comptime K: type, comptime T: type, allocator: std.mem.Allocator, groups: []GroupResult(K, T)) void {
    for (groups) |g| allocator.free(g.items);
    allocator.free(groups);
}

/// Fetches the array at path via db.getJson and decodes it with
/// std.json's reflection-based parsing. Returns the standard
/// std.json.Parsed([]T) wrapper directly — see the file-level doc
/// comment's "Ownership" section for why, and for how to turn this into
/// a Query(T). Fails if path doesn't exist, isn't an array, or its
/// elements don't decode into T.
pub fn queryLoad(comptime T: type, allocator: std.mem.Allocator, db: root.Database, path: []const u8) !std.json.Parsed([]T) {
    const raw = try db.getJson(allocator, path);
    defer allocator.free(raw);
    return std.json.parseFromSlice([]T, allocator, raw, .{}) catch error.MdixFailed;
}

/// Decodes db.selectManyAsJson(pattern) into a []T (via the same
/// std.json.Parsed([]T) wrapper queryLoad returns — see its doc
/// comment). Every match must decode into T; for heterogeneous matches
/// call db.selectManyAsJson directly and decode by hand.
pub fn queryMany(comptime T: type, allocator: std.mem.Allocator, db: root.Database, pattern: []const u8) !std.json.Parsed([]T) {
    const raw = try db.selectManyAsJson(allocator, pattern);
    defer allocator.free(raw);
    return std.json.parseFromSlice([]T, allocator, raw, .{}) catch error.MdixFailed;
}

// ── Sanity tests ────────────────────────────────────────────────────────

const TestItem = struct { name: []const u8, hp: i32 };

fn isHeavy(e: TestItem) bool {
    return e.hp > 500;
}
fn hpOf(e: TestItem) i32 {
    return e.hp;
}
fn hpOfI64(e: TestItem) i64 {
    return e.hp;
}

test "Query.where / count / first / last" {
    const allocator = std.testing.allocator;
    var items = [_]TestItem{
        .{ .name = "goblin", .hp = 20 },
        .{ .name = "dragon", .hp = 900 },
        .{ .name = "troll", .hp = 600 },
    };
    const q = Query(TestItem).new(&items);

    try std.testing.expectEqual(@as(usize, 3), q.count());
    try std.testing.expectEqualStrings("goblin", q.first().?.name);
    try std.testing.expectEqualStrings("troll", q.last().?.name);

    const heavies = try q.where(allocator, isHeavy);
    defer heavies.deinit(allocator);
    try std.testing.expectEqual(@as(usize, 2), heavies.count());
}

test "Query.orderBy / minByKey / maxByKey / sumInt" {
    const allocator = std.testing.allocator;
    var items = [_]TestItem{
        .{ .name = "goblin", .hp = 20 },
        .{ .name = "dragon", .hp = 900 },
        .{ .name = "troll", .hp = 600 },
    };
    const q = Query(TestItem).new(&items);

    const sorted = try q.orderBy(i32, allocator, hpOf);
    defer sorted.deinit(allocator);
    try std.testing.expectEqualStrings("goblin", sorted.items[0].name);
    try std.testing.expectEqualStrings("dragon", sorted.items[2].name);

    try std.testing.expectEqualStrings("dragon", q.maxByKey(i32, hpOf).?.name);
    try std.testing.expectEqualStrings("goblin", q.minByKey(i32, hpOf).?.name);
    try std.testing.expectEqual(@as(i64, 1520), q.sumInt(hpOfI64));
}

test "Query.groupBy by []const u8 key" {
    const allocator = std.testing.allocator;
    const Item = struct { faction: []const u8, name: []const u8 };
    var items = [_]Item{
        .{ .faction = "orcs", .name = "grok" },
        .{ .faction = "elves", .name = "lira" },
        .{ .faction = "orcs", .name = "durga" },
    };
    const q = Query(Item).new(&items);

    const key_fn = struct {
        fn f(i: Item) []const u8 {
            return i.faction;
        }
    }.f;

    const groups = try q.groupBy([]const u8, allocator, key_fn);
    defer freeGroups([]const u8, Item, allocator, groups);

    try std.testing.expectEqual(@as(usize, 2), groups.len);
    try std.testing.expectEqualStrings("orcs", groups[0].key);
    try std.testing.expectEqual(@as(usize, 2), groups[0].items.len);
    try std.testing.expectEqualStrings("elves", groups[1].key);
}

test "queryLoad decodes an array at a path" {
    const allocator = std.testing.allocator;
    var db = try root.Database.loadStr(allocator,
        \\@DATA( enemies = [
        \\    { name = "goblin", hp = 20 },
        \\    { name = "dragon", hp = 900 }
        \\] )
    );
    defer db.deinit();

    var parsed = try queryLoad(TestItem, allocator, db, "enemies");
    defer parsed.deinit();

    const q = Query(TestItem).new(parsed.value);
    try std.testing.expectEqual(@as(usize, 2), q.count());
    try std.testing.expectEqualStrings("dragon", q.maxByKey(i32, hpOf).?.name);
}
