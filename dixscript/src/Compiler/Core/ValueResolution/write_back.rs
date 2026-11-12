// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/ValueResolution/write_back.rs"
// ============================================================================
//! Writing the result of a resolved call back into the arena data section (phase 4).
//!
//! Phase 4 runs every quickfunction call found in `@DATA` and replaces the call with its result.
//! The data section of a [`DocData`] is read-only at the node level (nodes are shared and the
//! arena only grows), so a replacement allocates the new leaf and then a copy of every node on
//! the way down to it, and points the entry at the copy. Everything the path does not touch is
//! shared with the old tree.
//!
//! A call is located by a path of child indices (see [`Path`]). [`index_call_targets`] finds the
//! first node at each call position once, and [`replace_value_at`] follows that path. If the
//! node there no longer has the position, because an earlier replacement changed the tree, it
//! falls back to a search for the first node with the position, so the result is the same as
//! searching the whole tree every time.
//!
//! Positions are not unique. A node can share its position with a call inside it, or with a
//! node in another entry. The search order and the "first node wins" rule are the behaviour
//! the owned tree had and are kept as they were; `tests/value_resolver_writeback_document.rs`
//! pins them.

use std::collections::{HashMap, HashSet};

use crate::Builtins::Core::{DixType, DixValue};
use crate::Compiler::AST::arena::{Arena, DocData, DocDataEntry, ObjectPropNode, ValueId, ValueList, ValueNode};
use crate::Compiler::AST::Position;

/// Where a node sits in a data section: the entry index, then the index of the property (table)
/// or item (group array) when the entry has several values, then at every level below the
/// index of the array element, object property or constructor argument to step into.
pub(crate) type Path = Vec<u32>;

/// The path to the first node, depth first, at each position in `targets`.
///
/// Entries are visited in order and the search stops once every target is found. A node is
/// matched before its children.
///
/// # Panics
///
/// Panics if an id in `data` is out of range for `arena`.
pub(crate) fn index_call_targets(
    arena: &Arena,
    data: &DocData,
    targets: &HashSet<Position>,
) -> HashMap<Position, Path> {
    let mut found = HashMap::with_capacity(targets.len());
    let mut path: Path = Vec::with_capacity(16);
    for (i, entry) in data.entries.iter().enumerate() {
        if found.len() == targets.len() {
            break;
        }
        path.push(i as u32);
        index_entry(arena, entry, targets, &mut path, &mut found);
        path.pop();
    }
    found
}

fn index_entry(
    arena: &Arena,
    entry: &DocDataEntry,
    targets: &HashSet<Position>,
    path: &mut Path,
    found: &mut HashMap<Position, Path>,
) {
    match entry {
        DocDataEntry::SimpleProperty { value, .. } => index_value(arena, *value, targets, path, found),
        DocDataEntry::TableProperty { properties, .. } => {
            for (i, prop) in properties.iter().enumerate() {
                path.push(i as u32);
                index_value(arena, prop.value, targets, path, found);
                path.pop();
            }
        }
        DocDataEntry::GroupArray { items, .. } => {
            for (i, &item) in arena.value_list(*items).iter().enumerate() {
                path.push(i as u32);
                index_value(arena, item, targets, path, found);
                path.pop();
            }
        }
        DocDataEntry::ObjectProperty { object, .. } => index_value(arena, *object, targets, path, found),
    }
}

fn index_value(
    arena: &Arena,
    id: ValueId,
    targets: &HashSet<Position>,
    path: &mut Path,
    found: &mut HashMap<Position, Path>,
) {
    let node = *arena.value(id);
    let pos = node.position();
    if targets.contains(&pos) && !found.contains_key(&pos) {
        found.insert(pos, path.clone());
    }
    match node {
        ValueNode::Array { values, .. } | ValueNode::NestedArray { values, .. } => {
            index_list(arena, values, targets, path, found);
        }
        ValueNode::Object { properties, .. } => {
            for (i, prop) in arena.prop_list(properties).iter().enumerate() {
                path.push(i as u32);
                index_value(arena, prop.value, targets, path, found);
                path.pop();
            }
        }
        ValueNode::PrefixedConstructor { arguments, .. } => index_list(arena, arguments, targets, path, found),
        _ => {}
    }
}

