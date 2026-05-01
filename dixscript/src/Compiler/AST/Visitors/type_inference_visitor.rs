use crate::Compiler::AST::*;
use crate::Compiler::Utilities::SymbolTable;
use crate::Builtins::Core::DixType;
use std::collections::HashMap;

/// Infers types from values and expressions.
/// Used by DataSectionAnalyzer, QuickFuncsSectionAnalyzer, and the LSP inlay-hints feature.
///
/// ## Visitor usage
/// `TypeInferenceVisitor` is NOT an `AstVisitorBase` implementor — it is a focused
/// query object. `AstVisitorBase` handles full-tree traversal (side-effects per node).
/// `TypeInferenceVisitor` answers "what type does this subtree produce?" which is a
/// pure query. Using both together: AstVisitorBase-based analyzers call into
/// TypeInferenceVisitor for type queries during their traversal.
pub struct TypeInferenceVisitor<'a> {
    symbol_table: &'a SymbolTable,
    local_variable_types: HashMap<String, Option<DataType>>,
}

impl<'a> TypeInferenceVisitor<'a> {
    /// Create TypeInferenceVisitor with optional local variable type information.
    /// `local_variable_types`: param/var name → Some(DataType) if annotated, None if untyped.
    pub fn new(
        symbol_table: &'a SymbolTable,
        local_variable_types: Option<HashMap<String, Option<DataType>>>,
    ) -> Self {
        TypeInferenceVisitor {
            symbol_table,
            local_variable_types: local_variable_types.unwrap_or_default(),
        }
    }

    /// Convenience: create with a parameter list from a QuickFunction.
    pub fn from_quickfunc_params(
        symbol_table: &'a SymbolTable,
        params: &[crate::Compiler::AST::QuickFuncParam],
    ) -> Self {
        let local_variable_types: HashMap<String, Option<DataType>> = params.iter()
            .map(|p| (p.name.clone(), p.data_type))
            .collect();
        TypeInferenceVisitor { symbol_table, local_variable_types }
    }

    // ── Public inference API ──────────────────────────────────────────────────

    /// Infer type from a Value node.
    pub fn infer_type_from_value(&self, value: &Value) -> Option<DataType> {
        match value {
            Value::Integer { .. }            => Some(DataType::Int),
            Value::Float { .. }              => Some(DataType::Float),
            Value::Double { .. }             => Some(DataType::Double),
            Value::ScientificNotation { .. } => Some(DataType::Double),
            Value::String { .. }             => Some(DataType::String),
            Value::InterpolatedString { .. } => Some(DataType::String),
            Value::Boolean { .. }            => Some(DataType::Bool),
            Value::HexColor { .. }           => Some(DataType::Hex),
            Value::Date { .. }               => Some(DataType::Date),
            Value::Timestamp { .. }          => Some(DataType::Timestamp),
            Value::Null { .. }               => None,
            Value::Array { .. }              => Some(DataType::Array),
            Value::NestedArray { .. }        => Some(DataType::Array),
            Value::Object { .. }             => Some(DataType::Object),
            Value::PrefixedConstructor { prefix, .. } => self.infer_prefixed_constructor_type(prefix),
            Value::EnumValue { .. }          => Some(DataType::Enum),
            Value::QuickFuncCall { function_name, .. } => {
                self.infer_function_call_type(function_name)
            }
            Value::Expression { expr, .. }   => self.infer_type_from_expression(expr),
            Value::Lambda { .. }             => Some(DataType::Function),
            Value::Range { .. }              => Some(DataType::Range),
            Value::Identifier { value: name, .. } => self.infer_identifier_type(name),
            _ => None,
        }
    }

    /// Infer type from an Expression node.
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
            Expression::LogicalOp { .. }    => Some(DataType::Bool),
            Expression::BitwiseOp { .. }    => Some(DataType::Int),

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

            // Static method call — e.g. Math.floor(x)
            Expression::StaticMethodCall { object_name, method_name, .. } => {
                self.infer_static_method_call_type(object_name, method_name)
            }

            // StaticFunction is an alternative representation (pre-enhancement)
            Expression::StaticFunction { class_name, method, .. } => {
                self.infer_static_method_call_type(class_name, method)
            }

