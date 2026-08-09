mod error;
mod handle;
mod merge;
mod string_utils;

use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use std::sync::OnceLock;
use std::collections::HashMap;

use dixscript::Runtime::{
    DixCompactor, DixConverter, DixFormatOptions, DixLoader, DixLoadOptions, DixValue,
    HotReloadWatcher,
};

use error::{clear_last_error, get_last_error_ptr, set_last_error};
use handle::{MdixBuilderHandle, MdixHandle, MdixWatcherHandle};
use merge::{read_source_array, run_merge};
use string_utils::{
    c_str_to_str, free_c_char, free_c_char_array, str_to_c_char, string_vec_to_c_array,
};


// =============================================================================
// TYPE DISCRIMINANTS
// =============================================================================

/// Type discriminants returned by mdix_get_type().
/// Values are stable — the C# MdixValueType enum MUST match exactly.
/// Numeric types are contiguous: Int=2, Long=3, Float=4, Double=5.
#[repr(i32)]
pub enum MdixType {
    Unknown   = -1,
    Null      =  0,
    Bool      =  1,
    Int       =  2,
    Long      =  3,
    Float     =  4,
    Double    =  5,
    String    =  6,
    Date      =  7,
    Timestamp =  8,
    HexColor  =  9,
    Blob      = 10,
    Regex     = 11,
    Array     = 12,
    Object    = 13,
    Tuple     = 14,
    Enum      = 15,
}

/// Output format mode for mdix_to_mdix() and mdix_format_source().
#[repr(i32)]
pub enum MdixFormatMode {
    Default  = 0,
    Pretty   = 1,
    Compact  = 2,
    Minified = 3,
}

/// How to resolve a key defined by more than one source in mdix_merge_sources()
/// / mdix_merge_sources_weighted(). Mirrors dixscript::Runtime::MdixMergeStrategy
/// — kept as a separate, explicitly #[repr(i32)] local type (same pattern as
/// MdixType / MdixFormatMode above) since the core enum's repr is not
/// FFI-guaranteed and lives in a different crate. See merge.rs's to_core().
#[repr(i32)]
pub enum MdixMergeStrategy {
    /// Each source's weight decides the winner; equal weights fall back to
    /// the lower-indexed (primary) source. This is what mdix_merge_sources()
    /// (no explicit weights) effectively resolves to, since it auto-assigns
    /// descending weights — source 0 gets 1.0, the last source gets ~0.0.
    WeightedPriority = 0,
    /// The lower-indexed source always wins, regardless of weight.
    PrimaryWins = 1,
    /// The higher-indexed source always wins, regardless of weight.
    SecondaryWins = 2,
    /// Any key defined by more than one source is a hard error — the merge
    /// fails and mdix_get_last_error() reports every conflicting path.
    ThrowOnConflict = 3,
}

/// How to combine two array-valued entries (GroupArray, or an array-valued
/// SimpleProperty) that share a path across sources.
#[repr(i32)]
pub enum ArrayMergeStrategy {
    /// The winning source's array entirely replaces the losing one's.
    Replace = 0,
    /// Both arrays are concatenated, winner's items first.
    Concat = 1,
    /// Concatenated (winner first), with exact-duplicate primitive values
    /// removed. Complex values (objects, nested arrays) are never deduped.
    ConcatDedup = 2,
}

// =============================================================================
// INTERNAL CASTING HELPERS
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
unsafe fn as_watcher<'a>(ptr: *const c_void) -> Option<&'a MdixWatcherHandle> {
    if ptr.is_null() { None } else { Some(&*(ptr as *const MdixWatcherHandle)) }
}

