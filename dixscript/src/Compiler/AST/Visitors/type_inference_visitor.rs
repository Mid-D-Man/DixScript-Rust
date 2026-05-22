use crate::Compiler::AST::*;
use crate::Compiler::Utilities::SymbolTable;
use crate::Builtins::Core::DixType;
use std::collections::HashMap;

/// Array instance methods that return the element type rather than a fixed type.
/// The registry marks these as DixType::Any — we override using element_type_hints.
const ARRAY_ELEMENT_METHODS: &[&str] = &[
    "first", "last", "get", "at", "pop", "random",
];

/// Tuple positional accessors that return element type.
const TUPLE_ELEMENT_METHODS: &[&str] = &[
    "first", "second", "third", "fourth", "fifth", "sixth", "get", "at",
];

/// Infers types from values and expressions.
///
/// ## element_type_hints
/// Maps variable/parameter name → the element type of that array or tuple variable.
/// Populated from `LocalScopeTracker::get_all_element_type_hints()` during QuickFuncs
/// analysis. When a method like `.first()` or `.last()` is called on an array whose
/// element type is known, this lets us return the actual element type instead of `Any`.
pub struct TypeInferenceVisitor<'a> {
    symbol_table: &'a SymbolTable,
    local_variable_types: HashMap<String, Option<DataType>>,
    element_type_hints: HashMap<String, DataType>,
}

impl<'a> TypeInferenceVisitor<'a> {
    /// Standard constructor — no element type hints.
    pub fn new(
        symbol_table: &'a SymbolTable,
        local_variable_types: Option<HashMap<String, Option<DataType>>>,
    ) -> Self {
        TypeInferenceVisitor {
            symbol_table,
            local_variable_types: local_variable_types.unwrap_or_default(),
            element_type_hints: HashMap::new(),
        }
    }

    /// Constructor with element type hints for richer array/tuple inference.
    pub fn new_with_element_hints(
        symbol_table: &'a SymbolTable,
        local_variable_types: Option<HashMap<String, Option<DataType>>>,
        element_type_hints: Option<HashMap<String, DataType>>,
    ) -> Self {
        TypeInferenceVisitor {
            symbol_table,
            local_variable_types: local_variable_types.unwrap_or_default(),
            element_type_hints: element_type_hints.unwrap_or_default(),
        }
    }

    /// Convenience: create from a QuickFunction parameter list.
    pub fn from_quickfunc_params(
        symbol_table: &'a SymbolTable,
        params: &[crate::Compiler::AST::QuickFuncParam],
    ) -> Self {
        let local_variable_types: HashMap<String, Option<DataType>> = params
            .iter()
            .map(|p| (p.name.clone(), p.data_type))
            .collect();
        TypeInferenceVisitor {
            symbol_table,
            local_variable_types,
            element_type_hints: HashMap::new(),
        }
    }

    // ── Public inference API ──────────────────────────────────────────────────

