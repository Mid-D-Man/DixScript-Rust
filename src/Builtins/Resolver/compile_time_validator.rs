// src/Builtins/Resolver/compile_time_validator.rs
//! Compile-time validator for built-in function and method calls
//! Provides early error detection and type checking

use crate::Builtins::Core::{DixType, DixValue, IBuiltinMethod};
use crate::Builtins::Resolver::{
    builtin_call_resolver::{CallType, CallValidationResult},
    instance_method_registry, static_object_registry,
};

/// Initialize the validator
pub fn initialize() {
    crate::Builtins::Resolver::builtin_call_resolver::initialize();
}

// ==================== STATIC CALL VALIDATION ====================

/// Validate a static method call at compile time
pub fn validate_static_call(
    object_name: &str,
    method_name: &str,
    arg_count: usize,
    line_number: Option<usize>,
    column_number: Option<usize>,
) -> CallValidationResult {
    // Basic validation
    if object_name.is_empty() {
        return create_error("Static object name cannot be empty", line_number, column_number);
    }

    if method_name.is_empty() {
        return create_error("Method name cannot be empty", line_number, column_number);
    }

    // Check if static object exists
    if !static_object_registry::has_static_object(object_name) {
        return create_error(
            &format!("Unknown static object: {}", object_name),
            line_number,
            column_number,
        );
    }

    // Check if method exists
    if !static_object_registry::has_static_method(object_name, method_name) {
        return create_error(
            &format!("{} has no method: {}", object_name, method_name),
            line_number,
            column_number,
        );
    }

    // Check parameter count
    if let Some(method) = static_object_registry::get_method(object_name, method_name) {
        let expected_count = method.parameter_count();
        if expected_count != -1 && expected_count as usize != arg_count {
            return create_error(
                &format!(
                    "{}.{} expects {} arguments, got {}",
                    object_name, method_name, expected_count, arg_count
                ),
                line_number,
                column_number,
            );
        }
    }

    CallValidationResult::success(CallType::Static)
}

/// Validate a static method call with argument types
pub fn validate_static_call_with_types(
    object_name: &str,
    method_name: &str,
    arg_types: &[DixType],
    line_number: Option<usize>,
    column_number: Option<usize>,
) -> CallValidationResult {
    // First do basic validation
    let basic_result = validate_static_call(
        object_name,
        method_name,
        arg_types.len(),
        line_number,
        column_number,
    );

    if !basic_result.is_valid {
        return basic_result;
    }

    // Get method for type validation
    if let Some(method) = static_object_registry::get_method(object_name, method_name) {
        // Create dummy values for validation
        let dummy_args: Result<Vec<DixValue>, String> =
            arg_types.iter().map(|&t| create_dummy_value(t)).collect();

        match dummy_args {
            Ok(args) => {
                if !method.validate_arguments(&args) {
                    return create_error(
                        &format!("Invalid argument types for {}.{}", object_name, method_name),
                        line_number,
                        column_number,
                    );
                }
            }
            Err(e) => {
                return create_error(&e, line_number, column_number);
            }
        }
    }

    CallValidationResult::success(CallType::Static)
}

// ==================== INSTANCE CALL VALIDATION ====================

/// Validate an instance method call at compile time
pub fn validate_instance_call(
    instance_type: DixType,
    method_name: &str,
    arg_count: usize,
    line_number: Option<usize>,
    column_number: Option<usize>,
) -> CallValidationResult {
    // Basic validation
    if method_name.is_empty() {
        return create_error("Method name cannot be empty", line_number, column_number);
    }

    // Check if type has methods
    if !instance_method_registry::has_instance_method(instance_type, method_name) {
        return create_error(
            &format!("Type {:?} has no method: {}", instance_type, method_name),
            line_number,
            column_number,
        );
    }

    // Check parameter count
    if let Some(method) = instance_method_registry::get_instance_method(instance_type, method_name)
    {
        let expected_params = method.parameter_count().saturating_sub(1).max(0);
        if expected_params as usize != arg_count {
            return create_error(
                &format!(
                    "{:?}.{} expects {} arguments, got {}",
                    instance_type, method_name, expected_params, arg_count
                ),
                line_number,
                column_number,
            );
        }
    }

    CallValidationResult::success(CallType::Instance)
}

