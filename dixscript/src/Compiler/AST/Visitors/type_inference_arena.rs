// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/Visitors/type_inference_arena.rs"
// ============================================================================
//! Type inference over the arena AST.
//!
//! [`TypeInferenceVisitor`] infers the type of a value or an expression from the symbol table
//! and from the types of the local variables it was given. This file holds the implementation:
//! it reads the nodes of an [`Arena`] through four entry points, named with an `_id` suffix:
//!
//! * [`infer_type_from_value_id`](TypeInferenceVisitor::infer_type_from_value_id)
//! * [`infer_type_from_expression_id`](TypeInferenceVisitor::infer_type_from_expression_id)
//! * [`infer_element_type_from_value_id`](TypeInferenceVisitor::infer_element_type_from_value_id)
//! * [`infer_element_type_from_expression_id`](TypeInferenceVisitor::infer_element_type_from_expression_id)
//!
//! The owned entry points in `type_inference_visitor.rs` (`infer_type_from_value` and the three
//! others) copy their argument into a scratch arena and call these, so there is one implementation
//! of the walk. Everything that depends on names alone (the identifier lookup, the function return
//! types, the method tables and the builtin registries) is in `type_inference_visitor.rs` and
//! shared. A recorded test pins the answers to the ones the owned walker gave before it was removed.

use super::type_inference_visitor::{
    TypeInferenceVisitor, ARRAY_ELEMENT_METHODS, ARRAY_PRESERVING_METHODS,
    TUPLE_DYNAMIC_ELEMENT_METHODS, TUPLE_PRESERVING_METHODS,
};
use crate::Builtins::Core::DixType;
use crate::Compiler::AST::arena::{Arena, ExprId, ExprList, ExprNode, StrList, ValueId, ValueList, ValueNode};
use crate::Compiler::AST::{DataType, ElemType};

impl<'a> TypeInferenceVisitor<'a> {
    // ── Public inference API ──────────────────────────────────────────────────

    /// Infers the type of the value `id` names in `arena`.
    ///
    /// This is [`infer_type_from_value`](TypeInferenceVisitor::infer_type_from_value) for the
    /// arena form.
    ///
    /// # Panics
    ///
    /// Panics if `id` (or an id inside the node) is out of range for `arena`, for example when
    /// `arena` is not the one the id was allocated in.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, ValueNode};
    /// use dixscript::Compiler::AST::{DataType, Position};
    /// use dixscript::Compiler::AST::Visitors::TypeInferenceVisitor;
    /// use dixscript::Compiler::Utilities::SymbolTable;
    ///
    /// let mut arena = Arena::new();
    /// let pos = Position::new(1, 1);
    /// let one = arena.alloc_value(ValueNode::Integer { value: 1, position: pos });
    /// let two = arena.alloc_value(ValueNode::Integer { value: 2, position: pos });
    /// let list = arena.alloc_value_list(&[one, two]);
    /// let array = arena.alloc_value(ValueNode::Array { values: list, position: pos });
    ///
    /// let table = SymbolTable::new();
    /// let visitor = TypeInferenceVisitor::new(&table, None);
    /// assert_eq!(visitor.infer_type_from_value_id(&arena, one), Some(DataType::Int));
    /// // An array whose items are all integers is a typed array.
    /// assert!(matches!(
    ///     visitor.infer_type_from_value_id(&arena, array),
    ///     Some(DataType::TypedArray(_))
    /// ));
    /// ```
    pub fn infer_type_from_value_id(&self, arena: &Arena, id: ValueId) -> Option<DataType> {
        match arena.value(id) {
            ValueNode::Integer { .. }            => Some(DataType::Int),
            ValueNode::Long { .. }               => Some(DataType::Long),
            ValueNode::Float { .. }              => Some(DataType::Float),
            ValueNode::Double { .. }             => Some(DataType::Double),
            ValueNode::ScientificNotation { .. } => Some(DataType::Double),
            ValueNode::String { .. }             => Some(DataType::String),
            ValueNode::InterpolatedString { .. } => Some(DataType::String),
            ValueNode::Boolean { .. }            => Some(DataType::Bool),
            ValueNode::HexColor { .. }           => Some(DataType::Hex),
            ValueNode::Date { .. }               => Some(DataType::Date),
            ValueNode::Timestamp { .. }          => Some(DataType::Timestamp),
            ValueNode::Null { .. }               => None,

            ValueNode::Array { values, .. } | ValueNode::NestedArray { values, .. } => {
                self.try_infer_typed_array_from_value_ids(arena, *values)
                    .or(Some(DataType::Array))
            }

            ValueNode::Object { .. } => Some(DataType::Object),

            ValueNode::PrefixedConstructor { prefix, arguments, .. } => {
                self.infer_prefixed_constructor_type_ids(arena, arena.text(*prefix), *arguments)
            }

            ValueNode::EnumValue { .. } => Some(DataType::Enum),

            ValueNode::QuickFuncCall { function_name, .. } => {
                self.infer_function_call_type(arena.text(*function_name))
            }

            ValueNode::Expression { expr, .. } => self.infer_type_from_expression_id(arena, *expr),
            ValueNode::Lambda { .. }           => Some(DataType::Function),
            ValueNode::Range { .. }            => Some(DataType::Range),
            ValueNode::Identifier { value: name, .. } => self.infer_identifier_type(arena.text(*name)),
            _ => None,
        }
    }

