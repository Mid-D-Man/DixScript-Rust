package dixscript

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestEnableHotReloadRequiresLoadedFromFile(t *testing.T) {
	db, err := LoadStr(`@DATA( x = 1 )`)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	if err := db.EnableHotReload(0); err == nil {
		t.Error("EnableHotReload on a LoadStr-loaded Database returned nil error")
	}
	if db.IsHotReloadEnabled() {
		t.Error("IsHotReloadEnabled() = true after a failed EnableHotReload")
	}
}

func TestEnableHotReloadTwiceErrors(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "config.mdix")
	if err := os.WriteFile(path, []byte(`@DATA( x = 1 )`), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	db, err := Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	defer db.Close()

	if err := db.EnableHotReload(50 * time.Millisecond); err != nil {
		t.Fatalf("first EnableHotReload: %v", err)
	}
	defer db.DisableHotReload()

	if err := db.EnableHotReload(50 * time.Millisecond); err == nil {
		t.Error("second EnableHotReload (without disabling first) returned nil error")
	}
	if !db.IsHotReloadEnabled() {
		t.Error("IsHotReloadEnabled() = false while enabled")
	}
}

func TestDisableHotReloadIsIdempotent(t *testing.T) {
	db, err := LoadStr(`@DATA( x = 1 )`)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	// Never enabled — must still be a safe no-op.
	db.DisableHotReload()
	db.DisableHotReload()
}

// TestHotReloadPicksUpFileChange is the end-to-end test: write a file,
// Load it, enable hot reload with a short poll interval, rewrite the
// file, and confirm OnReloaded fires and the same *Database now serves
// the new value — with no re-fetch on the caller's part.
func TestHotReloadPicksUpFileChange(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "config.mdix")
	if err := os.WriteFile(path, []byte(`@DATA( value = 1 )`), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	db, err := Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	defer db.Close()

	reloaded := make(chan struct{}, 1)
	var failErr error
	db.OnReloaded(func(*Database) {
		select {
		case reloaded <- struct{}{}:
		default:
		}
	})
	db.OnReloadFailed(func(err error) { failErr = err })

	const interval = 30 * time.Millisecond
	if err := db.EnableHotReload(interval); err != nil {
		t.Fatalf("EnableHotReload: %v", err)
	}
	defer db.DisableHotReload()

	// Give the poll goroutine one full interval to record the initial
	// mtime before we change the file, so the change is unambiguous.
	time.Sleep(interval)

	// mtime resolution on some filesystems is coarse (~1s on older
	// ext4/HFS+ setups) — sleep past a full second boundary before
	// rewriting so the new mtime is guaranteed distinguishable, rather
	// than risking a flaky test tied to sub-second precision.
	time.Sleep(1100 * time.Millisecond)
	if err := os.WriteFile(path, []byte(`@DATA( value = 2 )`), 0o644); err != nil {
		t.Fatalf("rewrite WriteFile: %v", err)
	}

	select {
	case <-reloaded:
		// good
	case <-time.After(5 * time.Second):
		t.Fatal("OnReloaded did not fire within 5s of the file changing")
	}

	if failErr != nil {
		t.Errorf("OnReloadFailed fired unexpectedly: %v", failErr)
	}

	got, err := db.GetInt("value")
	if err != nil {
		t.Fatalf("GetInt(value) after reload: %v", err)
	}
	if got != 2 {
		t.Errorf("GetInt(value) after reload = %d, want 2 (same *Database, handle should have swapped in place)", got)
	}
}

func TestCloseDisablesHotReloadFirst(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "config.mdix")
	if err := os.WriteFile(path, []byte(`@DATA( x = 1 )`), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	db, err := Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if err := db.EnableHotReload(20 * time.Millisecond); err != nil {
		t.Fatalf("EnableHotReload: %v", err)
	}

	// Close must stop the poll goroutine cleanly, not race with it over
	// the native handle. If this ever deadlocks or panics under `go test
	// -race`, that's exactly the bug this test exists to catch.
	if err := db.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
	if db.IsHotReloadEnabled() {
		t.Error("IsHotReloadEnabled() = true after Close()")
	}
}