/// Validate an instance method call with argument types
pub fn validate_instance_call_with_types(
    instance_type: DixType,
    method_name: &str,
    arg_types: &[DixType],
    line_number: Option<usize>,
    column_number: Option<usize>,
) -> CallValidationResult {
    // First do basic validation
    let basic_result = validate_instance_call(
        instance_type,
        method_name,
        arg_types.len(),
        line_number,
        column_number,
    );

    if !basic_result.is_valid {
        return basic_result;
    }

    // Get method for type validation
    if let Some(method) = instance_method_registry::get_instance_method(instance_type, method_name)
    {
        // Create dummy instance + arguments
        let dummy_instance = create_dummy_value(instance_type);
        if let Err(e) = dummy_instance {
            return create_error(&e, line_number, column_number);
        }

        let dummy_args: Result<Vec<DixValue>, String> =
            arg_types.iter().map(|&t| create_dummy_value(t)).collect();

        match dummy_args {
            Ok(mut args) => {
                // Prepend instance
                args.insert(0, dummy_instance.unwrap());

                if !method.validate_arguments(&args) {
                    return create_error(
                        &format!(
                            "Invalid argument types for {:?}.{}",
                            instance_type, method_name
                        ),
                        line_number,
                        column_number,
                    );
                }
            }
            Err(e) => {
                return create_error(&e, line_number, column_number);
            }
        }
    }

    CallValidationResult::success(CallType::Instance)
}

// ==================== BATCH VALIDATION ====================

/// Request for call validation
#[derive(Debug, Clone)]
pub struct CallValidationRequest {
    pub call_type: CallType,
    pub object_name: Option<String>,
    pub instance_type: Option<DixType>,
    pub method_name: String,
    pub argument_count: usize,
    pub argument_types: Option<Vec<DixType>>,
    pub line_number: Option<usize>,
    pub column_number: Option<usize>,
    pub context: Option<String>,
}

/// Validate multiple function calls at once
pub fn validate_multiple_calls(
    requests: &[CallValidationRequest],
) -> Vec<CallValidationResult> {
    requests
        .iter()
        .map(|request| match request.call_type {
            CallType::Static => validate_static_call(
                request.object_name.as_deref().unwrap_or(""),
                &request.method_name,
                request.argument_count,
                request.line_number,
                request.column_number,
            ),
            CallType::Instance => validate_instance_call(
                request.instance_type.unwrap_or(DixType::Null),
                &request.method_name,
                request.argument_count,
                request.line_number,
                request.column_number,
            ),
        })
        .collect()
}

// ==================== DISCOVERY AND INTROSPECTION ====================

/// Completion information for IDE support
#[derive(Debug, Clone)]
pub struct CompletionInfo {
    pub objects: Vec<String>,
    pub methods: HashMap<String, Vec<String>>,
}

/// Get all available static objects and methods
pub fn get_static_completions(object_prefix: Option<&str>) -> CompletionInfo {
    let mut objects = Vec::new();
    let mut methods = HashMap::new();

    for object_name in static_object_registry::get_object_names() {
        if let Some(prefix) = object_prefix {
            if !object_name.starts_with(prefix) {
                continue;
            }
        }

        objects.push(object_name.clone());
        methods.insert(
            object_name.clone(),
            static_object_registry::get_method_names(&object_name),
        );
    }

    CompletionInfo { objects, methods }
}

/// Get all available instance methods for a type
pub fn get_instance_completions(
    dix_type: DixType,
    method_prefix: Option<&str>,
) -> Vec<String> {
    let mut methods = instance_method_registry::get_instance_methods(dix_type);

    if let Some(prefix) = method_prefix {
        methods.retain(|m| m.starts_with(prefix));
    }

    methods
}

/// Method signature information for IDE support
#[derive(Debug, Clone)]
pub struct MethodSignatureInfo {
    pub full_name: String,
    pub parameter_count: i32,
    pub return_type: DixType,
    pub description: String,
    pub parameters: Vec<ParameterInfo>,
}

/// Parameter information
#[derive(Debug, Clone)]
pub struct ParameterInfo {
    pub name: String,
    pub param_type: DixType,
    pub is_optional: bool,
    pub default_value: Option<String>,
    pub description: String,
}

/// Get method signature information for IDE support
pub fn get_method_signature(object_name: &str, method_name: &str) -> Option<MethodSignatureInfo> {
    let method = static_object_registry::get_method(object_name, method_name)?;

    Some(MethodSignatureInfo {
        full_name: format!("{}.{}", object_name, method_name),
        parameter_count: method.parameter_count(),
        return_type: method.return_type(),
        description: method.description().to_string(),
        parameters: generate_parameter_info(method, false),
    })
}

