package dixscript

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
)

const basicFixture = `
@DATA(
  app_name = "TestApp"
  version  = 2
  big_id   = 9_000_000_000L
  pi       = 3.14159
  debug    = false
  server: host = "localhost", port = 8080, ssl = true
  tags:: "go", "config", "fast"
)`

func TestLoadStrAndGetters(t *testing.T) {
	db, err := LoadStr(basicFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	if !db.IsValid() {
		t.Error("IsValid() = false on a freshly loaded database")
	}

	name, err := db.GetString("app_name")
	if err != nil || name != "TestApp" {
		t.Errorf("GetString(app_name) = %q, %v; want \"TestApp\", nil", name, err)
	}

	version, err := db.GetInt("version")
	if err != nil || version != 2 {
		t.Errorf("GetInt(version) = %d, %v; want 2, nil", version, err)
	}

	pi, err := db.GetFloat64("pi")
	if err != nil || pi != 3.14159 {
		t.Errorf("GetFloat64(pi) = %v, %v; want 3.14159, nil", pi, err)
	}

	debug, err := db.GetBool("debug")
	if err != nil || debug != false {
		t.Errorf("GetBool(debug) = %v, %v; want false, nil", debug, err)
	}

	host, err := db.GetString("server.host")
	if err != nil || host != "localhost" {
		t.Errorf("GetString(server.host) = %q, %v; want \"localhost\", nil", host, err)
	}

	port, err := db.GetInt("server.port")
	if err != nil || port != 8080 {
		t.Errorf("GetInt(server.port) = %d, %v; want 8080, nil", port, err)
	}
}

// TestGetInt64ReadsGenuineLong is the regression test for the bug found
// alongside the ValueType fix: Database.GetInt64 previously called GetInt
// (mdix_get_int, i32-based) and widened the result, so it could never
// actually read a Long value outside i32's range — it always uses
// mdix_get_long now. 9_000_000_000 overflows int32 (max ~2.1 billion) by
// a wide margin, so this fails loudly under the old implementation
// instead of silently truncating.
func TestGetInt64ReadsGenuineLong(t *testing.T) {
	db, err := LoadStr(basicFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	got, err := db.GetInt64("big_id")
	if err != nil {
		t.Fatalf("GetInt64(big_id): %v", err)
	}
	const want = 9_000_000_000
	if got != want {
		t.Errorf("GetInt64(big_id) = %d, want %d", got, want)
	}
}

func TestExistsAndArrayLength(t *testing.T) {
	db, err := LoadStr(basicFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	if !db.Exists("app_name") {
		t.Error("Exists(app_name) = false, want true")
	}
	if db.Exists("does.not.exist") {
		t.Error("Exists(does.not.exist) = true, want false")
	}

	n, err := db.ArrayLength("tags")
	if err != nil || n != 3 {
		t.Errorf("ArrayLength(tags) = %d, %v; want 3, nil", n, err)
	}
}

func TestGetMissingPathReturnsNotFound(t *testing.T) {
	db, err := LoadStr(basicFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	_, err = db.GetString("does.not.exist")
	if err == nil {
		t.Fatal("GetString(does.not.exist) returned nil error")
	}
	var mdixErr *MdixError
	if errors.As(err, &mdixErr) && mdixErr.Kind != ErrNotFound && mdixErr.Kind != ErrNative {
		// The native layer may report either, depending on how deep the
		// missing segment is — both are acceptable here; anything else
		// (e.g. TypeMismatch) would be a real bug.
		t.Errorf("GetString(does.not.exist) error kind = %v, want NotFound or NativeError", mdixErr.Kind)
	}
}

func TestCloseIsIdempotentAndInvalidatesReads(t *testing.T) {
	db, err := LoadStr(basicFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}

	if err := db.Close(); err != nil {
		t.Fatalf("first Close(): %v", err)
	}
	if err := db.Close(); err != nil {
		t.Fatalf("second Close() should be a no-op, got: %v", err)
	}

	if db.IsValid() {
		t.Error("IsValid() = true after Close()")
	}
	if _, err := db.GetString("app_name"); err == nil {
		t.Error("GetString after Close() returned nil error, want ErrClosed")
	} else {
		var mdixErr *MdixError
		if errors.As(err, &mdixErr) && mdixErr.Kind != ErrClosed {
			t.Errorf("GetString after Close() error kind = %v, want ErrClosed", mdixErr.Kind)
		}
	}
}

func TestLoadFromFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "config.mdix")
	if err := os.WriteFile(path, []byte(basicFixture), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	db, err := Load(path)
	if err != nil {
		t.Fatalf("Load(%q): %v", path, err)
	}
	defer db.Close()

	got, ok := db.SourcePath()
	if !ok || got != path {
		t.Errorf("SourcePath() = %q, %v; want %q, true", got, ok, path)
	}

	name, err := db.GetString("app_name")
	if err != nil || name != "TestApp" {
		t.Errorf("GetString(app_name) = %q, %v; want \"TestApp\", nil", name, err)
	}
}

func TestLoadStrSourcePathIsEmpty(t *testing.T) {
	db, err := LoadStr(basicFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	if _, ok := db.SourcePath(); ok {
		t.Error("SourcePath() reported a path for a database loaded via LoadStr")
	}
}
