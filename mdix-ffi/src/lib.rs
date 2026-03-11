// mdix-ffi/src/lib.rs
//
// Public C FFI surface for DixScript.
//
// Every function here is parsed by csbindgen (via build.rs) and becomes a
// static extern method in MdixNative.cs. Keep function signatures C-safe:
//   - primitives only: *const c_char, *mut c_char, i32, f32, f64, bool
//   - opaque handle pointers: *mut MdixHandle, *mut MdixBuilderHandle
//   - no Rust types, no generics, no references
//
// Error contract (mirrors C errno):
//   - On success: clear error slot, return valid value
//   - On failure: write to error slot, return sentinel (null / 0 / false)
//   - Caller checks sentinel, then calls mdix_get_last_error() for details

mod error;
mod handle;
mod string_utils;

use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::OnceLock;

use dixscript::Runtime::{
    DixConverter, DixData, DixFormatOptions, DixLoader, DixLoadOptions, DixValue,
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
///
/// Maps 1:1 to DixValue variants. Use these constants on the C# side to decide
/// which getter to call. Unknown (-1) means the path does not exist.
#[repr(i32)]
pub enum MdixType {
    Unknown   = -1,
    Null      = 0,
    Bool      = 1,
    Int       = 2,
    Float     = 3,
    Double    = 4,
    String    = 5,
    Date      = 6,
    Timestamp = 7,
    HexColor  = 8,
    Blob      = 9,
    Regex     = 10,
    Array     = 11,
    Object    = 12,
    Tuple     = 13,
    Enum      = 14,
}

// =============================================================================
// Metadata
// =============================================================================

/// Return the DixScript library version as a null-terminated C string.
///
/// The returned pointer is static — do NOT free it with mdix_free_string.
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
///
/// Returns an opaque handle on success, null on failure.
/// The caller must free the handle with mdix_free() when done.
/// On failure, call mdix_get_last_error() for a description.
#[no_mangle]
pub extern "C" fn mdix_load(path: *const c_char) -> *mut MdixHandle {
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
        Ok(data) => MdixHandle::new(data),
        Err(e) => {
            set_last_error(&format!("mdix_load: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Load a .mdix file from a raw source string (no disk access).
///
/// Useful for loading .mdix content bundled as a TextAsset in Unity.
///
/// Returns an opaque handle on success, null on failure.
/// The caller must free the handle with mdix_free() when done.
#[no_mangle]
pub extern "C" fn mdix_load_str(source: *const c_char) -> *mut MdixHandle {
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
        Ok(data) => MdixHandle::new(data),
        Err(e) => {
            set_last_error(&format!("mdix_load_str: {}", e));
            std::ptr::null_mut()
        }
    }
}

// =============================================================================
// Handle lifecycle — encrypted .mdix.enc files
// =============================================================================

/// Load an encrypted .mdix.enc file using a key file for decryption.
///
/// `enc_path` — path to the .mdix.enc file.
/// `key_path` — path to the .mdix.key file, or null to auto-detect next to the .enc file.
///
/// Returns an opaque handle on success, null on failure.
/// The caller must free the handle with mdix_free() when done.
#[no_mangle]
pub extern "C" fn mdix_load_encrypted(
    enc_path: *const c_char,
    key_path: *const c_char,
) -> *mut MdixHandle {
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
        Ok(data) => MdixHandle::new(data),
        Err(e) => {
            set_last_error(&format!("mdix_load_encrypted: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Load an encrypted .mdix.enc file using a password for decryption.
///
/// `enc_path` — path to the .mdix.enc file.
/// `password` — decryption password (must match the one used during compilation).
///
/// Returns an opaque handle on success, null on failure.
/// The caller must free the handle with mdix_free() when done.
#[no_mangle]
pub extern "C" fn mdix_load_encrypted_password(
    enc_path: *const c_char,
    password: *const c_char,
) -> *mut MdixHandle {
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
        Ok(data) => MdixHandle::new(data),
        Err(e) => {
            set_last_error(&format!("mdix_load_encrypted_password: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Load encrypted data from raw bytes with the key file contents as a string.
///
/// `encrypted_bytes`  — pointer to the encrypted data buffer.
/// `byte_count`       — number of bytes in the buffer.
/// `key_file_content` — full text content of the .mdix.key file.
/// `password`         — decryption password, or null if key file mode.
///
/// Returns an opaque handle on success, null on failure.
#[no_mangle]
pub extern "C" fn mdix_load_encrypted_bytes(
    encrypted_bytes: *const u8,
    byte_count: i32,
    key_file_content: *const c_char,
    password: *const c_char,
) -> *mut MdixHandle {
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
        Ok(data) => MdixHandle::new(data),
        Err(e) => {
            set_last_error(&format!("mdix_load_encrypted_bytes: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Free a handle returned by any mdix_load* function.
///
/// After calling this the handle pointer is invalid — do not use it again.
/// Passing null is safe and does nothing.
#[no_mangle]
pub extern "C" fn mdix_free(handle: *mut MdixHandle) {
    unsafe { MdixHandle::free(handle) };
}

// =============================================================================
// Validity and metadata
// =============================================================================

/// Return true if the handle is non-null.
#[no_mangle]
pub extern "C" fn mdix_is_valid(handle: *const MdixHandle) -> bool {
    !handle.is_null()
}

/// Return the total number of data entries in the loaded file.
///
/// Returns -1 if the handle is null.
#[no_mangle]
pub extern "C" fn mdix_entry_count(handle: *const MdixHandle) -> i32 {
    if handle.is_null() {
        return -1;
    }
    unsafe { (*handle).data.entry_count() as i32 }
}

// =============================================================================
// Type inspection
// =============================================================================

/// Return the MdixType discriminant of the value at the given path.
///
/// Returns -1 (MdixType::Unknown) if the path does not exist or the handle is null.
/// Call this before a getter when the schema is not known at compile time.
#[no_mangle]
pub extern "C" fn mdix_get_type(handle: *const MdixHandle, path: *const c_char) -> i32 {
    if handle.is_null() {
        return -1;
    }

    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => return -1,
    };

    match unsafe { (*handle).data.get_value(path_str) } {
        None                         => -1,
        Some(DixValue::Null)         =>  0,
        Some(DixValue::Bool(_))      =>  1,
        Some(DixValue::Int(_))       =>  2,
        Some(DixValue::Float(_))     =>  3,
        Some(DixValue::Double(_))    =>  4,
        Some(DixValue::String(_))    =>  5,
        Some(DixValue::Date(_))      =>  6,
        Some(DixValue::Timestamp(_)) =>  7,
        Some(DixValue::HexColor(_))  =>  8,
        Some(DixValue::Blob(_))      =>  9,
        Some(DixValue::Regex(_))     => 10,
        Some(DixValue::Array(_))     => 11,
        Some(DixValue::Object(_))    => 12,
        Some(DixValue::Tuple(_))     => 13,
        Some(DixValue::Enum { .. })  => 14,
    }
}

/// Return the number of items in the array at the given path.
///
/// Returns -1 if the path does not exist, the value is not an array, or the
/// handle is null. Use this to drive a loop over indexed paths:
///   for i in 0..mdix_get_array_length(h, "enemies") { mdix_get_int(h, $"enemies[{i}].health") }
#[no_mangle]
pub extern "C" fn mdix_get_array_length(handle: *const MdixHandle, path: *const c_char) -> i32 {
    if handle.is_null() {
        return -1;
    }

    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => return -1,
    };

    match unsafe { (*handle).data.get_value(path_str) } {
        Some(DixValue::Array(arr)) => arr.len() as i32,
        _ => -1,
    }
}

// =============================================================================
// Data access — typed getters
// =============================================================================

/// Get a string value by dotted path.
///
/// Also works for Date, Timestamp, and HexColor — all are stored as strings.
/// Returns a heap-allocated C string on success, null on failure.
/// The caller must free the result with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_get_string(
    handle: *const MdixHandle,
    path: *const c_char,
) -> *mut c_char {
    clear_last_error();

    let (data, path_str) = match validate_read_args(handle, path, "mdix_get_string") {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };

    match data.get::<String>(path_str) {
        Ok(s) => str_to_c_char(s),
        Err(e) => {
            set_last_error(&format!("mdix_get_string('{}'): {}", path_str, e));
            std::ptr::null_mut()
        }
    }
}

/// Get an integer value by dotted path.
///
/// Also works for Enum values — returns the resolved integer (e.g. BOSS → 2).
/// Returns 0 on failure. Use mdix_exists() to distinguish 0 from not-found.
#[no_mangle]
pub extern "C" fn mdix_get_int(handle: *const MdixHandle, path: *const c_char) -> i32 {
    clear_last_error();

    let (data, path_str) = match validate_read_args(handle, path, "mdix_get_int") {
        Some(v) => v,
        None => return 0,
    };

    match data.get::<i32>(path_str) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(&format!("mdix_get_int('{}'): {}", path_str, e));
            0
        }
    }
}

/// Get a float (f32) value by dotted path.
///
/// Returns 0.0 on failure.
#[no_mangle]
pub extern "C" fn mdix_get_float(handle: *const MdixHandle, path: *const c_char) -> f32 {
    clear_last_error();

    let (data, path_str) = match validate_read_args(handle, path, "mdix_get_float") {
        Some(v) => v,
        None => return 0.0,
    };

    match data.get::<f64>(path_str) {
        Ok(v) => v as f32,
        Err(e) => {
            set_last_error(&format!("mdix_get_float('{}'): {}", path_str, e));
            0.0
        }
    }
}

/// Get a double (f64) value by dotted path.
///
/// Returns 0.0 on failure.
#[no_mangle]
pub extern "C" fn mdix_get_double(handle: *const MdixHandle, path: *const c_char) -> f64 {
    clear_last_error();

    let (data, path_str) = match validate_read_args(handle, path, "mdix_get_double") {
        Some(v) => v,
        None => return 0.0,
    };

    match data.get::<f64>(path_str) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(&format!("mdix_get_double('{}'): {}", path_str, e));
            0.0
        }
    }
}

/// Get a boolean value by dotted path.
///
/// Returns false on failure.
#[no_mangle]
pub extern "C" fn mdix_get_bool(handle: *const MdixHandle, path: *const c_char) -> bool {
    clear_last_error();

    let (data, path_str) = match validate_read_args(handle, path, "mdix_get_bool") {
        Some(v) => v,
        None => return false,
    };

    match data.get::<bool>(path_str) {
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
///
/// Returns null if the path does not exist or the value is not an enum.
/// The caller must free the result with mdix_free_string().
///
/// Use mdix_get_int for the resolved integer value and mdix_get_enum_field
/// for the field name. Together these give the full picture:
///   type  = mdix_get_enum_name(h, "enemy.ai")   → "AIType"
///   field = mdix_get_enum_field(h, "enemy.ai")  → "BOSS"
///   value = mdix_get_int(h, "enemy.ai")         →  2
#[no_mangle]
pub extern "C" fn mdix_get_enum_name(
    handle: *const MdixHandle,
    path: *const c_char,
) -> *mut c_char {
    clear_last_error();

    let (data, path_str) = match validate_read_args(handle, path, "mdix_get_enum_name") {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };

    match data.get_value(path_str) {
        Some(DixValue::Enum { enum_name, .. }) => str_to_c_char(enum_name.clone()),
        Some(_) => {
            set_last_error(&format!(
                "mdix_get_enum_name('{}'): value is not an enum",
                path_str
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
///
/// Returns null if the path does not exist or the value is not an enum.
/// The caller must free the result with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_get_enum_field(
    handle: *const MdixHandle,
    path: *const c_char,
) -> *mut c_char {
    clear_last_error();

    let (data, path_str) = match validate_read_args(handle, path, "mdix_get_enum_field") {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };

    match data.get_value(path_str) {
        Some(DixValue::Enum { field_name, .. }) => str_to_c_char(field_name.clone()),
        Some(_) => {
            set_last_error(&format!(
                "mdix_get_enum_field('{}'): value is not an enum",
                path_str
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
///
/// This is the escape hatch for Blob, Regex, Tuple, and any nested structure
/// where you want the whole subtree handed to JsonUtility or Newtonsoft in one call.
///
/// Returns a heap-allocated C string on success, null on failure.
/// The caller must free the result with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_get_json(
    handle: *const MdixHandle,
    path: *const c_char,
) -> *mut c_char {
    clear_last_error();

    let (data, path_str) = match validate_read_args(handle, path, "mdix_get_json") {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };

    match data.get_value(path_str) {
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

/// Check whether a dotted path exists in the loaded data.
///
/// Returns false if the handle is null.
#[no_mangle]
pub extern "C" fn mdix_exists(handle: *const MdixHandle, path: *const c_char) -> bool {
    if handle.is_null() {
        return false;
    }

    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => return false,
    };

    unsafe { (*handle).data.exists(path_str) }
}

/// Get the direct child key names under a path prefix.
///
/// `prefix`    — dotted path prefix, or null / empty string for top-level keys.
/// `out_count` — receives the number of keys returned. Must not be null.
///
/// Returns a heap-allocated array of null-terminated C strings, or null if empty.
/// The caller must free the result with mdix_free_string_array(result, out_count).
#[no_mangle]
pub extern "C" fn mdix_get_keys(
    handle: *const MdixHandle,
    prefix: *const c_char,
    out_count: *mut i32,
) -> *mut *mut c_char {
    clear_last_error();

    if out_count.is_null() {
        set_last_error("mdix_get_keys: out_count must not be null");
        return std::ptr::null_mut();
    }

    unsafe { *out_count = 0 };

    if handle.is_null() {
        set_last_error("mdix_get_keys: handle is null");
        return std::ptr::null_mut();
    }

    let prefix_str = unsafe { c_str_to_str(prefix) }.unwrap_or("");
    let keys = unsafe { (*handle).data.get_keys(prefix_str) };
    string_vec_to_c_array(keys, out_count)
}

// =============================================================================
// Memory management
// =============================================================================

/// Free a string returned by any mdix getter that returns *mut c_char.
///
/// Passing null is safe. Do NOT call this on the pointer from mdix_version().
#[no_mangle]
pub extern "C" fn mdix_free_string(s: *mut c_char) {
    unsafe { free_c_char(s) };
}

/// Free an array of strings returned by mdix_get_keys.
///
/// `arr`   — pointer returned by mdix_get_keys.
/// `count` — value written to out_count by mdix_get_keys.
///
/// Passing a null arr is safe.
#[no_mangle]
pub extern "C" fn mdix_free_string_array(arr: *mut *mut c_char, count: i32) {
    unsafe { free_c_char_array(arr, count) };
}

// =============================================================================
// Error reporting
// =============================================================================

/// Return the last error message as a null-terminated C string, or null
/// if the last operation succeeded.
///
/// The returned pointer is valid only until the next mdix FFI call.
/// Copy the string immediately — do not cache the pointer.
/// Do NOT free this pointer.
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
// Builder — lifecycle
// =============================================================================

/// Create a new empty builder handle.
///
/// The caller must free it with mdix_builder_free() when done.
#[no_mangle]
pub extern "C" fn mdix_builder_new() -> *mut MdixBuilderHandle {
    clear_last_error();
    MdixBuilderHandle::new()
}

/// Free a builder handle. Passing null is safe.
#[no_mangle]
pub extern "C" fn mdix_builder_free(builder: *mut MdixBuilderHandle) {
    unsafe { MdixBuilderHandle::free(builder) };
}

/// Return the number of entries currently in the builder. Returns -1 if null.
#[no_mangle]
pub extern "C" fn mdix_builder_entry_count(builder: *const MdixBuilderHandle) -> i32 {
    if builder.is_null() {
        return -1;
    }
    unsafe { (*builder).entries.len() as i32 }
}

/// Remove all entries from the builder without freeing the handle.
///
/// Returns true on success, false if the builder is null.
#[no_mangle]
pub extern "C" fn mdix_builder_clear(builder: *mut MdixBuilderHandle) -> bool {
    clear_last_error();

    if builder.is_null() {
        set_last_error("mdix_builder_clear: builder is null");
        return false;
    }

    unsafe { (*builder).entries.clear() };
    true
}

// =============================================================================
// Builder — write
// =============================================================================

/// Set a string value at the given dotted path.
///
/// Returns true on success, false if the builder or path is null.
#[no_mangle]
pub extern "C" fn mdix_builder_set_string(
    builder: *mut MdixBuilderHandle,
    path: *const c_char,
    value: *const c_char,
) -> bool {
    clear_last_error();

    let (builder_ref, path_str) =
        match validate_builder_args(builder, path, "mdix_builder_set_string") {
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

    builder_ref.entries.insert(path_str.to_string(), DixValue::String(value_str));
    true
}

/// Set an integer value at the given dotted path.
///
/// Returns true on success, false if the builder or path is null.
#[no_mangle]
pub extern "C" fn mdix_builder_set_int(
    builder: *mut MdixBuilderHandle,
    path: *const c_char,
    value: i32,
) -> bool {
    clear_last_error();

    let (builder_ref, path_str) =
        match validate_builder_args(builder, path, "mdix_builder_set_int") {
            Some(v) => v,
            None => return false,
        };

    builder_ref.entries.insert(path_str.to_string(), DixValue::Int(value));
    true
}

/// Set a float value at the given dotted path.
///
/// Returns true on success, false if the builder or path is null.
#[no_mangle]
pub extern "C" fn mdix_builder_set_float(
    builder: *mut MdixBuilderHandle,
    path: *const c_char,
    value: f32,
) -> bool {
    clear_last_error();

    let (builder_ref, path_str) =
        match validate_builder_args(builder, path, "mdix_builder_set_float") {
            Some(v) => v,
            None => return false,
        };

    builder_ref.entries.insert(path_str.to_string(), DixValue::Float(value));
    true
}

/// Set a double value at the given dotted path.
///
/// Returns true on success, false if the builder or path is null.
#[no_mangle]
pub extern "C" fn mdix_builder_set_double(
    builder: *mut MdixBuilderHandle,
    path: *const c_char,
    value: f64,
) -> bool {
    clear_last_error();

    let (builder_ref, path_str) =
        match validate_builder_args(builder, path, "mdix_builder_set_double") {
            Some(v) => v,
            None => return false,
        };

    builder_ref.entries.insert(path_str.to_string(), DixValue::Double(value));
    true
}

/// Set a boolean value at the given dotted path.
///
/// Returns true on success, false if the builder or path is null.
#[no_mangle]
pub extern "C" fn mdix_builder_set_bool(
    builder: *mut MdixBuilderHandle,
    path: *const c_char,
    value: bool,
) -> bool {
    clear_last_error();

    let (builder_ref, path_str) =
        match validate_builder_args(builder, path, "mdix_builder_set_bool") {
            Some(v) => v,
            None => return false,
        };

    builder_ref.entries.insert(path_str.to_string(), DixValue::Bool(value));
    true
}

/// Remove a key from the builder.
///
/// Returns true if the key existed and was removed, false otherwise.
#[no_mangle]
pub extern "C" fn mdix_builder_remove(
    builder: *mut MdixBuilderHandle,
    path: *const c_char,
) -> bool {
    clear_last_error();

    let (builder_ref, path_str) =
        match validate_builder_args(builder, path, "mdix_builder_remove") {
            Some(v) => v,
            None => return false,
        };

    builder_ref.entries.remove(path_str).is_some()
}

// =============================================================================
// Builder — read back
// =============================================================================

/// Check whether a key exists in the builder.
///
/// Returns false if the builder is null or the key is not present.
#[no_mangle]
pub extern "C" fn mdix_builder_has_key(
    builder: *const MdixBuilderHandle,
    path: *const c_char,
) -> bool {
    if builder.is_null() {
        return false;
    }

    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => return false,
    };

    unsafe { (*builder).entries.contains_key(path_str) }
}

/// Get a string value from the builder by dotted path.
///
/// Returns a heap-allocated C string on success, null on failure.
/// The caller must free the result with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_builder_get_string(
    builder: *const MdixBuilderHandle,
    path: *const c_char,
) -> *mut c_char {
    clear_last_error();

    let (builder_ref, path_str) =
        match validate_builder_read_args(builder, path, "mdix_builder_get_string") {
            Some(v) => v,
            None => return std::ptr::null_mut(),
        };

    match builder_ref.entries.get(path_str) {
        Some(DixValue::String(s)) => str_to_c_char(s.clone()),
        Some(other) => {
            set_last_error(&format!(
                "mdix_builder_get_string('{}'): value is {} not string",
                path_str,
                other.type_name()
            ));
            std::ptr::null_mut()
        }
        None => {
            set_last_error(&format!(
                "mdix_builder_get_string('{}'): key not found",
                path_str
            ));
            std::ptr::null_mut()
        }
    }
}

/// Get an integer value from the builder by dotted path.
///
/// Returns 0 on failure. Use mdix_builder_has_key() to distinguish 0 from not-found.
#[no_mangle]
pub extern "C" fn mdix_builder_get_int(
    builder: *const MdixBuilderHandle,
    path: *const c_char,
) -> i32 {
    clear_last_error();

    let (builder_ref, path_str) =
        match validate_builder_read_args(builder, path, "mdix_builder_get_int") {
            Some(v) => v,
            None => return 0,
        };

    match builder_ref.entries.get(path_str) {
        Some(DixValue::Int(i))    => *i,
        Some(DixValue::Float(f))  => *f as i32,
        Some(DixValue::Double(d)) => *d as i32,
        Some(other) => {
            set_last_error(&format!(
                "mdix_builder_get_int('{}'): value is {} not numeric",
                path_str,
                other.type_name()
            ));
            0
        }
        None => {
            set_last_error(&format!("mdix_builder_get_int('{}'): key not found", path_str));
            0
        }
    }
}

/// Get a float value from the builder by dotted path.
///
/// Returns 0.0 on failure.
#[no_mangle]
pub extern "C" fn mdix_builder_get_float(
    builder: *const MdixBuilderHandle,
    path: *const c_char,
) -> f32 {
    clear_last_error();

    let (builder_ref, path_str) =
        match validate_builder_read_args(builder, path, "mdix_builder_get_float") {
            Some(v) => v,
            None => return 0.0,
        };

    match builder_ref.entries.get(path_str) {
        Some(DixValue::Float(f))  => *f,
        Some(DixValue::Int(i))    => *i as f32,
        Some(DixValue::Double(d)) => *d as f32,
        Some(other) => {
            set_last_error(&format!(
                "mdix_builder_get_float('{}'): value is {} not numeric",
                path_str,
                other.type_name()
            ));
            0.0
        }
        None => {
            set_last_error(&format!(
                "mdix_builder_get_float('{}'): key not found",
                path_str
            ));
            0.0
        }
    }
}

/// Get a double value from the builder by dotted path.
///
/// Returns 0.0 on failure.
#[no_mangle]
pub extern "C" fn mdix_builder_get_double(
    builder: *const MdixBuilderHandle,
    path: *const c_char,
) -> f32 {
    clear_last_error();

    let (builder_ref, path_str) =
        match validate_builder_read_args(builder, path, "mdix_builder_get_double") {
            Some(v) => v,
            None => return 0.0,
        };

    match builder_ref.entries.get(path_str) {
        Some(DixValue::Double(d)) => *d as f32,
        Some(DixValue::Float(f))  => *f as f32,
        Some(DixValue::Int(i))    => *i as f32,
        Some(other) => {
            set_last_error(&format!(
                "mdix_builder_get_double('{}'): value is {} not numeric",
                path_str,
                other.type_name()
            ));
            0.0
        }
        None => {
            set_last_error(&format!(
                "mdix_builder_get_double('{}'): key not found",
                path_str
            ));
            0.0
        }
    }
}

/// Get a boolean value from the builder by dotted path.
///
/// Returns false on failure.
#[no_mangle]
pub extern "C" fn mdix_builder_get_bool(
    builder: *const MdixBuilderHandle,
    path: *const c_char,
) -> bool {
    clear_last_error();

    let (builder_ref, path_str) =
        match validate_builder_read_args(builder, path, "mdix_builder_get_bool") {
            Some(v) => v,
            None => return false,
        };

    match builder_ref.entries.get(path_str) {
        Some(DixValue::Bool(b)) => *b,
        Some(other) => {
            set_last_error(&format!(
                "mdix_builder_get_bool('{}'): value is {} not bool",
                path_str,
                other.type_name()
            ));
            false
        }
        None => {
            set_last_error(&format!(
                "mdix_builder_get_bool('{}'): key not found",
                path_str
            ));
            false
        }
    }
}

// =============================================================================
// Builder — persistence
// =============================================================================

/// Save the builder contents to a .mdix file on disk.
///
/// Creates the file and any intermediate directories automatically.
/// Returns true on success, false on failure.
/// On failure, call mdix_get_last_error() for details.
#[no_mangle]
pub extern "C" fn mdix_builder_save(
    builder: *const MdixBuilderHandle,
    path: *const c_char,
) -> bool {
    clear_last_error();

    if builder.is_null() {
        set_last_error("mdix_builder_save: builder is null");
        return false;
    }

    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => {
            set_last_error("mdix_builder_save: path is null or invalid UTF-8");
            return false;
        }
    };

    let entries = unsafe { (*builder).entries.clone() };

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
                "mdix_builder_save: could not create directories: {}",
                e
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
///
/// Returns a heap-allocated C string on success, null on failure.
/// The caller must free the result with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_builder_to_string(builder: *const MdixBuilderHandle) -> *mut c_char {
    clear_last_error();

    if builder.is_null() {
        set_last_error("mdix_builder_to_string: builder is null");
        return std::ptr::null_mut();
    }

    let entries = unsafe { (*builder).entries.clone() };

    let converter = DixConverter::new();
    let ast = match converter.from_hashmap(entries) {
        Ok(a) => a,
        Err(e) => {
            set_last_error(&format!(
                "mdix_builder_to_string: AST conversion failed: {}",
                e
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
// Private helpers
// =============================================================================

/// Validate a read call's handle and path.
///
/// The returned references derive from raw pointers and must be used
/// immediately within the calling function only.
fn validate_read_args<'a>(
    handle: *const MdixHandle,
    path: *const c_char,
    fn_name: &str,
) -> Option<(&'a DixData, &'a str)> {
    if handle.is_null() {
        set_last_error(&format!("{}: handle is null", fn_name));
        return None;
    }

    let path_str = unsafe { c_str_to_str(path) }?;
    if path_str.is_empty() {
        set_last_error(&format!("{}: path is empty", fn_name));
        return None;
    }

    Some((unsafe { &(*handle).data }, path_str))
}

/// Validate a mutable builder call's handle and path.
///
/// The returned references derive from raw pointers and must be used
/// immediately within the calling function only.
fn validate_builder_args<'a>(
    builder: *mut MdixBuilderHandle,
    path: *const c_char,
    fn_name: &str,
) -> Option<(&'a mut MdixBuilderHandle, &'a str)> {
    if builder.is_null() {
        set_last_error(&format!("{}: builder is null", fn_name));
        return None;
    }

    let path_str = unsafe { c_str_to_str(path) }?;
    if path_str.is_empty() {
        set_last_error(&format!("{}: path is empty", fn_name));
        return None;
    }

    Some((unsafe { &mut *builder }, path_str))
}

/// Validate an immutable builder read call's handle and path.
///
/// The returned references derive from raw pointers and must be used
/// immediately within the calling function only.
fn validate_builder_read_args<'a>(
    builder: *const MdixBuilderHandle,
    path: *const c_char,
    fn_name: &str,
) -> Option<(&'a MdixBuilderHandle, &'a str)> {
    if builder.is_null() {
        set_last_error(&format!("{}: builder is null", fn_name));
        return None;
    }

    let path_str = unsafe { c_str_to_str(path) }?;
    if path_str.is_empty() {
        set_last_error(&format!("{}: path is empty", fn_name));
        return None;
    }

    Some((unsafe { &*builder }, path_str))
}
