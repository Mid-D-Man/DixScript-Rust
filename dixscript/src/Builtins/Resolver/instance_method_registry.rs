// dixscript/src/Builtins/Resolver/instance_method_registry.rs
//! Instance Method Registry — Central registry for all instance methods.
//! Handles type-based method dispatch for instance calls.

use crate::Builtins::Core::{DixType, DixValue, IBuiltinMethod};
use crate::Builtins::Instance::{
    array_methods, blob_methods, number_methods, regex_methods,
    string_methods, tuple_methods, universal_methods,
};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Global instance method registry
static REGISTRY: OnceLock<InstanceMethodRegistry> = OnceLock::new();

/// All DixTypes that can carry instance methods (used in universal-method loop).
/// Long is now included so it receives the same universal set as Int/Float/Double.
const ALL_INSTANCE_TYPES: &[DixType] = &[
    DixType::Int,
    DixType::Long,
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
];

/// Registry for all instance methods
pub struct InstanceMethodRegistry {
    type_methods:      HashMap<DixType, HashMap<String, Box<dyn IBuiltinMethod>>>,
    universal_methods: HashMap<String, Box<dyn IBuiltinMethod>>,
}

impl InstanceMethodRegistry {
    /// Create and initialise the registry
    fn new() -> Self {
        let mut registry = InstanceMethodRegistry {
            type_methods:      HashMap::new(),
            universal_methods: HashMap::new(),
        };
        registry.initialize_methods();
        registry
    }

    /// Register all instance methods
    fn initialize_methods(&mut self) {
        self.register_string_methods();
        self.register_number_methods();
        self.register_array_methods();
        self.register_tuple_methods();
        self.register_blob_methods();
        self.register_regex_methods();
        self.register_universal_methods();
    }

    fn register_string_methods(&mut self) {
        self.type_methods
            .insert(DixType::String, string_methods::get_methods());
    }

    fn register_number_methods(&mut self) {
        self.type_methods
            .insert(DixType::Int,    number_methods::get_int_methods());
        self.type_methods
            .insert(DixType::Long,   number_methods::get_long_methods());   // ← was missing
        self.type_methods
            .insert(DixType::Float,  number_methods::get_float_methods());
        self.type_methods
            .insert(DixType::Double, number_methods::get_double_methods());
    }

    fn register_array_methods(&mut self) {
        self.type_methods
            .insert(DixType::Array, array_methods::get_methods());
    }

    fn register_tuple_methods(&mut self) {
        self.type_methods
            .insert(DixType::Tuple, tuple_methods::get_methods());
    }

    fn register_blob_methods(&mut self) {
        self.type_methods
            .insert(DixType::Blob, blob_methods::get_methods());
    }

    fn register_regex_methods(&mut self) {
        self.type_methods
            .insert(DixType::Regex, regex_methods::get_methods());
    }

    /// Register universal methods and attach them to every type.
    ///
    /// Universal methods are NOT added when a type already has a same-named
    /// method (type-specific variants take precedence).
    fn register_universal_methods(&mut self) {
        // Store the canonical set for public inspection
        self.universal_methods = universal_methods::get_methods();

        // Attach universals to every type in ALL_INSTANCE_TYPES
        for &dix_type in ALL_INSTANCE_TYPES {
            let type_methods = self
                .type_methods
                .entry(dix_type)
                .or_insert_with(HashMap::new);

            // Get a fresh copy for this type (each Box needs an independent owner)
            let universal_for_type = universal_methods::get_methods();

            for (name, method) in universal_for_type {
                // Don't overwrite type-specific methods with the universal version
                if !type_methods.contains_key(&name) {
                    type_methods.insert(name, method);
                }
            }
        }
    }