fn index_list(
    arena: &Arena,
    list: ValueList,
    targets: &HashSet<Position>,
    path: &mut Path,
    found: &mut HashMap<Position, Path>,
) {
    for (i, &item) in arena.value_list(list).iter().enumerate() {
        path.push(i as u32);
        index_value(arena, item, targets, path, found);
        path.pop();
    }
}

/// The node a path points at, or `None` if the tree no longer has that shape.
fn slot_at_path(arena: &Arena, data: &DocData, path: &[u32]) -> Option<ValueId> {
    let (&first, rest) = path.split_first()?;
    let (mut id, rest) = match data.entries.get(first as usize)? {
        DocDataEntry::SimpleProperty { value, .. } => (*value, rest),
        DocDataEntry::ObjectProperty { object, .. } => (*object, rest),
        DocDataEntry::TableProperty { properties, .. } => {
            let (&i, rest) = rest.split_first()?;
            (properties.get(i as usize)?.value, rest)
        }
        DocDataEntry::GroupArray { items, .. } => {
            let (&i, rest) = rest.split_first()?;
            (*arena.value_list(*items).get(i as usize)?, rest)
        }
    };
    for &i in rest {
        id = match *arena.value(id) {
            ValueNode::Array { values, .. } | ValueNode::NestedArray { values, .. } => {
                *arena.value_list(values).get(i as usize)?
            }
            ValueNode::Object { properties, .. } => arena.prop_list(properties).get(i as usize)?.value,
            ValueNode::PrefixedConstructor { arguments, .. } => *arena.value_list(arguments).get(i as usize)?,
            _ => return None,
        };
    }
    Some(id)
}

/// The path to the first node, depth first and parent before children, whose position is
/// `target`.
fn find_first_path(arena: &Arena, data: &DocData, target: Position) -> Option<Path> {
    let mut path: Path = Vec::with_capacity(16);
    for (i, entry) in data.entries.iter().enumerate() {
        path.push(i as u32);
        let hit = match entry {
            DocDataEntry::SimpleProperty { value, .. } => find_in_value(arena, *value, target, &mut path),
            DocDataEntry::ObjectProperty { object, .. } => find_in_value(arena, *object, target, &mut path),
            DocDataEntry::TableProperty { properties, .. } => {
                properties.iter().enumerate().any(|(j, prop)| {
                    path.push(j as u32);
                    let hit = find_in_value(arena, prop.value, target, &mut path);
                    if !hit {
                        path.pop();
                    }
                    hit
                })
            }
            DocDataEntry::GroupArray { items, .. } => find_in_list(arena, *items, target, &mut path),
        };
        if hit {
            return Some(path);
        }
        path.pop();
    }
    None
}

/// Whether `target` is at or under `id`; on a hit `path` ends at the node, on a miss it is as
/// it was.
fn find_in_value(arena: &Arena, id: ValueId, target: Position, path: &mut Path) -> bool {
    let node = *arena.value(id);
    if node.position() == target {
        return true;
    }
    match node {
        ValueNode::Array { values, .. } | ValueNode::NestedArray { values, .. } => {
            find_in_list(arena, values, target, path)
        }
        ValueNode::Object { properties, .. } => arena.prop_list(properties).iter().enumerate().any(|(i, prop)| {
            path.push(i as u32);
            let hit = find_in_value(arena, prop.value, target, path);
            if !hit {
                path.pop();
            }
            hit
        }),
        ValueNode::PrefixedConstructor { arguments, .. } => find_in_list(arena, arguments, target, path),
        _ => false,
    }
}

