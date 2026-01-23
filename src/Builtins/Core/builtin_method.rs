// src/Builtins/Core/builtin_method.rs
//! Builtin method trait and implementation for DixScript
//! Provides structure for all instance and static methods

use super::dix_type::DixType;
use super::dix_value::DixValue;

/// Trait for all built-in methods in DixScript
pub trait IBuiltinMethod {
    fn name(&self) -> &str;
    fn parameter_count(&self) -> i32;
    fn min_parameter_count(&self) -> i32;
    fn return_type(&self) -> DixType;
    fn description(&self) -> &str;

    /// Validates arguments before execution
    fn validate_arguments(&self, args: &[DixValue]) -> bool;

    /// Executes the method with given arguments
    fn call(&self, args: &[DixValue]) -> Result<DixValue, String>;
}

/// Function signature for builtin method implementation
pub type BuiltinMethodImpl = fn(&[DixValue]) -> Result<DixValue, String>;

/// Function signature for custom validation
pub type BuiltinMethodValidator = fn(&[DixValue]) -> bool;

/// Standard implementation of IBuiltinMethod
pub struct BuiltinMethod {
    name: String,
    parameter_count: i32,
    min_parameter_count: i32,
    return_type: DixType,
    description: String,
    implementation: BuiltinMethodImpl,
    validator: Option<BuiltinMethodValidator>,
}

impl BuiltinMethod {
    /// Create a new builtin method
    pub fn new(
        name: String,
        parameter_count: i32,
        return_type: DixType,
        implementation: BuiltinMethodImpl,
        description: String,
    ) -> Self {
        let min_param_count = if parameter_count >= 0 {
            parameter_count
        } else {
            0
        };

        BuiltinMethod {
            name,
            parameter_count,
            min_parameter_count: min_param_count,
            return_type,
            description,
            implementation,
            validator: None,
        }
    }

    /// Create a new builtin method with custom validator
    pub fn new_with_validator(
        name: String,
        parameter_count: i32,
        return_type: DixType,
        implementation: BuiltinMethodImpl,
        description: String,
        validator: BuiltinMethodValidator,
    ) -> Self {
        let min_param_count = if parameter_count >= 0 {
            parameter_count
        } else {
            0
        };

        BuiltinMethod {
            name,
            parameter_count,
            min_parameter_count: min_param_count,
            return_type,
            description,
            implementation,
            validator: Some(validator),
        }
    }

    /// Create a new builtin method with variable arguments
    pub fn new_variadic(
        name: String,
        min_parameter_count: i32,
        return_type: DixType,
        implementation: BuiltinMethodImpl,
        description: String,
    ) -> Self {
        BuiltinMethod {
            name,
            parameter_count: -1, // -1 indicates variable arguments
            min_parameter_count,
            return_type,
            description,
            implementation,
            validator: None,
        }
    }
}

impl IBuiltinMethod for BuiltinMethod {
    fn name(&self) -> &str {
        &self.name
    }

    fn parameter_count(&self) -> i32 {
        self.parameter_count
    }

    fn min_parameter_count(&self) -> i32 {
        self.min_parameter_count
    }

    fn return_type(&self) -> DixType {
        self.return_type
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn validate_arguments(&self, args: &[DixValue]) -> bool {
        // Variable-length parameters (-1)
        if self.parameter_count == -1 {
            // Check minimum if specified
            if self.min_parameter_count > 0 && args.len() < self.min_parameter_count as usize {
                return false;
            }
        }
        // Fixed parameter count
        else if args.len() != self.parameter_count as usize {
            return false;
        }

        // Custom validation if provided
        if let Some(validator) = self.validator {
            return validator(args);
        }

        true
    }

    fn call(&self, args: &[DixValue]) -> Result<DixValue, String> {
        if !self.validate_arguments(args) {
            return Err(format!("Invalid arguments for method {}", self.name));
        }

        (self.implementation)(args)
    }
}

/// Exception type for builtin method errors
#[derive(Debug, Clone)]
pub struct BuiltinMethodException {
    pub message: String,
}

impl BuiltinMethodException {
    pub fn new(message: String) -> Self {
        BuiltinMethodException { message }
    }
}

impl std::fmt::Display for BuiltinMethodException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BuiltinMethodException: {}", self.message)
    }
}

