// mdix-ffi/src/lib.rs

mod error;
mod handle;
mod string_utils;

use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use std::sync::OnceLock;

use dixscript::Runtime::{
    DixCompactor, DixConverter, DixFormatOptions, DixLoader, DixLoadOptions, DixValue,
};

use error::{clear_last_error, get_last_error_ptr, set_last_error};
use handle::{MdixBuilderHandle, MdixHandle};
use string_utils::{
    c_str_to_str, free_c_char, free_c_char_array, str_to_c_char, string_vec_to_c_array,
};

// =============================================================================
// Type discriminants
// =============================================================================

/// Type discriminants returned by mdix_get_type().
#[repr(i32)]
pub enum MdixType {
    Unknown   = -1,
    Null      =  0,
    Bool      =  1,
    Int       =  2,
    Float     =  3,
    Double    =  4,
    String    =  5,
    Date      =  6,
    Timestamp =  7,
    HexColor  =  8,
    Blob      =  9,
    Regex     = 10,
    Array     = 11,
    Object    = 12,
    Tuple     = 13,
    Enum      = 14,
}

/// Controls output format for mdix_to_mdix() and mdix_format_source().
#[repr(i32)]
pub enum MdixFormatMode {
    Default  = 0,
    Pretty   = 1,
    Compact  = 2,
    Minified = 3,
}

// =============================================================================
// Internal casting helpers
// =============================================================================

#[inline]
unsafe fn as_handle<'a>(ptr: *const c_void) -> Option<&'a MdixHandle> {
    if ptr.is_null() { None } else { Some(&*(ptr as *const MdixHandle)) }
}

#[inline]
unsafe fn as_handle_mut<'a>(ptr: *mut c_void) -> Option<&'a mut MdixHandle> {
    if ptr.is_null() { None } else { Some(&mut *(ptr as *mut MdixHandle)) }
}

#[inline]
unsafe fn as_builder<'a>(ptr: *const c_void) -> Option<&'a MdixBuilderHandle> {
    if ptr.is_null() { None } else { Some(&*(ptr as *const MdixBuilderHandle)) }
}

#[inline]
unsafe fn as_builder_mut<'a>(ptr: *mut c_void) -> Option<&'a mut MdixBuilderHandle> {
    if ptr.is_null() { None } else { Some(&mut *(ptr as *mut MdixBuilderHandle)) }
}

// =============================================================================
// Metadata
// =============================================================================

/// Return the library version as a static null-terminated C string.
/// Do NOT free this pointer.
#[no_mangle]
pub extern "C" fn mdix_version() -> *const c_char {
    static VERSION_PTR: OnceLock<CString> = OnceLock::new();
    VERSION_PTR
        .get_or_init(|| CString::new("1.0.0").expect("version string contained null byte"))
        .as_ptr()
}

// =============================================================================
// Handle lifecycle — plain .mdix files
// =============================================================================

