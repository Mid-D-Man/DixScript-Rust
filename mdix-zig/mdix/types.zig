//! types.zig — typed convenience wrappers for values whose canonical
//! mdix-ffi representation is a plain string (mdix_get_string) or, for
//! Blob/Regex specifically, a JSON string (mdix_get_json) — see
//! getStringLikeViaJson's doc comment below for why those two split
//! from the rest. Mirrors mdix-odin/mdix/types.odin. None of these need
//! new FFI surface — the value is already retrievable as text at the
//! FFI layer, these just parse it into something more useful than a
//! bare []u8.
//!
//! Two deliberate gaps versus the Odin version, both because Zig's std
//! lib doesn't ship what Odin's core: collection does:
//!   - No civil-calendar/instant conversion (Odin's core:time). MdixDate
//!     / MdixTimestamp below store the parsed year/month/day/etc. fields
//!     directly rather than converting to a unified instant — correct
//!     and dependency-free, if less convenient for date arithmetic than
//!     Odin's time.Time. Bring your own calendar math (or a package) if
//!     you need that.
//!   - No regex engine (Odin's core:text/regex). MdixRegex below is a
//!     thin pattern wrapper with no compile() — bring your own regex
//!     library and compile `.pattern` with it.
//!
//! These take `db: Database` as a plain parameter rather than being
//! Database methods — Zig structs can't be extended with more methods
//! from a second file the way Odin's free-proc style (which this module
//! already uses even in the original) allows, so this is both the
//! natural Zig shape here and matches Odin's own convention for this
//! particular module.

const std = @import("std");
const root = @import("mdix.zig");

// ── HexColor ────────────────────────────────────────────────────────────

pub const HexColor = struct {
    raw: []const u8, // original hex string, e.g. "#FF5733" — owned, see getHexColor
    r: f32,
    g: f32,
    b: f32,
    a: f32, // channels, 0-1
};

fn hexNibble(c: u8) ?f32 {
    const v: u32 = switch (c) {
        '0'...'9' => c - '0',
        'a'...'f' => c - 'a' + 10,
        'A'...'F' => c - 'A' + 10,
        else => return null,
    };
    // Single nibble expands to a full byte the same way "#RGB" CSS
    // shorthand does: 0xF -> 0xFF, not 0x0F.
    return @as(f32, @floatFromInt(v * 16 + v)) / 255.0;
}

fn hexByte(s: []const u8, offset: usize) ?f32 {
    const hi = std.fmt.parseInt(u8, s[offset .. offset + 1], 16) catch return null;
    const lo = std.fmt.parseInt(u8, s[offset + 1 .. offset + 2], 16) catch return null;
    return @as(f32, @floatFromInt(@as(u32, hi) * 16 + lo)) / 255.0;
}

/// Parses #RGB, #RRGGBB, or #RRGGBBAA. `raw` is stored as-is in the
/// result (not cloned) — keep it alive as long as you use the result's
/// `.raw` field.
pub fn parseHexColor(raw: []const u8) ?HexColor {
    var s = raw;
    if (s.len > 0 and s[0] == '#') s = s[1..];

    switch (s.len) {
        3 => {
            const r = hexNibble(s[0]) orelse return null;
            const g = hexNibble(s[1]) orelse return null;
            const b = hexNibble(s[2]) orelse return null;
            return .{ .raw = raw, .r = r, .g = g, .b = b, .a = 1 };
        },
        6 => {
            const r = hexByte(s, 0) orelse return null;
            const g = hexByte(s, 2) orelse return null;
            const b = hexByte(s, 4) orelse return null;
            return .{ .raw = raw, .r = r, .g = g, .b = b, .a = 1 };
        },
        8 => {
            const r = hexByte(s, 0) orelse return null;
            const g = hexByte(s, 2) orelse return null;
            const b = hexByte(s, 4) orelse return null;
            const a = hexByte(s, 6) orelse return null;
            return .{ .raw = raw, .r = r, .g = g, .b = b, .a = a };
        },
        else => return null,
    }
}

/// `raw` is owned by the returned HexColor (cloned via `allocator`) —
/// free it (`allocator.free(color.raw)`) when done, same as any other
/// owned-string getter in this package.
pub fn getHexColor(db: root.Database, allocator: std.mem.Allocator, path: []const u8) !HexColor {
    const raw = try db.getString(allocator, path);
    errdefer allocator.free(raw);
    return parseHexColor(raw) orelse error.MdixFailed;
}

