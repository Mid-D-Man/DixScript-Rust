// src/Builtins/mod.rs
//! Builtins - Built-in types and methods

pub mod Core;
pub mod Instance;
pub mod Static;
pub mod Resolver;

// Re-export commonly used items
pub use Core::{DixType, DixValue, IBuiltinMethod, BuiltinMethod};
pub use Resolver::{call_instance_method, has_instance_method, validate_instance_call};