package mdix

// mdix.odin — idiomatic Odin wrapper over mdix_ffi (DixScript runtime).
//
// Quick start:
//
//   import "mdix"
//
//   db, ok := mdix.load_str(`@DATA( port = 8080, host = "localhost" )`)
//   if !ok {
//       fmt.println("load failed:", mdix.last_error())
//       return
//   }
//   defer mdix.destroy(&db)
//
//   port, _ := mdix.get_int(db, "port")
//   host, _ := mdix.get_string(db, "host")
//   defer delete(host)
//
// All getters follow Odin's (value, ok) convention instead of the C API's
// null-sentinel + mdix_get_last_error() pattern. On ok == false, call
// last_error() for the reason.
//
// String results are heap-allocated Odin strings (cloned out of the C
// buffer, which is freed immediately) using `context.allocator` unless an
// explicit allocator is passed — callers own and must `delete()` them.
// Path/value arguments passed *in* are converted via `context.temp_allocator`;
// if you call into this package outside Odin's normal per-frame temp-alloc
// reset (e.g. a long-running loop with no surrounding temp scope), call
// `free_all(context.temp_allocator)` periodically yourself.

import "core:c"
import "core:strings"
import ffi "../mdix_ffi"

// ── Errors ───────────────────────────────────────────────────────────────

last_error :: proc() -> string {
	e := ffi.mdix_get_last_error()
	if e == nil {
		return ""
	}
	return string(e)
}

clear_error :: proc() {
	ffi.mdix_clear_error()
}

version :: proc() -> string {
	return string(ffi.mdix_version())
}

// ── Database ─────────────────────────────────────────────────────────────

// Mdix_Type is get_type's result type — re-exported from mdix_ffi so
// callers can write mdix.Mdix_Type.Long instead of reaching into the ffi
// package directly for it (schema.odin and the tests below both need to
// name it standalone, not just at a call site where it'd be inferred).
Mdix_Type :: ffi.Mdix_Type

Database :: struct {
	handle: rawptr,
}

is_valid :: proc(db: Database) -> bool {
	return db.handle != nil && ffi.mdix_is_valid(db.handle)
}

load :: proc(path: string) -> (db: Database, ok: bool) {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	h := ffi.mdix_load(cpath)
	if h == nil {
		return {}, false
	}
	return Database{handle = h}, true
}

load_str :: proc(source: string) -> (db: Database, ok: bool) {
	csrc := strings.clone_to_cstring(source, context.temp_allocator)
	h := ffi.mdix_load_str(csrc)
	if h == nil {
		return {}, false
	}
	return Database{handle = h}, true
}

// key_path == "" auto-detects the .mdix.key file next to enc_path.
load_encrypted :: proc(enc_path: string, key_path: string = "") -> (db: Database, ok: bool) {
	cenc := strings.clone_to_cstring(enc_path, context.temp_allocator)
	ckey: cstring = nil
	if key_path != "" {
		ckey = strings.clone_to_cstring(key_path, context.temp_allocator)
	}
	h := ffi.mdix_load_encrypted(cenc, ckey)
	if h == nil {
		return {}, false
	}
	return Database{handle = h}, true
}

load_encrypted_password :: proc(enc_path: string, password: string) -> (db: Database, ok: bool) {
	cenc := strings.clone_to_cstring(enc_path, context.temp_allocator)
	cpw := strings.clone_to_cstring(password, context.temp_allocator)
	h := ffi.mdix_load_encrypted_password(cenc, cpw)
	if h == nil {
		return {}, false
	}
	return Database{handle = h}, true
}

// password == "" for key-file-only mode (no password layer).
load_encrypted_bytes :: proc(
	encrypted_bytes: []u8,
	key_file_content: string,
	password: string = "",
) -> (db: Database, ok: bool) {
	if len(encrypted_bytes) == 0 {
		return {}, false
	}
	ckey := strings.clone_to_cstring(key_file_content, context.temp_allocator)
	cpw: cstring = nil
	if password != "" {
		cpw = strings.clone_to_cstring(password, context.temp_allocator)
	}
	h := ffi.mdix_load_encrypted_bytes(
		raw_data(encrypted_bytes),
		c.int32_t(len(encrypted_bytes)),
		ckey,
		cpw,
	)
	if h == nil {
		return {}, false
	}
	return Database{handle = h}, true
}

