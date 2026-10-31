// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/AST/Visitors/type_inference_visitor.rs"
// ============================================================================
//! Type inference for values and expressions.
//!
//! [`TypeInferenceVisitor`] infers the type of a value or an expression from the symbol table and
//! from the types of the local variables it was given.
//!
//! The inference itself is in `type_inference_arena.rs`, which reads the nodes of an
//! [`Arena`]. The four entry points in this file take the owned
//! [`Value`] and [`Expression`]
//! types: each copies its argument into a scratch arena and calls the entry point of the same name
//! with an `_id` suffix. What depends on names alone (the identifier lookup, the function return
//! types, the method tables and the builtin registries) is kept here and shared.
//!
//! A caller that already holds an arena should call the `_id` entry points: they copy nothing.

use crate::Compiler::AST::arena::Arena;
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
pub(super) const ARRAY_ELEMENT_METHODS: &[&str] = &[
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
pub(super) const ARRAY_PRESERVING_METHODS: &[&str] = &[
    "set", "push", "pop", "shift", "unshift",
    "slice", "reverse", "sort", "concat",
    "filter", "distinct",
];

/// Tuple instance methods that return an element but whose slot index is not
/// statically known at the call site (e.g. `get(i)`).
pub(super) const TUPLE_DYNAMIC_ELEMENT_METHODS: &[&str] = &["get"];

/// Tuple instance methods that return `DixType::Tuple` but change element
/// ordering, making the TypedTuple slot annotations inaccurate.
/// These return plain `DataType::Tuple` even when the receiver is TypedTuple.
pub(super) const TUPLE_PRESERVING_METHODS: &[&str] = &["reverse", "swap"];

// ─────────────────────────────────────────────────────────────────────────────
// TypeInferenceVisitor
// ─────────────────────────────────────────────────────────────────────────────

/// Infers the type of a value or an expression.
///
/// It is given the symbol table the semantic analysis produced and, optionally, the types of the
/// local variables in scope (and, for local collections, the types of their elements). Both the
/// owned entry points (`infer_type_from_value`, `infer_type_from_expression`,
/// `infer_element_type_from_value`, `infer_element_type_from_expression`) and the arena ones (the
/// same names with an `_id` suffix) are on this type.
pub struct TypeInferenceVisitor<'a> {
    pub(super) symbol_table:         &'a SymbolTable,
    pub(super) local_variable_types: HashMap<String, Option<DataType>>,
    pub(super) element_type_hints:   HashMap<String, DataType>,
}

impl<'a> TypeInferenceVisitor<'a> {
    // ── Constructors ──────────────────────────────────────────────────────────

    /// A visitor over `symbol_table` that knows the types of the given local variables.
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

    /// Like [`new`](Self::new), and also knows the element type of the given local collections.
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

    /// A visitor whose local variables are the parameters of a quick function.
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

    /// Infers the type of an owned value.
    ///
    /// Copies `value` into a scratch arena and calls
    /// [`infer_type_from_value_id`](Self::infer_type_from_value_id). A caller that already has an
    /// arena should call that instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::{DataType, Position, Value};
    /// use dixscript::Compiler::AST::Visitors::TypeInferenceVisitor;
    /// use dixscript::Compiler::Utilities::SymbolTable;
    ///
    /// let pos = Position::new(1, 1);
    /// let array = Value::Array {
    ///     values: vec![
    ///         Value::Integer { value: 1, position: pos },
    ///         Value::Integer { value: 2, position: pos },
    ///     ],
    ///     position: pos,
    /// };
    ///
    /// let table = SymbolTable::new();
    /// let visitor = TypeInferenceVisitor::new(&table, None);
    /// assert!(matches!(visitor.infer_type_from_value(&array), Some(DataType::TypedArray(_))));
    /// ```
    pub fn infer_type_from_value(&self, value: &Value) -> Option<DataType> {
        let mut arena = Arena::new();
        let id = arena.import_value(value);
        self.infer_type_from_value_id(&arena, id)
    }

