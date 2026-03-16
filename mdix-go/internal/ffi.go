// Package internal contains the raw cgo bindings for the mdix_ffi native library.
// This package is internal — it must not be imported outside mdix-go/.
//
// Build requirements:
//   1. Run `cargo build -p mdix-ffi` to generate:
//      - internal/include/mdix_ffi.h   (C header via cbindgen)
//      - internal/lib/<os>-<arch>/     (copy libmdix_ffi.* here from target/release/)
//   2. CGO_ENABLED=1 (the default for native builds)
//
// Platform lib layout expected under this package:
//   lib/linux-amd64/libmdix_ffi.so
//   lib/linux-arm64/libmdix_ffi.so
//   lib/darwin-amd64/libmdix_ffi.dylib
//   lib/darwin-arm64/libmdix_ffi.dylib
//   lib/windows-amd64/mdix_ffi.dll
package internal

/*
// ── CGO preamble ─────────────────────────────────────────────────────────────
// Include the generated C header from cbindgen.
#cgo CFLAGS: -I${SRCDIR}/include

// ── Platform-specific linker flags ───────────────────────────────────────────
// ${SRCDIR} resolves to the directory of THIS file at cgo processing time.

#cgo linux,amd64  LDFLAGS: -L${SRCDIR}/lib/linux-amd64  -lmdix_ffi -Wl,-rpath,${SRCDIR}/lib/linux-amd64
#cgo linux,arm64  LDFLAGS: -L${SRCDIR}/lib/linux-arm64  -lmdix_ffi -Wl,-rpath,${SRCDIR}/lib/linux-arm64
#cgo darwin,amd64 LDFLAGS: -L${SRCDIR}/lib/darwin-amd64 -lmdix_ffi
#cgo darwin,arm64 LDFLAGS: -L${SRCDIR}/lib/darwin-arm64 -lmdix_ffi
#cgo windows,amd64 LDFLAGS: -L${SRCDIR}/lib/windows-amd64 -lmdix_ffi

#include <stdlib.h>
#include "mdix_ffi.h"
*/
import "C"
import "unsafe"

// ── Version ───────────────────────────────────────────────────────────────────

// Version returns the DixScript library version string.
// The returned pointer is static — do NOT free it.
func Version() string {
	return C.GoString(C.mdix_version())
}

// ── Load / Free ───────────────────────────────────────────────────────────────

// Load loads a .mdix file from disk. Returns nil on failure; call LastError().
func Load(path string) unsafe.Pointer {
	cs := C.CString(path)
	defer C.free(unsafe.Pointer(cs))
	return unsafe.Pointer(C.mdix_load(cs))
}

// LoadStr loads .mdix content from a source string.
func LoadStr(source string) unsafe.Pointer {
	cs := C.CString(source)
	defer C.free(unsafe.Pointer(cs))
	return unsafe.Pointer(C.mdix_load_str(cs))
}

// LoadEncrypted loads an encrypted .mdix.enc file with an optional key file path.
// Pass empty string for keyPath to auto-detect next to the enc file.
func LoadEncrypted(encPath, keyPath string) unsafe.Pointer {
	cEnc := C.CString(encPath)
	defer C.free(unsafe.Pointer(cEnc))
	if keyPath == "" {
		return unsafe.Pointer(C.mdix_load_encrypted(cEnc, nil))
	}
	cKey := C.CString(keyPath)
	defer C.free(unsafe.Pointer(cKey))
	return unsafe.Pointer(C.mdix_load_encrypted(cEnc, cKey))
}

// LoadEncryptedPassword loads an encrypted .mdix.enc file using a password.
func LoadEncryptedPassword(encPath, password string) unsafe.Pointer {
	cEnc := C.CString(encPath)
	defer C.free(unsafe.Pointer(cEnc))
	cPwd := C.CString(password)
	defer C.free(unsafe.Pointer(cPwd))
	return unsafe.Pointer(C.mdix_load_encrypted_password(cEnc, cPwd))
}

// LoadEncryptedBytes loads encrypted data from a byte slice with the key file content as string.
func LoadEncryptedBytes(data []byte, keyContent, password string) unsafe.Pointer {
	if len(data) == 0 {
		return nil
	}
	cKey := C.CString(keyContent)
	defer C.free(unsafe.Pointer(cKey))

	if password == "" {
		return unsafe.Pointer(C.mdix_load_encrypted_bytes(
			(*C.uint8_t)(unsafe.Pointer(&data[0])),
			C.int(len(data)),
			cKey,
			nil,
		))
	}
	cPwd := C.CString(password)
	defer C.free(unsafe.Pointer(cPwd))
	return unsafe.Pointer(C.mdix_load_encrypted_bytes(
		(*C.uint8_t)(unsafe.Pointer(&data[0])),
		C.int(len(data)),
		cKey,
		cPwd,
	))
}

