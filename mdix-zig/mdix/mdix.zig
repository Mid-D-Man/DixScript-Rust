//! mdix.zig — idiomatic Zig wrapper over mdix_ffi (DixScript runtime).
//! Mirrors mdix-odin/mdix/mdix.odin's surface, translated from Odin's
//! (value, ok) convention into Zig error unions (`!T`).
//!
//! Quick start:
//!
//!   const mdix = @import("mdix");
//!
//!   var db = try mdix.Database.loadStr(allocator,
//!       \\@DATA( port = 8080, host = "localhost" )
//!   );
//!   defer db.deinit();
//!
//!   const port = try db.getInt("port");
//!   const host = try db.getString(allocator, "host");
//!   defer allocator.free(host);
//!
//! On error, call `mdix.lastError()` for the human-readable reason —
//! same role as Odin's `mdix.last_error()`.
//!
//! Allocator rule of thumb (Zig has no implicit context.allocator, so
//! this is spelled out where Odin's doc comment could stay implicit):
//!   - Short path/key/glob-pattern arguments (a dotted lookup path, an
//!     @CONFIG key, a select_many_as_json pattern) are converted with an
//!     internal stack buffer — no allocator needed to call in. Every
//!     real one of these in this project is a short identifier; an
//!     input that doesn't fit is a caller bug, not a runtime condition
//!     to recover from, so the conversion asserts rather than erroring.
//!   - Arbitrary-length text arguments (DixScript/JSON/TOML source, a
//!     Builder string *value*, a key-file's content, a password) take
//!     an explicit `allocator` and are heap-copied to a null-terminated
//!     buffer internally, freed before the function returns.
//!   - Every OWNED return value (a String, a []u8, a [][]u8 of keys)
//!     takes an explicit `allocator` and is the caller's to free —
//!     cloned out of the C buffer, which is freed immediately.

const std = @import("std");
const mdix_ffi = @import("mdix_ffi");

pub const MdixType = mdix_ffi.MdixType;
pub const MdixFormatMode = mdix_ffi.MdixFormatMode;

// ── Errors ──────────────────────────────────────────────────────────────

pub fn lastError() ?[]const u8 {
    const e = mdix_ffi.mdix_get_last_error();
    if (e == null) return null;
    return std.mem.span(e.?);
}

pub fn clearError() void {
    mdix_ffi.mdix_clear_error();
}

pub fn version() []const u8 {
    return std.mem.span(mdix_ffi.mdix_version().?);
}

/// `pub` so sibling files (watch.zig, and the forthcoming merge.zig /
/// query.zig / schema.zig / types.zig) can reuse this same path-class
/// conversion via `@import("mdix.zig")` instead of duplicating it — the
/// Zig-module equivalent of everything sharing one flat namespace the
/// way Odin's per-directory package system does automatically.
pub const PATH_BUF_LEN = 4096;

/// See the file-level doc comment's "Allocator rule of thumb" — short
/// identifiers only, asserts rather than erroring on an oversized input.
pub fn cPath(buf: *[PATH_BUF_LEN:0]u8, s: []const u8) [:0]const u8 {
    std.debug.assert(s.len < PATH_BUF_LEN);
    @memcpy(buf[0..s.len], s);
    buf[s.len] = 0;
    return buf[0..s.len :0];
}

// ── Database ────────────────────────────────────────────────────────────

