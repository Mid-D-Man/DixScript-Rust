// dixscript/src/Compiler/AST/Visitors/type_inference_visitor.rs

use crate::Compiler::AST::*;
use crate::Compiler::Utilities::SymbolTable;
use crate::Builtins::Core::DixType;
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// Method-classification tables
// ─────────────────────────────────────────────────────────────────────────────

/// Array instance methods that return the **element** type rather than a fixed
/// return type.  When the receiver is `TypedArray(ElemType::T)` these return `T`.
const ARRAY_ELEMENT_METHODS: &[&str] = &[
    "first", "last", "get", "at", "pop", "random", "shift",
];

/// Tuple positional accessors whose return type mirrors the stored element type.
const TUPLE_ELEMENT_METHODS: &[&str] = &[
    "first", "second", "third", "fourth", "fifth", "sixth", "get", "at",
];

/// Array instance methods that return a **new array of the same element type**.
/// When the receiver is `TypedArray(T)` these also return `TypedArray(T)`.
const ARRAY_PRESERVING_METHODS: &[&str] = &[
    "reverse", "sort", "distinct", "slice", "concat", "push", "unshift",
];

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

            // Attempt to produce TypedArray(T) when all elements share type T.
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

            Value::Expression { expr, .. } => self.infer_type_from_expression(expr),

            Value::Lambda { .. } => Some(DataType::Function),
            Value::Range { .. }  => Some(DataType::Range),

            Value::Identifier { value: name, .. } => self.infer_identifier_type(name),

            // ParseError / Error / Unknown — no meaningful type
            _ => None,
        }
    }

    /// Exhaustive match over every `Expression` variant.
    /// Every variant is handled explicitly — no `_ => None` escape hatch.
    pub fn infer_type_from_expression(&self, expr: &Expression) -> Option<DataType> {
        match expr {
            Expression::Value { value, .. } => self.infer_type_from_value(value),

            Expression::Identifier { name, .. } => self.infer_identifier_type(name),

            Expression::QualifiedIdentifier { parts, arguments, .. } => {
                self.infer_qualified_identifier_type(parts, arguments.as_ref())
            }

            // General function call — look up in QuickFuncs / user functions table.
            Expression::FunctionCall { name, .. } => self.infer_function_call_type(name),

            // DixScript QuickFunc call: ~myFunc(args)
            Expression::QuickFuncCall { name, .. } => self.infer_quick_func_call_type(name),

            // Dix.Log / Dix.Format / Dix.Join etc.
            Expression::DixFunctionCall { function_name, .. } => {
                self.infer_dix_function_call_type(function_name)
            }

            // Explicit static call node: Math.sqrt(x)
            Expression::StaticMethodCall { object_name, method_name, arguments, .. } => {
                self.infer_static_method_call_type_with_args(object_name, method_name, arguments)
            }

            // Alternative static call representation used by some parser paths.
            Expression::StaticFunction { class_name, method, arguments, .. } => {
                self.infer_static_method_call_type_with_args(class_name, method, arguments)
            }

            // Explicit instance call node: myArr.first()
            Expression::InstanceMethodCall { instance, method_name, .. } => {
                self.infer_instance_method_call_type(instance, method_name)
            }

            // Built-in method/property access: target.method(args?) or target.prop
            Expression::BuiltinFunction { target, method, arguments, .. } => {
                if arguments.is_some() {
                    self.infer_instance_method_call_type(target, method)
                } else {
                    self.infer_property_access_type(target, method)
                }
            }

            // Imported namespace function call: utils.computeTax(x)
            Expression::ImportedFunctionCall { namespace_name, function_name, .. } => {
                self.infer_imported_function_call_type(namespace_name, function_name)
            }

            // Operators
            Expression::ArithmeticOp { left, right, .. } => {
                self.infer_arithmetic_op_type(left, right)
            }
            Expression::ComparisonOp { .. } => Some(DataType::Bool),
            Expression::LogicalOp { .. }    => Some(DataType::Bool),
            Expression::BitwiseOp { .. }    => Some(DataType::Int),

            Expression::UnaryOp { operator, operand, .. } => {
                self.infer_unary_op_type(operator, operand)
            }

            // Enum / config / object access
            Expression::EnumAccess { .. } => Some(DataType::Enum),

            Expression::ConfigAccess { .. } => {
                // Config values carry varied primitive types; returning None avoids
                // false type-mismatch errors downstream.
                None
            }

            Expression::ObjectAccess { path, .. } => {
                // Try the dot-joined path with and without the DATA. prefix.
                let joined    = path.join(".");
                let prefixed  = format!("DATA.{}", joined);
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

            // TypeCast is Copy, so dereferencing target_type is fine for all variants.
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
                // 1. Explicit element-type hint (populated by scope tracker for TypedArray declarations).
                if let Some(&hint) = self.element_type_hints.get(name.as_str()) {
                    return Some(hint);
                }
                // 2. TypedArray / TypedTuple annotation in local variable map.
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

            // Recurse through chained property access: data.items  ->  data
            Expression::PropertyAccess { object, .. } => {
                self.infer_element_type_from_expression(object)
            }

            _ => None,
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn infer_identifier_type(&self, name: &str) -> Option<DataType> {
        // Local variable / parameter with known type takes priority.
        if let Some(local_type) = self.local_variable_types.get(name) {
            return *local_type;
        }
        // Bare enum name used as a type reference.
        if self.symbol_table.has_enum(name) {
            return Some(DataType::Enum);
        }
        // Static objects, imported namespaces, and functions have no value type.
        None
    }

    /// Core qualified-identifier type inference.
    ///
    /// Handles three broad shapes:
    ///   - No-arg: enum / property access      `Status.ACTIVE`, `server.host`
    ///   - 2-part call with uppercase prefix:  `Math.sqrt(x)`
    ///   - 2-part call with lowercase prefix:  `myArr.first()`, `utils.fn()`
    fn infer_qualified_identifier_type(
        &self,
        parts:     &[String],
        arguments: Option<&Vec<Expression>>,
    ) -> Option<DataType> {
        if parts.len() < 2 {
            return None;
        }

        let first_part  = &parts[0];
        let second_part = &parts[1];

        // ── No-argument forms (property / enum access) ────────────────────────

        if arguments.is_none() {
            // Local enum: Status.ACTIVE (2 parts)
            if parts.len() == 2 && self.symbol_table.has_enum(first_part) {
                return Some(DataType::Enum);
            }

            // Imported namespace enum: utils.Status.ACTIVE (3 parts)
            if parts.len() == 3
                && self.symbol_table.is_imported_namespace(&parts[0])
                && self.symbol_table.get_namespaced_enum(&parts[0], &parts[1]).is_some()
            {
                return Some(DataType::Enum);
            }

            // DATA property path: server.host (2 parts, no call)
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

        // ── Call forms (arguments.is_some()) ─────────────────────────────────

        let args = arguments.unwrap(); // safe: checked above

        // Static builtin call: uppercase first letter → Math.sqrt(), DateTime.now()
        if parts.len() == 2
            && first_part.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
        {
            return self.infer_static_method_call_type_with_args(first_part, second_part, args);
        }

        if parts.len() == 2 {
            // Imported namespace function call: utils.computeTax()
            if self.symbol_table.is_imported_namespace(first_part) {
                if let Some(func_info) =
                    self.symbol_table.get_namespaced_function(first_part, second_part)
                {
                    return func_info.signature.return_type;
                }
            }

            // Instance method call on a local variable with a known type.
            //
            // IMPORTANT: Object type is excluded from the registry lookup here.
            // Object variables may carry lambda functions as properties (e.g.
            // `calculator.add` = `(a,b) => a+b`).  The built-in Object method
            // registry has its own `add(key, value) → Object` that would produce
            // a false `DataType::Object` result and fire spurious QFUNC015 errors.
            // Returning None for Object method calls defers type checking to runtime.
            if let Some(Some(var_type)) = self.local_variable_types.get(first_part.as_str()) {
                let base_type = var_type.base_collection_type();

                if base_type != DataType::Object {
                    // ── 1. Element-returning methods on Array ────────────────
                    if base_type == DataType::Array {
                        if ARRAY_ELEMENT_METHODS.contains(&second_part.as_str()) {
                            // TypedArray annotation is authoritative.
                            if let DataType::TypedArray(elem) = *var_type {
                                return Some(elem.to_data_type());
                            }
                            // Fall back to explicit element hint (e.g. from scope tracker).
                            if let Some(&hint) = self.element_type_hints.get(first_part.as_str()) {
                                return Some(hint);
                            }
                            // Element type is unknown — return None rather than guess.
                            return None;
                        }

                        // Methods that return a new array of the same element type.
                        if ARRAY_PRESERVING_METHODS.contains(&second_part.as_str()) {
                            return match *var_type {
                                DataType::TypedArray(_) => Some(*var_type),
                                _                       => Some(DataType::Array),
                            };
                        }
                    }

                    // ── 2. Element-returning methods on Tuple ────────────────
                    if base_type == DataType::Tuple
                        && TUPLE_ELEMENT_METHODS.contains(&second_part.as_str())
                    {
                        if let DataType::TypedTuple(arr) = *var_type {
                            if let Some(first_elem) = arr[0] {
                                return Some(first_elem.to_data_type());
                            }
                        }
                        return None;
                    }

                    // ── 3. General registry lookup for everything else ────────
                    if let Some(dix_type) = Self::convert_data_type_to_dix_type(base_type) {
                        use crate::Builtins::Resolver::instance_method_registry;
                        instance_method_registry::initialize();
                        if let Some(method) = instance_method_registry::get_instance_method(
                            dix_type,
                            second_part.as_str(),
                        ) {
                            let ret = method.return_type();
                            // Void / Null / Any cannot be meaningfully propagated upward.
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

        // String concatenation wins over everything.
        if lt == Some(DataType::String) || rt == Some(DataType::String) {
            return Some(DataType::String);
        }

        match (lt, rt) {
            (Some(l), Some(r)) => {
                // Any + Any or unknown operand — defer.
                if l == DataType::Any && r == DataType::Any { return None; }
                if l == DataType::Any { return Some(r); }
                if r == DataType::Any { return Some(l); }

                if Self::is_numeric_type(l) && Self::is_numeric_type(r) {
                    // Numeric type promotion: Double > Float > Long > Int.
                    if l == DataType::Double || r == DataType::Double { return Some(DataType::Double); }
                    if l == DataType::Float  || r == DataType::Float  { return Some(DataType::Float);  }
                    if l == DataType::Long   || r == DataType::Long   { return Some(DataType::Long);   }
                    return Some(DataType::Int);
                }

                // Same non-numeric type (e.g. Array + Array for concat).
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

    /// Infer the type produced by a prefixed constructor expression.
    ///
    /// For `t:(v1, v2, ...)` we attempt to build a `TypedTuple` from the
    /// element types.  For `b:(...)` → Blob, `r:(...)` → Regex.
    fn infer_prefixed_constructor_type(&self, prefix: &str, arguments: &[Value]) -> Option<DataType> {
        match prefix.to_lowercase().as_str() {
            "t" => {
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
                if any_known {
                    Some(DataType::TypedTuple(arr))
                } else {
                    Some(DataType::Tuple)
                }
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
            // Unary + / - preserves numeric type; prefix ++ / -- same.
            _ => self.infer_type_from_expression(operand),
        }
    }

    fn infer_quick_func_call_type(&self, name: &str) -> Option<DataType> {
        self.symbol_table
            .try_get_function(name)
            .and_then(|sig| sig.return_type)
    }

    /// Return types for Dix.* built-in utility functions.
    fn infer_dix_function_call_type(&self, function_name: &str) -> Option<DataType> {
        match function_name {
            "Format" | "Join" => Some(DataType::String),
            // Log*, Assert, Trace, Print* are side-effectful void calls.
            _ => None,
        }
    }

    /// Type inference for static method calls **with argument information**.
    ///
    /// Several built-in static methods return a type that depends on their
    /// arguments (e.g. `Random.choice(arr)` returns the element type of `arr`).
    /// This method handles those special cases before falling back to the
    /// compile-time registry.
    fn infer_static_method_call_type_with_args(
        &self,
        object_name: &str,
        method_name: &str,
        arguments:   &[Expression],
    ) -> Option<DataType> {
        match (object_name, method_name) {
            // Random.choice(arr) / Random.weighted(vals, weights)
            // → element type of the first array argument.
            ("Random", "choice") | ("Random", "weighted") => {
                if let Some(first_arg) = arguments.first() {
                    if let Some(elem) = self.infer_element_type_from_expression(first_arg) {
                        return Some(elem);
                    }
                }
                // Registered as DixType::Any in the runtime registry; registry
                // fallback will return None — correct here since we couldn't infer.
            }

            // Random.choices(arr, n) / Random.sample(arr, n)
            // → TypedArray preserving the element type of the source array.
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

            // Array.range(start, end) is always an array of integers.
            ("Array", "range") => return Some(DataType::TypedArray(ElemType::Int)),

            // Array.fill(value, count) → TypedArray typed to the fill value.
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

            // Array.of(v1, v2, ...) → TypedArray typed to the first element.
            ("Array", "of") => {
                if let Some(first_arg) = arguments.first() {
                    if let Some(elem_dt) = self.infer_type_from_expression(first_arg) {
                        if let Some(et) = ElemType::from_data_type(elem_dt) {
                            return Some(DataType::TypedArray(et));
                        }
                    }
                }
                return Some(DataType::Array);
            }

            // Array static methods that take an array and return same-typed array.
            ("Array", "filter")
            | ("Array", "reverse")
            | ("Array", "sort")
            | ("Array", "unique")
            | ("Array", "slice")
            | ("Array", "concat") => {
                if let Some(first_arg) = arguments.first() {
                    if let Some(arr_type) = self.infer_type_from_expression(first_arg) {
                        if let DataType::TypedArray(_) = arr_type {
                            return Some(arr_type);
                        }
                    }
                }
                return Some(DataType::Array);
            }

            _ => {}
        }

        // General registry fallback.
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

    /// Infer the return type of an instance method call.
    ///
    /// Precedence:
    ///   1. Element-returning methods on `TypedArray` / `TypedTuple` → element type.
    ///   2. Array-preserving methods on `TypedArray` → same `TypedArray(T)`.
    ///   3. General registry lookup; `Any` / `Void` / `Null` are suppressed to `None`.
    fn infer_instance_method_call_type(
        &self,
        instance:    &Expression,
        method_name: &str,
    ) -> Option<DataType> {
        let instance_data_type = self.infer_type_from_expression(instance)?;
        let base_type          = instance_data_type.base_collection_type();

        // ── Array element-returning methods ───────────────────────────────────
        if base_type == DataType::Array && ARRAY_ELEMENT_METHODS.contains(&method_name) {
            if let DataType::TypedArray(elem) = instance_data_type {
                return Some(elem.to_data_type());
            }
            // Try element_type_hints / value inference as a fallback.
            return self.infer_element_type_from_expression(instance);
        }

        // ── Array-preserving methods ──────────────────────────────────────────
        if base_type == DataType::Array && ARRAY_PRESERVING_METHODS.contains(&method_name) {
            return match instance_data_type {
                DataType::TypedArray(_) => Some(instance_data_type),
                _                       => Some(DataType::Array),
            };
        }

        // ── Tuple element-returning methods ───────────────────────────────────
        if base_type == DataType::Tuple && TUPLE_ELEMENT_METHODS.contains(&method_name) {
            if let DataType::TypedTuple(arr) = instance_data_type {
                if let Some(first_elem) = arr[0] {
                    return Some(first_elem.to_data_type());
                }
            }
            return self.infer_element_type_from_expression(instance);
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

    /// Infer the element type produced by a subscript / index expression.
    fn infer_index_access_type(&self, object: &Expression) -> Option<DataType> {
        let obj_type = self.infer_type_from_expression(object)?;
        match obj_type {
            // Typed annotation is authoritative — return the declared element type.
            DataType::TypedArray(elem)       => Some(elem.to_data_type()),
            DataType::TypedTuple(arr)        => arr[0].map(|e| e.to_data_type()),
            // For plain collections fall back to runtime inference.
            DataType::Array | DataType::Tuple => {
                self.infer_element_type_from_expression(object)
            }
            // String[i] yields a single character (represented as String).
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

    /// Attempt to produce `TypedArray(T)` when all elements share element type `T`.
    ///
    /// Returns `None` — not `Some(DataType::Array)` — when the array is empty,
    /// types are mixed, or the element type cannot be determined.  The caller is
    /// responsible for falling back to plain `Array`.
    fn try_infer_typed_array_from_values(&self, values: &[Value]) -> Option<DataType> {
        if values.is_empty() {
            return None;
        }
        let first_elem_type = self.infer_type_from_value(&values[0])?;
        for v in values.iter().skip(1) {
            match self.infer_type_from_value(v) {
                Some(t) if t == first_elem_type => {}
                _ => return None, // mixed or unknown types
            }
        }
        ElemType::from_data_type(first_elem_type).map(DataType::TypedArray)
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
            // Typed collections strip down to their base DixType for registry lookup.
            DataType::TypedArray(_) => Some(DixType::Array),
            DataType::TypedTuple(_) => Some(DixType::Tuple),
            // No DixType counterpart.
            DataType::Any | DataType::Function | DataType::Range => None,
        }
    }
                }