#[inline]
unsafe fn as_watcher_mut<'a>(ptr: *mut c_void) -> Option<&'a mut MdixWatcherHandle> {
    if ptr.is_null() { None } else { Some(&mut *(ptr as *mut MdixWatcherHandle)) }
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
// INTERNAL UTILITIES
// =============================================================================

/// Strip synthetic indexed array keys from a hashmap before export or round-trip.
///
/// DixData.to_hashmap() stores both the parent "enemies" Array AND every
/// indexed item ("enemies[0]", "enemies[0].name", etc.) for fast O(1)
/// runtime access. Feeding that map through from_hashmap produces duplicate
/// or malformed .mdix output. This filter removes any key containing '['.
fn strip_indexed_keys(raw: HashMap<String, DixValue>) -> HashMap<String, DixValue> {
    raw.into_iter()
        .filter(|(k, _)| !k.contains('['))
        .collect()
}

/// Wildcard pattern matching over the flat DixData hashmap.
///
/// `*` matches exactly one dotted path segment.
///
/// Examples:
///   "server.*"         → server.host, server.port, server.ssl
///   "config.*"         → config.version, config.author, ...
///
/// LIMITATION: array-indexed keys use bracket notation (enemies[0].name),
/// not dotted segments, so they do NOT match dot-wildcard patterns.
/// For array item access use mdix_get_array_length + indexed paths like
/// enemies[{i}].name from the C# caller.
fn select_by_pattern(data: &dixscript::Runtime::DixData, pattern: &str) -> Vec<DixValue> {
    let pattern_segs: Vec<&str> = pattern.split('.').collect();
    data.to_hashmap()
        .into_iter()
        .filter(|(key, _)| {
            let key_segs: Vec<&str> = key.split('.').collect();
            key_segs.len() == pattern_segs.len()
                && key_segs.iter().zip(pattern_segs.iter())
                    .all(|(k, p)| *p == "*" || *k == *p)
        })
        .map(|(_, v)| v)
        .collect()
}

// =============================================================================
// METADATA
// =============================================================================

/// Returns a static pointer to the DixScript FFI version string.
/// Do NOT pass this pointer to mdix_free_string() — it is a static allocation.
#[no_mangle]
pub extern "C" fn mdix_version() -> *const c_char {
    static VERSION_PTR: OnceLock<CString> = OnceLock::new();
    VERSION_PTR
        .get_or_init(|| CString::new("1.0.0").expect("version string contained null byte"))
        .as_ptr()
}

// =============================================================================
// HANDLE LIFECYCLE — plain .mdix files
// =============================================================================

/// Load and compile a .mdix source file from disk.
/// Returns an opaque handle on success, null on failure (check mdix_get_last_error).
/// Caller must free with mdix_free when done.
#[no_mangle]
pub extern "C" fn mdix_load(path: *const c_char) -> *mut c_void {
    clear_last_error();
    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => { set_last_error("mdix_load: path is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    let loader = DixLoader::new();
    match loader.load_text(path_str, &DixLoadOptions::new()) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e)   => { set_last_error(&format!("mdix_load: {}", e)); std::ptr::null_mut() }
    }
}

/// Compile and load .mdix source from an in-memory string.
/// Useful for Unity TextAssets and embedded configs.
/// Returns an opaque handle on success, null on failure.
/// Caller must free with mdix_free when done.
#[no_mangle]
pub extern "C" fn mdix_load_str(source: *const c_char) -> *mut c_void {
    clear_last_error();
    let source_str = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None => { set_last_error("mdix_load_str: source is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    let loader = DixLoader::new();
    match loader.load_from_str(source_str, &DixLoadOptions::new()) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e)   => { set_last_error(&format!("mdix_load_str: {}", e)); std::ptr::null_mut() }
    }
}

/// Free a handle created by any mdix_load* function.
/// Passing null is safe. Do not call twice on the same pointer.
#[no_mangle]
pub extern "C" fn mdix_free(handle: *mut c_void) {
    unsafe { MdixHandle::free(handle as *mut MdixHandle) };
}

// =============================================================================
// HANDLE LIFECYCLE — encrypted files
// =============================================================================

/// Load an encrypted .mdix.enc file using a .mdix.key file for key resolution.
/// Pass null for key_path to auto-detect the key file next to the enc file.
#[no_mangle]
pub extern "C" fn mdix_load_encrypted(
    enc_path: *const c_char,
    key_path: *const c_char,
) -> *mut c_void {
    clear_last_error();
    let enc_str = match unsafe { c_str_to_str(enc_path) } {
        Some(s) => s,
        None => { set_last_error("mdix_load_encrypted: enc_path is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    let mut opts = DixLoadOptions::new();
    if let Some(kp) = unsafe { c_str_to_str(key_path) } {
        opts.key_file_path = Some(kp.to_string());
    }
    let loader = DixLoader::new();
    match loader.load_encrypted(enc_str, &opts) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e)   => { set_last_error(&format!("mdix_load_encrypted: {}", e)); std::ptr::null_mut() }
    }
}

/// Load an encrypted .mdix.enc file using a password for key derivation.
#[no_mangle]
pub extern "C" fn mdix_load_encrypted_password(
    enc_path: *const c_char,
    password: *const c_char,
) -> *mut c_void {
    clear_last_error();
    let enc_str = match unsafe { c_str_to_str(enc_path) } {
        Some(s) => s,
        None => { set_last_error("mdix_load_encrypted_password: enc_path is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    let pw_str = match unsafe { c_str_to_str(password) } {
        Some(s) => s,
        None => { set_last_error("mdix_load_encrypted_password: password is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    let opts   = DixLoadOptions::with_password(pw_str);
    let loader = DixLoader::new();
    match loader.load_encrypted(enc_str, &opts) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e)   => { set_last_error(&format!("mdix_load_encrypted_password: {}", e)); std::ptr::null_mut() }
    }
}

/// Load encrypted data from raw bytes with key file content provided as a string.
/// Intended for platforms without filesystem access (asset bundles, network delivery).
/// Pass null for password if the key file does not use password mode.
#[no_mangle]
pub extern "C" fn mdix_load_encrypted_bytes(
    encrypted_bytes:  *const u8,
    byte_count:       i32,
    key_file_content: *const c_char,
    password:         *const c_char,
) -> *mut c_void {
    clear_last_error();
    if encrypted_bytes.is_null() || byte_count <= 0 {
        set_last_error("mdix_load_encrypted_bytes: encrypted_bytes is null or empty");
        return std::ptr::null_mut();
    }
    let key_content = match unsafe { c_str_to_str(key_file_content) } {
        Some(s) => s,
        None => { set_last_error("mdix_load_encrypted_bytes: key_file_content is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    let bytes    = unsafe { std::slice::from_raw_parts(encrypted_bytes, byte_count as usize) };
    let mut opts = DixLoadOptions::new();
    if let Some(pw) = unsafe { c_str_to_str(password) } {
        opts.password = Some(pw.to_string());
    }
    let loader = DixLoader::new();
    match loader.load_from_encrypted_bytes(bytes, key_content, &opts) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e)   => { set_last_error(&format!("mdix_load_encrypted_bytes: {}", e)); std::ptr::null_mut() }
    }
}

// =============================================================================
// VALIDITY AND METADATA
// =============================================================================

/// Returns true if the handle is non-null (i.e. load succeeded).
#[no_mangle]
pub extern "C" fn mdix_is_valid(handle: *const c_void) -> bool { !handle.is_null() }

/// Returns the number of entries in the flat data map, or -1 if handle is null.
#[no_mangle]
pub extern "C" fn mdix_entry_count(handle: *const c_void) -> i32 {
    match unsafe { as_handle(handle) } {
        Some(h) => h.data.entry_count() as i32,
        None    => -1,
    }
}

/// Returns true if the source file was encrypted.
#[no_mangle]
pub extern "C" fn mdix_is_encrypted(handle: *const c_void) -> bool {
    unsafe { as_handle(handle) }.map(|h| h.data.is_encrypted).unwrap_or(false)
}

/// Returns true if the source file was compressed.
#[no_mangle]
pub extern "C" fn mdix_is_compressed(handle: *const c_void) -> bool {
    unsafe { as_handle(handle) }.map(|h| h.data.is_compressed).unwrap_or(false)
}

/// Returns the runtime version string from the loaded DixData.
/// Caller must free the returned string with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_get_loaded_version(handle: *const c_void) -> *mut c_char {
    clear_last_error();
    match unsafe { as_handle(handle) } {
        Some(h) => str_to_c_char(h.data.version.clone()),
        None    => { set_last_error("mdix_get_loaded_version: handle is null"); std::ptr::null_mut() }
    }
}

/// Returns all keys in the flat data map as a null-terminated C string array.
/// Writes the count to out_count. Caller must free with mdix_free_string_array.
/// Includes synthetic indexed keys (tags[0], server.host, etc.).
#[no_mangle]
pub extern "C" fn mdix_get_all_keys(
    handle:    *const c_void,
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
        None    => { set_last_error("mdix_get_all_keys: handle is null"); return std::ptr::null_mut(); }
    };
    let mut keys: Vec<String> = h.data.to_hashmap().into_keys().collect();
    keys.sort_unstable();
    string_vec_to_c_array(keys, out_count)
}

/// Read a value from the @CONFIG section by key (e.g. "version", "author").
/// Returns null if the section is absent or the key does not exist.
/// Caller must free the returned string with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_get_config_value(
    handle: *const c_void,
    key:    *const c_char,
) -> *mut c_char {
    clear_last_error();
    let (h, key_str) = match validate_read(handle, key, "mdix_get_config_value") {
        Some(v) => v,
        None    => return std::ptr::null_mut(),
    };
    match &h.data.config {
        Some(cfg) => match cfg.get(key_str) {
            Some(val) => str_to_c_char(val.clone()),
            None => {
                set_last_error(&format!(
                    "mdix_get_config_value('{}'): key not found in @CONFIG section", key_str
                ));
                std::ptr::null_mut()
            }
        },
        None => {
            set_last_error("mdix_get_config_value: file has no @CONFIG section");
            std::ptr::null_mut()
        }
    }
}

// =============================================================================
// VALIDATION
// =============================================================================

/// Validate .mdix source text without loading the result.
///
/// Runs the full compilation pipeline (tokenize → parse → semantic analysis →
/// AST enhance → value resolve). Returns true on success; on failure, the
/// error detail is available via mdix_get_last_error().
///
/// Useful for Unity editor tooling — validate before shipping.
#[no_mangle]
pub extern "C" fn mdix_validate(source: *const c_char) -> bool {
    clear_last_error();
    let source_str = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None => { set_last_error("mdix_validate: source is null or invalid UTF-8"); return false; }
    };
    if source_str.trim().is_empty() {
        set_last_error("mdix_validate: source is empty");
        return false;
    }
    let loader = DixLoader::new();
    match loader.load_from_str(source_str, &DixLoadOptions::new()) {
        Ok(_)  => true,
        Err(e) => { set_last_error(&e); false }
    }
}

// =============================================================================
// TYPE INSPECTION
// =============================================================================

/// Returns the MdixType discriminant for the value at path, or MdixType::Unknown
/// if the path does not exist or the handle is null.
#[no_mangle]
pub extern "C" fn mdix_get_type(handle: *const c_void, path: *const c_char) -> MdixType {
    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None    => return MdixType::Unknown,
    };
    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None    => return MdixType::Unknown,
    };
    match h.data.get_value(path_str) {
        None                         => MdixType::Unknown,
        Some(DixValue::Null)         => MdixType::Null,
        Some(DixValue::Bool(_))      => MdixType::Bool,
        Some(DixValue::Int(_))       => MdixType::Int,
        Some(DixValue::Long(_))      => MdixType::Long,
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

/// Returns the element count of an Array or Tuple at path, or -1 if the path
/// does not exist, is not a collection, or the handle is null.
#[no_mangle]
pub extern "C" fn mdix_get_array_length(handle: *const c_void, path: *const c_char) -> i32 {
    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None    => return -1,
    };
    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None    => return -1,
    };
    match h.data.get_value(path_str) {
        Some(DixValue::Array(arr))  => arr.len() as i32,
        Some(DixValue::Tuple(items)) => items.len() as i32,
        _                            => -1,
    }
}

