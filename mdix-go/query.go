// query.go — LINQ-style querying over Go-native data, the Go counterpart
// to dixscript::Runtime::query::DixQuery (dixscript/src/Runtime/query.rs).
//
// Deliberately does not bind DixQuery itself: every predicate, key, or
// selector it takes is a Rust closure, which a Go caller can't hand
// across the cgo boundary. Instead — the same choice Python's MdixQuery
// (mdix-python/src/query.rs) and C#'s MdixQueryExtensions
// (mdix-csharp/src/MidManStudio.Mdix.Core/MdixQuery.cs) already made —
// this fetches the array natively via GetJSON/QueryManyJSON, decodes it
// into a plain Go slice, and queries it with Go's own idioms: generics
// and closures, not a transliteration of Rust's Iterator API.
//
//	type Enemy struct {
//	    Name string `json:"name"`
//	    HP   int    `json:"hp"`
//	}
//
//	q, err := dixscript.LoadQuery[Enemy](db, "enemies")
//	if err != nil { /* handle */ }
//	heavies := q.Where(func(e Enemy) bool { return e.HP > 500 })
//	names := dixscript.Select(heavies, func(e Enemy) string { return e.Name })
//
//	// Sibling paths sharing shape, wildcarding one segment:
//	statuses, err := dixscript.QueryMany[string](db, "servers.*.status")
package dixscript

import (
	"cmp"
	"encoding/json"
	"sort"

	"github.com/Mid-D-Man/dixscript-go/internal"
)

// ── QueryManyJSON — wildcard sibling-path query, lives on Database ────────

// QueryManyJSON matches a single '*' wildcard segment (e.g.
// "servers.*.status") against every sibling path and returns every match
// as a JSON array string. Wraps mdix_select_many_as_json — see its doc
// comment in mdix-ffi/src/lib.rs for the exact wildcard semantics (one
// segment only; does not match bracket-indexed paths like
// "enemies[0].name").
func (db *Database) QueryManyJSON(pattern string) (string, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return "", err
	}
	val, ok := internal.SelectManyAsJSON(db.handle, pattern)
	if !ok {
		return "", nativeOrNotFound(pattern)
	}
	return val, nil
}

// Query is an in-memory, chainable view over a decoded slice of T. Every
// method returns a new Query (or a plain slice/value for terminal
// operations) — the original is never mutated in place.
type Query[T any] struct {
	items []T
}

// NewQuery wraps an existing slice for querying.
func NewQuery[T any](items []T) *Query[T] {
	return &Query[T]{items: items}
}

// LoadQuery fetches the array at path via GetJSON and decodes it into a
// Query[T]. Returns an error if path doesn't exist, isn't an array, or
// its elements don't decode into T.
func LoadQuery[T any](db *Database, path string) (*Query[T], error) {
	raw, err := db.GetJSON(path)
	if err != nil {
		return nil, err
	}
	var items []T
	if jerr := json.Unmarshal([]byte(raw), &items); jerr != nil {
		return nil, errTypeMismatch(path, "array decodable into target type", jerr.Error())
	}
	return &Query[T]{items: items}, nil
}

// QueryMany decodes db.QueryManyJSON(pattern) into a []T. Every match
// must decode into T; for heterogeneous matches call QueryManyJSON
// directly and decode by hand.
func QueryMany[T any](db *Database, pattern string) ([]T, error) {
	raw, err := db.QueryManyJSON(pattern)
	if err != nil {
		return nil, err
	}
	var items []T
	if jerr := json.Unmarshal([]byte(raw), &items); jerr != nil {
		return nil, errTypeMismatch(pattern, "array decodable into target type", jerr.Error())
	}
	return items, nil
}

// ── filtering / slicing (methods — single type param, so allowed) ─────────

// Where keeps only elements matching predicate. (LINQ Where)
func (q *Query[T]) Where(predicate func(T) bool) *Query[T] {
	out := make([]T, 0, len(q.items))
	for _, item := range q.items {
		if predicate(item) {
			out = append(out, item)
		}
	}
	return &Query[T]{items: out}
}

// Skip discards the first n elements. (LINQ Skip)
func (q *Query[T]) Skip(n int) *Query[T] {
	if n >= len(q.items) {
		return &Query[T]{}
	}
	if n <= 0 {
		return &Query[T]{items: q.items}
	}
	return &Query[T]{items: q.items[n:]}
}

// Take keeps only the first n elements. (LINQ Take)
func (q *Query[T]) Take(n int) *Query[T] {
	if n <= 0 {
		return &Query[T]{}
	}
	if n >= len(q.items) {
		return &Query[T]{items: q.items}
	}
	return &Query[T]{items: q.items[:n]}
}

// ── terminal predicates / accessors ────────────────────────────────────────

// Any reports whether at least one element matches predicate.
func (q *Query[T]) Any(predicate func(T) bool) bool {
	for _, item := range q.items {
		if predicate(item) {
			return true
		}
	}
	return false
}

// All reports whether every element matches predicate (true on empty).
func (q *Query[T]) All(predicate func(T) bool) bool {
	for _, item := range q.items {
		if !predicate(item) {
			return false
		}
	}
	return true
}

// Count returns the number of elements in the current result set.
func (q *Query[T]) Count() int { return len(q.items) }

