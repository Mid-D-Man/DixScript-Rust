package mdix_tests

import "core:testing"
import mdix "../"

Test_Enemy :: struct {
	name: string,
	hp:   int,
}

enemies := []Test_Enemy{
	{"Goblin", 50},
	{"Orc", 120},
	{"Orc", 120},
	{"Dragon", 900},
	{"Skeleton", 40},
}

@(test)
query_count_and_empty :: proc(t: ^testing.T) {
	q := mdix.query_new(enemies)
	testing.expect_value(t, mdix.query_count(q), 5)
	testing.expect(t, !mdix.query_is_empty(q), "query_is_empty should be false for a non-empty query")
}

@(test)
query_where_filters :: proc(t: ^testing.T) {
	q := mdix.query_new(enemies)
	heavies := mdix.query_where(q, proc(e: Test_Enemy) -> bool { return e.hp > 100 })
	defer mdix.query_delete(heavies)
	testing.expect_value(t, mdix.query_count(heavies), 3) // Orc, Orc, Dragon
}

@(test)
query_select_projects :: proc(t: ^testing.T) {
	q := mdix.query_new(enemies)
	names := mdix.query_select(q, proc(e: Test_Enemy) -> string { return e.name })
	defer delete(names)
	testing.expect_value(t, len(names), 5)
	testing.expect_value(t, names[0], "Goblin")
	testing.expect_value(t, names[4], "Skeleton")
}

@(test)
query_order_by_ascending :: proc(t: ^testing.T) {
	q := mdix.query_new(enemies)
	asc := mdix.query_order_by(q, proc(e: Test_Enemy) -> int { return e.hp })
	defer mdix.query_delete(asc)
	testing.expect_value(t, asc.items[0].hp, 40)
	testing.expect_value(t, asc.items[len(asc.items) - 1].hp, 900)
}

@(test)
query_order_by_desc :: proc(t: ^testing.T) {
	q := mdix.query_new(enemies)
	desc := mdix.query_order_by_desc(q, proc(e: Test_Enemy) -> int { return e.hp })
	defer mdix.query_delete(desc)
	testing.expect_value(t, desc.items[0].hp, 900)
	testing.expect_value(t, desc.items[len(desc.items) - 1].hp, 40)
}

@(test)
query_distinct_by_name :: proc(t: ^testing.T) {
	q := mdix.query_new(enemies)
	names := mdix.query_select(q, proc(e: Test_Enemy) -> string { return e.name })
	defer delete(names)
	name_query := mdix.query_new(names)
	distinct_names := mdix.query_distinct(name_query)
	defer mdix.query_delete(distinct_names)
	// first-seen order, one Orc: Goblin, Orc, Dragon, Skeleton
	testing.expect_value(t, len(distinct_names.items), 4)
	testing.expect_value(t, distinct_names.items[1], "Orc")
}

@(test)
query_group_by_name :: proc(t: ^testing.T) {
	q := mdix.query_new(enemies)
	groups := mdix.query_group_by(q, proc(e: Test_Enemy) -> string { return e.name })
	defer {
		for g in groups {
			delete(g.items)
		}
		delete(groups)
	}
	testing.expect_value(t, len(groups), 4)
	testing.expect_value(t, groups[1].key, "Orc")
	testing.expect_value(t, len(groups[1].items), 2)
}

@(test)
query_min_max_by_key :: proc(t: ^testing.T) {
	q := mdix.query_new(enemies)
	weakest, ok1 := mdix.query_min_by_key(q, proc(e: Test_Enemy) -> int { return e.hp })
	testing.expect(t, ok1, "query_min_by_key should find a result on a non-empty query")
	testing.expect_value(t, weakest.name, "Skeleton")

	strongest, ok2 := mdix.query_max_by_key(q, proc(e: Test_Enemy) -> int { return e.hp })
	testing.expect(t, ok2, "query_max_by_key should find a result on a non-empty query")
	testing.expect_value(t, strongest.name, "Dragon")
}

@(test)
query_sum_and_avg :: proc(t: ^testing.T) {
	q := mdix.query_new(enemies)
	sum := mdix.query_sum_int(q, proc(e: Test_Enemy) -> i64 { return i64(e.hp) })
	testing.expect_value(t, sum, i64(50 + 120 + 120 + 900 + 40))

	avg, ok := mdix.query_avg_float(q, proc(e: Test_Enemy) -> f64 { return f64(e.hp) })
	testing.expect(t, ok, "query_avg_float should succeed on a non-empty query")
	testing.expect_value(t, avg, f64(50 + 120 + 120 + 900 + 40) / 5.0)
}

@(test)
query_skip_take_first_last_nth :: proc(t: ^testing.T) {
	q := mdix.query_new(enemies)

	first, ok1 := mdix.query_first(q)
	testing.expect(t, ok1, "query_first should succeed")
	testing.expect_value(t, first.name, "Goblin")

	last, ok2 := mdix.query_last(q)
	testing.expect(t, ok2, "query_last should succeed")
	testing.expect_value(t, last.name, "Skeleton")

	nth, ok3 := mdix.query_nth(q, 2)
	testing.expect(t, ok3, "query_nth(2) should succeed")
	testing.expect_value(t, nth.name, "Orc")

	skipped := mdix.query_skip(q, 3)
	testing.expect_value(t, mdix.query_count(skipped), 2)

	taken := mdix.query_take(q, 2)
	testing.expect_value(t, mdix.query_count(taken), 2)
}

@(test)
query_any_all :: proc(t: ^testing.T) {
	q := mdix.query_new(enemies)
	testing.expect(t, mdix.query_any(q, proc(e: Test_Enemy) -> bool { return e.hp > 800 }), "query_any(hp > 800) should be true (Dragon)")
	testing.expect(t, !mdix.query_all(q, proc(e: Test_Enemy) -> bool { return e.hp > 800 }), "query_all(hp > 800) should be false")
}

@(test)
query_on_empty_result_set :: proc(t: ^testing.T) {
	q := mdix.query_new(enemies)
	empty := mdix.query_where(q, proc(e: Test_Enemy) -> bool { return e.hp > 100000 })
	defer mdix.query_delete(empty)

	testing.expect(t, mdix.query_is_empty(empty), "query_is_empty should be true")
	testing.expect_value(t, mdix.query_count(empty), 0)

	_, ok := mdix.query_first(empty)
	testing.expect(t, !ok, "query_first on an empty query should return ok=false")

	fallback := mdix.query_first_or(empty, Test_Enemy{name = "none"})
	testing.expect_value(t, fallback.name, "none")

	_, ok2 := mdix.query_min_by_key(empty, proc(e: Test_Enemy) -> int { return e.hp })
	testing.expect(t, !ok2, "query_min_by_key on an empty query should return ok=false")

	_, ok3 := mdix.query_avg_float(empty, proc(e: Test_Enemy) -> f64 { return f64(e.hp) })
	testing.expect(t, !ok3, "query_avg_float on an empty query should return ok=false")
}