    /// Infers the type of the expression `id` names in `arena`.
    ///
    /// This is [`infer_type_from_expression`](TypeInferenceVisitor::infer_type_from_expression)
    /// for the arena form. The panics are those of
    /// [`infer_type_from_value_id`](Self::infer_type_from_value_id).
    pub fn infer_type_from_expression_id(&self, arena: &Arena, id: ExprId) -> Option<DataType> {
        match arena.expr(id) {
            ExprNode::Value { value, .. } => self.infer_type_from_value_id(arena, *value),

            ExprNode::Identifier { name, .. } => self.infer_identifier_type(arena.text(*name)),

            ExprNode::QualifiedIdentifier { parts, arguments, .. } => {
                self.infer_qualified_identifier_type_ids(arena, *parts, *arguments)
            }

            ExprNode::FunctionCall { name, .. } => self.infer_function_call_type(arena.text(*name)),

            ExprNode::QuickFuncCall { name, .. } => self.infer_quick_func_call_type(arena.text(*name)),

            ExprNode::DixFunctionCall { function_name, .. } => {
                self.infer_dix_function_call_type(arena.text(*function_name))
            }

            ExprNode::StaticMethodCall { object_name, method_name, arguments, .. } => {
                self.infer_static_method_call_type_with_arg_ids(
                    arena,
                    arena.text(*object_name),
                    arena.text(*method_name),
                    *arguments,
                )
            }

            ExprNode::StaticFunction { class_name, method, arguments, .. } => {
                self.infer_static_method_call_type_with_arg_ids(
                    arena,
                    arena.text(*class_name),
                    arena.text(*method),
                    *arguments,
                )
            }

            ExprNode::InstanceMethodCall { instance, method_name, .. } => {
                self.infer_instance_method_call_type_id(arena, *instance, arena.text(*method_name))
            }

            ExprNode::BuiltinFunction { target, method, arguments, .. } => {
                if arguments.is_some() {
                    self.infer_instance_method_call_type_id(arena, *target, arena.text(*method))
                } else {
                    self.infer_property_access_type_id(arena, *target, arena.text(*method))
                }
            }

            ExprNode::ImportedFunctionCall { namespace_name, function_name, .. } => {
                self.infer_imported_function_call_type(
                    arena.text(*namespace_name),
                    arena.text(*function_name),
                )
            }

            ExprNode::ArithmeticOp { left, right, .. } => {
                self.infer_arithmetic_op_type_ids(arena, *left, *right)
            }

            ExprNode::ComparisonOp { .. } => Some(DataType::Bool),
            ExprNode::LogicalOp { .. }    => Some(DataType::Bool),
            ExprNode::BitwiseOp { .. }    => Some(DataType::Int),

            ExprNode::UnaryOp { operator, operand, .. } => {
                self.infer_unary_op_type_id(arena, arena.text(*operator), *operand)
            }

            ExprNode::EnumAccess { .. } => Some(DataType::Enum),

            ExprNode::ConfigAccess { .. } => None,

            ExprNode::ObjectAccess { path, .. } => {
                let joined = arena
                    .str_list(*path)
                    .iter()
                    .map(|s| arena.text(*s))
                    .collect::<Vec<_>>()
                    .join(".");
                let prefixed = format!("DATA.{}", joined);
                self.symbol_table.try_get_data_variable(&joined)
                    .or_else(|| self.symbol_table.try_get_data_variable(&prefixed))
                    .and_then(|v| v.effective_type())
            }

            ExprNode::PropertyAccess { object, property, .. } => {
                self.infer_property_access_type_id(arena, *object, arena.text(*property))
            }

            ExprNode::IndexAccess { object, .. } => {
                self.infer_index_access_type_id(arena, *object)
            }

            ExprNode::Parenthesized { expression, .. } => {
                self.infer_type_from_expression_id(arena, *expression)
            }

            ExprNode::Conditional { true_value, false_value, .. } => {
                self.infer_type_from_expression_id(arena, *true_value)
                    .or_else(|| self.infer_type_from_expression_id(arena, *false_value))
            }

            ExprNode::TypeCast { target_type, .. } => Some(*target_type),
        }
    }