// =============================================================================
// DATA ACCESS — typed getters
// =============================================================================

/// Get a string value at path.
/// Also returns Date, Timestamp, HexColor, Blob, and Regex values as strings.
/// Caller must free the returned string with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_get_string(handle: *const c_void, path: *const c_char) -> *mut c_char {
    clear_last_error();
    let (h, path_str) = match validate_read(handle, path, "mdix_get_string") {
        Some(v) => v,
        None    => return std::ptr::null_mut(),
    };
    match h.data.get::<String>(path_str) {
        Ok(s)  => str_to_c_char(s),
        Err(e) => { set_last_error(&format!("mdix_get_string('{}'): {}", path_str, e)); std::ptr::null_mut() }
    }
}

/// Get a 32-bit integer at path. Also works on Enum paths (returns the integer value).
/// Returns 0 on failure; check mdix_get_last_error() to distinguish from a real 0.
#[no_mangle]
pub extern "C" fn mdix_get_int(handle: *const c_void, path: *const c_char) -> i32 {
    clear_last_error();
    let (h, path_str) = match validate_read(handle, path, "mdix_get_int") {
        Some(v) => v,
        None    => return 0,
    };
    match h.data.get::<i32>(path_str) {
        Ok(v)  => v,
        Err(e) => { set_last_error(&format!("mdix_get_int('{}'): {}", path_str, e)); 0 }
    }
}

/// Get a 64-bit integer at path. Also accepts Int values (widened without loss).
/// Returns 0 on failure.
#[no_mangle]
pub extern "C" fn mdix_get_long(handle: *const c_void, path: *const c_char) -> i64 {
    clear_last_error();
    let (h, path_str) = match validate_read(handle, path, "mdix_get_long") {
        Some(v) => v,
        None    => return 0,
    };
    match h.data.get::<i64>(path_str) {
        Ok(v)  => v,
        Err(e) => { set_last_error(&format!("mdix_get_long('{}'): {}", path_str, e)); 0 }
    }
}

/// Get a 32-bit float at path. Returns 0.0 on failure.
#[no_mangle]
pub extern "C" fn mdix_get_float(handle: *const c_void, path: *const c_char) -> f32 {
    clear_last_error();
    let (h, path_str) = match validate_read(handle, path, "mdix_get_float") {
        Some(v) => v,
        None    => return 0.0,
    };
    match h.data.get::<f64>(path_str) {
        Ok(v)  => v as f32,
        Err(e) => { set_last_error(&format!("mdix_get_float('{}'): {}", path_str, e)); 0.0 }
    }
}

/// Get a 64-bit double at path. Returns 0.0 on failure.
#[no_mangle]
pub extern "C" fn mdix_get_double(handle: *const c_void, path: *const c_char) -> f64 {
    clear_last_error();
    let (h, path_str) = match validate_read(handle, path, "mdix_get_double") {
        Some(v) => v,
        None    => return 0.0,
    };
    match h.data.get::<f64>(path_str) {
        Ok(v)  => v,
        Err(e) => { set_last_error(&format!("mdix_get_double('{}'): {}", path_str, e)); 0.0 }
    }
}

/// Get a boolean at path. Returns false on failure.
#[no_mangle]
pub extern "C" fn mdix_get_bool(handle: *const c_void, path: *const c_char) -> bool {
    clear_last_error();
    let (h, path_str) = match validate_read(handle, path, "mdix_get_bool") {
        Some(v) => v,
        None    => return false,
    };
    match h.data.get::<bool>(path_str) {
        Ok(v)  => v,
        Err(e) => { set_last_error(&format!("mdix_get_bool('{}'): {}", path_str, e)); false }
    }
}

// =============================================================================
// ENUM ACCESS
// =============================================================================

/// Returns the enum type name at path (e.g. "WeaponClass").
/// Returns null if the path does not exist or is not an enum.
/// Caller must free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_get_enum_name(handle: *const c_void, path: *const c_char) -> *mut c_char {
    clear_last_error();
    let (h, path_str) = match validate_read(handle, path, "mdix_get_enum_name") {
        Some(v) => v,
        None    => return std::ptr::null_mut(),
    };
    match h.data.get_value(path_str) {
        Some(DixValue::Enum { enum_name, .. }) => str_to_c_char(enum_name.clone()),
        Some(_) => { set_last_error(&format!("mdix_get_enum_name('{}'): not an enum", path_str)); std::ptr::null_mut() }
        None    => { set_last_error(&format!("mdix_get_enum_name('{}'): path not found", path_str)); std::ptr::null_mut() }
    }
}

/// Returns the enum field name at path (e.g. "ASSAULT_RIFLE").
/// Returns null if the path does not exist or is not an enum.
/// Caller must free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_get_enum_field(handle: *const c_void, path: *const c_char) -> *mut c_char {
    clear_last_error();
    let (h, path_str) = match validate_read(handle, path, "mdix_get_enum_field") {
        Some(v) => v,
        None    => return std::ptr::null_mut(),
    };
    match h.data.get_value(path_str) {
        Some(DixValue::Enum { field_name, .. }) => str_to_c_char(field_name.clone()),
        Some(_) => { set_last_error(&format!("mdix_get_enum_field('{}'): not an enum", path_str)); std::ptr::null_mut() }
        None    => { set_last_error(&format!("mdix_get_enum_field('{}'): path not found", path_str)); std::ptr::null_mut() }
    }
}

