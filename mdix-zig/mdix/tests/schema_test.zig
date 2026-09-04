const std = @import("std");
const mdix = @import("mdix");

test "SchemaBuilder: multiple failures all reported, not just the first" {
    const allocator = std.testing.allocator;
    var db = try mdix.Database.loadStr(allocator, "@DATA( port = \"not-a-number\" )");
    defer db.deinit();

    var schema = mdix.SchemaBuilder.init(allocator);
    defer schema.deinit();
    try schema.requireString("app_name"); // missing
    try schema.requireInt("port"); // wrong type
    try schema.requireBool("debug"); // missing

    var report = try schema.validate(allocator, db);
    defer report.deinit(allocator);

    try std.testing.expect(!report.isValid());
    try std.testing.expectEqual(@as(usize, 3), report.errors.len);
}

test "SchemaBuilder: array and object type checks" {
    const allocator = std.testing.allocator;
    var db = try mdix.Database.loadStr(allocator,
        \\@DATA( tags = ["a", "b"], server = { host = "localhost" } )
    );
    defer db.deinit();

    var schema = mdix.SchemaBuilder.init(allocator);
    defer schema.deinit();
    try schema.requireArray("tags");
    try schema.requireObject("server");

    var report = try schema.validate(allocator, db);
    defer report.deinit(allocator);
    try std.testing.expect(report.isValid());
}

test "SchemaBuilder: validate() is safe against an invalid Database handle" {
    var schema = mdix.SchemaBuilder.init(std.testing.allocator);
    defer schema.deinit();
    try schema.requireString("anything");

    const invalid_db: mdix.Database = .{}; // handle == null
    var report = try schema.validate(std.testing.allocator, invalid_db);
    defer report.deinit(std.testing.allocator);

    try std.testing.expect(!report.isValid());
    try std.testing.expectEqual(mdix.ValidationErrorKind.missing, report.errors[0].kind);
}

test "validationErrorToString formats both error kinds" {
    const allocator = std.testing.allocator;
    const missing = mdix.ValidationError{
        .path = "app_name",
        .expected = .string,
        .actual = .unknown,
        .kind = .missing,
    };
    const msg = try mdix.validationErrorToString(allocator, missing);
    defer allocator.free(msg);
    try std.testing.expect(std.mem.indexOf(u8, msg, "app_name") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "missing") != null);
}