pub const Database = struct {
    handle: ?*anyopaque = null,

    pub fn isValid(self: Database) bool {
        return self.handle != null and mdix_ffi.mdix_is_valid(self.handle);
    }

    pub fn load(path: []const u8) !Database {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const h = mdix_ffi.mdix_load(cPath(&buf, path));
        if (h == null) return error.MdixFailed;
        return .{ .handle = h };
    }

    pub fn loadStr(allocator: std.mem.Allocator, source: []const u8) !Database {
        const csrc = try allocator.dupeZ(u8, source);
        defer allocator.free(csrc);
        const h = mdix_ffi.mdix_load_str(csrc);
        if (h == null) return error.MdixFailed;
        return .{ .handle = h };
    }

    /// `key_path` null auto-detects the `.mdix.key` file next to `enc_path`.
    pub fn loadEncrypted(enc_path: []const u8, key_path: ?[]const u8) !Database {
        var enc_buf: [PATH_BUF_LEN:0]u8 = undefined;
        var key_buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cenc = cPath(&enc_buf, enc_path);
        const ckey: ?[:0]const u8 = if (key_path) |kp| cPath(&key_buf, kp) else null;
        const h = mdix_ffi.mdix_load_encrypted(cenc, ckey);
        if (h == null) return error.MdixFailed;
        return .{ .handle = h };
    }

    pub fn loadEncryptedPassword(allocator: std.mem.Allocator, enc_path: []const u8, password: []const u8) !Database {
        var enc_buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cenc = cPath(&enc_buf, enc_path);
        const cpw = try allocator.dupeZ(u8, password);
        defer allocator.free(cpw);
        const h = mdix_ffi.mdix_load_encrypted_password(cenc, cpw);
        if (h == null) return error.MdixFailed;
        return .{ .handle = h };
    }

    /// `password` null for key-file-only mode (no password layer).
    pub fn loadEncryptedBytes(
        allocator: std.mem.Allocator,
        encrypted_bytes: []const u8,
        key_file_content: []const u8,
        password: ?[]const u8,
    ) !Database {
        if (encrypted_bytes.len == 0) return error.MdixFailed;
        const ckey = try allocator.dupeZ(u8, key_file_content);
        defer allocator.free(ckey);
        const cpw: ?[:0]u8 = if (password) |pw| try allocator.dupeZ(u8, pw) else null;
        defer if (cpw) |p| allocator.free(p);
        const h = mdix_ffi.mdix_load_encrypted_bytes(
            encrypted_bytes.ptr,
            @intCast(encrypted_bytes.len),
            ckey,
            cpw,
        );
        if (h == null) return error.MdixFailed;
        return .{ .handle = h };
    }

    pub fn fromJson(allocator: std.mem.Allocator, source: []const u8) !Database {
        const csrc = try allocator.dupeZ(u8, source);
        defer allocator.free(csrc);
        const h = mdix_ffi.mdix_from_json(csrc);
        if (h == null) return error.MdixFailed;
        return .{ .handle = h };
    }

    pub fn fromToml(allocator: std.mem.Allocator, source: []const u8) !Database {
        const csrc = try allocator.dupeZ(u8, source);
        defer allocator.free(csrc);
        const h = mdix_ffi.mdix_from_toml(csrc);
        if (h == null) return error.MdixFailed;
        return .{ .handle = h };
    }

    pub fn deinit(self: *Database) void {
        if (self.handle) |h| {
            mdix_ffi.mdix_free(h);
            self.handle = null;
        }
    }

    pub fn entryCount(self: Database) i32 {
        return mdix_ffi.mdix_entry_count(self.handle);
    }

    pub fn isEncrypted(self: Database) bool {
        return mdix_ffi.mdix_is_encrypted(self.handle);
    }

    pub fn isCompressed(self: Database) bool {
        return mdix_ffi.mdix_is_compressed(self.handle);
    }

    pub fn loadedVersion(self: Database, allocator: std.mem.Allocator) ![]u8 {
        const cs = mdix_ffi.mdix_get_loaded_version(self.handle);
        if (cs == null) return error.MdixFailed;
        defer mdix_ffi.mdix_free_string(cs);
        return allocator.dupe(u8, std.mem.span(cs.?));
    }

    /// Reads a key from the loaded @CONFIG section.
    pub fn configValue(self: Database, allocator: std.mem.Allocator, key: []const u8) ![]u8 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cs = mdix_ffi.mdix_get_config_value(self.handle, cPath(&buf, key));
        if (cs == null) return error.MdixFailed;
        defer mdix_ffi.mdix_free_string(cs);
        return allocator.dupe(u8, std.mem.span(cs.?));
    }

    // ── Type inspection ────────────────────────────────────────────────

    pub fn getType(self: Database, path: []const u8) MdixType {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_get_type(self.handle, cPath(&buf, path));
    }

    pub fn arrayLength(self: Database, path: []const u8) i32 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_get_array_length(self.handle, cPath(&buf, path));
    }

    pub fn exists(self: Database, path: []const u8) bool {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_exists(self.handle, cPath(&buf, path));
    }

    // ── Typed getters ──────────────────────────────────────────────────
    // 0 / "" / false on failure is ambiguous with a real zero value —
    // that's exactly what the `!` error union catches here.

    pub fn getString(self: Database, allocator: std.mem.Allocator, path: []const u8) ![]u8 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cs = mdix_ffi.mdix_get_string(self.handle, cPath(&buf, path));
        if (cs == null) return error.MdixFailed;
        defer mdix_ffi.mdix_free_string(cs);
        return allocator.dupe(u8, std.mem.span(cs.?));
    }

    pub fn getInt(self: Database, path: []const u8) !i32 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cpath = cPath(&buf, path);
        mdix_ffi.mdix_clear_error();
        const v = mdix_ffi.mdix_get_int(self.handle, cpath);
        if (mdix_ffi.mdix_get_last_error() != null) return error.MdixFailed;
        return v;
    }

    pub fn getLong(self: Database, path: []const u8) !i64 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cpath = cPath(&buf, path);
        mdix_ffi.mdix_clear_error();
        const v = mdix_ffi.mdix_get_long(self.handle, cpath);
        if (mdix_ffi.mdix_get_last_error() != null) return error.MdixFailed;
        return v;
    }

    pub fn getFloat(self: Database, path: []const u8) !f32 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cpath = cPath(&buf, path);
        mdix_ffi.mdix_clear_error();
        const v = mdix_ffi.mdix_get_float(self.handle, cpath);
        if (mdix_ffi.mdix_get_last_error() != null) return error.MdixFailed;
        return v;
    }

    pub fn getDouble(self: Database, path: []const u8) !f64 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cpath = cPath(&buf, path);
        mdix_ffi.mdix_clear_error();
        const v = mdix_ffi.mdix_get_double(self.handle, cpath);
        if (mdix_ffi.mdix_get_last_error() != null) return error.MdixFailed;
        return v;
    }

    pub fn getBool(self: Database, path: []const u8) !bool {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cpath = cPath(&buf, path);
        mdix_ffi.mdix_clear_error();
        const v = mdix_ffi.mdix_get_bool(self.handle, cpath);
        if (mdix_ffi.mdix_get_last_error() != null) return error.MdixFailed;
        return v;
    }

    pub fn getEnumName(self: Database, allocator: std.mem.Allocator, path: []const u8) ![]u8 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cs = mdix_ffi.mdix_get_enum_name(self.handle, cPath(&buf, path));
        if (cs == null) return error.MdixFailed;
        defer mdix_ffi.mdix_free_string(cs);
        return allocator.dupe(u8, std.mem.span(cs.?));
    }

    pub fn getEnumField(self: Database, allocator: std.mem.Allocator, path: []const u8) ![]u8 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cs = mdix_ffi.mdix_get_enum_field(self.handle, cPath(&buf, path));
        if (cs == null) return error.MdixFailed;
        defer mdix_ffi.mdix_free_string(cs);
        return allocator.dupe(u8, std.mem.span(cs.?));
    }

    /// Serializes the raw value at path to a JSON string — useful for
    /// Object/Array values you want to hand off wholesale.
    pub fn getJson(self: Database, allocator: std.mem.Allocator, path: []const u8) ![]u8 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cs = mdix_ffi.mdix_get_json(self.handle, cPath(&buf, path));
        if (cs == null) return error.MdixFailed;
        defer mdix_ffi.mdix_free_string(cs);
        return allocator.dupe(u8, std.mem.span(cs.?));
    }

    /// Whole-segment glob (e.g. "levels.*.enemies") gathered across every
    /// path matching the pattern — returned as a JSON array string.
    pub fn selectManyAsJson(self: Database, allocator: std.mem.Allocator, pattern: []const u8) ![]u8 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cs = mdix_ffi.mdix_select_many_as_json(self.handle, cPath(&buf, pattern));
        if (cs == null) return error.MdixFailed;
        defer mdix_ffi.mdix_free_string(cs);
        return allocator.dupe(u8, std.mem.span(cs.?));
    }

    // ── Keys ───────────────────────────────────────────────────────────

    /// Direct child keys under prefix ("" for top-level). Empty slice
    /// (not an error) if there are none.
    pub fn getKeys(self: Database, allocator: std.mem.Allocator, prefix: []const u8) ![][]u8 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        var count: i32 = 0;
        const arr = mdix_ffi.mdix_get_keys(self.handle, cPath(&buf, prefix), &count);
        return dupeStringArray(allocator, arr, count);
    }

    /// Every key in the flat data map, including synthetic indexed
    /// children (tags[0], server.host, ...). Empty slice (not an error)
    /// if there are none.
    pub fn getAllKeys(self: Database, allocator: std.mem.Allocator) ![][]u8 {
        var count: i32 = 0;
        const arr = mdix_ffi.mdix_get_all_keys(self.handle, &count);
        return dupeStringArray(allocator, arr, count);
    }

    // ── Export ─────────────────────────────────────────────────────────

    pub fn toJson(self: Database, allocator: std.mem.Allocator, indented: bool) ![]u8 {
        const cs = mdix_ffi.mdix_to_json(self.handle, indented);
        if (cs == null) return error.MdixFailed;
        defer mdix_ffi.mdix_free_string(cs);
        return allocator.dupe(u8, std.mem.span(cs.?));
    }

    pub fn toToml(self: Database, allocator: std.mem.Allocator) ![]u8 {
        const cs = mdix_ffi.mdix_to_toml(self.handle);
        if (cs == null) return error.MdixFailed;
        defer mdix_ffi.mdix_free_string(cs);
        return allocator.dupe(u8, std.mem.span(cs.?));
    }

    pub fn toMdix(self: Database, allocator: std.mem.Allocator, mode: MdixFormatMode) ![]u8 {
        const raw = mdix_ffi.mdix_to_mdix(self.handle, mode);
        if (raw == null) return error.MdixFailed;
        // See mdix_ffi.mdix_to_mdix's doc comment — char* cast as void*
        // on the C side, treated as a string here the same way.
        const cs: [*:0]u8 = @ptrCast(raw.?);
        defer mdix_ffi.mdix_free_string(cs);
        return allocator.dupe(u8, std.mem.span(cs));
    }
};

