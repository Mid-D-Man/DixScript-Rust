// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/ValueResolution/enum_resolution.rs"
// ============================================================================
//! Enum pre-resolution over the arena AST: phase 1 of value resolution.
//!
//! [`resolve_data_enums`] walks the `@DATA` section of a [`DocData`], checks every enum
//! reference against the symbol table and rewrites the references that must become integers.
//! This file holds the only implementation of that walk. [`ValueResolver`] calls it with the
//! document's own arena when a [`Document`] is attached and with a scratch arena otherwise.
//!
//! The walk rewrites by allocation: a node whose children did not change keeps its id, so an
//! untouched subtree allocates nothing and a data section without enums comes back unchanged.
//! Two kinds of reference are handled differently, and the difference is part of the contract:
//!
//! * A bare `Enum.FIELD` at a data position (a [`ValueNode::EnumValue`]) is checked and left in
//!   place, because later stages need the enum identity.
//! * An enum access inside an expression (a [`ExprNode::EnumAccess`]) becomes an integer
//!   literal, because the expression is about to be evaluated.
//!
//! A reference that names an enum of an imported namespace through the dotted form of
//! [`ValueNode::EnumValue`] is also recorded in [`EnumPass::usages`], so the caller can merge
//! the enum into the file's own `@ENUMS` section.
//!
//! [`ValueResolver`]: super::value_resolver::ValueResolver
//! [`Document`]: crate::Compiler::AST::arena::Document

use std::collections::HashMap;

use super::value_resolver::ResolverError;
use crate::Compiler::AST::arena::{
    Arena, DocData, DocDataEntry, DocPropertyAssignment, ExprId, ExprList, ExprNode,
    ObjectPropNode, PropList, Str, ValueId, ValueList, ValueNode,
};
use crate::Compiler::AST::Position;
use crate::Compiler::Utilities::SymbolTable;
use crate::Utilities::RustcHash::FxHashMap;

/// What one run of [`resolve_data_enums`] found.
pub(crate) struct EnumPass {
    /// Whether any entry of the data section was rewritten.
    ///
    /// `false` means the section is exactly what the caller passed in, so a caller that keeps an
    /// owned copy of it has nothing to refresh.
    pub changed: bool,
    /// Enum references of the file's own enums that were checked.
    pub local: usize,
    /// Enum references into imported namespaces that were checked.
    pub imported: usize,
    /// Every imported enum referenced through the dotted value form, keyed by its qualified name
    /// (for example `Types.Priority`), with all of its fields.
    pub usages: FxHashMap<String, HashMap<String, i32>>,
}

/// Resolves the enum references of `data` against `symbols`, allocating rewritten nodes in
/// `arena`.
///
/// `arena` must be the arena `data` was built in. On success the entries that changed are
/// replaced in `data` and the returned [`EnumPass`] says so. On the first bad reference the
/// function returns its error and leaves `data` as it was; nodes allocated before the error stay
/// in the arena, unreferenced.
///
/// # Errors
///
/// [`ResolverError::InvalidEnumAccess`] when a reference names an unknown namespace, enum or
/// field.
///
/// # Panics
///
/// Panics if an id in `data` is out of range for `arena`.
pub(crate) fn resolve_data_enums(
    arena: &mut Arena,
    data: &mut DocData,
    symbols: &SymbolTable,
) -> Result<EnumPass, ResolverError> {
    let mut walk = Walk { arena, symbols, local: 0, imported: 0, usages: FxHashMap::default() };
    let mut changes = Vec::new();
    for (index, entry) in data.entries.iter().enumerate() {
        if let Some(new) = walk.entry(entry)? {
            changes.push((index, new));
        }
    }
    let changed = !changes.is_empty();
    for (index, new) in changes {
        data.entries[index] = new;
    }
    Ok(EnumPass { changed, local: walk.local, imported: walk.imported, usages: walk.usages })
}

/// The state of one walk: the arena being rewritten and the references counted so far.
struct Walk<'a> {
    arena: &'a mut Arena,
    symbols: &'a SymbolTable,
    local: usize,
    imported: usize,
    usages: FxHashMap<String, HashMap<String, i32>>,
}

