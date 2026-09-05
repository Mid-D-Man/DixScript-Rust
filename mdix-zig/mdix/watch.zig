//! watch.zig — hot reload for a Database loaded from a file.
//! Mirrors mdix-odin/mdix/watch.odin. Re-exported as `mdix.HotReload`
//! from mdix.zig's top level.
//!
//! Deliberately NOT a background thread, unlike the Go binding's
//! EnableHotReload (which spins up a goroutine polling on a ticker).
//! This package's usual consumer already has its own per-frame update
//! loop (a game, an editor, a renderer) — spinning up a second thread
//! and a mutex to protect Database.handle from it would add real
//! complexity for something a one-line call already covers:
//!
//!   var hr = try mdix.HotReload.init(allocator, io, "config.mdix");
//!   defer hr.deinit();
//!
//!   while (running) {
//!       if (hr.check(io, &db)) {
//!           std.debug.print("config reloaded\n", .{});
//!       }
//!       // ... rest of frame
//!   }
//!
//! `io: std.Io` — Zig 0.16 moved filesystem operations behind an
//! explicit Io parameter (std.fs.Dir/File's old no-Io methods are gone).
//! Get one from your own `main`'s `std.process.Init` (`init.io`) in a
//! real program, or `std.testing.io` in a test — this package doesn't
//! manufacture one internally, so you stay in control of the execution
//! model, matching Zig 0.16's own design intent for Io.
//!
//! check() does the mtime stat + conditional reload + in-place handle
//! swap every call — call it as often as you'd poll anything else in
//! your loop (once a frame is fine; it's a stat() call, not a full
//! reparse, unless the file actually changed).
//!
//! Note this does NOT use mdix_ffi's mdix_watcher_* functions (an
//! MdixWatcher C handle) at all — same design choice mdix-odin made:
//! doing the mtime stat and reload directly with the host language's own
//! filesystem facilities is simpler than managing a second opaque
//! handle alongside Database's, for something this small.

const std = @import("std");
const mdix_ffi = @import("mdix_ffi");
const root = @import("mdix.zig");

pub const HotReload = struct {
    path: []const u8, // owned clone
    allocator: std.mem.Allocator,
    last_mtime: i128,
    valid: bool = false,

    /// Records path (cloned — the caller's slice doesn't need to
    /// outlive this call) and its current mtime. Returns an error if
    /// path doesn't exist / can't be stat'd; check() re-stats on every
    /// call anyway and will surface the same failure there for a file
    /// that goes missing later.
    pub fn init(allocator: std.mem.Allocator, io: std.Io, path: []const u8) !HotReload {
        const stat = try std.Io.Dir.cwd().statFile(io, path, .{});
        return .{
            .path = try allocator.dupe(u8, path),
            .allocator = allocator,
            .last_mtime = stat.mtime,
            .valid = true,
        };
    }

    pub fn deinit(self: *HotReload) void {
        if (self.valid) {
            self.allocator.free(self.path);
            self.valid = false;
        }
    }

    /// Stats the watched file; if its mtime advanced since the last
    /// check, reloads it and swaps db.handle in place (freeing the old
    /// one) so every other reference to the same Database sees fresh
    /// data with no re-fetch needed. Returns true only on an actual
    /// successful reload — false on "nothing changed" and on "changed
    /// but failed to reload" alike; check mdix.lastError() to tell those
    /// apart when it matters (e.g. the file was mid-write and failed to
    /// parse — db keeps serving its last-good data either way).
    ///
    /// A no-op returning false if `self` wasn't initialized (init
    /// failed, or deinit already ran) or db.handle is null.
    pub fn check(self: *HotReload, io: std.Io, db: *root.Database) bool {
        if (!self.valid or db.handle == null) return false;

        const stat = std.Io.Dir.cwd().statFile(io, self.path, .{}) catch {
            // File momentarily missing/unreadable (e.g. an editor
            // mid-rewrite deleted-then-recreated it) — try again next
            // check rather than treating this as a reload failure.
            return false;
        };
        if (stat.mtime <= self.last_mtime) return false;
        self.last_mtime = stat.mtime;

        var buf: [root.PATH_BUF_LEN:0]u8 = undefined;
        const new_handle = mdix_ffi.mdix_load(root.cPath(&buf, self.path));
        if (new_handle == null) return false;

        mdix_ffi.mdix_free(db.handle);
        db.handle = new_handle;
        return true;
    }
};

// ── Sanity tests ────────────────────────────────────────────────────────
// std.testing.io supplies the Io value here — see the file-level doc
// comment for where a real caller gets one instead.

test "HotReload.init on a missing file fails" {
    const allocator = std.testing.allocator;
    const io = std.testing.io;
    try std.testing.expectError(
        error.FileNotFound,
        HotReload.init(allocator, io, "/nonexistent/path/mdix-zig-test/should-not-exist.mdix"),
    );
}

test "HotReload.check reloads after the file's mtime advances" {
    const allocator = std.testing.allocator;
    const io = std.testing.io;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();

    try tmp.dir.writeFile(io, .{ .sub_path = "config.mdix", .data = "@DATA( port = 8080 )" });
    const path = try tmp.dir.realPathFileAlloc(io, "config.mdix", allocator);
    defer allocator.free(path);

    var hr = try HotReload.init(allocator, io, path);
    defer hr.deinit();

    var db = try root.Database.load(path);
    defer db.deinit();

    // No change yet.
    try std.testing.expect(!hr.check(io, &db));

    // Bump the mtime forward explicitly — a same-tick rewrite can
    // otherwise land on an mtime with insufficient resolution to read
    // as "changed" within a fast test run.
    std.Thread.sleep(10 * std.time.ns_per_ms);
    try tmp.dir.writeFile(io, .{ .sub_path = "config.mdix", .data = "@DATA( port = 9000 )" });

    try std.testing.expect(hr.check(io, &db));
    try std.testing.expectEqual(@as(i32, 9000), try db.getInt("port"));
}