/// Shared by getKeys/getAllKeys — dupes a `mdix_free_string_array`-owned
/// char** into a caller-owned `[][]u8`, freeing the C array either way.
/// `arr == null` or `count == 0` is a valid empty result, not an error.
fn dupeStringArray(allocator: std.mem.Allocator, arr: ?[*][*:0]u8, count: i32) ![][]u8 {
    if (arr == null or count == 0) return &.{};
    defer mdix_ffi.mdix_free_string_array(arr, count);

    const n: usize = @intCast(count);
    const result = try allocator.alloc([]u8, n);
    var filled: usize = 0;
    errdefer {
        for (result[0..filled]) |s| allocator.free(s);
        allocator.free(result);
    }
    for (0..n) |i| {
        result[i] = try allocator.dupe(u8, std.mem.span(arr.?[i]));
        filled += 1;
    }
    return result;
}

/// Frees a `[][]u8` returned by getKeys/getAllKeys — each string, then
/// the slice itself.
pub fn freeKeys(allocator: std.mem.Allocator, keys: [][]u8) void {
    for (keys) |k| allocator.free(k);
    allocator.free(keys);
}

// ── Source text utilities (no Database required) ──────────────────────

pub fn formatSource(allocator: std.mem.Allocator, source: []const u8, mode: MdixFormatMode) ![]u8 {
    const csrc = try allocator.dupeZ(u8, source);
    defer allocator.free(csrc);
    const cs = mdix_ffi.mdix_format_source(csrc, mode);
    if (cs == null) return error.MdixFailed;
    defer mdix_ffi.mdix_free_string(cs);
    return allocator.dupe(u8, std.mem.span(cs.?));
}

