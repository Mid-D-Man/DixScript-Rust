// mdix-ffi/src/string_utils.rs
//
// CStr / String conversion utilities used throughout the FFI surface.
//
// Every FFI function that accepts a *const c_char goes through c_str_to_str.
// Every FFI function that returns a *mut c_char goes through str_to_c_char.
// This keeps the unsafe surface small and centralized.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Convert a raw C string pointer to a Rust &str.
///
/// Returns None if:
/// - the pointer is null
/// - the bytes are not valid UTF-8
///
/// # Safety
/// Caller must ensure `ptr` points to a valid null-terminated C string that
/// remains alive for the duration of the returned reference.
pub unsafe fn c_str_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok()
}

/// Convert a Rust String into a heap-allocated C string, returning a raw pointer.
///
/// The caller is responsible for freeing the returned pointer via mdix_free_string().
/// Returns null if the string contains interior null bytes (which would corrupt
/// the C string terminator).
pub fn str_to_c_char(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Convert a static str to a C string pointer.
///
/// The memory is 'static — do NOT free this pointer with mdix_free_string().
/// Only used for constant responses like the version string.
pub fn static_str_to_c_char(s: &'static str) -> *const c_char {
    // Safety: we immediately call as_ptr() — the CString would normally be
    // dropped here but since s is 'static and has no interior nulls we leak it
    // intentionally as a one-time allocation.
    let cs = CString::new(s).expect("static string contained null byte");
    let ptr = cs.as_ptr();
    std::mem::forget(cs);
    ptr
}

/// Convert a Vec<String> into a heap-allocated array of C strings.
///
/// Returns a pointer to an array of `*mut c_char`, with count written to `out_count`.
/// The entire allocation (array + each string) must be freed via mdix_free_string_array().
/// Returns null on allocation failure.
pub fn string_vec_to_c_array(strings: Vec<String>, out_count: *mut i32) -> *mut *mut c_char {
    let count = strings.len();

    // Write count to the output parameter before any early return.
    if !out_count.is_null() {
        unsafe { *out_count = count as i32 };
    }

    if count == 0 {
        return std::ptr::null_mut();
    }

    // Allocate the pointer array on the heap via Vec, then leak it.
    let mut ptrs: Vec<*mut c_char> = strings
        .into_iter()
        .map(|s| str_to_c_char(s))
        .collect();

    let raw = ptrs.as_mut_ptr();
    std::mem::forget(ptrs);
    raw
}

/// Free a C string that was returned by an mdix FFI function.
///
/// Must only be called on strings allocated by str_to_c_char — not on
/// static pointers like the one returned by mdix_version().
///
/// # Safety
/// `ptr` must be a pointer previously returned by an mdix get_string function,
/// or null. Calling this twice on the same pointer is undefined behavior.
pub unsafe fn free_c_char(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

/// Free an array of C strings returned by mdix_get_keys().
///
/// # Safety
/// `arr` must be the exact pointer returned by mdix_get_keys().
/// `count` must match the count written by that call.
/// Calling this twice on the same pointer is undefined behavior.
pub unsafe fn free_c_char_array(arr: *mut *mut c_char, count: i32) {
    if arr.is_null() || count <= 0 {
        return;
    }
    let count = count as usize;
    // Reconstruct the slice to free each string, then free the array itself.
    let slice = std::slice::from_raw_parts_mut(arr, count);
    for ptr in slice.iter() {
        if !ptr.is_null() {
            drop(CString::from_raw(*ptr));
        }
    }
    // Reconstruct the Vec to free the array allocation.
    drop(Vec::from_raw_parts(arr, count, count));
  }