// Free releases the native handle. Safe to call with nil.
func Free(handle unsafe.Pointer) {
	C.mdix_free(handle)
}

// ── Validity / metadata ───────────────────────────────────────────────────────

// IsValid returns true if the handle is non-nil.
func IsValid(handle unsafe.Pointer) bool {
	return bool(C.mdix_is_valid(handle))
}

// EntryCount returns the number of entries in the loaded file, or -1 on null.
func EntryCount(handle unsafe.Pointer) int {
	return int(C.mdix_entry_count(handle))
}

// ── Type inspection ───────────────────────────────────────────────────────────

// GetType returns the raw integer type discriminant for the value at path.
// Maps to dixscript.ValueType constants.
func GetType(handle unsafe.Pointer, path string) int32 {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return int32(C.mdix_get_type(handle, cPath))
}

// GetArrayLength returns the length of the array at path, or -1 if not array.
func GetArrayLength(handle unsafe.Pointer, path string) int {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return int(C.mdix_get_array_length(handle, cPath))
}

// ── Typed getters ─────────────────────────────────────────────────────────────

// GetString returns the string at path and whether it was found.
func GetString(handle unsafe.Pointer, path string) (string, bool) {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	result := C.mdix_get_string(handle, cPath)
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

// GetInt returns the int32 at path and whether it was found.
func GetInt(handle unsafe.Pointer, path string) (int32, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := int32(C.mdix_get_int(handle, cPath))
	if HasError() {
		return 0, false
	}
	return val, true
}

// GetFloat returns the float32 at path and whether it was found.
func GetFloat(handle unsafe.Pointer, path string) (float32, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := float32(C.mdix_get_float(handle, cPath))
	if HasError() {
		return 0, false
	}
	return val, true
}

// GetDouble returns the float64 at path and whether it was found.
func GetDouble(handle unsafe.Pointer, path string) (float64, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := float64(C.mdix_get_double(handle, cPath))
	if HasError() {
		return 0, false
	}
	return val, true
}

// GetBool returns the bool at path and whether it was found.
func GetBool(handle unsafe.Pointer, path string) (bool, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := bool(C.mdix_get_bool(handle, cPath))
	if HasError() {
		return false, false
	}
	return val, true
}

// GetEnumName returns the enum type name (e.g. "AIType") at path.
func GetEnumName(handle unsafe.Pointer, path string) (string, bool) {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	result := C.mdix_get_enum_name(handle, cPath)
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

// GetEnumField returns the enum field name (e.g. "BOSS") at path.
func GetEnumField(handle unsafe.Pointer, path string) (string, bool) {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	result := C.mdix_get_enum_field(handle, cPath)
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

// GetJSON returns the value at path serialized as JSON.
func GetJSON(handle unsafe.Pointer, path string) (string, bool) {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	result := C.mdix_get_json(handle, cPath)
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

// ── Key existence / enumeration ───────────────────────────────────────────────

// Exists returns true if the dotted path exists in the loaded data.
func Exists(handle unsafe.Pointer, path string) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_exists(handle, cPath))
}

// GetKeys returns direct child key names under prefix.
// Pass empty string for top-level keys.
func GetKeys(handle unsafe.Pointer, prefix string) []string {
	cPrefix := C.CString(prefix)
	defer C.free(unsafe.Pointer(cPrefix))

	var count C.int
	arr := C.mdix_get_keys(handle, cPrefix, &count)
	if arr == nil || count <= 0 {
		return nil
	}
	defer C.mdix_free_string_array(arr, count)

	n := int(count)
	result := make([]string, n)
	// arr is a **char — index using unsafe pointer arithmetic
	ptrs := (*[1 << 28]*C.char)(unsafe.Pointer(arr))[:n:n]
	for i, p := range ptrs {
		result[i] = C.GoString(p)
	}
	return result
}

// ── Error handling ────────────────────────────────────────────────────────────

// LastError returns the last native error message, or empty string if none.
func LastError() string {
	p := C.mdix_get_last_error()
	if p == nil {
		return ""
	}
	return C.GoString(p)
}

// ClearError clears the thread-local error slot.
func ClearError() {
	C.mdix_clear_error()
}

// HasError returns true if there is a pending native error.
func HasError() bool {
	return C.mdix_get_last_error() != nil
}

// ── Conversion — export ───────────────────────────────────────────────────────

// ToJSON exports the database as a JSON string.
func ToJSON(handle unsafe.Pointer, indented bool) (string, bool) {
	result := C.mdix_to_json(handle, C.bool(indented))
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

// ToMdix re-serializes the database to .mdix text.
func ToMdix(handle unsafe.Pointer, mode int32) (string, bool) {
	result := C.mdix_to_mdix(handle, C.MdixFormatMode(mode))
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string((*C.char)(result))
	return C.GoString((*C.char)(result)), true
}

// ToToml exports the database as a TOML string.
func ToToml(handle unsafe.Pointer) (string, bool) {
	result := C.mdix_to_toml(handle)
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

// FromJSON loads a JSON object string and returns a handle.
func FromJSON(source string) unsafe.Pointer {
	cs := C.CString(source)
	defer C.free(unsafe.Pointer(cs))
	return unsafe.Pointer(C.mdix_from_json(cs))
}

// FromToml loads a TOML table string and returns a handle.
func FromToml(source string) unsafe.Pointer {
	cs := C.CString(source)
	defer C.free(unsafe.Pointer(cs))
	return unsafe.Pointer(C.mdix_from_toml(cs))
}

// FormatSource formats raw .mdix source text.
func FormatSource(source string, mode int32) (string, bool) {
	cs := C.CString(source)
	defer C.free(unsafe.Pointer(cs))
	result := C.mdix_format_source(cs, C.MdixFormatMode(mode))
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

// MinifySource removes all unnecessary whitespace and comments from .mdix source.
func MinifySource(source string) (string, bool) {
	cs := C.CString(source)
	defer C.free(unsafe.Pointer(cs))
	result := C.mdix_minify_source(cs)
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

// ── Builder ───────────────────────────────────────────────────────────────────

// BuilderNew creates a new builder handle.
func BuilderNew() unsafe.Pointer {
	return unsafe.Pointer(C.mdix_builder_new())
}

// BuilderFree frees a builder handle.
func BuilderFree(handle unsafe.Pointer) {
	C.mdix_builder_free(handle)
}

// BuilderEntryCount returns the number of entries in the builder.
func BuilderEntryCount(handle unsafe.Pointer) int {
	return int(C.mdix_builder_entry_count(handle))
}

// BuilderClear removes all entries from the builder.
func BuilderClear(handle unsafe.Pointer) bool {
	return bool(C.mdix_builder_clear(handle))
}

// BuilderSetString sets a string value in the builder.
func BuilderSetString(handle unsafe.Pointer, path, value string) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	cVal := C.CString(value)
	defer C.free(unsafe.Pointer(cVal))
	return bool(C.mdix_builder_set_string(handle, cPath, cVal))
}

// BuilderSetInt sets an int32 value in the builder.
func BuilderSetInt(handle unsafe.Pointer, path string, value int32) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_set_int(handle, cPath, C.int(value)))
}

// BuilderSetFloat sets a float32 value in the builder.
func BuilderSetFloat(handle unsafe.Pointer, path string, value float32) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_set_float(handle, cPath, C.float(value)))
}

// BuilderSetDouble sets a float64 value in the builder.
func BuilderSetDouble(handle unsafe.Pointer, path string, value float64) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_set_double(handle, cPath, C.double(value)))
}

