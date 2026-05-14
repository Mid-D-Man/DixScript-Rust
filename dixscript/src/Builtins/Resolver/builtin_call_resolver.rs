// src/Builtins/Resolver/builtin_call_resolver.rs
//! Main resolver for all built-in function and method calls in DixScript
//! Handles conversion from AST values to DixValues and call dispatch

use crate::Builtins::Core::{DixType, DixValue, IBuiltinMethod};
use crate::Builtins::Resolver::{
    instance_method_registry, static_object_registry,
};
use crate::Compiler::AST::values::Value as ASTValue;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Global initialization flag
static INITIALIZED: OnceLock<bool> = OnceLock::new();

/// Initialize the builtin call resolver
pub fn initialize() {
    INITIALIZED.get_or_init(|| {
        static_object_registry::initialize_static_registry();
        instance_method_registry::initialize();
        true
    });
}

/// Check if resolver is initialized
pub fn is_initialized() -> bool {
    INITIALIZED.get().is_some()
}

// ==================== STATIC METHOD CALLS ====================

/// Resolve a static method call
pub fn resolve_static_call(
    object_name: &str,
    method_name: &str,
    args: &[DixValue],
) -> Result<DixValue, String> {
    ensure_initialized();

    static_object_registry::call_static_method(object_name, method_name, args)
        .map_err(|e| format!("Error calling {}.{}: {}", object_name, method_name, e))
}

/// Resolve a static method call with argument conversion
pub fn resolve_static_call_with_conversion(
    object_name: &str,
    method_name: &str,
    args: &[ASTValue],
) -> Result<DixValue, String> {
    ensure_initialized();
    let converted_args = convert_arguments(args)?;
    resolve_static_call(object_name, method_name, &converted_args)
}

// ==================== INSTANCE METHOD CALLS ====================

/// Resolve an instance method call
pub fn resolve_instance_call(
    instance: &DixValue,
    method_name: &str,
    args: &[DixValue],
) -> Result<DixValue, String> {
    ensure_initialized();

    instance_method_registry::call_instance_method(instance, method_name, args)
        .map_err(|e| format!("Error calling {:?}.{}: {}", instance.get_type(), method_name, e))
}

/// Resolve an instance method call with argument conversion
pub fn resolve_instance_call_with_conversion(
    instance: &DixValue,
    method_name: &str,
    args: &[ASTValue],
) -> Result<DixValue, String> {
    ensure_initialized();
    let converted_args = convert_arguments(args)?;
    resolve_instance_call(instance, method_name, &converted_args)
}

// ==================== VALIDATION METHODS ====================

/// Validation result for builtin calls
#[derive(Debug, Clone)]
pub struct CallValidationResult {
    pub is_valid: bool,
    pub error_message: Option<String>,
    pub call_type: CallType,
}

impl CallValidationResult {
    pub fn success(call_type: CallType) -> Self {
        CallValidationResult {
            is_valid: true,
            error_message: None,
            call_type,
        }
    }

    pub fn error(message: String, call_type: CallType) -> Self {
        CallValidationResult {
            is_valid: false,
            error_message: Some(message),
            call_type,
        }
    }
}

impl std::fmt::Display for CallValidationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_valid {
            write!(f, "Valid {:?} call", self.call_type)
        } else {
            write!(
                f,
                "Invalid {:?} call: {}",
                self.call_type,
                self.error_message.as_ref().unwrap_or(&"Unknown error".to_string())
            )
        }
    }
}

/// Call type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallType {
    Static,
    Instance,
}

/// Validate a static call
pub fn validate_static_call(
    object_name: &str,
    method_name: &str,
    arg_count: usize,
) -> CallValidationResult {
    ensure_initialized();

    let result = static_object_registry::validate_call(object_name, method_name, arg_count);

    CallValidationResult {
        is_valid: result.is_valid(),
        error_message: result.error_message().map(|s| s.to_string()),
        call_type: CallType::Static,
    }
}

/// Validate a static call with argument types
pub fn validate_static_call_with_types(
    object_name: &str,
    method_name: &str,
    arg_types: &[DixType],
) -> CallValidationResult {
    ensure_initialized();

    // First validate basic call
    let basic_result = validate_static_call(object_name, method_name, arg_types.len());
    if !basic_result.is_valid {
        return basic_result;
    }

    // TODO: Add type-specific validation if needed
    CallValidationResult::success(CallType::Static)
}

/// Validate an instance call
pub fn validate_instance_call(
    instance_type: DixType,
    method_name: &str,
    arg_count: usize,
) -> CallValidationResult {
    ensure_initialized();

    let result = instance_method_registry::validate_instance_call(instance_type, method_name, arg_count);

    CallValidationResult {
        is_valid: result.is_valid(),
        error_message: result.error_message().map(|s| s.to_string()),
        call_type: CallType::Instance,
    }
}

// ==================== DISCOVERY METHODS ====================

/// Check if a static object exists
pub fn has_static_object(object_name: &str) -> bool {
    ensure_initialized();
    static_object_registry::has_static_object(object_name)
}

