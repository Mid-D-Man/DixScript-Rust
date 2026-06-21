package mdix_ffi

// mdix_ffi.odin — raw Odin bindings to the DixScript C FFI layer.
//
// This file mirrors mdix-ffi/src/lib.rs symbol-for-symbol — every
// `#[no_mangle] pub extern "C" fn` in that crate has a matching foreign
// proc declaration here. Hand-maintained, not generated; re-check against
// mdix-ffi/src/lib.rs after any FFI surface change, the same way
// mdix-c/include/mdix.h is hand-maintained relative to the crate.
//
// Link against the platform build of mdix_ffi (same artifact the C/C++
// and Go wrappers use):
//   Linux:   libmdix_ffi.so
//   macOS:   libmdix_ffi.dylib
//   Windows: mdix_ffi.dll (import lib mdix_ffi.lib)
//
// Ownership rules (identical to the C API):
//   - Every cstring returned by a mdix_get_*/mdix_to_*/mdix_format_*/
//     mdix_builder_get_string/mdix_builder_to_string call must be freed
//     with mdix_free_string().
//   - Every rawptr handle (Database or Builder) must be freed with
//     mdix_free() or mdix_builder_free() respectively.
//   - Passing nil for a handle or path is safe — the FFI layer returns a
//     sentinel value and records a message retrievable via
//     mdix_get_last_error().

import "core:c"

when ODIN_OS == .Windows {
	foreign import mdix_ffi_lib "system:mdix_ffi.lib"
} else {
	foreign import mdix_ffi_lib "system:mdix_ffi"
}

// ── Type discriminants ──────────────────────────────────────────────────

// Mirrors the Rust `MdixType` repr(i32) enum exactly.
Mdix_Type :: enum c.int {
	Unknown   = -1,
	Null      = 0,
	Bool      = 1,
	Int       = 2,
	Long      = 3,
	Float     = 4,
	Double    = 5,
	String    = 6,
	Date      = 7,
	Timestamp = 8,
	Hex_Color = 9,
	Blob      = 10,
	Regex     = 11,
	Array     = 12,
	Object    = 13,
	Tuple     = 14,
	Enum      = 15,
}

// Mirrors the Rust `MdixFormatMode` repr(i32) enum exactly.
Mdix_Format_Mode :: enum c.int {
	Default  = 0,
	Pretty   = 1,
	Compact  = 2,
	Minified = 3,
}

