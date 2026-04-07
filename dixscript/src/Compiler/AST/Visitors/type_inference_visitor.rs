
use crate::Compiler::AST::*;
use crate::Compiler::Utilities::SymbolTable;
use crate::Builtins::Core::DixType;
use std::collections::HashMap;

/// Infers types from values and expressions
/// Used by DataSectionAnalyzer and QuickFuncsSectionAnalyzer
pub struct TypeInferenceVisitor<'a> {
    symbol_table: &'a SymbolTable,
    local_variable_types: HashMap<String, Option<DataType>>,
}

impl<'a> TypeInferenceVisitor<'a> {
    /// Create TypeInferenceVisitor with optional local variable type information
    pub fn new(
        symbol_table: &'a SymbolTable,
        local_variable_types: Option<HashMap<String, Option<DataType>>>,
    ) -> Self {
        TypeInferenceVisitor {
            symbol_table,
            local_variable_types: local_variable_types.unwrap_or_default(),
        }
    }

    /// Infer type from a Value node
    pub fn infer_type_from_value(&self, value: &Value) -> Option<DataType> {
        match value {
            Value::Integer { .. } => Some(DataType::Int),
            Value::Float { .. } => Some(DataType::Float),
            Value::Double { .. } => Some(DataType::Double),
            Value::ScientificNotation { .. } => Some(DataType::Double),
            Value::String { .. } => Some(DataType::String),
            Value::InterpolatedString { .. } => Some(DataType::String),
            Value::Boolean { .. } => Some(DataType::Bool),
            Value::HexColor { .. } => Some(DataType::Hex),
            Value::Date { .. } => Some(DataType::Date),
            Value::Timestamp { .. } => Some(DataType::Timestamp),
            Value::Null { .. } => None,
            Value::Array { .. } => Some(DataType::Array),
            Value::NestedArray { .. } => Some(DataType::Array),
            Value::Object { .. } => Some(DataType::Object),
            Value::PrefixedConstructor { prefix, .. } => self.infer_prefixed_constructor_type(prefix),
            Value::EnumValue { .. } => Some(DataType::Enum),
            Value::QuickFuncCall { function_name, .. } => {
                self.infer_function_call_type(function_name)
            }
            Value::Expression { expr, .. } => self.infer_type_from_expression(expr),
            Value::Lambda { .. } => Some(DataType::Function),
            Value::Range { .. } => Some(DataType::Range),
            _ => None,
        }
    }

    /// Infer type from an Expression node
    pub fn infer_type_from_expression(&self, expr: &Expression) -> Option<DataType> {
        match expr {
            Expression::Value { value, .. } => self.infer_type_from_value(value),
            
            Expression::Identifier { name, .. } => self.infer_identifier_type(name),
            
            Expression::QualifiedIdentifier { parts, arguments, .. } => {
                self.infer_qualified_identifier_type(parts, arguments.as_ref())
            }
            
            Expression::ArithmeticOp { left, right, .. } => {
                self.infer_arithmetic_op_type(left, right)
            }
            
            Expression::ComparisonOp { .. } => Some(DataType::Bool),
            Expression::LogicalOp { .. } => Some(DataType::Bool),
            Expression::BitwiseOp { .. } => Some(DataType::Int),
            
            Expression::UnaryOp { operator, operand, .. } => {
                self.infer_unary_op_type(operator, operand)
            }
            
            Expression::EnumAccess { .. } => Some(DataType::Enum),
            
            Expression::QuickFuncCall { name, .. } => {
                self.infer_quick_func_call_type(name)
            }
            
            Expression::ImportedFunctionCall { namespace_name, function_name, .. } => {
                self.infer_imported_function_call_type(namespace_name, function_name)
            }
            
            Expression::StaticMethodCall { object_name, method_name, .. } => {
                self.infer_static_method_call_type(object_name, method_name)
            }
            
            Expression::InstanceMethodCall { instance, method_name, .. } => {
                self.infer_instance_method_call_type(instance, method_name)
            }
            
            Expression::Conditional { true_value, .. } => {
                self.infer_type_from_expression(true_value)
            }
            
            Expression::Parenthesized { expression, .. } => {
                self.infer_type_from_expression(expression)
            }
            
            _ => None,
        }
    }