pub fn minifySource(allocator: std.mem.Allocator, source: []const u8) ![]u8 {
    const csrc = try allocator.dupeZ(u8, source);
    defer allocator.free(csrc);
    const cs = mdix_ffi.mdix_minify_source(csrc);
    if (cs == null) return error.MdixFailed;
    defer mdix_ffi.mdix_free_string(cs);
    return allocator.dupe(u8, std.mem.span(cs.?));
}

/// Removes blank/redundant whitespace without touching comments or
/// overall structure — see minifySource for the more aggressive pass.
pub fn compactSource(allocator: std.mem.Allocator, source: []const u8) ![]u8 {
    const csrc = try allocator.dupeZ(u8, source);
    defer allocator.free(csrc);
    const cs = mdix_ffi.mdix_compact_source(csrc);
    if (cs == null) return error.MdixFailed;
    defer mdix_ffi.mdix_free_string(cs);
    return allocator.dupe(u8, std.mem.span(cs.?));
}

/// Strips line/block comments, formatting otherwise untouched.
pub fn stripComments(allocator: std.mem.Allocator, source: []const u8) ![]u8 {
    const csrc = try allocator.dupeZ(u8, source);
    defer allocator.free(csrc);
    const cs = mdix_ffi.mdix_strip_comments(csrc);
    if (cs == null) return error.MdixFailed;
    defer mdix_ffi.mdix_free_string(cs);
    return allocator.dupe(u8, std.mem.span(cs.?));
}

