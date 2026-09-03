//! mdix_ffi.zig — raw Zig bindings to the DixScript C FFI layer.
//!
//! This file mirrors mdix-ffi/src/lib.rs symbol-for-symbol — every
//! `#[no_mangle] pub extern "C" fn` in that crate has a matching `extern`
//! declaration here. Hand-maintained, not generated from
//! mdix-c/include/mdix.h; re-check against mdix-ffi/src/lib.rs after any
//! FFI surface change, the same way mdix.h and
//! mdix-odin/mdix_ffi/mdix_ffi.odin are hand-maintained relative to the
//! crate. Keep all three in sync.
//!
//! Link against the platform build of mdix_ffi (same artifact the C/C++,
//! Go, and Odin wrappers use):
//!   Linux:   libmdix_ffi.so
//!   macOS:   libmdix_ffi.dylib
//!   Windows: mdix_ffi.dll (import lib mdix_ffi.lib)
//! See ../build.zig (the `-Dmdix-lib-path=` option) and ../README.md.
//!
//! Ownership rules (identical to the C API):
//!   - Every `[*:0]u8` returned by an `mdix_get_*` / `mdix_to_*` /
//!     `mdix_format_*` / `mdix_builder_get_string` / `mdix_builder_to_string`
//!     call must be freed with `mdix_free_string`.
//!   - Every opaque handle (Database, Builder, or Watcher) must be freed
//!     with `mdix_free`, `mdix_builder_free`, or `mdix_watcher_free`
//!     (matching whichever created it).
//!   - Passing `null` for a handle or path is safe — the FFI layer
//!     returns a sentinel value and records a message retrievable via
//!     `mdix_get_last_error`.
//!   - Several functions return `false`/`null` for two different reasons:
//!     a legitimate negative result ("unchanged", "invalid") AND an
//!     actual error (bad handle, I/O failure, parse failure). Where that
//!     ambiguity exists it's called out on the function; call
//!     `mdix_get_last_error` to tell them apart — it returns `null` when
//!     there was no error.

const std = @import("std");

// ── Type discriminants ───────────────────────────────────────────────────

/// Mirrors the Rust `MdixType` repr(i32) enum exactly. Numeric types are
/// contiguous: int=2, long=3, float=4, double=5. Hand-maintained — does
/// NOT auto-correct when the Rust side changes; update this alongside
/// mdix.h and mdix_ffi.odin's `Mdix_Type`.
///
/// `null` and `bool` are Zig keywords, so those two fields are written
/// with `@""` escaping (`.@"null"`, `.@"bool"`) — the values themselves
/// are unchanged from the C/Rust side.
pub const MdixType = enum(c_int) {
    unknown = -1,
    @"null" = 0,
    @"bool" = 1,
    int = 2,
    long = 3,
    float = 4,
    double = 5,
    string = 6,
    date = 7,
    timestamp = 8,
    hex_color = 9,
    blob = 10,
    regex = 11,
    array = 12,
    object = 13,
    tuple = 14,
    @"enum" = 15,
};

/// Mirrors the Rust `MdixFormatMode` repr(i32) enum exactly.
pub const MdixFormatMode = enum(c_int) {
    default = 0,
    pretty = 1,
    compact = 2,
    minified = 3,
};

/// How to resolve a key defined by more than one source in
/// `mdix_merge_sources` / `mdix_merge_sources_weighted`. Mirrors the Rust
/// `MdixMergeStrategy` repr(i32) enum exactly (hand-maintained).
pub const MdixMergeStrategy = enum(c_int) {
    /// Each source's weight decides the winner; equal weights fall back
    /// to the lower-indexed (primary) source. What `mdix_merge_sources`
    /// (no explicit weights) effectively resolves to — it auto-assigns
    /// descending weights, source 0 gets 1.0, the last source gets ~0.0.
    weighted_priority = 0,
    /// The lower-indexed source always wins, regardless of weight.
    primary_wins = 1,
    /// The higher-indexed source always wins, regardless of weight.
    secondary_wins = 2,
    /// Any key defined by more than one source is a hard error — the
    /// merge fails outright (`mdix_merge_sources*` returns `null`).
    throw_on_conflict = 3,
};

