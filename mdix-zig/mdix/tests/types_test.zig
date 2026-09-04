const std = @import("std");
const mdix = @import("mdix");

test "getHexColor reads a HexColor-typed field" {
    const allocator = std.testing.allocator;
    // Real DixScript hex-literal syntax, confirmed against
    // mdix_files/tests/cli/04_all_types.mdix: `<hex>` annotation, bare
    // "#RRGGBB" value (no quotes).
    var db = try mdix.Database.loadStr(allocator, "@DATA( accent<hex> = #FF5733 )");
    defer db.deinit();

    const color = try mdix.getHexColor(db, allocator, "accent");
    defer allocator.free(color.raw);
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), color.a, 0.01);
}

test "getDate / getTimestamp round-trip through a real Database" {
    const allocator = std.testing.allocator;
    // Real syntax, confirmed against the same fixture: `<date>`/
    // `<timestamp>` annotation, bare unquoted value — NOT a `@DATE(...)`/
    // `@TIMESTAMP(...)` call form.
    var db = try mdix.Database.loadStr(allocator,
        \\@DATA( released<date> = 2026-09-04, event<timestamp> = 2026-09-04T12:30:00Z )
    );
    defer db.deinit();

    const date = try mdix.getDate(db, allocator, "released");
    defer allocator.free(date.raw);
    try std.testing.expectEqual(@as(i32, 2026), date.year);
    try std.testing.expectEqual(@as(u8, 9), date.month);

    const ts = try mdix.getTimestamp(db, allocator, "event");
    defer allocator.free(ts.raw);
    try std.testing.expectEqual(@as(u8, 12), ts.hour);
}

test "getBlob decodes base64 content" {
    const allocator = std.testing.allocator;
    // Real syntax: `b:("...")` — confirmed against
    // mdix_files/tests/cli/04_all_types.mdix's blob_val entry.
    // base64 of "hi" below.
    var db = try mdix.Database.loadStr(allocator, "@DATA( payload = b:(\"aGk=\") )");
    defer db.deinit();

    const blob = try mdix.getBlob(db, allocator, "payload");
    defer allocator.free(blob.raw_base64);

    const decoded = try blob.bytes(allocator);
    defer allocator.free(decoded);
    try std.testing.expectEqualStrings("hi", decoded);
}

test "getEnumValue reads an Enum path's resolved integer value" {
    const allocator = std.testing.allocator;
    // Real syntax, confirmed against mdix-cli/tests/fixtures/with_enums.mdix:
    // @ENUMS( Name { Member = N, ... } ) declaration block, `<enum>`
    // annotation required on the usage site.
    var db = try mdix.Database.loadStr(allocator,
        \\@ENUMS( Status { Idle = 0, Running = 1, Stopped = 2 } )
        \\@DATA( state<enum> = Status.Running )
    );
    defer db.deinit();

    const v = try mdix.getEnumValue(db, "state");
    try std.testing.expectEqual(@as(i32, 1), v);
}

test "getHexColor on a non-color string fails" {
    const allocator = std.testing.allocator;
    var db = try mdix.Database.loadStr(allocator, "@DATA( accent = \"not a color\" )");
    defer db.deinit();

    try std.testing.expectError(error.MdixFailed, mdix.getHexColor(db, allocator, "accent"));
}

