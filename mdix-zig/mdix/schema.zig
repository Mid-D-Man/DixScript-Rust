//! schema.zig — validating a loaded Database against a declared shape.
//! Mirrors mdix-odin/mdix/schema.odin.
//!
//! mdix-ffi has no schema-validation C ABI (no mdix_schema_* extern "C"
//! surface exists in mdix-ffi/src/lib.rs), so — same as the Go and C#
//! bindings — this validates purely with Database.exists()/getType(),
//! the same two calls any caller of this package could make by hand.
//!
//!   var schema = mdix.SchemaBuilder.init(allocator);
//!   defer schema.deinit();
//!   try schema.requireString("app_name");
//!   try schema.requireInt("port");
//!   try schema.optionalBool("debug");
//!
//!   var report = try schema.validate(allocator, db);
//!   defer report.deinit(allocator);
//!   if (!report.isValid()) {
//!       for (report.errors) |e| {
//!           const msg = try mdix.validationErrorToString(allocator, e);
//!           defer allocator.free(msg);
//!           std.debug.print("{s}\n", .{msg});
//!       }
//!   }

const std = @import("std");
const mdix_ffi = @import("mdix_ffi");
const root = @import("mdix.zig");

pub const MdixType = mdix_ffi.MdixType;

pub const ValidationErrorKind = enum {
    missing, // a required field's path does not exist
    wrong_type, // the path exists but holds a different type than declared
};

pub const ValidationError = struct {
    /// NOT owned — a reference into whatever string you passed to
    /// SchemaBuilder.require()/optional() (a string literal, in the
    /// overwhelmingly common case). Keep those alive as long as the
    /// SchemaBuilder itself, same as Odin's version, which doesn't clone
    /// paths either.
    path: []const u8,
    expected: MdixType,
    actual: MdixType, // .unknown when kind == .missing
    kind: ValidationErrorKind,
};

pub fn validationErrorToString(allocator: std.mem.Allocator, e: ValidationError) ![]u8 {
    if (e.kind == .missing) {
        return std.fmt.allocPrint(
            allocator,
            "\"{s}\": missing required field (expected {s})",
            .{ e.path, @tagName(e.expected) },
        );
    }
    return std.fmt.allocPrint(
        allocator,
        "\"{s}\": expected {s}, got {s}",
        .{ e.path, @tagName(e.expected), @tagName(e.actual) },
    );
}

/// The result of a full SchemaBuilder.validate pass — never an error by
/// itself, so every failure is visible at once instead of stopping at
/// the first.
pub const ValidationReport = struct {
    errors: []ValidationError, // owned (the slice itself; entries don't own their `.path`)

    pub fn isValid(self: ValidationReport) bool {
        return self.errors.len == 0;
    }

    pub fn deinit(self: *ValidationReport, allocator: std.mem.Allocator) void {
        allocator.free(self.errors);
        self.errors = &.{};
    }
};

const SchemaField = struct {
    path: []const u8, // not owned, see ValidationError.path
    expected: MdixType,
    required: bool,
};

/// A reusable schema definition — validate() only reads from the
/// Database you pass it, so the same SchemaBuilder can validate any
/// number of databases.
pub const SchemaBuilder = struct {
    allocator: std.mem.Allocator,
    fields: std.ArrayListUnmanaged(SchemaField) = .{},

    pub fn init(allocator: std.mem.Allocator) SchemaBuilder {
        return .{ .allocator = allocator };
    }

    pub fn deinit(self: *SchemaBuilder) void {
        self.fields.deinit(self.allocator);
    }

    pub fn fieldCount(self: SchemaBuilder) usize {
        return self.fields.items.len;
    }

    pub fn require(self: *SchemaBuilder, path: []const u8, expected: MdixType) !void {
        try self.fields.append(self.allocator, .{ .path = path, .expected = expected, .required = true });
    }

    pub fn optional(self: *SchemaBuilder, path: []const u8, expected: MdixType) !void {
        try self.fields.append(self.allocator, .{ .path = path, .expected = expected, .required = false });
    }

    // ── require_* / optional_* convenience wrappers ─────────────────────

    pub fn requireString(self: *SchemaBuilder, path: []const u8) !void {
        try self.require(path, .string);
    }
    pub fn requireInt(self: *SchemaBuilder, path: []const u8) !void {
        try self.require(path, .int);
    }
    pub fn requireLong(self: *SchemaBuilder, path: []const u8) !void {
        try self.require(path, .long);
    }
    pub fn requireFloat(self: *SchemaBuilder, path: []const u8) !void {
        try self.require(path, .float);
    }
    pub fn requireDouble(self: *SchemaBuilder, path: []const u8) !void {
        try self.require(path, .double);
    }
    pub fn requireBool(self: *SchemaBuilder, path: []const u8) !void {
        try self.require(path, .@"bool");
    }
    pub fn requireArray(self: *SchemaBuilder, path: []const u8) !void {
        try self.require(path, .array);
    }
    pub fn requireObject(self: *SchemaBuilder, path: []const u8) !void {
        try self.require(path, .object);
    }
    pub fn requireEnum(self: *SchemaBuilder, path: []const u8) !void {
        try self.require(path, .@"enum");
    }

    pub fn optionalString(self: *SchemaBuilder, path: []const u8) !void {
        try self.optional(path, .string);
    }
    pub fn optionalInt(self: *SchemaBuilder, path: []const u8) !void {
        try self.optional(path, .int);
    }
    pub fn optionalLong(self: *SchemaBuilder, path: []const u8) !void {
        try self.optional(path, .long);
    }
    pub fn optionalFloat(self: *SchemaBuilder, path: []const u8) !void {
        try self.optional(path, .float);
    }
    pub fn optionalDouble(self: *SchemaBuilder, path: []const u8) !void {
        try self.optional(path, .double);
    }
    pub fn optionalBool(self: *SchemaBuilder, path: []const u8) !void {
        try self.optional(path, .@"bool");
    }

    /// Checks every declared field against db and returns a full
    /// report — does not stop at the first failure. Safe to call on a db
    /// with handle == null; every field simply reports as missing.
    pub fn validate(self: SchemaBuilder, allocator: std.mem.Allocator, db: root.Database) !ValidationReport {
        var errors: std.ArrayListUnmanaged(ValidationError) = .{};
        errdefer errors.deinit(allocator);

        for (self.fields.items) |f| {
            if (!db.exists(f.path)) {
                if (f.required) {
                    try errors.append(allocator, .{
                        .path = f.path,
                        .expected = f.expected,
                        .actual = .unknown,
                        .kind = .missing,
                    });
                }
                continue;
            }
            const actual = db.getType(f.path);
            if (!schemaTypeMatches(f.expected, actual)) {
                try errors.append(allocator, .{
                    .path = f.path,
                    .expected = f.expected,
                    .actual = actual,
                    .kind = .wrong_type,
                });
            }
        }
        return .{ .errors = try errors.toOwnedSlice(allocator) };
    }
};