    /// Get the singleton registry instance
    fn get() -> &'static InstanceMethodRegistry {
        REGISTRY.get_or_init(InstanceMethodRegistry::new)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════════════

/// Force initialisation of the registry (idempotent)
pub fn initialize() {
    let _ = InstanceMethodRegistry::get();
}

/// Call an instance method on a value
pub fn call_instance_method(
    instance:    &DixValue,
    method_name: &str,
    args:        &[DixValue],
) -> Result<DixValue, String> {
    let registry = InstanceMethodRegistry::get();

    let methods = registry
        .type_methods
        .get(&instance.get_type())
        .ok_or_else(|| format!("Type {:?} has no methods", instance.get_type()))?;

    let method = methods
        .get(method_name)
        .ok_or_else(|| format!(
            "Type {:?} has no method: {}",
            instance.get_type(), method_name
        ))?;

    // Prepend instance as argument 0
    let mut all_args = Vec::with_capacity(args.len() + 1);
    all_args.push(instance.clone());
    all_args.extend_from_slice(args);

    method.call(&all_args)
}

/// Check if a type has a specific instance method
pub fn has_instance_method(dix_type: DixType, method_name: &str) -> bool {
    InstanceMethodRegistry::get()
        .type_methods
        .get(&dix_type)
        .map(|m| m.contains_key(method_name))
        .unwrap_or(false)
}

/// Get all instance method names for a type (includes universals)
pub fn get_instance_methods(dix_type: DixType) -> Vec<String> {
    InstanceMethodRegistry::get()
        .type_methods
        .get(&dix_type)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// Get a reference to a specific instance method (used by type inference / hover)
pub fn get_instance_method(
    dix_type:    DixType,
    method_name: &str,
) -> Option<&'static dyn IBuiltinMethod> {
    InstanceMethodRegistry::get()
        .type_methods
        .get(&dix_type)
        .and_then(|m| m.get(method_name))
        .map(|boxed| &**boxed as &dyn IBuiltinMethod)
}

/// Validate an instance method call at compile time
pub fn validate_instance_call(
    instance_type: DixType,
    method_name:   &str,
    arg_count:     usize,
) -> ValidationResult {
    if method_name.is_empty() {
        return ValidationResult::error("Method name cannot be empty");
    }

    let registry = InstanceMethodRegistry::get();

    let methods = match registry.type_methods.get(&instance_type) {
        Some(m) => m,
        None => return ValidationResult::error(
            &format!("Type {:?} has no methods", instance_type)
        ),
    };

    let method = match methods.get(method_name) {
        Some(m) => m,
        None => return ValidationResult::error(
            &format!("Type {:?} has no method: {}", instance_type, method_name)
        ),
    };

    // parameter_count includes the instance as arg 0 — subtract 1
    let expected = if method.parameter_count() > 0 {
        (method.parameter_count() - 1) as usize
    } else {
        0
    };

    if expected != arg_count {
        return ValidationResult::error(&format!(
            "{:?}.{} expects {} arguments, got {}",
            instance_type, method_name, expected, arg_count
        ));
    }

    ValidationResult::success()
}

/// Check if a method is universal (available on all types)
pub fn is_universal_method(method_name: &str) -> bool {
    InstanceMethodRegistry::get()
        .universal_methods
        .contains_key(method_name)
}

/// Get all universal method names
pub fn get_universal_methods() -> Vec<String> {
    InstanceMethodRegistry::get()
        .universal_methods
        .keys()
        .cloned()
        .collect()
}

/// Get all registered types that have instance methods
pub fn get_types_with_methods() -> Vec<DixType> {
    InstanceMethodRegistry::get()
        .type_methods
        .keys()
        .copied()
        .collect()
}

/// Get method count for a specific type (useful for tests / diagnostics)
pub fn get_method_count(dix_type: DixType) -> usize {
    InstanceMethodRegistry::get()
        .type_methods
        .get(&dix_type)
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Generate Markdown documentation for all instance methods
pub fn generate_documentation() -> String {
    let registry = InstanceMethodRegistry::get();
    let mut doc  = String::new();

    doc.push_str("# DixScript Instance Methods\n\n");

    // Universal methods
    doc.push_str("## Universal Methods (available on all types)\n\n");
    let mut universal_names: Vec<_> = registry.universal_methods.keys().collect();
    universal_names.sort();

    for name in universal_names {
        if let Some(method) = registry.universal_methods.get(name) {
            doc.push_str(&format!(
                "- **{}()** → `{}`\n",
                name,
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
            // Only list methods that are NOT in the universal set
            let type_specific: Vec<_> = methods
                .iter()
                .filter(|(name, _)| !registry.universal_methods.contains_key(*name))
                .collect();

            if type_specific.is_empty() { continue; }

            doc.push_str(&format!("### {}\n\n", dix_type.get_type_name()));

            let mut names: Vec<_> = type_specific.iter().map(|(n, _)| *n).collect();
            names.sort();

            for method_name in names {
                if let Some(method) = methods.get(method_name) {
                    let extra_params = (method.parameter_count() as i32 - 1).max(0) as usize;
                    let param_str = if extra_params > 0 {
                        (1..=extra_params)
                            .map(|i| format!("arg{}", i))
                            .collect::<Vec<_>>()
                            .join(", ")
                    } else {
                        String::new()
                    };
                    doc.push_str(&format!(
                        "- **{}({})** → `{}`\n",
                        method_name,
                        param_str,
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

    doc
}

// ═══════════════════════════════════════════════════════════════════════════════
// Validation result type
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct ValidationResult {
    is_valid:      bool,
    error_message: Option<String>,
}

impl ValidationResult {
    pub fn success() -> Self {
        ValidationResult { is_valid: true, error_message: None }
    }

    pub fn error(message: &str) -> Self {
        ValidationResult { is_valid: false, error_message: Some(message.to_string()) }
    }

    pub fn is_valid(&self)        -> bool            { self.is_valid }
    pub fn error_message(&self)   -> Option<&str>    { self.error_message.as_deref() }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

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
    fn long_type_is_registered() {
        initialize();
        assert!(
            has_instance_method(DixType::Long, "abs"),
            "Long.abs should be registered"
        );
        assert!(
            has_instance_method(DixType::Long, "toString"),
            "Long.toString should be registered"
        );
        assert!(
            has_instance_method(DixType::Long, "isEven"),
            "Long.isEven should be registered"
        );
        assert!(
            has_instance_method(DixType::Long, "fitsInInt"),
            "Long.fitsInInt should be registered"
        );
    }

    #[test]
    fn long_receives_universal_methods() {
        initialize();
        let universals = get_universal_methods();
        for name in &universals {
            assert!(
                has_instance_method(DixType::Long, name),
                "Long should have universal method: {}",
                name
            );
        }
    }

    #[test]
    fn test_has_instance_method() {
        initialize();
        assert!(has_instance_method(DixType::String, "toUpper"));
        assert!(has_instance_method(DixType::Int,    "abs"));
        assert!(has_instance_method(DixType::Long,   "abs"));
        assert!(has_instance_method(DixType::Float,  "round"));
        assert!(has_instance_method(DixType::Double, "round"));
        assert!(!has_instance_method(DixType::String, "nonexistent"));
    }

    #[test]
    fn test_universal_methods() {
        initialize();
        let universal = get_universal_methods();
        assert!(!universal.is_empty());
        // These two are always present in universal_methods
        assert!(is_universal_method("toString"));
        assert!(is_universal_method("type"));
    }

    #[test]
    fn test_call_string_to_upper() {
        initialize();
        let value  = DixValue::from_string("hello".to_string());
        let result = call_instance_method(&value, "toUpper", &[]).unwrap();
        assert_eq!(result.as_string(), "HELLO");
    }

    #[test]
    fn test_call_long_abs() {
        initialize();
        let value  = DixValue::from_long(-9_000_000_000_i64);
        let result = call_instance_method(&value, "abs", &[]).unwrap();
        assert_eq!(result.as_long(), 9_000_000_000_i64);
    }

    #[test]
    fn all_numeric_types_have_abs() {
        initialize();
        for &dt in &[DixType::Int, DixType::Long, DixType::Float, DixType::Double] {
            assert!(
                has_instance_method(dt, "abs"),
                "{:?} missing abs()", dt
            );
        }
    }

    #[test]
    fn tuple_methods_registered() {
        initialize();
        let methods = get_instance_methods(DixType::Tuple);
        // Tuple should have positional accessors
        assert!(
            methods.iter().any(|m| m == "first" || m == "get" || m == "length"),
            "Tuple methods missing positional accessors; got: {:?}", methods
        );
    }

    #[test]
    fn method_count_sanity() {
        initialize();
        // String should have a good number of methods
        assert!(get_method_count(DixType::String) >= 10,
            "String has too few methods: {}", get_method_count(DixType::String));
        // Long should now have methods too
        assert!(get_method_count(DixType::Long) >= 5,
            "Long has too few methods: {}", get_method_count(DixType::Long));
    }
}