// ── Blob ─────────────────────────────────────────────────────────────────

pub const Blob = struct {
    raw_base64: []const u8, // owned, see getBlob

    /// Decodes the base64 content into raw bytes, allocated with
    /// `allocator` and owned by the caller.
    pub fn bytes(self: Blob, allocator: std.mem.Allocator) ![]u8 {
        const decoder = std.base64.standard.Decoder;
        const size = try decoder.calcSizeForSlice(self.raw_base64);
        const buf = try allocator.alloc(u8, size);
        errdefer allocator.free(buf);
        try decoder.decode(buf, self.raw_base64);
        return buf;
    }
};

/// Blob and Regex are the two DixScript types mdix_get_string cannot
/// read — checked directly against dixscript's `impl TryFrom<DixValue>
/// for String` (dixscript/src/Runtime/dix_data.rs), which only covers
/// String/Date/Timestamp/HexColor. mdix_get_json's serializer, on the
/// other hand, maps every one of String/Date/Timestamp/HexColor/Blob/
/// Regex to a plain JSON string (dixscript/src/Runtime/converter.rs) —
/// so fetching the JSON form and decoding that one JSON string back out
/// works for exactly the two types getString can't reach.
fn getStringLikeViaJson(db: root.Database, allocator: std.mem.Allocator, path: []const u8) ![]u8 {
    const raw_json = try db.getJson(allocator, path);
    defer allocator.free(raw_json);
    var parsed = std.json.parseFromSlice([]const u8, allocator, raw_json, .{}) catch return error.MdixFailed;
    defer parsed.deinit();
    return allocator.dupe(u8, parsed.value);
}

pub fn getBlob(db: root.Database, allocator: std.mem.Allocator, path: []const u8) !Blob {
    return .{ .raw_base64 = try getStringLikeViaJson(db, allocator, path) };
}

// ── Regex ────────────────────────────────────────────────────────────────

/// A pattern string, nothing more — see the file-level doc comment for
/// why there's no compile() here.
pub const MdixRegex = struct {
    pattern: []const u8, // owned, see getRegex
};

pub fn getRegex(db: root.Database, allocator: std.mem.Allocator, path: []const u8) !MdixRegex {
    return .{ .pattern = try getStringLikeViaJson(db, allocator, path) };
}

// ── Date ─────────────────────────────────────────────────────────────────

pub const MdixDate = struct {
    raw: []const u8, // owned, see getDate — the original "YYYY-MM-DD" string
    year: i32,
    month: u8, // 1-12
    day: u8, // 1-31
};

/// Parses a "YYYY-MM-DD" string. Manual field-by-field parsing rather
/// than a general date-format engine — DixScript's date format is fixed
/// and simple enough that this is more direct, same reasoning as the
/// Odin version's parse_mdix_date. `raw` is stored as-is in the result
/// (not cloned) — keep it alive as long as you use the result's `.raw`.
pub fn parseMdixDate(raw: []const u8) ?MdixDate {
    if (raw.len != 10 or raw[4] != '-' or raw[7] != '-') return null;
    const year = std.fmt.parseInt(i32, raw[0..4], 10) catch return null;
    const month = std.fmt.parseInt(u8, raw[5..7], 10) catch return null;
    const day = std.fmt.parseInt(u8, raw[8..10], 10) catch return null;
    if (month < 1 or month > 12 or day < 1 or day > 31) return null;
    return .{ .raw = raw, .year = year, .month = month, .day = day };
}

/// `raw` is owned by the returned MdixDate (cloned via `allocator`) —
/// free it when done.
pub fn getDate(db: root.Database, allocator: std.mem.Allocator, path: []const u8) !MdixDate {
    const raw = try db.getString(allocator, path);
    errdefer allocator.free(raw);
    return parseMdixDate(raw) orelse error.MdixFailed;
}

// ── Timestamp ────────────────────────────────────────────────────────────

pub const MdixTimestamp = struct {
    raw: []const u8, // owned, see getTimestamp
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    nanosecond: u32, // 0 - 999_999_999
};