/// Parses source through the full compile pipeline WITHOUT constructing
/// a handle — reports only whether it's syntactically valid DixScript,
/// not schema validation. Check lastError() on false.
pub fn validate(allocator: std.mem.Allocator, source: []const u8) !bool {
    const csrc = try allocator.dupeZ(u8, source);
    defer allocator.free(csrc);
    return mdix_ffi.mdix_validate(csrc);
}

// ── Builder ─────────────────────────────────────────────────────────────

pub const Builder = struct {
    handle: ?*anyopaque = null,

    pub fn new() Builder {
        return .{ .handle = mdix_ffi.mdix_builder_new() };
    }

    /// Forks a builder pre-populated from db's root-level structural
    /// values (synthetic indexed children like tags[0] are stripped).
    /// db remains valid and independent. Odin's binding doesn't
    /// error-check this call at all (a nil db.handle silently produces
    /// a nil-handle Builder); this one does, since every other
    /// "construct from a possibly-failed handle" path in this file
    /// already does the same.
    pub fn fromDatabase(db: Database) !Builder {
        const h = mdix_ffi.mdix_builder_from_handle(db.handle);
        if (h == null) return error.MdixFailed;
        return .{ .handle = h };
    }

    pub fn deinit(self: *Builder) void {
        if (self.handle) |h| {
            mdix_ffi.mdix_builder_free(h);
            self.handle = null;
        }
    }

    pub fn entryCount(self: Builder) i32 {
        return mdix_ffi.mdix_builder_entry_count(self.handle);
    }

    pub fn clear(self: Builder) bool {
        return mdix_ffi.mdix_builder_clear(self.handle);
    }

    /// `value` goes through the heap (allocator-backed) path, not the
    /// path-class stack buffer — a string *value* being written isn't
    /// bounded the way a lookup path is.
    pub fn setString(self: Builder, allocator: std.mem.Allocator, path: []const u8, value: []const u8) !bool {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cpath = cPath(&buf, path);
        const cval = try allocator.dupeZ(u8, value);
        defer allocator.free(cval);
        return mdix_ffi.mdix_builder_set_string(self.handle, cpath, cval);
    }

    pub fn setInt(self: Builder, path: []const u8, value: i32) bool {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_set_int(self.handle, cPath(&buf, path), value);
    }

    pub fn setLong(self: Builder, path: []const u8, value: i64) bool {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_set_long(self.handle, cPath(&buf, path), value);
    }

    pub fn setFloat(self: Builder, path: []const u8, value: f32) bool {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_set_float(self.handle, cPath(&buf, path), value);
    }

    pub fn setDouble(self: Builder, path: []const u8, value: f64) bool {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_set_double(self.handle, cPath(&buf, path), value);
    }

    pub fn setBool(self: Builder, path: []const u8, value: bool) bool {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_set_bool(self.handle, cPath(&buf, path), value);
    }

    pub fn remove(self: Builder, path: []const u8) bool {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_remove(self.handle, cPath(&buf, path));
    }

    pub fn hasKey(self: Builder, path: []const u8) bool {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_has_key(self.handle, cPath(&buf, path));
    }

    pub fn getString(self: Builder, allocator: std.mem.Allocator, path: []const u8) ![]u8 {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        const cs = mdix_ffi.mdix_builder_get_string(self.handle, cPath(&buf, path));
        if (cs == null) return error.MdixFailed;
        defer mdix_ffi.mdix_free_string(cs);
        return allocator.dupe(u8, std.mem.span(cs.?));
    }

    pub fn getInt(self: Builder, path: []const u8) !i32 {
        if (!self.hasKey(path)) return error.MdixFailed;
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_get_int(self.handle, cPath(&buf, path));
    }

    pub fn getLong(self: Builder, path: []const u8) !i64 {
        if (!self.hasKey(path)) return error.MdixFailed;
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_get_long(self.handle, cPath(&buf, path));
    }

    pub fn getFloat(self: Builder, path: []const u8) !f32 {
        if (!self.hasKey(path)) return error.MdixFailed;
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_get_float(self.handle, cPath(&buf, path));
    }

    pub fn getDouble(self: Builder, path: []const u8) !f64 {
        if (!self.hasKey(path)) return error.MdixFailed;
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_get_double(self.handle, cPath(&buf, path));
    }

    pub fn getBool(self: Builder, path: []const u8) !bool {
        if (!self.hasKey(path)) return error.MdixFailed;
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_get_bool(self.handle, cPath(&buf, path));
    }

    pub fn toStringOwned(self: Builder, allocator: std.mem.Allocator) ![]u8 {
        const cs = mdix_ffi.mdix_builder_to_string(self.handle);
        if (cs == null) return error.MdixFailed;
        defer mdix_ffi.mdix_free_string(cs);
        return allocator.dupe(u8, std.mem.span(cs.?));
    }

    pub fn save(self: Builder, path: []const u8) bool {
        var buf: [PATH_BUF_LEN:0]u8 = undefined;
        return mdix_ffi.mdix_builder_save(self.handle, cPath(&buf, path));
    }

    /// Serializes the builder and immediately reloads it as a read-only
    /// Database — useful right after building runtime save data. Unlike
    /// Odin's version (which round-trips through a temp-allocator Odin
    /// string), this stays entirely on the C side of the boundary: no
    /// Zig-owned allocation needed for the intermediate string at all.
    pub fn toDatabase(self: Builder) !Database {
        const cs = mdix_ffi.mdix_builder_to_string(self.handle);
        if (cs == null) return error.MdixFailed;
        defer mdix_ffi.mdix_free_string(cs);
        const h = mdix_ffi.mdix_load_str(cs);
        if (h == null) return error.MdixFailed;
        return .{ .handle = h };
    }
};

