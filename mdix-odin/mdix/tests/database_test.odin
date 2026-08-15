package mdix_tests

import "core:testing"
import mdix "../"

BASIC_FIXTURE :: `
@DATA(
  app_name = "TestApp"
  version  = 2
  big_id   = 9_000_000_000L
  pi       = 3.14159
  debug    = false
  server: host = "localhost", port = 8080, ssl = true
  tags::   "odin", "config", "fast"
)`

@(test)
load_str_and_getters :: proc(t: ^testing.T) {
	db, ok := mdix.load_str(BASIC_FIXTURE)
	testing.expect(t, ok, "load_str should succeed on valid source")
	defer mdix.destroy(&db)

	testing.expect(t, mdix.is_valid(db), "is_valid should be true right after load")

	name, ok1 := mdix.get_string(db, "app_name")
	defer delete(name)
	testing.expect(t, ok1, "get_string(app_name) should succeed")
	testing.expect_value(t, name, "TestApp")

	version, ok2 := mdix.get_int(db, "version")
	testing.expect(t, ok2, "get_int(version) should succeed")
	testing.expect_value(t, version, 2)

	pi, ok3 := mdix.get_double(db, "pi")
	testing.expect(t, ok3, "get_double(pi) should succeed")
	testing.expect_value(t, pi, 3.14159)

	debug, ok4 := mdix.get_bool(db, "debug")
	testing.expect(t, ok4, "get_bool(debug) should succeed")
	testing.expect_value(t, debug, false)

	host, ok5 := mdix.get_string(db, "server.host")
	defer delete(host)
	testing.expect(t, ok5, "get_string(server.host) should succeed")
	testing.expect_value(t, host, "localhost")

	port, ok6 := mdix.get_int(db, "server.port")
	testing.expect(t, ok6, "get_int(server.port) should succeed")
	testing.expect_value(t, port, 8080)
}

@(test)
get_long_reads_genuine_long :: proc(t: ^testing.T) {
	db, ok := mdix.load_str(BASIC_FIXTURE)
	testing.expect(t, ok, "load_str should succeed")
	defer mdix.destroy(&db)

	// 9_000_000_000 overflows i32 (max ~2.1 billion) by a wide margin —
	// this only reads correctly through get_long, never get_int.
	got, ok2 := mdix.get_long(db, "big_id")
	testing.expect(t, ok2, "get_long(big_id) should succeed")
	testing.expect_value(t, got, i64(9_000_000_000))
}

@(test)
exists_and_array_length :: proc(t: ^testing.T) {
	db, ok := mdix.load_str(BASIC_FIXTURE)
	testing.expect(t, ok, "load_str should succeed")
	defer mdix.destroy(&db)

	testing.expect(t, mdix.exists(db, "app_name"), "exists(app_name) should be true")
	testing.expect(t, !mdix.exists(db, "does.not.exist"), "exists(does.not.exist) should be false")

	n := mdix.array_length(db, "tags")
	testing.expect_value(t, n, 3)
}

@(test)
value_type_matches_ffi_discriminants :: proc(t: ^testing.T) {
	// Regression coverage for the same class of bug fixed in mdix-go:
	// confirms Mdix_Type's Long variant actually round-trips through
	// get_type for a genuinely Long-typed value, not just that the enum
	// numerically lines up with the header (already true here — see
	// mdix_ffi.odin's own comment — but this exercises it end to end).
	db, ok := mdix.load_str(BASIC_FIXTURE)
	testing.expect(t, ok, "load_str should succeed")
	defer mdix.destroy(&db)

	testing.expect_value(t, mdix.get_type(db, "app_name"), mdix.Mdix_Type.String)
	testing.expect_value(t, mdix.get_type(db, "version"), mdix.Mdix_Type.Int)
	testing.expect_value(t, mdix.get_type(db, "big_id"), mdix.Mdix_Type.Long)
	testing.expect_value(t, mdix.get_type(db, "pi"), mdix.Mdix_Type.Double)
	testing.expect_value(t, mdix.get_type(db, "debug"), mdix.Mdix_Type.Bool)
	testing.expect_value(t, mdix.get_type(db, "tags"), mdix.Mdix_Type.Array)
}

@(test)
destroy_is_safe_to_call_twice :: proc(t: ^testing.T) {
	db, ok := mdix.load_str(BASIC_FIXTURE)
	testing.expect(t, ok, "load_str should succeed")

	mdix.destroy(&db)
	testing.expect(t, db.handle == nil, "handle should be nil after destroy")
	mdix.destroy(&db) // must not double-free
}

@(test)
load_missing_file_fails_cleanly :: proc(t: ^testing.T) {
	_, ok := mdix.load("/nonexistent/path/config.mdix")
	testing.expect(t, !ok, "load on a nonexistent path should fail, not panic")
}
