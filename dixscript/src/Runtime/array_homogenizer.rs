// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Runtime/array_homogenizer.rs"
// ============================================================================
//! Post-resolution numeric array homogenization.
//!
//! QuickFunc value resolution can produce array literals whose elements
//! retain whatever literal type their *source expression* had, so
//! `[12.3, someIntExpr(), 4.9]` can end up as
//! `[Double(12.3), Integer(4), Double(4.9)]` even though the array is
//! semantically "an array of doubles".
//!
//! This pass walks every array / nested-array / group-array in the `@DATA`
//! section and, when **every** element is a numeric literal but they don't
//! all share the same rank, promotes the lower-rank elements up to the
//! highest rank present:
//!
//! ```text
//! Integer < Long < Float < (Double | ScientificNotation)
//! ```
//!
//! Arrays containing any non-numeric element (strings, objects, enums, ...)
//! are left completely untouched; this only fires when the array is
//! homogeneously numeric except for the int/float distinction.
//!
//! Tuples (`t:(...)`) are intentionally skipped at the sibling level: they
//! are heterogeneous by design, but their *individual* elements are still
//! recursed into in case one of them is itself a numeric array.
//!
//! ## One walk, two entry points
//!
//! The walk is implemented once, over the arena AST, in [`homogenize_document_data`]. It
//! rewrites by allocation: a node whose children did not change keeps its id, a list is copied
//! only from its first changed element, and the return value says whether anything changed, so
//! a pass over a section with nothing to promote allocates nothing.
//! [`homogenize_document`] runs it over a [`Document`]'s own data, which is what the loader
//! does; [`homogenize_data_section`] keeps the owned-tree entry point, copying the section into
//! a scratch arena and exporting it back only when something changed.

use crate::Compiler::AST::arena::{
    Arena, DocData, DocDataEntry, DocPropertyAssignment, Document, ObjectPropNode, PropList,
    ValueId, ValueList, ValueNode,
};
use crate::Compiler::AST::DixScript;

/// Homogenizes the numeric arrays of an owned AST's `@DATA` section in place.
///
/// Call once on a fully-resolved AST, before handing it to the binary serializer / converters /
/// "Create Resolved" output. The section is copied into a scratch arena, run through
/// [`homogenize_document_data`] and exported back only when something was promoted, so a
/// section with nothing to promote is left exactly as it was. An AST without a data section is
/// left alone.
///
/// # Panics
///
/// Panics if the section needs more nodes or text than an arena can address (more than
/// `u32::MAX` of either).
pub fn homogenize_data_section(ast: &mut DixScript) {
    let Some(section) = ast.data.as_ref() else { return };
    let mut arena = Arena::new();
    let mut data = DocData::from_owned(&mut arena, section);
    if homogenize_document_data(&mut arena, &mut data) {
        ast.data = Some(data.export(&arena));
    }
}

/// Homogenizes the numeric arrays of `document`'s `@DATA` section in its own arena.
///
/// Returns whether anything was promoted. A document without a data section is left alone and
/// returns `false`. Running it again on its own result returns `false`.
///
/// # Examples
///
/// The loader has already run this pass on a document it returns, so a second run finds
/// nothing to do:
///
/// ```
/// use dixscript::Runtime::{homogenize_document, DixLoader};
///
/// let mut document = DixLoader::new_silent()
///     .compile_to_document_from_str("@DATA(\n  weights = [1.5, 2.5]\n)\n", "example")
///     .expect("the source compiles");
/// assert!(!homogenize_document(&mut document));
/// ```
///
/// # Panics
///
/// Panics if an id in the document's data section is out of range for its arena.
pub fn homogenize_document(document: &mut Document) -> bool {
    let Document { arena, data, .. } = document;
    match data.as_mut() {
        Some(data) => homogenize_document_data(arena, data),
        None => false,
    }
}