// =============================================================================
// JSON ESCAPE HATCH
// =============================================================================

/// Serialize the raw DixValue at path to a JSON string.
///
/// Useful for reading Object or Array values wholesale into C# for further
/// processing. Note: DixValue::Enum serializes as {"enum_name":...,"value":N}
/// via serde — use mdix_get_int on enum paths to get just the integer.
/// Caller must free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_get_json(handle: *const c_void, path: *const c_char) -> *mut c_char {
    clear_last_error();
    let (h, path_str) = match validate_read(handle, path, "mdix_get_json") {
        Some(v) => v,
        None    => return std::ptr::null_mut(),
    };
    match h.data.get_value(path_str) {
        None        => { set_last_error(&format!("mdix_get_json('{}'): path not found", path_str)); std::ptr::null_mut() }
        Some(value) => match serde_json::to_string(value) {
            Ok(json) => str_to_c_char(json),
            Err(e)   => { set_last_error(&format!("mdix_get_json('{}'): {}", path_str, e)); std::ptr::null_mut() }
        },
    }
}

// =============================================================================
// WILDCARD SELECTION
// =============================================================================

/// Select all values matching a dotted path pattern and return them as a JSON array.
///
/// Use `*` as a wildcard for a single path segment:
///   "server.*"   → all direct children of the server table property
///   "weapons.*"  → all direct children of the weapons object
///
/// LIMITATION: bracket-indexed paths (enemies[0].name) do not match dot
/// wildcards. Use mdix_get_array_length + mdix_get_* with explicit indexed
/// paths for array item access from the C# side.
///
/// Caller must free the returned string with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_select_many_as_json(
    handle:  *const c_void,
    pattern: *const c_char,
) -> *mut c_char {
    clear_last_error();
    let (h, pattern_str) = match validate_read(handle, pattern, "mdix_select_many_as_json") {
        Some(v) => v,
        None    => return std::ptr::null_mut(),
    };
    let matches = select_by_pattern(&h.data, pattern_str);
    match serde_json::to_string(&matches) {
        Ok(json) => str_to_c_char(json),
        Err(e)   => {
            set_last_error(&format!("mdix_select_many_as_json: serialization failed: {}", e));
            std::ptr::null_mut()
        }
    }
}

// =============================================================================
// EXISTENCE AND ENUMERATION
// =============================================================================

/// Returns true if a value exists at path.
#[no_mangle]
pub extern "C" fn mdix_exists(handle: *const c_void, path: *const c_char) -> bool {
    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None    => return false,
    };
    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None    => return false,
    };
    h.data.exists(path_str)
}

/// Returns the direct child key names under prefix as a C string array.
/// Pass an empty string for top-level keys.
/// Writes the count to out_count. Caller must free with mdix_free_string_array().
#[no_mangle]
pub extern "C" fn mdix_get_keys(
    handle:    *const c_void,
    prefix:    *const c_char,
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
        None    => { set_last_error("mdix_get_keys: handle is null"); return std::ptr::null_mut(); }
    };
    let prefix_str = unsafe { c_str_to_str(prefix) }.unwrap_or("");
    let keys       = h.data.get_keys(prefix_str);
    string_vec_to_c_array(keys, out_count)
}

// =============================================================================
// MEMORY MANAGEMENT
// =============================================================================

/// Free a C string returned by any mdix getter function.
/// Do NOT call on the static pointer from mdix_version(). Passing null is safe.
#[no_mangle]
pub extern "C" fn mdix_free_string(s: *mut c_char) {
    unsafe { free_c_char(s) };
}

/// Free a string array returned by mdix_get_keys() or mdix_get_all_keys().
/// count must match the value written to out_count by the original call.
#[no_mangle]
pub extern "C" fn mdix_free_string_array(arr: *mut *mut c_char, count: i32) {
    unsafe { free_c_char_array(arr, count) };
}

// =============================================================================
// ERROR REPORTING
// =============================================================================

/// Returns a pointer to the last error string set on this thread, or null if
/// no error is pending. Valid only until the next mdix_* call on this thread.
/// Do NOT free this pointer.
#[no_mangle]
pub extern "C" fn mdix_get_last_error() -> *const c_char { get_last_error_ptr() }

/// Clear the pending error on this thread.
#[no_mangle]
pub extern "C" fn mdix_clear_error() { clear_last_error(); }

// =============================================================================
// CONVERSION — database export
// =============================================================================

/// Export the loaded data as a JSON string.
/// Enum values are emitted as plain integers (not as JSON objects).
/// Pass indented=true for pretty-printed output.
/// Caller must free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_to_json(handle: *const c_void, indented: bool) -> *mut c_char {
    clear_last_error();
    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None    => { set_last_error("mdix_to_json: handle is null"); return std::ptr::null_mut(); }
    };
    let entries   = strip_indexed_keys(h.data.to_hashmap());
    let converter = DixConverter::new();
    let ast       = match converter.from_hashmap(entries) {
        Ok(a)  => a,
        Err(e) => { set_last_error(&format!("mdix_to_json: AST conversion failed: {}", e)); return std::ptr::null_mut(); }
    };
    match converter.to_json(&ast, indented) {
        Ok(s)  => str_to_c_char(s),
        Err(e) => { set_last_error(&format!("mdix_to_json: {}", e)); std::ptr::null_mut() }
    }
}

/// Export the loaded data as .mdix source text.
/// Caller must free with mdix_free_string().
///
/// FIX: previously returned *mut c_void — corrected to *mut c_char.
#[no_mangle]
pub extern "C" fn mdix_to_mdix(handle: *const c_void, mode: MdixFormatMode) -> *mut c_char {
    clear_last_error();
    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None    => { set_last_error("mdix_to_mdix: handle is null"); return std::ptr::null_mut(); }
    };
    let entries   = strip_indexed_keys(h.data.to_hashmap());
    let converter = DixConverter::new();
    let ast       = match converter.from_hashmap(entries) {
        Ok(a)  => a,
        Err(e) => { set_last_error(&format!("mdix_to_mdix: AST conversion failed: {}", e)); return std::ptr::null_mut(); }
    };
    let options = format_mode_to_options(mode);
    match converter.to_mdix(&ast, Some(&options)) {
        Ok(s)  => str_to_c_char(s),
        Err(e) => { set_last_error(&format!("mdix_to_mdix: {}", e)); std::ptr::null_mut() }
    }
}

