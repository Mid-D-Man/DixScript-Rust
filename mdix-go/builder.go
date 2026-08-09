package dixscript

import (
	"sync"
	"time"
	"unsafe"

	"github.com/Mid-D-Man/dixscript-go/internal"
)

// Builder is a mutable key-value store for constructing .mdix files at runtime.
// Always call Close() when done.
//
//	b := dixscript.NewBuilder()
//	defer b.Close()
//
//	b.SetString("app.name", "MyGame")
//	b.SetInt("app.version", 1)
//	b.SetBool("server.ssl", true)
//
//	if err := b.SaveToFile("config.mdix"); err != nil { /* handle */ }
type Builder struct {
	handle unsafe.Pointer
	mu     sync.Mutex
	closed bool
}

// NewBuilder creates a new empty Builder.
func NewBuilder() *Builder {
	return &Builder{handle: internal.BuilderNew()}
}

// Close releases the native memory. Safe to call multiple times.
func (b *Builder) Close() error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if !b.closed {
		internal.BuilderFree(b.handle)
		b.handle = nil
		b.closed = true
	}
	return nil
}

func (b *Builder) checkOpen() error {
	if b.closed || b.handle == nil {
		return errClosed("Builder")
	}
	return nil
}

// EntryCount returns the number of entries currently in the builder.
func (b *Builder) EntryCount() int {
	b.mu.Lock()
	defer b.mu.Unlock()
	return internal.BuilderEntryCount(b.handle)
}

// Clear removes all entries without freeing the builder.
func (b *Builder) Clear() error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return err
	}
	if !internal.BuilderClear(b.handle) {
		return errNative(internal.LastError())
	}
	return nil
}

// ── Write ─────────────────────────────────────────────────────────────────────

// SetString sets a string value at the dotted path.
func (b *Builder) SetString(path, value string) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return err
	}
	if !internal.BuilderSetString(b.handle, path, value) {
		return errNative(internal.LastError())
	}
	return nil
}

// SetInt sets an int value at the dotted path.
func (b *Builder) SetInt(path string, value int) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return err
	}
	if !internal.BuilderSetInt(b.handle, path, int32(value)) {
		return errNative(internal.LastError())
	}
	return nil
}

// SetInt32 sets an int32 value at the dotted path.
func (b *Builder) SetInt32(path string, value int32) error {
	return b.SetInt(path, int(value))
}

// SetInt64 sets a genuine 64-bit Long value at the dotted path — uses
// the real 64-bit FFI setter (mdix_builder_set_long), not SetInt widened.
// Previously there was no way to build a Long field at all through this
// package; only Int (32-bit) was reachable.
func (b *Builder) SetInt64(path string, value int64) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return err
	}
	if !internal.BuilderSetLong(b.handle, path, value) {
		return errNative(internal.LastError())
	}
	return nil
}

// SetFloat32 sets a float32 value at the dotted path.
func (b *Builder) SetFloat32(path string, value float32) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return err
	}
	if !internal.BuilderSetFloat(b.handle, path, value) {
		return errNative(internal.LastError())
	}
	return nil
}

// SetFloat64 sets a float64 value at the dotted path.
func (b *Builder) SetFloat64(path string, value float64) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return err
	}
	if !internal.BuilderSetDouble(b.handle, path, value) {
		return errNative(internal.LastError())
	}
	return nil
}

// SetBool sets a bool value at the dotted path.
func (b *Builder) SetBool(path string, value bool) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return err
	}
	if !internal.BuilderSetBool(b.handle, path, value) {
		return errNative(internal.LastError())
	}
	return nil
}

// SetDate stores a time.Time as a YYYY-MM-DD date string.
func (b *Builder) SetDate(path string, value time.Time) error {
	return b.SetString(path, value.UTC().Format("2006-01-02"))
}

// SetTimestamp stores a time.Time as an ISO 8601 timestamp string.
func (b *Builder) SetTimestamp(path string, value time.Time) error {
	return b.SetString(path, value.UTC().Format(time.RFC3339Nano))
}

// Remove deletes a key from the builder. Returns true if the key existed.
func (b *Builder) Remove(path string) (bool, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return false, err
	}
	return internal.BuilderRemove(b.handle, path), nil
}

// ── Read back ──────────────────────────────────────────────────────────────────

// HasKey reports whether a key currently exists in the builder.
func (b *Builder) HasKey(path string) bool {
	b.mu.Lock()
	defer b.mu.Unlock()
	if b.closed {
		return false
	}
	return internal.BuilderHasKey(b.handle, path)
}

// GetString reads a string back from the builder.
func (b *Builder) GetString(path string) (string, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return "", err
	}
	val, ok := internal.BuilderGetString(b.handle, path)
	if !ok {
		return "", nativeOrNotFound(path)
	}
	return val, nil
}

// GetInt reads an int back from the builder.
func (b *Builder) GetInt(path string) (int, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return 0, err
	}
	val, ok := internal.BuilderGetInt(b.handle, path)
	if !ok {
		return 0, nativeOrNotFound(path)
	}
	return int(val), nil
}

// GetInt64 reads a genuine 64-bit Long value back from the builder —
// uses mdix_builder_get_long, not GetInt widened. Previously absent
// entirely, so a value set via SetInt64 had no matching getter.
func (b *Builder) GetInt64(path string) (int64, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return 0, err
	}
	val, ok := internal.BuilderGetLong(b.handle, path)
	if !ok {
		return 0, nativeOrNotFound(path)
	}
	return val, nil
}

// GetFloat32 reads a float32 back from the builder.
func (b *Builder) GetFloat32(path string) (float32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return 0, err
	}
	val, ok := internal.BuilderGetFloat(b.handle, path)
	if !ok {
		return 0, nativeOrNotFound(path)
	}
	return val, nil
}

// GetFloat64 reads a float64 back from the builder.
func (b *Builder) GetFloat64(path string) (float64, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return 0, err
	}
	val, ok := internal.BuilderGetDouble(b.handle, path)
	if !ok {
		return 0, nativeOrNotFound(path)
	}
	return val, nil
}

// GetBool reads a bool back from the builder.
func (b *Builder) GetBool(path string) (bool, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return false, err
	}
	val, ok := internal.BuilderGetBool(b.handle, path)
	if !ok {
		return false, nativeOrNotFound(path)
	}
	return val, nil
}

// ── Persistence ───────────────────────────────────────────────────────────────

// SaveToFile saves the builder contents to a .mdix file on disk.
// Intermediate directories are created automatically.
func (b *Builder) SaveToFile(path string) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return err
	}
	if !internal.BuilderSave(b.handle, path) {
		return errIO(internal.LastError())
	}
	return nil
}

// ToString serializes the builder contents to a .mdix format string.
func (b *Builder) ToString() (string, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.checkOpen(); err != nil {
		return "", err
	}
	val, ok := internal.BuilderToString(b.handle)
	if !ok {
		return "", errNative(internal.LastError())
	}
	return val, nil
}

// ToDatabase serializes and immediately loads the builder into a new Database.
// The caller is responsible for closing the returned Database.
func (b *Builder) ToDatabase() (*Database, error) {
	src, err := b.ToString()
	if err != nil {
		return nil, err
	}
	return LoadStr(src)
}