fn find_in_list(arena: &Arena, list: ValueList, target: Position, path: &mut Path) -> bool {
    arena.value_list(list).iter().enumerate().any(|(i, &item)| {
        path.push(i as u32);
        let hit = find_in_value(arena, item, target, path);
        if !hit {
            path.pop();
        }
        hit
    })
}

/// Replaces the node at `path` with `leaf`, allocating a copy of every container on the way and
/// pointing the entry at the copy. Containers keep their kind, level and position.
///
/// # Panics
///
/// Panics if `path` does not lead to a node, so it must come from [`slot_at_path`] or
/// [`find_first_path`] on the same tree.
fn rewrite_at(arena: &mut Arena, data: &mut DocData, path: &[u32], leaf: ValueId) {
    let (&first, rest) = path.split_first().expect("a path starts with an entry index");
    match &mut data.entries[first as usize] {
        DocDataEntry::SimpleProperty { value, .. } => *value = rebuild(arena, *value, rest, leaf),
        DocDataEntry::ObjectProperty { object, .. } => *object = rebuild(arena, *object, rest, leaf),
        DocDataEntry::TableProperty { properties, .. } => {
            let (&i, rest) = rest.split_first().expect("a table path names a property");
            let prop = &mut properties[i as usize];
            prop.value = rebuild(arena, prop.value, rest, leaf);
        }
        DocDataEntry::GroupArray { items, .. } => {
            let (&i, rest) = rest.split_first().expect("a group path names an item");
            *items = with_child(arena, *items, i as usize, rest, leaf);
        }
    }
}

/// A copy of `id` whose descendant at `path` is `leaf`; `leaf` itself when `path` is empty.
fn rebuild(arena: &mut Arena, id: ValueId, path: &[u32], leaf: ValueId) -> ValueId {
    let Some((&i, rest)) = path.split_first() else {
        return leaf;
    };
    let i = i as usize;
    let node = *arena.value(id);
    match node {
        ValueNode::Array { values, position } => {
            let values = with_child(arena, values, i, rest, leaf);
            arena.alloc_value(ValueNode::Array { values, position })
        }
        ValueNode::NestedArray { values, level, position } => {
            let values = with_child(arena, values, i, rest, leaf);
            arena.alloc_value(ValueNode::NestedArray { values, level, position })
        }
        ValueNode::Object { properties, position } => {
            let mut props = arena.prop_list(properties).to_vec();
            props[i].value = rebuild(arena, props[i].value, rest, leaf);
            let properties = arena.alloc_prop_list(&props);
            arena.alloc_value(ValueNode::Object { properties, position })
        }
        ValueNode::PrefixedConstructor { prefix, arguments, position } => {
            let arguments = with_child(arena, arguments, i, rest, leaf);
            arena.alloc_value(ValueNode::PrefixedConstructor { prefix, arguments, position })
        }
        _ => unreachable!("a path only steps into arrays, objects and constructors"),
    }
}

/// A new list equal to `list` except that element `i` is rebuilt along `rest`.
fn with_child(arena: &mut Arena, list: ValueList, i: usize, rest: &[u32], leaf: ValueId) -> ValueList {
    let mut ids = arena.value_list(list).to_vec();
    ids[i] = rebuild(arena, ids[i], rest, leaf);
    arena.alloc_value_list(&ids)
}

/// Replaces the node at `target` with the value `result`, converted at the position `target`.
///
/// Uses the path from [`index_call_targets`] and checks that the node there still has the
/// target position. If an earlier replacement changed the tree under the index, it searches for
/// the first node with the position instead. A position that was not in the tree when the
/// index was built is left alone, as a search would not find it either.
///
/// Returns whether the data section was changed. Nothing is allocated when it was not.
///
/// # Panics
///
/// Panics if an id in `data` is out of range for `arena`.
pub(crate) fn replace_value_at(
    arena: &mut Arena,
    data: &mut DocData,
    index: &HashMap<Position, Path>,
    target: Position,
    result: &DixValue,
) -> bool {
    let Some(indexed) = index.get(&target) else {
        return false;
    };
    let searched: Path;
    let path: &[u32] = match slot_at_path(arena, data, indexed) {
        Some(id) if arena.value(id).position() == target => indexed,
        _ => match find_first_path(arena, data, target) {
            Some(found) => {
                searched = found;
                &searched
            }
            None => return false,
        },
    };
    let leaf = alloc_dix_value(arena, result, target);
    rewrite_at(arena, data, path, leaf);
    true
}

