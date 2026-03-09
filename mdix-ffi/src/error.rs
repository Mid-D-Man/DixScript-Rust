// mdix-ffi/src/error.rs
//
// Thread-local error storage for the FFI layer.
//
// FFI functions cannot throw exceptions across the C boundary.
// The pattern used here mirrors C's errno:
//   - on success: clear the slot, return a valid value
//   - on failure: write an error message into the slot, return a sentinel (null / 0 / false)
//   - caller: check the sentinel, then call mdix_get_last_error() for details
//
// The returned pointer from get_last_error_ptr() is valid only until the next
// FFI call that may set an error. Callers must copy the string before calling
// any other mdix function.

use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_char;

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Store an error message in the thread-local slot.
/// Replaces any previously stored message.
pub fn set_last_error(msg: &str) {
    LAST_ERROR.with(|slot| {
        // If the message contains interior null bytes, store a fallback.
        let cs = CString::new(msg)
            .unwrap_or_else(|_| CString::new("error message contained null bytes").unwrap());
        *slot.borrow_mut() = Some(cs);
    });
}

/// Clear the thread-local error slot.
/// Called at the start of every FFI function so stale errors do not persist.
pub fn clear_last_error() {
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

/// Return a raw pointer to the last error string, or null if there is no error.
///
/// # Safety
/// The pointer is valid only until the next FFI call that may write an error.
/// The C# wrapper copies this string immediately and does not cache the pointer.
pub fn get_last_error_ptr() -> *const c_char {
    LAST_ERROR.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|cs| cs.as_ptr())
            .unwrap_or(std::ptr::null())
    })
}

/// Return true if there is a pending error in the thread-local slot.
pub fn has_error() -> bool {
    LAST_ERROR.with(|slot| slot.borrow().is_some())
                            }