    /// Infer type from a Value node.
    pub fn infer_type_from_value(&self, value: &Value) -> Option<DataType> {
        match value {
            Value::Integer { .. }            => Some(DataType::Int),
            Value::Long { .. }               => Some(DataType::Long),
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
            Value::PrefixedConstructor { prefix, .. } => {
                self.infer_prefixed_constructor_type(prefix)
            }
            Value::EnumValue { .. }                  => Some(DataType::Enum),
            Value::QuickFuncCall { function_name, .. } => {
                self.infer_function_call_type(function_name)
            }
            Value::Expression { expr, .. } => self.infer_type_from_expression(expr),
            Value::Lambda { .. }           => Some(DataType::Function),
            Value::Range { .. }            => Some(DataType::Range),
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

            Expression::StaticMethodCall { object_name, method_name, .. } => {
                self.infer_static_method_call_type(object_name, method_name)
            }

            Expression::StaticFunction { class_name, method, .. } => {
                self.infer_static_method_call_type(class_name, method)
            }

            Expression::InstanceMethodCall { instance, method_name, .. } => {
                self.infer_instance_method_call_type(instance, method_name)
            }

            // After AST enhancement, obj.prop chains become PropertyAccess nodes.
            // Try symbol-table path lookup before giving up.
            Expression::PropertyAccess { object, property, .. } => {
                self.infer_property_access_type(object, property)
            }

            // array[i] or tuple[i] — try to return the element type
            Expression::IndexAccess { object, .. } => {
                self.infer_index_access_type(object)
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

    // ── Element type API (called by analyzers to populate element_type_hints) ─

    /// Infer the element type of an array or tuple VALUE node.
    /// Returns Some(T) if the collection is non-empty and has a uniform element type.
    /// For tuples (which can be heterogeneous) this returns the first element's type.
    pub fn infer_element_type_from_value(&self, value: &Value) -> Option<DataType> {
        match value {
            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                self.infer_uniform_element_type_from_values(values)
            }
            Value::PrefixedConstructor { prefix, arguments, .. }
                if prefix.eq_ignore_ascii_case("t") =>
            {
                // Tuple: representative type = first element
                arguments.first().and_then(|v| self.infer_type_from_value(v))
            }
            _ => None,
        }
    }

    /// Infer the element type of an array or tuple EXPRESSION node.
    /// For an Identifier, checks element_type_hints map.
    pub fn infer_element_type_from_expression(&self, expr: &Expression) -> Option<DataType> {
        match expr {
            Expression::Value { value, .. } => self.infer_element_type_from_value(value),
            Expression::Identifier { name, .. } => {
                self.element_type_hints.get(name.as_str()).copied()
            }
            // Chained: myMap.items.first() — recurse into the object to try
            Expression::PropertyAccess { object, .. } => {
                self.infer_element_type_from_expression(object)
            }
            _ => None,
        }
    }

    // ── Private inference helpers ─────────────────────────────────────────────

    fn infer_identifier_type(&self, name: &str) -> Option<DataType> {
        // Local variables / parameters take priority.
        if let Some(local_type) = self.local_variable_types.get(name) {
            return *local_type;
        }
        if self.symbol_table.has_enum(name) {
            return Some(DataType::Enum);
        }
        // Function references — no data type (they are callables, not values).
        if self.symbol_table.has_function(name) {
            return None;
        }
        if self.symbol_table.is_builtin_static_object(name) {
            return None;
        }
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
                if self.symbol_table.get_namespaced_enum(first_part, second_part).is_some() {
                    return Some(DataType::Enum);
                }
            }
        }

        if arguments.is_some() {
            // Static method call (2 parts, PascalCase first): Math.sqrt(x)
            if parts.len() == 2
                && first_part.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
            {
                return self.infer_static_method_call_return_type(first_part, second_part);
            }

            // Namespaced function call (2 parts): ns.func()
            if parts.len() == 2 {
                if let Some(func_info) =
                    self.symbol_table.get_namespaced_function(first_part, second_part)
                {
                    return func_info.signature.return_type;
                }
            }
        }

        // Property access (2 parts, no call) — may be a DATA table path property
        // that the enhancer hasn't turned into PropertyAccess yet (pre-enhancement path).
        if parts.len() == 2 && arguments.is_none() {
            let path = format!("{}.{}", first_part, second_part);
            if let Some(var) = self.symbol_table.try_get_data_variable(&path) {
                return var.effective_type();
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

    fn infer_arithmetic_op_type(
        &self,
        left:  &Expression,
        right: &Expression,
    ) -> Option<DataType> {
        let left_type  = self.infer_type_from_expression(left);
        let right_type = self.infer_type_from_expression(right);

        // String concatenation takes precedence.
        if left_type == Some(DataType::String) || right_type == Some(DataType::String) {
            return Some(DataType::String);
        }

        if let (Some(lt), Some(rt)) = (left_type, right_type) {
            // Any on one side — defer to the other operand's type.
            if lt == DataType::Any && rt == DataType::Any { return None; }
            if lt == DataType::Any { return Some(rt); }
            if rt == DataType::Any { return Some(lt); }

            // Numeric promotion: Double > Float > Long > Int.
            if Self::is_numeric_type(lt) && Self::is_numeric_type(rt) {
                if lt == DataType::Double || rt == DataType::Double {
                    return Some(DataType::Double);
                }
                if lt == DataType::Float || rt == DataType::Float {
                    return Some(DataType::Float);
                }
                if lt == DataType::Long || rt == DataType::Long {
                    return Some(DataType::Long);
                }
                return Some(DataType::Int);
            }
            // Same non-numeric type — use it.
            if lt == rt { return Some(lt); }
            // One side known — best-effort.
            return Some(lt);
        }

        left_type.or(right_type)
    }

    fn is_numeric_type(dt: DataType) -> bool {
        matches!(dt, DataType::Int | DataType::Long | DataType::Float | DataType::Double)
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

    fn infer_static_method_call_type(
        &self,
        object_name: &str,
        method_name: &str,
    ) -> Option<DataType> {
        self.infer_static_method_call_return_type(object_name, method_name)
    }

    fn infer_static_method_call_return_type(
        &self,
        object_name: &str,
        method_name: &str,
    ) -> Option<DataType> {
        use crate::Builtins::Resolver::static_object_registry;
        static_object_registry::initialize_static_registry();
        if let Some(info) = static_object_registry::get_method_info(object_name, method_name) {
            return Self::convert_dix_type_to_data_type(info.return_type);
        }
        None
    }

    /// Infer the return type of an instance method call.
    ///
    /// For methods in ARRAY_ELEMENT_METHODS / TUPLE_ELEMENT_METHODS that the
    /// instance method registry marks as returning `DixType::Any`, we try to
    /// resolve the actual element type from `element_type_hints` first.
    fn infer_instance_method_call_type(
        &self,
        instance:    &Expression,
        method_name: &str,
    ) -> Option<DataType> {
        let instance_data_type = self.infer_type_from_expression(instance)?;

        // Array element-returning methods — check element_type_hints
        if instance_data_type == DataType::Array
            && ARRAY_ELEMENT_METHODS.contains(&method_name)
        {
            if let Some(elem) = self.infer_element_type_from_expression(instance) {
                return Some(elem);
            }
            // No hint available — fall through to registry (will return None for Any)
        }

        // Tuple element-returning methods — check element_type_hints
        if instance_data_type == DataType::Tuple
            && TUPLE_ELEMENT_METHODS.contains(&method_name)
        {
            if let Some(elem) = self.infer_element_type_from_expression(instance) {
                return Some(elem);
            }
        }

        let dix_type = Self::convert_data_type_to_dix_type(instance_data_type)?;

        use crate::Builtins::Resolver::instance_method_registry;
        instance_method_registry::initialize();
        if let Some(method) = instance_method_registry::get_instance_method(dix_type, method_name) {
            let ret = method.return_type();
            // Registry returns Any → return None rather than propagating Any,
            // which would trigger false "can't infer" warnings downstream.
            if ret == DixType::Any || ret == DixType::Void || ret == DixType::Null {
                return None;
            }
            return Self::convert_dix_type_to_data_type(ret);
        }
        None
    }

    /// Infer type of `object.property` by building the dotted path and
    /// looking it up in the symbol table's data variables.
    ///
    /// Works for TABLE properties stored as "path.field" in the symbol table.
    /// Object properties declared as flat `name = { field = value }` are NOT
    /// in the symbol table at field depth — their type inference returns None.
    fn infer_property_access_type(
        &self,
        object:   &Expression,
        property: &str,
    ) -> Option<DataType> {
        if let Some(base) = Self::build_property_path(object) {
            let full_path = format!("{}.{}", base, property);

            // Direct symbol table lookup (covers TABLE properties: "path.field")
            if let Some(var) = self.symbol_table.try_get_data_variable(&full_path) {
                return var.effective_type();
            }
        }

        // Could not resolve via symbol table — return None rather than Any
        // so callers know this is genuinely unknown rather than "accepts anything".
        None
    }

    /// Infer the type of `array[index]` — returns the element type when known.
    fn infer_index_access_type(&self, object: &Expression) -> Option<DataType> {
        let obj_type = self.infer_type_from_expression(object)?;
        match obj_type {
            DataType::Array | DataType::Tuple => {
                self.infer_element_type_from_expression(object)
            }
            _ => None,
        }
    }

    /// Recursively build a dotted string path from a chain of PropertyAccess /
    /// Identifier nodes: `a.b.c` → `"a.b.c"`.
    fn build_property_path(expr: &Expression) -> Option<String> {
        match expr {
            Expression::Identifier { name, .. } => Some(name.clone()),
            Expression::PropertyAccess { object, property, .. } => {
                Self::build_property_path(object)
                    .map(|base| format!("{}.{}", base, property))
            }
            _ => None,
        }
    }

    /// Infer a uniform element type from a slice of values.
    /// Returns `Some(T)` only when every element whose type can be inferred
    /// shares the same type T.  Returns `None` for empty or heterogeneous slices.
    fn infer_uniform_element_type_from_values(&self, values: &[Value]) -> Option<DataType> {
        if values.is_empty() {
            return None;
        }
        let first = self.infer_type_from_value(&values[0])?;
        for v in values.iter().skip(1) {
            match self.infer_type_from_value(v) {
                Some(t) if t == first => {}    // same type — keep going
                Some(_)               => return None, // heterogeneous
                None                  => {}    // unknown — assume compatible, keep going
            }
        }
        Some(first)
    }

    // ── Type conversion helpers ───────────────────────────────────────────────

    pub fn convert_dix_type_to_data_type(dix_type: DixType) -> Option<DataType> {
        match dix_type {
            DixType::Int       => Some(DataType::Int),
            DixType::Long      => Some(DataType::Long),
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
            DataType::Long      => Some(DixType::Long),
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
