// src/Builtins/Core/mod.rs
//! Core - DixType, DixValue, and IBuiltinMethod

pub mod dix_type;
pub mod dix_value;
pub mod builtin_method;

// Re-export for convenience
pub use dix_type::DixType;
pub use dix_value::DixValue;
pub use builtin_method::{
    IBuiltinMethod,
    BuiltinMethod,
    BuiltinMethodException,
    BuiltinMethodImpl,
    BuiltinMethodValidator,
    validation_helpers,
};