@(default_calling_convention = "c")
foreign mdix_ffi_lib {
	// ── Metadata ─────────────────────────────────────────────────────────
	// Static pointer — never pass the result to mdix_free_string().
	mdix_version :: proc() -> cstring ---

	// ── Handle lifecycle — plain .mdix ─────────────────────────────────────
	mdix_load     :: proc(path: cstring) -> rawptr ---
	mdix_load_str :: proc(source: cstring) -> rawptr ---
	mdix_free     :: proc(handle: rawptr) ---

	// ── Handle lifecycle — encrypted .mdix.enc ──────────────────────────────
	mdix_load_encrypted :: proc(enc_path: cstring, key_path: cstring) -> rawptr ---
	mdix_load_encrypted_password :: proc(enc_path: cstring, password: cstring) -> rawptr ---
	mdix_load_encrypted_bytes :: proc(
		encrypted_bytes:  [^]u8,
		byte_count:       c.int32_t,
		key_file_content: cstring,
		password:         cstring,
	) -> rawptr ---

	// ── Validity and metadata ───────────────────────────────────────────────
	mdix_is_valid           :: proc(handle: rawptr) -> bool ---
	mdix_entry_count        :: proc(handle: rawptr) -> c.int32_t ---
	mdix_is_encrypted       :: proc(handle: rawptr) -> bool ---
	mdix_is_compressed      :: proc(handle: rawptr) -> bool ---
	mdix_get_loaded_version :: proc(handle: rawptr) -> cstring ---
	mdix_get_all_keys       :: proc(handle: rawptr, out_count: ^c.int32_t) -> [^]cstring ---
	mdix_get_config_value   :: proc(handle: rawptr, key: cstring) -> cstring ---

	// ── Validation ───────────────────────────────────────────────────────────
	mdix_validate :: proc(source: cstring) -> bool ---

	// ── Type inspection ─────────────────────────────────────────────────────
	mdix_get_type         :: proc(handle: rawptr, path: cstring) -> Mdix_Type ---
	mdix_get_array_length :: proc(handle: rawptr, path: cstring) -> c.int32_t ---

	// ── Typed getters ────────────────────────────────────────────────────────
	mdix_get_string      :: proc(handle: rawptr, path: cstring) -> cstring ---
	mdix_get_int         :: proc(handle: rawptr, path: cstring) -> c.int32_t ---
	mdix_get_long         :: proc(handle: rawptr, path: cstring) -> c.int64_t ---
	mdix_get_float        :: proc(handle: rawptr, path: cstring) -> f32 ---
	mdix_get_double       :: proc(handle: rawptr, path: cstring) -> f64 ---
	mdix_get_bool         :: proc(handle: rawptr, path: cstring) -> bool ---
	mdix_get_enum_name    :: proc(handle: rawptr, path: cstring) -> cstring ---
	mdix_get_enum_field   :: proc(handle: rawptr, path: cstring) -> cstring ---
	mdix_get_json         :: proc(handle: rawptr, path: cstring) -> cstring ---
	mdix_select_many_as_json :: proc(handle: rawptr, pattern: cstring) -> cstring ---

	// ── Existence and enumeration ───────────────────────────────────────────
	mdix_exists   :: proc(handle: rawptr, path: cstring) -> bool ---
	mdix_get_keys :: proc(handle: rawptr, prefix: cstring, out_count: ^c.int32_t) -> [^]cstring ---

	// ── Memory management ───────────────────────────────────────────────────
	mdix_free_string       :: proc(s: cstring) ---
	mdix_free_string_array :: proc(arr: [^]cstring, count: c.int32_t) ---

	// ── Error reporting ─────────────────────────────────────────────────────
	// Valid only until the next mdix_* call on this thread. Do not free.
	mdix_get_last_error :: proc() -> cstring ---
	mdix_clear_error    :: proc() ---

	// ── Conversion — export ─────────────────────────────────────────────────
	mdix_to_json :: proc(handle: rawptr, indented: bool) -> cstring ---
	mdix_to_toml :: proc(handle: rawptr) -> cstring ---
	mdix_to_mdix :: proc(handle: rawptr, mode: Mdix_Format_Mode) -> cstring ---

	// ── Conversion — source text formatting ─────────────────────────────────
	mdix_format_source  :: proc(source: cstring, mode: Mdix_Format_Mode) -> cstring ---
	mdix_minify_source  :: proc(source: cstring) -> cstring ---
	mdix_compact_source :: proc(source: cstring) -> cstring ---
	mdix_strip_comments :: proc(source: cstring) -> cstring ---

	// ── Conversion — foreign format import ───────────────────────────────────
	mdix_from_json :: proc(source: cstring) -> rawptr ---
	mdix_from_toml :: proc(source: cstring) -> rawptr ---

	// ── Builder — lifecycle ──────────────────────────────────────────────────
	mdix_builder_new         :: proc() -> rawptr ---
	mdix_builder_from_handle :: proc(handle: rawptr) -> rawptr ---
	mdix_builder_free        :: proc(builder: rawptr) ---
	mdix_builder_entry_count :: proc(builder: rawptr) -> c.int32_t ---
	mdix_builder_clear       :: proc(builder: rawptr) -> bool ---

	// ── Builder — write ──────────────────────────────────────────────────────
	mdix_builder_set_string :: proc(builder: rawptr, path: cstring, value: cstring) -> bool ---
	mdix_builder_set_int    :: proc(builder: rawptr, path: cstring, value: c.int32_t) -> bool ---
	mdix_builder_set_long   :: proc(builder: rawptr, path: cstring, value: c.int64_t) -> bool ---
	mdix_builder_set_float  :: proc(builder: rawptr, path: cstring, value: f32) -> bool ---
	mdix_builder_set_double :: proc(builder: rawptr, path: cstring, value: f64) -> bool ---
	mdix_builder_set_bool   :: proc(builder: rawptr, path: cstring, value: bool) -> bool ---
	mdix_builder_remove     :: proc(builder: rawptr, path: cstring) -> bool ---

	// ── Builder — read back ───────────────────────────────────────────────────
	mdix_builder_has_key    :: proc(builder: rawptr, path: cstring) -> bool ---
	mdix_builder_get_string :: proc(builder: rawptr, path: cstring) -> cstring ---
	mdix_builder_get_int    :: proc(builder: rawptr, path: cstring) -> c.int32_t ---
	mdix_builder_get_long   :: proc(builder: rawptr, path: cstring) -> c.int64_t ---
	mdix_builder_get_float  :: proc(builder: rawptr, path: cstring) -> f32 ---
	mdix_builder_get_double :: proc(builder: rawptr, path: cstring) -> f64 ---
	mdix_builder_get_bool   :: proc(builder: rawptr, path: cstring) -> bool ---

	// ── Builder — persistence ─────────────────────────────────────────────────
	mdix_builder_to_string :: proc(builder: rawptr) -> cstring ---
	mdix_builder_save      :: proc(builder: rawptr, path: cstring) -> bool ---
}