from_json :: proc(source: string) -> (db: Database, ok: bool) {
	csrc := strings.clone_to_cstring(source, context.temp_allocator)
	h := ffi.mdix_from_json(csrc)
	if h == nil {
		return {}, false
	}
	return Database{handle = h}, true
}

from_toml :: proc(source: string) -> (db: Database, ok: bool) {
	csrc := strings.clone_to_cstring(source, context.temp_allocator)
	h := ffi.mdix_from_toml(csrc)
	if h == nil {
		return {}, false
	}
	return Database{handle = h}, true
}

destroy :: proc(db: ^Database) {
	if db.handle != nil {
		ffi.mdix_free(db.handle)
		db.handle = nil
	}
}

entry_count :: proc(db: Database) -> int {
	return int(ffi.mdix_entry_count(db.handle))
}

is_encrypted :: proc(db: Database) -> bool {
	return ffi.mdix_is_encrypted(db.handle)
}

is_compressed :: proc(db: Database) -> bool {
	return ffi.mdix_is_compressed(db.handle)
}

loaded_version :: proc(db: Database, allocator := context.allocator) -> (string, bool) {
	cs := ffi.mdix_get_loaded_version(db.handle)
	if cs == nil {
		return "", false
	}
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator), true
}

config_value :: proc(db: Database, key: string, allocator := context.allocator) -> (string, bool) {
	ckey := strings.clone_to_cstring(key, context.temp_allocator)
	cs := ffi.mdix_get_config_value(db.handle, ckey)
	if cs == nil {
		return "", false
	}
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator), true
}

// Validates .mdix source through the full compile pipeline without
// constructing a handle. Check last_error() on false.
validate :: proc(source: string) -> bool {
	csrc := strings.clone_to_cstring(source, context.temp_allocator)
	return ffi.mdix_validate(csrc)
}

// ── Type inspection ─────────────────────────────────────────────────────

get_type :: proc(db: Database, path: string) -> ffi.Mdix_Type {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_get_type(db.handle, cpath)
}

array_length :: proc(db: Database, path: string) -> int {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return int(ffi.mdix_get_array_length(db.handle, cpath))
}

exists :: proc(db: Database, path: string) -> bool {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_exists(db.handle, cpath)
}

// ── Typed getters ────────────────────────────────────────────────────────
// 0 / "" / false on failure is ambiguous with a real zero value — check `ok`.

get_string :: proc(db: Database, path: string, allocator := context.allocator) -> (string, bool) {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	cs := ffi.mdix_get_string(db.handle, cpath)
	if cs == nil {
		return "", false
	}
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator), true
}

get_int :: proc(db: Database, path: string) -> (int, bool) {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	ffi.mdix_clear_error()
	v := ffi.mdix_get_int(db.handle, cpath)
	if ffi.mdix_get_last_error() != nil {
		return 0, false
	}
	return int(v), true
}

get_long :: proc(db: Database, path: string) -> (i64, bool) {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	ffi.mdix_clear_error()
	v := ffi.mdix_get_long(db.handle, cpath)
	if ffi.mdix_get_last_error() != nil {
		return 0, false
	}
	return i64(v), true
}

get_float :: proc(db: Database, path: string) -> (f32, bool) {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	ffi.mdix_clear_error()
	v := ffi.mdix_get_float(db.handle, cpath)
	if ffi.mdix_get_last_error() != nil {
		return 0, false
	}
	return v, true
}

get_double :: proc(db: Database, path: string) -> (f64, bool) {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	ffi.mdix_clear_error()
	v := ffi.mdix_get_double(db.handle, cpath)
	if ffi.mdix_get_last_error() != nil {
		return 0, false
	}
	return v, true
}

get_bool :: proc(db: Database, path: string) -> (bool, bool) {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	ffi.mdix_clear_error()
	v := ffi.mdix_get_bool(db.handle, cpath)
	if ffi.mdix_get_last_error() != nil {
		return false, false
	}
	return v, true
}

