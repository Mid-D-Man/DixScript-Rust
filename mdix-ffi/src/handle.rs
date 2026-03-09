// mdix-ffi/src/handle.rs
//
// Opaque handle types that live on the Rust heap.
//
// C# receives these as IntPtr — it never inspects the internals.
// Rust owns all memory. C# calls mdix_free / mdix_builder_free to release it.
//
// Two handle types:
//   MdixHandle        — wraps a loaded, read-only DixData (load → read → free)
//   MdixBuilderHandle — wraps a mutable HashMap for building save data (new → set → save → free)

use std::collections::HashMap;
use dixscript::Runtime::{DixData, DixValue};

/// Read handle wrapping a fully loaded DixData.
///
/// Created by mdix_load / mdix_load_str / mdix_load_encrypted.
/// Freed by mdix_free.
pub struct MdixHandle {
    pub data: DixData,
}

impl MdixHandle {
    pub fn new(data: DixData) -> *mut Self {
        Box::into_raw(Box::new(MdixHandle { data }))
    }

    /// Consume a raw pointer, freeing the allocation.
    ///
    /// # Safety
    /// `ptr` must have been created by MdixHandle::new and not yet freed.
    pub unsafe fn free(ptr: *mut Self) {
        if !ptr.is_null() {
            drop(Box::from_raw(ptr));
        }
    }
}

/// Write handle holding a mutable key-value store.
///
/// Used for building save data at runtime without needing a template file.
/// Created by mdix_builder_new.
/// Freed by mdix_builder_free.
pub struct MdixBuilderHandle {
    /// Flat dotted-path key → value store, mirrors DixData's internal layout.
    pub entries: HashMap<String, DixValue>,
}

impl MdixBuilderHandle {
    pub fn new() -> *mut Self {
        Box::into_raw(Box::new(MdixBuilderHandle {
            entries: HashMap::new(),
        }))
    }

    /// Consume a raw pointer, freeing the allocation.
    ///
    /// # Safety
    /// `ptr` must have been created by MdixBuilderHandle::new and not yet freed.
    pub unsafe fn free(ptr: *mut Self) {
        if !ptr.is_null() {
            drop(Box::from_raw(ptr));
        }
    }
  }