/// Export the loaded data as TOML.
/// Caller must free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_to_toml(handle: *const c_void) -> *mut c_char {
    clear_last_error();
    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None    => { set_last_error("mdix_to_toml: handle is null"); return std::ptr::null_mut(); }
    };
    let entries   = strip_indexed_keys(h.data.to_hashmap());
    let converter = DixConverter::new();
    let ast       = match converter.from_hashmap(entries) {
        Ok(a)  => a,
        Err(e) => { set_last_error(&format!("mdix_to_toml: AST conversion failed: {}", e)); return std::ptr::null_mut(); }
    };
    match converter.to_toml(&ast) {
        Ok(s)  => str_to_c_char(s),
        Err(e) => { set_last_error(&format!("mdix_to_toml: {}", e)); std::ptr::null_mut() }
    }
}

// =============================================================================
// SOURCE TEXT FORMATTING
// =============================================================================

/// Format .mdix source text according to the given mode.
/// Minified mode removes all unnecessary whitespace.
/// Compact mode collapses blank lines and strips trailing whitespace.
/// Caller must free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_format_source(source: *const c_char, mode: MdixFormatMode) -> *mut c_char {
    clear_last_error();
    let source_str = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None    => { set_last_error("mdix_format_source: source is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    let result = match mode {
        MdixFormatMode::Minified => DixCompactor::minify(source_str),
        _                        => DixCompactor::compact(source_str),
    };
    str_to_c_char(result)
}

/// Minify .mdix source — remove all unnecessary whitespace using the tokenizer.
/// Caller must free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_minify_source(source: *const c_char) -> *mut c_char {
    clear_last_error();
    let source_str = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None    => { set_last_error("mdix_minify_source: source is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    str_to_c_char(DixCompactor::minify(source_str))
}

/// Compact .mdix source — strip trailing whitespace and collapse blank lines.
/// Preserves indentation and code structure. Lighter than minify.
/// Caller must free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_compact_source(source: *const c_char) -> *mut c_char {
    clear_last_error();
    let source_str = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None    => { set_last_error("mdix_compact_source: source is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    str_to_c_char(DixCompactor::compact(source_str))
}

/// Strip all comments from .mdix source. Content inside string literals is preserved.
/// Caller must free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_strip_comments(source: *const c_char) -> *mut c_char {
    clear_last_error();
    let source_str = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None    => { set_last_error("mdix_strip_comments: source is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    str_to_c_char(DixCompactor::remove_comments(source_str))
}

// =============================================================================
// BUILDER — lifecycle
// =============================================================================

/// Create an empty builder for constructing new save data.
/// Caller must free with mdix_builder_free when done.
#[no_mangle]
pub extern "C" fn mdix_builder_new() -> *mut c_void {
    clear_last_error();
    MdixBuilderHandle::new() as *mut c_void
}

/// Create a builder pre-populated from an existing loaded handle.
///
/// Copies all root-level structural values from the handle into the builder.
/// Synthetic indexed children (tags[0], server.host, etc.) are stripped so
/// only values that map to valid .mdix identifiers remain editable.
///
/// Typical use: load a save file → fork into builder → change a few fields
/// → mdix_builder_save back to disk. The original handle remains valid.
///
/// Caller must free the returned builder with mdix_builder_free when done.
#[no_mangle]
pub extern "C" fn mdix_builder_from_handle(handle: *const c_void) -> *mut c_void {
    clear_last_error();
    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None    => { set_last_error("mdix_builder_from_handle: handle is null"); return std::ptr::null_mut(); }
    };
    let structural = h.data.to_structural_hashmap();
    MdixBuilderHandle::from_flat_map(structural) as *mut c_void
}

/// Free a builder created by mdix_builder_new or mdix_builder_from_handle.
/// Passing null is safe. Do not call twice on the same pointer.
#[no_mangle]
pub extern "C" fn mdix_builder_free(builder: *mut c_void) {
    unsafe { MdixBuilderHandle::free(builder as *mut MdixBuilderHandle) };
}

/// Returns the number of entries currently in the builder, or -1 if null.
#[no_mangle]
pub extern "C" fn mdix_builder_entry_count(builder: *const c_void) -> i32 {
    match unsafe { as_builder(builder) } {
        Some(b) => b.entries.len() as i32,
        None    => -1,
    }
}

/// Clear all entries from the builder. Returns true on success.
#[no_mangle]
pub extern "C" fn mdix_builder_clear(builder: *mut c_void) -> bool {
    clear_last_error();
    match unsafe { as_builder_mut(builder) } {
        Some(b) => { b.entries.clear(); true }
        None    => { set_last_error("mdix_builder_clear: builder is null"); false }
    }
}

// =============================================================================
// BUILDER — write
// =============================================================================

#[no_mangle]
pub extern "C" fn mdix_builder_set_string(
    builder: *mut c_void, path: *const c_char, value: *const c_char,
) -> bool {
    clear_last_error();
    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_set_string") {
        Some(v) => v,
        None    => return false,
    };
    let value_str = match unsafe { c_str_to_str(value) } {
        Some(s) => s.to_string(),
        None    => { set_last_error("mdix_builder_set_string: value is null or invalid UTF-8"); return false; }
    };
    b.entries.insert(path_str.to_string(), DixValue::String(value_str));
    true
}

#[no_mangle]
pub extern "C" fn mdix_builder_set_int(
    builder: *mut c_void, path: *const c_char, value: i32,
) -> bool {
    clear_last_error();
    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_set_int") {
        Some(v) => v,
        None    => return false,
    };
    b.entries.insert(path_str.to_string(), DixValue::Int(value));
    true
}

#[no_mangle]
pub extern "C" fn mdix_builder_set_long(
    builder: *mut c_void, path: *const c_char, value: i64,
) -> bool {
    clear_last_error();
    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_set_long") {
        Some(v) => v,
        None    => return false,
    };
    b.entries.insert(path_str.to_string(), DixValue::Long(value));
    true
}

#[no_mangle]
pub extern "C" fn mdix_builder_set_float(
    builder: *mut c_void, path: *const c_char, value: f32,
) -> bool {
    clear_last_error();
    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_set_float") {
        Some(v) => v,
        None    => return false,
    };
    b.entries.insert(path_str.to_string(), DixValue::Float(value));
    true
}

#[no_mangle]
pub extern "C" fn mdix_builder_set_double(
    builder: *mut c_void, path: *const c_char, value: f64,
) -> bool {
    clear_last_error();
    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_set_double") {
        Some(v) => v,
        None    => return false,
    };
    b.entries.insert(path_str.to_string(), DixValue::Double(value));
    true
}

#[no_mangle]
pub extern "C" fn mdix_builder_set_bool(
    builder: *mut c_void, path: *const c_char, value: bool,
) -> bool {
    clear_last_error();
    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_set_bool") {
        Some(v) => v,
        None    => return false,
    };
    b.entries.insert(path_str.to_string(), DixValue::Bool(value));
    true
}

/// Remove the entry at path from the builder. Returns true if it existed.
#[no_mangle]
pub extern "C" fn mdix_builder_remove(builder: *mut c_void, path: *const c_char) -> bool {
    clear_last_error();
    let (b, path_str) = match validate_builder_write(builder, path, "mdix_builder_remove") {
        Some(v) => v,
        None    => return false,
    };
    b.entries.remove(path_str).is_some()
}