get_enum_name :: proc(db: Database, path: string, allocator := context.allocator) -> (string, bool) {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	cs := ffi.mdix_get_enum_name(db.handle, cpath)
	if cs == nil {
		return "", false
	}
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator), true
}

get_enum_field :: proc(db: Database, path: string, allocator := context.allocator) -> (string, bool) {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	cs := ffi.mdix_get_enum_field(db.handle, cpath)
	if cs == nil {
		return "", false
	}
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator), true
}

// Serializes the raw value at path to a JSON string — useful for Object
// or Array values you want to hand off wholesale.
get_json :: proc(db: Database, path: string, allocator := context.allocator) -> (string, bool) {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	cs := ffi.mdix_get_json(db.handle, cpath)
	if cs == nil {
		return "", false
	}
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator), true
}

// Wildcard selection (e.g. "server.*") returned as a JSON array string.
select_many_as_json :: proc(db: Database, pattern: string, allocator := context.allocator) -> (string, bool) {
	cpattern := strings.clone_to_cstring(pattern, context.temp_allocator)
	cs := ffi.mdix_select_many_as_json(db.handle, cpattern)
	if cs == nil {
		return "", false
	}
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator), true
}

// ── Keys ─────────────────────────────────────────────────────────────────

// Direct child keys under prefix ("" for top-level).
get_keys :: proc(db: Database, prefix: string = "", allocator := context.allocator) -> []string {
	cprefix := strings.clone_to_cstring(prefix, context.temp_allocator)
	count: c.int32_t
	arr := ffi.mdix_get_keys(db.handle, cprefix, &count)
	if arr == nil || count == 0 {
		return nil
	}
	defer ffi.mdix_free_string_array(arr, count)

	result := make([]string, int(count), allocator)
	for i in 0 ..< int(count) {
		result[i] = strings.clone(string(arr[i]), allocator)
	}
	return result
}

// Every key in the flat data map, including synthetic indexed children
// (tags[0], server.host, ...).
get_all_keys :: proc(db: Database, allocator := context.allocator) -> []string {
	count: c.int32_t
	arr := ffi.mdix_get_all_keys(db.handle, &count)
	if arr == nil || count == 0 {
		return nil
	}
	defer ffi.mdix_free_string_array(arr, count)

	result := make([]string, int(count), allocator)
	for i in 0 ..< int(count) {
		result[i] = strings.clone(string(arr[i]), allocator)
	}
	return result
}

// ── Export ───────────────────────────────────────────────────────────────

to_json :: proc(db: Database, indented := true, allocator := context.allocator) -> (string, bool) {
	cs := ffi.mdix_to_json(db.handle, indented)
	if cs == nil {
		return "", false
	}
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator), true
}

to_toml :: proc(db: Database, allocator := context.allocator) -> (string, bool) {
	cs := ffi.mdix_to_toml(db.handle)
	if cs == nil {
		return "", false
	}
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator), true
}

to_mdix :: proc(
	db: Database,
	mode := ffi.Mdix_Format_Mode.Default,
	allocator := context.allocator,
) -> (string, bool) {
	cs := ffi.mdix_to_mdix(db.handle, mode)
	if cs == nil {
		return "", false
	}
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator), true
}

// ── Source text utilities (no Database required) ──────────────────────────

format_source :: proc(
	source: string,
	mode := ffi.Mdix_Format_Mode.Default,
	allocator := context.allocator,
) -> string {
	csrc := strings.clone_to_cstring(source, context.temp_allocator)
	cs := ffi.mdix_format_source(csrc, mode)
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator)
}

minify_source :: proc(source: string, allocator := context.allocator) -> string {
	csrc := strings.clone_to_cstring(source, context.temp_allocator)
	cs := ffi.mdix_minify_source(csrc)
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator)
}

compact_source :: proc(source: string, allocator := context.allocator) -> string {
	csrc := strings.clone_to_cstring(source, context.temp_allocator)
	cs := ffi.mdix_compact_source(csrc)
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator)
}

strip_comments :: proc(source: string, allocator := context.allocator) -> string {
	csrc := strings.clone_to_cstring(source, context.temp_allocator)
	cs := ffi.mdix_strip_comments(csrc)
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator)
}

