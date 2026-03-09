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

use std::os::raw::c_char;
use dixscript::Runtime::{
    DixConverter, DixFormatOptions, DixLoader, DixLoadOptions, DixValue,
};

use error::{clear_last_error, get_last_error_ptr, set_last_error};
use handle::{MdixBuilderHandle, MdixHandle};
use string_utils::{
    c_str_to_str, free_c_char, free_c_char_array, static_str_to_c_char, str_to_c_char,
    string_vec_to_c_array,
};

// =============================================================================
// Metadata
// =============================================================================

/// Return the DixScript library version as a null-terminated C string.
///
/// The returned pointer is static — do NOT free it with mdix_free_string.
#[no_mangle]
pub extern "C" fn mdix_version() -> *const c_char {
    static_str_to_c_char("1.0.0")
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
/// Useful for loading .mdix content that was downloaded, bundled as a
/// TextAsset in Unity, or generated at runtime.
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
/// `enc_path`  — path to the .mdix.enc file
/// `key_path`  — path to the .mdix.key file (pass null for auto-detection:
///               the key file is searched next to the .enc file first, then
///               the paths configured in config key_search_paths)
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
/// `enc_path` — path to the .mdix.enc file
/// `password` — decryption password (must match the one used during compilation)
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
/// Useful when both the encrypted payload and the key are fetched from a
/// network source or a secrets manager — no disk access required.
///
/// `encrypted_bytes`    — pointer to the encrypted data buffer
/// `byte_count`         — number of bytes in the buffer
/// `key_file_content`   — full text content of the .mdix.key file
/// `password`           — decryption password, or null if key file mode
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
            set_last_error("mdix_load_encrypted_bytes: key_file_content is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };

    let bytes = unsafe {
        std::slice::from_raw_parts(encrypted_bytes, byte_count as usize)
    };

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
/// After calling this, the handle pointer is invalid — do not use it again.
/// Passing null is safe and does nothing.
#[no_mangle]
pub extern "C" fn mdix_free(handle: *mut MdixHandle) {
    unsafe { MdixHandle::free(handle) };
}

// =============================================================================
// Validity check
// =============================================================================

/// Return true if the handle is non-null and was loaded successfully.
///
/// This is a fast check — it does not re-validate the data.
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
// Data access — typed getters
// =============================================================================

/// Get a string value by dotted path.
///
/// Returns a heap-allocated C string on success, null if the path does not
/// exist or the value is not a string type.
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
/// Returns 0 if the path does not exist or the value cannot be converted.
/// Use mdix_exists() first if you need to distinguish 0 from "not found".
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
/// Returns 0.0 if not found or not convertible.
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
/// Returns 0.0 if not found or not convertible.
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
/// Returns false if not found or not convertible.
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

// =============================================================================
// Key enumeration
// =============================================================================

/// Get the direct child key names under a path prefix.
///
/// `prefix` — dotted path prefix, or null / empty string for top-level keys.
/// `out_count` — receives the number of keys returned. Must not be null.
///
/// Returns a heap-allocated array of null-terminated C strings.
/// The caller must free the result with mdix_free_string_array(result, out_count).
/// Returns null if there are no keys or on error.
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
// Memory management — strings and arrays returned to C#
// =============================================================================

/// Free a string returned by mdix_get_string or mdix_builder_to_mdix_string.
///
/// Passing null is safe. Do NOT call this on the pointer from mdix_version()
/// — that pointer is static and must not be freed.
#[no_mangle]
pub extern "C" fn mdix_free_string(s: *mut c_char) {
    unsafe { free_c_char(s) };
}

/// Free an array of strings returned by mdix_get_keys.
///
/// `arr`   — the pointer returned by mdix_get_keys
/// `count` — the value written to out_count by mdix_get_keys
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
// Builder — create and write .mdix save data at runtime
// =============================================================================

/// Create a new empty builder handle.
///
/// The caller must free it with mdix_builder_free() when done.
#[no_mangle]
pub extern "C" fn mdix_builder_new() -> *mut MdixBuilderHandle {
    clear_last_error();
    MdixBuilderHandle::new()
}

/// Free a builder handle.
///
/// Passing null is safe.
#[no_mangle]
pub extern "C" fn mdix_builder_free(builder: *mut MdixBuilderHandle) {
    unsafe { MdixBuilderHandle::free(builder) };
}

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

    let (builder_ref, path_str) = match validate_builder_args(builder, path, "mdix_builder_set_string") {
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

    let (builder_ref, path_str) = match validate_builder_args(builder, path, "mdix_builder_set_int") {
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

    let (builder_ref, path_str) = match validate_builder_args(builder, path, "mdix_builder_set_float") {
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

    let (builder_ref, path_str) = match validate_builder_args(builder, path, "mdix_builder_set_double") {
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

    let (builder_ref, path_str) = match validate_builder_args(builder, path, "mdix_builder_set_bool") {
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

    let (builder_ref, path_str) = match validate_builder_args(builder, path, "mdix_builder_remove") {
        Some(v) => v,
        None => return false,
    };

    builder_ref.entries.remove(path_str).is_some()
}

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

    // Create parent directories if needed.
    if let Some(parent) = std::path::Path::new(path_str).parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            set_last_error(&format!("mdix_builder_save: could not create directories: {}", e));
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
pub extern "C" fn mdix_builder_to_string(
    builder: *const MdixBuilderHandle,
) -> *mut c_char {
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
            set_last_error(&format!("mdix_builder_to_string: AST conversion failed: {}", e));
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

/// Return the number of entries currently in the builder.
///
/// Returns -1 if the builder is null.
#[no_mangle]
pub extern "C" fn mdix_builder_entry_count(builder: *const MdixBuilderHandle) -> i32 {
    if builder.is_null() {
        return -1;
    }
    unsafe { (*builder).entries.len() as i32 }
}

// =============================================================================
// Private helpers
// =============================================================================

/// Validate a read call's handle and path, returning references or None on error.
fn validate_read_args<'a>(
    handle: *const MdixHandle,
    path: *const c_char,
    fn_name: &str,
) -> Option<(&'a dixscript::Runtime::DixData, &'a str)> {
    if handle.is_null() {
        set_last_error(&format!("{}: handle is null", fn_name));
        return None;
    }

    let path_str = unsafe { c_str_to_str(path) }?;
    if path_str.is_empty() {
        set_last_error(&format!("{}: path is empty", fn_name));
        return None;
    }

    let data = unsafe { &(*handle).data };
    Some((data, path_str))
}

/// Validate a builder call's handle and path, returning mutable references or None on error.
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

    let builder_ref = unsafe { &mut *builder };
    Some((builder_ref, path_str))
  }