/// Homogenizes the numeric arrays of `data`, allocating promoted nodes in `arena`.
///
/// `arena` must be the arena `data` was built in. The entries that changed are replaced in
/// `data` and the function returns `true`; when it returns `false` the section is exactly what
/// the caller passed in and nothing was allocated. Nodes that were replaced stay in the arena,
/// unreferenced.
///
/// # Panics
///
/// Panics if an id in `data` is out of range for `arena`.
pub fn homogenize_document_data(arena: &mut Arena, data: &mut DocData) -> bool {
    let mut walk = Walk { arena };
    let mut changes = Vec::new();
    for (index, entry) in data.entries.iter().enumerate() {
        if let Some(new) = walk.entry(entry) {
            changes.push((index, new));
        }
    }
    let changed = !changes.is_empty();
    for (index, new) in changes {
        data.entries[index] = new;
    }
    changed
}

/// The state of one walk: the arena being rewritten.
struct Walk<'a> {
    arena: &'a mut Arena,
}

impl Walk<'_> {
    /// The rewritten form of `entry`, or `None` when nothing in it changed.
    fn entry(&mut self, entry: &DocDataEntry) -> Option<DocDataEntry> {
        match entry {
            DocDataEntry::SimpleProperty { name, data_type, value, position } => {
                let new = self.value(*value);
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
                    let new = self.value(property.value);
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
            // The items of a group array are siblings of each other, like the elements of an
            // array.
            DocDataEntry::GroupArray { path, items, position } => {
                self.elements(*items).map(|items| DocDataEntry::GroupArray {
                    path: path.clone(),
                    items,
                    position: *position,
                })
            }
            DocDataEntry::ObjectProperty { name, data_type, object, position } => {
                let new = self.value(*object);
                (new != *object).then(|| DocDataEntry::ObjectProperty {
                    name: name.clone(),
                    data_type: *data_type,
                    object: new,
                    position: *position,
                })
            }
        }
    }

    /// The id of the value `id` names after homogenization, which is `id` itself when nothing
    /// under it changed.
    fn value(&mut self, id: ValueId) -> ValueId {
        let node = *self.arena.value(id);
        match node {
            ValueNode::Array { values, position } => match self.elements(values) {
                Some(values) => self.arena.alloc_value(ValueNode::Array { values, position }),
                None => id,
            },
            // The variant and its nesting level are kept.
            ValueNode::NestedArray { values, level, position } => match self.elements(values) {
                Some(values) => {
                    self.arena.alloc_value(ValueNode::NestedArray { values, level, position })
                }
                None => id,
            },
            ValueNode::Object { properties, position } => match self.properties(properties) {
                Some(properties) => {
                    self.arena.alloc_value(ValueNode::Object { properties, position })
                }
                None => id,
            },
            // Tuples are heterogeneous by design: each element is recursed into on its own,
            // but never homogenized across its siblings.
            ValueNode::PrefixedConstructor { prefix, arguments, position } => {
                match self.recurse(arguments) {
                    Some(ids) => {
                        let arguments = self.arena.alloc_value_list(&ids);
                        self.arena.alloc_value(ValueNode::PrefixedConstructor {
                            prefix,
                            arguments,
                            position,
                        })
                    }
                    None => id,
                }
            }
            ValueNode::Range { start, end, position } => {
                let new = (self.value(start), self.value(end));
                if new == (start, end) {
                    id
                } else {
                    self.arena.alloc_value(ValueNode::Range { start: new.0, end: new.1, position })
                }
            }
            _ => id,
        }
    }

    /// Homogenizes the elements of an array: first each element on its own, then the elements
    /// as siblings of each other. `None` when nothing changed, so the list is shared.
    fn elements(&mut self, list: ValueList) -> Option<ValueList> {
        let recursed = self.recurse(list);
        // Recursion only replaces elements that hold other values, and those have no numeric
        // rank, so the rank of the elements is the same before and after it.
        match self.promotion_target(self.arena.value_list(list)) {
            Some(rank) => {
                let mut ids = recursed.unwrap_or_else(|| self.arena.value_list(list).to_vec());
                for id in ids.iter_mut() {
                    let node = *self.arena.value(*id);
                    if let Some(promoted) = promote(node, rank) {
                        *id = self.arena.alloc_value(promoted);
                    }
                }
                Some(self.arena.alloc_value_list(&ids))
            }
            None => recursed.map(|ids| self.arena.alloc_value_list(&ids)),
        }
    }

    /// Homogenizes every element of `list` on its own. The ids of the new list, or `None` when
    /// no element changed; the ids are copied only from the first element that did.
    fn recurse(&mut self, list: ValueList) -> Option<Vec<ValueId>> {
        let mut out: Option<Vec<ValueId>> = None;
        for index in 0..self.arena.value_list(list).len() {
            let id = self.arena.value_list(list)[index];
            let new = self.value(id);
            if new != id && out.is_none() {
                out = Some(self.arena.value_list(list)[..index].to_vec());
            }
            if let Some(ids) = out.as_mut() {
                ids.push(new);
            }
        }
        out
    }

    /// Homogenizes the value of every property of an object literal; `None` when none changed.
    fn properties(&mut self, list: PropList) -> Option<PropList> {
        let mut out: Option<Vec<ObjectPropNode>> = None;
        for index in 0..self.arena.prop_list(list).len() {
            let prop = self.arena.prop_list(list)[index];
            let value = self.value(prop.value);
            if value != prop.value && out.is_none() {
                out = Some(self.arena.prop_list(list)[..index].to_vec());
            }
            if let Some(props) = out.as_mut() {
                props.push(ObjectPropNode { value, ..prop });
            }
        }
        out.map(|props| self.arena.alloc_prop_list(&props))
    }

    /// The rank the elements of an array are promoted to, or `None` when nothing is promoted:
    /// the array holds an element that is not a numeric literal, or all of its elements share
    /// one rank. An array of fewer than two elements always shares one, so it needs no check of
    /// its own.
    fn promotion_target(&self, ids: &[ValueId]) -> Option<u8> {
        let (mut min, mut max) = (u8::MAX, 0u8);
        for id in ids {
            let rank = numeric_rank(self.arena.value(*id))?;
            min = min.min(rank);
            max = max.max(rank);
        }
        (min < max).then_some(max)
    }
}