// ── Builder ──────────────────────────────────────────────────────────────

Builder :: struct {
	handle: rawptr,
}

builder_new :: proc() -> Builder {
	return Builder{handle = ffi.mdix_builder_new()}
}

// Forks a builder pre-populated from db's root-level structural values
// (synthetic indexed children like tags[0] are stripped). The original
// db remains valid and independent.
builder_from_database :: proc(db: Database) -> Builder {
	return Builder{handle = ffi.mdix_builder_from_handle(db.handle)}
}

builder_destroy :: proc(b: ^Builder) {
	if b.handle != nil {
		ffi.mdix_builder_free(b.handle)
		b.handle = nil
	}
}

builder_entry_count :: proc(b: Builder) -> int {
	return int(ffi.mdix_builder_entry_count(b.handle))
}

builder_clear :: proc(b: Builder) -> bool {
	return ffi.mdix_builder_clear(b.handle)
}

builder_set_string :: proc(b: Builder, path: string, value: string) -> bool {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	cval := strings.clone_to_cstring(value, context.temp_allocator)
	return ffi.mdix_builder_set_string(b.handle, cpath, cval)
}

builder_set_int :: proc(b: Builder, path: string, value: int) -> bool {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_builder_set_int(b.handle, cpath, c.int32_t(value))
}

builder_set_long :: proc(b: Builder, path: string, value: i64) -> bool {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_builder_set_long(b.handle, cpath, c.int64_t(value))
}

builder_set_float :: proc(b: Builder, path: string, value: f32) -> bool {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_builder_set_float(b.handle, cpath, value)
}

builder_set_double :: proc(b: Builder, path: string, value: f64) -> bool {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_builder_set_double(b.handle, cpath, value)
}

builder_set_bool :: proc(b: Builder, path: string, value: bool) -> bool {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_builder_set_bool(b.handle, cpath, value)
}

builder_remove :: proc(b: Builder, path: string) -> bool {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_builder_remove(b.handle, cpath)
}

builder_has_key :: proc(b: Builder, path: string) -> bool {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_builder_has_key(b.handle, cpath)
}

builder_get_string :: proc(b: Builder, path: string, allocator := context.allocator) -> (string, bool) {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	cs := ffi.mdix_builder_get_string(b.handle, cpath)
	if cs == nil {
		return "", false
	}
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator), true
}

builder_get_int :: proc(b: Builder, path: string) -> (int, bool) {
	if !builder_has_key(b, path) {
		return 0, false
	}
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return int(ffi.mdix_builder_get_int(b.handle, cpath)), true
}

builder_get_long :: proc(b: Builder, path: string) -> (i64, bool) {
	if !builder_has_key(b, path) {
		return 0, false
	}
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return i64(ffi.mdix_builder_get_long(b.handle, cpath)), true
}

builder_get_float :: proc(b: Builder, path: string) -> (f32, bool) {
	if !builder_has_key(b, path) {
		return 0, false
	}
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_builder_get_float(b.handle, cpath), true
}

builder_get_double :: proc(b: Builder, path: string) -> (f64, bool) {
	if !builder_has_key(b, path) {
		return 0, false
	}
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_builder_get_double(b.handle, cpath), true
}

builder_get_bool :: proc(b: Builder, path: string) -> (bool, bool) {
	if !builder_has_key(b, path) {
		return false, false
	}
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_builder_get_bool(b.handle, cpath), true
}

builder_to_string :: proc(b: Builder, allocator := context.allocator) -> (string, bool) {
	cs := ffi.mdix_builder_to_string(b.handle)
	if cs == nil {
		return "", false
	}
	defer ffi.mdix_free_string(cs)
	return strings.clone(string(cs), allocator), true
}

builder_save :: proc(b: Builder, path: string) -> bool {
	cpath := strings.clone_to_cstring(path, context.temp_allocator)
	return ffi.mdix_builder_save(b.handle, cpath)
}

// Serializes the builder and immediately reloads it as a read-only
// Database — useful right after building runtime save data.
builder_to_database :: proc(b: Builder) -> (Database, bool) {
	src, ok := builder_to_string(b, context.temp_allocator)
	if !ok {
		return {}, false
	}
	return load_str(src)
}
