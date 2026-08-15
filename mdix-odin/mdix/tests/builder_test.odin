package mdix_tests

import "core:testing"
import mdix "../"

@(test)
builder_set_get_round_trip :: proc(t: ^testing.T) {
	b := mdix.builder_new()
	defer mdix.builder_destroy(&b)

	testing.expect(t, mdix.builder_set_string(b, "name", "Widget"), "builder_set_string should succeed")
	testing.expect(t, mdix.builder_set_int(b, "count", 7), "builder_set_int should succeed")
	testing.expect(t, mdix.builder_set_double(b, "price", 19.99), "builder_set_double should succeed")
	testing.expect(t, mdix.builder_set_bool(b, "active", true), "builder_set_bool should succeed")

	name, ok1 := mdix.builder_get_string(b, "name")
	defer delete(name)
	testing.expect(t, ok1, "builder_get_string should succeed")
	testing.expect_value(t, name, "Widget")

	count, ok2 := mdix.builder_get_int(b, "count")
	testing.expect(t, ok2, "builder_get_int should succeed")
	testing.expect_value(t, count, 7)

	testing.expect_value(t, mdix.builder_entry_count(b), 4)
}

@(test)
builder_long_round_trip :: proc(t: ^testing.T) {
	b := mdix.builder_new()
	defer mdix.builder_destroy(&b)

	testing.expect(t, mdix.builder_set_long(b, "big_id", 9_000_000_000), "builder_set_long should succeed")
	got, ok := mdix.builder_get_long(b, "big_id")
	testing.expect(t, ok, "builder_get_long should succeed")
	testing.expect_value(t, got, i64(9_000_000_000))

	// And it must survive a real round trip through builder_to_database.
	db, ok2 := mdix.builder_to_database(b)
	testing.expect(t, ok2, "builder_to_database should succeed")
	defer mdix.destroy(&db)

	testing.expect_value(t, mdix.get_type(db, "big_id"), mdix.Mdix_Type.Long)
	db_got, ok3 := mdix.get_long(db, "big_id")
	testing.expect(t, ok3, "get_long on the built database should succeed")
	testing.expect_value(t, db_got, i64(9_000_000_000))
}

@(test)
builder_remove_and_has_key :: proc(t: ^testing.T) {
	b := mdix.builder_new()
	defer mdix.builder_destroy(&b)

	mdix.builder_set_string(b, "temp", "x")
	testing.expect(t, mdix.builder_has_key(b, "temp"), "has_key should be true right after set")

	testing.expect(t, mdix.builder_remove(b, "temp"), "remove should succeed on an existing key")
	testing.expect(t, !mdix.builder_has_key(b, "temp"), "has_key should be false after remove")
	testing.expect(t, !mdix.builder_remove(b, "temp"), "remove on an already-removed key should return false")
}

@(test)
builder_clear :: proc(t: ^testing.T) {
	b := mdix.builder_new()
	defer mdix.builder_destroy(&b)

	mdix.builder_set_string(b, "a", "1")
	mdix.builder_set_string(b, "b", "2")
	testing.expect_value(t, mdix.builder_entry_count(b), 2)

	testing.expect(t, mdix.builder_clear(b), "clear should succeed")
	testing.expect_value(t, mdix.builder_entry_count(b), 0)
}

@(test)
builder_save_and_load :: proc(t: ^testing.T) {
	b := mdix.builder_new()
	defer mdix.builder_destroy(&b)
	mdix.builder_set_string(b, "app_name", "SavedApp")
	mdix.builder_set_int(b, "version", 1)

	path := "/tmp/mdix_odin_builder_test.mdix"
	testing.expect(t, mdix.builder_save(b, path), "builder_save should succeed")

	db, ok := mdix.load(path)
	testing.expect(t, ok, "load of the saved file should succeed")
	defer mdix.destroy(&db)

	name, ok2 := mdix.get_string(db, "app_name")
	defer delete(name)
	testing.expect(t, ok2, "get_string(app_name) after save+load should succeed")
	testing.expect_value(t, name, "SavedApp")
}
