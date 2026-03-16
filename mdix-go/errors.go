// Package dixscript provides Go bindings for the DixScript (.mdix) runtime.
// Load, read, build, and convert .mdix config files from Go via cgo.
package dixscript

import "fmt"

// ErrorKind classifies the category of a DixScript error.
type ErrorKind int

const (
	// ErrNotFound is returned when a dotted path does not exist in the data.
	ErrNotFound ErrorKind = iota
	// ErrTypeMismatch is returned when the value cannot be converted to the requested type.
	ErrTypeMismatch
	// ErrNullHandle is returned when the native handle is nil or has been freed.
	ErrNullHandle
	// ErrInvalidPath is returned when the path argument is empty or nil.
	ErrInvalidPath
	// ErrNative is returned when the native Rust layer reports an error.
	ErrNative
	// ErrIO is returned when a file system operation fails.
	ErrIO
	// ErrParse is returned when the source cannot be parsed as valid DixScript.
	ErrParse
	// ErrClosed is returned when a Database or Builder has already been closed.
	ErrClosed
)

func (k ErrorKind) String() string {
	switch k {
	case ErrNotFound:
		return "NotFound"
	case ErrTypeMismatch:
		return "TypeMismatch"
	case ErrNullHandle:
		return "NullHandle"
	case ErrInvalidPath:
		return "InvalidPath"
	case ErrNative:
		return "NativeError"
	case ErrIO:
		return "IOError"
	case ErrParse:
		return "ParseError"
	case ErrClosed:
		return "Closed"
	default:
		return "Unknown"
	}
}

// MdixError is the error type returned by all DixScript operations.
// It implements the standard error interface.
type MdixError struct {
	Kind    ErrorKind
	Message string
	Path    string // dotted path that triggered the error, may be empty
}

func (e *MdixError) Error() string {
	if e.Path != "" {
		return fmt.Sprintf("[%s] %s (path: %q)", e.Kind, e.Message, e.Path)
	}
	return fmt.Sprintf("[%s] %s", e.Kind, e.Message)
}

// ── Error constructors ────────────────────────────────────────────────────────

func errNotFound(path string) *MdixError {
	return &MdixError{Kind: ErrNotFound, Message: fmt.Sprintf("path not found: %q", path), Path: path}
}

func errTypeMismatch(path, expected, actual string) *MdixError {
	return &MdixError{
		Kind:    ErrTypeMismatch,
		Message: fmt.Sprintf("expected %s, got %s", expected, actual),
		Path:    path,
	}
}

func errNullHandle() *MdixError {
	return &MdixError{Kind: ErrNullHandle, Message: "native handle is nil or has been freed"}
}

func errClosed(typ string) *MdixError {
	return &MdixError{Kind: ErrClosed, Message: fmt.Sprintf("%s has been closed", typ)}
}

func errNative(msg string) *MdixError {
	return &MdixError{Kind: ErrNative, Message: msg}
}

func errIO(msg string) *MdixError {
	return &MdixError{Kind: ErrIO, Message: msg}
}

func errParse(msg string) *MdixError {
	return &MdixError{Kind: ErrParse, Message: msg}
}
