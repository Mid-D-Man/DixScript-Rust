package mdix_tests

import "core:os"
import "core:testing"
import "core:time"
import mdix "../"

// hot_reload_init is pure os.stat + string clone — no native lib
// required, so this genuinely runs (not just compiles) in this sandbox.
@(test)
hot_reload_init_succeeds_on_real_file :: proc(t: ^testing.T) {
	path := "/tmp/mdix_odin_watch_test_init.mdix"
	write_err := os.write_entire_file(path, `@DATA( x = 1 )`)
	testing.expect(t, write_err == nil, "test setup: writing the fixture file should succeed")

	hr: mdix.Hot_Reload
	ok := mdix.hot_reload_init(&hr, path)
	defer mdix.hot_reload_destroy(&hr)
	testing.expect(t, ok, "hot_reload_init should succeed on a real, existing file")
	testing.expect(t, hr.valid, "hr.valid should be true after a successful init")
	testing.expect_value(t, hr.path, path)
}

@(test)
hot_reload_init_fails_on_missing_file :: proc(t: ^testing.T) {
	hr: mdix.Hot_Reload
	ok := mdix.hot_reload_init(&hr, "/nonexistent/path/config.mdix")
	testing.expect(t, !ok, "hot_reload_init should fail on a nonexistent path")
	testing.expect(t, !hr.valid, "hr.valid should stay false on a failed init")
}

@(test)
hot_reload_destroy_is_safe_when_never_initialized :: proc(t: ^testing.T) {
	hr: mdix.Hot_Reload
	mdix.hot_reload_destroy(&hr) // must not panic on a zero-value Hot_Reload
}

@(test)
hot_reload_check_is_noop_on_uninitialized :: proc(t: ^testing.T) {
	hr: mdix.Hot_Reload
	db: mdix.Database
	reloaded := mdix.hot_reload_check(&hr, &db)
	testing.expect(t, !reloaded, "hot_reload_check on an uninitialized Hot_Reload should return false, not panic")
}

// hot_reload_check's actual reload path calls ffi.mdix_load, which needs
// the real native lib this sandbox doesn't have — that path is exercised
// by CI (real Rust-built lib), not here. What's testable without it: the
// mtime-advance detection itself, isolated from the reload call.
@(test)
hot_reload_detects_mtime_advance :: proc(t: ^testing.T) {
	path := "/tmp/mdix_odin_watch_test_mtime.mdix"
	_ = os.write_entire_file(path, `@DATA( value = 1 )`)

	hr: mdix.Hot_Reload
	ok := mdix.hot_reload_init(&hr, path)
	testing.expect(t, ok, "hot_reload_init should succeed")
	defer mdix.hot_reload_destroy(&hr)

	initial_mod := hr.last_mod

	// mtime resolution can be coarse on some filesystems — sleep past a
	// full second boundary so the rewritten file's mtime is unambiguous.
	time.sleep(1100 * time.Millisecond)
	_ = os.write_entire_file(path, `@DATA( value = 2 )`)

	new_mod, err := os.modification_time_by_path(path)
	testing.expect(t, err == nil, "re-stat after rewrite should succeed")
	testing.expect(t, time.diff(initial_mod, new_mod) > 0, "mtime should have advanced after the rewrite")
}
