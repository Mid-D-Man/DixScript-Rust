package mdix

// merge.odin — merging multiple .mdix sources into one Database.
//
// Wraps mdix_merge_sources / mdix_merge_sources_weighted — the real
// AST-level DixScript merger, not a JSON round-trip, so every DixScript
// type survives exactly (Long/Float/Double/Hex_Color/Blob/Regex/Date/
// Timestamp/Enum), and conflicts are reported per key instead of
// silently resolved.

import "core:c"
import "core:encoding/json"
import "core:strings"
import ffi "../mdix_ffi"

Merge_Strategy :: ffi.Mdix_Merge_Strategy
Array_Merge_Strategy :: ffi.Array_Merge_Strategy

Weighted_Priority :: Merge_Strategy.Weighted_Priority
Primary_Wins :: Merge_Strategy.Primary_Wins
Secondary_Wins :: Merge_Strategy.Secondary_Wins
Throw_On_Conflict :: Merge_Strategy.Throw_On_Conflict

Array_Replace :: Array_Merge_Strategy.Replace
Array_Concat :: Array_Merge_Strategy.Concat
Array_Concat_Dedup :: Array_Merge_Strategy.Concat_Dedup

// One path that more than one source defined, and which source won.
Merge_Conflict :: struct {
	path:           string `json:"path"`,
	winning_source: int `json:"winningSource"`,
	winning_label:  string `json:"winningLabel"`,
}

// merge_sources merges two or more .mdix source strings into a new
// Database. Sources are weighted in descending order — sources[0] gets
// the highest weight, the last source the lowest — which only matters
// under .Weighted_Priority; use merge_sources_weighted for explicit
// weights.
//
// Returns the merged Database, its conflict report (empty, not the zero
// value, when there were none), and ok == false only if the merge itself
// failed (a source failed to parse, or .Throw_On_Conflict hit a
// conflicting key) — check last_error(). The caller must destroy() the
// returned Database when ok.
//
// conflicts is allocated with `allocator` and owned by the caller —
// delete each .path/.winning_label and the slice itself (or use a
// temp/arena allocator and skip manual cleanup, same as every other
// allocator-taking proc in this package).
merge_sources :: proc(
	sources: []string,
	strategy := Merge_Strategy.Primary_Wins,
	array_strategy := Array_Merge_Strategy.Replace,
	allocator := context.allocator,
) -> (db: Database, conflicts: []Merge_Conflict, ok: bool) {
	if len(sources) == 0 {
		return {}, nil, false
	}

	csources := make([]cstring, len(sources), context.temp_allocator)
	for s, i in sources {
		csources[i] = strings.clone_to_cstring(s, context.temp_allocator)
	}

	out_conflicts: cstring
	h := ffi.mdix_merge_sources(
		raw_data(csources),
		c.int32_t(len(sources)),
		strategy,
		array_strategy,
		&out_conflicts,
	)
	if h == nil {
		return {}, nil, false
	}

	conflicts = parse_merge_conflicts(out_conflicts, allocator)
	return Database{handle = h}, conflicts, true
}

// merge_sources_weighted is merge_sources with explicit per-source
// weights. weights must be the same length as sources; a higher weight
// wins under .Weighted_Priority.
merge_sources_weighted :: proc(
	sources: []string,
	weights: []f64,
	strategy := Merge_Strategy.Weighted_Priority,
	array_strategy := Array_Merge_Strategy.Replace,
	allocator := context.allocator,
) -> (db: Database, conflicts: []Merge_Conflict, ok: bool) {
	if len(sources) == 0 || len(sources) != len(weights) {
		return {}, nil, false
	}

	csources := make([]cstring, len(sources), context.temp_allocator)
	for s, i in sources {
		csources[i] = strings.clone_to_cstring(s, context.temp_allocator)
	}

	out_conflicts: cstring
	h := ffi.mdix_merge_sources_weighted(
		raw_data(csources),
		raw_data(weights),
		c.int32_t(len(sources)),
		strategy,
		array_strategy,
		&out_conflicts,
	)
	if h == nil {
		return {}, nil, false
	}

	conflicts = parse_merge_conflicts(out_conflicts, allocator)
	return Database{handle = h}, conflicts, true
}

@(private = "file")
parse_merge_conflicts :: proc(raw: cstring, allocator := context.allocator) -> []Merge_Conflict {
	if raw == nil {
		return nil
	}
	defer ffi.mdix_free_string(raw)

	// mdix_merge_sources reports "[]" (not nil) when there were no
	// conflicts — json.parse handles that fine as an empty array, but
	// skip the round trip for the common case.
	raw_str := string(raw)
	if raw_str == "" || raw_str == "[]" {
		return nil
	}

	value, err := json.parse_string(raw_str, allocator = context.temp_allocator)
	if err != nil {
		return nil
	}
	arr, is_arr := value.(json.Array)
	if !is_arr || len(arr) == 0 {
		return nil
	}

	out := make([]Merge_Conflict, len(arr), allocator)
	for entry, i in arr {
		obj, is_obj := entry.(json.Object)
		if !is_obj {
			continue
		}
		if p, has_p := obj["path"]; has_p {
			if s, is_s := p.(json.String); is_s {
				out[i].path = strings.clone(s, allocator)
			}
		}
		if w, has_w := obj["winningSource"]; has_w {
			if n, is_n := w.(json.Float); is_n {
				out[i].winning_source = int(n)
			}
		}
		if l, has_l := obj["winningLabel"]; has_l {
			if s, is_s := l.(json.String); is_s {
				out[i].winning_label = strings.clone(s, allocator)
			}
		}
	}
	return out
}
