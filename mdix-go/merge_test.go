package dixscript

import "testing"

const mergePrimary = `
@DATA(
  app_name = "PrimaryApp"
  port     = 8080
  tags::   "primary", "base"
)`

const mergeSecondary = `
@DATA(
  app_name = "SecondaryApp"
  debug    = true
  tags::   "secondary"
)`

func TestMergeSourcesPrimaryWins(t *testing.T) {
	db, conflicts, err := MergeSources([]string{mergePrimary, mergeSecondary}, PrimaryWins, ArrayReplace)
	if err != nil {
		t.Fatalf("MergeSources: %v", err)
	}
	defer db.Close()

	name, err := db.GetString("app_name")
	if err != nil || name != "PrimaryApp" {
		t.Errorf("GetString(app_name) = %q, %v; want \"PrimaryApp\" (primary wins), nil", name, err)
	}
	// debug only exists in secondary — no conflict, should still merge in.
	debug, err := db.GetBool("debug")
	if err != nil || !debug {
		t.Errorf("GetBool(debug) = %v, %v; want true, nil", debug, err)
	}
	if len(conflicts) == 0 {
		t.Error("conflicts report is empty, expected at least the app_name conflict")
	}
}

func TestMergeSourcesSecondaryWins(t *testing.T) {
	db, _, err := MergeSources([]string{mergePrimary, mergeSecondary}, SecondaryWins, ArrayReplace)
	if err != nil {
		t.Fatalf("MergeSources: %v", err)
	}
	defer db.Close()

	name, err := db.GetString("app_name")
	if err != nil || name != "SecondaryApp" {
		t.Errorf("GetString(app_name) = %q, %v; want \"SecondaryApp\" (secondary wins), nil", name, err)
	}
}

func TestMergeSourcesThrowOnConflict(t *testing.T) {
	_, _, err := MergeSources([]string{mergePrimary, mergeSecondary}, ThrowOnConflict, ArrayReplace)
	if err == nil {
		t.Fatal("MergeSources with ThrowOnConflict returned nil error for a genuinely conflicting key (app_name)")
	}
}

func TestMergeSourcesNoConflictsDoesNotThrow(t *testing.T) {
	const a = `@DATA( only_a = 1 )`
	const b = `@DATA( only_b = 2 )`

	db, conflicts, err := MergeSources([]string{a, b}, ThrowOnConflict, ArrayReplace)
	if err != nil {
		t.Fatalf("MergeSources with no actual conflicts should not error, got: %v", err)
	}
	defer db.Close()
	if len(conflicts) != 0 {
		t.Errorf("conflicts = %v, want empty", conflicts)
	}
}

func TestMergeSourcesWeighted(t *testing.T) {
	// Secondary weighted higher than primary — should win despite being
	// second in the source list.
	db, _, err := MergeSourcesWeighted(
		[]string{mergePrimary, mergeSecondary},
		[]float64{0.1, 0.9},
		WeightedPriority,
		ArrayReplace,
	)
	if err != nil {
		t.Fatalf("MergeSourcesWeighted: %v", err)
	}
	defer db.Close()

	name, err := db.GetString("app_name")
	if err != nil || name != "SecondaryApp" {
		t.Errorf("GetString(app_name) = %q, %v; want \"SecondaryApp\" (higher weight wins), nil", name, err)
	}
}

func TestMergeSourcesArrayConcat(t *testing.T) {
	db, _, err := MergeSources([]string{mergePrimary, mergeSecondary}, PrimaryWins, ArrayConcat)
	if err != nil {
		t.Fatalf("MergeSources: %v", err)
	}
	defer db.Close()

	n, err := db.ArrayLength("tags")
	if err != nil {
		t.Fatalf("ArrayLength(tags): %v", err)
	}
	// primary has 2, secondary has 1 — concat should give 3.
	if n != 3 {
		t.Errorf("ArrayLength(tags) with ArrayConcat = %d, want 3", n)
	}
}

func TestMergeSourcesEmptyListErrors(t *testing.T) {
	if _, _, err := MergeSources(nil, PrimaryWins, ArrayReplace); err == nil {
		t.Error("MergeSources(nil sources) returned nil error")
	}
}

func TestMergeSourcesWeightedMismatchedLengthErrors(t *testing.T) {
	_, _, err := MergeSourcesWeighted([]string{mergePrimary, mergeSecondary}, []float64{1.0}, PrimaryWins, ArrayReplace)
	if err == nil {
		t.Error("MergeSourcesWeighted with mismatched sources/weights lengths returned nil error")
	}
}