// IsEmpty reports whether the current result set has zero elements.
func (q *Query[T]) IsEmpty() bool { return len(q.items) == 0 }

// First returns the first element, or false if empty.
func (q *Query[T]) First() (T, bool) {
	var zero T
	if len(q.items) == 0 {
		return zero, false
	}
	return q.items[0], true
}

// FirstOr returns the first element, or def if empty.
func (q *Query[T]) FirstOr(def T) T {
	if len(q.items) == 0 {
		return def
	}
	return q.items[0]
}

// Last returns the last element, or false if empty.
func (q *Query[T]) Last() (T, bool) {
	var zero T
	if len(q.items) == 0 {
		return zero, false
	}
	return q.items[len(q.items)-1], true
}

// Nth returns the element at index, or false if out of range.
func (q *Query[T]) Nth(index int) (T, bool) {
	var zero T
	if index < 0 || index >= len(q.items) {
		return zero, false
	}
	return q.items[index], true
}

// ToSlice returns the current result set as a plain, independent []T.
func (q *Query[T]) ToSlice() []T {
	out := make([]T, len(q.items))
	copy(out, q.items)
	return out
}

// ── free functions (need a second type parameter, so can't be methods) ────

// Select projects every element of q through mapper. (LINQ Select)
func Select[T, R any](q *Query[T], mapper func(T) R) []R {
	out := make([]R, len(q.items))
	for i, item := range q.items {
		out[i] = mapper(item)
	}
	return out
}

// OrderBy stable-sorts q ascending by key(item). (LINQ OrderBy)
func OrderBy[T any, K cmp.Ordered](q *Query[T], key func(T) K) *Query[T] {
	out := make([]T, len(q.items))
	copy(out, q.items)
	sort.SliceStable(out, func(i, j int) bool { return key(out[i]) < key(out[j]) })
	return &Query[T]{items: out}
}

// OrderByDesc stable-sorts q descending by key(item). (LINQ OrderByDescending)
func OrderByDesc[T any, K cmp.Ordered](q *Query[T], key func(T) K) *Query[T] {
	out := make([]T, len(q.items))
	copy(out, q.items)
	sort.SliceStable(out, func(i, j int) bool { return key(out[i]) > key(out[j]) })
	return &Query[T]{items: out}
}

// Distinct removes duplicate elements (by ==), preserving first-seen
// order. T must be comparable — for non-comparable T, use Where with your
// own equality logic instead.
func Distinct[T comparable](q *Query[T]) *Query[T] {
	seen := make(map[T]struct{}, len(q.items))
	out := make([]T, 0, len(q.items))
	for _, item := range q.items {
		if _, ok := seen[item]; !ok {
			seen[item] = struct{}{}
			out = append(out, item)
		}
	}
	return &Query[T]{items: out}
}

// GroupResult is one group produced by GroupBy: a key and the elements
// that share it, in first-seen order.
type GroupResult[K comparable, T any] struct {
	Key   K
	Items []T
}

// GroupBy groups elements by key(item), preserving first-seen key order
// and first-seen element order within each group. (LINQ GroupBy)
//
// Returns a slice of groups rather than a map specifically to preserve
// that order — a plain map[K][]T would silently lose it on every
// range/print, since Go map iteration order is unspecified.
func GroupBy[T any, K comparable](q *Query[T], key func(T) K) []GroupResult[K, T] {
	index := make(map[K]int, len(q.items))
	var groups []GroupResult[K, T]
	for _, item := range q.items {
		k := key(item)
		if i, ok := index[k]; ok {
			groups[i].Items = append(groups[i].Items, item)
		} else {
			index[k] = len(groups)
			groups = append(groups, GroupResult[K, T]{Key: k, Items: []T{item}})
		}
	}
	return groups
}

// MinByKey returns the element with the minimum key(item). On ties, the
// first such element wins.
func MinByKey[T any, K cmp.Ordered](q *Query[T], key func(T) K) (T, bool) {
	var best T
	var bestKey K
	found := false
	for _, item := range q.items {
		k := key(item)
		if !found || k < bestKey {
			best, bestKey, found = item, k, true
		}
	}
	return best, found
}

// MaxByKey returns the element with the maximum key(item). On ties, the
// last such element wins.
func MaxByKey[T any, K cmp.Ordered](q *Query[T], key func(T) K) (T, bool) {
	var best T
	var bestKey K
	found := false
	for _, item := range q.items {
		k := key(item)
		if !found || k >= bestKey {
			best, bestKey, found = item, k, true
		}
	}
	return best, found
}

// SumInt sums key(item) across every element.
func SumInt[T any](q *Query[T], key func(T) int64) int64 {
	var sum int64
	for _, item := range q.items {
		sum += key(item)
	}
	return sum
}

// SumFloat sums key(item) across every element.
func SumFloat[T any](q *Query[T], key func(T) float64) float64 {
	var sum float64
	for _, item := range q.items {
		sum += key(item)
	}
	return sum
}

// AvgFloat averages key(item) across every element; false if q is empty.
func AvgFloat[T any](q *Query[T], key func(T) float64) (float64, bool) {
	if len(q.items) == 0 {
		return 0, false
	}
	return SumFloat(q, key) / float64(len(q.items)), true
}