// =============================================================================
// BUILDER — read back
// =============================================================================

/// Returns true if the builder contains an entry at path.
#[no_mangle]
pub extern "C" fn mdix_builder_has_key(builder: *const c_void, path: *const c_char) -> bool {
    let b = match unsafe { as_builder(builder) } {
        Some(b) => b,
        None    => return false,
    };
    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None    => return false,
    };
    b.entries.contains_key(path_str)
}

#[no_mangle]
pub extern "C" fn mdix_builder_get_string(
    builder: *const c_void, path: *const c_char,
) -> *mut c_char {
    clear_last_error();
    let (b, path_str) = match validate_builder_read(builder, path, "mdix_builder_get_string") {
        Some(v) => v,
        None    => return std::ptr::null_mut(),
    };
    match b.entries.get(path_str) {
        Some(DixValue::String(s)) => str_to_c_char(s.clone()),
        Some(other) => { set_last_error(&format!("mdix_builder_get_string('{}'): value is {} not string", path_str, other.type_name())); std::ptr::null_mut() }
        None        => { set_last_error(&format!("mdix_builder_get_string('{}'): key not found", path_str)); std::ptr::null_mut() }
    }
}

#[no_mangle]
pub extern "C" fn mdix_builder_get_int(builder: *const c_void, path: *const c_char) -> i32 {
    clear_last_error();
    let (b, path_str) = match validate_builder_read(builder, path, "mdix_builder_get_int") {
        Some(v) => v,
        None    => return 0,
    };
    match b.entries.get(path_str) {
        Some(DixValue::Int(i))    => *i,
        Some(DixValue::Long(l))   => *l as i32,
        Some(DixValue::Float(f))  => *f as i32,
        Some(DixValue::Double(d)) => *d as i32,
        Some(other) => { set_last_error(&format!("mdix_builder_get_int('{}'): value is {} not numeric", path_str, other.type_name())); 0 }
        None        => { set_last_error(&format!("mdix_builder_get_int('{}'): key not found", path_str)); 0 }
    }
}

#[no_mangle]
pub extern "C" fn mdix_builder_get_long(builder: *const c_void, path: *const c_char) -> i64 {
    clear_last_error();
    let (b, path_str) = match validate_builder_read(builder, path, "mdix_builder_get_long") {
        Some(v) => v,
        None    => return 0,
    };
    match b.entries.get(path_str) {
        Some(DixValue::Long(l))   => *l,
        Some(DixValue::Int(i))    => *i as i64,
        Some(DixValue::Float(f))  => *f as i64,
        Some(DixValue::Double(d)) => *d as i64,
        Some(other) => { set_last_error(&format!("mdix_builder_get_long('{}'): value is {} not numeric", path_str, other.type_name())); 0 }
        None        => { set_last_error(&format!("mdix_builder_get_long('{}'): key not found", path_str)); 0 }
    }
}

#[no_mangle]
pub extern "C" fn mdix_builder_get_float(builder: *const c_void, path: *const c_char) -> f32 {
    clear_last_error();
    let (b, path_str) = match validate_builder_read(builder, path, "mdix_builder_get_float") {
        Some(v) => v,
        None    => return 0.0,
    };
    match b.entries.get(path_str) {
        Some(DixValue::Float(f))  => *f,
        Some(DixValue::Int(i))    => *i as f32,
        Some(DixValue::Long(l))   => *l as f32,
        Some(DixValue::Double(d)) => *d as f32,
        Some(other) => { set_last_error(&format!("mdix_builder_get_float('{}'): value is {} not numeric", path_str, other.type_name())); 0.0 }
        None        => { set_last_error(&format!("mdix_builder_get_float('{}'): key not found", path_str)); 0.0 }
    }
}

#[no_mangle]
pub extern "C" fn mdix_builder_get_double(builder: *const c_void, path: *const c_char) -> f64 {
    clear_last_error();
    let (b, path_str) = match validate_builder_read(builder, path, "mdix_builder_get_double") {
        Some(v) => v,
        None    => return 0.0,
    };
    match b.entries.get(path_str) {
        Some(DixValue::Double(d)) => *d,
        Some(DixValue::Float(f))  => *f as f64,
        Some(DixValue::Int(i))    => *i as f64,
        Some(DixValue::Long(l))   => *l as f64,
        Some(other) => { set_last_error(&format!("mdix_builder_get_double('{}'): value is {} not numeric", path_str, other.type_name())); 0.0 }
        None        => { set_last_error(&format!("mdix_builder_get_double('{}'): key not found", path_str)); 0.0 }
    }
}

#[no_mangle]
pub extern "C" fn mdix_builder_get_bool(builder: *const c_void, path: *const c_char) -> bool {
    clear_last_error();
    let (b, path_str) = match validate_builder_read(builder, path, "mdix_builder_get_bool") {
        Some(v) => v,
        None    => return false,
    };
    match b.entries.get(path_str) {
        Some(DixValue::Bool(bv)) => *bv,
        Some(other) => { set_last_error(&format!("mdix_builder_get_bool('{}'): value is {} not bool", path_str, other.type_name())); false }
        None        => { set_last_error(&format!("mdix_builder_get_bool('{}'): key not found", path_str)); false }
    }
}

// =============================================================================
// BUILDER — persistence
// =============================================================================

/// Serialize the builder's entries and write them to a .mdix file at path.
/// Parent directories are created if they do not exist.
/// Returns true on success.
#[no_mangle]
pub extern "C" fn mdix_builder_save(builder: *const c_void, path: *const c_char) -> bool {
    clear_last_error();
    let b = match unsafe { as_builder(builder) } {
        Some(b) => b,
        None    => { set_last_error("mdix_builder_save: builder is null"); return false; }
    };
    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None    => { set_last_error("mdix_builder_save: path is null or invalid UTF-8"); return false; }
    };
    let entries   = strip_indexed_keys(b.entries.clone());
    let converter = DixConverter::new();
    let ast       = match converter.from_hashmap(entries) {
        Ok(a)  => a,
        Err(e) => { set_last_error(&format!("mdix_builder_save: AST conversion failed: {}", e)); return false; }
    };
    let content = match converter.to_mdix(&ast, None) {
        Ok(s)  => s,
        Err(e) => { set_last_error(&format!("mdix_builder_save: serialization failed: {}", e)); return false; }
    };
    if let Some(parent) = std::path::Path::new(path_str).parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            set_last_error(&format!("mdix_builder_save: could not create directories: {}", e));
            return false;
        }
    }
    match std::fs::write(path_str, content) {
        Ok(())  => true,
        Err(e) => { set_last_error(&format!("mdix_builder_save: write failed: {}", e)); false }
    }
}

