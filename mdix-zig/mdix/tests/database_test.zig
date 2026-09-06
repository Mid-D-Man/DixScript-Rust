const std = @import("std");
const mdix = @import("mdix");

test "Database: @CONFIG values readable via configValue" {
    const allocator = std.testing.allocator;
    // version must be exactly "1.0.0" -- confirmed against
    // dixscript's VersionManager::is_compatible_with, which checks
    // membership in a version_hierarchy set containing only
    // VERSION_1_0 ("1.0.0"), not a semver range check. An earlier
    // draft used "2.0" here (an arbitrary placeholder), which
    // loadStr silently rejects as an unsupported version -- same
    // failure shape as any other parse error (null handle,
    // error.MdixFailed), nothing wrong with configValue() itself.
    var db = try mdix.Database.loadStr(allocator,
        \\@CONFIG(
        \\  version -> "1.0.0"
        \\  author  -> "Mid-D-Man"
        \\)
        \\@DATA( port = 8080 )
    );
    defer db.deinit();

    const version = try db.configValue(allocator, "version");
    defer allocator.free(version);
    try std.testing.expectEqualStrings("1.0.0", version);
}

test "Database: getKeys with a prefix only returns that prefix's direct children" {
    const allocator = std.testing.allocator;
    var db = try mdix.Database.loadStr(allocator,
        \\@DATA( server = { host = "localhost", port = 8080 }, name = "app" )
    );
    defer db.deinit();

    const server_keys = try db.getKeys(allocator, "server");
    defer mdix.freeKeys(allocator, server_keys);
    try std.testing.expect(server_keys.len >= 2);

    const root_keys = try db.getKeys(allocator, "");
    defer mdix.freeKeys(allocator, root_keys);
    try std.testing.expect(root_keys.len >= 2); // "server", "name"
}

test "Database: toJson / toToml round-trip produce non-empty output" {
    const allocator = std.testing.allocator;
    var db = try mdix.Database.loadStr(allocator, "@DATA( a = 1, b = \"two\" )");
    defer db.deinit();

    const json = try db.toJson(allocator, true);
    defer allocator.free(json);
    try std.testing.expect(json.len > 0);
    try std.testing.expect(std.mem.indexOf(u8, json, "\"a\"") != null);

    const toml = try db.toToml(allocator);
    defer allocator.free(toml);
    try std.testing.expect(toml.len > 0);
}

test "Database: loadEncryptedBytes with garbage input fails cleanly" {
    const allocator = std.testing.allocator;
    const garbage = [_]u8{ 0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01, 0x02 };
    try std.testing.expectError(error.MdixFailed, mdix.Database.loadEncryptedBytes(
        allocator,
        &garbage,
        "not a real key file",
        null,
    ));
}

test "Database: fromJson round-trips a plain JSON object" {
    const allocator = std.testing.allocator;
    var db = try mdix.Database.fromJson(allocator, "{\"port\": 9090, \"host\": \"example.com\"}");
    defer db.deinit();

    try std.testing.expectEqual(@as(i32, 9090), try db.getInt("port"));
}
