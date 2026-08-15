package mdix

// query.odin — querying decoded Odin data, the Odin counterpart to
// dixscript::Runtime::query::DixQuery (dixscript/src/Runtime/query.rs).
//
// Deliberately doesn't bind DixQuery itself: every predicate/key/selector
// it takes is a Rust closure, which can't cross the FFI boundary. Same
// choice the Go, Python, and C# bindings already made — fetch the array
// natively via get_json/query_many, decode it with core:encoding/json's
// reflection-based unmarshal, and query the resulting Odin slice with
// Odin's own tools (parametric polymorphism + procs), not a
// transliteration of Rust's Iterator API.
//
//   Enemy :: struct {
//       name: string `json:"name"`,
//       hp:   int    `json:"hp"`,
//   }
//
//   q, ok := query_load(Enemy, db, "enemies")
//   defer query_delete(q)
//   heavies := query_where(q, proc(e: Enemy) -> bool { return e.hp > 500 })
//   defer delete(heavies.items)
//
//   // Sibling paths sharing shape, wildcarding one segment:
//   statuses, ok := query_many(string, db, "servers.*.status")

import "core:encoding/json"
import "core:slice"
import "base:intrinsics"

Query :: struct($T: typeid) {
	items: []T,
}

query_new :: proc(items: []$T) -> Query(T) {
	return Query(T){items = items}
}

// query_load fetches the array at path via get_json and decodes it into
// a Query(T) using core:encoding/json's reflection-based unmarshal —
// struct fields need `json:"..."` tags the same as anywhere else in this
// package (see Merge_Conflict in merge.odin for an example). Returns
// ok == false if path doesn't exist, isn't an array, or its elements
// don't decode into T.
query_load :: proc($T: typeid, db: Database, path: string, allocator := context.allocator) -> (Query(T), bool) {
	raw, ok := get_json(db, path, context.temp_allocator)
	if !ok {
		return {}, false
	}
	items: []T
	if err := json.unmarshal_string(raw, &items, allocator = allocator); err != nil {
		return {}, false
	}
	return Query(T){items = items}, true
}

// query_many decodes select_many_as_json(db, pattern) into a []T. Every
// match must decode into T; for heterogeneous matches call
// select_many_as_json directly and decode by hand.
query_many :: proc($T: typeid, db: Database, pattern: string, allocator := context.allocator) -> ([]T, bool) {
	raw, ok := select_many_as_json(db, pattern, context.temp_allocator)
	if !ok {
		return nil, false
	}
	items: []T
	if err := json.unmarshal_string(raw, &items, allocator = allocator); err != nil {
		return nil, false
	}
	return items, true
}

// query_delete frees the slice backing q. Only call this on a Query
// returned by query_load or query_new-over-an-owned-slice — Query values
// returned by query_where/query_skip/etc. either share or independently
// own their backing storage; see each proc's doc comment.
query_delete :: proc(q: Query($T)) {
	delete(q.items)
}

// ── filtering / slicing ─────────────────────────────────────────────────
// Each of these allocates a new backing slice — the input q is never
// mutated, and the returned Query owns its own storage (query_delete it
// independently of q).

query_where :: proc(q: Query($T), predicate: proc(T) -> bool, allocator := context.allocator) -> Query(T) {
	out := make([dynamic]T, 0, len(q.items), allocator)
	for item in q.items {
		if predicate(item) {
			append(&out, item)
		}
	}
	return Query(T){items = out[:]}
}

// query_skip and query_take return a Query sharing q's backing array
// (a sub-slice, not a copy) — do not query_delete a skip/take result
// independently of the Query it was sliced from.
query_skip :: proc(q: Query($T), n: int) -> Query(T) {
	if n >= len(q.items) {
		return Query(T){}
	}
	if n <= 0 {
		return q
	}
	return Query(T){items = q.items[n:]}
}

query_take :: proc(q: Query($T), n: int) -> Query(T) {
	if n <= 0 {
		return Query(T){}
	}
	if n >= len(q.items) {
		return q
	}
	return Query(T){items = q.items[:n]}
}

// ── terminal predicates / accessors ────────────────────────────────────

query_any :: proc(q: Query($T), predicate: proc(T) -> bool) -> bool {
	for item in q.items {
		if predicate(item) {
			return true
		}
	}
	return false
}

query_all :: proc(q: Query($T), predicate: proc(T) -> bool) -> bool {
	for item in q.items {
		if !predicate(item) {
			return false
		}
	}
	return true
}

query_count :: proc(q: Query($T)) -> int {
	return len(q.items)
}

query_is_empty :: proc(q: Query($T)) -> bool {
	return len(q.items) == 0
}

query_first :: proc(q: Query($T)) -> (T, bool) {
	if len(q.items) == 0 {
		return {}, false
	}
	return q.items[0], true
}

