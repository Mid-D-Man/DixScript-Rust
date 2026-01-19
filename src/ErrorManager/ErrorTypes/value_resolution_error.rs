use std::fmt;

/// Value resolution error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueResolutionErrorType {
    UndefinedVariable,
    UndefinedFunction,
    InvalidFunctionCall,
    TypeMismatch,
    InvalidArgument,
    CircularReference,
    DivisionByZero,
    NullReference,
    IndexOutOfBounds,
    InvalidOperation,
    UnsupportedOperation,
    InvalidConversion,
    ParameterCountMismatch,
    MissingRequiredParameter,
    InvalidEnumValue,
    InvalidRegexPattern,
    StaticMethodNotFound,
    InstanceMethodNotFound,
    PropertyNotFound,
    InvalidPropertyAccess,
}

/// Value resolution error with context
#[derive(Debug, Clone)]
pub struct ValueResolutionError {
    pub error_id: String,
    pub error_type: ValueResolutionErrorType,
    pub message: String,
    pub line: i32,
    pub column: i32,
    pub section_name: Option<String>,
    pub variable_name: Option<String>,
    pub function_name: Option<String>,
    pub suggestion: Option<String>,
    pub severity: super::ErrorSeverity,
    pub quick_fixes: Vec<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl ValueResolutionError {
    pub fn new(
        error_type: ValueResolutionErrorType,
        message: String,
        line: i32,
        column: i32,
        section_name: Option<String>,
        variable_name: Option<String>,
        function_name: Option<String>,
        suggestion: Option<String>,
        severity: super::ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXVAL{:03}L{}C{}", error_type as u32, line, column);
        let quick_fixes = Self::generate_quick_fixes(error_type);

        ValueResolutionError {
            error_id,
            error_type,
            message,
            line,
            column,
            section_name,
            variable_name,
            function_name,
            suggestion,
            severity,
            quick_fixes,
            metadata: std::collections::HashMap::new(),
        }
    }

    fn generate_quick_fixes(error_type: ValueResolutionErrorType) -> Vec<String> {
        match error_type {
            ValueResolutionErrorType::UndefinedVariable => vec![
                "Check variable name spelling".to_string(),
                "Ensure variable is defined in @VARS or @DEFAULTS".to_string(),
                "Check variable scope".to_string(),
            ],
            ValueResolutionErrorType::UndefinedFunction => vec![
                "Check function name spelling".to_string(),
                "Ensure function is defined in @FUNCTIONS".to_string(),
                "Check if function is imported".to_string(),
            ],
            ValueResolutionErrorType::InvalidFunctionCall => vec![
                "Check function signature".to_string(),
                "Verify argument types".to_string(),
                "Check parameter count".to_string(),
            ],
            ValueResolutionErrorType::TypeMismatch => vec![
                "Check expected type".to_string(),
                "Add type conversion".to_string(),
                "Verify value type".to_string(),
            ],
            ValueResolutionErrorType::CircularReference => vec![
                "Break circular dependency".to_string(),
                "Restructure variable references".to_string(),
            ],
            ValueResolutionErrorType::DivisionByZero => vec![
                "Check divisor value".to_string(),
                "Add zero check before division".to_string(),
            ],
            ValueResolutionErrorType::InvalidEnumValue => vec![
                "Check enum definition".to_string(),
                "Use valid enum member".to_string(),
            ],
            ValueResolutionErrorType::ParameterCountMismatch => vec![
                "Check function signature".to_string(),
                "Add missing arguments".to_string(),
                "Remove extra arguments".to_string(),
            ],
            ValueResolutionErrorType::MissingRequiredParameter => vec![
                "Provide required parameter".to_string(),
                "Check function definition".to_string(),
            ],
            _ => Vec::new(),
        }
    }

    /// Generate suggestion for value resolution error
    pub fn generate_suggestion(
        error_type: ValueResolutionErrorType,
        context: &str,
        additional_info: Option<&str>,
    ) -> String {
        match error_type {
            ValueResolutionErrorType::UndefinedVariable => {
                format!("Variable '{}' is not defined. Check @VARS or @DEFAULTS section.", context)
            }
            ValueResolutionErrorType::UndefinedFunction => {
                format!("Function '{}' is not defined. Check @FUNCTIONS section or imports.", context)
            }
            ValueResolutionErrorType::InvalidFunctionCall => {
                format!("Invalid call to function '{}'. Check parameter types and count.", context)
            }
            ValueResolutionErrorType::TypeMismatch => {
                if let Some(expected) = additional_info {
                    format!("Type mismatch: expected '{}', got '{}'.", expected, context)
                } else {
                    format!("Type mismatch for '{}'.", context)
                }
            }
            ValueResolutionErrorType::CircularReference => {
                format!("Circular reference detected involving '{}'.", context)
            }
            ValueResolutionErrorType::DivisionByZero => {
                "Division by zero is not allowed.".to_string()
            }
            ValueResolutionErrorType::InvalidEnumValue => {
                format!("'{}' is not a valid enum value. Check @ENUMS section.", context)
            }
            ValueResolutionErrorType::ParameterCountMismatch => {
                format!("Parameter count mismatch for function '{}'. {}",
                        context, additional_info.unwrap_or(""))
            }
            ValueResolutionErrorType::MissingRequiredParameter => {
                format!("Missing required parameter '{}' in function call.", context)
            }
            ValueResolutionErrorType::StaticMethodNotFound => {
                format!("Static method '{}' not found.", context)
            }
            ValueResolutionErrorType::InstanceMethodNotFound => {
                format!("Instance method '{}' not found.", context)
            }
            _ => format!("Value resolution error: {}", context),
        }
    }
}

impl fmt::Display for ValueResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[{}] {:?}: {:?}", self.error_id, self.severity, self.error_type)?;

        if let Some(ref section) = self.section_name {
            writeln!(f, "Section: {}", section)?;
        }

        if self.line > 0 || self.column > 0 {
            writeln!(f, "Location: Line {}, Column {}", self.line, self.column)?;
        }

        if let Some(ref var) = self.variable_name {
            writeln!(f, "Variable: {}", var)?;
        }

        if let Some(ref func) = self.function_name {
            writeln!(f, "Function: {}", func)?;
        }

        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref suggestion) = self.suggestion {
            writeln!(f, "Suggestion: {}", suggestion)?;
        }

        if !self.quick_fixes.is_empty() {
            writeln!(f, "Quick Fixes:")?;
            for fix in &self.quick_fixes {
                writeln!(f, "  - {}", fix)?;
            }
        }

        Ok(())
    }
}