/// Numeric "rank" used to pick the common promoted type.
/// `None` means "not a numeric literal": if ANY element in an array is
/// non-numeric, the whole array is left alone.
#[inline]
fn numeric_rank(node: &ValueNode) -> Option<u8> {
    match node {
        ValueNode::Integer { .. }            => Some(0),
        ValueNode::Long { .. }               => Some(1),
        ValueNode::Float { .. }              => Some(2),
        ValueNode::Double { .. }             => Some(3),
        ValueNode::ScientificNotation { .. } => Some(3),
        _ => None,
    }
}

/// The node `node` becomes when promoted to `target_rank`, or `None` when it stays as it is.
/// A scientific-notation literal is already at the top rank and is never converted.
fn promote(node: ValueNode, target_rank: u8) -> Option<ValueNode> {
    match (target_rank, node) {
        (3, ValueNode::Integer { value, position }) => Some(ValueNode::Double { value: value as f64, position }),
        (3, ValueNode::Long    { value, position }) => Some(ValueNode::Double { value: value as f64, position }),
        (3, ValueNode::Float   { value, position }) => Some(ValueNode::Double { value: value as f64, position }),
        (2, ValueNode::Integer { value, position }) => Some(ValueNode::Float { value: value as f32, position }),
        (2, ValueNode::Long    { value, position }) => Some(ValueNode::Float { value: value as f32, position }),
        (1, ValueNode::Integer { value, position }) => Some(ValueNode::Long { value: value as i64, position }),
        _ => None,
    }
}