    // ── Element-type API ──────────────────────────────────────────────────────

    /// Infers the element type of the array or typed tuple the value `id` names.
    ///
    /// This is
    /// [`infer_element_type_from_value`](TypeInferenceVisitor::infer_element_type_from_value)
    /// for the arena form. The panics are those of
    /// [`infer_type_from_value_id`](Self::infer_type_from_value_id).
    pub fn infer_element_type_from_value_id(&self, arena: &Arena, id: ValueId) -> Option<DataType> {
        match arena.value(id) {
            ValueNode::Array { values, .. } | ValueNode::NestedArray { values, .. } => {
                self.infer_uniform_element_type_from_value_ids(arena, *values)
            }
            ValueNode::PrefixedConstructor { prefix, arguments, .. }
                if arena.text(*prefix).eq_ignore_ascii_case("t") =>
            {
                arena
                    .value_list(*arguments)
                    .first()
                    .and_then(|&v| self.infer_type_from_value_id(arena, v))
            }
            _ => None,
        }
    }

    /// Infers the element type of the array or tuple the expression `id` names.
    ///
    /// This is
    /// [`infer_element_type_from_expression`](TypeInferenceVisitor::infer_element_type_from_expression)
    /// for the arena form. The panics are those of
    /// [`infer_type_from_value_id`](Self::infer_type_from_value_id).
    pub fn infer_element_type_from_expression_id(&self, arena: &Arena, id: ExprId) -> Option<DataType> {
        match arena.expr(id) {
            ExprNode::Value { value, .. } => self.infer_element_type_from_value_id(arena, *value),

            ExprNode::Identifier { name, .. } => {
                let name = arena.text(*name);
                if let Some(&hint) = self.element_type_hints.get(name) {
                    return Some(hint);
                }
                if let Some(Some(var_type)) = self.local_variable_types.get(name) {
                    if let DataType::TypedArray(elem) = *var_type {
                        return Some(elem.to_data_type());
                    }
                    if let DataType::TypedTuple(arr) = *var_type {
                        return arr[0].map(|e| e.to_data_type());
                    }
                }
                None
            }

            ExprNode::PropertyAccess { object, .. } => {
                self.infer_element_type_from_expression_id(arena, *object)
            }

            _ => None,
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Core qualified-identifier type inference covering no-argument (property and enum
    /// access) and call forms (static and instance method calls).
    fn infer_qualified_identifier_type_ids(
        &self,
        arena:     &Arena,
        parts:     StrList,
        arguments: Option<ExprList>,
    ) -> Option<DataType> {
        let parts: Vec<&str> = arena.str_list(parts).iter().map(|s| arena.text(*s)).collect();
        if parts.len() < 2 { return None; }

        let first_part  = parts[0];
        let second_part = parts[1];

        // ── No-argument forms ─────────────────────────────────────────────────

        let Some(args) = arguments else {
            if parts.len() == 2 && self.symbol_table.has_enum(first_part) {
                return Some(DataType::Enum);
            }
            if parts.len() == 3
                && self.symbol_table.is_imported_namespace(parts[0])
                && self.symbol_table.get_namespaced_enum(parts[0], parts[1]).is_some()
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
        };

        // ── Call forms ────────────────────────────────────────────────────────

        // Static builtin: uppercase first letter → Math.sqrt(), DateTime.now()
        if parts.len() == 2
            && first_part.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
        {
            return self.infer_static_method_call_type_with_arg_ids(arena, first_part, second_part, args);
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
            if let Some(Some(var_type)) = self.local_variable_types.get(first_part) {
                let base_type = var_type.base_collection_type();

                // Object variables may carry lambda properties, so the registry is skipped
                // for Object to avoid conflating Object.add (builtin) with user-defined add
                // lambdas. Specific well-typed Object methods are handled here first.
                if base_type == DataType::Object {
                    match second_part {
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
                    if ARRAY_ELEMENT_METHODS.contains(&second_part) {
                        if let DataType::TypedArray(elem) = *var_type {
                            return Some(elem.to_data_type());
                        }
                        if let Some(&hint) = self.element_type_hints.get(first_part) {
                            return Some(hint);
                        }
                        return None;
                    }

                    if ARRAY_PRESERVING_METHODS.contains(&second_part) {
                        return match *var_type {
                            DataType::TypedArray(_) => Some(*var_type),
                            _                       => Some(DataType::Array),
                        };
                    }

                    if second_part == "flatten" { return Some(DataType::Array); }
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

                    if TUPLE_DYNAMIC_ELEMENT_METHODS.contains(&second_part) {
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

                    if TUPLE_PRESERVING_METHODS.contains(&second_part) {
                        return Some(DataType::Tuple);
                    }
                }

                // ── String methods returning typed arrays ────────────────────
                if base_type == DataType::String && second_part == "split" {
                    return Some(DataType::TypedArray(ElemType::String));
                }

                // ── Regex methods returning typed arrays ─────────────────────
                if base_type == DataType::Regex {
                    match second_part {
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
                        second_part,
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

    fn infer_arithmetic_op_type_ids(
        &self,
        arena: &Arena,
        left:  ExprId,
        right: ExprId,
    ) -> Option<DataType> {
        let lt = self.infer_type_from_expression_id(arena, left);
        let rt = self.infer_type_from_expression_id(arena, right);

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
                Some(l)
            }
            (Some(l), None) => Some(l),
            (None, Some(r)) => Some(r),
            (None, None)    => None,
        }
    }

    fn infer_prefixed_constructor_type_ids(
        &self,
        arena:     &Arena,
        prefix:    &str,
        arguments: ValueList,
    ) -> Option<DataType> {
        match prefix.to_lowercase().as_str() {
            "t" => {
                // Try to build TypedTuple from element types.
                let mut arr: [Option<ElemType>; 6] = [None; 6];
                let mut any_known = false;
                for (i, &arg) in arena.value_list(arguments).iter().enumerate().take(6) {
                    if let Some(dt) = self.infer_type_from_value_id(arena, arg) {
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

    fn infer_unary_op_type_id(&self, arena: &Arena, operator: &str, operand: ExprId) -> Option<DataType> {
        match operator {
            "!" | "not" => Some(DataType::Bool),
            _           => self.infer_type_from_expression_id(arena, operand),
        }
    }

    // ── Static method inference with argument types ───────────────────────────

    /// Infers a static method's return type, using argument types for the builtins whose
    /// result depends on what they receive.
    fn infer_static_method_call_type_with_arg_ids(
        &self,
        arena:       &Arena,
        object_name: &str,
        method_name: &str,
        arguments:   ExprList,
    ) -> Option<DataType> {
        let arguments = arena.expr_list(arguments);
        match (object_name, method_name) {
            // choice(arr) / weighted(vals, weights) → element type of the array
            ("Random", "choice") | ("Random", "weighted") => {
                if let Some(&first_arg) = arguments.first() {
                    if let Some(elem) = self.infer_element_type_from_expression_id(arena, first_arg) {
                        return Some(elem);
                    }
                }
            }

            // choices(arr, n) / sample(arr, n) → TypedArray preserving element type
            ("Random", "choices") | ("Random", "sample") => {
                if let Some(&first_arg) = arguments.first() {
                    if let Some(elem) = self.infer_element_type_from_expression_id(arena, first_arg) {
                        if let Some(et) = ElemType::from_data_type(elem) {
                            return Some(DataType::TypedArray(et));
                        }
                    }
                }
                return Some(DataType::Array);
            }

            // range(start, end) → always array of ints
            ("Array", "range") => return Some(DataType::TypedArray(ElemType::Int)),

            // fromString(text, sep) → always array of strings
            ("Array", "fromString") => return Some(DataType::TypedArray(ElemType::String)),

            // empty() → plain Array (no element type)
            ("Array", "empty") => return Some(DataType::Array),

            // fill(value, count) → TypedArray(T) where T is value's type
            ("Array", "fill") => {
                if let Some(&val_arg) = arguments.first() {
                    if let Some(elem_dt) = self.infer_type_from_expression_id(arena, val_arg) {
                        if let Some(et) = ElemType::from_data_type(elem_dt) {
                            return Some(DataType::TypedArray(et));
                        }
                    }
                }
                return Some(DataType::Array);
            }

            // of(v1, v2, ...) → TypedArray(T) when all values share type T
            ("Array", "of") => {
                if let Some(&first_arg) = arguments.first() {
                    if let Some(elem_dt) = self.infer_type_from_expression_id(arena, first_arg) {
                        if let Some(et) = ElemType::from_data_type(elem_dt) {
                            // Verify remaining args share the same type
                            let all_match = arguments[1..].iter().all(|&arg| {
                                self.infer_type_from_expression_id(arena, arg)
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

            // repeat(array, times) → same element type as input array
            ("Array", "repeat") => {
                if let Some(&first_arg) = arguments.first() {
                    if let Some(arr_type) = self.infer_type_from_expression_id(arena, first_arg) {
                        if let DataType::TypedArray(_) = arr_type {
                            return Some(arr_type);
                        }
                    }
                }
                return Some(DataType::Array);
            }

            // reverse, sort, unique, slice, filter → same element type as input array
            ("Array", "reverse")
            | ("Array", "sort")
            | ("Array", "unique")
            | ("Array", "slice")
            | ("Array", "filter") => {
                if let Some(&first_arg) = arguments.first() {
                    if let Some(arr_type) = self.infer_type_from_expression_id(arena, first_arg) {
                        if let DataType::TypedArray(_) = arr_type {
                            return Some(arr_type);
                        }
                    }
                }
                return Some(DataType::Array);
            }

            // concat(arr1, arr2, ...) → TypedArray if all inputs have same element type
            ("Array", "concat") => {
                let first_typed = arguments.iter().find_map(|&arg| {
                    if let Some(DataType::TypedArray(et)) = self.infer_type_from_expression_id(arena, arg) {
                        Some(et)
                    } else {
                        None
                    }
                });
                if let Some(et) = first_typed {
                    let all_same = arguments.iter().all(|&arg| {
                        match self.infer_type_from_expression_id(arena, arg) {
                            Some(DataType::TypedArray(e)) => e == et,
                            Some(DataType::Array)         => true, // untyped: tolerate
                            None                          => true, // unknown: tolerate
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

    // ── Instance method inference ─────────────────────────────────────────────

    /// The authoritative inference path for explicit instance method calls.
    fn infer_instance_method_call_type_id(
        &self,
        arena:       &Arena,
        instance:    ExprId,
        method_name: &str,
    ) -> Option<DataType> {
        let instance_data_type = self.infer_type_from_expression_id(arena, instance)?;
        let base_type          = instance_data_type.base_collection_type();

        // ── Array: element-returning ──────────────────────────────────────────
        if base_type == DataType::Array && ARRAY_ELEMENT_METHODS.contains(&method_name) {
            if let DataType::TypedArray(elem) = instance_data_type {
                return Some(elem.to_data_type());
            }
            return self.infer_element_type_from_expression_id(arena, instance);
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
        if base_type == DataType::Tuple {
            if let Some(slot) = Self::tuple_method_slot_index(method_name) {
                if let DataType::TypedTuple(arr) = instance_data_type {
                    if slot < 6 {
                        if let Some(elem) = arr[slot] {
                            return Some(elem.to_data_type());
                        }
                    }
                }
                // No TypedTuple annotation: fall back to element inference.
                return self.infer_element_type_from_expression_id(arena, instance);
            }

            // ── Tuple: get(i), index not known statically ────────────────────
            if TUPLE_DYNAMIC_ELEMENT_METHODS.contains(&method_name) {
                if let DataType::TypedTuple(arr) = instance_data_type {
                    // Use the first defined slot as best approximation.
                    if let Some(elem) = arr.iter().flatten().next() {
                        return Some(elem.to_data_type());
                    }
                }
                return self.infer_element_type_from_expression_id(arena, instance);
            }

            // ── Tuple: toArray ────────────────────────────────────────────────
            if method_name == "toArray" {
                if let DataType::TypedTuple(arr) = instance_data_type {
                    let defined: Vec<ElemType> = arr.iter().filter_map(|&e| e).collect();
                    if !defined.is_empty() && defined.iter().all(|&e| e == defined[0]) {
                        return Some(DataType::TypedArray(defined[0]));
                    }
                }
                return Some(DataType::Array);
            }

            // ── Tuple: reverse / swap change positions, so drop TypedTuple ────
            if TUPLE_PRESERVING_METHODS.contains(&method_name) {
                return Some(DataType::Tuple);
            }
        }

        // ── String: split → always TypedArray(String) ─────────────────────────
        if base_type == DataType::String && method_name == "split" {
            return Some(DataType::TypedArray(ElemType::String));
        }

        // ── Regex: collection-returning methods ───────────────────────────────
        if base_type == DataType::Regex {
            match method_name {
                "match" | "split" => return Some(DataType::TypedArray(ElemType::String)),
                "matchAll"        => return Some(DataType::Array),
                _ => {}
            }
        }

        // ── Object: well-typed collection methods ─────────────────────────────
        if base_type == DataType::Object {
            match method_name {
                "keys"           => return Some(DataType::TypedArray(ElemType::String)),
                "values"         => return Some(DataType::Array),
                "entries"
                | "toArray"      => return Some(DataType::Array),
                _ => {}
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

    fn infer_property_access_type_id(&self, arena: &Arena, object: ExprId, property: &str) -> Option<DataType> {
        if let Some(base) = Self::build_property_path_id(arena, object) {
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

    fn infer_index_access_type_id(&self, arena: &Arena, object: ExprId) -> Option<DataType> {
        let obj_type = self.infer_type_from_expression_id(arena, object)?;
        match obj_type {
            // TypedArray: element type is authoritative.
            DataType::TypedArray(elem) => Some(elem.to_data_type()),
            // TypedTuple: use slot 0 as the best static approximation.
            DataType::TypedTuple(arr)  => arr[0].map(|e| e.to_data_type()),
            // Plain collections: fall back to value-level element inference.
            DataType::Array | DataType::Tuple => {
                self.infer_element_type_from_expression_id(arena, object)
            }
            // String character access.
            DataType::String => Some(DataType::String),
            _ => None,
        }
    }

    fn build_property_path_id(arena: &Arena, expr: ExprId) -> Option<String> {
        match arena.expr(expr) {
            ExprNode::Identifier { name, .. } => Some(arena.text(*name).to_string()),
            ExprNode::PropertyAccess { object, property, .. } => {
                Self::build_property_path_id(arena, *object)
                    .map(|base| format!("{}.{}", base, arena.text(*property)))
            }
            _ => None,
        }
    }

    // ── Collection helpers ────────────────────────────────────────────────────

    /// Attempts to produce `TypedArray(T)` when all elements share type `T`. Returns `None`
    /// (not `Some(Array)`) on failure so callers can apply their own fallback.
    fn try_infer_typed_array_from_value_ids(&self, arena: &Arena, values: ValueList) -> Option<DataType> {
        let ids = arena.value_list(values);
        if ids.is_empty() { return None; }
        let first_elem = self.infer_type_from_value_id(arena, ids[0])?;
        for &v in ids.iter().skip(1) {
            match self.infer_type_from_value_id(arena, v) {
                Some(t) if t == first_elem => {}
                _ => return None,
            }
        }
        ElemType::from_data_type(first_elem).map(DataType::TypedArray)
    }

    fn infer_uniform_element_type_from_value_ids(&self, arena: &Arena, values: ValueList) -> Option<DataType> {
        let ids = arena.value_list(values);
        if ids.is_empty() { return None; }
        let first = self.infer_type_from_value_id(arena, ids[0])?;
        for &v in ids.iter().skip(1) {
            match self.infer_type_from_value_id(arena, v) {
                Some(t) if t == first => {}
                Some(_)               => return None,
                None                  => {}
            }
        }
        Some(first)
    }
}