/// Serialize the builder's entries to a pretty-printed .mdix string.
/// Caller must free with mdix_free_string().
#[no_mangle]
pub extern "C" fn mdix_builder_to_string(builder: *const c_void) -> *mut c_char {
    clear_last_error();
    let b = match unsafe { as_builder(builder) } {
        Some(b) => b,
        None    => { set_last_error("mdix_builder_to_string: builder is null"); return std::ptr::null_mut(); }
    };
    let entries   = strip_indexed_keys(b.entries.clone());
    let converter = DixConverter::new();
    let ast       = match converter.from_hashmap(entries) {
        Ok(a)  => a,
        Err(e) => { set_last_error(&format!("mdix_builder_to_string: AST conversion failed: {}", e)); return std::ptr::null_mut(); }
    };
    match converter.to_mdix(&ast, Some(&DixFormatOptions::pretty())) {
        Ok(s)  => str_to_c_char(s),
        Err(e) => { set_last_error(&format!("mdix_builder_to_string: serialization failed: {}", e)); std::ptr::null_mut() }
    }
}

// =============================================================================
// JSON / TOML IMPORT
// =============================================================================

/// Parse a JSON string and load it as a DixData handle.
/// Caller must free with mdix_free when done.
#[no_mangle]
pub extern "C" fn mdix_from_json(source: *const c_char) -> *mut c_void {
    clear_last_error();
    let src = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None    => { set_last_error("mdix_from_json: source is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    let converter = DixConverter::new();
    let ast       = match converter.from_json(src) {
        Ok(a)  => a,
        Err(e) => { set_last_error(&format!("mdix_from_json: {}", e)); return std::ptr::null_mut(); }
    };
    let mdix_src = match converter.to_mdix(&ast, None) {
        Ok(s)  => s,
        Err(e) => { set_last_error(&format!("mdix_from_json: re-serialization failed: {}", e)); return std::ptr::null_mut(); }
    };
    let loader = DixLoader::new();
    match loader.load_from_str(&mdix_src, &DixLoadOptions::new()) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e)   => { set_last_error(&format!("mdix_from_json: load failed: {}", e)); std::ptr::null_mut() }
    }
}

/// Parse a TOML string and load it as a DixData handle.
/// Caller must free with mdix_free when done.
#[no_mangle]
pub extern "C" fn mdix_from_toml(source: *const c_char) -> *mut c_void {
    clear_last_error();
    let src = match unsafe { c_str_to_str(source) } {
        Some(s) => s,
        None    => { set_last_error("mdix_from_toml: source is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    let converter = DixConverter::new();
    let ast       = match converter.from_toml(src) {
        Ok(a)  => a,
        Err(e) => { set_last_error(&format!("mdix_from_toml: {}", e)); return std::ptr::null_mut(); }
    };
    let mdix_src = match converter.to_mdix(&ast, None) {
        Ok(s)  => s,
        Err(e) => { set_last_error(&format!("mdix_from_toml: re-serialization failed: {}", e)); return std::ptr::null_mut(); }
    };
    let loader = DixLoader::new();
    match loader.load_from_str(&mdix_src, &DixLoadOptions::new()) {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e)   => { set_last_error(&format!("mdix_from_toml: load failed: {}", e)); std::ptr::null_mut() }
    }
}

// =============================================================================
// MERGE
// =============================================================================

/// Merge two or more .mdix source strings into a new handle using the real
/// AST-level DixScript merger (dixscript::Runtime::MdixMerger) — full type
/// fidelity (Long / Float / Double / HexColor / Blob / Regex / Date /
/// Timestamp / Enum all survive exactly, unlike a JSON round-trip), a real
/// per-key conflict report, and configurable array merge behavior. See
/// merge.rs's module doc for why this takes source strings rather than
/// existing handles or file paths.
///
/// Sources are weighted in descending order: sources[0] gets weight 1.0,
/// sources[count-1] gets the lowest weight (only matters under
/// MdixMergeStrategy::WeightedPriority). Use mdix_merge_sources_weighted for
/// explicit per-source weights.
///
/// `out_conflicts_json`, if non-null, receives a heap string describing
/// every conflict that was resolved:
/// `[{"path":"...","winningSource":0,"winningLabel":"..."}, ...]`
/// (`"[]"` when there were none). Caller must free it with mdix_free_string()
/// — independently of whether the merge itself succeeded, except that on
/// failure it is left null instead (matching every other out-param in this
/// crate: check the pointer, don't assume it was written).
///
/// Returns a new opaque handle on success (caller must free with mdix_free),
/// null on failure — check mdix_get_last_error().
#[no_mangle]
pub extern "C" fn mdix_merge_sources(
    sources: *const *const c_char,
    count: i32,
    strategy: MdixMergeStrategy,
    array_strategy: ArrayMergeStrategy,
    out_conflicts_json: *mut *mut c_char,
) -> *mut c_void {
    clear_last_error();
    if !out_conflicts_json.is_null() {
        unsafe { *out_conflicts_json = std::ptr::null_mut(); }
    }

    let source_strings = match unsafe {
        read_source_array(sources, count, "mdix_merge_sources")
    } {
        Ok(v) => v,
        Err(e) => { set_last_error(&e); return std::ptr::null_mut(); }
    };

    match run_merge("mdix_merge_sources", source_strings, None, strategy, array_strategy) {
        Ok((handle, conflicts_json)) => {
            if !out_conflicts_json.is_null() {
                unsafe { *out_conflicts_json = str_to_c_char(conflicts_json); }
            }
            handle
        }
        Err(e) => { set_last_error(&e); std::ptr::null_mut() }
    }
}

/// Merge .mdix source strings with explicit per-source weights (`weights`
/// must be the same length as `sources`). Higher weight wins under
/// MdixMergeStrategy::WeightedPriority. See mdix_merge_sources for the
/// shared semantics (fidelity, conflict report, error handling).
#[no_mangle]
pub extern "C" fn mdix_merge_sources_weighted(
    sources: *const *const c_char,
    weights: *const f64,
    count: i32,
    strategy: MdixMergeStrategy,
    array_strategy: ArrayMergeStrategy,
    out_conflicts_json: *mut *mut c_char,
) -> *mut c_void {
    clear_last_error();
    if !out_conflicts_json.is_null() {
        unsafe { *out_conflicts_json = std::ptr::null_mut(); }
    }

    if weights.is_null() {
        set_last_error("mdix_merge_sources_weighted: weights is null");
        return std::ptr::null_mut();
    }

    let source_strings = match unsafe {
        read_source_array(sources, count, "mdix_merge_sources_weighted")
    } {
        Ok(v) => v,
        Err(e) => { set_last_error(&e); return std::ptr::null_mut(); }
    };

    // read_source_array already rejected count <= 0, so `count as usize` is safe here.
    let weight_vec = unsafe { std::slice::from_raw_parts(weights, count as usize) }.to_vec();
    if weight_vec.len() != source_strings.len() {
        set_last_error(&format!(
            "mdix_merge_sources_weighted: weights length ({}) does not match sources length ({})",
            weight_vec.len(), source_strings.len()
        ));
        return std::ptr::null_mut();
    }

    match run_merge(
        "mdix_merge_sources_weighted",
        source_strings,
        Some(weight_vec),
        strategy,
        array_strategy,
    ) {
        Ok((handle, conflicts_json)) => {
            if !out_conflicts_json.is_null() {
                unsafe { *out_conflicts_json = str_to_c_char(conflicts_json); }
            }
            handle
        }
        Err(e) => { set_last_error(&e); std::ptr::null_mut() }
    }
}

// =============================================================================
// PRIVATE HELPERS
// =============================================================================

fn validate_read<'a>(
    handle: *const c_void, path: *const c_char, fn_name: &str,
) -> Option<(&'a MdixHandle, &'a str)> {
    let h = match unsafe { as_handle(handle) } {
        Some(h) => h,
        None    => { set_last_error(&format!("{}: handle is null", fn_name)); return None; }
    };
    let path_str = unsafe { c_str_to_str(path) }?;
    if path_str.is_empty() {
        set_last_error(&format!("{}: path is empty", fn_name));
        return None;
    }
    Some((h, path_str))
}

fn validate_builder_write<'a>(
    builder: *mut c_void, path: *const c_char, fn_name: &str,
) -> Option<(&'a mut MdixBuilderHandle, &'a str)> {
    let b = match unsafe { as_builder_mut(builder) } {
        Some(b) => b,
        None    => { set_last_error(&format!("{}: builder is null", fn_name)); return None; }
    };
    let path_str = unsafe { c_str_to_str(path) }?;
    if path_str.is_empty() {
        set_last_error(&format!("{}: path is empty", fn_name));
        return None;
    }
    Some((b, path_str))
}