/// The owned-tree implementation as it was before the arena walk replaced it, kept verbatim
/// as the oracle of the differential tests in `arena_tests`.
#[cfg(test)]
mod legacy {
    use crate::Compiler::AST::{DixScript, DataEntry, Value, Position};

/// Entry point — call once on a fully-resolved AST, before handing it to
/// the binary serializer / converters / "Create Resolved" output.
pub fn homogenize_data_section(ast: &mut DixScript) {
    let data = match ast.data.as_mut() {
        Some(d) => d,
        None => return,
    };

    for entry in data.entries.iter_mut() {
        match entry {
            DataEntry::SimpleProperty { value, .. } => homogenize_value(value),
            DataEntry::TableProperty { properties, .. } => {
                for prop in properties.iter_mut() {
                    homogenize_value(&mut prop.value);
                }
            }
            DataEntry::GroupArray { items, .. } => {
                for item in items.iter_mut() {
                    homogenize_value(item);
                }
                homogenize_numeric_siblings(items);
            }
            DataEntry::ObjectProperty { object, .. } => homogenize_value(object),
        }
    }
}

fn homogenize_value(value: &mut Value) {
    match value {
        Value::Array { values, .. } | Value::NestedArray { values, .. } => {
            for v in values.iter_mut() {
                homogenize_value(v);
            }
            homogenize_numeric_siblings(values);
        }
        Value::Object { properties, .. } => {
            for prop in properties.iter_mut() {
                homogenize_value(&mut prop.value);
            }
        }
        Value::PrefixedConstructor { arguments, .. } => {
            // Tuples are heterogeneous by design — recurse into each
            // element independently, but never homogenize across siblings.
            for arg in arguments.iter_mut() {
                homogenize_value(arg);
            }
        }
        Value::Range { start, end, .. } => {
            homogenize_value(start);
            homogenize_value(end);
        }
        _ => {}
    }
}

/// Numeric "rank" used to pick the common promoted type.
/// `None` means "not a numeric literal" — if ANY element in an array is
/// non-numeric, the whole array is left alone.
#[inline]
fn numeric_rank(value: &Value) -> Option<u8> {
    match value {
        Value::Integer { .. }            => Some(0),
        Value::Long { .. }               => Some(1),
        Value::Float { .. }              => Some(2),
        Value::Double { .. }             => Some(3),
        Value::ScientificNotation { .. } => Some(3),
        _ => None,
    }
}

fn promote(value: Value, target_rank: u8) -> Value {
    match (target_rank, value) {
        (3, Value::Integer { value: v, position }) => Value::Double { value: v as f64, position },
        (3, Value::Long    { value: v, position }) => Value::Double { value: v as f64, position },
        (3, Value::Float   { value: v, position }) => Value::Double { value: v as f64, position },
        (2, Value::Integer { value: v, position }) => Value::Float { value: v as f32, position },
        (2, Value::Long    { value: v, position }) => Value::Float { value: v as f32, position },
        (1, Value::Integer { value: v, position }) => Value::Long { value: v as i64, position },
        (_, other) => other,
    }
}

/// If every element of `values` is numeric and they don't all share the
/// same rank, promote the lower-rank elements up to the highest rank
/// present. No-op for arrays with fewer than 2 elements, all-Integer
/// arrays, or arrays containing any non-numeric element.
fn homogenize_numeric_siblings(values: &mut [Value]) {
    if values.len() < 2 {
        return;
    }

    let mut max_rank: u8 = 0;
    for v in values.iter() {
        match numeric_rank(v) {
            Some(r) => max_rank = max_rank.max(r),
            None    => return,
        }
    }

    if max_rank == 0 {
        return; // all plain Integer — nothing to promote
    }

    for v in values.iter_mut() {
        let current = numeric_rank(v).unwrap();
        if current < max_rank {
            let owned = std::mem::replace(v, Value::Null { position: Position::UNKNOWN });
            *v = promote(owned, max_rank);
        }
    }
}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler::AST::{DataEntry, DataSection, ObjectProperty, Position, PropertyAssignment, TablePath, Value};

    fn int(v: i32) -> Value { Value::Integer { value: v, position: Position::UNKNOWN } }
    fn dbl(v: f64) -> Value { Value::Double { value: v, position: Position::UNKNOWN } }
    fn long(v: i64) -> Value { Value::Long { value: v, position: Position::UNKNOWN } }
    fn s(v: &str) -> Value { Value::String { value: v.into(), position: Position::UNKNOWN } }

    fn ast_with(entries: Vec<DataEntry>) -> DixScript {
        DixScript {
            data: Some(DataSection { entries, position: Position::UNKNOWN }),
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None, raw: Vec::new(), schema: None,
        }
    }