// BuilderSetBool sets a bool value in the builder.
func BuilderSetBool(handle unsafe.Pointer, path string, value bool) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_set_bool(handle, cPath, C.bool(value)))
}

// BuilderRemove removes a key from the builder.
func BuilderRemove(handle unsafe.Pointer, path string) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_remove(handle, cPath))
}

// BuilderHasKey returns true if the key exists in the builder.
func BuilderHasKey(handle unsafe.Pointer, path string) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_has_key(handle, cPath))
}

// BuilderGetString reads a string back from the builder.
func BuilderGetString(handle unsafe.Pointer, path string) (string, bool) {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	result := C.mdix_builder_get_string(handle, cPath)
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

// BuilderGetInt reads an int32 back from the builder.
func BuilderGetInt(handle unsafe.Pointer, path string) (int32, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := int32(C.mdix_builder_get_int(handle, cPath))
	return val, !HasError()
}

// BuilderGetFloat reads a float32 back from the builder.
func BuilderGetFloat(handle unsafe.Pointer, path string) (float32, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := float32(C.mdix_builder_get_float(handle, cPath))
	return val, !HasError()
}

// BuilderGetDouble reads a float64 back from the builder.
func BuilderGetDouble(handle unsafe.Pointer, path string) (float64, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := float64(C.mdix_builder_get_double(handle, cPath))
	return val, !HasError()
}

// BuilderGetBool reads a bool back from the builder.
func BuilderGetBool(handle unsafe.Pointer, path string) (bool, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := bool(C.mdix_builder_get_bool(handle, cPath))
	return val, !HasError()
}

// BuilderSave saves the builder contents to a .mdix file on disk.
func BuilderSave(handle unsafe.Pointer, path string) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_save(handle, cPath))
}

// BuilderToString serializes the builder contents to a .mdix string.
func BuilderToString(handle unsafe.Pointer) (string, bool) {
	result := C.mdix_builder_to_string(handle)
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}