/// Check if a static method exists
pub fn has_static_method(object_name: &str, method_name: &str) -> bool {
    ensure_initialized();
    static_object_registry::has_static_method(object_name, method_name)
}

/// Check if an instance method exists
pub fn has_instance_method(dix_type: DixType, method_name: &str) -> bool {
    ensure_initialized();
    instance_method_registry::has_instance_method(dix_type, method_name)
}

/// Get all static object names
pub fn get_static_objects() -> Vec<String> {
    ensure_initialized();
    static_object_registry::get_object_names()
}

/// Get all static method names for an object
pub fn get_static_methods(object_name: &str) -> Vec<String> {
    ensure_initialized();
    static_object_registry::get_method_names(object_name)
}

/// Get all instance method names for a type
pub fn get_instance_methods(dix_type: DixType) -> Vec<String> {
    ensure_initialized();
    instance_method_registry::get_instance_methods(dix_type)
}

// ==================== SIGNATURE INFORMATION ====================

/// Get static method signature
pub fn get_static_method_signature(
    object_name: &str,
    method_name: &str,
) -> Option<&'static dyn IBuiltinMethod> {
    ensure_initialized();
    static_object_registry::get_method(object_name, method_name)
}

/// Get instance method signature
pub fn get_instance_method_signature(
    dix_type: DixType,
    method_name: &str,
) -> Option<&'static dyn IBuiltinMethod> {
    ensure_initialized();
    instance_method_registry::get_instance_method(dix_type, method_name)
}

// ==================== ARGUMENT CONVERSION ====================

/// Convert array of AST values to DixValues
fn convert_arguments(args: &[ASTValue]) -> Result<Vec<DixValue>, String> {
    args.iter().map(convert_to_dix_value).collect()
}

/// Convert a single AST value to DixValue
/// CRITICAL: Handles all AST value types including PrefixedConstructor
pub fn convert_to_dix_value(value: &ASTValue) -> Result<DixValue, String> {
    match value {
        ASTValue::Integer { value, .. } => Ok(DixValue::from_int(*value)),

        ASTValue::Long { value, .. } => Ok(DixValue::from_long(*value)),

        ASTValue::Float { value, .. } => Ok(DixValue::from_float(*value)),

        ASTValue::Double { value, .. } => Ok(DixValue::from_double(*value)),

        ASTValue::ScientificNotation { value, .. } => Ok(DixValue::from_double(*value)),

        ASTValue::String { value, .. } => Ok(DixValue::from_string(value.clone())),

        ASTValue::Boolean { value, .. } => Ok(DixValue::from_bool(*value)),

        ASTValue::Date { value, .. } => {
            use chrono::NaiveDate;
            let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date format: {}", e))?
                .and_hms_opt(0, 0, 0)
                .ok_or("Failed to create date")?;
            Ok(DixValue::from_date(chrono::DateTime::from_naive_utc_and_offset(
                date,
                chrono::Utc,
            )))
        }

        ASTValue::Timestamp { value, .. } => {
            let timestamp = value.parse::<chrono::DateTime<chrono::Utc>>()
                .map_err(|e| format!("Invalid timestamp format: {}", e))?;
            Ok(DixValue::from_timestamp(timestamp))
        }

        ASTValue::HexColor { value, .. } => Ok(DixValue::from_hex(value.clone())),

        ASTValue::Null { .. } => Ok(DixValue::null()),

        ASTValue::Array { values, .. } => {
            let converted: Result<Vec<DixValue>, String> =
                values.iter().map(convert_to_dix_value).collect();
            Ok(DixValue::from_array(converted?))
        }

        ASTValue::NestedArray { values, .. } => {
            let converted: Result<Vec<DixValue>, String> =
                values.iter().map(convert_to_dix_value).collect();
            Ok(DixValue::from_array(converted?))
        }

        ASTValue::Object { properties, .. } => {
            let mut obj = HashMap::new();
            for prop in properties {
                let value = convert_to_dix_value(&prop.value)?;
                obj.insert(prop.key.clone(), value);
            }
            Ok(DixValue::from_object(obj))
        }

        // CRITICAL: PrefixedConstructor (Blob, Regex, Tuple)
        ASTValue::PrefixedConstructor { prefix, arguments, .. } => {
            convert_prefixed_constructor(prefix, arguments)
        }

        ASTValue::EnumValue { value, .. } => {
            // Enum values will be resolved separately
            Ok(DixValue::from_string(value.clone()))
        }

        ASTValue::Identifier { value, .. } => {
            // Identifiers need to be resolved in the runtime context
            Err(format!("Cannot convert identifier '{}' to value at this stage", value))
        }

        ASTValue::Expression { .. } => {
            Err("Cannot convert expression to value - must be evaluated first".to_string())
        }

        ASTValue::QuickFuncCall { .. } => {
            Err("Cannot convert function call to value - must be evaluated first".to_string())
        }

        ASTValue::Range { .. } => {
            Err("Range values not yet implemented".to_string())
        }

        ASTValue::Lambda { .. } => {
            Err("Lambda values not yet implemented".to_string())
        }

        ASTValue::InterpolatedString { .. } => {
            Err("Interpolated strings must be evaluated first".to_string())
        }

        ASTValue::ParseError { message, .. } => {
            Err(format!("Parse error in value: {}", message))
        }

        ASTValue::Error { message, .. } => {
            Err(format!("Error in value: {}", message))
        }

        ASTValue::Unknown { element_type, element_name, .. } => {
            Err(format!("Unknown value type: {} ({})", element_type, element_name))
        }
    }
}

