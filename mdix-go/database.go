package dixscript

import (
	"sync"
	"unsafe"

	"github.com/Mid-D-Man/dixscript-go/internal"
)

// Database is a loaded, read-only DixScript database.
// Always call Close() when done — it releases the native memory.
//
//	db, err := dixscript.LoadStr("@DATA( port = 8080 )")
//	if err != nil { /* handle */ }
//	defer db.Close()
//
//	port, err := db.GetInt("port")
type Database struct {
	handle unsafe.Pointer
	mu     sync.RWMutex
	closed bool

	// sourcePath is the file path this Database was loaded from via
	// Load() — empty for LoadStr/LoadEncrypted*/FromJSON/FromToml/merge
	// results, all of which have no single on-disk file to re-read.
	// Read by watch.go's EnableHotReload; see SourcePath().
	sourcePath string
	hotReload  *hotReloadState // nil until EnableHotReload is called
}

// SourcePath returns the file path this Database was loaded from, and
// whether one exists. Only Load() populates it — LoadStr, the
// LoadEncrypted* family, FromJSON/FromToml, and merge results all return
// false, since none of them have a single on-disk file to point back to.
func (db *Database) SourcePath() (string, bool) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	return db.sourcePath, db.sourcePath != ""
}

// Close releases the native memory for this database. Also disables hot
// reload first, if it was enabled — stopping the poll goroutine before
// freeing the handle it might otherwise try to swap out from under Close.
// Safe to call multiple times. Implements io.Closer.
func (db *Database) Close() error {
	db.DisableHotReload()
	db.mu.Lock()
	defer db.mu.Unlock()
	if !db.closed {
		internal.Free(db.handle)
		db.handle = nil
		db.closed = true
	}
	return nil
}

func (db *Database) checkOpen() error {
	if db.closed || db.handle == nil {
		return errClosed("Database")
	}
	return nil
}

// IsValid returns true if the database is open and the native handle is valid.
func (db *Database) IsValid() bool {
	db.mu.RLock()
	defer db.mu.RUnlock()
	return !db.closed && internal.IsValid(db.handle)
}

// EntryCount returns the total number of data entries.
func (db *Database) EntryCount() (int, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return -1, err
	}
	return internal.EntryCount(db.handle), nil
}

// ── Existence and type ────────────────────────────────────────────────────────

// Exists reports whether a dotted path is present in the data.
func (db *Database) Exists(path string) bool {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if db.closed {
		return false
	}
	return internal.Exists(db.handle, path)
}

// ValueTypeAt returns the ValueType of the entry at path.
func (db *Database) ValueTypeAt(path string) ValueType {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if db.closed {
		return TypeUnknown
	}
	return ValueType(internal.GetType(db.handle, path))
}

// ── Typed getters ─────────────────────────────────────────────────────────────

// GetString returns the string value at path.
func (db *Database) GetString(path string) (string, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return "", err
	}
	val, ok := internal.GetString(db.handle, path)
	if !ok {
		return "", nativeOrNotFound(path)
	}
	return val, nil
}

// GetInt returns the int value at path as int.
func (db *Database) GetInt(path string) (int, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return 0, err
	}
	val, ok := internal.GetInt(db.handle, path)
	if !ok {
		return 0, nativeOrNotFound(path)
	}
	return int(val), nil
}

// GetInt32 returns the int32 value at path.
func (db *Database) GetInt32(path string) (int32, error) {
	v, err := db.GetInt(path)
	return int32(v), err
}

// GetInt64 returns the int64 value at path.
func (db *Database) GetInt64(path string) (int64, error) {
	v, err := db.GetInt(path)
	return int64(v), err
}

// GetFloat32 returns the float32 value at path.
func (db *Database) GetFloat32(path string) (float32, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return 0, err
	}
	val, ok := internal.GetFloat(db.handle, path)
	if !ok {
		return 0, nativeOrNotFound(path)
	}
	return val, nil
}

