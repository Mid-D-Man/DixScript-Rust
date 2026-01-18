//! Value resolution errors (compile-time function execution)

use super::ErrorSeverity;
use crate::DixCore::List;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
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
    pub metadata: HashMap<String, String>,
}

impl ValueResolutionError {
    #[allow(clippy::too_many_arguments)]
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
        let error_id = format!("DXVR{:?}L{}C{}", error_type, line, column);
        let quick_fixes = Self::generate_quick_fixes(error_type);

        Self {
            error_id,
            error_type,
            message,
            line,
            column,
            suggestion,
            function_name,
            variable_name,
            location,
            severity,
            quick_fixes,
            metadata: HashMap::new(),
        }
    }

    #[inline]
    fn generate_quick_fixes(error_type: ValueResolutionErrorType) -> List<String> {
        let mut fixes = List::New();

        match error_type {
            ValueResolutionErrorType::FunctionNotFound => {
                fixes.Add("Check function name spelling".to_string());
                fixes.Add("Ensure function is defined in @QUICKFUNCS".to_string());
            }
            ValueResolutionErrorType::ParameterCountMismatch => {
                fixes.Add("Adjust the number of arguments".to_string());
            }
            ValueResolutionErrorType::DivisionByZero => {
                fixes.Add("Add a check to prevent division by zero".to_string());
            }
            ValueResolutionErrorType::RecursionLimitExceeded => {
                fixes.Add("Reduce recursion depth or use iteration".to_string());
            }
            _ => {}
        }

        fixes
    }
}

impl fmt::Display for ValueResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} at Line {}, Column {}: {}",
            self.severity, self.error_id, self.line, self.column, self.message
        )?;

        if let Some(ref func) = self.function_name {
            write!(f, "\n📍 Function: {}", func)?;
        }

        if let Some(ref var) = self.variable_name {
            write!(f, "\n📍 Variable: {}", var)?;
        }

        if let Some(ref loc) = self.location {
            write!(f, "\n📍 Location: {}", loc)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n💡 Suggestion: {}", suggestion)?;
        }

        if !self.quick_fixes.IsEmpty() {
            write!(f, "\n🔧 Quick Fixes:")?;
            for fix in self.quick_fixes.Iter() {
                write!(f, "\n  - {}", fix)?;
            }
        }

        Ok(())
    }
}