    /// Infer type from identifier (checks local variables first, then symbol table)
    fn infer_identifier_type(&self, name: &str) -> Option<DataType> {
        // Check local variables first
        if let Some(local_type) = self.local_variable_types.get(name) {
            return *local_type;
        }

        // Check if it's an enum
        if self.symbol_table.has_enum(name) {
            return Some(DataType::Enum);
        }

        // Check if it's a function
        if self.symbol_table.has_function(name) {
            return None; // Functions don't have a data type
        }

        // Check if it's a builtin static object
        if self.symbol_table.is_builtin_static_object(name) {
            return None;
        }

        // Check if it's an imported namespace
        if self.symbol_table.is_imported_namespace(name) {
            return None;
        }

        None
    }

    /// Infer type from qualified identifier
    fn infer_qualified_identifier_type(
        &self,
        parts: &[String],
        arguments: Option<&Vec<Expression>>,
    ) -> Option<DataType> {
        if parts.len() < 2 {
            return None;
        }

        let first_part = &parts[0];
        let second_part = &parts[1];

        // Check for enum access (2 parts, no call)
        if parts.len() == 2 && arguments.is_none() {
            if self.symbol_table.has_enum(first_part) {
                return Some(DataType::Enum);
            }
        }

        // Check for namespaced enum access (3 parts, no call)
        if parts.len() == 3 && arguments.is_none() {
            if self.symbol_table.is_imported_namespace(first_part) {
                if let Some(_enum_fields) = self.symbol_table.get_namespaced_enum(first_part, second_part) {
                    return Some(DataType::Enum);
                }
            }
        }

        // Check for function calls
        if arguments.is_some() {
            // Static method call (2 parts, PascalCase first part)
            if parts.len() == 2 && first_part.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                return self.infer_static_method_call_return_type(first_part, second_part);
            }

            // Namespaced function call (2 parts)
            if parts.len() == 2 {
                if let Some(func_info) = self.symbol_table.get_namespaced_function(first_part, second_part) {
                    return func_info.signature.return_type;
                }
            }
        }

