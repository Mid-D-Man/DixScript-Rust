// src/Builtins/Static/static_object_base.rs
//! Base traits and utilities for static objects

use crate::Builtins::Core::{DixValue, IBuiltinMethod};
use std::collections::HashMap;

/// Trait for static objects (Math, DateTime, Array, etc.)
pub trait IStaticObject: Send + Sync {
    /// Get the name of this static object
    fn name(&self) -> &str;

    /// Call a method on this static object
    fn call_method(&self, method_name: &str, args: &[DixValue]) -> Result<DixValue, String>;

    /// Check if this object has a specific method
    fn has_method(&self, method_name: &str) -> bool;

    /// Get all available method names
    fn get_method_names(&self) -> Vec<String>;

    /// Get method signature for documentation/validation
    fn get_method(&self, method_name: &str) -> Option<&dyn IBuiltinMethod>;
}

/// Helper struct for building static objects
/// Provides common functionality for storing and managing methods
pub struct StaticObjectBase {
    name: String,
    methods: HashMap<String, Box<dyn IBuiltinMethod>>,
}

impl StaticObjectBase {
    /// Create a new static object base
    pub fn new(name: String) -> Self {
        StaticObjectBase {
            name,
            methods: HashMap::new(),
        }
    }

    /// Register a method with this static object
    pub fn register_method(&mut self, method: Box<dyn IBuiltinMethod>) {
        let name = method.name().to_string();
        self.methods.insert(name, method);
    }

    /// Get the object name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Call a method
    pub fn call_method(&self, method_name: &str, args: &[DixValue]) -> Result<DixValue, String> {
        let method = self
            .methods
            .get(method_name)
            .ok_or_else(|| format!("{} object has no method: {}", self.name, method_name))?;

        method.call(args)
    }

    /// Check if has method
    pub fn has_method(&self, method_name: &str) -> bool {
        self.methods.contains_key(method_name)
    }

    /// Get all method names
    pub fn get_method_names(&self) -> Vec<String> {
        self.methods.keys().cloned().collect()
    }

    /// Get a method
    pub fn get_method(&self, method_name: &str) -> Option<&dyn IBuiltinMethod> {
        self.methods
            .get(method_name)
            .map(|boxed| &**boxed as &dyn IBuiltinMethod)
    }
}