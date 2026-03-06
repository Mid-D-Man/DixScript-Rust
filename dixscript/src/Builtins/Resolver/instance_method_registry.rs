// src/Builtins/Resolver/instance_method_registry.rs
//! Instance Method Registry - Central registry for all instance methods
//! Handles type-based method dispatch for instance calls

use crate::Builtins::Core::{DixType, DixValue, IBuiltinMethod};
use crate::Builtins::Instance::{
    array_methods, blob_methods, number_methods, regex_methods,
    string_methods, tuple_methods, universal_methods,
};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Global instance method registry
static REGISTRY: OnceLock<InstanceMethodRegistry> = OnceLock::new();

/// Registry for all instance methods
pub struct InstanceMethodRegistry {
    type_methods: HashMap<DixType, HashMap<String, Box<dyn IBuiltinMethod>>>,
    universal_methods: HashMap<String, Box<dyn IBuiltinMethod>>,
}

impl InstanceMethodRegistry {
    /// Create and initialize the registry
    fn new() -> Self {
        let mut registry = InstanceMethodRegistry {
            type_methods: HashMap::new(),
            universal_methods: HashMap::new(),
        };

        registry.initialize_methods();
        registry
    }

    /// Initialize all instance methods
    fn initialize_methods(&mut self) {
        self.register_string_methods();
        self.register_number_methods();
        self.register_array_methods();
        self.register_tuple_methods();
        self.register_blob_methods();
        self.register_regex_methods();
        self.register_universal_methods();
    }

    /// Register string instance methods
    fn register_string_methods(&mut self) {
        self.type_methods
            .insert(DixType::String, string_methods::get_methods());
    }

    /// Register number instance methods
    fn register_number_methods(&mut self) {
        self.type_methods
            .insert(DixType::Int, number_methods::get_int_methods());
        self.type_methods
            .insert(DixType::Float, number_methods::get_float_methods());
        self.type_methods
            .insert(DixType::Double, number_methods::get_double_methods());
    }

    /// Register array instance methods
    fn register_array_methods(&mut self) {
        self.type_methods
            .insert(DixType::Array, array_methods::get_methods());
    }

    /// Register tuple instance methods
    fn register_tuple_methods(&mut self) {
        self.type_methods
            .insert(DixType::Tuple, tuple_methods::get_methods());
    }

    /// Register blob instance methods
    fn register_blob_methods(&mut self) {
        self.type_methods
            .insert(DixType::Blob, blob_methods::get_methods());
    }

    /// Register regex instance methods
    fn register_regex_methods(&mut self) {
        self.type_methods
            .insert(DixType::Regex, regex_methods::get_methods());
    }

    /// Register universal methods (available on all types)
    fn register_universal_methods(&mut self) {
        // Get universal methods once and store them
        self.universal_methods = universal_methods::get_methods();

        // Add universal methods to all types
        for dix_type in [
            DixType::Int,
            DixType::Float,
            DixType::Double,
            DixType::String,
            DixType::Bool,
            DixType::Array,
            DixType::Tuple,
            DixType::Object,
            DixType::Hex,
            DixType::Blob,
            DixType::Regex,
            DixType::Date,
            DixType::Timestamp,
            DixType::Enum,
            DixType::Null,
        ] {
            let type_methods = self.type_methods.entry(dix_type).or_insert_with(HashMap::new);

            // Get a fresh set of universal methods for this type
            let universal_for_type = universal_methods::get_methods();

            // Add universal methods, but don't override type-specific methods
            for (name, method) in universal_for_type {
                if !type_methods.contains_key(&name) {
                    type_methods.insert(name, method);
                }
            }
        }
    }

    /// Get the global registry instance
    fn get() -> &'static InstanceMethodRegistry {
        REGISTRY.get_or_init(InstanceMethodRegistry::new)
    }
}

// ==================== PUBLIC API ====================

/// Initialize the instance method registry
pub fn initialize() {
    // Force initialization of the registry
    let _ = InstanceMethodRegistry::get();
}

/// Call an instance method on a value
pub fn call_instance_method(
    instance: &DixValue,
    method_name: &str,
    args: &[DixValue],
) -> Result<DixValue, String> {
    let registry = InstanceMethodRegistry::get();

    // Get methods for the instance type
    let methods = registry
        .type_methods
        .get(&instance.get_type())
        .ok_or_else(|| format!("Type {:?} has no methods", instance.get_type()))?;

    // Find the method
    let method = methods
        .get(method_name)
        .ok_or_else(|| format!("Type {:?} has no method: {}", instance.get_type(), method_name))?;

    // Prepare arguments with instance as first parameter
    let mut all_args = Vec::with_capacity(args.len() + 1);
    all_args.push(instance.clone());
    all_args.extend_from_slice(args);

    // Call the method
    method.call(&all_args)
}

/// Check if a type has a specific instance method
pub fn has_instance_method(dix_type: DixType, method_name: &str) -> bool {
    let registry = InstanceMethodRegistry::get();

    registry
        .type_methods
        .get(&dix_type)
        .map(|methods| methods.contains_key(method_name))
        .unwrap_or(false)
}

/// Get all instance methods for a type
pub fn get_instance_methods(dix_type: DixType) -> Vec<String> {
    let registry = InstanceMethodRegistry::get();

    registry
        .type_methods
        .get(&dix_type)
        .map(|methods| methods.keys().cloned().collect())
        .unwrap_or_default()
}