/// Get instance method signature information
pub fn get_instance_method_signature(
    dix_type: DixType,
    method_name: &str,
) -> Option<MethodSignatureInfo> {
    let method = instance_method_registry::get_instance_method(dix_type, method_name)?;

    let param_count = method.parameter_count().saturating_sub(1).max(0);

    Some(MethodSignatureInfo {
        full_name: format!("{:?}.{}", dix_type, method_name),
        parameter_count: param_count,
        return_type: method.return_type(),
        description: method.description().to_string(),
        parameters: generate_parameter_info(method, true),
    })
}

// ==================== VALIDATION REPORT ====================

/// Validation summary statistics
#[derive(Debug, Clone)]
pub struct ValidationSummary {
    pub total_calls: usize,
    pub valid_calls: usize,
    pub error_count: usize,
    pub warning_count: usize,
}

/// Comprehensive validation report
#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub summary: ValidationSummary,
    pub all_results: Vec<CallValidationResult>,
    pub errors: Vec<CallValidationResult>,
    pub warnings: Vec<String>,
}

/// Generate comprehensive validation report
pub fn generate_validation_report(requests: &[CallValidationRequest]) -> ValidationReport {
    let results = validate_multiple_calls(requests);
    let errors: Vec<CallValidationResult> = results.iter().filter(|r| !r.is_valid).cloned().collect();
    let warnings = Vec::new(); // Could add warnings for deprecated methods, etc.

    let summary = ValidationSummary {
        total_calls: requests.len(),
        valid_calls: results.iter().filter(|r| r.is_valid).count(),
        error_count: errors.len(),
        warning_count: warnings.len(),
    };

    ValidationReport {
        summary,
        all_results: results,
        errors,
        warnings,
    }
}

// ==================== HELPER METHODS ====================

use std::collections::HashMap;

/// Create dummy value of specified type for validation
fn create_dummy_value(dix_type: DixType) -> Result<DixValue, String> {
    Ok(match dix_type {
        DixType::String => DixValue::from_string(String::new()),
        DixType::Int => DixValue::from_int(0),
        DixType::Float => DixValue::from_float(0.0),
        DixType::Double => DixValue::from_double(0.0),
        DixType::Bool => DixValue::from_bool(false),
        DixType::Array => DixValue::from_array(Vec::new()),
        DixType::Object => DixValue::from_object(HashMap::new()),
        DixType::Date => DixValue::from_date(chrono::Utc::now()),
        DixType::Timestamp => DixValue::from_timestamp(chrono::Utc::now()),
        DixType::Null => DixValue::null(),
        DixType::Tuple => DixValue::from_tuple(Vec::new()),
        DixType::Blob => DixValue::from_blob(String::new()).map_err(|e| e.to_string())?,
        DixType::Regex => DixValue::from_regex(".*".to_string()).map_err(|e| e.to_string())?,
        DixType::Hex => DixValue::from_hex("#000000".to_string()),
        _ => DixValue::null(),
    })
}

/// Generate parameter information for method signature
fn generate_parameter_info(method: &dyn IBuiltinMethod, skip_first: bool) -> Vec<ParameterInfo> {
    let mut parameters = Vec::new();
    let start_index = if skip_first { 1 } else { 0 };
    let param_count = method.parameter_count();

    if param_count < 0 {
        // Variadic
        return vec![ParameterInfo {
            name: "...args".to_string(),
            param_type: DixType::Any,
            is_optional: false,
            default_value: None,
            description: "Variable number of arguments".to_string(),
        }];
    }

    for i in start_index..param_count {
        parameters.push(ParameterInfo {
            name: format!("arg{}", i - start_index + 1),
            param_type: DixType::Any, // Would need more sophisticated signature info
            is_optional: false,
            default_value: None,
            description: String::new(),
        });
    }

    parameters
}

/// Create error validation result with location info
fn create_error(
    message: &str,
    line: Option<usize>,
    column: Option<usize>,
) -> CallValidationResult {
    let full_message = if let (Some(l), Some(c)) = (line, column) {
        format!("{} at line {}, column {}", message, l, c)
    } else {
        message.to_string()
    };

    CallValidationResult::error(full_message, CallType::Static)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_static_call() {
        initialize();

        let result = validate_static_call("Math", "max", 2, None, None);
        assert!(result.is_valid);

        let result = validate_static_call("Math", "max", 3, None, None);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_validate_instance_call() {
        initialize();

        let result = validate_instance_call(DixType::String, "toUpper", 0, None, None);
        assert!(result.is_valid);

        let result = validate_instance_call(DixType::String, "nonexistent", 0, None, None);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_get_static_completions() {
        initialize();

        let completions = get_static_completions(None);
        assert!(!completions.objects.is_empty());
        assert!(completions.objects.contains(&"Math".to_string()));
    }
}