impl<'a> Walk<'a> {
    /// The rewritten form of `entry`, or `None` when nothing in it changed.
    fn entry(&mut self, entry: &DocDataEntry) -> Result<Option<DocDataEntry>, ResolverError> {
        Ok(match entry {
            DocDataEntry::SimpleProperty { name, data_type, value, position } => {
                let new = self.resolve_enums_in_value(*value)?;
                (new != *value).then(|| DocDataEntry::SimpleProperty {
                    name: name.clone(),
                    data_type: *data_type,
                    value: new,
                    position: *position,
                })
            }
            DocDataEntry::TableProperty { path, properties, position } => {
                let mut new_properties: Option<Vec<DocPropertyAssignment>> = None;
                for (index, property) in properties.iter().enumerate() {
                    let new = self.resolve_enums_in_value(property.value)?;
                    if new != property.value {
                        new_properties.get_or_insert_with(|| properties.clone())[index].value = new;
                    }
                }
                new_properties.map(|properties| DocDataEntry::TableProperty {
                    path: path.clone(),
                    properties,
                    position: *position,
                })
            }
            DocDataEntry::GroupArray { path, items, position } => {
                self.value_list(*items)?.map(|items| DocDataEntry::GroupArray {
                    path: path.clone(),
                    items,
                    position: *position,
                })
            }
            DocDataEntry::ObjectProperty { name, data_type, object, position } => {
                // Only an object literal is walked; any other value here is left as it is.
                let new = if matches!(self.arena.value(*object), ValueNode::Object { .. }) {
                    self.resolve_enums_in_value(*object)?
                } else {
                    *object
                };
                (new != *object).then(|| DocDataEntry::ObjectProperty {
                    name: name.clone(),
                    data_type: *data_type,
                    object: new,
                    position: *position,
                })
            }
        })
    }

    /// The id of the value `id` names after enum resolution, which is `id` itself when nothing
    /// under it changed.
    fn resolve_enums_in_value(&mut self, id: ValueId) -> Result<ValueId, ResolverError> {
        let node = *self.arena.value(id);
        Ok(match node {
            ValueNode::EnumValue { enum_name, value, position } => {
                self.check_enum_value(enum_name, value, position)?;
                id
            }
            ValueNode::Expression { expr, position } => {
                let new = self.resolve_enums_in_expr(expr)?;
                if new == expr {
                    id
                } else {
                    self.arena.alloc_value(ValueNode::Expression { expr: new, position })
                }
            }
            ValueNode::QuickFuncCall { function_name, arguments, position } => {
                match self.expr_list(arguments)? {
                    Some(arguments) => self.arena.alloc_value(ValueNode::QuickFuncCall {
                        function_name,
                        arguments,
                        position,
                    }),
                    None => id,
                }
            }
            ValueNode::Array { values, position } => match self.value_list(values)? {
                Some(values) => self.arena.alloc_value(ValueNode::Array { values, position }),
                None => id,
            },
            // A nested array that held any enum reference comes back as a plain array, as it
            // always did; the reference itself may be left in place, so what counts is that one
            // was seen, not that an element changed.
            ValueNode::NestedArray { values, position, .. } => {
                let seen = self.local + self.imported;
                let new = self.value_list(values)?;
                if self.local + self.imported == seen {
                    id
                } else {
                    let values = new.unwrap_or(values);
                    self.arena.alloc_value(ValueNode::Array { values, position })
                }
            }
            ValueNode::Object { properties, position } => match self.prop_list(properties)? {
                Some(properties) => {
                    self.arena.alloc_value(ValueNode::Object { properties, position })
                }
                None => id,
            },
            ValueNode::PrefixedConstructor { prefix, arguments, position } => {
                match self.value_list(arguments)? {
                    Some(arguments) => self.arena.alloc_value(ValueNode::PrefixedConstructor {
                        prefix,
                        arguments,
                        position,
                    }),
                    None => id,
                }
            }
            ValueNode::InterpolatedString { template, expressions, position } => {
                match self.expr_list(expressions)? {
                    Some(expressions) => self.arena.alloc_value(ValueNode::InterpolatedString {
                        template,
                        expressions,
                        position,
                    }),
                    None => id,
                }
            }
            _ => id,
        })
    }

