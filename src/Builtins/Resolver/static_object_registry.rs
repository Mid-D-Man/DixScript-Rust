// src/Builtins/Static/static_object_registry.rs
//! Central registry for all static objects in DixScript
//! Provides thread-safe access to Math, DateTime, Array, Dix, etc.

use super::*;
use crate::Builtins::Core::{DixValue, IBuiltinMethod};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// Global static object registry
static REGISTRY: OnceLock<StaticObjectRegistry> = OnceLock::new();

/// Registry for all static objects
pub struct StaticObjectRegistry {
    objects: RwLock<HashMap<String, Box<dyn IStaticObject>>>,
}

impl StaticObjectRegistry {
    /// Create and initialize the registry
    fn new() -> Self {
        let mut registry = StaticObjectRegistry {
            objects: RwLock::new(HashMap::new()),
        };
        registry.initialize_objects();
        registry
    }

    /// Initialize all built-in static objects
    fn initialize_objects(&mut self) {
        let mut objects = self.objects.write().unwrap();

        // Register core static objects
        objects.insert("Dix".to_string(), Box::new(DixObject::new()));
        objects.insert("Math".to_string(), Box::new(MathObject::new()));
        objects.insert("DateTime".to_string(), Box::new(DateTimeObject::new()));
        objects.insert("Array".to_string(), Box::new(ArrayObject::new()));
        objects.insert("Random".to_string(), Box::new(RandomObject::new()));
        objects.insert("Enum".to_string(), Box::new(EnumObject::new()));
        objects.insert("Guid".to_string(), Box::new(GuidObject::new()));
        objects.insert("IpAddress".to_string(), Box::new(IpAddressObject::new()));
    }

    /// Get the global registry instance
    fn get() -> &'static StaticObjectRegistry {
        REGISTRY.get_or_init(StaticObjectRegistry::new)
    }
}

// ==================== PUBLIC API ====================

/// Initialize the static object registry
pub fn initialize_static_registry() {
    // Force initialization
    let _ = StaticObjectRegistry::get();
}

/// Check if a static object exists
pub fn has_static_object(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let registry = StaticObjectRegistry::get();
    let objects = registry.objects.read().unwrap();
    objects.contains_key(name)
}

/// Get a static object by name
pub fn get_static_object(name: &str) -> Option<&'static dyn IStaticObject> {
    if name.is_empty() {
        return None;
    }

    let registry = StaticObjectRegistry::get();
    let objects = registry.objects.read().unwrap();

    // This is tricky - we can't return a reference from inside the lock
    // We'll need to redesign this slightly
    objects.get(name).map(|boxed| &**boxed as &dyn IStaticObject)
}

/// Call a method on a static object
pub fn call_static_method(
    object_name: &str,
    method_name: &str,
    args: &[DixValue],
) -> Result<DixValue, String> {
    if object_name.is_empty() {
        return Err("Object name cannot be empty".to_string());
    }

    if method_name.is_empty() {
        return Err("Method name cannot be empty".to_string());
    }

    let registry = StaticObjectRegistry::get();
    let objects = registry.objects.read().unwrap();

    let obj = objects
        .get(object_name)
        .ok_or_else(|| format!("Unknown static object: {}", object_name))?;

    obj.call_method(method_name, args)
}

/// Check if a static object has a specific method
pub fn has_static_method(object_name: &str, method_name: &str) -> bool {
    if object_name.is_empty() || method_name.is_empty() {
        return false;
    }

    let registry = StaticObjectRegistry::get();
    let objects = registry.objects.read().unwrap();

    objects
        .get(object_name)
        .map(|obj| obj.has_method(method_name))
        .unwrap_or(false)
}

/// Get all registered object names
pub fn get_object_names() -> Vec<String> {
    let registry = StaticObjectRegistry::get();
    let objects = registry.objects.read().unwrap();
    objects.keys().cloned().collect()
}

/// Get all method names for a specific object
pub fn get_method_names(object_name: &str) -> Vec<String> {
    if object_name.is_empty() {
        return Vec::new();
    }

    let registry = StaticObjectRegistry::get();
    let objects = registry.objects.read().unwrap();

    objects
        .get(object_name)
        .map(|obj| obj.get_method_names())
        .unwrap_or_default()
}

/// Get method signature for a specific object and method
pub fn get_method(object_name: &str, method_name: &str) -> Option<&'static dyn IBuiltinMethod> {
    if object_name.is_empty() || method_name.is_empty() {
        return None;
    }

    let registry = StaticObjectRegistry::get();
    let objects = registry.objects.read().unwrap();

    objects
        .get(object_name)
        .and_then(|obj| obj.get_method(method_name))
}

