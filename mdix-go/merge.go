// merge.go — merging multiple .mdix sources into one Database.
//
// Go counterpart to mdix_merge_sources / mdix_merge_sources_weighted
// (mdix-ffi/src/lib.rs), wrapping the real AST-level DixScript merger
// (dixscript::Runtime::merge) rather than a JSON round-trip — every
// DixScript type survives exactly (Long/Float/Double/HexColor/Blob/
// Regex/Date/Timestamp/Enum), and conflicts are reported per key instead
// of silently resolved.
package dixscript

import (
	"encoding/json"

	"github.com/Mid-D-Man/dixscript-go/internal"
)

// MergeConflict describes one path that more than one source defined,
// and which source's value won.
type MergeConflict struct {
	Path          string `json:"path"`
	WinningSource int    `json:"winningSource"`
	WinningLabel  string `json:"winningLabel"`
}

// MergeConflicts is the full conflict report from a merge — empty (never
// nil) when there were no conflicts.
type MergeConflicts []MergeConflict

// MergeSources merges two or more .mdix source strings into a new
// Database. Sources are weighted in descending order — sources[0] gets
// the highest weight, sources[len-1] the lowest — which only matters
// under WeightedPriority; use MergeSourcesWeighted for explicit weights.
//
// Returns the merged Database, its conflict report, and an error only if
// the merge itself failed (e.g. a source failed to parse, or
// ThrowOnConflict hit a conflicting key). The caller must Close() the
// returned Database.
func MergeSources(sources []string, strategy MergeStrategy, arrayStrategy ArrayMergeStrategy) (*Database, MergeConflicts, error) {
	if len(sources) == 0 {
		return nil, nil, errNative("MergeSources: at least one source is required")
	}
	h, conflictsJSON, ok := internal.MergeSources(sources, int32(strategy), int32(arrayStrategy))
	if !ok {
		return nil, nil, mergeError()
	}
	conflicts, err := parseMergeConflicts(conflictsJSON)
	if err != nil {
		// The merge itself succeeded — a malformed conflicts report is a
		// bug in this binding's parsing, not a merge failure, so the
		// caller still gets a usable Database back alongside the error.
		return &Database{handle: h}, nil, err
	}
	return &Database{handle: h}, conflicts, nil
}

// MergeSourcesWeighted is MergeSources with explicit per-source weights.
// weights must be the same length as sources; a higher weight wins under
// WeightedPriority.
func MergeSourcesWeighted(sources []string, weights []float64, strategy MergeStrategy, arrayStrategy ArrayMergeStrategy) (*Database, MergeConflicts, error) {
	if len(sources) == 0 {
		return nil, nil, errNative("MergeSourcesWeighted: at least one source is required")
	}
	if len(weights) != len(sources) {
		return nil, nil, errNative("MergeSourcesWeighted: weights length must match sources length")
	}
	h, conflictsJSON, ok := internal.MergeSourcesWeighted(sources, weights, int32(strategy), int32(arrayStrategy))
	if !ok {
		return nil, nil, mergeError()
	}
	conflicts, err := parseMergeConflicts(conflictsJSON)
	if err != nil {
		return &Database{handle: h}, nil, err
	}
	return &Database{handle: h}, conflicts, nil
}

func parseMergeConflicts(raw string) (MergeConflicts, error) {
	if raw == "" {
		return MergeConflicts{}, nil
	}
	var out MergeConflicts
	if err := json.Unmarshal([]byte(raw), &out); err != nil {
		return nil, errNative("failed to parse merge conflicts report: " + err.Error())
	}
	return out, nil
}

func mergeError() error {
	if msg := internal.LastError(); msg != "" {
		return errNative(msg)
	}
	return errNative("merge failed")
}