/// Load a plain .mdix file from disk.
/// Returns an opaque handle on success, null on failure.
/// Free with mdix_free() when done.
#[no_mangle]
pub extern "C" fn mdix_load(path: *const c_char) -> *mut c_void {
    clear_last_error();

    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => {
            set_last_error("mdix_load: path is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };

    let loader = DixLoader::new();
    match loader.load_text(path_str, &DixLoadOptions::new()) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e) => {
            set_last_error(&format!("mdix_load: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Load .mdix source from a raw string (no disk access).
/// Useful for TextAssets in Unity. Free handle with mdix_free().
#[no_mangle]
pub extern "C" fn mdix_load_str(source: *const c_char) -> *mut c_void {
    clear_last_error();

    let source_str = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None => {
            set_last_error("mdix_load_str: source is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };

    let loader = DixLoader::new();
    match loader.load_from_str(source_str, &DixLoadOptions::new()) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e) => {
            set_last_error(&format!("mdix_load_str: {}", e));
            std::ptr::null_mut()
        }
    }
}

// =============================================================================
// Handle lifecycle — encrypted .mdix.enc files
// =============================================================================

/// Load an encrypted .mdix.enc file using a key file.
/// Pass null for key_path to auto-detect the .mdix.key next to the .enc file.
/// Free handle with mdix_free().
#[no_mangle]
pub extern "C" fn mdix_load_encrypted(
    enc_path: *const c_char,
    key_path: *const c_char,
) -> *mut c_void {
    clear_last_error();

    let enc_str = match unsafe { c_str_to_str(enc_path) } {
        Some(s) => s,
        None => {
            set_last_error("mdix_load_encrypted: enc_path is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };

    let mut opts = DixLoadOptions::new();
    if let Some(kp) = unsafe { c_str_to_str(key_path) } {
        opts.key_file_path = Some(kp.to_string());
    }

    let loader = DixLoader::new();
    match loader.load_encrypted(enc_str, &opts) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e) => {
            set_last_error(&format!("mdix_load_encrypted: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Load an encrypted .mdix.enc file using a password for decryption.
/// The password must match the one used during compilation.
/// Free handle with mdix_free().
#[no_mangle]
pub extern "C" fn mdix_load_encrypted_password(
    enc_path: *const c_char,
    password: *const c_char,
) -> *mut c_void {
    clear_last_error();

    let enc_str = match unsafe { c_str_to_str(enc_path) } {
        Some(s) => s,
        None => {
            set_last_error("mdix_load_encrypted_password: enc_path is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };

    let pw_str = match unsafe { c_str_to_str(password) } {
        Some(s) => s,
        None => {
            set_last_error("mdix_load_encrypted_password: password is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };

    let opts = DixLoadOptions::with_password(pw_str);

    let loader = DixLoader::new();
    match loader.load_encrypted(enc_str, &opts) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e) => {
            set_last_error(&format!("mdix_load_encrypted_password: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Load encrypted data from raw bytes with key file content as a string.
/// Pass null for password if using key file mode.
/// Free handle with mdix_free().
#[no_mangle]
pub extern "C" fn mdix_load_encrypted_bytes(
    encrypted_bytes: *const u8,
    byte_count: i32,
    key_file_content: *const c_char,
    password: *const c_char,
) -> *mut c_void {
    clear_last_error();

    if encrypted_bytes.is_null() || byte_count <= 0 {
        set_last_error("mdix_load_encrypted_bytes: encrypted_bytes is null or empty");
        return std::ptr::null_mut();
    }

    let key_content = match unsafe { c_str_to_str(key_file_content) } {
        Some(s) => s,
        None => {
            set_last_error(
                "mdix_load_encrypted_bytes: key_file_content is null or invalid UTF-8",
            );
            return std::ptr::null_mut();
        }
    };

    let bytes = unsafe { std::slice::from_raw_parts(encrypted_bytes, byte_count as usize) };

    let mut opts = DixLoadOptions::new();
    if let Some(pw) = unsafe { c_str_to_str(password) } {
        opts.password = Some(pw.to_string());
    }

    let loader = DixLoader::new();
    match loader.load_from_encrypted_bytes(bytes, key_content, &opts) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e) => {
            set_last_error(&format!("mdix_load_encrypted_bytes: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Free a handle returned by any mdix_load* function. Passing null is safe.
#[no_mangle]
pub extern "C" fn mdix_free(handle: *mut c_void) {
    unsafe { MdixHandle::free(handle as *mut MdixHandle) };
}

// =============================================================================
// Validity, metadata, and handle inspection
// =============================================================================

/// Return true if the handle pointer is non-null.
#[no_mangle]
pub extern "C" fn mdix_is_valid(handle: *const c_void) -> bool {
    !handle.is_null()
}

/// Return the total number of data entries in the loaded file. Returns -1 if null.
#[no_mangle]
pub extern "C" fn mdix_entry_count(handle: *const c_void) -> i32 {
    match unsafe { as_handle(handle) } {
        Some(h) => h.data.entry_count() as i32,
        None => -1,
    }
}

/// Return true if the file was encrypted when loaded.
#[no_mangle]
pub extern "C" fn mdix_is_encrypted(handle: *const c_void) -> bool {
    unsafe { as_handle(handle) }.map(|h| h.data.is_encrypted).unwrap_or(false)
}

/// Return true if the file was compressed when loaded.
#[no_mangle]
pub extern "C" fn mdix_is_compressed(handle: *const c_void) -> bool {
    unsafe { as_handle(handle) }.map(|h| h.data.is_compressed).unwrap_or(false)
}

/// Return the DixScript version string from the loaded file.
/// The caller must free the result with mdix_free_string().
/// Returns null if the handle is null.
#[no_mangle]
pub extern "C" fn mdix_get_loaded_version(handle: *const c_void) -> *mut c_char {
    clear_last_error();
    match unsafe { as_handle(handle) } {
        Some(h) => str_to_c_char(h.data.version.clone()),
        None => {
            set_last_error("mdix_get_loaded_version: handle is null");
            std::ptr::null_mut()
        }
    }
}

/// Return all dotted-path keys in the loaded data as a flat array.
///
/// Unlike mdix_get_keys (which returns child keys under a prefix),
/// this returns every key at every depth in the dataset.
///
/// `out_count` receives the number of keys. Must not be null.
/// Free the result with mdix_free_string_array(result, out_count).
#[no_mangle]
pub extern "C" fn mdix_get_all_keys(
    handle: *const c_void,
    out_count: *mut i32,
) -> *mut *mut c_char {
    clear_last_error();

    if out_count.is_null() {
        set_last_error("mdix_get_all_keys: out_count must not be null");
        return std::ptr::null_mut();
    }

    unsafe { *out_count = 0 };

    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None => {
            set_last_error("mdix_get_all_keys: handle is null");
            return std::ptr::null_mut();
        }
    };

    let map  = h.data.to_hashmap();
    let mut keys: Vec<String> = map.into_keys().collect();
    keys.sort_unstable();
    string_vec_to_c_array(keys, out_count)
}

// =============================================================================
// Type inspection
// =============================================================================

/// Return the MdixType discriminant of the value at the given path.
/// Returns MdixType::Unknown (-1) if the path does not exist or handle is null.
#[no_mangle]
pub extern "C" fn mdix_get_type(handle: *const c_void, path: *const c_char) -> MdixType {
    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None => return MdixType::Unknown,
    };

    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => return MdixType::Unknown,
    };

    match h.data.get_value(path_str) {
        None                         => MdixType::Unknown,
        Some(DixValue::Null)         => MdixType::Null,
        Some(DixValue::Bool(_))      => MdixType::Bool,
        Some(DixValue::Int(_))       => MdixType::Int,
        Some(DixValue::Float(_))     => MdixType::Float,
        Some(DixValue::Double(_))    => MdixType::Double,
        Some(DixValue::String(_))    => MdixType::String,
        Some(DixValue::Date(_))      => MdixType::Date,
        Some(DixValue::Timestamp(_)) => MdixType::Timestamp,
        Some(DixValue::HexColor(_))  => MdixType::HexColor,
        Some(DixValue::Blob(_))      => MdixType::Blob,
        Some(DixValue::Regex(_))     => MdixType::Regex,
        Some(DixValue::Array(_))     => MdixType::Array,
        Some(DixValue::Object(_))    => MdixType::Object,
        Some(DixValue::Tuple(_))     => MdixType::Tuple,
        Some(DixValue::Enum { .. })  => MdixType::Enum,
    }
}

/// Return the number of items in the array at the given path.
/// Returns -1 if the path does not exist, is not an array, or handle is null.
#[no_mangle]
pub extern "C" fn mdix_get_array_length(handle: *const c_void, path: *const c_char) -> i32 {
    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None => return -1,
    };

    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => return -1,
    };

    match h.data.get_value(path_str) {
        Some(DixValue::Array(arr)) => arr.len() as i32,
        _ => -1,
    }
}

// =============================================================================
// Data access — typed getters
// =============================================================================

/// Get a string value by dotted path.
/// Also works for Date, Timestamp, and HexColor.
/// The caller must free the result with mdix_free_string(). Returns null on failure.
#[no_mangle]
pub extern "C" fn mdix_get_string(
    handle: *const c_void,
    path: *const c_char,
) -> *mut c_char {
    clear_last_error();

    let (h, path_str) = match validate_read(handle, path, "mdix_get_string") {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };

    match h.data.get::<String>(path_str) {
        Ok(s) => str_to_c_char(s),
        Err(e) => {
            set_last_error(&format!("mdix_get_string('{}'): {}", path_str, e));
            std::ptr::null_mut()
        }
    }
}

/// Get an integer value by dotted path. Also works for Enum (returns resolved integer).
/// Returns 0 on failure. Use mdix_exists() to distinguish 0 from not-found.
#[no_mangle]
pub extern "C" fn mdix_get_int(handle: *const c_void, path: *const c_char) -> i32 {
    clear_last_error();

    let (h, path_str) = match validate_read(handle, path, "mdix_get_int") {
        Some(v) => v,
        None => return 0,
    };

    match h.data.get::<i32>(path_str) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(&format!("mdix_get_int('{}'): {}", path_str, e));
            0
        }
    }
}

/// Get a float (f32) value by dotted path. Returns 0.0 on failure.
#[no_mangle]
pub extern "C" fn mdix_get_float(handle: *const c_void, path: *const c_char) -> f32 {
    clear_last_error();

    let (h, path_str) = match validate_read(handle, path, "mdix_get_float") {
        Some(v) => v,
        None => return 0.0,
    };

    match h.data.get::<f64>(path_str) {
        Ok(v) => v as f32,
        Err(e) => {
            set_last_error(&format!("mdix_get_float('{}'): {}", path_str, e));
            0.0
        }
    }
}

/// Get a double (f64) value by dotted path. Returns 0.0 on failure.
#[no_mangle]
pub extern "C" fn mdix_get_double(handle: *const c_void, path: *const c_char) -> f64 {
    clear_last_error();

    let (h, path_str) = match validate_read(handle, path, "mdix_get_double") {
        Some(v) => v,
        None => return 0.0,
    };

    match h.data.get::<f64>(path_str) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(&format!("mdix_get_double('{}'): {}", path_str, e));
            0.0
        }
    }
}

/// Get a boolean value by dotted path. Returns false on failure.
#[no_mangle]
pub extern "C" fn mdix_get_bool(handle: *const c_void, path: *const c_char) -> bool {
    clear_last_error();

    let (h, path_str) = match validate_read(handle, path, "mdix_get_bool") {
        Some(v) => v,
        None => return false,
    };

    match h.data.get::<bool>(path_str) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(&format!("mdix_get_bool('{}'): {}", path_str, e));
            false
        }
    }
}

// =============================================================================
// Data access — enum names
// =============================================================================

/// Return the enum type name at the given path (e.g. "AIType").
/// The caller must free the result with mdix_free_string(). Returns null on failure.
#[no_mangle]
pub extern "C" fn mdix_get_enum_name(
    handle: *const c_void,
    path: *const c_char,
) -> *mut c_char {
    clear_last_error();

    let (h, path_str) = match validate_read(handle, path, "mdix_get_enum_name") {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };

    match h.data.get_value(path_str) {
        Some(DixValue::Enum { enum_name, .. }) => str_to_c_char(enum_name.clone()),
        Some(_) => {
            set_last_error(&format!(
                "mdix_get_enum_name('{}'): value is not an enum", path_str
            ));
            std::ptr::null_mut()
        }
        None => {
            set_last_error(&format!("mdix_get_enum_name('{}'): path not found", path_str));
            std::ptr::null_mut()
        }
    }
}

/// Return the enum field name at the given path (e.g. "BOSS").
/// The caller must free the result with mdix_free_string(). Returns null on failure.
#[no_mangle]
pub extern "C" fn mdix_get_enum_field(
    handle: *const c_void,
    path: *const c_char,
) -> *mut c_char {
    clear_last_error();

    let (h, path_str) = match validate_read(handle, path, "mdix_get_enum_field") {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };

    match h.data.get_value(path_str) {
        Some(DixValue::Enum { field_name, .. }) => str_to_c_char(field_name.clone()),
        Some(_) => {
            set_last_error(&format!(
                "mdix_get_enum_field('{}'): value is not an enum", path_str
            ));
            std::ptr::null_mut()
        }
        None => {
            set_last_error(&format!("mdix_get_enum_field('{}'): path not found", path_str));
            std::ptr::null_mut()
        }
    }
}

// =============================================================================
// Data access — JSON escape hatch
// =============================================================================

/// Serialize the value at the given path to a JSON string.
/// Handles Blob, Regex, Tuple, and nested structures.
/// The caller must free the result with mdix_free_string(). Returns null on failure.
#[no_mangle]
pub extern "C" fn mdix_get_json(
    handle: *const c_void,
    path: *const c_char,
) -> *mut c_char {
    clear_last_error();

    let (h, path_str) = match validate_read(handle, path, "mdix_get_json") {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };

    match h.data.get_value(path_str) {
        None => {
            set_last_error(&format!("mdix_get_json('{}'): path not found", path_str));
            std::ptr::null_mut()
        }
        Some(value) => match serde_json::to_string(value) {
            Ok(json) => str_to_c_char(json),
            Err(e) => {
                set_last_error(&format!("mdix_get_json('{}'): {}", path_str, e));
                std::ptr::null_mut()
            }
        },
    }
}

// =============================================================================
// Key existence and enumeration
// =============================================================================

/// Check whether a dotted path exists in the loaded data. Returns false if handle is null.
#[no_mangle]
pub extern "C" fn mdix_exists(handle: *const c_void, path: *const c_char) -> bool {
    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None => return false,
    };

    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => return false,
    };

    h.data.exists(path_str)
}

/// Get the direct child key names under a path prefix.
/// Pass null or empty string for top-level keys.
/// `out_count` receives the key count — must not be null.
/// Free with mdix_free_string_array(result, out_count).
#[no_mangle]
pub extern "C" fn mdix_get_keys(
    handle: *const c_void,
    prefix: *const c_char,
    out_count: *mut i32,
) -> *mut *mut c_char {
    clear_last_error();

    if out_count.is_null() {
        set_last_error("mdix_get_keys: out_count must not be null");
        return std::ptr::null_mut();
    }

    unsafe { *out_count = 0 };

    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None => {
            set_last_error("mdix_get_keys: handle is null");
            return std::ptr::null_mut();
        }
    };

    let prefix_str = unsafe { c_str_to_str(prefix) }.unwrap_or("");
    let keys = h.data.get_keys(prefix_str);
    string_vec_to_c_array(keys, out_count)
}

// =============================================================================
// Memory management
// =============================================================================

/// Free a string returned by any mdix getter. Passing null is safe.
/// Do NOT call this on the pointer from mdix_version().
#[no_mangle]
pub extern "C" fn mdix_free_string(s: *mut c_char) {
    unsafe { free_c_char(s) };
}

/// Free a string array returned by mdix_get_keys or mdix_get_all_keys.
/// `count` must match the value written to out_count by the call that allocated the array.
#[no_mangle]
pub extern "C" fn mdix_free_string_array(arr: *mut *mut c_char, count: i32) {
    unsafe { free_c_char_array(arr, count) };
}

// =============================================================================
// Error reporting
// =============================================================================

/// Return the last error message, or null if there is no error.
/// The pointer is valid only until the next mdix FFI call. Do NOT free it.
#[no_mangle]
pub extern "C" fn mdix_get_last_error() -> *const c_char {
    get_last_error_ptr()
}

/// Clear the last error without making any other call.
#[no_mangle]
pub extern "C" fn mdix_clear_error() {
    clear_last_error();
}

// =============================================================================
// Conversion — database export
// =============================================================================

/// Export all entries as a JSON string.
/// `indented` controls pretty-printing. Free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_to_json(handle: *const c_void, indented: bool) -> *mut c_char {
    clear_last_error();

    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None => {
            set_last_error("mdix_to_json: handle is null");
            return std::ptr::null_mut();
        }
    };

    let entries   = h.data.to_hashmap();
    let converter = DixConverter::new();

    let ast = match converter.from_hashmap(entries) {
        Ok(a) => a,
        Err(e) => {
            set_last_error(&format!("mdix_to_json: AST conversion failed: {}", e));
            return std::ptr::null_mut();
        }
    };

    let map = converter.to_hashmap(&ast);
    let result = if indented {
        serde_json::to_string_pretty(&map)
    } else {
        serde_json::to_string(&map)
    };

    match result {
        Ok(s) => str_to_c_char(s),
        Err(e) => {
            set_last_error(&format!("mdix_to_json: JSON serialization failed: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Serialize the loaded database back to .mdix text.
/// `mode` controls output style. Free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_to_mdix(handle: *const c_void, mode: MdixFormatMode) -> *mut c_void {
    clear_last_error();

    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None => {
            set_last_error("mdix_to_mdix: handle is null");
            return std::ptr::null_mut();
        }
    };

    let entries   = h.data.to_hashmap();
    let converter = DixConverter::new();

    let ast = match converter.from_hashmap(entries) {
        Ok(a) => a,
        Err(e) => {
            set_last_error(&format!("mdix_to_mdix: AST conversion failed: {}", e));
            return std::ptr::null_mut();
        }
    };

    let options = format_mode_to_options(mode);

    match converter.to_mdix(&ast, Some(&options)) {
        Ok(s) => str_to_c_char(s) as *mut c_void,
        Err(e) => {
            set_last_error(&format!("mdix_to_mdix: serialization failed: {}", e));
            std::ptr::null_mut()
        }
    }
}

// =============================================================================
// Conversion — source text formatting
// =============================================================================

/// Format a raw .mdix source string. Free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_format_source(
    source: *const c_char,
    mode:   MdixFormatMode,
) -> *mut c_char {
    clear_last_error();

    let source_str = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None => {
            set_last_error("mdix_format_source: source is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };

    let result = match mode {
        MdixFormatMode::Minified => DixCompactor::minify(source_str),
        _                        => DixCompactor::compact(source_str),
    };

    str_to_c_char(result)
}

/// Minify a raw .mdix source string. Free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_minify_source(source: *const c_char) -> *mut c_char {
    clear_last_error();

    let source_str = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None => {
            set_last_error("mdix_minify_source: source is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };

    str_to_c_char(DixCompactor::minify(source_str))
}

// =============================================================================
// Builder — lifecycle
// =============================================================================

/// Create a new empty builder handle. Free with mdix_builder_free().
#[no_mangle]
pub extern "C" fn mdix_builder_new() -> *mut c_void {
    clear_last_error();
    MdixBuilderHandle::new() as *mut c_void
}

/// Free a builder handle. Passing null is safe.
#[no_mangle]
pub extern "C" fn mdix_builder_free(builder: *mut c_void) {
    unsafe { MdixBuilderHandle::free(builder as *mut MdixBuilderHandle) };
}

/// Return the number of entries in the builder. Returns -1 if null.
#[no_mangle]
pub extern "C" fn mdix_builder_entry_count(builder: *const c_void) -> i32 {
    match unsafe { as_builder(builder) } {
        Some(b) => b.entries.len() as i32,
        None => -1,
    }
}

/// Remove all entries from the builder without freeing the handle.
/// Returns true on success, false if the builder is null.
#[no_mangle]
pub extern "C" fn mdix_builder_clear(builder: *mut c_void) -> bool {
    clear_last_error();

    match unsafe { as_builder_mut(builder) } {
        Some(b) => { b.entries.clear(); true }
        None => {
            set_last_error("mdix_builder_clear: builder is null");
            false
        }
    }
}

// =============================================================================
// Builder — write
// =============================================================================

/// Set a string value. Returns true on success.
#[no_mangle]
pub extern "C" fn mdix_builder_set_string(
    builder: *mut c_void,
    path: *const c_char,
    value: *const c_char,
) -> bool {
    clear_last_error();

    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_set_string") {
        Some(v) => v,
        None => return false,
    };

    let value_str = match unsafe { c_str_to_str(value) } {
        Some(s) => s.to_string(),
        None => {
            set_last_error("mdix_builder_set_string: value is null or invalid UTF-8");
            return false;
        }
    };

    b.entries.insert(path_str.to_string(), DixValue::String(value_str));
    true
}

/// Set an integer value. Returns true on success.
#[no_mangle]
pub extern "C" fn mdix_builder_set_int(
    builder: *mut c_void,
    path: *const c_char,
    value: i32,
) -> bool {
    clear_last_error();

    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_set_int") {
        Some(v) => v,
        None => return false,
    };

    b.entries.insert(path_str.to_string(), DixValue::Int(value));
    true
}

/// Set a float value. Returns true on success.
#[no_mangle]
pub extern "C" fn mdix_builder_set_float(
    builder: *mut c_void,
    path: *const c_char,
    value: f32,
) -> bool {
    clear_last_error();

    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_set_float") {
        Some(v) => v,
        None => return false,
    };

    b.entries.insert(path_str.to_string(), DixValue::Float(value));
    true
}

/// Set a double value. Returns true on success.
#[no_mangle]
pub extern "C" fn mdix_builder_set_double(
    builder: *mut c_void,
    path: *const c_char,
    value: f64,
) -> bool {
    clear_last_error();

    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_set_double") {
        Some(v) => v,
        None => return false,
    };

    b.entries.insert(path_str.to_string(), DixValue::Double(value));
    true
}

/// Set a boolean value. Returns true on success.
#[no_mangle]
pub extern "C" fn mdix_builder_set_bool(
    builder: *mut c_void,
    path: *const c_char,
    value: bool,
) -> bool {
    clear_last_error();

    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_set_bool") {
        Some(v) => v,
        None => return false,
    };

    b.entries.insert(path_str.to_string(), DixValue::Bool(value));
    true
}

/// Remove a key from the builder. Returns true if it existed and was removed.
#[no_mangle]
pub extern "C" fn mdix_builder_remove(
    builder: *mut c_void,
    path: *const c_char,
) -> bool {
    clear_last_error();

    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_remove") {
        Some(v) => v,
        None => return false,
    };

    b.entries.remove(path_str).is_some()
}

// =============================================================================
// Builder — read back
// =============================================================================

/// Check whether a key exists in the builder.
#[no_mangle]
pub extern "C" fn mdix_builder_has_key(
    builder: *const c_void,
    path: *const c_char,
) -> bool {
    let b = match unsafe { as_builder(builder) } {
        Some(b) => b,
        None => return false,
    };

    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => return false,
    };

    b.entries.contains_key(path_str)
}

/// Get a string value from the builder. Free with mdix_free_string(). Returns null on failure.
#[no_mangle]
pub extern "C" fn mdix_builder_get_string(
    builder: *const c_void,
    path: *const c_char,
) -> *mut c_char {
    clear_last_error();

    let (b, path_str) = match validate_builder_read(builder, path, "mdix_builder_get_string") {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };

    match b.entries.get(path_str) {
        Some(DixValue::String(s)) => str_to_c_char(s.clone()),
        Some(other) => {
            set_last_error(&format!(
                "mdix_builder_get_string('{}'): value is {} not string",
                path_str, other.type_name()
            ));
            std::ptr::null_mut()
        }
        None => {
            set_last_error(&format!(
                "mdix_builder_get_string('{}'): key not found", path_str
            ));
            std::ptr::null_mut()
        }
    }
}

/// Get an integer value from the builder. Returns 0 on failure.
#[no_mangle]
pub extern "C" fn mdix_builder_get_int(
    builder: *const c_void,
    path: *const c_char,
) -> i32 {
    clear_last_error();

    let (b, path_str) = match validate_builder_read(builder, path, "mdix_builder_get_int") {
        Some(v) => v,
        None => return 0,
    };

    match b.entries.get(path_str) {
        Some(DixValue::Int(i))    => *i,
        Some(DixValue::Float(f))  => *f as i32,
        Some(DixValue::Double(d)) => *d as i32,
        Some(other) => {
            set_last_error(&format!(
                "mdix_builder_get_int('{}'): value is {} not numeric",
                path_str, other.type_name()
            ));
            0
        }
        None => {
            set_last_error(&format!("mdix_builder_get_int('{}'): key not found", path_str));
            0
        }
    }
}

/// Get a float value from the builder. Returns 0.0 on failure.
#[no_mangle]
pub extern "C" fn mdix_builder_get_float(
    builder: *const c_void,
    path: *const c_char,
) -> f32 {
    clear_last_error();

    let (b, path_str) = match validate_builder_read(builder, path, "mdix_builder_get_float") {
        Some(v) => v,
        None => return 0.0,
    };

    match b.entries.get(path_str) {
        Some(DixValue::Float(f))  => *f,
        Some(DixValue::Int(i))    => *i as f32,
        Some(DixValue::Double(d)) => *d as f32,
        Some(other) => {
            set_last_error(&format!(
                "mdix_builder_get_float('{}'): value is {} not numeric",
                path_str, other.type_name()
            ));
            0.0
        }
        None => {
            set_last_error(&format!(
                "mdix_builder_get_float('{}'): key not found", path_str
            ));
            0.0
        }
    }
}

/// Get a double value from the builder. Returns 0.0 on failure.
#[no_mangle]
pub extern "C" fn mdix_builder_get_double(
    builder: *const c_void,
    path: *const c_char,
) -> f64 {
    clear_last_error();

    let (b, path_str) = match validate_builder_read(builder, path, "mdix_builder_get_double") {
        Some(v) => v,
        None => return 0.0,
    };

    match b.entries.get(path_str) {
        Some(DixValue::Double(d)) => *d,
        Some(DixValue::Float(f))  => *f as f64,
        Some(DixValue::Int(i))    => *i as f64,
        Some(other) => {
            set_last_error(&format!(
                "mdix_builder_get_double('{}'): value is {} not numeric",
                path_str, other.type_name()
            ));
            0.0
        }
        None => {
            set_last_error(&format!(
                "mdix_builder_get_double('{}'): key not found", path_str
            ));
            0.0
        }
    }
}

/// Get a boolean value from the builder. Returns false on failure.
#[no_mangle]
pub extern "C" fn mdix_builder_get_bool(
    builder: *const c_void,
    path: *const c_char,
) -> bool {
    clear_last_error();

    let (b, path_str) = match validate_builder_read(builder, path, "mdix_builder_get_bool") {
        Some(v) => v,
        None => return false,
    };

    match b.entries.get(path_str) {
        Some(DixValue::Bool(bv)) => *bv,
        Some(other) => {
            set_last_error(&format!(
                "mdix_builder_get_bool('{}'): value is {} not bool",
                path_str, other.type_name()
            ));
            false
        }
        None => {
            set_last_error(&format!(
                "mdix_builder_get_bool('{}'): key not found", path_str
            ));
            false
        }
    }
}

// =============================================================================
// Builder — persistence
// =============================================================================

/// Save the builder contents to a .mdix file on disk. Returns true on success.
#[no_mangle]
pub extern "C" fn mdix_builder_save(
    builder: *const c_void,
    path: *const c_char,
) -> bool {
    clear_last_error();

    let b = match unsafe { as_builder(builder) } {
        Some(b) => b,
        None => {
            set_last_error("mdix_builder_save: builder is null");
            return false;
        }
    };

    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => {
            set_last_error("mdix_builder_save: path is null or invalid UTF-8");
            return false;
        }
    };

    let entries   = b.entries.clone();
    let converter = DixConverter::new();

    let ast = match converter.from_hashmap(entries) {
        Ok(a) => a,
        Err(e) => {
            set_last_error(&format!("mdix_builder_save: AST conversion failed: {}", e));
            return false;
        }
    };

    let content = match converter.to_mdix(&ast, None) {
        Ok(s) => s,
        Err(e) => {
            set_last_error(&format!("mdix_builder_save: serialization failed: {}", e));
            return false;
        }
    };

    if let Some(parent) = std::path::Path::new(path_str).parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            set_last_error(&format!(
                "mdix_builder_save: could not create directories: {}", e
            ));
            return false;
        }
    }

    match std::fs::write(path_str, content) {
        Ok(()) => true,
        Err(e) => {
            set_last_error(&format!("mdix_builder_save: write failed: {}", e));
            false
        }
    }
}

/// Serialize the builder contents to a .mdix format string.
/// Free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_builder_to_string(builder: *const c_void) -> *mut c_char {
    clear_last_error();

    let b = match unsafe { as_builder(builder) } {
        Some(b) => b,
        None => {
            set_last_error("mdix_builder_to_string: builder is null");
            return std::ptr::null_mut();
        }
    };

    let entries   = b.entries.clone();
    let converter = DixConverter::new();

    let ast = match converter.from_hashmap(entries) {
        Ok(a) => a,
        Err(e) => {
            set_last_error(&format!(
                "mdix_builder_to_string: AST conversion failed: {}", e
            ));
            return std::ptr::null_mut();
        }
    };

    match converter.to_mdix(&ast, Some(&DixFormatOptions::pretty())) {
        Ok(s) => str_to_c_char(s),
        Err(e) => {
            set_last_error(&format!("mdix_builder_to_string: serialization failed: {}", e));
            std::ptr::null_mut()
        }
    }
}

// =============================================================================
// Conversion — TOML and foreign format import
// =============================================================================

/// Export all entries as a TOML string. Free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_to_toml(handle: *const c_void) -> *mut c_char {
    clear_last_error();

    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None => {
            set_last_error("mdix_to_toml: handle is null");
            return std::ptr::null_mut();
        }
    };

    let entries   = h.data.to_hashmap();
    let converter = DixConverter::new();

    let ast = match converter.from_hashmap(entries) {
        Ok(a) => a,
        Err(e) => {
            set_last_error(&format!("mdix_to_toml: AST conversion failed: {}", e));
            return std::ptr::null_mut();
        }
    };

    match converter.to_toml(&ast) {
        Ok(s) => str_to_c_char(s),
        Err(e) => {
            set_last_error(&format!("mdix_to_toml: TOML serialization failed: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Load a JSON object string and return a handle. Free with mdix_free().
#[no_mangle]
pub extern "C" fn mdix_from_json(source: *const c_char) -> *mut c_void {
    clear_last_error();

    let src = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None => {
            set_last_error("mdix_from_json: source is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };

    let converter = DixConverter::new();
    let ast = match converter.from_json(src) {
        Ok(a) => a,
        Err(e) => {
            set_last_error(&format!("mdix_from_json: {}", e));
            return std::ptr::null_mut();
        }
    };

    let mdix_src = match converter.to_mdix(&ast, None) {
        Ok(s) => s,
        Err(e) => {
            set_last_error(&format!("mdix_from_json: re-serialization failed: {}", e));
            return std::ptr::null_mut();
        }
    };

    let loader = DixLoader::new();
    match loader.load_from_str(&mdix_src, &DixLoadOptions::new()) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e) => {
            set_last_error(&format!("mdix_from_json: load failed: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Load a TOML table string and return a handle. Free with mdix_free().
#[no_mangle]
pub extern "C" fn mdix_from_toml(source: *const c_char) -> *mut c_void {
    clear_last_error();

    let src = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None => {
            set_last_error("mdix_from_toml: source is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };

    let converter = DixConverter::new();
    let ast = match converter.from_toml(src) {
        Ok(a) => a,
        Err(e) => {
            set_last_error(&format!("mdix_from_toml: {}", e));
            return std::ptr::null_mut();
        }
    };

    let mdix_src = match converter.to_mdix(&ast, None) {
        Ok(s) => s,
        Err(e) => {
            set_last_error(&format!("mdix_from_toml: re-serialization failed: {}", e));
            return std::ptr::null_mut();
        }
    };

    let loader = DixLoader::new();
    match loader.load_from_str(&mdix_src, &DixLoadOptions::new()) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e) => {
            set_last_error(&format!("mdix_from_toml: load failed: {}", e));
            std::ptr::null_mut()
        }
    }
}

// =============================================================================
// Private helpers
// =============================================================================

fn validate_read<'a>(
    handle: *const c_void,
    path: *const c_char,
    fn_name: &str,
) -> Option<(&'a MdixHandle, &'a str)> {
    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None => {
            set_last_error(&format!("{}: handle is null", fn_name));
            return None;
        }
    };

    let path_str = unsafe { c_str_to_str(path) }?;
    if path_str.is_empty() {
        set_last_error(&format!("{}: path is empty", fn_name));
        return None;
    }

    Some((h, path_str))
}

fn validate_builder_write<'a>(
    builder: *mut c_void,
    path: *const c_char,
    fn_name: &str,
) -> Option<(&'a mut MdixBuilderHandle, &'a str)> {
    let b = match unsafe { as_builder_mut(builder) } {
        Some(b) => b,
        None => {
            set_last_error(&format!("{}: builder is null", fn_name));
            return None;
        }
    };

    let path_str = unsafe { c_str_to_str(path) }?;
    if path_str.is_empty() {
        set_last_error(&format!("{}: path is empty", fn_name));
        return None;
    }

    Some((b, path_str))
}

fn validate_builder_read<'a>(
    builder: *const c_void,
    path: *const c_char,
    fn_name: &str,
) -> Option<(&'a MdixBuilderHandle, &'a str)> {
    let b = match unsafe { as_builder(builder) } {
        Some(b) => b,
        None => {
            set_last_error(&format!("{}: builder is null", fn_name));
            return None;
        }
    };

    let path_str = unsafe { c_str_to_str(path) }?;
    if path_str.is_empty() {
        set_last_error(&format!("{}: path is empty", fn_name));
        return None;
    }

    Some((b, path_str))
}

fn format_mode_to_options(mode: MdixFormatMode) -> DixFormatOptions {
    match mode {
        MdixFormatMode::Default  => DixFormatOptions::new(),
        MdixFormatMode::Pretty   => DixFormatOptions::pretty(),
        MdixFormatMode::Compact  => DixFormatOptions::compact(),
        MdixFormatMode::Minified => DixFormatOptions::minified(),
    }
}