/// How to combine two array-valued entries that share a path across merge
/// sources. Mirrors the Rust `ArrayMergeStrategy` repr(i32) enum exactly
/// (hand-maintained).
pub const MdixArrayMergeStrategy = enum(c_int) {
    /// The winning source's array entirely replaces the losing one's.
    replace = 0,
    /// Both arrays are concatenated, winner's items first.
    concat = 1,
    /// Concatenated (winner first), with exact-duplicate primitive
    /// values removed. Complex values (objects, nested arrays) are never
    /// deduped.
    concat_dedup = 2,
};

// ── Metadata ────────────────────────────────────────────────────────────

/// Static pointer — do NOT free.
pub extern fn mdix_version() callconv(.c) ?[*:0]const u8;

/// Runtime version string recorded in the loaded data itself (may differ
/// from `mdix_version` if the file was produced by a different
/// mdix-cli). Caller must free with `mdix_free_string`. `null` if handle
/// is `null`.
pub extern fn mdix_get_loaded_version(handle: ?*anyopaque) callconv(.c) ?[*:0]u8;

// ── Handle lifecycle — plain .mdix ─────────────────────────────────────

pub extern fn mdix_load(path: ?[*:0]const u8) callconv(.c) ?*anyopaque;
pub extern fn mdix_load_str(source: ?[*:0]const u8) callconv(.c) ?*anyopaque;
pub extern fn mdix_free(handle: ?*anyopaque) callconv(.c) void;

// ── Handle lifecycle — encrypted .mdix.enc ─────────────────────────────

/// `key_path` may be `null` to auto-detect next to the `.enc` file.
pub extern fn mdix_load_encrypted(
    enc_path: ?[*:0]const u8,
    key_path: ?[*:0]const u8,
) callconv(.c) ?*anyopaque;

pub extern fn mdix_load_encrypted_password(
    enc_path: ?[*:0]const u8,
    password: ?[*:0]const u8,
) callconv(.c) ?*anyopaque;

/// `password` may be `null` when using key-file mode.
pub extern fn mdix_load_encrypted_bytes(
    encrypted_bytes: ?[*]const u8,
    byte_count: i32,
    key_file_content: ?[*:0]const u8,
    password: ?[*:0]const u8,
) callconv(.c) ?*anyopaque;

// ── Validity and metadata ───────────────────────────────────────────────

pub extern fn mdix_is_valid(handle: ?*anyopaque) callconv(.c) bool;
pub extern fn mdix_entry_count(handle: ?*anyopaque) callconv(.c) i32;
/// DLM compression flag recorded when the source was loaded. `false`
/// (not an error) if handle is `null`.
pub extern fn mdix_is_compressed(handle: ?*anyopaque) callconv(.c) bool;
/// DLM encryption flag recorded when the source was loaded. `false` (not
/// an error) if handle is `null`.
pub extern fn mdix_is_encrypted(handle: ?*anyopaque) callconv(.c) bool;

// ── Type inspection ─────────────────────────────────────────────────────

pub extern fn mdix_get_type(handle: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) MdixType;
pub extern fn mdix_get_array_length(handle: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) i32;

// ── Typed getters ────────────────────────────────────────────────────────

/// Returned pointer must be freed with `mdix_free_string`. `null` on
/// failure.
pub extern fn mdix_get_string(handle: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) ?[*:0]u8;
pub extern fn mdix_get_int(handle: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) i32;
/// Get a 64-bit integer at path. Also accepts int values (widened
/// without loss).
pub extern fn mdix_get_long(handle: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) i64;
pub extern fn mdix_get_float(handle: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) f32;
pub extern fn mdix_get_double(handle: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) f64;
pub extern fn mdix_get_bool(handle: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) bool;
pub extern fn mdix_get_json(handle: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) ?[*:0]u8;
pub extern fn mdix_get_enum_name(handle: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) ?[*:0]u8;
pub extern fn mdix_get_enum_field(handle: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) ?[*:0]u8;

