
//! Type System Manager v1.0.0 - Pure functions for type operations
//!
//! All methods are stateless utility functions.

use crate::Compiler::AST::{
    DataType, QuickFunction, QuickFuncParam, Expression, Value, ObjectProperty, Position
};
use crate::Builtins::Core::DixType;
use crate::ErrorManager::ErrorManager;
use crate::Compiler::Core::Tokenizer::TokenType;

/// Unified type system manager - all static methods
pub struct TypeSystemManager;

impl TypeSystemManager {
    /// Get default value for a data type
    pub fn get_default_value_for_type(data_type: DataType) -> Value {
        let pos = Position::UNKNOWN;

        match data_type {
            // Primitives
            DataType::Int => Value::Integer { value: 0, position: pos },
            DataType::Float => Value::Float { value: 0.0, position: pos },
            DataType::Double => Value::Double { value: 0.0, position: pos },
            DataType::String => Value::String { value: String::new(), position: pos },
            DataType::Bool => Value::Boolean { value: false, position: pos },

            // Collections
            DataType::Array => Value::Array { values: Vec::new(), position: pos },
            DataType::Object => Value::Object { properties: Vec::new(), position: pos },
            DataType::Tuple => Value::PrefixedConstructor {
                prefix: "t".to_string(),
                arguments: Vec::new(),
                position: pos,
            },

            // Special types
            DataType::Hex => Value::HexColor { value: "#000000".to_string(), position: pos },
            DataType::Date => {
                let date_str = chrono::Utc::now().format("%Y-%m-%d").to_string();
                Value::Date { value: date_str, position: pos }
            }
            DataType::Timestamp => {
                let ts_str = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
                Value::Timestamp { value: ts_str, position: pos }
            }
            DataType::Blob => Value::PrefixedConstructor {
                prefix: "b".to_string(),
                arguments: vec![Value::String { value: String::new(), position: pos }],
                position: pos,
            },
            DataType::Regex => Value::PrefixedConstructor {
                prefix: "r".to_string(),
                arguments: vec![Value::String { value: ".*".to_string(), position: pos }],
                position: pos,
            },

            // No defaults for these
            DataType::Enum | DataType::Any | DataType::Function | DataType::Range => {
                Value::Null { position: pos }
            }
        }
    }

    /// Get default value from string type name
    pub fn get_default_value_for_string(type_name: &str) -> Value {
        if type_name.trim().is_empty() {
            return Value::Null { position: Position::UNKNOWN };
        }

        let data_type = match type_name.to_lowercase().as_str() {
            "int" => DataType::Int,
            "float" => DataType::Float,
            "double" => DataType::Double,
            "string" => DataType::String,
            "bool" => DataType::Bool,
            "array" => DataType::Array,
            "object" => DataType::Object,
            "tuple" => DataType::Tuple,
            "hex" => DataType::Hex,
            "blob" => DataType::Blob,
            "regex" => DataType::Regex,
            "date" => DataType::Date,
            "timestamp" => DataType::Timestamp,
            "enum" => DataType::Enum,
            _ => DataType::Any,
        };

        Self::get_default_value_for_type(data_type)
    }

    /// Get data type from token type
    pub fn get_data_type_from_token(token_type: &TokenType) -> Option<DataType> {
        match token_type {
            TokenType::Keyword(k) => match k.to_lowercase().as_str() {
                "int" => Some(DataType::Int),
                "float" => Some(DataType::Float),
                "double" => Some(DataType::Double),
                "string" => Some(DataType::String),
                "bool" => Some(DataType::Bool),
                "array" => Some(DataType::Array),
                "tuple" => Some(DataType::Tuple),
                "object" => Some(DataType::Object),
                "hex" => Some(DataType::Hex),
                "blob" => Some(DataType::Blob),
                "regex" => Some(DataType::Regex),
                "date" => Some(DataType::Date),
                "timestamp" => Some(DataType::Timestamp),
                "enum" => Some(DataType::Enum),
                _ => None,
            },
            _ => None,
        }
    }

    /// Check if token is a literal value
    pub fn is_literal_token(token_type: &TokenType) -> bool {
        matches!(
            token_type,
            TokenType::Integer(_) | TokenType::Float(_) | TokenType::Double(_) |
            TokenType::String(_) | TokenType::StringSingle(_) | TokenType::Bool(_) |
            TokenType::HexColor(_) | TokenType::Date(_) | TokenType::Timestamp(_) |
            TokenType::HexLiteral(_)
        )
    }

