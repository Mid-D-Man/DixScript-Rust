// dixscript/src/Compiler/AST/Visitors/type_inference_visitor.rs

use crate::Compiler::AST::*;
use crate::Compiler::Utilities::SymbolTable;
use crate::Builtins::Core::DixType;
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// Method-classification tables — derived directly from the builtin source files
// ─────────────────────────────────────────────────────────────────────────────

/// Array instance methods whose return value IS the stored element.
/// `pop` and `shift` are NOT here — in DixScript they return a NEW ARRAY
/// (functional/immutable style), not the removed element.
const ARRAY_ELEMENT_METHODS: &[&str] = &[
    "first", "last", "get",
];

/// Array instance methods that return a new array whose element type matches
/// the receiver's element type.  When the receiver is `TypedArray(T)` the
/// return type is also `TypedArray(T)`.
///
/// Sources: array_methods.rs return types (all DixType::Array):
///   set, push, pop, shift, unshift, slice, reverse, sort, concat,
///   filter, distinct
///
/// NOTE: `flatten` is intentionally absent — it dissolves the TypedArray
/// nesting and is handled separately (always returns plain Array).
const ARRAY_PRESERVING_METHODS: &[&str] = &[
    "set", "push", "pop", "shift", "unshift",
    "slice", "reverse", "sort", "concat",
    "filter", "distinct",
];

/// Tuple instance methods that map to a **specific positional slot** of a
/// TypedTuple annotation.  The index in this array is the TypedTuple slot index.
const TUPLE_POSITIONAL_METHODS: &[(&str, usize)] = &[
    ("first",  0),
    ("second", 1),
    ("third",  2),
    ("fourth", 3),
    ("fifth",  4),
    ("sixth",  5),
];

/// Tuple instance methods that return an element but whose slot index is not
/// statically known at the call site (e.g. `get(i)`).
const TUPLE_DYNAMIC_ELEMENT_METHODS: &[&str] = &["get"];

/// All tuple element-returning methods (positional + dynamic) for quick lookup.
const TUPLE_ELEMENT_METHODS: &[&str] = &[
    "first", "second", "third", "fourth", "fifth", "sixth", "get",
];

/// Tuple instance methods that return `DixType::Tuple` but change element
/// ordering, making the TypedTuple slot annotations inaccurate.
/// These return plain `DataType::Tuple` even when the receiver is TypedTuple.
const TUPLE_PRESERVING_METHODS: &[&str] = &["reverse", "swap"];

// ─────────────────────────────────────────────────────────────────────────────
// TypeInferenceVisitor
// ─────────────────────────────────────────────────────────────────────────────

pub struct TypeInferenceVisitor<'a> {
    symbol_table:         &'a SymbolTable,
    local_variable_types: HashMap<String, Option<DataType>>,
    element_type_hints:   HashMap<String, DataType>,
}

impl<'a> TypeInferenceVisitor<'a> {
    // ── Constructors ──────────────────────────────────────────────────────────

    pub fn new(
        symbol_table:         &'a SymbolTable,
        local_variable_types: Option<HashMap<String, Option<DataType>>>,
    ) -> Self {
        TypeInferenceVisitor {
            symbol_table,
            local_variable_types: local_variable_types.unwrap_or_default(),
            element_type_hints:   HashMap::new(),
        }
    }

    pub fn new_with_element_hints(
        symbol_table:         &'a SymbolTable,
        local_variable_types: Option<HashMap<String, Option<DataType>>>,
        element_type_hints:   Option<HashMap<String, DataType>>,
    ) -> Self {
        TypeInferenceVisitor {
            symbol_table,
            local_variable_types: local_variable_types.unwrap_or_default(),
            element_type_hints:   element_type_hints.unwrap_or_default(),
        }
    }

    pub fn from_quickfunc_params(
        symbol_table: &'a SymbolTable,
        params:       &[crate::Compiler::AST::QuickFuncParam],
    ) -> Self {
        let local_variable_types = params
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

            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                self.try_infer_typed_array_from_values(values)
                    .or(Some(DataType::Array))
            }

