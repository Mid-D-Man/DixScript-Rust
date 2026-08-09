package dixscript

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestBuilderSetGetRoundTrip(t *testing.T) {
	b := NewBuilder()
	defer b.Close()

	if err := b.SetString("name", "Widget"); err != nil {
		t.Fatalf("SetString: %v", err)
	}
	if err := b.SetInt("count", 7); err != nil {
		t.Fatalf("SetInt: %v", err)
	}
	if err := b.SetFloat64("price", 19.99); err != nil {
		t.Fatalf("SetFloat64: %v", err)
	}
	if err := b.SetBool("active", true); err != nil {
		t.Fatalf("SetBool: %v", err)
	}

	if got, err := b.GetString("name"); err != nil || got != "Widget" {
		t.Errorf("GetString(name) = %q, %v; want \"Widget\", nil", got, err)
	}
	if got, err := b.GetInt("count"); err != nil || got != 7 {
		t.Errorf("GetInt(count) = %d, %v; want 7, nil", got, err)
	}
	if got, err := b.GetFloat64("price"); err != nil || got != 19.99 {
		t.Errorf("GetFloat64(price) = %v, %v; want 19.99, nil", got, err)
	}
	if got, err := b.GetBool("active"); err != nil || got != true {
		t.Errorf("GetBool(active) = %v, %v; want true, nil", got, err)
	}

	if n := b.EntryCount(); n != 4 {
		t.Errorf("EntryCount() = %d, want 4", n)
	}
}

// TestBuilderInt64RoundTrip is the regression test for the third bug
// found in this pass: BuilderSetLong/BuilderGetLong (mdix_builder_set_long
// / mdix_builder_get_long) didn't exist in this package at all before —
// only the 32-bit Int setter/getter did, so a genuine Long value could
// never be built through Builder. 9_000_000_000 overflows int32.
func TestBuilderInt64RoundTrip(t *testing.T) {
	b := NewBuilder()
	defer b.Close()

	const want = 9_000_000_000
	if err := b.SetInt64("big_id", want); err != nil {
		t.Fatalf("SetInt64: %v", err)
	}
	got, err := b.GetInt64("big_id")
	if err != nil {
		t.Fatalf("GetInt64: %v", err)
	}
	if got != want {
		t.Errorf("GetInt64(big_id) = %d, want %d", got, want)
	}

	// And it must survive a real round trip through ToDatabase — this is
	// what actually exercises mdix_builder_set_long end to end, not just
	// the builder's own in-memory read-back.
	db, err := b.ToDatabase()
	if err != nil {
		t.Fatalf("ToDatabase: %v", err)
	}
	defer db.Close()

	if typ := db.ValueTypeAt("big_id"); typ != TypeLong {
		t.Errorf("ValueTypeAt(big_id) on built database = %v, want Long", typ)
	}
	dbGot, err := db.GetInt64("big_id")
	if err != nil || dbGot != want {
		t.Errorf("Database.GetInt64(big_id) = %d, %v; want %d, nil", dbGot, err, want)
	}
}

func TestBuilderRemoveAndHasKey(t *testing.T) {
	b := NewBuilder()
	defer b.Close()

	_ = b.SetString("temp", "x")
	if !b.HasKey("temp") {
		t.Fatal("HasKey(temp) = false right after SetString")
	}

	removed, err := b.Remove("temp")
	if err != nil || !removed {
		t.Errorf("Remove(temp) = %v, %v; want true, nil", removed, err)
	}
	if b.HasKey("temp") {
		t.Error("HasKey(temp) = true after Remove")
	}

	removedAgain, err := b.Remove("temp")
	if err != nil || removedAgain {
		t.Errorf("Remove(temp) on an already-removed key = %v, %v; want false, nil", removedAgain, err)
	}
}

func TestBuilderClear(t *testing.T) {
	b := NewBuilder()
	defer b.Close()

	_ = b.SetString("a", "1")
	_ = b.SetString("b", "2")
	if n := b.EntryCount(); n != 2 {
		t.Fatalf("EntryCount() before Clear = %d, want 2", n)
	}

	if err := b.Clear(); err != nil {
		t.Fatalf("Clear: %v", err)
	}
	if n := b.EntryCount(); n != 0 {
		t.Errorf("EntryCount() after Clear = %d, want 0", n)
	}
}

func TestBuilderSetDateAndTimestamp(t *testing.T) {
	b := NewBuilder()
	defer b.Close()

	when := time.Date(2025, 6, 15, 9, 30, 0, 0, time.UTC)
	if err := b.SetDate("released", when); err != nil {
		t.Fatalf("SetDate: %v", err)
	}
	if err := b.SetTimestamp("last_ping", when); err != nil {
		t.Fatalf("SetTimestamp: %v", err)
	}

	db, err := b.ToDatabase()
	if err != nil {
		t.Fatalf("ToDatabase: %v", err)
	}
	defer db.Close()

	date, err := db.GetDate("released")
	if err != nil {
		t.Fatalf("GetDate: %v", err)
	}
	if date.Value.Year() != 2025 || date.Value.Month() != 6 || date.Value.Day() != 15 {
		t.Errorf("GetDate(released) = %v, want 2025-06-15", date.Value)
	}
}

func TestBuilderSaveToFileAndLoad(t *testing.T) {
	b := NewBuilder()
	defer b.Close()
	_ = b.SetString("app_name", "SavedApp")
	_ = b.SetInt("version", 1)

	dir := t.TempDir()
	path := filepath.Join(dir, "nested", "saved.mdix") // exercises intermediate-dir creation
	if err := b.SaveToFile(path); err != nil {
		t.Fatalf("SaveToFile: %v", err)
	}
	if _, err := os.Stat(path); err != nil {
		t.Fatalf("saved file not found on disk: %v", err)
	}

	db, err := Load(path)
	if err != nil {
		t.Fatalf("Load(%q): %v", path, err)
	}
	defer db.Close()

	name, err := db.GetString("app_name")
	if err != nil || name != "SavedApp" {
		t.Errorf("GetString(app_name) after save+load = %q, %v; want \"SavedApp\", nil", name, err)
	}
}

func TestBuilderCloseIsIdempotent(t *testing.T) {
	b := NewBuilder()
	if err := b.Close(); err != nil {
		t.Fatalf("first Close(): %v", err)
	}
	if err := b.Close(); err != nil {
		t.Fatalf("second Close() should be a no-op, got: %v", err)
	}
	if err := b.SetString("x", "y"); err == nil {
		t.Error("SetString after Close() returned nil error, want ErrClosed")
	}
}
