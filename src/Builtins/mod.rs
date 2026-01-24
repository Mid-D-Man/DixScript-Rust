// src/Builtins/mod.rs
//! Builtins - Built-in types and methods

pub mod Core;
pub mod Instance;
pub mod Static;
pub mod Resolver;

// Re-export commonly used items from Core
pub use Core::{DixType, DixValue, IBuiltinMethod, BuiltinMethod};

// Re-export commonly used items from Resolver
pub use Resolver::{
    initialize, is_initialized,
    call_instance_method, has_instance_method, validate_instance_call,
    initialize_static_registry, registry_has_static_object,
    registry_has_static_method, registry_call_static_method,
};