// GetFloat64 returns the float64 value at path.
func (db *Database) GetFloat64(path string) (float64, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return 0, err
	}
	val, ok := internal.GetDouble(db.handle, path)
	if !ok {
		return 0, nativeOrNotFound(path)
	}
	return val, nil
}

// GetBool returns the bool value at path.
func (db *Database) GetBool(path string) (bool, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return false, err
	}
	val, ok := internal.GetBool(db.handle, path)
	if !ok {
		return false, nativeOrNotFound(path)
	}
	return val, nil
}

// ── Special types ─────────────────────────────────────────────────────────────

// GetHexColor returns the HexColor value at path.
func (db *Database) GetHexColor(path string) (HexColor, error) {
	raw, err := db.GetString(path)
	if err != nil {
		return HexColor{}, err
	}
	return ParseHexColor(raw)
}

// GetBlob returns the Blob value at path.
func (db *Database) GetBlob(path string) (Blob, error) {
	raw, err := db.GetString(path)
	if err != nil {
		return Blob{}, err
	}
	return Blob{RawBase64: raw}, nil
}

// GetRegex returns the MdixRegex value at path.
func (db *Database) GetRegex(path string) (MdixRegex, error) {
	raw, err := db.GetString(path)
	if err != nil {
		return MdixRegex{}, err
	}
	return MdixRegex{Pattern: raw}, nil
}

// GetDate returns the MdixDate value at path.
func (db *Database) GetDate(path string) (MdixDate, error) {
	raw, err := db.GetString(path)
	if err != nil {
		return MdixDate{}, err
	}
	return ParseMdixDate(raw)
}

// GetTimestamp returns the MdixTimestamp value at path.
func (db *Database) GetTimestamp(path string) (MdixTimestamp, error) {
	raw, err := db.GetString(path)
	if err != nil {
		return MdixTimestamp{}, err
	}
	return ParseMdixTimestamp(raw)
}

// ── Enum ──────────────────────────────────────────────────────────────────────

// GetEnumName returns the enum type name at path (e.g. "AIType").
func (db *Database) GetEnumName(path string) (string, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return "", err
	}
	val, ok := internal.GetEnumName(db.handle, path)
	if !ok {
		return "", nativeOrNotFound(path)
	}
	return val, nil
}

// GetEnumField returns the enum field name at path (e.g. "BOSS").
func (db *Database) GetEnumField(path string) (string, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return "", err
	}
	val, ok := internal.GetEnumField(db.handle, path)
	if !ok {
		return "", nativeOrNotFound(path)
	}
	return val, nil
}

// GetEnumValue returns the resolved integer value of the enum at path.
func (db *Database) GetEnumValue(path string) (int, error) {
	return db.GetInt(path)
}

// ── JSON escape hatch ─────────────────────────────────────────────────────────

// GetJSON serializes the value at path to a JSON string.
// Useful for Blob, Regex, Tuple, and nested structures.
func (db *Database) GetJSON(path string) (string, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return "", err
	}
	val, ok := internal.GetJSON(db.handle, path)
	if !ok {
		return "", nativeOrNotFound(path)
	}
	return val, nil
}

// ── Array ─────────────────────────────────────────────────────────────────────

// ArrayLength returns the number of items in the array at path.
func (db *Database) ArrayLength(path string) (int, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return -1, err
	}
	n := internal.GetArrayLength(db.handle, path)
	if n < 0 {
		return -1, errTypeMismatch(path, "array", db.ValueTypeAt(path).String())
	}
	return n, nil
}

// ── Key enumeration ───────────────────────────────────────────────────────────

// Keys returns the direct child key names under prefix.
// Pass empty string for top-level keys.
func (db *Database) Keys(prefix string) ([]string, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return nil, err
	}
	return internal.GetKeys(db.handle, prefix), nil
}

// ── Internal ──────────────────────────────────────────────────────────────────

// RawHandle exposes the native pointer for use by Converter (package-internal only).
func (db *Database) rawHandle() unsafe.Pointer { return db.handle }

func nativeOrNotFound(path string) error {
	if msg := internal.LastError(); msg != "" {
		return errNative(msg)
	}
	return errNotFound(path)
}