    /// Check if AST Value type needs special conversion
    pub fn is_special_type(value: &Value) -> bool {
        matches!(
            value,
            Value::PrefixedConstructor { .. } |
            Value::EnumValue { .. } |
            Value::Object { .. } |
            Value::Array { .. }
        )
    }

    /// Check if value can be converted to target type
    pub fn can_convert(from: DataType, to: DataType) -> bool {
        // Same type always okay
        if from == to {
            return true;
        }

        // Any converts to anything (like null in C#)
        if from == DataType::Any {
            return true;
        }

        // Numeric conversions
        if Self::is_numeric_type(from) && Self::is_numeric_type(to) {
            return true;
        }

        // Everything converts to string
        if to == DataType::String {
            return true;
        }

        // Date/Timestamp interchangeable
        if (from == DataType::Date && to == DataType::Timestamp) ||
            (from == DataType::Timestamp && to == DataType::Date)
        {
            return true;
        }

        false
    }

    /// Check if type is numeric
    pub fn is_numeric_type(data_type: DataType) -> bool {
        matches!(data_type, DataType::Int | DataType::Float | DataType::Double)
    }

    /// Apply default values to QuickFunction parameters (returns new vector)
    pub fn apply_defaults_to_parameters(parameters: Vec<QuickFuncParam>) -> Vec<QuickFuncParam> {
        if parameters.is_empty() {
            return parameters;
        }

        let error_manager = ErrorManager::get_shared_instance();
        let mut result = Vec::with_capacity(parameters.len());

        for param in parameters {
            // Only generate default if: no default value AND has type annotation
            if param.default_value.is_none() && param.data_type.is_some() {
                let data_type = param.data_type.unwrap();
                let default_value = Self::get_default_value_for_type(data_type);
                let default_expression = Expression::Value {
                    value: default_value,
                    position: param.position,
                };

                error_manager.log_debug(&format!(
                    "Applied default value for parameter '{}' of type {:?}",
                    param.name, data_type
                ));

                result.push(QuickFuncParam {
                    name: param.name,
                    data_type: param.data_type,
                    default_value: Some(default_expression),
                    position: param.position,
                });
            } else {
                result.push(param);
            }
        }

        result
    }

