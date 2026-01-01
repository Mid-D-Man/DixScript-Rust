//! Value Resolution error types and handling

use super::error_enums::ErrorSeverity;
use crate::DixCore::List;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueResolutionErrorType {
    FunctionNotFound,
    InvalidFunctionScope,
    ParameterCountMismatch,
    ParameterTypeMismatch,
    UndefinedVariable,
    InvalidOperation,
    DivisionByZero,
    NullReferenceError,
    TypeConversionError,
    RecursionLimitExceeded,
    ExecutionTimeout,
    InvalidReturnType,
    BuiltinCallFailed,
    ArrayIndexOutOfBounds,
    ObjectPropertyNotFound,
    InvalidEnumAccess,
}

#[derive(Debug, Clone)]
pub struct ValueResolutionError {
    pub error_id: String,
    pub error_type: ValueResolutionErrorType,
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub suggestion: Option<String>,
    pub function_name: Option<String>,
    pub variable_name: Option<String>,
    pub location: Option<String>,
    pub severity: ErrorSeverity,
    pub quick_fixes: List<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl ValueResolutionError {
    pub fn new(
        error_type: ValueResolutionErrorType,
        message: String,
        line: usize,
        column: usize,
        suggestion: Option<String>,
        function_name: Option<String>,
        variable_name: Option<String>,
        location: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXVR{:03}L{}C{}", error_type as u32, line, column);

        let mut error = Self {
            error_id,
            error_type: error_type.clone(),
            message,
            line,
            column,
            suggestion,
            function_name,
            variable_name,
            location,
            severity,
            quick_fixes: List::New(),
            metadata: std::collections::HashMap::new(),
        };

        error.generate_quick_fixes(&error_type);
        error
    }

    pub fn generate_suggestion(error_type: &ValueResolutionErrorType, context: &str) -> String {
        match error_type {
            ValueResolutionErrorType::FunctionNotFound => {
                format!("Function '{}' not found. Check @QUICKFUNCS section.", context)
            }
            ValueResolutionErrorType::InvalidFunctionScope => {
                format!("Function '{}' not accessible from current scope.", context)
            }
            ValueResolutionErrorType::ParameterCountMismatch => {
                format!("Function '{}' called with wrong number of parameters.", context)
            }
            ValueResolutionErrorType::ParameterTypeMismatch => {
                format!("Parameter type mismatch in function '{}'.", context)
            }
            ValueResolutionErrorType::UndefinedVariable => {
                format!("Variable '{}' is not defined in current scope.", context)
            }
            ValueResolutionErrorType::InvalidOperation => {
                format!("Invalid operation: {}", context)
            }
            ValueResolutionErrorType::DivisionByZero => {
                "Division by zero is not allowed.".to_string()
            }
            ValueResolutionErrorType::NullReferenceError => {
                format!("Null reference error: {}", context)
            }
            ValueResolutionErrorType::TypeConversionError => {
                format!("Cannot convert type in: {}", context)
            }
            ValueResolutionErrorType::RecursionLimitExceeded => {
                format!("Recursion limit exceeded in function '{}'.", context)
            }
            ValueResolutionErrorType::ExecutionTimeout => {
                format!("Function '{}' execution timeout.", context)
            }
            ValueResolutionErrorType::InvalidReturnType => {
                format!("Return type mismatch in function '{}'.", context)
            }
            ValueResolutionErrorType::BuiltinCallFailed => {
                format!("Built-in method call failed: {}", context)
            }
            ValueResolutionErrorType::ArrayIndexOutOfBounds => {
                "Array index is out of bounds.".to_string()
            }
            ValueResolutionErrorType::ObjectPropertyNotFound => {
                format!("Property '{}' not found on object.", context)
            }
            ValueResolutionErrorType::InvalidEnumAccess => {
                format!("Invalid enum access: {}", context)
            }
        }
    }

    fn generate_quick_fixes(&mut self, error_type: &ValueResolutionErrorType) {
        match error_type {
            ValueResolutionErrorType::FunctionNotFound => {
                self.quick_fixes.Add("Check function name spelling".to_string());
                self.quick_fixes.Add("Verify function is defined in @QUICKFUNCS".to_string());
                self.quick_fixes.Add("Check if function is in correct scope".to_string());
            }
            ValueResolutionErrorType::InvalidFunctionScope => {
                self.quick_fixes.Add("Check function scope declaration".to_string());
                self.quick_fixes.Add("Verify call location matches function scope".to_string());
                self.quick_fixes.Add("Consider making function global".to_string());
            }
            ValueResolutionErrorType::ParameterCountMismatch => {
                self.quick_fixes.Add("Check function parameter count".to_string());
                self.quick_fixes.Add("Add missing parameters".to_string());
                self.quick_fixes.Add("Remove extra parameters".to_string());
            }
            ValueResolutionErrorType::UndefinedVariable => {
                self.quick_fixes.Add("Check variable name spelling".to_string());
                self.quick_fixes.Add("Verify variable is defined before use".to_string());
                self.quick_fixes.Add("Check variable scope".to_string());
            }
            ValueResolutionErrorType::DivisionByZero => {
                self.quick_fixes.Add("Add check for zero before division".to_string());
                self.quick_fixes.Add("Use conditional expression to handle zero case".to_string());
            }
            ValueResolutionErrorType::TypeConversionError => {
                self.quick_fixes.Add("Check value type compatibility".to_string());
                self.quick_fixes.Add("Add explicit type conversion".to_string());
                self.quick_fixes.Add("Verify operation is valid for these types".to_string());
            }
            ValueResolutionErrorType::RecursionLimitExceeded => {
                self.quick_fixes.Add("Check for infinite recursion".to_string());
                self.quick_fixes.Add("Add base case to recursive function".to_string());
                self.quick_fixes.Add("Consider iterative approach".to_string());
            }
            ValueResolutionErrorType::BuiltinCallFailed => {
                self.quick_fixes.Add("Check built-in method signature".to_string());
                self.quick_fixes.Add("Verify parameter types".to_string());
                self.quick_fixes.Add("Check built-in documentation".to_string());
            }
            ValueResolutionErrorType::ArrayIndexOutOfBounds => {
                self.quick_fixes.Add("Check array bounds before access".to_string());
                self.quick_fixes.Add("Verify index is within valid range".to_string());
                self.quick_fixes.Add("Use array.length() to check size".to_string());
            }
            ValueResolutionErrorType::ObjectPropertyNotFound => {
                self.quick_fixes.Add("Check property name spelling".to_string());
                self.quick_fixes.Add("Verify property exists on object".to_string());
                self.quick_fixes.Add("Use conditional access operator".to_string());
            }
            ValueResolutionErrorType::InvalidEnumAccess => {
                self.quick_fixes.Add("Check enum name spelling".to_string());
                self.quick_fixes.Add("Verify enum value exists".to_string());
                self.quick_fixes.Add("Check @ENUMS section".to_string());
            }
            _ => {}
        }
    }
}

impl fmt::Display for ValueResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "[{}] {}: {:?}",
            self.error_id, self.severity, self.error_type
        )?;

        if let Some(ref func) = self.function_name {
            writeln!(f, "Function: {}", func)?;
        }

        if let Some(ref loc) = self.location {
            writeln!(f, "Location: {}", loc)?;
        }

        if let Some(ref var) = self.variable_name {
            writeln!(f, "Variable: {}", var)?;
        }

        writeln!(f, "Line {}, Column {}", self.line, self.column)?;
        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref suggestion) = self.suggestion {
            writeln!(f, "Suggestion: {}", suggestion)?;
        }

        if !self.quick_fixes.IsEmpty() {
            writeln!(f, "Quick Fixes:")?;
            for fix in self.quick_fixes.Iter() {
                writeln!(f, "  - {}", fix)?;
            }
        }

        Ok(())
    }
}