/// Reads a key from the loaded `@CONFIG` section (e.g. "version",
/// "author", "debug_mode" — all `@CONFIG` values are strings). Caller
/// must free with `mdix_free_string`. `null` if the key isn't set or
/// handle is `null`.
pub extern fn mdix_get_config_value(handle: ?*anyopaque, key: ?[*:0]const u8) callconv(.c) ?[*:0]u8;

// ── Key existence and enumeration ──────────────────────────────────────

pub extern fn mdix_exists(handle: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) bool;

/// Returns a heap-allocated array of null-terminated strings. `out_count`
/// receives the count. `prefix` may be `null` or `""` for top-level
/// keys. Free the result with `mdix_free_string_array(result, out_count.*)`.
pub extern fn mdix_get_keys(
    handle: ?*anyopaque,
    prefix: ?[*:0]const u8,
    out_count: *i32,
) callconv(.c) ?[*][*:0]u8;

/// Like `mdix_get_keys`, but every key in the entire flattened data set
/// (recursive — not just direct children of a prefix). `out_count`
/// receives the count. Free the result with
/// `mdix_free_string_array(result, out_count.*)`.
pub extern fn mdix_get_all_keys(handle: ?*anyopaque, out_count: *i32) callconv(.c) ?[*][*:0]u8;

// ── Query ────────────────────────────────────────────────────────────────

/// Sibling-path glob query (whole-segment `*` only, e.g.
/// "levels.*.enemies") — every value matching the pattern across paths
/// that share structure, gathered via
/// `dixscript::Runtime::DixData::select_many`. Returns a JSON array of
/// the matched values (caller must free with `mdix_free_string`). For a
/// single array/value at one fixed path, `mdix_get_json` already covers
/// it — this is specifically for the wildcarded, multi-path case.
pub extern fn mdix_select_many_as_json(handle: ?*anyopaque, pattern: ?[*:0]const u8) callconv(.c) ?[*:0]u8;

// ── Validation ──────────────────────────────────────────────────────────

/// Parses `source` and reports only whether it's syntactically valid
/// DixScript — this is NOT schema validation against expected
/// fields/types, just "does it parse". `false` on either a real parse
/// failure or a `null`/empty source — check `mdix_get_last_error` to
/// tell them apart.
pub extern fn mdix_validate(source: ?[*:0]const u8) callconv(.c) bool;

// ── Memory management ──────────────────────────────────────────────────

pub extern fn mdix_free_string(s: ?[*:0]u8) callconv(.c) void;
pub extern fn mdix_free_string_array(arr: ?[*][*:0]u8, count: i32) callconv(.c) void;

// ── Error reporting ─────────────────────────────────────────────────────

/// Valid until the next FFI call. Do NOT free. Returns `null` when there
/// was no error.
pub extern fn mdix_get_last_error() callconv(.c) ?[*:0]const u8;
pub extern fn mdix_clear_error() callconv(.c) void;

// ── Conversion — export ────────────────────────────────────────────────

pub extern fn mdix_to_json(handle: ?*anyopaque, indented: bool) callconv(.c) ?[*:0]u8;
pub extern fn mdix_to_toml(handle: ?*anyopaque) callconv(.c) ?[*:0]u8;
/// Returns a `char*` cast as `void*` on the C side — the pointee is a
/// null-terminated string; treat it as `[*:0]u8` and free with
/// `mdix_free_string` (cast through `@ptrCast`), same as every other
/// owned-string return in this file.
pub extern fn mdix_to_mdix(handle: ?*anyopaque, mode: MdixFormatMode) callconv(.c) ?*anyopaque;

// ── Conversion — source text formatting ────────────────────────────────

pub extern fn mdix_format_source(source: ?[*:0]const u8, mode: MdixFormatMode) callconv(.c) ?[*:0]u8;
pub extern fn mdix_minify_source(source: ?[*:0]const u8) callconv(.c) ?[*:0]u8;
/// Removes blank/redundant whitespace without touching comments or
/// overall structure — see `mdix_minify_source` for the more aggressive
/// pass.
pub extern fn mdix_compact_source(source: ?[*:0]const u8) callconv(.c) ?[*:0]u8;
/// Strips line and block comments, leaving formatting otherwise
/// untouched.
pub extern fn mdix_strip_comments(source: ?[*:0]const u8) callconv(.c) ?[*:0]u8;