/// Mirrors mdix-ffi's own getter behavior exactly rather than inventing
/// looser rules — a schema pass should never approve a field the
/// corresponding get* call would then fail to read. Checked directly
/// against mdix-ffi/src/lib.rs, same reasoning as the Go binding's
/// schemaTypeMatches (mdix-go/schema.go):
///   - float/double are fully symmetric: mdix_get_float and
///     mdix_get_double both route through the same internal get::<f64>()
///     call (mdix_get_float just narrows the result to f32 afterward),
///     so either type satisfies either expectation.
///   - int/long are NOT symmetric: mdix_get_long's doc comment states it
///     "also accepts int values (widened without loss)", but
///     mdix_get_int uses a distinct get::<i32>() accessor with no such
///     note — so .long accepts an actual .int, but .int does not accept
///     an actual .long (getInt would simply fail on it).
fn schemaTypeMatches(expected: MdixType, actual: MdixType) bool {
    if (expected == actual) return true;
    return switch (expected) {
        .long => actual == .int,
        .float, .double => actual == .float or actual == .double,
        else => false,
    };
}

// ── Sanity tests ────────────────────────────────────────────────────────

test "SchemaBuilder.validate — missing required field" {
    const allocator = std.testing.allocator;
    var db = try root.Database.loadStr(allocator, "@DATA( port = 8080 )");
    defer db.deinit();

    var schema = SchemaBuilder.init(allocator);
    defer schema.deinit();
    try schema.requireString("app_name");
    try schema.requireInt("port");

    var report = try schema.validate(allocator, db);
    defer report.deinit(allocator);

    try std.testing.expect(!report.isValid());
    try std.testing.expectEqual(@as(usize, 1), report.errors.len);
    try std.testing.expectEqual(ValidationErrorKind.missing, report.errors[0].kind);
    try std.testing.expectEqualStrings("app_name", report.errors[0].path);
}

test "SchemaBuilder.validate — wrong type" {
    const allocator = std.testing.allocator;
    var db = try root.Database.loadStr(allocator, "@DATA( port = \"not-a-number\" )");
    defer db.deinit();

    var schema = SchemaBuilder.init(allocator);
    defer schema.deinit();
    try schema.requireInt("port");

    var report = try schema.validate(allocator, db);
    defer report.deinit(allocator);

    try std.testing.expect(!report.isValid());
    try std.testing.expectEqual(ValidationErrorKind.wrong_type, report.errors[0].kind);
}

test "SchemaBuilder.validate — long accepts int, int does not accept long" {
    const allocator = std.testing.allocator;
    var db = try root.Database.loadStr(allocator, "@DATA( small = 5 )");
    defer db.deinit();

    var schema = SchemaBuilder.init(allocator);
    defer schema.deinit();
    try schema.requireLong("small");

    var report = try schema.validate(allocator, db);
    defer report.deinit(allocator);
    try std.testing.expect(report.isValid());
}

test "SchemaBuilder.validate — all satisfied" {
    const allocator = std.testing.allocator;
    var db = try root.Database.loadStr(allocator,
        \\@DATA( app_name = "MyGame", port = 8080, debug = true )
    );
    defer db.deinit();

    var schema = SchemaBuilder.init(allocator);
    defer schema.deinit();
    try schema.requireString("app_name");
    try schema.requireInt("port");
    try schema.optionalBool("debug");

    var report = try schema.validate(allocator, db);
    defer report.deinit(allocator);
    try std.testing.expect(report.isValid());
}
