// Package dixscript provides Go bindings for the DixScript (.mdix) runtime.
//
// Quick start:
//
//	db, err := dixscript.LoadStr(`@DATA( port = 8080, host = "localhost" )`)
//	if err != nil {
//	    log.Fatal(err)
//	}
//	defer db.Close()
//
//	port, _ := db.GetInt("port")
//	host, _ := db.GetString("host")
//	fmt.Printf("connecting to %s:%d\n", host, port)
//
// Build requirements:
//  1. CGO_ENABLED=1  (the default for native builds)
//  2. Run `cargo build -p mdix-ffi` to populate:
//     - internal/include/mdix_ffi.h
//     - internal/lib/<GOOS>-<GOARCH>/libmdix_ffi.*
package dixscript

import "github.com/Mid-D-Man/dixscript-go/internal"

// Version returns the DixScript native library version string.
func Version() string {
	return internal.Version()
}

// ── Loading ───────────────────────────────────────────────────────────────────

// Load loads a .mdix file from disk.
// The caller must close the returned Database.
func Load(path string) (*Database, error) {
	if path == "" {
		return nil, errNative("path is empty")
	}
	h := internal.Load(path)
	if h == nil {
		return nil, loadError(path)
	}
	return &Database{handle: h, sourcePath: path}, nil
}

// LoadStr loads .mdix content from a source string.
// The caller must close the returned Database.
func LoadStr(source string) (*Database, error) {
	if source == "" {
		return nil, errParse("source string is empty")
	}
	h := internal.LoadStr(source)
	if h == nil {
		return nil, parseError()
	}
	return &Database{handle: h}, nil
}

// LoadEncrypted loads an encrypted .mdix.enc file using a key file.
// Pass empty keyPath to auto-detect the key file next to the encrypted file.
// The caller must close the returned Database.
func LoadEncrypted(encPath, keyPath string) (*Database, error) {
	if encPath == "" {
		return nil, errNative("encPath is empty")
	}
	h := internal.LoadEncrypted(encPath, keyPath)
	if h == nil {
		return nil, loadError(encPath)
	}
	return &Database{handle: h}, nil
}

// LoadEncryptedPassword loads an encrypted .mdix.enc file using a password.
// The caller must close the returned Database.
func LoadEncryptedPassword(encPath, password string) (*Database, error) {
	if encPath == "" {
		return nil, errNative("encPath is empty")
	}
	if password == "" {
		return nil, errNative("password is empty")
	}
	h := internal.LoadEncryptedPassword(encPath, password)
	if h == nil {
		return nil, loadError(encPath)
	}
	return &Database{handle: h}, nil
}

// LoadEncryptedBytes loads encrypted data from a byte slice.
// keyContent is the full text content of the .mdix.key file.
// Pass empty password when using key-file mode.
// The caller must close the returned Database.
func LoadEncryptedBytes(data []byte, keyContent, password string) (*Database, error) {
	if len(data) == 0 {
		return nil, errNative("encrypted byte slice is empty")
	}
	if keyContent == "" {
		return nil, errNative("keyContent is empty")
	}
	h := internal.LoadEncryptedBytes(data, keyContent, password)
	if h == nil {
		return nil, errNative(internal.LastError())
	}
	return &Database{handle: h}, nil
}

// ── JSON/TOML shortcuts (delegates to Convert) ────────────────────────────────

// LoadJSON parses a JSON object string into a new Database.
// The caller must close the returned Database.
func LoadJSON(json string) (*Database, error) {
	return Convert.FromJSON(json)
}

// LoadToml parses a TOML table string into a new Database.
// The caller must close the returned Database.
func LoadToml(toml string) (*Database, error) {
	return Convert.FromToml(toml)
}

// ── Builder shortcut ──────────────────────────────────────────────────────────

// NewBuilder creates a new empty Builder.
// The caller must close the returned Builder.
func NewBuilderFunc() *Builder {
	return NewBuilder()
}

// ── Private helpers ───────────────────────────────────────────────────────────

func loadError(path string) error {
	if msg := internal.LastError(); msg != "" {
		return errIO(msg)
	}
	return errIO("failed to load: " + path)
}

func parseError() error {
	if msg := internal.LastError(); msg != "" {
		return errParse(msg)
	}
	return errParse("failed to parse source")
}