    /// The id of the expression `id` names after enum resolution, which is `id` itself when
    /// nothing under it changed.
    ///
    /// Only the expression kinds that can lead to an enum access are entered; every other kind
    /// is returned as it is.
    fn resolve_enums_in_expr(&mut self, id: ExprId) -> Result<ExprId, ResolverError> {
        let node = *self.arena.expr(id);
        Ok(match node {
            ExprNode::EnumAccess { namespace_name: None, enum_name, value, position } => {
                let (enum_name, field) = (self.arena.text(enum_name), self.arena.text(value));
                let int = self
                    .symbols
                    .try_get_enum_field_value(enum_name, field)
                    .ok_or_else(|| ResolverError::InvalidEnumAccess {
                        location: format!("{}.{}", enum_name, field),
                        message: format!("Enum '{}' field '{}' not found", enum_name, field),
                        position,
                    })?;
                self.local += 1;
                self.integer_expr(int, position)
            }
            ExprNode::EnumAccess { namespace_name: Some(namespace), enum_name, value, position } => {
                let (ns_name, enum_name, field) = (
                    self.arena.text(namespace),
                    self.arena.text(enum_name),
                    self.arena.text(value),
                );
                let fail = |message: String| ResolverError::InvalidEnumAccess {
                    location: format!("{}.{}.{}", ns_name, enum_name, field),
                    message,
                    position,
                };
                let ns = self
                    .symbols
                    .try_get_namespace(ns_name)
                    .ok_or_else(|| fail(format!("Namespace '{}' not found", ns_name)))?;
                let fields = ns.enums.get(enum_name).ok_or_else(|| {
                    fail(format!("Enum '{}' not found in namespace '{}'", enum_name, ns_name))
                })?;
                let int = *fields
                    .get(field)
                    .ok_or_else(|| fail(format!("Field '{}' not found", field)))?;
                self.imported += 1;
                self.integer_expr(int, position)
            }
            ExprNode::Value { value, position } => {
                let new = self.resolve_enums_in_value(value)?;
                if new == value {
                    id
                } else {
                    self.arena.alloc_expr(ExprNode::Value { value: new, position })
                }
            }
            ExprNode::QuickFuncCall { name, arguments, position } => {
                match self.expr_list(arguments)? {
                    Some(arguments) => {
                        self.arena.alloc_expr(ExprNode::QuickFuncCall { name, arguments, position })
                    }
                    None => id,
                }
            }
            ExprNode::ImportedFunctionCall { namespace_name, function_name, arguments, position } => {
                match self.expr_list(arguments)? {
                    Some(arguments) => self.arena.alloc_expr(ExprNode::ImportedFunctionCall {
                        namespace_name,
                        function_name,
                        arguments,
                        position,
                    }),
                    None => id,
                }
            }
            ExprNode::StaticMethodCall { object_name, method_name, arguments, position } => {
                match self.expr_list(arguments)? {
                    Some(arguments) => self.arena.alloc_expr(ExprNode::StaticMethodCall {
                        object_name,
                        method_name,
                        arguments,
                        position,
                    }),
                    None => id,
                }
            }
            ExprNode::ArithmeticOp { left, operator, right, position } => {
                match self.pair(left, right)? {
                    Some((left, right)) => self.arena.alloc_expr(ExprNode::ArithmeticOp {
                        left,
                        operator,
                        right,
                        position,
                    }),
                    None => id,
                }
            }
            ExprNode::BitwiseOp { left, operator, right, position } => {
                match self.pair(left, right)? {
                    Some((left, right)) => self.arena.alloc_expr(ExprNode::BitwiseOp {
                        left,
                        operator,
                        right,
                        position,
                    }),
                    None => id,
                }
            }
            ExprNode::ComparisonOp { left, operator, right, position } => {
                match self.pair(left, right)? {
                    Some((left, right)) => self.arena.alloc_expr(ExprNode::ComparisonOp {
                        left,
                        operator,
                        right,
                        position,
                    }),
                    None => id,
                }
            }
            ExprNode::LogicalOp { left, operator, right, position } => {
                match self.pair(left, right)? {
                    Some((left, right)) => self.arena.alloc_expr(ExprNode::LogicalOp {
                        left,
                        operator,
                        right,
                        position,
                    }),
                    None => id,
                }
            }
            ExprNode::UnaryOp { operator, operand, position } => {
                let new = self.resolve_enums_in_expr(operand)?;
                if new == operand {
                    id
                } else {
                    self.arena.alloc_expr(ExprNode::UnaryOp { operator, operand: new, position })
                }
            }
            ExprNode::Conditional { condition, true_value, false_value, position } => {
                let c = self.resolve_enums_in_expr(condition)?;
                let t = self.resolve_enums_in_expr(true_value)?;
                let f = self.resolve_enums_in_expr(false_value)?;
                if (c, t, f) == (condition, true_value, false_value) {
                    id
                } else {
                    self.arena.alloc_expr(ExprNode::Conditional {
                        condition: c,
                        true_value: t,
                        false_value: f,
                        position,
                    })
                }
            }
            ExprNode::Parenthesized { expression, position } => {
                let new = self.resolve_enums_in_expr(expression)?;
                if new == expression {
                    id
                } else {
                    self.arena.alloc_expr(ExprNode::Parenthesized { expression: new, position })
                }
            }
            ExprNode::PropertyAccess { object, property, position } => {
                let new = self.resolve_enums_in_expr(object)?;
                if new == object {
                    id
                } else {
                    self.arena.alloc_expr(ExprNode::PropertyAccess {
                        object: new,
                        property,
                        position,
                    })
                }
            }
            ExprNode::IndexAccess { object, index, position } => {
                match self.pair(object, index)? {
                    Some((object, index)) => {
                        self.arena.alloc_expr(ExprNode::IndexAccess { object, index, position })
                    }
                    None => id,
                }
            }
            _ => id,
        })
    }

