use crate::Compiler::AST::*;
use crate::Compiler::Utilities::SymbolTable;
use crate::Builtins::Core::DixType;
use std::collections::HashMap;

/// Array instance methods that return the element type rather than a fixed type.
const ARRAY_ELEMENT_METHODS: &[&str] = &[
    "first", "last", "get", "at", "pop", "random",
];

/// Tuple positional accessors that return element type.
const TUPLE_ELEMENT_METHODS: &[&str] = &[
    "first", "second", "third", "fourth", "fifth", "sixth", "get", "at",
];

pub struct TypeInferenceVisitor<'a> {
    symbol_table: &'a SymbolTable,
    local_variable_types: HashMap<String, Option<DataType>>,
    element_type_hints: HashMap<String, DataType>,
}

impl<'a> TypeInferenceVisitor<'a> {
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
            Value::EnumValue { .. }                   => Some(DataType::Enum),
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
            Expression::PropertyAccess { object, property, .. } => {
                self.infer_property_access_type(object, property)
            }
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
            // TypeCast is Copy so *target_type is fine regardless of TypedArray/TypedTuple
            Expression::TypeCast { target_type, .. } => Some(*target_type),
            _ => None,
        }
    }

    // ── Element type API ──────────────────────────────────────────────────────

    pub fn infer_element_type_from_value(&self, value: &Value) -> Option<DataType> {
        match value {
            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                self.infer_uniform_element_type_from_values(values)
            }
            Value::PrefixedConstructor { prefix, arguments, .. }
                if prefix.eq_ignore_ascii_case("t") =>
            {
                arguments.first().and_then(|v| self.infer_type_from_value(v))
            }
            _ => None,
        }
    }

    pub fn infer_element_type_from_expression(&self, expr: &Expression) -> Option<DataType> {
        match expr {
            Expression::Value { value, .. } => self.infer_element_type_from_value(value),

            Expression::Identifier { name, .. } => {
                // 1. Explicit hint (populated from TypedArray declarations in scope tracker)
                if let Some(&hint) = self.element_type_hints.get(name.as_str()) {
                    return Some(hint);
                }
                // 2. Fallback: if the variable is a TypedArray, extract elem from its type
                if let Some(Some(var_type)) = self.local_variable_types.get(name.as_str()) {
                    if let DataType::TypedArray(elem) = *var_type {
                        return Some(elem.to_data_type());
                    }
                    // TypedTuple — first defined slot as representative
                    if let DataType::TypedTuple(arr) = *var_type {
                        return arr[0].map(|e| e.to_data_type());
                    }
                }
                None
            }

            // Chained: myMap.items.first() — recurse into the object
            Expression::PropertyAccess { object, .. } => {
                self.infer_element_type_from_expression(object)
            }
            _ => None,
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn infer_identifier_type(&self, name: &str) -> Option<DataType> {
        if let Some(local_type) = self.local_variable_types.get(name) {
            return *local_type;
        }
        if self.symbol_table.has_enum(name) {
            return Some(DataType::Enum);
        }
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
    if parts.len() < 2 { return None; }

    let first_part  = &parts[0];
    let second_part = &parts[1];

    // Local enum: Status.ACTIVE
    if parts.len() == 2 && arguments.is_none() && self.symbol_table.has_enum(first_part) {
        return Some(DataType::Enum);
    }

    // Imported namespace enum: utils.Status.ACTIVE (3 parts, no call)
    if parts.len() == 3 && arguments.is_none()
        && self.symbol_table.is_imported_namespace(first_part)
        && self.symbol_table.get_namespaced_enum(first_part, second_part).is_some()
    {
        return Some(DataType::Enum);
    }

    if arguments.is_some() {
        // Static builtin call: Math.round(), DateTime.now()
        if parts.len() == 2
            && first_part.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
        {
            return self.infer_static_method_call_return_type(first_part, second_part);
        }

        // Imported namespace function call: utils.computeTax()
        if parts.len() == 2 {
            if let Some(func_info) =
                self.symbol_table.get_namespaced_function(first_part, second_part)
            {
                return func_info.signature.return_type;
            }
        }

        // Instance method call on a local variable or parameter with a known type.
        // e.g. `myStr.contains("x")` where myStr: String  → Bool
        //      `items.length()`      where items: Array    → Int
        //      `dt.addDays(7)`       where dt: Timestamp   → Timestamp
        //
        // IMPORTANT: Object type is explicitly excluded from the registry lookup.
        // Object variables may have lambda functions as properties (e.g.,
        // `calculator.add` where add = (a,b) => a+b).  The built-in Object
        // instance method registry contains methods like `add(key, value) → Object`
        // for adding new properties, which has the same name but a completely
        // different signature and return type.  Consulting the registry for Object
        // would infer `DataType::Object` for the call and fire a false QFUNC015
        // return-type-mismatch error.  Returning None here defers type checking to
        // runtime, which is the correct behaviour for dynamic Object property calls.
        if parts.len() == 2 {
            if let Some(maybe_var_type) = self.local_variable_types.get(first_part.as_str()) {
                if let Some(var_type) = maybe_var_type {
                    // Strip TypedArray/TypedTuple wrappers for registry lookup
                    let base_type = var_type.base_collection_type();

                    // Object properties can be lambdas — skip registry to avoid
                    // conflating built-in Object methods with lambda property calls.
                    if base_type != DataType::Object {
                        if let Some(dix_type) = Self::convert_data_type_to_dix_type(base_type) {
                            use crate::Builtins::Resolver::instance_method_registry;
                            instance_method_registry::initialize();
                            if let Some(method) = instance_method_registry::get_instance_method(
                                dix_type,
                                second_part.as_str(),
                            ) {
                                let ret = method.return_type();
                                // Void/Null/Any cannot be usefully propagated
                                if ret != DixType::Any
                                    && ret != DixType::Void
                                    && ret != DixType::Null
                                {
                                    return Self::convert_dix_type_to_data_type(ret);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Property access (no call): data.server or server.host
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

        if left_type == Some(DataType::String) || right_type == Some(DataType::String) {
            return Some(DataType::String);
        }

        if let (Some(lt), Some(rt)) = (left_type, right_type) {
            if lt == DataType::Any && rt == DataType::Any { return None; }
            if lt == DataType::Any { return Some(rt); }
            if rt == DataType::Any { return Some(lt); }

            if Self::is_numeric_type(lt) && Self::is_numeric_type(rt) {
                if lt == DataType::Double || rt == DataType::Double { return Some(DataType::Double); }
                if lt == DataType::Float  || rt == DataType::Float  { return Some(DataType::Float);  }
                if lt == DataType::Long   || rt == DataType::Long   { return Some(DataType::Long);   }
                return Some(DataType::Int);
            }
            if lt == rt { return Some(lt); }
            return Some(lt);
        }

        left_type.or(right_type)
    }

    #[inline]
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
        self.symbol_table.try_get_function(function_name).and_then(|sig| sig.return_type)
    }

    fn infer_unary_op_type(&self, operator: &str, operand: &Expression) -> Option<DataType> {
        match operator {
            "!" | "not" => Some(DataType::Bool),
            _           => self.infer_type_from_expression(operand),
        }
    }

    fn infer_quick_func_call_type(&self, name: &str) -> Option<DataType> {
        self.symbol_table.try_get_function(name).and_then(|sig| sig.return_type)
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
    /// Handles TypedArray/TypedTuple by stripping to base type for registry lookup,
    /// while still using the typed element info for element-returning methods.
    fn infer_instance_method_call_type(
        &self,
        instance:    &Expression,
        method_name: &str,
    ) -> Option<DataType> {
        let instance_data_type = self.infer_type_from_expression(instance)?;

        // Strip typed-collection wrapper for base comparisons
        let base_type = instance_data_type.base_collection_type();

        // Array element-returning methods
        if base_type == DataType::Array && ARRAY_ELEMENT_METHODS.contains(&method_name) {
            // TypedArray annotation is authoritative
            if let DataType::TypedArray(elem) = instance_data_type {
                return Some(elem.to_data_type());
            }
            // Fall back to element_type_hints / value inference
            if let Some(elem) = self.infer_element_type_from_expression(instance) {
                return Some(elem);
            }
        }

        // Tuple element-returning methods
        if base_type == DataType::Tuple && TUPLE_ELEMENT_METHODS.contains(&method_name) {
            // TypedTuple: first defined slot is the representative element type
            if let DataType::TypedTuple(arr) = instance_data_type {
                if let Some(first) = arr[0] {
                    return Some(first.to_data_type());
                }
            }
            if let Some(elem) = self.infer_element_type_from_expression(instance) {
                return Some(elem);
            }
        }

        let dix_type = Self::convert_data_type_to_dix_type(base_type)?;

        use crate::Builtins::Resolver::instance_method_registry;
        instance_method_registry::initialize();
        if let Some(method) = instance_method_registry::get_instance_method(dix_type, method_name) {
            let ret = method.return_type();
            if ret == DixType::Any || ret == DixType::Void || ret == DixType::Null {
                return None;
            }
            return Self::convert_dix_type_to_data_type(ret);
        }
        None
    }

    fn infer_property_access_type(
        &self,
        object:   &Expression,
        property: &str,
    ) -> Option<DataType> {
        if let Some(base) = Self::build_property_path(object) {
            let full_path = format!("{}.{}", base, property);
            if let Some(var) = self.symbol_table.try_get_data_variable(&full_path) {
                return var.effective_type();
            }
        }
        None
    }

    /// Infer the type of `collection[index]`.
    /// TypedArray/TypedTuple annotations are used directly when available,
    /// giving precise element types without needing to inspect the value.
    fn infer_index_access_type(&self, object: &Expression) -> Option<DataType> {
        let obj_type = self.infer_type_from_expression(object)?;
        match obj_type {
            // Typed array: annotation is authoritative
            DataType::TypedArray(elem) => Some(elem.to_data_type()),

            // Typed tuple: return first defined element type as best approximation
            // (per-index typing requires constant-index analysis — deferred)
            DataType::TypedTuple(arr) => arr[0].map(|e| e.to_data_type()),

            // Plain (untyped) collections: try element_type_hints / value inference
            DataType::Array | DataType::Tuple => {
                self.infer_element_type_from_expression(object)
            }
            _ => None,
        }
    }

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

    fn infer_uniform_element_type_from_values(&self, values: &[Value]) -> Option<DataType> {
        if values.is_empty() { return None; }
        let first = self.infer_type_from_value(&values[0])?;
        for v in values.iter().skip(1) {
            match self.infer_type_from_value(v) {
                Some(t) if t == first => {}
                Some(_)               => return None,
                None                  => {}
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
            DataType::Int           => Some(DixType::Int),
            DataType::Long          => Some(DixType::Long),
            DataType::Float         => Some(DixType::Float),
            DataType::Double        => Some(DixType::Double),
            DataType::String        => Some(DixType::String),
            DataType::Bool          => Some(DixType::Bool),
            DataType::Array         => Some(DixType::Array),
            DataType::Tuple         => Some(DixType::Tuple),
            DataType::Object        => Some(DixType::Object),
            DataType::Hex           => Some(DixType::Hex),
            DataType::Blob          => Some(DixType::Blob),
            DataType::Regex         => Some(DixType::Regex),
            DataType::Date          => Some(DixType::Date),
            DataType::Timestamp     => Some(DixType::Timestamp),
            DataType::Enum          => Some(DixType::Enum),
            // Typed collections map to their base DixType
            DataType::TypedArray(_) => Some(DixType::Array),
            DataType::TypedTuple(_) => Some(DixType::Tuple),
            // No DixType mapping
            DataType::Any | DataType::Function | DataType::Range => None,
        }
    }
            }
