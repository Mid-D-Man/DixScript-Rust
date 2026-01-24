// src/Builtins/Resolver/mod.rs
//! Resolver - Resolves builtin method calls and validates them

pub mod instance_method_registry;
mod static_object_registry;

// Re-export commonly used items
pub use instance_method_registry::{
    call_instance_method, generate_documentation, get_instance_method, get_instance_methods,
    get_method_count, get_types_with_methods, get_universal_methods, has_instance_method,
    is_universal_method, validate_instance_call, ValidationResult,
};