            Value::Object { .. } => Some(DataType::Object),

            Value::PrefixedConstructor { prefix, arguments, .. } => {
                self.infer_prefixed_constructor_type(prefix, arguments)
            }

            Value::EnumValue { .. } => Some(DataType::Enum),

            Value::QuickFuncCall { function_name, .. } => {
                self.infer_function_call_type(function_name)
            }

            Value::Expression { expr, .. }    => self.infer_type_from_expression(expr),
            Value::Lambda { .. }              => Some(DataType::Function),
            Value::Range { .. }               => Some(DataType::Range),
            Value::Identifier { value: name, .. } => self.infer_identifier_type(name),
            _ => None,
        }
    }

    /// Exhaustive match over every `Expression` variant.
    pub fn infer_type_from_expression(&self, expr: &Expression) -> Option<DataType> {
        match expr {
            Expression::Value { value, .. } => self.infer_type_from_value(value),

            Expression::Identifier { name, .. } => self.infer_identifier_type(name),

            Expression::QualifiedIdentifier { parts, arguments, .. } => {
                self.infer_qualified_identifier_type(parts, arguments.as_ref())
            }

            Expression::FunctionCall { name, .. } => self.infer_function_call_type(name),

            Expression::QuickFuncCall { name, .. } => self.infer_quick_func_call_type(name),

            Expression::DixFunctionCall { function_name, .. } => {
                self.infer_dix_function_call_type(function_name)
            }

            Expression::StaticMethodCall { object_name, method_name, arguments, .. } => {
                self.infer_static_method_call_type_with_args(object_name, method_name, arguments)
            }

            Expression::StaticFunction { class_name, method, arguments, .. } => {
                self.infer_static_method_call_type_with_args(class_name, method, arguments)
            }

            Expression::InstanceMethodCall { instance, method_name, .. } => {
                self.infer_instance_method_call_type(instance, method_name)
            }

            Expression::BuiltinFunction { target, method, arguments, .. } => {
                if arguments.is_some() {
                    self.infer_instance_method_call_type(target, method)
                } else {
                    self.infer_property_access_type(target, method)
                }
            }

            Expression::ImportedFunctionCall { namespace_name, function_name, .. } => {
                self.infer_imported_function_call_type(namespace_name, function_name)
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

            Expression::ConfigAccess { .. } => None,

            Expression::ObjectAccess { path, .. } => {
                let joined   = path.join(".");
                let prefixed = format!("DATA.{}", joined);
                self.symbol_table.try_get_data_variable(&joined)
                    .or_else(|| self.symbol_table.try_get_data_variable(&prefixed))
                    .and_then(|v| v.effective_type())
            }

            Expression::PropertyAccess { object, property, .. } => {
                self.infer_property_access_type(object, property)
            }

            Expression::IndexAccess { object, .. } => {
                self.infer_index_access_type(object)
            }

            Expression::Parenthesized { expression, .. } => {
                self.infer_type_from_expression(expression)
            }

            Expression::Conditional { true_value, false_value, .. } => {
                self.infer_type_from_expression(true_value)
                    .or_else(|| self.infer_type_from_expression(false_value))
            }

            Expression::TypeCast { target_type, .. } => Some(*target_type),
        }
    }

    // ── Element-type API ──────────────────────────────────────────────────────

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
                if let Some(&hint) = self.element_type_hints.get(name.as_str()) {
                    return Some(hint);
                }
                if let Some(Some(var_type)) = self.local_variable_types.get(name.as_str()) {
                    if let DataType::TypedArray(elem) = *var_type {
                        return Some(elem.to_data_type());
                    }
                    if let DataType::TypedTuple(arr) = *var_type {
                        return arr[0].map(|e| e.to_data_type());
                    }
                }
                None
            }

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
        None
    }

    /// Core qualified-identifier type inference covering no-arg (property/enum
    /// access) and call forms (static + instance method calls).
    fn infer_qualified_identifier_type(
        &self,
        parts:     &[String],
        arguments: Option<&Vec<Expression>>,
    ) -> Option<DataType> {
        if parts.len() < 2 { return None; }

        let first_part  = &parts[0];
        let second_part = &parts[1];

        // ── No-argument forms ─────────────────────────────────────────────────

        if arguments.is_none() {
            if parts.len() == 2 && self.symbol_table.has_enum(first_part) {
                return Some(DataType::Enum);
            }
            if parts.len() == 3
                && self.symbol_table.is_imported_namespace(&parts[0])
                && self.symbol_table.get_namespaced_enum(&parts[0], &parts[1]).is_some()
            {
                return Some(DataType::Enum);
            }
            if parts.len() == 2 {
                let path      = format!("{}.{}", first_part, second_part);
                let data_path = format!("DATA.{}", path);
                if let Some(var) = self.symbol_table.try_get_data_variable(&path)
                    .or_else(|| self.symbol_table.try_get_data_variable(&data_path))
                {
                    return var.effective_type();
                }
            }
            return None;
        }

        // ── Call forms ────────────────────────────────────────────────────────

        let args = arguments.unwrap();

        // Static builtin: uppercase first letter → Math.sqrt(), DateTime.now()
        if parts.len() == 2
            && first_part.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
        {
            return self.infer_static_method_call_type_with_args(first_part, second_part, args);
        }

        if parts.len() == 2 {
            // Imported namespace function
            if self.symbol_table.is_imported_namespace(first_part) {
                if let Some(func_info) =
                    self.symbol_table.get_namespaced_function(first_part, second_part)
                {
                    return func_info.signature.return_type;
                }
            }

            // Instance method on a local variable with a known type.
            if let Some(Some(var_type)) = self.local_variable_types.get(first_part.as_str()) {
                let base_type = var_type.base_collection_type();

                // Object variables may carry lambda properties — skip registry
                // for Object to avoid conflating Object.add (builtin) with
                // user-defined add lambdas.  Specific well-typed Object methods
                // (keys, values, etc.) are still handled below before the guard.
                if base_type == DataType::Object {
                    match second_part.as_str() {
                        "keys"          => return Some(DataType::TypedArray(ElemType::String)),
                        "values"        => return Some(DataType::Array),
                        "entries"
                        | "toArray"     => return Some(DataType::Array),
                        "count"         => return Some(DataType::Int),
                        "has"
                        | "containsValue" => return Some(DataType::Bool),
                        _ => return None,
                    }
                }

                // ── Array element-returning ──────────────────────────────────
                if base_type == DataType::Array {
                    if ARRAY_ELEMENT_METHODS.contains(&second_part.as_str()) {
                        if let DataType::TypedArray(elem) = *var_type {
                            return Some(elem.to_data_type());
                        }
                        if let Some(&hint) = self.element_type_hints.get(first_part.as_str()) {
                            return Some(hint);
                        }
                        return None;
                    }

                    if ARRAY_PRESERVING_METHODS.contains(&second_part.as_str()) {
                        return match *var_type {
                            DataType::TypedArray(_) => Some(*var_type),
                            _                       => Some(DataType::Array),
                        };
                    }

                    if second_part == "flatten" { return Some(DataType::Array); }

                    // String.split inside an array chain — shouldn't happen
                    // but fall through to registry.
                }

                // ── Tuple element-returning ──────────────────────────────────
                if base_type == DataType::Tuple {
                    if let Some(slot) = Self::tuple_method_slot_index(second_part) {
                        if let DataType::TypedTuple(arr) = *var_type {
                            if slot < 6 {
                                if let Some(elem) = arr[slot] {
                                    return Some(elem.to_data_type());
                                }
                            }
                        }
                        return None;
                    }

                    if TUPLE_DYNAMIC_ELEMENT_METHODS.contains(&second_part.as_str()) {
                        if let DataType::TypedTuple(arr) = *var_type {
                            if let Some(elem) = arr.iter().flatten().next() {
                                return Some(elem.to_data_type());
                            }
                        }
                        return None;
                    }

                    if second_part == "toArray" {
                        if let DataType::TypedTuple(arr) = *var_type {
                            let defined: Vec<ElemType> = arr.iter().filter_map(|&e| e).collect();
                            if !defined.is_empty() && defined.iter().all(|&e| e == defined[0]) {
                                return Some(DataType::TypedArray(defined[0]));
                            }
                        }
                        return Some(DataType::Array);
                    }

                    if TUPLE_PRESERVING_METHODS.contains(&second_part.as_str()) {
                        return Some(DataType::Tuple);
                    }
                }

                // ── String methods returning typed arrays ────────────────────
                if base_type == DataType::String && second_part == "split" {
                    return Some(DataType::TypedArray(ElemType::String));
                }

                // ── Regex methods returning typed arrays ─────────────────────
                if base_type == DataType::Regex {
                    match second_part.as_str() {
                        "match" | "split" => {
                            return Some(DataType::TypedArray(ElemType::String));
                        }
                        "matchAll" => return Some(DataType::Array),
                        _ => {}
                    }
                }

                // ── Registry lookup for everything else ──────────────────────
                if let Some(dix_type) = Self::convert_data_type_to_dix_type(base_type) {
                    use crate::Builtins::Resolver::instance_method_registry;
                    instance_method_registry::initialize();
                    if let Some(method) = instance_method_registry::get_instance_method(
                        dix_type,
                        second_part.as_str(),
                    ) {
                        let ret = method.return_type();
                        if ret != DixType::Any && ret != DixType::Void && ret != DixType::Null {
                            return Self::convert_dix_type_to_data_type(ret);
                        }
                    }
                }
            }
        }

        None
    }

    fn infer_imported_function_call_type(
        &self,
        namespace_name: &str,
        function_name:  &str,
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
        let lt = self.infer_type_from_expression(left);
        let rt = self.infer_type_from_expression(right);

        if lt == Some(DataType::String) || rt == Some(DataType::String) {
            return Some(DataType::String);
        }

        match (lt, rt) {
            (Some(l), Some(r)) => {
                if l == DataType::Any && r == DataType::Any { return None; }
                if l == DataType::Any { return Some(r); }
                if r == DataType::Any { return Some(l); }
                if Self::is_numeric_type(l) && Self::is_numeric_type(r) {
                    if l == DataType::Double || r == DataType::Double { return Some(DataType::Double); }
                    if l == DataType::Float  || r == DataType::Float  { return Some(DataType::Float);  }
                    if l == DataType::Long   || r == DataType::Long   { return Some(DataType::Long);   }
                    return Some(DataType::Int);
                }
                if l == r { Some(l) } else { Some(l) }
            }
            (Some(l), None) => Some(l),
            (None, Some(r)) => Some(r),
            (None, None)    => None,
        }
    }

    #[inline]
    fn is_numeric_type(dt: DataType) -> bool {
        matches!(dt, DataType::Int | DataType::Long | DataType::Float | DataType::Double)
    }

    fn infer_prefixed_constructor_type(&self, prefix: &str, arguments: &[Value]) -> Option<DataType> {
        match prefix.to_lowercase().as_str() {
            "t" => {
                // Try to build TypedTuple from element types.
                let mut arr: [Option<ElemType>; 6] = [None; 6];
                let mut any_known = false;
                for (i, arg) in arguments.iter().enumerate().take(6) {
                    if let Some(dt) = self.infer_type_from_value(arg) {
                        if let Some(et) = ElemType::from_data_type(dt) {
                            arr[i]    = Some(et);
                            any_known = true;
                        }
                    }
                }
                if any_known { Some(DataType::TypedTuple(arr)) } else { Some(DataType::Tuple) }
            }
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

    fn infer_dix_function_call_type(&self, function_name: &str) -> Option<DataType> {
        match function_name {
            "Format" | "Join" => Some(DataType::String),
            _ => None,
        }
    }

    // ── Static method inference with argument types ───────────────────────────

    /// Infer static method return type, using argument types for the builtins
    /// whose result depends on what they receive.
    ///
    /// Derived directly from `array_object.rs` and `random_object.rs`.
    fn infer_static_method_call_type_with_args(
        &self,
        object_name: &str,
        method_name: &str,
        arguments:   &[Expression],
    ) -> Option<DataType> {
        match (object_name, method_name) {
            // ── Random ────────────────────────────────────────────────────────
            // choice(arr) / weighted(vals, weights) → element type of the array
            ("Random", "choice") | ("Random", "weighted") => {
                if let Some(first_arg) = arguments.first() {
                    if let Some(elem) = self.infer_element_type_from_expression(first_arg) {
                        return Some(elem);
                    }
                }
                // Registered as DixType::Any → None via registry fallback.
            }

            // choices(arr, n) / sample(arr, n) → TypedArray preserving element type
            ("Random", "choices") | ("Random", "sample") => {
                if let Some(first_arg) = arguments.first() {
                    if let Some(elem) = self.infer_element_type_from_expression(first_arg) {
                        if let Some(et) = ElemType::from_data_type(elem) {
                            return Some(DataType::TypedArray(et));
                        }
                    }
                }
                return Some(DataType::Array);
            }

            // ── Array constructors ────────────────────────────────────────────

            // range(start, end) → always array of ints
            ("Array", "range") => return Some(DataType::TypedArray(ElemType::Int)),

            // fromString(text, sep) → always array of strings
            ("Array", "fromString") => return Some(DataType::TypedArray(ElemType::String)),

            // empty() → plain Array (no element type)
            ("Array", "empty") => return Some(DataType::Array),

            // fill(value, count) → TypedArray(T) where T is value's type
            ("Array", "fill") => {
                if let Some(val_arg) = arguments.first() {
                    if let Some(elem_dt) = self.infer_type_from_expression(val_arg) {
                        if let Some(et) = ElemType::from_data_type(elem_dt) {
                            return Some(DataType::TypedArray(et));
                        }
                    }
                }
                return Some(DataType::Array);
            }

            // of(v1, v2, ...) → TypedArray(T) when all values share type T
            ("Array", "of") => {
                if let Some(first_arg) = arguments.first() {
                    if let Some(elem_dt) = self.infer_type_from_expression(first_arg) {
                        if let Some(et) = ElemType::from_data_type(elem_dt) {
                            // Verify remaining args share the same type
                            let all_match = arguments[1..].iter().all(|arg| {
                                self.infer_type_from_expression(arg)
                                    .and_then(ElemType::from_data_type)
                                    .map(|e| e == et)
                                    .unwrap_or(true) // unknown = tolerate
                            });
                            if all_match {
                                return Some(DataType::TypedArray(et));
                            }
                        }
                    }
                }
                return Some(DataType::Array);
            }

            // ── Array transforms (array arg → same typed array) ───────────────

            // repeat(array, times) → same element type as input array
            ("Array", "repeat") => {
                if let Some(first_arg) = arguments.first() {
                    if let Some(arr_type) = self.infer_type_from_expression(first_arg) {
                        if let DataType::TypedArray(_) = arr_type {
                            return Some(arr_type);
                        }
                    }
                }
                return Some(DataType::Array);
            }

            // reverse(array), sort(array), unique(array),
            // slice(array, start, end), filter(array, value)
            // → same element type as input array
            ("Array", "reverse")
            | ("Array", "sort")
            | ("Array", "unique")
            | ("Array", "slice")
            | ("Array", "filter") => {
                if let Some(first_arg) = arguments.first() {
                    if let Some(arr_type) = self.infer_type_from_expression(first_arg) {
                        if let DataType::TypedArray(_) = arr_type {
                            return Some(arr_type);
                        }
                    }
                }
                return Some(DataType::Array);
            }

            // concat(arr1, arr2, ...) → TypedArray if all inputs have same element type
            ("Array", "concat") => {
                // Collect the element type of the first typed-array argument.
                let first_typed = arguments.iter().find_map(|arg| {
                    if let Some(DataType::TypedArray(et)) = self.infer_type_from_expression(arg) {
                        Some(et)
                    } else {
                        None
                    }
                });
                if let Some(et) = first_typed {
                    let all_same = arguments.iter().all(|arg| {
                        match self.infer_type_from_expression(arg) {
                            Some(DataType::TypedArray(e)) => e == et,
                            Some(DataType::Array)         => true, // untyped — tolerate
                            None                          => true, // unknown — tolerate
                            _                             => false,
                        }
                    });
                    if all_same {
                        return Some(DataType::TypedArray(et));
                    }
                }
                return Some(DataType::Array);
            }

            // flatten(array) → always plain Array (nesting destroyed)
            ("Array", "flatten") => return Some(DataType::Array),

            _ => {}
        }

        // General registry fallback for everything else.
        self.infer_static_method_call_return_type(object_name, method_name)
    }

    fn infer_static_method_call_return_type(
        &self,
        object_name: &str,
        method_name: &str,
    ) -> Option<DataType> {
        use crate::Builtins::Resolver::static_object_registry;
        static_object_registry::initialize_static_registry();
        static_object_registry::get_method_info(object_name, method_name)
            .and_then(|info| Self::convert_dix_type_to_data_type(info.return_type))
    }

    // ── Instance method inference ─────────────────────────────────────────────

    /// The authoritative inference path for explicit instance method calls.
    /// Covers every category of collection method from the builtin source files.
    fn infer_instance_method_call_type(
        &self,
        instance:    &Expression,
        method_name: &str,
    ) -> Option<DataType> {
        let instance_data_type = self.infer_type_from_expression(instance)?;
        let base_type          = instance_data_type.base_collection_type();

        // ── Array: element-returning ──────────────────────────────────────────
        // Only first / last / get truly return an element.
        // pop / shift return a new Array (DixScript is functional/immutable).
        if base_type == DataType::Array && ARRAY_ELEMENT_METHODS.contains(&method_name) {
            if let DataType::TypedArray(elem) = instance_data_type {
                return Some(elem.to_data_type());
            }
            return self.infer_element_type_from_expression(instance);
        }

        // ── Array: preserving (returns new array of same element type) ────────
        if base_type == DataType::Array && ARRAY_PRESERVING_METHODS.contains(&method_name) {
            return match instance_data_type {
                DataType::TypedArray(_) => Some(instance_data_type),
                _                       => Some(DataType::Array),
            };
        }

        // ── Array: flatten always strips TypedArray nesting ───────────────────
        if base_type == DataType::Array && method_name == "flatten" {
            return Some(DataType::Array);
        }

        // ── Tuple: positional element-returning ───────────────────────────────
        // first() → slot 0, second() → slot 1, …, sixth() → slot 5
        if base_type == DataType::Tuple {
            if let Some(slot) = Self::tuple_method_slot_index(method_name) {
                if let DataType::TypedTuple(arr) = instance_data_type {
                    if slot < 6 {
                        if let Some(elem) = arr[slot] {
                            return Some(elem.to_data_type());
                        }
                    }
                }
                // No TypedTuple annotation — fall back to element inference.
                return self.infer_element_type_from_expression(instance);
            }

            // ── Tuple: get(i) — index not known statically ───────────────────
            if TUPLE_DYNAMIC_ELEMENT_METHODS.contains(&method_name) {
                if let DataType::TypedTuple(arr) = instance_data_type {
                    // Use the first defined slot as best approximation.
                    if let Some(elem) = arr.iter().flatten().next() {
                        return Some(elem.to_data_type());
                    }
                }
                return self.infer_element_type_from_expression(instance);
            }

            // ── Tuple: toArray ────────────────────────────────────────────────
            // When all slots share one element type → TypedArray(T).
            if method_name == "toArray" {
                if let DataType::TypedTuple(arr) = instance_data_type {
                    let defined: Vec<ElemType> = arr.iter().filter_map(|&e| e).collect();
                    if !defined.is_empty() && defined.iter().all(|&e| e == defined[0]) {
                        return Some(DataType::TypedArray(defined[0]));
                    }
                }
                return Some(DataType::Array);
            }

            // ── Tuple: reverse / swap — positions change, drop TypedTuple ─────
            if TUPLE_PRESERVING_METHODS.contains(&method_name) {
                return Some(DataType::Tuple);
            }
        }

        // ── String: split → always TypedArray(String) ─────────────────────────
        if base_type == DataType::String && method_name == "split" {
            return Some(DataType::TypedArray(ElemType::String));
        }

        // ── Regex: collection-returning methods ───────────────────────────────
        // match(str)    → [full_match, group1, group2, …] — all strings
        // split(str)    → all parts are strings
        // matchAll(str) → array of per-match arrays (too complex to fully type)
        if base_type == DataType::Regex {
            match method_name {
                "match" | "split" => return Some(DataType::TypedArray(ElemType::String)),
                "matchAll"        => return Some(DataType::Array),
                _ => {}
            }
        }

        // ── Object: well-typed collection methods ─────────────────────────────
        // We skip the generic registry lookup for Object (see the QualifiedIdentifier
        // comment), but these specific methods have known return types.
        if base_type == DataType::Object {
            match method_name {
                "keys"           => return Some(DataType::TypedArray(ElemType::String)),
                "values"         => return Some(DataType::Array),
                "entries"
                | "toArray"      => return Some(DataType::Array), // array of [key,val] tuples
                _ => {}
                // For add, set, remove, merge, get, has, count, containsValue
                // fall through to registry — we're in InstanceMethodCall context
                // so the parser already resolved this as a method, not a lambda property.
            }
        }

        // ── General registry lookup ───────────────────────────────────────────
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

    // ── Path / index helpers ──────────────────────────────────────────────────

    fn infer_property_access_type(&self, object: &Expression, property: &str) -> Option<DataType> {
        if let Some(base) = Self::build_property_path(object) {
            let full      = format!("{}.{}", base, property);
            let full_data = format!("DATA.{}", full);
            if let Some(var) = self.symbol_table.try_get_data_variable(&full)
                .or_else(|| self.symbol_table.try_get_data_variable(&full_data))
            {
                return var.effective_type();
            }
        }
        None
    }

    fn infer_index_access_type(&self, object: &Expression) -> Option<DataType> {
        let obj_type = self.infer_type_from_expression(object)?;
        match obj_type {
            // TypedArray: element type is authoritative.
            DataType::TypedArray(elem) => Some(elem.to_data_type()),
            // TypedTuple: use slot 0 as the best static approximation.
            // Per-index precision requires constant-index analysis (deferred).
            DataType::TypedTuple(arr)  => arr[0].map(|e| e.to_data_type()),
            // Plain collections: fall back to value-level element inference.
            DataType::Array | DataType::Tuple => {
                self.infer_element_type_from_expression(object)
            }
            // String character access.
            DataType::String => Some(DataType::String),
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

    // ── Collection helpers ────────────────────────────────────────────────────

    /// Attempt to produce `TypedArray(T)` when all elements share type `T`.
    /// Returns `None` (not `Some(Array)`) on failure so callers can apply
    /// their own fallback.
    fn try_infer_typed_array_from_values(&self, values: &[Value]) -> Option<DataType> {
        if values.is_empty() { return None; }
        let first_elem = self.infer_type_from_value(&values[0])?;
        for v in values.iter().skip(1) {
            match self.infer_type_from_value(v) {
                Some(t) if t == first_elem => {}
                _ => return None,
            }
        }
        ElemType::from_data_type(first_elem).map(DataType::TypedArray)
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

    /// Returns the 0-based TypedTuple slot index for a positional tuple accessor.
    /// Returns `None` for non-positional methods (`get`, etc.).
    #[inline]
    fn tuple_method_slot_index(method_name: &str) -> Option<usize> {
        match method_name {
            "first"  => Some(0),
            "second" => Some(1),
            "third"  => Some(2),
            "fourth" => Some(3),
            "fifth"  => Some(4),
            "sixth"  => Some(5),
            _        => None,
        }
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
            DataType::TypedArray(_) => Some(DixType::Array),
            DataType::TypedTuple(_) => Some(DixType::Tuple),
            DataType::Any | DataType::Function | DataType::Range => None,
        }
    }
}