// ── Hot reload ──────────────────────────────────────────────────────────
// Re-exported here so external callers see `mdix.HotReload` directly —
// same flat-namespace experience Odin's per-directory package gives
// `mdix.Hot_Reload` for free; Zig needs an explicit re-export per
// sibling file instead. watch.zig itself reaches back in via
// `@import("mdix.zig")` for Database/PATH_BUF_LEN/cPath.

pub const HotReload = @import("watch.zig").HotReload;

// ── Sanity tests ────────────────────────────────────────────────────────
// Link-level coverage only — see mdix/tests/ (forthcoming) for the real
// behavioral suite, matching mdix-odin/mdix/tests/.

test "Database.loadStr / getInt / getString / deinit round-trip" {
    const allocator = std.testing.allocator;
    var db = try Database.loadStr(allocator,
        \\@DATA( port = 8080, host = "localhost", ssl = true )
    );
    defer db.deinit();

    try std.testing.expect(db.isValid());
    try std.testing.expectEqual(@as(i32, 8080), try db.getInt("port"));
    try std.testing.expect(try db.getBool("ssl"));

    const host = try db.getString(allocator, "host");
    defer allocator.free(host);
    try std.testing.expectEqualStrings("localhost", host);
}

test "Database.getInt on a missing path fails" {
    const allocator = std.testing.allocator;
    var db = try Database.loadStr(allocator, "@DATA( port = 8080 )");
    defer db.deinit();

    try std.testing.expectError(error.MdixFailed, db.getInt("does.not.exist"));
}

test "Database.loadStr with invalid source fails" {
    const allocator = std.testing.allocator;
    try std.testing.expectError(error.MdixFailed, Database.loadStr(allocator, "not valid dixscript {{{"));
}

test "Builder set/get/toDatabase round-trip" {
    const allocator = std.testing.allocator;
    var b = Builder.new();
    defer b.deinit();

    try std.testing.expect(try b.setString(allocator, "app", "MyGame"));
    try std.testing.expect(b.setInt("port", 9000));
    try std.testing.expect(b.setBool("ssl", true));

    try std.testing.expectEqualStrings("MyGame", try b.getString(allocator, "app"));
    try std.testing.expectEqual(@as(i32, 9000), try b.getInt("port"));

    var db = try b.toDatabase();
    defer db.deinit();
    try std.testing.expectEqual(@as(i32, 9000), try db.getInt("port"));
}

test "Database.getKeys / getAllKeys / freeKeys" {
    const allocator = std.testing.allocator;
    var db = try Database.loadStr(allocator,
        \\@DATA( a = 1, b = 2, nested = @OBJECT( c = 3 ) )
    );
    defer db.deinit();

    const keys = try db.getKeys(allocator, "");
    defer freeKeys(allocator, keys);
    try std.testing.expect(keys.len > 0);
}