fn validate_builder_read<'a>(
    builder: *const c_void, path: *const c_char, fn_name: &str,
) -> Option<(&'a MdixBuilderHandle, &'a str)> {
    let b = match unsafe { as_builder(builder) } {
        Some(b) => b,
        None    => { set_last_error(&format!("{}: builder is null", fn_name)); return None; }
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

// =============================================================================
// HOT RELOAD
// =============================================================================
//
// Thin wrapper over dixscript::Runtime::HotReloadWatcher — deliberately a
// single-file, std::fs::metadata poll rather than an OS filesystem-event
// subscription (see hot_reload.rs's own doc comment for why: no
// notify/inotify/FSEvents/ReadDirectoryChangesW dependency, identical
// behavior on every platform this crate ships to). Call
// mdix_watcher_check_and_reload from a game loop / timer tick — a single
// stat() call per check is cheap enough to run every frame.
//
// Encrypted .mdix files are not supported: HotReloadWatcher::force_reload()
// always reloads through the plaintext loader path internally, a core
// Runtime limitation, not something this binding adds on top.
//
// bool/pointer sentinel ambiguity: mdix_watcher_has_changed's `false` means
// either "unchanged" or "error" (same as mdix_validate's existing bool
// convention elsewhere in this file); mdix_watcher_check_and_reload's null
// means either "unchanged" or "error". Call mdix_get_last_error() after
// either sentinel if the caller needs to tell them apart — it returns an
// empty/null result when there was no error.

/// Starts watching a single plaintext `.mdix` path. Does not read the file
/// yet — the first `mdix_watcher_has_changed`/`check_and_reload` call always
/// reports a change. Returns an opaque handle, or null on failure (null/
/// invalid-UTF8 path). Caller must free with mdix_watcher_free.
#[no_mangle]
pub extern "C" fn mdix_watcher_new(path: *const c_char) -> *mut c_void {
    clear_last_error();
    let path_str = match unsafe { c_str_to_str(path) } {
        Some(s) => s,
        None => { set_last_error("mdix_watcher_new: path is null or invalid UTF-8"); return std::ptr::null_mut(); }
    };
    MdixWatcherHandle::new(HotReloadWatcher::new(path_str)) as *mut c_void
}

/// Frees a watcher handle created by mdix_watcher_new. Safe to call with null.
#[no_mangle]
pub extern "C" fn mdix_watcher_free(handle: *mut c_void) {
    unsafe { MdixWatcherHandle::free(handle as *mut MdixWatcherHandle) };
}

/// Returns the watched path. Caller must free with mdix_free_string.
/// Returns null if `handle` is null.
#[no_mangle]
pub extern "C" fn mdix_watcher_path(handle: *const c_void) -> *mut c_char {
    clear_last_error();
    let w = match unsafe { as_watcher(handle) } {
        Some(w) => w,
        None => { set_last_error("mdix_watcher_path: handle is null"); return std::ptr::null_mut(); }
    };
    str_to_c_char(w.watcher.path().to_string_lossy().into_owned())
}

/// True once a successful reload has happened at least once.
#[no_mangle]
pub extern "C" fn mdix_watcher_has_loaded(handle: *const c_void) -> bool {
    clear_last_error();
    match unsafe { as_watcher(handle) } {
        Some(w) => w.watcher.has_loaded(),
        None => { set_last_error("mdix_watcher_has_loaded: handle is null"); false }
    }
}

/// Checks whether the file's modified-time differs from the last successful
/// reload, without reloading it. See the section header comment above for
/// the false-return ambiguity.
#[no_mangle]
pub extern "C" fn mdix_watcher_has_changed(handle: *const c_void) -> bool {
    clear_last_error();
    let w = match unsafe { as_watcher(handle) } {
        Some(w) => w,
        None => { set_last_error("mdix_watcher_has_changed: handle is null"); return false; }
    };
    match w.watcher.has_changed() {
        Ok(b) => b,
        Err(e) => { set_last_error(&format!("mdix_watcher_has_changed: {}", e)); false }
    }
}

/// Reloads only if the file has changed since the last successful reload
/// (or since construction, on the first call). Returns a new read handle
/// (free with mdix_free) on a successful reload, or null when unchanged OR
/// on error — see the section header comment above for telling them apart.
#[no_mangle]
pub extern "C" fn mdix_watcher_check_and_reload(handle: *mut c_void) -> *mut c_void {
    clear_last_error();
    let w = match unsafe { as_watcher_mut(handle) } {
        Some(w) => w,
        None => { set_last_error("mdix_watcher_check_and_reload: handle is null"); return std::ptr::null_mut(); }
    };
    match w.watcher.check_and_reload() {
        Ok(Some(data)) => MdixHandle::new(data) as *mut c_void,
        Ok(None) => std::ptr::null_mut(),
        Err(e) => { set_last_error(&format!("mdix_watcher_check_and_reload: {}", e)); std::ptr::null_mut() }
    }
}

/// Reloads unconditionally, regardless of whether the file has changed.
/// Returns a new read handle (free with mdix_free), or null on failure.
#[no_mangle]
pub extern "C" fn mdix_watcher_force_reload(handle: *mut c_void) -> *mut c_void {
    clear_last_error();
    let w = match unsafe { as_watcher_mut(handle) } {
        Some(w) => w,
        None => { set_last_error("mdix_watcher_force_reload: handle is null"); return std::ptr::null_mut(); }
    };
    match w.watcher.force_reload() {
        Ok(data) => MdixHandle::new(data) as *mut c_void,
        Err(e) => { set_last_error(&format!("mdix_watcher_force_reload: {}", e)); std::ptr::null_mut() }
    }
}