/// Validate a static method call
pub fn validate_call(
    object_name: &str,
    method_name: &str,
    arg_count: usize,
) -> ValidationResult {
    if object_name.is_empty() {
        return ValidationResult::error("Object name cannot be empty");
    }

    if method_name.is_empty() {
        return ValidationResult::error("Method name cannot be empty");
    }

    let registry = StaticObjectRegistry::get();
    let objects = registry.objects.read().unwrap();

    let obj = match objects.get(object_name) {
        Some(o) => o,
        None => return ValidationResult::error(&format!("Unknown static object: {}", object_name)),
    };

    if !obj.has_method(method_name) {
        return ValidationResult::error(&format!(
            "{} has no method: {}",
            object_name, method_name
        ));
    }

    let method = match obj.get_method(method_name) {
        Some(m) => m,
        None => return ValidationResult::error("Could not get method signature"),
    };

    // Check parameter count (-1 means variadic)
    if method.parameter_count() != -1 && method.parameter_count() as usize != arg_count {
        return ValidationResult::error(&format!(
            "{}.{} expects {} arguments, got {}",
            object_name,
            method_name,
            method.parameter_count(),
            arg_count
        ));
    }

    ValidationResult::success()
}

/// Get full registry information
pub fn get_full_registry() -> HashMap<String, Vec<String>> {
    let registry = StaticObjectRegistry::get();
    let objects = registry.objects.read().unwrap();

    let mut result = HashMap::new();
    for (name, obj) in objects.iter() {
        result.insert(name.clone(), obj.get_method_names());
    }
    result
}

/// Export registry information for documentation
pub fn export_registry_info() -> RegistryInfo {
    let registry = StaticObjectRegistry::get();
    let objects = registry.objects.read().unwrap();

    let mut object_infos = Vec::new();

    for (name, obj) in objects.iter() {
        let mut method_infos = Vec::new();

        for method_name in obj.get_method_names() {
            if let Some(method) = obj.get_method(&method_name) {
                method_infos.push(MethodInfo {
                    name: method.name().to_string(),
                    parameter_count: method.parameter_count(),
                    return_type: method.return_type(),
                    description: method.description().to_string(),
                });
            }
        }

        object_infos.push(ObjectInfo {
            name: name.clone(),
            methods: method_infos,
        });
    }

    RegistryInfo {
        objects: object_infos,
    }
}

// ==================== VALIDATION RESULT ====================

/// Validation result for static method calls
#[derive(Debug, Clone)]
pub struct ValidationResult {
    is_valid: bool,
    error_message: Option<String>,
}

impl ValidationResult {
    pub fn success() -> Self {
        ValidationResult {
            is_valid: true,
            error_message: None,
        }
    }

    pub fn error(message: &str) -> Self {
        ValidationResult {
            is_valid: false,
            error_message: Some(message.to_string()),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }
}

// ==================== REGISTRY INFORMATION TYPES ====================

use crate::Builtins::Core::DixType;

/// Complete registry information
#[derive(Debug, Clone)]
pub struct RegistryInfo {
    pub objects: Vec<ObjectInfo>,
}

/// Information about a static object
#[derive(Debug, Clone)]
pub struct ObjectInfo {
    pub name: String,
    pub methods: Vec<MethodInfo>,
}

/// Information about a method
#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    pub parameter_count: i32,
    pub return_type: DixType,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_initialization() {
        initialize_static_registry();
        let names = get_object_names();
        assert!(!names.is_empty());
        assert!(names.contains(&"Math".to_string()));
        assert!(names.contains(&"DateTime".to_string()));
    }

    #[test]
    fn test_has_static_object() {
        assert!(has_static_object("Math"));
        assert!(has_static_object("DateTime"));
        assert!(!has_static_object("NonExistent"));
    }

    #[test]
    fn test_has_static_method() {
        assert!(has_static_method("Math", "max"));
        assert!(has_static_method("DateTime", "now"));
        assert!(!has_static_method("Math", "nonexistent"));
    }

    #[test]
    fn test_validate_call() {
        let result = validate_call("Math", "max", 2);
        assert!(result.is_valid());

        let result = validate_call("Math", "max", 3);
        assert!(!result.is_valid());

        let result = validate_call("NonExistent", "method", 0);
        assert!(!result.is_valid());
    }
}