            // Instance method call — e.g. myStr.toUpper()
            Expression::InstanceMethodCall { instance, method_name, .. } => {
                self.infer_instance_method_call_type(instance, method_name)
            }

            Expression::Conditional { true_value, false_value, .. } => {
                self.infer_type_from_expression(true_value)
                    .or_else(|| self.infer_type_from_expression(false_value))
            }

            Expression::Parenthesized { expression, .. } => {
                self.infer_type_from_expression(expression)
            }

            Expression::TypeCast { target_type, .. } => Some(*target_type),

            _ => None,
        }
    }

    // ── Private inference helpers ─────────────────────────────────────────────

    /// Infer type from identifier: checks local variables first, then symbol table.
    fn infer_identifier_type(&self, name: &str) -> Option<DataType> {
        // Local variables / parameters take priority.
        if let Some(local_type) = self.local_variable_types.get(name) {
            return *local_type;
        }

        // Enum name → the value it resolves to is Enum typed.
        if self.symbol_table.has_enum(name) {
            return Some(DataType::Enum);
        }

        // Function references have no data type.
        if self.symbol_table.has_function(name) {
            return None;
        }

        // Built-in static objects (Math, DateTime, etc.) have no data type — they're namespaces.
        if self.symbol_table.is_builtin_static_object(name) {
            return None;
        }

        // Imported namespace — no data type.
        if self.symbol_table.is_imported_namespace(name) {
            return None;
        }

        None
    }

    fn infer_qualified_identifier_type(
        &self,
        parts: &[String],
        arguments: Option<&Vec<Expression>>,
    ) -> Option<DataType> {
        if parts.len() < 2 {
            return None;
        }

        let first_part  = &parts[0];
        let second_part = &parts[1];

        // Enum access (2 parts, no call): EnumName.VALUE
        if parts.len() == 2 && arguments.is_none() {
            if self.symbol_table.has_enum(first_part) {
                return Some(DataType::Enum);
            }
        }

        // Namespaced enum access (3 parts, no call): ns.EnumName.VALUE
        if parts.len() == 3 && arguments.is_none() {
            if self.symbol_table.is_imported_namespace(first_part) {
                if let Some(_) = self.symbol_table.get_namespaced_enum(first_part, second_part) {
                    return Some(DataType::Enum);
                }
            }
        }

        // Function calls (has arguments)
        if arguments.is_some() {
            // Static method call (2 parts, PascalCase first)
            if parts.len() == 2
                && first_part.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
            {
                return self.infer_static_method_call_return_type(first_part, second_part);
            }

            // Namespaced function call
            if parts.len() == 2 {
                if let Some(func_info) = self.symbol_table.get_namespaced_function(first_part, second_part) {
                    return func_info.signature.return_type;
                }
            }
        }

        None
    }

    fn infer_imported_function_call_type(
        &self,
        namespace_name: &str,
        function_name: &str,
    ) -> Option<DataType> {
        self.symbol_table
            .get_namespaced_function(namespace_name, function_name)
            .and_then(|f| f.signature.return_type)
    }

    fn infer_arithmetic_op_type(&self, left: &Expression, right: &Expression) -> Option<DataType> {
        let left_type  = self.infer_type_from_expression(left);
        let right_type = self.infer_type_from_expression(right);

        // String concatenation takes precedence.
        if left_type == Some(DataType::String) || right_type == Some(DataType::String) {
            return Some(DataType::String);
        }

        // Numeric type promotion: Double > Float > Int.
        if let (Some(lt), Some(rt)) = (left_type, right_type) {
            if Self::is_numeric_type(lt) && Self::is_numeric_type(rt) {
                if lt == DataType::Double || rt == DataType::Double {
                    return Some(DataType::Double);
                }
                if lt == DataType::Float || rt == DataType::Float {
                    return Some(DataType::Float);
                }
                return Some(DataType::Int);
            }
            // If both sides are the same (non-numeric) type, return that.
            if lt == rt { return Some(lt); }
            // One side known — use it as best guess.
            return Some(lt);
        }

        // One side known.
        left_type.or(right_type)
    }

    fn is_numeric_type(data_type: DataType) -> bool {
        matches!(data_type, DataType::Int | DataType::Float | DataType::Double)
    }

    fn infer_prefixed_constructor_type(&self, prefix: &str) -> Option<DataType> {
        match prefix.to_lowercase().as_str() {
            "t" => Some(DataType::Tuple),
            "b" => Some(DataType::Blob),
            "r" => Some(DataType::Regex),
            _   => None,
        }
    }

    fn infer_function_call_type(&self, function_name: &str) -> Option<DataType> {
        self.symbol_table
            .try_get_function(function_name)
            .and_then(|sig| sig.return_type)
    }

    fn infer_unary_op_type(&self, operator: &str, operand: &Expression) -> Option<DataType> {
        match operator {
            "!" | "not" => Some(DataType::Bool),
            _           => self.infer_type_from_expression(operand),
        }
    }

    fn infer_quick_func_call_type(&self, name: &str) -> Option<DataType> {
        self.symbol_table
            .try_get_function(name)
            .and_then(|sig| sig.return_type)
    }

    /// Infer return type of a static method call by querying StaticObjectRegistry.
    fn infer_static_method_call_type(&self, object_name: &str, method_name: &str) -> Option<DataType> {
        self.infer_static_method_call_return_type(object_name, method_name)
    }

    fn infer_static_method_call_return_type(
        &self,
        object_name: &str,
        method_name: &str,
    ) -> Option<DataType> {
        use crate::Builtins::Resolver::static_object_registry;
        // Ensure registry is initialised (idempotent).
        static_object_registry::initialize_static_registry();
        if let Some(info) = static_object_registry::get_method_info(object_name, method_name) {
            return Self::convert_dix_type_to_data_type(info.return_type);
        }
        None
    }

    /// Infer return type of an instance method call by querying InstanceMethodRegistry.
    fn infer_instance_method_call_type(
        &self,
        instance: &Expression,
        method_name: &str,
    ) -> Option<DataType> {
        let instance_data_type = self.infer_type_from_expression(instance)?;
        let dix_type = Self::convert_data_type_to_dix_type(instance_data_type)?;

        use crate::Builtins::Resolver::instance_method_registry;
        instance_method_registry::initialize();
        if let Some(method) = instance_method_registry::get_instance_method(dix_type, method_name) {
            return Self::convert_dix_type_to_data_type(method.return_type());
        }
        None
    }

    // ── Type conversion helpers (now actively used) ───────────────────────────

    pub fn convert_dix_type_to_data_type(dix_type: DixType) -> Option<DataType> {
        match dix_type {
            DixType::Int       => Some(DataType::Int),
            DixType::Float     => Some(DataType::Float),
            DixType::Double    => Some(DataType::Double),
            DixType::String    => Some(DataType::String),
            DixType::Bool      => Some(DataType::Bool),
            DixType::Array     => Some(DataType::Array),
            DixType::Tuple     => Some(DataType::Tuple),
            DixType::Object    => Some(DataType::Object),
            DixType::Hex       => Some(DataType::Hex),
            DixType::Blob      => Some(DataType::Blob),
            DixType::Regex     => Some(DataType::Regex),
            DixType::Date      => Some(DataType::Date),
            DixType::Timestamp => Some(DataType::Timestamp),
            DixType::Enum      => Some(DataType::Enum),
            DixType::Any       => Some(DataType::Any),
            DixType::Void | DixType::Null => None,
        }
    }

    pub fn convert_data_type_to_dix_type(data_type: DataType) -> Option<DixType> {
        match data_type {
            DataType::Int       => Some(DixType::Int),
            DataType::Float     => Some(DixType::Float),
            DataType::Double    => Some(DixType::Double),
            DataType::String    => Some(DixType::String),
            DataType::Bool      => Some(DixType::Bool),
            DataType::Array     => Some(DixType::Array),
            DataType::Tuple     => Some(DixType::Tuple),
            DataType::Object    => Some(DixType::Object),
            DataType::Hex       => Some(DixType::Hex),
            DataType::Blob      => Some(DixType::Blob),
            DataType::Regex     => Some(DixType::Regex),
            DataType::Date      => Some(DixType::Date),
            DataType::Timestamp => Some(DixType::Timestamp),
            DataType::Enum      => Some(DixType::Enum),
            DataType::Any | DataType::Function | DataType::Range => None,
        }
    }
}
