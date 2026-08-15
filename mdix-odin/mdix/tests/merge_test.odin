package mdix_tests

import "core:testing"
import mdix "../"

MERGE_PRIMARY :: `
@DATA(
  app_name = "PrimaryApp"
  port     = 8080
  tags::   "primary", "base"
)`

MERGE_SECONDARY :: `
@DATA(
  app_name = "SecondaryApp"
  debug    = true
  tags::   "secondary"
)`

@(test)
merge_primary_wins :: proc(t: ^testing.T) {
	db, conflicts, ok := mdix.merge_sources({MERGE_PRIMARY, MERGE_SECONDARY}, .Primary_Wins, .Replace)
	testing.expect(t, ok, "merge_sources should succeed")
	defer mdix.destroy(&db)
	defer delete(conflicts)

	name, ok2 := mdix.get_string(db, "app_name")
	defer delete(name)
	testing.expect(t, ok2, "get_string(app_name) should succeed")
	testing.expect_value(t, name, "PrimaryApp")

	debug, ok3 := mdix.get_bool(db, "debug")
	testing.expect(t, ok3, "get_bool(debug) should succeed — no conflict, merges straight in")
	testing.expect(t, debug, "debug should be true (only defined in secondary)")

	testing.expect(t, len(conflicts) > 0, "conflicts should be non-empty (app_name is defined by both)")
}

@(test)
merge_secondary_wins :: proc(t: ^testing.T) {
	db, conflicts, ok := mdix.merge_sources({MERGE_PRIMARY, MERGE_SECONDARY}, .Secondary_Wins, .Replace)
	testing.expect(t, ok, "merge_sources should succeed")
	defer mdix.destroy(&db)
	defer delete(conflicts)

	name, ok2 := mdix.get_string(db, "app_name")
	defer delete(name)
	testing.expect(t, ok2, "get_string(app_name) should succeed")
	testing.expect_value(t, name, "SecondaryApp")
}

@(test)
merge_throw_on_conflict :: proc(t: ^testing.T) {
	_, _, ok := mdix.merge_sources({MERGE_PRIMARY, MERGE_SECONDARY}, .Throw_On_Conflict, .Replace)
	testing.expect(t, !ok, "merge with Throw_On_Conflict should fail on a genuinely conflicting key (app_name)")
}

@(test)
merge_no_conflicts_does_not_throw :: proc(t: ^testing.T) {
	a :: `@DATA( only_a = 1 )`
	b :: `@DATA( only_b = 2 )`
	db, conflicts, ok := mdix.merge_sources({a, b}, .Throw_On_Conflict, .Replace)
	testing.expect(t, ok, "merge with no actual conflicts should not fail under Throw_On_Conflict")
	defer mdix.destroy(&db)
	defer delete(conflicts)
	testing.expect_value(t, len(conflicts), 0)
}

@(test)
merge_weighted :: proc(t: ^testing.T) {
	// Secondary weighted higher than primary — should win despite being
	// second in the source list.
	db, conflicts, ok := mdix.merge_sources_weighted(
		{MERGE_PRIMARY, MERGE_SECONDARY},
		{0.1, 0.9},
		.Weighted_Priority,
		.Replace,
	)
	testing.expect(t, ok, "merge_sources_weighted should succeed")
	defer mdix.destroy(&db)
	defer delete(conflicts)

	name, ok2 := mdix.get_string(db, "app_name")
	defer delete(name)
	testing.expect(t, ok2, "get_string(app_name) should succeed")
	testing.expect_value(t, name, "SecondaryApp")
}

@(test)
merge_array_concat :: proc(t: ^testing.T) {
	db, conflicts, ok := mdix.merge_sources({MERGE_PRIMARY, MERGE_SECONDARY}, .Primary_Wins, .Concat)
	testing.expect(t, ok, "merge_sources should succeed")
	defer mdix.destroy(&db)
	defer delete(conflicts)

	n := mdix.array_length(db, "tags")
	testing.expect_value(t, n, 3) // primary has 2, secondary has 1 -> concat = 3
}

@(test)
merge_empty_sources_fails :: proc(t: ^testing.T) {
	_, _, ok := mdix.merge_sources({}, .Primary_Wins, .Replace)
	testing.expect(t, !ok, "merge_sources with no sources should fail")
}

@(test)
merge_weighted_mismatched_lengths_fails :: proc(t: ^testing.T) {
	_, _, ok := mdix.merge_sources_weighted({MERGE_PRIMARY, MERGE_SECONDARY}, {1.0}, .Weighted_Priority, .Replace)
	testing.expect(t, !ok, "merge_sources_weighted with mismatched sources/weights lengths should fail")
}