// ── Conversion — foreign format import ─────────────────────────────────

/// Returned handle must be freed with `mdix_free`.
pub extern fn mdix_from_json(source: ?[*:0]const u8) callconv(.c) ?*anyopaque;
pub extern fn mdix_from_toml(source: ?[*:0]const u8) callconv(.c) ?*anyopaque;

// ── Merge — weighted AST-level merge of multiple sources ───────────────

/// Merges `count` DixScript source strings with auto-descending weights
/// (source 0 highest) and the given strategies. On success, returns a
/// new read handle (free with `mdix_free`) AND writes a JSON conflict
/// report to `out_conflicts_json.*` (caller must free with
/// `mdix_free_string`; an empty "[]" means no key was defined by more
/// than one source) — pass `null` for `out_conflicts_json` to skip the
/// report. Returns `null` on failure (a source that fails to parse, or
/// `.throw_on_conflict` hitting an actual conflict) — check
/// `mdix_get_last_error` for why.
pub extern fn mdix_merge_sources(
    sources: ?[*]const ?[*:0]const u8,
    count: i32,
    strategy: MdixMergeStrategy,
    array_strategy: MdixArrayMergeStrategy,
    out_conflicts_json: ?*?[*:0]u8,
) callconv(.c) ?*anyopaque;

/// As `mdix_merge_sources`, but with explicit per-source weights —
/// `weights` must have exactly `count` entries, one per source, in the
/// same order.
pub extern fn mdix_merge_sources_weighted(
    sources: ?[*]const ?[*:0]const u8,
    weights: ?[*]const f64,
    count: i32,
    strategy: MdixMergeStrategy,
    array_strategy: MdixArrayMergeStrategy,
    out_conflicts_json: ?*?[*:0]u8,
) callconv(.c) ?*anyopaque;

// ── Hot reload — poll-based file watching ──────────────────────────────

/// Watches a single plaintext `.mdix` path via
/// `dixscript::Runtime::HotReloadWatcher` — a cheap `stat()`-based poll,
/// not an OS filesystem-event subscription. Cheap enough to call
/// `mdix_watcher_check_and_reload` from a game loop / timer tick every
/// frame. Does not read the file yet — the first
/// `mdix_watcher_has_changed`/`mdix_watcher_check_and_reload` call
/// always reports a change. Returns an opaque handle, or `null` on
/// failure (`null`/invalid path). Caller must free with
/// `mdix_watcher_free`. Encrypted `.mdix` files are NOT supported here —
/// `HotReloadWatcher::force_reload` always reloads through the plaintext
/// loader path, a core Runtime limitation.
pub extern fn mdix_watcher_new(path: ?[*:0]const u8) callconv(.c) ?*anyopaque;

pub extern fn mdix_watcher_free(watcher: ?*anyopaque) callconv(.c) void;

/// Caller must free with `mdix_free_string`. `null` if watcher is
/// `null`.
pub extern fn mdix_watcher_path(watcher: ?*anyopaque) callconv(.c) ?[*:0]u8;

/// `true` once a successful reload has happened at least once.
pub extern fn mdix_watcher_has_loaded(watcher: ?*anyopaque) callconv(.c) bool;

/// Checks the file's modified-time without reloading. `false` means
/// either "unchanged" or "error" (bad handle, file missing) — check
/// `mdix_get_last_error` to tell them apart.
pub extern fn mdix_watcher_has_changed(watcher: ?*anyopaque) callconv(.c) bool;

/// Reloads only if the file changed since the last successful reload (or
/// since construction, on the first call). Returns a new read handle
/// (free with `mdix_free`) on a successful reload, or `null` when
/// unchanged OR on error — check `mdix_get_last_error` to tell them
/// apart. On a reload failure the watcher's internal modified-time stamp
/// is NOT updated, so the next call retries against the same file state
/// rather than silently giving up on that change.
pub extern fn mdix_watcher_check_and_reload(watcher: ?*anyopaque) callconv(.c) ?*anyopaque;

/// Reloads unconditionally. Returns a new read handle (free with
/// `mdix_free`), or `null` on failure.
pub extern fn mdix_watcher_force_reload(watcher: ?*anyopaque) callconv(.c) ?*anyopaque;

