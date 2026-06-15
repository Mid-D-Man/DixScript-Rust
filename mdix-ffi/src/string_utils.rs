// mdix-ffi/src/string_utils.rs
//
// CStr / String conversion utilities used throughout the FFI surface.
//
// Every FFI function that accepts *const c_char goes through c_str_to_str.
// Every FFI function that returns *mut c_char goes through str_to_c_char.
// This keeps the unsafe surface small and centralized.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Convert a raw C string pointer to a Rust &str.
///
/// Returns None if the pointer is null or the bytes are not valid UTF-8.
///
/// # Safety
/// `ptr` must point to a valid null-terminated C string that remains alive
/// for the duration of the returned reference.
pub unsafe fn c_str_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok()
}

/// Convert a Rust String into a heap-allocated C string, returning a raw pointer.
///
/// The caller is responsible for freeing the returned pointer via mdix_free_string().
/// Returns null if the string contains interior null bytes (should never happen
/// for valid DixScript output).
pub fn str_to_c_char(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Convert a Vec<String> into a heap-allocated boxed slice of C string pointers.
///
/// Returns a pointer to the first element and writes the element count to
/// `out_count`. The allocation is a `Box<[*mut c_char]>` leaked via
/// `Box::into_raw`. The caller must free the entire allocation with
/// mdix_free_string_array(result, out_count). Returns null when the input
/// is empty.
pub fn string_vec_to_c_array(strings: Vec<String>, out_count: *mut i32) -> *mut *mut c_char {
    let count = strings.len();

    if !out_count.is_null() {
        unsafe { *out_count = count as i32 };
    }

    if count == 0 {
        return std::ptr::null_mut();
    }

    // Use a boxed slice so capacity == len, which is required for the matching
    // Box::from_raw in free_c_char_array to be sound.
    let boxed: Box<[*mut c_char]> = strings
        .into_iter()
        .map(str_to_c_char)
        .collect::<Vec<_>>()
        .into_boxed_slice();

    Box::into_raw(boxed) as *mut *mut c_char
}

/// Free a C string that was returned by an mdix FFI getter function.
///
/// Passing null is safe. Do NOT call this on the static pointer returned by
/// mdix_version() — that pointer must never be freed.
///
/// # Safety
/// `ptr` must have been produced by an mdix get/to/format/builder function,
/// or be null. Calling this twice on the same pointer is undefined behaviour.
pub unsafe fn free_c_char(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

/// Free an array of C strings returned by mdix_get_keys() or mdix_get_all_keys().
///
/// # Safety
/// `arr` must be the exact pointer returned by the corresponding call.
/// `count` must match the value written to out_count by that call.
/// Calling this twice on the same pointer is undefined behaviour.
pub unsafe fn free_c_char_array(arr: *mut *mut c_char, count: i32) {
    if arr.is_null() || count <= 0 {
        return;
    }
    let count = count as usize;
    // Reconstruct the boxed slice. This is sound because string_vec_to_c_array
    // always uses into_boxed_slice(), so the allocation length equals count exactly.
    let slice_ptr = std::ptr::slice_from_raw_parts_mut(arr, count);
    let boxed = Box::from_raw(slice_ptr);
    for ptr in boxed.iter() {
        if !ptr.is_null() {
            drop(CString::from_raw(*ptr));
        }
    }
    // boxed drops here, freeing the slice allocation.
}