    /// Validate QuickFunction signature
    pub fn validate_quick_function_signature(function: &QuickFunction) -> Result<bool, Vec<String>> {
        let mut errors = Vec::new();

        // Validate return type if specified
        if let Some(return_type) = function.return_type {
            if !Self::is_valid_data_type(return_type) {
                errors.push(format!("Invalid return type: {:?}", return_type));
            }
        }

        // Validate parameters
        for param in &function.parameters {
            if let Some(param_type) = param.data_type {
                if !Self::is_valid_data_type(param_type) {
                    errors.push(format!("Parameter '{}' has invalid type: {:?}", param.name, param_type));
                }
            }

            // If has default value, check it matches type
            if let (Some(ref default_expr), Some(param_type)) = (&param.default_value, param.data_type) {
                let value_type = Self::infer_expression_type(default_expr);
                if !Self::can_convert(value_type, param_type) {
                    errors.push(format!(
                        "Parameter '{}' default value type {:?} cannot convert to declared type {:?}",
                        param.name, value_type, param_type
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(true)
        } else {
            Err(errors)
        }
    }

    /// Validate individual parameter
    pub fn validate_parameter(param: &QuickFuncParam) -> Result<bool, String> {
        if param.name.trim().is_empty() {
            return Err("Parameter name cannot be empty".to_string());
        }

        if let Some(param_type) = param.data_type {
            if !Self::is_valid_data_type(param_type) {
                return Err(format!("Invalid parameter type: {:?}", param_type));
            }
        }

        Ok(true)
    }

    /// Check if DataType is valid
    fn is_valid_data_type(data_type: DataType) -> bool {
        // All enum variants are valid in Rust
        true
    }

    /// Infer type from Expression
    fn infer_expression_type(expr: &Expression) -> DataType {
        match expr {
            Expression::Value { value, .. } => Self::infer_value_type(value),
            _ => DataType::Any,
        }
    }

    /// Infer type from Value
    fn infer_value_type(value: &Value) -> DataType {
        match value {
            Value::Integer { .. } => DataType::Int,
            Value::Float { .. } => DataType::Float,
            Value::Double { .. } | Value::ScientificNotation { .. } => DataType::Double,
            Value::String { .. } | Value::InterpolatedString { .. } => DataType::String,
            Value::Boolean { .. } => DataType::Bool,
            Value::Array { .. } | Value::NestedArray { .. } => DataType::Array,
            Value::Object { .. } => DataType::Object,
            Value::Date { .. } => DataType::Date,
            Value::Timestamp { .. } => DataType::Timestamp,
            Value::HexColor { .. } => DataType::Hex,
            Value::PrefixedConstructor { prefix, .. } => match prefix.as_str() {
                "b" => DataType::Blob,
                "t" => DataType::Tuple,
                "r" => DataType::Regex,
                _ => DataType::Any,
            },
            Value::EnumValue { .. } => DataType::Enum,
            Value::Null { .. } => DataType::Any,
            _ => DataType::Any,
        }
    }

    /// Convert DixType to DataType
    pub fn dix_type_to_data_type(dix_type: DixType) -> DataType {
        match dix_type {
            DixType::Int => DataType::Int,
            DixType::Float => DataType::Float,
            DixType::Double => DataType::Double,
            DixType::String => DataType::String,
            DixType::Bool => DataType::Bool,
            DixType::Array => DataType::Array,
            DixType::Tuple => DataType::Tuple,
            DixType::Object => DataType::Object,
            DixType::Hex => DataType::Hex,
            DixType::Blob => DataType::Blob,
            DixType::Regex => DataType::Regex,
            DixType::Date => DataType::Date,
            DixType::Timestamp => DataType::Timestamp,
            DixType::Enum => DataType::Enum,
            DixType::Any => DataType::Any,
            DixType::Null | DixType::Void => DataType::Any,
        }
    }

    /// Convert DataType to DixType
    pub fn data_type_to_dix_type(data_type: DataType) -> DixType {
        match data_type {
            DataType::Int => DixType::Int,
            DataType::Float => DixType::Float,
            DataType::Double => DixType::Double,
            DataType::String => DixType::String,
            DataType::Bool => DixType::Bool,
            DataType::Array => DixType::Array,
            DataType::Tuple => DixType::Tuple,
            DataType::Object => DixType::Object,
            DataType::Hex => DixType::Hex,
            DataType::Blob => DixType::Blob,
            DataType::Regex => DixType::Regex,
            DataType::Date => DixType::Date,
            DataType::Timestamp => DixType::Timestamp,
            DataType::Enum => DixType::Enum,
            DataType::Any | DataType::Function | DataType::Range => DixType::Null,
        }
    }

    /// Get supported types for current version
    pub fn get_supported_types() -> Vec<&'static str> {
        vec![
            "int", "float", "double", "string", "bool", "array", "tuple",
            "hex", "blob", "regex", "object", "timestamp", "date", "enum"
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_default_values() {
        let int_val = TypeSystemManager::get_default_value_for_type(DataType::Int);
        assert!(matches!(int_val, Value::Integer { value: 0, .. }));

        let str_val = TypeSystemManager::get_default_value_for_type(DataType::String);
        assert!(matches!(str_val, Value::String { .. }));
    }

    #[test]
    fn test_can_convert() {
        assert!(TypeSystemManager::can_convert(DataType::Int, DataType::Int));
        assert!(TypeSystemManager::can_convert(DataType::Int, DataType::Float));
        assert!(TypeSystemManager::can_convert(DataType::Int, DataType::String));
        assert!(TypeSystemManager::can_convert(DataType::Date, DataType::Timestamp));
    }

    #[test]
    fn test_is_numeric_type() {
        assert!(TypeSystemManager::is_numeric_type(DataType::Int));
        assert!(TypeSystemManager::is_numeric_type(DataType::Float));
        assert!(TypeSystemManager::is_numeric_type(DataType::Double));
        assert!(!TypeSystemManager::is_numeric_type(DataType::String));
    }

    #[test]
    fn test_type_conversions() {
        let dix_int = DixType::Int;
        let data_int = TypeSystemManager::dix_type_to_data_type(dix_int);
        assert_eq!(data_int, DataType::Int);

        let data_string = DataType::String;
        let dix_string = TypeSystemManager::data_type_to_dix_type(data_string);
        assert_eq!(dix_string, DixType::String);
    }

    #[test]
    fn test_get_data_type_from_token() {
        let token = TokenType::Keyword("int");
        let data_type = TypeSystemManager::get_data_type_from_token(&token);
        assert_eq!(data_type, Some(DataType::Int));

        let token2 = TokenType::Keyword("unknown");
        let data_type2 = TypeSystemManager::get_data_type_from_token(&token2);
        assert_eq!(data_type2, None);
    }
}