/// Convert PrefixedConstructor to DixValue
/// Handles: b:(blob), r:(regex), t:(tuple)
fn convert_prefixed_constructor(
    prefix: &str,
    arguments: &[ASTValue],
) -> Result<DixValue, String> {
    match prefix.to_lowercase().as_str() {
        "b" => {
            // Blob - expects base64 string
            if arguments.is_empty() {
                return DixValue::from_blob(String::new());
            }

            let base64_data = match &arguments[0] {
                ASTValue::String { value, .. } => value.clone(),
                other => convert_to_dix_value(other)?.as_string(),
            };

            DixValue::from_blob(base64_data)
        }

        "r" => {
            // Regex - expects pattern string
            if arguments.is_empty() {
                return DixValue::from_regex(".*".to_string());
            }

            let pattern = match &arguments[0] {
                ASTValue::String { value, .. } => value.clone(),
                other => convert_to_dix_value(other)?.as_string(),
            };

            DixValue::from_regex(pattern)
        }

        "t" => {
            // Tuple - convert all arguments
            let tuple_values: Result<Vec<DixValue>, String> =
                arguments.iter().map(convert_to_dix_value).collect();
            Ok(DixValue::from_tuple(tuple_values?))
        }

        _ => Err(format!("Unknown prefixed constructor: {}", prefix)),
    }
}

// ==================== DOCUMENTATION GENERATION ====================

/// Generate comprehensive documentation
pub fn generate_documentation() -> String {
    ensure_initialized();

    let mut doc = String::new();
    doc.push_str("# DixScript Built-in Functions and Methods\n\n");

    // Static objects
    doc.push_str("## Static Objects\n\n");

    for object_name in get_static_objects() {
        doc.push_str(&format!("### {}\n\n", object_name));

        for method_name in get_static_methods(&object_name) {
            if let Some(method) = get_static_method_signature(&object_name, &method_name) {
                let param_string = get_parameter_string(method, false);
                doc.push_str(&format!(
                    "- **{}.{}({})** → `{}`\n",
                    object_name,
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

    // Instance methods
    doc.push_str("## Instance Methods\n\n");

    for dix_type in get_types_with_instance_methods() {
        if dix_type == DixType::Void {
            continue;
        }

        let methods = get_instance_methods(dix_type);
        if !methods.is_empty() {
            doc.push_str(&format!("### {}\n\n", dix_type.get_type_name()));

            for method_name in methods {
                if let Some(method) = get_instance_method_signature(dix_type, &method_name) {
                    let param_string = get_parameter_string(method, true);
                    doc.push_str(&format!(
                        "- **{}.{}({})** → `{}`\n",
                        dix_type.get_type_name(),
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

    doc
}

/// Get parameter string for documentation
fn get_parameter_string(method: &dyn IBuiltinMethod, is_instance: bool) -> String {
    let param_count = if is_instance {
        method.parameter_count().saturating_sub(1).max(0)
    } else {
        method.parameter_count()
    };

    if param_count <= 0 {
        return String::new();
    }

    (1..=param_count)
        .map(|i| format!("arg{}", i))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Get all types that have instance methods
fn get_types_with_instance_methods() -> Vec<DixType> {
    instance_method_registry::get_types_with_methods()
}

// ==================== HELPER METHODS ====================

/// Ensure the resolver is initialized
fn ensure_initialized() {
    if !is_initialized() {
        initialize();
    }
}

/// Determine call type from expression
pub fn determine_call_type(call_expression: &str) -> CallType {
    if call_expression.contains('.') {
        let parts: Vec<&str> = call_expression.split('.').collect();
        if parts.len() >= 2 && !parts[0].is_empty() {
            if parts[0].chars().next().map_or(false, |c| c.is_uppercase()) {
                return CallType::Static;
            }
        }
    }
    CallType::Instance
}

/// Reset the resolver (for testing)
pub fn reset() {
    // Note: OnceLock doesn't support reset in stable Rust
    // This would need to be handled differently in production
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialization() {
        initialize();
        assert!(is_initialized());
    }

    #[test]
    fn test_has_static_object() {
        initialize();
        assert!(has_static_object("Math"));
        assert!(has_static_object("DateTime"));
        assert!(!has_static_object("NonExistent"));
    }

    #[test]
    fn test_has_static_method() {
        initialize();
        assert!(has_static_method("Math", "max"));
        assert!(!has_static_method("Math", "nonexistent"));
    }

    #[test]
    fn test_determine_call_type() {
        assert_eq!(determine_call_type("Math.max"), CallType::Static);
        assert_eq!(determine_call_type("myVar.toString"), CallType::Instance);
    }
}