/// Get method signature for a type and method
pub fn get_instance_method(
    dix_type: DixType,
    method_name: &str,
) -> Option<&'static dyn IBuiltinMethod> {
    let registry = InstanceMethodRegistry::get();

    registry
        .type_methods
        .get(&dix_type)
        .and_then(|methods| methods.get(method_name))
        .map(|boxed| &**boxed as &dyn IBuiltinMethod)
}

/// Validate an instance method call at compile time
pub fn validate_instance_call(
    instance_type: DixType,
    method_name: &str,
    arg_count: usize,
) -> ValidationResult {
    if method_name.is_empty() {
        return ValidationResult::error("Method name cannot be empty");
    }

    let registry = InstanceMethodRegistry::get();

    let methods = match registry.type_methods.get(&instance_type) {
        Some(m) => m,
        None => return ValidationResult::error(&format!("Type {:?} has no methods", instance_type)),
    };

    let method = match methods.get(method_name) {
        Some(m) => m,
        None => {
            return ValidationResult::error(&format!(
                "Type {:?} has no method: {}",
                instance_type, method_name
            ))
        }
    };

    // Check parameter count (subtract 1 for instance parameter)
    let expected_params = if method.parameter_count() > 0 {
        (method.parameter_count() - 1) as usize
    } else {
        0
    };

    if expected_params != arg_count {
        return ValidationResult::error(&format!(
            "{:?}.{} expects {} arguments, got {}",
            instance_type, method_name, expected_params, arg_count
        ));
    }

    ValidationResult::success()
}

/// Check if a method is universal (available on all types)
pub fn is_universal_method(method_name: &str) -> bool {
    let registry = InstanceMethodRegistry::get();
    registry.universal_methods.contains_key(method_name)
}

/// Get all universal method names
pub fn get_universal_methods() -> Vec<String> {
    let registry = InstanceMethodRegistry::get();
    registry.universal_methods.keys().cloned().collect()
}

/// Get all registered types that have instance methods
pub fn get_types_with_methods() -> Vec<DixType> {
    let registry = InstanceMethodRegistry::get();
    registry.type_methods.keys().copied().collect()
}

/// Get method count for a specific type
pub fn get_method_count(dix_type: DixType) -> usize {
    let registry = InstanceMethodRegistry::get();
    registry
        .type_methods
        .get(&dix_type)
        .map(|methods| methods.len())
        .unwrap_or(0)
}

/// Generate documentation for instance methods
pub fn generate_documentation() -> String {
    let registry = InstanceMethodRegistry::get();
    let mut doc = String::new();

    doc.push_str("# DixScript Instance Methods\n\n");

    doc.push_str("## Universal Methods (Available on all types)\n\n");
    let mut universal_names: Vec<_> = registry.universal_methods.keys().collect();
    universal_names.sort();

    for method_name in universal_names {
        if let Some(method) = registry.universal_methods.get(method_name) {
            doc.push_str(&format!(
                "- **{}()** → `{}`\n",
                method_name,
                method.return_type().get_type_name()
            ));
            if !method.description().is_empty() {
                doc.push_str(&format!("  - {}\n", method.description()));
            }
            doc.push('\n');
        }
    }

    doc.push_str("## Type-Specific Methods\n\n");

    let mut types: Vec<_> = registry.type_methods.keys().collect();
    types.sort_by_key(|t| t.get_type_name());

    for dix_type in types {
        if let Some(methods) = registry.type_methods.get(dix_type) {
            // Filter out universal methods
            let type_specific: Vec<_> = methods
                .iter()
                .filter(|(name, _)| !registry.universal_methods.contains_key(*name))
                .collect();

            if !type_specific.is_empty() {
                doc.push_str(&format!("### {}\n\n", dix_type.get_type_name()));

                let mut method_names: Vec<_> = type_specific.iter().map(|(name, _)| *name).collect();
                method_names.sort();

                for method_name in method_names {
                    if let Some(method) = methods.get(method_name) {
                        let param_count = if method.parameter_count() > 0 {
                            method.parameter_count() - 1
                        } else {
                            0
                        };
                        let param_string = if param_count > 0 {
                            (1..=param_count)
                                .map(|i| format!("arg{}", i))
                                .collect::<Vec<_>>()
                                .join(", ")
                        } else {
                            String::new()
                        };

                        doc.push_str(&format!(
                            "- **{}({})** → `{}`\n",
                            method_name,
                            param_string,
                            method.return_type().get_type_name()
                        ));
                        if !method.description().is_empty() {
                            doc.push_str(&format!("  - {}\n", method.description()));
                        }
                        doc.push('\n');
                    }
                }
            }
        }
    }

    doc
}

// ==================== VALIDATION RESULT ====================

/// Validation result for instance method calls
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_initialization() {
        initialize();
        let types = get_types_with_methods();
        assert!(!types.is_empty());
    }

    #[test]
    fn test_has_instance_method() {
        initialize();
        assert!(has_instance_method(DixType::String, "toUpper"));
        assert!(has_instance_method(DixType::Int, "abs"));
        assert!(!has_instance_method(DixType::String, "nonexistent"));
    }

    #[test]
    fn test_universal_methods() {
        initialize();
        let universal = get_universal_methods();
        assert!(!universal.is_empty());
        assert!(is_universal_method("toString"));
        assert!(is_universal_method("type"));
    }

    #[test]
    fn test_call_instance_method() {
        initialize();
        let value = DixValue::from_string("hello".to_string());
        let result = call_instance_method(&value, "toUpper", &[]).unwrap();
        assert_eq!(result.as_string(), "HELLO");
    }
}