impl std::error::Error for BuiltinMethodException {}

/// Validation helper functions
pub mod validation_helpers {
    use super::*;

    /// Validates that all arguments are numeric
    pub fn all_numeric(args: &[DixValue]) -> bool {
        args.iter().all(|arg| arg.is_numeric())
    }

    /// Validates that first argument is a string
    pub fn first_is_string(args: &[DixValue]) -> bool {
        !args.is_empty() && args[0].get_type() == DixType::String
    }

    /// Validates that first argument is an array
    pub fn first_is_array(args: &[DixValue]) -> bool {
        !args.is_empty() && args[0].get_type() == DixType::Array
    }

    /// Validates that first argument is an object
    pub fn first_is_object(args: &[DixValue]) -> bool {
        !args.is_empty() && args[0].get_type() == DixType::Object
    }

    /// Validates that argument at index has specific type
    pub fn argument_has_type(index: usize, expected_type: DixType, args: &[DixValue]) -> bool {
        args.len() > index && args[index].get_type() == expected_type
    }

    /// Validates that argument at index is not null
    pub fn argument_not_null(index: usize, args: &[DixValue]) -> bool {
        args.len() > index && !args[index].is_null()
    }

    /// Validates array index is within bounds
    pub fn valid_array_index(array: &DixValue, index: &DixValue) -> bool {
        if array.get_type() != DixType::Array || !index.is_numeric() {
            return false;
        }

        let array_list = array.as_array();
        let idx = index.as_int();
        idx >= 0 && (idx as usize) < array_list.len()
    }

    /// Validates string index is within bounds
    pub fn valid_string_index(string: &DixValue, index: &DixValue) -> bool {
        if string.get_type() != DixType::String || !index.is_numeric() {
            return false;
        }

        let text = string.as_string();
        let idx = index.as_int();
        idx >= 0 && (idx as usize) < text.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_impl(_args: &[DixValue]) -> Result<DixValue, String> {
        Ok(DixValue::from_int(42))
    }

    fn test_validator(args: &[DixValue]) -> bool {
        args.len() == 2 && args[0].is_numeric()
    }

    #[test]
    fn test_builtin_method_creation() {
        let method = BuiltinMethod::new(
            "test".to_string(),
            1,
            DixType::Int,
            test_impl,
            "A test method".to_string(),
        );

        assert_eq!(method.name(), "test");
        assert_eq!(method.parameter_count(), 1);
        assert_eq!(method.return_type(), DixType::Int);
    }

    #[test]
    fn test_validation() {
        let method = BuiltinMethod::new(
            "test".to_string(),
            1,
            DixType::Int,
            test_impl,
            "A test method".to_string(),
        );

        let args = vec![DixValue::from_int(10)];
        assert!(method.validate_arguments(&args));

        let wrong_args = vec![DixValue::from_int(10), DixValue::from_int(20)];
        assert!(!method.validate_arguments(&wrong_args));
    }

    #[test]
    fn test_custom_validator() {
        let method = BuiltinMethod::new_with_validator(
            "test".to_string(),
            2,
            DixType::Int,
            test_impl,
            "A test method".to_string(),
            test_validator,
        );

        let valid_args = vec![DixValue::from_int(10), DixValue::from_string("test".to_string())];
        assert!(method.validate_arguments(&valid_args));

        let invalid_args = vec![DixValue::from_string("test".to_string()), DixValue::from_int(10)];
        assert!(!method.validate_arguments(&invalid_args));
    }

    #[test]
    fn test_variadic_method() {
        let method = BuiltinMethod::new_variadic(
            "test".to_string(),
            2,
            DixType::Int,
            test_impl,
            "A variadic test method".to_string(),
        );

        assert_eq!(method.parameter_count(), -1);
        assert_eq!(method.min_parameter_count(), 2);

        let too_few = vec![DixValue::from_int(10)];
        assert!(!method.validate_arguments(&too_few));

        let enough = vec![DixValue::from_int(10), DixValue::from_int(20)];
        assert!(method.validate_arguments(&enough));

        let more = vec![DixValue::from_int(10), DixValue::from_int(20), DixValue::from_int(30)];
        assert!(method.validate_arguments(&more));
    }
}