    /// Checks a bare enum reference at a data position and counts it.
    ///
    /// A name with a dot (`Namespace.Enum`) is looked up in the imported namespace, and the whole
    /// enum is recorded for the caller; a name without one is looked up in the file's own enums.
    fn check_enum_value(
        &mut self,
        enum_name: Str,
        field: Str,
        position: Position,
    ) -> Result<(), ResolverError> {
        let (name, field) = (self.arena.text(enum_name), self.arena.text(field));
        if let Some(dot) = name.find('.') {
            let (ns_name, actual) = (&name[..dot], &name[dot + 1..]);
            let fail = |message: String| ResolverError::InvalidEnumAccess {
                location: format!("{}.{}.{}", ns_name, actual, field),
                message,
                position,
            };
            let ns = self
                .symbols
                .try_get_namespace(ns_name)
                .ok_or_else(|| fail(format!("Namespace '{}' not found", ns_name)))?;
            let fields = ns
                .enums
                .get(actual)
                .ok_or_else(|| fail(format!("Enum '{}' not found", actual)))?;
            fields.get(field).ok_or_else(|| fail(format!("Field '{}' not found", field)))?;
            self.usages.entry(name.to_owned()).or_insert_with(|| fields.clone());
            self.imported += 1;
        } else {
            self.symbols.try_get_enum_field_value(name, field).ok_or_else(|| {
                ResolverError::InvalidEnumAccess {
                    location: format!("{}.{}", name, field),
                    message: format!("Enum field '{}' not found", field),
                    position,
                }
            })?;
            self.local += 1;
        }
        Ok(())
    }

    /// An integer literal expression, the form an enum access takes once resolved.
    fn integer_expr(&mut self, value: i32, position: Position) -> ExprId {
        let value = self.arena.alloc_value(ValueNode::Integer { value, position });
        self.arena.alloc_expr(ExprNode::Value { value, position })
    }

    /// Resolves two operands in order; `None` when neither changed.
    fn pair(&mut self, left: ExprId, right: ExprId) -> Result<Option<(ExprId, ExprId)>, ResolverError> {
        let new = (self.resolve_enums_in_expr(left)?, self.resolve_enums_in_expr(right)?);
        Ok((new != (left, right)).then_some(new))
    }

    /// Resolves every element of a list of values; `None` when no element changed, so the list
    /// is shared.
    fn value_list(&mut self, list: ValueList) -> Result<Option<ValueList>, ResolverError> {
        self.list(list, Arena::value_list, Self::resolve_enums_in_value, Arena::alloc_value_list)
    }

    /// Resolves every element of a list of expressions; `None` when no element changed.
    fn expr_list(&mut self, list: ExprList) -> Result<Option<ExprList>, ResolverError> {
        self.list(list, Arena::expr_list, Self::resolve_enums_in_expr, Arena::alloc_expr_list)
    }

    /// Resolves the value of every property of an object literal; `None` when none changed.
    fn prop_list(&mut self, list: PropList) -> Result<Option<PropList>, ResolverError> {
        let mut out: Option<Vec<ObjectPropNode>> = None;
        for index in 0..self.arena.prop_list(list).len() {
            let prop = self.arena.prop_list(list)[index];
            let value = self.resolve_enums_in_value(prop.value)?;
            if value != prop.value && out.is_none() {
                out = Some(self.arena.prop_list(list)[..index].to_vec());
            }
            if let Some(props) = out.as_mut() {
                props.push(ObjectPropNode { value, ..prop });
            }
        }
        Ok(out.map(|props| self.arena.alloc_prop_list(&props)))
    }

    /// Walks the elements of `list` with `walk`; allocates a new list only from the first element
    /// that changed, and returns `None` when none did.
    fn list<L: Copy, I: Copy + PartialEq>(
        &mut self,
        list: L,
        get: fn(&Arena, L) -> &[I],
        walk: fn(&mut Self, I) -> Result<I, ResolverError>,
        alloc: fn(&mut Arena, &[I]) -> L,
    ) -> Result<Option<L>, ResolverError> {
        let mut out: Option<Vec<I>> = None;
        for index in 0..get(self.arena, list).len() {
            let id = get(self.arena, list)[index];
            let new = walk(self, id)?;
            if new != id && out.is_none() {
                out = Some(get(self.arena, list)[..index].to_vec());
            }
            if let Some(ids) = out.as_mut() {
                ids.push(new);
            }
        }
        Ok(out.map(|ids| alloc(self.arena, &ids)))
    }
}