// ── Builder — lifecycle ────────────────────────────────────────────────

pub extern fn mdix_builder_new() callconv(.c) ?*anyopaque;
/// Creates a builder pre-populated with `handle`'s root-level values —
/// for round-trip editing of an already-loaded file (load → modify a few
/// keys → save), rather than rebuilding one from scratch. Synthetic
/// indexed children (`tags[0]`, `server.host`, ...) are already
/// stripped; only aggregate/root values that map back to valid `.mdix`
/// identifiers carry over. Returns `null` if handle is `null`.
pub extern fn mdix_builder_from_handle(handle: ?*anyopaque) callconv(.c) ?*anyopaque;
pub extern fn mdix_builder_free(builder: ?*anyopaque) callconv(.c) void;
pub extern fn mdix_builder_entry_count(builder: ?*anyopaque) callconv(.c) i32;
pub extern fn mdix_builder_clear(builder: ?*anyopaque) callconv(.c) bool;

// ── Builder — write ─────────────────────────────────────────────────────

pub extern fn mdix_builder_set_string(builder: ?*anyopaque, path: ?[*:0]const u8, value: ?[*:0]const u8) callconv(.c) bool;
pub extern fn mdix_builder_set_int(builder: ?*anyopaque, path: ?[*:0]const u8, value: i32) callconv(.c) bool;
pub extern fn mdix_builder_set_long(builder: ?*anyopaque, path: ?[*:0]const u8, value: i64) callconv(.c) bool;
pub extern fn mdix_builder_set_float(builder: ?*anyopaque, path: ?[*:0]const u8, value: f32) callconv(.c) bool;
pub extern fn mdix_builder_set_double(builder: ?*anyopaque, path: ?[*:0]const u8, value: f64) callconv(.c) bool;
pub extern fn mdix_builder_set_bool(builder: ?*anyopaque, path: ?[*:0]const u8, value: bool) callconv(.c) bool;
pub extern fn mdix_builder_remove(builder: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) bool;

// ── Builder — read back ─────────────────────────────────────────────────

pub extern fn mdix_builder_has_key(builder: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) bool;
pub extern fn mdix_builder_get_string(builder: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) ?[*:0]u8;
pub extern fn mdix_builder_get_int(builder: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) i32;
pub extern fn mdix_builder_get_long(builder: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) i64;
pub extern fn mdix_builder_get_float(builder: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) f32;
pub extern fn mdix_builder_get_double(builder: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) f64;
pub extern fn mdix_builder_get_bool(builder: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) bool;

// ── Builder — persistence ──────────────────────────────────────────────

/// Returned pointer must be freed with `mdix_free_string`.
pub extern fn mdix_builder_to_string(builder: ?*anyopaque) callconv(.c) ?[*:0]u8;
pub extern fn mdix_builder_save(builder: ?*anyopaque, path: ?[*:0]const u8) callconv(.c) bool;

// ── Sanity tests ─────────────────────────────────────────────────────────
// Link-level smoke tests only (no allocation/ownership exercised) — the
// idiomatic `mdix` package's test suite (mdix/tests/, forthcoming) is
// where real behavioral coverage against a running libmdix_ffi lives,
// matching mdix-odin/mdix/tests/.

test "mdix_version returns a non-null static string" {
    const v = mdix_version();
    try std.testing.expect(v != null);
    try std.testing.expect(std.mem.span(v.?).len > 0);
}

test "mdix_load_str / mdix_is_valid / mdix_free round-trip" {
    const handle = mdix_load_str("@DATA( port = 8080 )");
    try std.testing.expect(handle != null);
    defer mdix_free(handle);
    try std.testing.expect(mdix_is_valid(handle));
    try std.testing.expectEqual(@as(i32, 8080), mdix_get_int(handle, "port"));
}

test "mdix_load_str with null source fails safely" {
    const handle = mdix_load_str(null);
    try std.testing.expect(handle == null);
    // mdix_get_last_error is thread-local FFI state, not asserted here —
    // see the idiomatic layer's error-handling tests for that coverage.
}