/// Parses an ISO 8601 / RFC 3339 string: YYYY-MM-DDTHH:MM:SS[.fraction][Z].
/// Same manual-parsing rationale as parseMdixDate. A non-"Z" numeric UTC
/// offset (+HH:MM) is intentionally not supported — DixScript's own
/// serializer always emits "Z", so this only needs to round-trip what
/// this project itself produces (same note as the Odin version). `raw`
/// is stored as-is (not cloned).
pub fn parseMdixTimestamp(raw: []const u8) ?MdixTimestamp {
    if (raw.len < 19 or raw[4] != '-' or raw[7] != '-' or raw[10] != 'T' or
        raw[13] != ':' or raw[16] != ':') return null;

    const year = std.fmt.parseInt(i32, raw[0..4], 10) catch return null;
    const month = std.fmt.parseInt(u8, raw[5..7], 10) catch return null;
    const day = std.fmt.parseInt(u8, raw[8..10], 10) catch return null;
    const hour = std.fmt.parseInt(u8, raw[11..13], 10) catch return null;
    const minute = std.fmt.parseInt(u8, raw[14..16], 10) catch return null;
    const second = std.fmt.parseInt(u8, raw[17..19], 10) catch return null;
    if (month < 1 or month > 12 or day < 1 or day > 31 or hour > 23 or minute > 59 or second > 60) {
        return null;
    }

    var nanos: u32 = 0;
    const rest = raw[19..];
    if (rest.len > 0 and rest[0] == '.') {
        var frac_end: usize = 1;
        while (frac_end < rest.len and rest[frac_end] >= '0' and rest[frac_end] <= '9') {
            frac_end += 1;
        }
        const frac_str = rest[1..frac_end];
        if (frac_str.len > 0) {
            if (std.fmt.parseInt(u32, frac_str, 10)) |n| {
                // Pad/truncate to exactly 9 digits (nanosecond precision):
                // ".123" (millis) -> 123_000_000 ns,
                // ".123456789012" -> truncated to 9 digits.
                var padded = n;
                var digit_count = frac_str.len;
                while (digit_count < 9) : (digit_count += 1) padded *= 10;
                while (digit_count > 9) : (digit_count -= 1) padded /= 10;
                nanos = padded;
            } else |_| {}
        }
    }

    return .{
        .raw = raw,
        .year = year,
        .month = month,
        .day = day,
        .hour = hour,
        .minute = minute,
        .second = second,
        .nanosecond = nanos,
    };
}

/// `raw` is owned by the returned MdixTimestamp (cloned via
/// `allocator`) — free it when done.
pub fn getTimestamp(db: root.Database, allocator: std.mem.Allocator, path: []const u8) !MdixTimestamp {
    const raw = try db.getString(allocator, path);
    errdefer allocator.free(raw);
    return parseMdixTimestamp(raw) orelse error.MdixFailed;
}

// ── Enum ─────────────────────────────────────────────────────────────────

/// An enum path's resolved integer value — mdix_get_int already works on
/// Enum paths directly (see mdix_ffi.mdix_get_int's doc comment), so
/// this is just a clearer name for that case.
pub fn getEnumValue(db: root.Database, path: []const u8) !i32 {
    return db.getInt(path);
}

// ── Sanity tests ────────────────────────────────────────────────────────

test "parseHexColor RGB / RRGGBB / RRGGBBAA" {
    const three = parseHexColor("#F00").?;
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), three.r, 0.01);
    try std.testing.expectApproxEqAbs(@as(f32, 0.0), three.g, 0.01);
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), three.a, 0.01);

    const six = parseHexColor("#FF5733").?;
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), six.r, 0.01);
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), six.a, 0.01);

    try std.testing.expect(parseHexColor("#GGG") == null);
    try std.testing.expect(parseHexColor("#12") == null);
}

test "parseMdixDate valid and invalid" {
    const d = parseMdixDate("2026-09-04").?;
    try std.testing.expectEqual(@as(i32, 2026), d.year);
    try std.testing.expectEqual(@as(u8, 9), d.month);
    try std.testing.expectEqual(@as(u8, 4), d.day);

    try std.testing.expect(parseMdixDate("2026/09/04") == null);
    try std.testing.expect(parseMdixDate("not-a-date") == null);
}

test "parseMdixTimestamp with and without fraction" {
    const t1 = parseMdixTimestamp("2026-09-04T12:30:00Z").?;
    try std.testing.expectEqual(@as(u8, 12), t1.hour);
    try std.testing.expectEqual(@as(u32, 0), t1.nanosecond);

    const t2 = parseMdixTimestamp("2026-09-04T12:30:00.123Z").?;
    try std.testing.expectEqual(@as(u32, 123_000_000), t2.nanosecond);

    try std.testing.expect(parseMdixTimestamp("not-a-timestamp") == null);
}
