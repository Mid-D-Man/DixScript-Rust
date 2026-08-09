// Package internal contains the raw cgo bindings for the mdix_ffi native library.
// This is internal — do not import outside mdix-go/.
//
// Build requirements:
//  1. Run `cargo build -p mdix-ffi` to generate:
//     - internal/include/mdix_ffi.h   (C header via cbindgen)
//     - internal/lib/<os>-<arch>/     (copy libmdix_ffi.* here)
//  2. CGO_ENABLED=1 (the default for native builds)
package internal

/*
#cgo CFLAGS: -I${SRCDIR}/include

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

func Version() string {
	return C.GoString(C.mdix_version())
}

// ── Load / Free ───────────────────────────────────────────────────────────────

func Load(path string) unsafe.Pointer {
	cs := C.CString(path)
	defer C.free(unsafe.Pointer(cs))
	return unsafe.Pointer(C.mdix_load(cs))
}

func LoadStr(source string) unsafe.Pointer {
	cs := C.CString(source)
	defer C.free(unsafe.Pointer(cs))
	return unsafe.Pointer(C.mdix_load_str(cs))
}

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

func LoadEncryptedPassword(encPath, password string) unsafe.Pointer {
	cEnc := C.CString(encPath)
	defer C.free(unsafe.Pointer(cEnc))
	cPwd := C.CString(password)
	defer C.free(unsafe.Pointer(cPwd))
	return unsafe.Pointer(C.mdix_load_encrypted_password(cEnc, cPwd))
}

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

func Free(handle unsafe.Pointer) {
	C.mdix_free(handle)
}

// ── Validity / metadata ───────────────────────────────────────────────────────

func IsValid(handle unsafe.Pointer) bool {
	return bool(C.mdix_is_valid(handle))
}

func EntryCount(handle unsafe.Pointer) int {
	return int(C.mdix_entry_count(handle))
}

// ── Type inspection ───────────────────────────────────────────────────────────

// GetType returns the raw int32 discriminant for the value at path.
// cbindgen with prefix_with_name=true emits variants as MDIX_TYPE_NULL,
// MDIX_TYPE_INT etc. We cast the return to int32 so the Go layer uses its own
// dixscript.ValueType constants — no dependency on the C enum names here.
func GetType(handle unsafe.Pointer, path string) int32 {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return int32(C.mdix_get_type(handle, cPath))
}

func GetArrayLength(handle unsafe.Pointer, path string) int {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return int(C.mdix_get_array_length(handle, cPath))
}

// ── Typed getters ─────────────────────────────────────────────────────────────

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

// GetLong reads a 64-bit integer at path (mdix_get_long). Also succeeds
// against a value actually stored as Int — mdix_get_long widens it
// losslessly (see its doc comment in mdix-ffi/src/lib.rs) — but the
// reverse isn't true: GetInt does not read an actual Long-stored value,
// since a 64-bit value that overflows i32 has no lossless narrowing.
// Previously Database.GetInt64 called GetInt and simply widened its i32
// result to int64, meaning it could never actually read a Long value
// bigger than i32's range (e.g. 9_000_000_000L, from the DixScript
// writing skill's own numeric-literal example) — it would always fail
// via mdix_get_int on paths like that. Fixed by adding this and pointing
// GetInt64 at it directly.
func GetLong(handle unsafe.Pointer, path string) (int64, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := int64(C.mdix_get_long(handle, cPath))
	if HasError() {
		return 0, false
	}
	return val, true
}

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

func Exists(handle unsafe.Pointer, path string) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_exists(handle, cPath))
}

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
	ptrs := (*[1 << 28]*C.char)(unsafe.Pointer(arr))[:n:n]
	for i, p := range ptrs {
		result[i] = C.GoString(p)
	}
	return result
}

// ── Error handling ────────────────────────────────────────────────────────────

func LastError() string {
	p := C.mdix_get_last_error()
	if p == nil {
		return ""
	}
	return C.GoString(p)
}

func ClearError() {
	C.mdix_clear_error()
}

func HasError() bool {
	return C.mdix_get_last_error() != nil
}

// ── Conversion — export ───────────────────────────────────────────────────────

func ToJSON(handle unsafe.Pointer, indented bool) (string, bool) {
	result := C.mdix_to_json(handle, C.bool(indented))
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

// ToMdix: cbindgen with prefix_with_name emits MdixFormatMode variants as
// MDIX_FORMAT_MODE_DEFAULT etc. We cast our int32 to the C type directly —
// no variant name needed in Go.
func ToMdix(handle unsafe.Pointer, mode int32) (string, bool) {
	result := C.mdix_to_mdix(handle, C.MdixFormatMode(mode))
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string((*C.char)(result))
	return C.GoString((*C.char)(result)), true
}

func ToToml(handle unsafe.Pointer) (string, bool) {
	result := C.mdix_to_toml(handle)
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

func FromJSON(source string) unsafe.Pointer {
	cs := C.CString(source)
	defer C.free(unsafe.Pointer(cs))
	return unsafe.Pointer(C.mdix_from_json(cs))
}

func FromToml(source string) unsafe.Pointer {
	cs := C.CString(source)
	defer C.free(unsafe.Pointer(cs))
	return unsafe.Pointer(C.mdix_from_toml(cs))
}

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

func BuilderNew() unsafe.Pointer {
	return unsafe.Pointer(C.mdix_builder_new())
}

func BuilderFree(handle unsafe.Pointer) {
	C.mdix_builder_free(handle)
}

func BuilderEntryCount(handle unsafe.Pointer) int {
	return int(C.mdix_builder_entry_count(handle))
}

func BuilderClear(handle unsafe.Pointer) bool {
	return bool(C.mdix_builder_clear(handle))
}

func BuilderSetString(handle unsafe.Pointer, path, value string) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	cVal := C.CString(value)
	defer C.free(unsafe.Pointer(cVal))
	return bool(C.mdix_builder_set_string(handle, cPath, cVal))
}

func BuilderSetInt(handle unsafe.Pointer, path string, value int32) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_set_int(handle, cPath, C.int(value)))
}

// BuilderSetLong sets a genuine 64-bit Long value (mdix_builder_set_long),
// distinct from BuilderSetInt's 32-bit mdix_builder_set_int. This binding
// didn't exist at all previously — mdix_builder_set_long is in the C ABI
// (mdix_ffi.h) but nothing on the Go side called it, so Builder had no
// way to construct a genuine Long field, only Int.
func BuilderSetLong(handle unsafe.Pointer, path string, value int64) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_set_long(handle, cPath, C.int64_t(value)))
}

func BuilderSetFloat(handle unsafe.Pointer, path string, value float32) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_set_float(handle, cPath, C.float(value)))
}

func BuilderSetDouble(handle unsafe.Pointer, path string, value float64) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_set_double(handle, cPath, C.double(value)))
}

func BuilderSetBool(handle unsafe.Pointer, path string, value bool) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_set_bool(handle, cPath, C.bool(value)))
}

func BuilderRemove(handle unsafe.Pointer, path string) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_remove(handle, cPath))
}

func BuilderHasKey(handle unsafe.Pointer, path string) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_has_key(handle, cPath))
}

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

func BuilderGetInt(handle unsafe.Pointer, path string) (int32, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := int32(C.mdix_builder_get_int(handle, cPath))
	return val, !HasError()
}

// BuilderGetLong reads back a genuine 64-bit Long value (mdix_builder_get_long).
// Same rationale as GetLong above — previously absent, so a Long set via
// the new BuilderSetLong couldn't be read back through this package at all.
func BuilderGetLong(handle unsafe.Pointer, path string) (int64, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := int64(C.mdix_builder_get_long(handle, cPath))
	return val, !HasError()
}

func BuilderGetFloat(handle unsafe.Pointer, path string) (float32, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := float32(C.mdix_builder_get_float(handle, cPath))
	return val, !HasError()
}

func BuilderGetDouble(handle unsafe.Pointer, path string) (float64, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := float64(C.mdix_builder_get_double(handle, cPath))
	return val, !HasError()
}

func BuilderGetBool(handle unsafe.Pointer, path string) (bool, bool) {
	ClearError()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	val := bool(C.mdix_builder_get_bool(handle, cPath))
	return val, !HasError()
}

func BuilderSave(handle unsafe.Pointer, path string) bool {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	return bool(C.mdix_builder_save(handle, cPath))
}

func BuilderToString(handle unsafe.Pointer) (string, bool) {
	result := C.mdix_builder_to_string(handle)
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

// ── Query ─────────────────────────────────────────────────────────────────────

// SelectManyAsJSON matches a single '*' wildcard segment (e.g.
// "servers.*.status") against every sibling path and returns every match
// as a JSON array string. Wraps mdix_select_many_as_json — see its doc
// comment in mdix-ffi/src/lib.rs for the exact wildcard semantics (single
// segment only).
func SelectManyAsJSON(handle unsafe.Pointer, pattern string) (string, bool) {
	cPattern := C.CString(pattern)
	defer C.free(unsafe.Pointer(cPattern))
	result := C.mdix_select_many_as_json(handle, cPattern)
	if result == nil {
		return "", false
	}
	defer C.mdix_free_string(result)
	return C.GoString(result), true
}

// ── Merge ─────────────────────────────────────────────────────────────────────

// MergeSources merges .mdix source strings with the real AST-level merger
// (mdix_merge_sources) — full DixScript type fidelity, unlike a JSON
// round-trip. strategy/arrayStrategy are the raw int32 discriminants of
// MdixMergeStrategy / ArrayMergeStrategy (dixscript.MergeStrategy /
// dixscript.ArrayMergeStrategy — cast at the call site so this package
// stays free of a dixscript import). Returns the new handle, the
// conflicts-JSON report ("[]" when there were none), and whether the
// merge succeeded; check LastError() on failure.
func MergeSources(sources []string, strategy, arrayStrategy int32) (unsafe.Pointer, string, bool) {
	if len(sources) == 0 {
		return nil, "", false
	}
	cSources := make([]*C.char, len(sources))
	for i, s := range sources {
		cSources[i] = C.CString(s)
	}
	defer func() {
		for _, cs := range cSources {
			C.free(unsafe.Pointer(cs))
		}
	}()

	var outConflicts *C.char
	h := C.mdix_merge_sources(
		(**C.char)(unsafe.Pointer(&cSources[0])),
		C.int(len(sources)),
		C.MdixMergeStrategy(strategy),
		C.ArrayMergeStrategy(arrayStrategy),
		&outConflicts,
	)

	conflicts := ""
	if outConflicts != nil {
		conflicts = C.GoString(outConflicts)
		C.mdix_free_string(outConflicts)
	}
	return unsafe.Pointer(h), conflicts, h != nil
}

// MergeSourcesWeighted is MergeSources with explicit per-source weights
// (mdix_merge_sources_weighted). weights must be the same length as
// sources; higher weight wins under MdixMergeStrategy::WeightedPriority.
func MergeSourcesWeighted(sources []string, weights []float64, strategy, arrayStrategy int32) (unsafe.Pointer, string, bool) {
	if len(sources) == 0 || len(sources) != len(weights) {
		return nil, "", false
	}
	cSources := make([]*C.char, len(sources))
	for i, s := range sources {
		cSources[i] = C.CString(s)
	}
	defer func() {
		for _, cs := range cSources {
			C.free(unsafe.Pointer(cs))
		}
	}()

	var outConflicts *C.char
	h := C.mdix_merge_sources_weighted(
		(**C.char)(unsafe.Pointer(&cSources[0])),
		(*C.double)(unsafe.Pointer(&weights[0])),
		C.int(len(sources)),
		C.MdixMergeStrategy(strategy),
		C.ArrayMergeStrategy(arrayStrategy),
		&outConflicts,
	)

	conflicts := ""
	if outConflicts != nil {
		conflicts = C.GoString(outConflicts)
		C.mdix_free_string(outConflicts)
	}
	return unsafe.Pointer(h), conflicts, h != nil
}