query_first_or :: proc(q: Query($T), fallback: T) -> T {
	if len(q.items) == 0 {
		return fallback
	}
	return q.items[0]
}

query_last :: proc(q: Query($T)) -> (T, bool) {
	if len(q.items) == 0 {
		return {}, false
	}
	return q.items[len(q.items) - 1], true
}

query_nth :: proc(q: Query($T), index: int) -> (T, bool) {
	if index < 0 || index >= len(q.items) {
		return {}, false
	}
	return q.items[index], true
}

// ── projection / ordering / grouping ────────────────────────────────────

// query_select projects every element of q through mapper into a
// freshly allocated slice — R is inferred from mapper's return type.
query_select :: proc(q: Query($T), mapper: proc(T) -> $R, allocator := context.allocator) -> []R {
	out := make([]R, len(q.items), allocator)
	for item, i in q.items {
		out[i] = mapper(item)
	}
	return out
}

// query_order_by returns a new Query, stable-sorted ascending by
// key(item). Owns its own backing storage (a clone of q.items) —
// query_delete it independently of q.
query_order_by :: proc(q: Query($T), key: proc(T) -> $K, allocator := context.allocator) -> Query(T) where intrinsics.type_is_ordered(K) {
	out, _ := slice.clone(q.items, allocator)
	slice.sort_by_key(out, key)
	return Query(T){items = out}
}

// query_order_by_desc is query_order_by, descending.
query_order_by_desc :: proc(q: Query($T), key: proc(T) -> $K, allocator := context.allocator) -> Query(T) where intrinsics.type_is_ordered(K) {
	out, _ := slice.clone(q.items, allocator)
	slice.sort_by_key(out, key)
	slice.reverse(out)
	return Query(T){items = out}
}

// query_distinct removes duplicate elements (by ==), preserving
// first-seen order. T must be a valid map key type — for T that isn't
// (e.g. a struct with a slice field), filter with query_where and your
// own equality logic instead.
query_distinct :: proc(q: Query($T), allocator := context.allocator) -> Query(T) {
	seen := make(map[T]bool, len(q.items), context.temp_allocator)
	out := make([dynamic]T, 0, len(q.items), allocator)
	for item in q.items {
		if !seen[item] {
			seen[item] = true
			append(&out, item)
		}
	}
	return Query(T){items = out[:]}
}

// Group_Result is one group produced by query_group_by: a key and the
// elements that share it, in first-seen order.
Group_Result :: struct($K, $T: typeid) {
	key:   K,
	items: [dynamic]T,
}

// query_group_by groups elements by key(item), preserving first-seen key
// order and first-seen element order within each group. Returns a slice
// of groups rather than a map specifically to preserve that order — Odin
// map iteration order is unspecified, same reason as the Go binding's
// GroupBy. Caller owns the result: delete each group's .items, then the
// returned slice itself.
query_group_by :: proc(q: Query($T), key: proc(T) -> $K, allocator := context.allocator) -> []Group_Result(K, T) {
	index := make(map[K]int, len(q.items), context.temp_allocator)
	groups := make([dynamic]Group_Result(K, T), 0, allocator)
	for item in q.items {
		k := key(item)
		if i, found := index[k]; found {
			append(&groups[i].items, item)
		} else {
			index[k] = len(groups)
			new_group := Group_Result(K, T){key = k, items = make([dynamic]T, 0, allocator)}
			append(&new_group.items, item)
			append(&groups, new_group)
		}
	}
	return groups[:]
}

// ── aggregation ──────────────────────────────────────────────────────────

query_min_by_key :: proc(q: Query($T), key: proc(T) -> $K) -> (T, bool) where intrinsics.type_is_ordered(K) {
	best: T
	best_key: K
	found := false
	for item in q.items {
		k := key(item)
		if !found || k < best_key {
			best, best_key, found = item, k, true
		}
	}
	return best, found
}

query_max_by_key :: proc(q: Query($T), key: proc(T) -> $K) -> (T, bool) where intrinsics.type_is_ordered(K) {
	best: T
	best_key: K
	found := false
	for item in q.items {
		k := key(item)
		if !found || k >= best_key {
			best, best_key, found = item, k, true
		}
	}
	return best, found
}

query_sum_int :: proc(q: Query($T), key: proc(T) -> i64) -> i64 {
	sum: i64
	for item in q.items {
		sum += key(item)
	}
	return sum
}

query_sum_float :: proc(q: Query($T), key: proc(T) -> f64) -> f64 {
	sum: f64
	for item in q.items {
		sum += key(item)
	}
	return sum
}

query_avg_float :: proc(q: Query($T), key: proc(T) -> f64) -> (f64, bool) {
	if len(q.items) == 0 {
		return 0, false
	}
	return query_sum_float(q, key) / f64(len(q.items)), true
}