/// Allocates the AST node for a runtime value, every node at `position`.
///
/// Scalars become the literal of their type, a blob and a regex become `b:(...)` and `r:(...)`
/// constructors, an array stays an array and a tuple becomes a `t:(...)` constructor so that a
/// later pass can read its elements again, and an object becomes an object literal with its
/// properties in the order the runtime map yields them. A type with no literal form (an enum
/// has already become an integer by now) becomes a string.
pub(crate) fn alloc_dix_value(arena: &mut Arena, dix: &DixValue, position: Position) -> ValueId {
    let node = match dix.get_type() {
        DixType::Int => ValueNode::Integer { value: dix.as_int(), position },
        DixType::Long => ValueNode::Long { value: dix.as_long(), position },
        DixType::Float => ValueNode::Float { value: dix.as_float(), position },
        DixType::Double => ValueNode::Double { value: dix.as_double(), position },
        DixType::String => ValueNode::String { value: arena.alloc_str(&dix.as_string()), position },
        DixType::Bool => ValueNode::Boolean { value: dix.as_bool(), position },
        DixType::Null => ValueNode::Null { position },
        DixType::Hex => ValueNode::HexColor { value: arena.alloc_str(&dix.as_string()), position },
        DixType::Blob => {
            let encoded = dix.as_blob_base64().unwrap_or_default();
            constructor(arena, "b", &encoded, position)
        }
        DixType::Regex => constructor(arena, "r", &dix.as_string(), position),
        DixType::Array => {
            let ids: Vec<ValueId> =
                dix.as_array().iter().map(|item| alloc_dix_value(arena, item, position)).collect();
            ValueNode::Array { values: arena.alloc_value_list(&ids), position }
        }
        DixType::Tuple => {
            let ids: Vec<ValueId> =
                dix.as_array().iter().map(|item| alloc_dix_value(arena, item, position)).collect();
            let prefix = arena.alloc_str("t");
            ValueNode::PrefixedConstructor { prefix, arguments: arena.alloc_value_list(&ids), position }
        }
        DixType::Object => {
            let props: Vec<ObjectPropNode> = dix
                .as_object()
                .iter()
                .map(|(key, val)| ObjectPropNode {
                    key: arena.alloc_str(key),
                    value: alloc_dix_value(arena, val, position),
                    position,
                })
                .collect();
            ValueNode::Object { properties: arena.alloc_prop_list(&props), position }
        }
        DixType::Date => ValueNode::Date { value: arena.alloc_str(&dix.as_string()), position },
        DixType::Timestamp => ValueNode::Timestamp { value: arena.alloc_str(&dix.as_string()), position },
        _ => ValueNode::String { value: arena.alloc_str(&dix.as_string()), position },
    };
    arena.alloc_value(node)
}