    #[test]
    fn promotes_int_to_double_when_sibling_is_double() {
        let mut ast = ast_with(vec![DataEntry::SimpleProperty {
            name: "values".into(), data_type: None,
            value: Value::Array {
                values: vec![dbl(12.3), int(4), dbl(4.9)],
                position: Position::UNKNOWN,
            },
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::SimpleProperty { value: Value::Array { values, .. }, .. }) =
            ast.data.as_ref().unwrap().entries.first()
        {
            assert!(matches!(values[0], Value::Double { value, .. } if (value - 12.3).abs() < 1e-9));
            assert!(matches!(values[1], Value::Double { value, .. } if value == 4.0));
            assert!(matches!(values[2], Value::Double { value, .. } if (value - 4.9).abs() < 1e-9));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn leaves_all_int_array_untouched() {
        let mut ast = ast_with(vec![DataEntry::SimpleProperty {
            name: "ids".into(), data_type: None,
            value: Value::Array { values: vec![int(1), int(2), int(3)], position: Position::UNKNOWN },
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::SimpleProperty { value: Value::Array { values, .. }, .. }) =
            ast.data.as_ref().unwrap().entries.first()
        {
            assert!(values.iter().all(|v| matches!(v, Value::Integer { .. })));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn leaves_mixed_type_array_untouched() {
        let mut ast = ast_with(vec![DataEntry::SimpleProperty {
            name: "mixed".into(), data_type: None,
            value: Value::Array { values: vec![int(1), s("two"), dbl(3.0)], position: Position::UNKNOWN },
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::SimpleProperty { value: Value::Array { values, .. }, .. }) =
            ast.data.as_ref().unwrap().entries.first()
        {
            assert!(matches!(values[0], Value::Integer { .. }));
            assert!(matches!(values[1], Value::String { .. }));
            assert!(matches!(values[2], Value::Double { .. }));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn promotes_long_over_int_without_any_float() {
        let mut ast = ast_with(vec![DataEntry::SimpleProperty {
            name: "ids".into(), data_type: None,
            value: Value::Array { values: vec![int(1), long(9_000_000_000), int(3)], position: Position::UNKNOWN },
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::SimpleProperty { value: Value::Array { values, .. }, .. }) =
            ast.data.as_ref().unwrap().entries.first()
        {
            assert!(matches!(values[0], Value::Long { value, .. } if value == 1));
            assert!(matches!(values[1], Value::Long { value, .. } if value == 9_000_000_000));
            assert!(matches!(values[2], Value::Long { value, .. } if value == 3));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn recurses_into_nested_arrays_independently() {
        let inner_a = Value::Array { values: vec![int(1), int(2)], position: Position::UNKNOWN };
        let inner_b = Value::Array { values: vec![dbl(1.5), int(2)], position: Position::UNKNOWN };

        let mut ast = ast_with(vec![DataEntry::SimpleProperty {
            name: "matrix".into(), data_type: None,
            value: Value::NestedArray { values: vec![inner_a, inner_b], level: 2, position: Position::UNKNOWN },
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::SimpleProperty { value: Value::NestedArray { values, .. }, .. }) =
            ast.data.as_ref().unwrap().entries.first()
        {
            if let Value::Array { values: a, .. } = &values[0] {
                assert!(a.iter().all(|v| matches!(v, Value::Integer { .. })));
            } else { panic!("expected array"); }

            if let Value::Array { values: b, .. } = &values[1] {
                assert!(b.iter().all(|v| matches!(v, Value::Double { .. })));
            } else { panic!("expected array"); }
        } else {
            panic!("expected nested array");
        }
    }

    #[test]
    fn homogenizes_group_array_entry() {
        let mut ast = ast_with(vec![DataEntry::GroupArray {
            path: TablePath::new(vec!["scores".into()]),
            items: vec![int(10), dbl(9.5), int(8)],
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::GroupArray { items, .. }) = ast.data.as_ref().unwrap().entries.first() {
            assert!(items.iter().all(|v| matches!(v, Value::Double { .. })));
        } else {
            panic!("expected group array");
        }
    }

    #[test]
    fn homogenizes_table_property_values() {
        let mut ast = ast_with(vec![DataEntry::TableProperty {
            path: TablePath::new(vec!["stats".into()]),
            properties: vec![PropertyAssignment::new(
                "weights".into(), None,
                Value::Array { values: vec![int(1), dbl(2.5)], position: Position::UNKNOWN },
                Position::UNKNOWN,
            )],
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::TableProperty { properties, .. }) = ast.data.as_ref().unwrap().entries.first() {
            if let Value::Array { values, .. } = &properties[0].value {
                assert!(values.iter().all(|v| matches!(v, Value::Double { .. })));
            } else { panic!("expected array"); }
        } else {
            panic!("expected table property");
        }
    }

    #[test]
    fn homogenizes_object_property_field_arrays() {
        let mut ast = ast_with(vec![DataEntry::ObjectProperty {
            name: "cfg".into(), data_type: None,
            object: Box::new(Value::Object {
                properties: vec![ObjectProperty::new(
                    "ratios".into(),
                    Value::Array { values: vec![int(1), dbl(0.5)], position: Position::UNKNOWN },
                    Position::UNKNOWN,
                )],
                position: Position::UNKNOWN,
            }),
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::ObjectProperty { object, .. }) = ast.data.as_ref().unwrap().entries.first() {
            if let Value::Object { properties, .. } = object.as_ref() {
                if let Value::Array { values, .. } = &properties[0].value {
                    assert!(values.iter().all(|v| matches!(v, Value::Double { .. })));
                } else { panic!("expected array"); }
            } else { panic!("expected object"); }
        } else {
            panic!("expected object property");
        }
    }
          }

/// Differential tests of the arena walk against [`legacy`], the owned-tree implementation it
/// replaced. The inputs are generated: every flat array of up to four literals over eight
/// kinds in five containers, and several thousand random trees with arrays, nested arrays,
/// objects, tuples and ranges. Every node gets its own position, so a promotion that loses or
/// reuses a position shows up as a difference.
#[cfg(test)]
mod arena_tests {
    use super::*;
    use crate::Compiler::AST::{
        DataEntry, DataSection, ObjectProperty, Position, PropertyAssignment, TablePath, Value,
    };

    struct Pos(usize);
    impl Pos {
        fn next(&mut self) -> Position {
            self.0 += 1;
            Position::new(self.0, self.0 * 7 + 1)
        }
    }

    /// Eight kinds of literal: the five numeric ones, a string, a boolean and null. The integer
    /// and long values are ones that do not survive a round trip through `f32`, so a promotion
    /// that goes through the wrong width differs from the right one.
    const KINDS: usize = 8;

    fn leaf(kind: usize, pos: &mut Pos) -> Value {
        let position = pos.next();
        match kind {
            0 => Value::Integer { value: 16_777_217, position },
            1 => Value::Long { value: 9_007_199_254_740_993, position },
            2 => Value::Float { value: 0.1, position },
            3 => Value::Double { value: 2.5, position },
            4 => Value::ScientificNotation { value: 1.0e3, position },
            5 => Value::String { value: "s".into(), position },
            6 => Value::Boolean { value: true, position },
            _ => Value::Null { position },
        }
    }

    /// Every sequence of up to four kinds.
    fn sequences() -> Vec<Vec<usize>> {
        let mut all = vec![Vec::new()];
        let mut layer: Vec<Vec<usize>> = vec![Vec::new()];
        for _ in 0..4 {
            let mut next = Vec::new();
            for prefix in &layer {
                for kind in 0..KINDS {
                    let mut sequence = prefix.clone();
                    sequence.push(kind);
                    next.push(sequence);
                }
            }
            all.extend(next.iter().cloned());
            layer = next;
        }
        all
    }

    fn ast_with(entries: Vec<DataEntry>) -> DixScript {
        DixScript {
            data: Some(DataSection { entries, position: Position::UNKNOWN }),
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None, raw: Vec::new(), schema: None,
        }
    }

    /// The tree as the original implementation leaves it, as the owned entry point leaves it,
    /// and as the document entry point leaves it.
    fn run_all(ast: &DixScript) -> (DixScript, DixScript, DixScript) {
        let mut old = ast.clone();
        legacy::homogenize_data_section(&mut old);
        let mut new = ast.clone();
        homogenize_data_section(&mut new);
        let mut document = Document::from_owned(ast);
        homogenize_document(&mut document);
        (old, new, document.export())
    }

    struct Rng(u64);
    impl Rng {
        fn below(&mut self, n: usize) -> usize {
            // xorshift64*: deterministic, so a failure names a reproducible input.
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) as usize % n
        }
    }

    fn tree(rng: &mut Rng, pos: &mut Pos, depth: usize) -> Value {
        if depth == 0 || rng.below(3) == 0 {
            return leaf(rng.below(KINDS), pos);
        }
        let position = pos.next();
        match rng.below(6) {
            0 | 1 => Value::Array { values: children(rng, pos, depth, 5), position },
            2 => Value::NestedArray { values: children(rng, pos, depth, 5), level: 2, position },
            3 => {
                let mut properties = Vec::new();
                for index in 0..rng.below(4) {
                    let value = tree(rng, pos, depth - 1);
                    properties.push(ObjectProperty::new(format!("k{index}"), value, pos.next()));
                }
                Value::Object { properties, position }
            }
            4 => Value::PrefixedConstructor {
                prefix: "t".into(),
                arguments: children(rng, pos, depth, 5),
                position,
            },
            _ => {
                let start = Box::new(tree(rng, pos, depth - 1));
                let end = Box::new(tree(rng, pos, depth - 1));
                Value::Range { start, end, position }
            }
        }
    }

    fn children(rng: &mut Rng, pos: &mut Pos, depth: usize, max: usize) -> Vec<Value> {
        let count = rng.below(max);
        (0..count).map(|_| tree(rng, pos, depth - 1)).collect()
    }

    #[test]
    fn matches_the_original_on_every_flat_array_of_up_to_four_literals() {
        let mut pos = Pos(0);
        let (mut total, mut promoted) = (0usize, 0usize);
        for kinds in sequences() {
            let values = |pos: &mut Pos| -> Vec<Value> { kinds.iter().map(|k| leaf(*k, pos)).collect() };
            let array = Value::Array { values: values(&mut pos), position: pos.next() };
            let nested = Value::NestedArray { values: values(&mut pos), level: 2, position: pos.next() };
            let table_array = Value::Array { values: values(&mut pos), position: pos.next() };
            let object_array = Value::Array { values: values(&mut pos), position: pos.next() };
            let ast = ast_with(vec![
                DataEntry::SimpleProperty { name: "a".into(), data_type: None, value: array, position: pos.next() },
                DataEntry::SimpleProperty { name: "n".into(), data_type: None, value: nested, position: pos.next() },
                DataEntry::GroupArray {
                    path: TablePath::new(vec!["g".into()]),
                    items: values(&mut pos),
                    position: pos.next(),
                },
                DataEntry::TableProperty {
                    path: TablePath::new(vec!["t".into()]),
                    properties: vec![PropertyAssignment::new("w".into(), None, table_array, pos.next())],
                    position: pos.next(),
                },
                DataEntry::ObjectProperty {
                    name: "o".into(),
                    data_type: None,
                    object: Box::new(Value::Object {
                        properties: vec![ObjectProperty::new("r".into(), object_array, pos.next())],
                        position: pos.next(),
                    }),
                    position: pos.next(),
                },
            ]);
            let (old, new, via_document) = run_all(&ast);
            assert_eq!(old, new, "owned entry point differs for {kinds:?}");
            assert_eq!(old, via_document, "document entry point differs for {kinds:?}");
            total += 1;
            promoted += usize::from(old != ast);
        }
        assert_eq!(total, 4681);
        assert_eq!(promoted, 738, "how many inputs the original changes");
    }

    #[test]
    fn matches_the_original_on_random_nested_trees() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let mut pos = Pos(0);
        let (mut total, mut promoted) = (0usize, 0usize);
        for _ in 0..4000 {
            let mut entries = Vec::new();
            for index in 0..=rng.below(3) {
                let position = pos.next();
                entries.push(match rng.below(4) {
                    0 => DataEntry::SimpleProperty {
                        name: format!("s{index}"),
                        data_type: None,
                        value: tree(&mut rng, &mut pos, 4),
                        position,
                    },
                    1 => DataEntry::GroupArray {
                        path: TablePath::new(vec![format!("g{index}")]),
                        items: children(&mut rng, &mut pos, 4, 6),
                        position,
                    },
                    2 => DataEntry::TableProperty {
                        path: TablePath::new(vec![format!("t{index}")]),
                        properties: vec![
                            PropertyAssignment::new("a".into(), None, tree(&mut rng, &mut pos, 4), pos.next()),
                            PropertyAssignment::new("b".into(), None, tree(&mut rng, &mut pos, 4), pos.next()),
                        ],
                        position,
                    },
                    _ => DataEntry::ObjectProperty {
                        name: format!("o{index}"),
                        data_type: None,
                        object: Box::new(tree(&mut rng, &mut pos, 4)),
                        position,
                    },
                });
            }
            let ast = ast_with(entries);
            let (old, new, via_document) = run_all(&ast);
            assert_eq!(old, new, "owned entry point differs for {ast:?}");
            assert_eq!(old, via_document, "document entry point differs for {ast:?}");
            total += 1;
            promoted += usize::from(old != ast);
        }
        assert_eq!(total, 4000);
        assert_eq!(promoted, 891, "how many inputs the original changes");
    }

    #[test]
    fn reports_whether_anything_changed_and_shares_what_it_does_not_touch() {
        let mut pos = Pos(0);
        let ints = |pos: &mut Pos| Value::Array {
            values: vec![leaf(0, pos), leaf(0, pos)],
            position: pos.next(),
        };
        let mixed = |pos: &mut Pos| Value::Array {
            values: vec![leaf(0, pos), leaf(3, pos)],
            position: pos.next(),
        };
        let simple = |name: &str, value: Value, pos: &mut Pos| DataEntry::SimpleProperty {
            name: name.into(),
            data_type: None,
            value,
            position: pos.next(),
        };

        // Nothing to promote: no change reported, nothing allocated, the section is the same.
        let untouched = ast_with(vec![simple("a", ints(&mut pos), &mut pos), simple("b", ints(&mut pos), &mut pos)]);
        let mut document = Document::from_owned(&untouched);
        let before = document.arena.value_count();
        assert!(!homogenize_document(&mut document));
        assert_eq!(document.arena.value_count(), before, "a pass with nothing to promote allocates nothing");
        assert_eq!(document.export(), untouched);

        // One array to promote: the other entry keeps its node, and only the promoted element
        // and the rewritten array are allocated.
        let touched = ast_with(vec![simple("a", ints(&mut pos), &mut pos), simple("b", mixed(&mut pos), &mut pos)]);
        let mut document = Document::from_owned(&touched);
        let ids_before: Vec<_> = document
            .data
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .map(|entry| match entry {
                DocDataEntry::SimpleProperty { value, .. } => *value,
                _ => unreachable!("the test builds simple properties only"),
            })
            .collect();
        let before = document.arena.value_count();
        assert!(homogenize_document(&mut document));
        assert_eq!(document.arena.value_count(), before + 2);
        let ids_after: Vec<_> = document
            .data
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .map(|entry| match entry {
                DocDataEntry::SimpleProperty { value, .. } => *value,
                _ => unreachable!("the test builds simple properties only"),
            })
            .collect();
        assert_eq!(ids_after[0], ids_before[0], "the untouched entry keeps its node");
        assert_ne!(ids_after[1], ids_before[1]);
        assert!(!homogenize_document(&mut document), "a second pass finds nothing to promote");

        // No data section: nothing to do, in either form.
        let mut empty = Document::from_owned(&DixScript {
            data: None,
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None, raw: Vec::new(), schema: None,
        });
        assert!(!homogenize_document(&mut empty));
        let mut owned_empty = empty.export();
        homogenize_data_section(&mut owned_empty);
        assert_eq!(owned_empty, empty.export());
    }

    #[test]
    fn the_owned_entry_point_leaves_a_section_with_nothing_to_promote_exactly_as_it_was() {
        let mut pos = Pos(0);
        let mut ast = ast_with(vec![DataEntry::SimpleProperty {
            name: "a".into(),
            data_type: None,
            value: Value::Array { values: vec![leaf(0, &mut pos), leaf(5, &mut pos)], position: pos.next() },
            position: pos.next(),
        }]);
        let expected = ast.clone();
        homogenize_data_section(&mut ast);
        assert_eq!(ast, expected);
    }
}