        None
    }

    /// Infer type from imported function call
    fn infer_imported_function_call_type(
        &self,
        namespace_name: &str,
        function_name: &str,
    ) -> Option<DataType> {
        if let Some(func_info) = self.symbol_table.get_namespaced_function(namespace_name, function_name) {
            return func_info.signature.return_type;
        }
        None
    }

    /// Infer arithmetic operation result type from operands
    fn infer_arithmetic_op_type(&self, left: &Expression, right: &Expression) -> Option<DataType> {
        let left_type = self.infer_type_from_expression(left);
        let right_type = self.infer_type_from_expression(right);

        // String concatenation takes precedence
        if left_type == Some(DataType::String) || right_type == Some(DataType::String) {
            return Some(DataType::String);
        }

        // Numeric type promotion
        if let (Some(left_t), Some(right_t)) = (left_type, right_type) {
            if Self::is_numeric_type(left_t) && Self::is_numeric_type(right_t) {
                // Double > Float > Int
                if left_t == DataType::Double || right_t == DataType::Double {
                    return Some(DataType::Double);
                }

                if left_t == DataType::Float || right_t == DataType::Float {
                    return Some(DataType::Float);
                }

                return Some(DataType::Int);
            }
        }

        None
    }

    /// Helper to check if type is numeric
    fn is_numeric_type(data_type: DataType) -> bool {
        matches!(data_type, DataType::Int | DataType::Float | DataType::Double)
    }

    /// Infer type from prefixed constructor
    fn infer_prefixed_constructor_type(&self, prefix: &str) -> Option<DataType> {
        match prefix.to_lowercase().as_str() {
            "t" => Some(DataType::Tuple),
            "b" => Some(DataType::Blob),
            "r" => Some(DataType::Regex),
            _ => None,
        }
    }

    /// Infer type from function call value
    fn infer_function_call_type(&self, function_name: &str) -> Option<DataType> {
        if let Some(func_sig) = self.symbol_table.try_get_function(function_name) {
            return func_sig.return_type;
        }
        None
    }

    /// Infer type from unary operation
    fn infer_unary_op_type(&self, operator: &str, operand: &Expression) -> Option<DataType> {
        if operator == "!" || operator == "not" {
            return Some(DataType::Bool);
        }

        self.infer_type_from_expression(operand)
    }

    /// Infer type from QuickFunc call expression
    fn infer_quick_func_call_type(&self, name: &str) -> Option<DataType> {
        if let Some(func_sig) = self.symbol_table.try_get_function(name) {
            return func_sig.return_type;
        }
        None
    }

    /// Infer type from static method call expression
    /// Queries StaticObjectRegistry for builtin method return types
    fn infer_static_method_call_type(&self, object_name: &str, method_name: &str) -> Option<DataType> {
        self.infer_static_method_call_return_type(object_name, method_name)
    }

    /// Infer return type of static method by querying registry
    fn infer_static_method_call_return_type(&self, object_name: &str, method_name: &str) -> Option<DataType> {
        // TODO: Query StaticObjectRegistry when it's fully ported
        // For now, return None
        // This will be implemented when Builtins are fully available
        
        // Placeholder for registry query:
        // if StaticObjectRegistry::has_method(object_name, method_name) {
        //     if let Some(method) = StaticObjectRegistry::get_method(object_name, method_name) {
        //         return Self::convert_dix_type_to_data_type(method.return_type);
        //     }
        // }
        
        None
    }

    /// Infer type from instance method call expression
    /// Queries InstanceMethodRegistry for builtin method return types
    fn infer_instance_method_call_type(&self, instance: &Expression, method_name: &str) -> Option<DataType> {
        let instance_type = self.infer_type_from_expression(instance)?;

        // TODO: Query InstanceMethodRegistry when it's fully ported
        // For now, return None
        // This will be implemented when Builtins are fully available
        
        // Placeholder for registry query:
        // let dix_type = Self::convert_data_type_to_dix_type(instance_type)?;
        // if InstanceMethodRegistry::has_instance_method(dix_type, method_name) {
        //     if let Some(method) = InstanceMethodRegistry::get_instance_method(dix_type, method_name) {
        //         return Self::convert_dix_type_to_data_type(method.return_type);
        //     }
        // }
        
        None
    }

    /// Convert DixType to DataType
    /// TODO: Implement when DixType is fully available
    #[allow(dead_code)]
    fn convert_dix_type_to_data_type(dix_type: DixType) -> Option<DataType> {
        match dix_type {
            DixType::Int => Some(DataType::Int),
            DixType::Float => Some(DataType::Float),
            DixType::Double => Some(DataType::Double),
            DixType::String => Some(DataType::String),
            DixType::Bool => Some(DataType::Bool),
            DixType::Array => Some(DataType::Array),
            DixType::Tuple => Some(DataType::Tuple),
            DixType::Object => Some(DataType::Object),
            DixType::Hex => Some(DataType::Hex),
            DixType::Blob => Some(DataType::Blob),
            DixType::Regex => Some(DataType::Regex),
            DixType::Date => Some(DataType::Date),
            DixType::Timestamp => Some(DataType::Timestamp),
            DixType::Enum => Some(DataType::Enum),
            DixType::Any  => Some(DataType::Any),
            DixType::Void | DixType::Null => None,
        }
    }

    /// Convert DataType to DixType
    /// TODO: Implement when DixType is fully available
    #[allow(dead_code)]
    fn convert_data_type_to_dix_type(data_type: DataType) -> Option<DixType> {
        match data_type {
            DataType::Int => Some(DixType::Int),
            DataType::Float => Some(DixType::Float),
            DataType::Double => Some(DixType::Double),
            DataType::String => Some(DixType::String),
            DataType::Bool => Some(DixType::Bool),
            DataType::Array => Some(DixType::Array),
            DataType::Tuple => Some(DixType::Tuple),
            DataType::Object => Some(DixType::Object),
            DataType::Hex => Some(DixType::Hex),
            DataType::Blob => Some(DixType::Blob),
            DataType::Regex => Some(DixType::Regex),
            DataType::Date => Some(DixType::Date),
            DataType::Timestamp => Some(DixType::Timestamp),
            DataType::Enum => Some(DixType::Enum),
            DataType::Any | DataType::Function | DataType::Range => None,
        }
    }
          }