    /// Infers the type of an owned expression.
    ///
    /// Copies `expr` into a scratch arena and calls
    /// [`infer_type_from_expression_id`](Self::infer_type_from_expression_id).
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::{DataType, Expression, Position, Value};
    /// use dixscript::Compiler::AST::Visitors::TypeInferenceVisitor;
    /// use dixscript::Compiler::Utilities::SymbolTable;
    ///
    /// let pos = Position::new(1, 1);
    /// let one = Expression::Value { value: Value::Integer { value: 1, position: pos }, position: pos };
    /// let negated = Expression::UnaryOp { operator: "-".to_string(), operand: Box::new(one), position: pos };
    ///
    /// let table = SymbolTable::new();
    /// let visitor = TypeInferenceVisitor::new(&table, None);
    /// assert_eq!(visitor.infer_type_from_expression(&negated), Some(DataType::Int));
    /// ```
    pub fn infer_type_from_expression(&self, expr: &Expression) -> Option<DataType> {
        let mut arena = Arena::new();
        let id = arena.import_expr(expr);
        self.infer_type_from_expression_id(&arena, id)
    }

    // ── Element-type API ──────────────────────────────────────────────────────

    /// Infers the type of the elements of an owned value that is a collection.
    ///
    /// Copies `value` into a scratch arena and calls
    /// [`infer_element_type_from_value_id`](Self::infer_element_type_from_value_id).
    pub fn infer_element_type_from_value(&self, value: &Value) -> Option<DataType> {
        let mut arena = Arena::new();
        let id = arena.import_value(value);
        self.infer_element_type_from_value_id(&arena, id)
    }

    /// Infers the type of the elements of an owned expression that is a collection.
    ///
    /// Copies `expr` into a scratch arena and calls
    /// [`infer_element_type_from_expression_id`](Self::infer_element_type_from_expression_id).
    pub fn infer_element_type_from_expression(&self, expr: &Expression) -> Option<DataType> {
        let mut arena = Arena::new();
        let id = arena.import_expr(expr);
        self.infer_element_type_from_expression_id(&arena, id)
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    pub(super) fn infer_identifier_type(&self, name: &str) -> Option<DataType> {
        if let Some(local_type) = self.local_variable_types.get(name) {
            return *local_type;
        }
        if self.symbol_table.has_enum(name) {
            return Some(DataType::Enum);
        }
        None
    }

    pub(super) fn infer_imported_function_call_type(
        &self,
        namespace_name: &str,
        function_name:  &str,
    ) -> Option<DataType> {
        self.symbol_table
            .get_namespaced_function(namespace_name, function_name)
            .and_then(|f| f.signature.return_type)
    }

    #[inline]
    pub(super) fn is_numeric_type(dt: DataType) -> bool {
        matches!(dt, DataType::Int | DataType::Long | DataType::Float | DataType::Double)
    }

    pub(super) fn infer_function_call_type(&self, function_name: &str) -> Option<DataType> {
        self.symbol_table
            .try_get_function(function_name)
            .and_then(|sig| sig.return_type)
    }

    pub(super) fn infer_quick_func_call_type(&self, name: &str) -> Option<DataType> {
        self.symbol_table
            .try_get_function(name)
            .and_then(|sig| sig.return_type)
    }

    pub(super) fn infer_dix_function_call_type(&self, function_name: &str) -> Option<DataType> {
        match function_name {
            "Format" | "Join" => Some(DataType::String),
            _ => None,
        }
    }

    // ── Static method inference with argument types ───────────────────────────

    pub(super) fn infer_static_method_call_return_type(
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

    // ── Path / index helpers ──────────────────────────────────────────────────

    // ── Collection helpers ────────────────────────────────────────────────────

    /// Returns the 0-based TypedTuple slot index for a positional tuple accessor.
    /// Returns `None` for non-positional methods (`get`, etc.).
    #[inline]
    pub(super) fn tuple_method_slot_index(method_name: &str) -> Option<usize> {
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