/// The constructor `prefix:("text")`, every node at `position`.
fn constructor(arena: &mut Arena, prefix: &str, text: &str, position: Position) -> ValueNode {
    let text = arena.alloc_str(text);
    let argument = arena.alloc_value(ValueNode::String { value: text, position });
    let prefix = arena.alloc_str(prefix);
    let arguments = arena.alloc_value_list(&[argument]);
    ValueNode::PrefixedConstructor { prefix, arguments, position }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::Compiler::AST::{DataEntry, DataSection, DataType, ObjectProperty, PropertyAssignment, TablePath, Value};
    use crate::Compiler::Core::ValueResolution::ValueResolver;

    fn at(n: usize) -> Position {
        Position { line: n, column: 1 }
    }

    fn int(n: usize) -> Value {
        Value::Integer { value: n as i32, position: at(n) }
    }

    fn simple(name: &str, value: Value) -> DataEntry {
        DataEntry::SimpleProperty { name: name.into(), data_type: None, value, position: at(900) }
    }

    fn property(name: &str, value: Value) -> ObjectProperty {
        ObjectProperty { key: name.into(), value, position: at(901) }
    }

    /// Every node has the number of its position, so a path names the node it leads to.
    /// The paths are in `PATHS`. Position 20 is an array and its first element, 21 is the
    /// second element and also a plain node of a later entry, and 6 is the first element of
    /// an array and also a plain node of the last entry: the first of them in depth first
    /// order is the one a path leads to.
    fn tree() -> (Arena, DocData) {
        let entries = vec![
            simple("a", int(1)),
            DataEntry::TableProperty {
                path: TablePath::new(vec!["t".into()]),
                properties: vec![
                    PropertyAssignment::new("k".into(), None, int(2), at(902)),
                    PropertyAssignment::new("m".into(), None, int(3), at(903)),
                ],
                position: at(904),
            },
            DataEntry::GroupArray {
                path: TablePath::new(vec!["rows".into()]),
                items: vec![
                    int(4),
                    Value::Array {
                        values: vec![
                            int(6),
                            Value::Object { properties: vec![property("x", int(8))], position: at(7) },
                        ],
                        position: at(5),
                    },
                ],
                position: at(905),
            },
            DataEntry::ObjectProperty {
                name: "o".into(),
                data_type: Some(DataType::Object),
                object: Box::new(Value::Object { properties: vec![property("y", int(10))], position: at(9) }),
                position: at(906),
            },
            simple("c", Value::PrefixedConstructor { prefix: "t".into(), arguments: vec![int(12)], position: at(11) }),
            simple("n", Value::NestedArray { values: vec![int(14)], level: 1, position: at(13) }),
            simple("s", Value::Array { values: vec![int(20), int(21)], position: at(20) }),
            simple("d", int(21)),
            simple("e", int(6)),
        ];
        let mut arena = Arena::default();
        let data = DocData::from_owned(&mut arena, &DataSection::new(entries, at(1)));
        (arena, data)
    }

    const PATHS: &[(usize, &[u32])] = &[
        (1, &[0]),
        (2, &[1, 0]),
        (3, &[1, 1]),
        (4, &[2, 0]),
        (5, &[2, 1]),
        (6, &[2, 1, 0]),
        (7, &[2, 1, 1]),
        (8, &[2, 1, 1, 0]),
        (9, &[3]),
        (10, &[3, 0]),
        (11, &[4]),
        (12, &[4, 0]),
        (13, &[5]),
        (14, &[5, 0]),
        (20, &[6]),
        (21, &[6, 1]),
    ];

    #[test]
    fn a_path_leads_to_the_first_node_with_the_position_in_every_kind_of_place() {
        let (arena, data) = tree();
        let targets: HashSet<Position> = PATHS.iter().map(|(n, _)| at(*n)).collect();
        let index = index_call_targets(&arena, &data, &targets);
        assert_eq!(index.len(), PATHS.len());
        for (n, path) in PATHS {
            assert_eq!(index.get(&at(*n)), Some(&path.to_vec()), "index of {n}");
            let slot = slot_at_path(&arena, &data, path).unwrap_or_else(|| panic!("no node at the path of {n}"));
            assert_eq!(arena.value(slot).position(), at(*n), "node at the path of {n}");
            assert_eq!(find_first_path(&arena, &data, at(*n)), Some(path.to_vec()), "search for {n}");
        }
    }

    #[test]
    fn a_position_or_path_that_is_not_there_finds_nothing() {
        let (arena, data) = tree();
        let targets: HashSet<Position> = [at(500)].into_iter().collect();
        assert!(index_call_targets(&arena, &data, &targets).is_empty());
        assert_eq!(find_first_path(&arena, &data, at(500)), None);
        for path in [&[][..], &[99], &[0, 0], &[1, 5], &[2, 9], &[2, 1, 1, 5]] {
            assert_eq!(slot_at_path(&arena, &data, path), None, "{path:?}");
        }
    }

    #[test]
    fn a_stale_path_is_searched_for_and_never_trusted() {
        let (mut arena, mut data) = tree();
        let targets: HashSet<Position> = [at(5), at(6)].into_iter().collect();
        let index = index_call_targets(&arena, &data, &targets);
        // Position 5 is the array; its result is an array whose elements all have position 5.
        let array = DixValue::from_array(vec![DixValue::from_int(1), DixValue::from_int(2)]);
        assert!(replace_value_at(&mut arena, &mut data, &index, at(5), &array));
        // The path of 6 now leads into that array, to an element with position 5. The first node
        // with position 6 is the plain one in the last entry, and that is the one that changes.
        let before = format!("{:?}", data.export(&arena));
        assert!(replace_value_at(&mut arena, &mut data, &index, at(6), &DixValue::from_int(99)));
        let after = format!("{:?}", data.export(&arena));
        assert_ne!(before, after);
        let last = data.entries.len() - 1;
        let DocDataEntry::SimpleProperty { value, .. } = &data.entries[last] else { panic!("a simple entry") };
        assert_eq!(arena.value(*value).position(), at(6));
        assert!(matches!(arena.value(*value), ValueNode::Integer { value: 99, .. }));
        // A result keeps the position it replaced, so a later write for 6 finds that node again.
        assert!(replace_value_at(&mut arena, &mut data, &index, at(6), &DixValue::from_int(7)));
        let DocDataEntry::SimpleProperty { value, .. } = &data.entries[last] else { panic!("a simple entry") };
        assert!(matches!(arena.value(*value), ValueNode::Integer { value: 7, .. }));
        // Nothing has position 500, so a write for it changes nothing.
        let settled = format!("{:?}", data.export(&arena));
        assert!(!replace_value_at(&mut arena, &mut data, &index, at(500), &DixValue::from_int(1)));
        assert_eq!(format!("{:?}", data.export(&arena)), settled);
    }

    /// One value of every kind the runtime has, with arrays and objects nested.
    fn values() -> Vec<DixValue> {
        let day = Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 58).unwrap();
        let mut inner = HashMap::new();
        inner.insert("k".to_string(), DixValue::from_array(vec![DixValue::from_int(1), DixValue::null()]));
        inner.insert("t".to_string(), DixValue::from_tuple(vec![DixValue::from_bool(true)]));
        let mut outer = HashMap::new();
        outer.insert("a".to_string(), DixValue::from_object(inner));
        outer.insert("b".to_string(), DixValue::from_string("text".into()));
        vec![
            DixValue::from_int(-7),
            DixValue::from_long(9_000_000_000),
            DixValue::from_float(1.5),
            DixValue::from_double(2.25),
            DixValue::from_string("hello".into()),
            DixValue::from_bool(true),
            DixValue::from_bool(false),
            DixValue::null(),
            DixValue::from_hex("#FF5733".into()),
            DixValue::from_blob("QUJD".into()).unwrap(),
            DixValue::from_regex("^a+$".into()).unwrap(),
            DixValue::from_date(day),
            DixValue::from_timestamp(day),
            DixValue::from_array(vec![]),
            DixValue::from_array(vec![DixValue::from_int(1), DixValue::from_string("x".into()), DixValue::null()]),
            DixValue::from_tuple(vec![DixValue::from_int(1), DixValue::from_int(2)]),
            DixValue::from_object(outer),
        ]
    }

    #[test]
    fn the_arena_conversion_agrees_with_the_owned_one() {
        for dix in values() {
            let mut arena = Arena::default();
            let id = alloc_dix_value(&mut arena, &dix, at(3));
            let owned = ValueResolver::convert_dix_value_to_value(&dix, at(3));
            assert_eq!(format!("{:?}", arena.export_value(id)), format!("{owned:?}"), "{dix:?}");
        }
    }
}
