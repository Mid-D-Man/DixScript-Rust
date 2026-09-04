//! tests.zig — aggregator for mdix/tests/, mirroring how
//! `odin test mdix-odin/mdix/tests` treats that whole directory as one
//! test package. This is the root file build.zig's `test-behavioral`
//! step points at; every sibling file's `test` blocks get pulled in
//! through the imports below.
//!
//! Unlike the inline `test` blocks scattered through mdix.zig/watch.zig/
//! types.zig/merge.zig/schema.zig/query.zig (which exercise the code
//! from inside its own file, including anything file-private), every
//! test in this directory goes through `@import("mdix")` — the same
//! public module surface an external consumer gets. Complements the
//! inline tests rather than duplicating them: a handful of focused
//! integration-style scenarios per area, not exhaustive re-coverage.

test {
    _ = @import("database_test.zig");
    _ = @import("builder_test.zig");
    _ = @import("merge_test.zig");
    _ = @import("query_test.zig");
    _ = @import("schema_test.zig");
    _ = @import("types_test.zig");
    _ = @import("watch_test.zig");
}
