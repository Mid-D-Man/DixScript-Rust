// mdix-ffi/src/handle.rs
//
// Opaque handle types that live on the Rust heap.
//
// C# receives these as IntPtr — it never inspects the internals.
// Rust owns all memory. C# calls mdix_free / mdix_builder_free to release it.
//
// Two handle types:
//   MdixHandle        — wraps a loaded, read-only DixData (load → read → free)
//   MdixBuilderHandle — wraps a mutable HashMap for building/editing save data
//                       (new | from_handle → set → save → free)
//
// Both structs are #[repr(C)] so csbindgen emits clean opaque C# partial structs:
//
//   internal unsafe partial struct MdixHandle { }
//   internal unsafe partial struct MdixBuilderHandle { }
//
// Fields are pub(crate), NOT pub. If the fields were pub, csbindgen would try
// to map DixData and HashMap<String, DixValue> into C# field declarations —
// types that have no C# equivalent. pub(crate) keeps the structs opaque.

use std::collections::HashMap;
use dixscript::Runtime::{DixData, DixValue};

/// Read handle wrapping a fully loaded DixData.
///
/// Created by mdix_load / mdix_load_str / mdix_load_encrypted.
/// Freed by mdix_free.
#[repr(C)]
pub struct MdixHandle {
    pub(crate) data: DixData,
}

impl MdixHandle {
    pub fn new(data: DixData) -> *mut Self {
        Box::into_raw(Box::new(MdixHandle { data }))
    }

    /// Consume a raw pointer, freeing the allocation.
    ///
    /// # Safety
    /// `ptr` must have been created by MdixHandle::new and not yet freed.
    /// Calling this twice on the same pointer is UB.
    pub unsafe fn free(ptr: *mut Self) {
        if !ptr.is_null() {
            drop(Box::from_raw(ptr));
        }
    }
}

/// Write handle holding a mutable key-value store.
///
/// Used for building save data at runtime (mdix_builder_new), or for
/// round-trip editing of an existing file (mdix_builder_from_handle →
/// modify fields → mdix_builder_save).
/// Freed by mdix_builder_free.
#[repr(C)]
pub struct MdixBuilderHandle {
    pub(crate) entries: HashMap<String, DixValue>,
}

impl MdixBuilderHandle {
    /// Create an empty builder — no pre-existing data.
    pub fn new() -> *mut Self {
        Box::into_raw(Box::new(MdixBuilderHandle {
            entries: HashMap::new(),
        }))
    }

    /// Create a builder pre-populated from a structural flat map.
    ///
    /// Called by mdix_builder_from_handle so that the builder starts with all
    /// root-level values from a loaded file. The caller passes
    /// DixData::to_structural_hashmap() so synthetic indexed children
    /// (tags[0], server.host, etc.) are already stripped — only aggregate/root
    /// values that map back to valid .mdix identifiers are editable.
    pub fn from_flat_map(entries: HashMap<String, DixValue>) -> *mut Self {
        Box::into_raw(Box::new(MdixBuilderHandle { entries }))
    }

    /// Consume a raw pointer, freeing the allocation.
    ///
    /// # Safety
    /// `ptr` must have been created by MdixBuilderHandle::new or
    /// MdixBuilderHandle::from_flat_map and not yet freed.
    /// Calling this twice on the same pointer is UB.
    pub unsafe fn free(ptr: *mut Self) {
        if !ptr.is_null() {
            drop(Box::from_raw(ptr));
        }
    }
        }
