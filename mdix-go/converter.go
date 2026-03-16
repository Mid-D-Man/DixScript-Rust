package dixscript

import "github.com/Mid-D-Man/dixscript-go/internal"

// Converter holds format-conversion methods backed by the native DixConverter.
// Obtain one via dixscript.Convert (the package-level singleton).
//
// All methods take or return a *Database — the caller is responsible for closing
// any *Database returned by From* methods.
type Converter struct{}

// Convert is the package-level Converter singleton.
var Convert = Converter{}

// ── Export ────────────────────────────────────────────────────────────────────

// ToJSON exports all entries in db as a JSON string.
// Pass indented=true for pretty-printed output.
func (Converter) ToJSON(db *Database, indented bool) (string, error) {
	if db == nil {
		return "", errNullHandle()
	}
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return "", err
	}
	val, ok := internal.ToJSON(db.handle, indented)
	if !ok {
		return "", errNative(internal.LastError())
	}
	return val, nil
}

// ToMdix re-serializes db back to .mdix text format.
func (Converter) ToMdix(db *Database, mode FormatMode) (string, error) {
	if db == nil {
		return "", errNullHandle()
	}
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return "", err
	}
	val, ok := internal.ToMdix(db.handle, int32(mode))
	if !ok {
		return "", errNative(internal.LastError())
	}
	return val, nil
}

// ToToml exports all entries in db as a TOML string.
func (Converter) ToToml(db *Database) (string, error) {
	if db == nil {
		return "", errNullHandle()
	}
	db.mu.RLock()
	defer db.mu.RUnlock()
	if err := db.checkOpen(); err != nil {
		return "", err
	}
	val, ok := internal.ToToml(db.handle)
	if !ok {
		return "", errNative(internal.LastError())
	}
	return val, nil
}

// ── Import ────────────────────────────────────────────────────────────────────

// FromJSON parses a JSON object string into a new Database.
// The caller must close the returned Database.
func (Converter) FromJSON(json string) (*Database, error) {
	if json == "" {
		return nil, errParse("JSON string is empty")
	}
	h := internal.FromJSON(json)
	if h == nil {
		return nil, errParse(internal.LastError())
	}
	return &Database{handle: h}, nil
}

// FromToml parses a TOML table string into a new Database.
// The caller must close the returned Database.
func (Converter) FromToml(toml string) (*Database, error) {
	if toml == "" {
		return nil, errParse("TOML string is empty")
	}
	h := internal.FromToml(toml)
	if h == nil {
		return nil, errParse(internal.LastError())
	}
	return &Database{handle: h}, nil
}

// ── Source text formatting ────────────────────────────────────────────────────

// FormatSource formats raw .mdix source text according to mode.
func (Converter) FormatSource(source string, mode FormatMode) (string, error) {
	if source == "" {
		return "", errNative("source is empty")
	}
	val, ok := internal.FormatSource(source, int32(mode))
	if !ok {
		return "", errNative(internal.LastError())
	}
	return val, nil
}

// MinifySource removes all unnecessary whitespace and comments from raw .mdix source.
// String literal contents are preserved.
func (Converter) MinifySource(source string) (string, error) {
	if source == "" {
		return "", errNative("source is empty")
	}
	val, ok := internal.MinifySource(source)
	if !ok {
		return "", errNative(internal.LastError())
	}
	return val, nil
}

// ── Round-trip helpers ────────────────────────────────────────────────────────

// JSONRoundTrip exports db to JSON and immediately loads it back.
// Useful for testing and for stripping DixScript-specific metadata.
func (c Converter) JSONRoundTrip(db *Database) (*Database, error) {
	json, err := c.ToJSON(db, false)
	if err != nil {
		return nil, err
	}
	return c.FromJSON(json)
}
