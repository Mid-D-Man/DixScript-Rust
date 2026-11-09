// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/ValueResolution/data_context.rs"
// ============================================================================
//! The data context of value resolution (phase 2), built over the arena AST.
//!
//! The context maps the path of every literal in `@DATA` (`DATA.server.port`,
//! `DATA.items[2]`) to its runtime value, so that a quickfunction call can name a data value
//! as an argument. [`populate_entry`] fills it from one entry of a [`DocData`], and
//! [`resolve_aliases_in_entry`] resolves the properties whose value is a plain identifier
//! (`tax = base_price`) once the literals are in. Both only read the arena.
//!
//! Insertion order matters. The context is a hash map whose iteration order decides which key
//! [`lookup_by_name_or_suffix`] finds first when a name matches the end of several paths, so
//! the entries are inserted in the order of the data section, depth first, exactly as the walk
//! over the owned tree did.

use std::collections::HashMap;

use super::value_resolver::MIN_CAPACITY;
use crate::Builtins::Core::DixValue;
use crate::Compiler::AST::arena::{Arena, DocDataEntry, ValueId, ValueNode};
use crate::Compiler::Utilities::PathBuilder;
use crate::Utilities::RustcHash::FxHashMap;

/// The data context: the path of a data value to its runtime value.
pub(crate) type Context = FxHashMap<String, DixValue>;

/// Inserts the literals of one data entry into `context` and returns how many values were
/// inserted.
///
/// An array is inserted as a whole only when every element converted, and an object is inserted
/// after its members, so both are present next to their parts.
///
/// # Panics
///
/// Panics if an id in `entry` is out of range for `arena`.
pub(crate) fn populate_entry(arena: &Arena, entry: &DocDataEntry, context: &mut Context) -> usize {
    match entry {
        DocDataEntry::SimpleProperty { name, value, .. } => {
            let path = PathBuilder::build(&[name.as_str()]);
            insert_value_recursive(arena, *value, &path, context)
        }

        DocDataEntry::TableProperty { path: table, properties, .. } => {
            let segments: Vec<&str> = table.segments.iter().map(|s| s.as_str()).collect();
            let mut count = 0usize;
            for prop in properties {
                let mut segs = segments.clone();
                segs.push(prop.name.as_str());
                let full = PathBuilder::build(&segs);
                count += insert_value_recursive(arena, prop.value, &full, context);
            }
            count
        }

        DocDataEntry::GroupArray { path: group, items, .. } => {
            let segments: Vec<&str> = group.segments.iter().map(|s| s.as_str()).collect();
            let base = PathBuilder::build(&segments);
            let mut count = 0usize;
            for (i, &item) in arena.value_list(*items).iter().enumerate() {
                let indexed = format!("{}[{}]", base, i);
                count += insert_value_recursive(arena, item, &indexed, context);
            }
            count
        }

        DocDataEntry::ObjectProperty { name, object, .. } => {
            let base = PathBuilder::build(&[name.as_str()]);
            insert_value_recursive(arena, *object, &base, context)
        }
    }
}

fn insert_value_recursive(arena: &Arena, id: ValueId, path: &str, context: &mut Context) -> usize {
    match *arena.value(id) {
        ValueNode::Object { properties, .. } => {
            let mut count = 0usize;
            for prop in arena.prop_list(properties) {
                let child = format!("{}.{}", path, arena.text(prop.key));
                count += insert_value_recursive(arena, prop.value, &child, context);
            }
            if let Some(dix) = try_value_to_dix(arena, id) {
                context.insert(path.to_string(), dix);
                count += 1;
            }
            count
        }

        ValueNode::Array { values, .. } | ValueNode::NestedArray { values, .. } => {
            let items = arena.value_list(values);
            let mut count = 0usize;
            let mut all_ok = true;
            for (i, &item) in items.iter().enumerate() {
                let idx = format!("{}[{}]", path, i);
                let inserted = insert_value_recursive(arena, item, &idx, context);
                if inserted == 0 {
                    all_ok = false;
                }
                count += inserted;
            }
            if all_ok && !items.is_empty() {
                if let Some(dix) = try_value_to_dix(arena, id) {
                    context.insert(path.to_string(), dix);
                    count += 1;
                }
            }
            count
        }

        _ => match try_value_to_dix(arena, id) {
            Some(dix) => {
                context.insert(path.to_string(), dix);
                1
            }
            None => 0,
        },
    }
}

/// Resolves the identifier aliases of one data entry against `context` and returns whether it
/// inserted anything.
///
/// An alias is a property whose value is a plain identifier naming another data value. It is
/// skipped when its own path is already in the context, and inserted when the name is found.
/// An object property is never an alias.
///
/// # Panics
///
/// Panics if an id in `entry` is out of range for `arena`.
pub(crate) fn resolve_aliases_in_entry(arena: &Arena, entry: &DocDataEntry, context: &mut Context) -> bool {
    match entry {
        DocDataEntry::SimpleProperty { name, value, .. } => {
            let path = PathBuilder::build(&[name.as_str()]);
            if context.contains_key(&path) {
                return false; // already resolved
            }
            if let Some(dix) = alias_target(arena, *value, context) {
                context.insert(path, dix);
                return true;
            }
            false
        }

        DocDataEntry::TableProperty { path: table, properties, .. } => {
            let segments: Vec<&str> = table.segments.iter().map(|s| s.as_str()).collect();
            let mut changed = false;

            for prop in properties {
                let mut segs = segments.clone();
                segs.push(prop.name.as_str());
                let full = PathBuilder::build(&segs);

                if context.contains_key(&full) {
                    continue; // already resolved
                }

                if let Some(dix) = alias_target(arena, prop.value, context) {
                    context.insert(full, dix);
                    changed = true;
                }
            }

            changed
        }

        DocDataEntry::GroupArray { path: group, items, .. } => {
            let segments: Vec<&str> = group.segments.iter().map(|s| s.as_str()).collect();
            let base = PathBuilder::build(&segments);
            let mut changed = false;

            for (i, &item) in arena.value_list(*items).iter().enumerate() {
                let indexed = format!("{}[{}]", base, i);
                if context.contains_key(&indexed) {
                    continue;
                }
                if let Some(dix) = alias_target(arena, item, context) {
                    context.insert(indexed, dix);
                    changed = true;
                }
            }

            changed
        }

        DocDataEntry::ObjectProperty { .. } => false,
    }
}

/// The value `id` aliases, when it is a plain identifier the context can name.
fn alias_target(arena: &Arena, id: ValueId, context: &Context) -> Option<DixValue> {
    match *arena.value(id) {
        ValueNode::Identifier { value, .. } => lookup_by_name_or_suffix(arena.text(value), context),
        _ => None,
    }
}

/// Looks `name` up in `context` by exact key first, then by path suffix (`"port"` matches
/// `"DATA.server.config.port"`). Returns a clone of the first match, or `None`.
fn lookup_by_name_or_suffix(name: &str, context: &Context) -> Option<DixValue> {
    if let Some(v) = context.get(name) {
        return Some(v.clone());
    }

    let suffix = format!(".{}", name);
    context.iter().find(|(k, _)| k.ends_with(&suffix)).map(|(_, v)| v.clone())
}

/// Converts a literal to its runtime value, or `None` when it is not a literal (a call, an
/// expression, an unresolved identifier) or a part of it is not.
///
/// A date or timestamp that does not parse becomes the current time, an enum value becomes `0`
/// (the enum pass has already checked it), and a prefixed constructor converts only for the
/// `b` (blob), `r` (regex) and `t` (tuple, at most six elements) prefixes.
fn try_value_to_dix(arena: &Arena, id: ValueId) -> Option<DixValue> {
    match *arena.value(id) {
        ValueNode::Integer { value, .. } => Some(DixValue::from_int(value)),
        ValueNode::Long { value, .. } => Some(DixValue::from_long(value)),
        ValueNode::Float { value, .. } => Some(DixValue::from_float(value)),
        ValueNode::Double { value, .. } => Some(DixValue::from_double(value)),
        ValueNode::ScientificNotation { value, .. } => Some(DixValue::from_double(value)),
        ValueNode::String { value, .. } => Some(DixValue::from_string(arena.text(value).to_string())),
        ValueNode::Boolean { value, .. } => Some(DixValue::from_bool(value)),
        ValueNode::Null { .. } => Some(DixValue::null()),
        ValueNode::HexColor { value, .. } => Some(DixValue::from_hex(arena.text(value).to_string())),

        ValueNode::Date { value, .. } => {
            let d = arena.text(value);
            let dt = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .ok()
                .and_then(|nd| nd.and_hms_opt(0, 0, 0))
                .map(|ndt| ndt.and_utc())
                .or_else(|| d.parse::<chrono::DateTime<chrono::Utc>>().ok())
                .unwrap_or_else(chrono::Utc::now);
            Some(DixValue::from_date(dt))
        }
        ValueNode::Timestamp { value, .. } => {
            let dt = arena
                .text(value)
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap_or_else(|_| chrono::Utc::now());
            Some(DixValue::from_timestamp(dt))
        }

        // The template text; the interpolation itself happens later.
        ValueNode::InterpolatedString { template, .. } => {
            Some(DixValue::from_string(arena.text(template).to_string()))
        }

        ValueNode::EnumValue { .. } => Some(DixValue::from_int(0)),

        ValueNode::PrefixedConstructor { prefix, arguments, .. } => {
            let arguments = arena.value_list(arguments);
            // The first argument when it is a string.
            let first_string = || {
                arguments.first().and_then(|&a| match *arena.value(a) {
                    ValueNode::String { value, .. } => Some(arena.text(value).to_string()),
                    _ => None,
                })
            };
            match arena.text(prefix).to_lowercase().as_str() {
                "b" => DixValue::from_blob(first_string().unwrap_or_default()).ok(),
                "r" => DixValue::from_regex(first_string().unwrap_or_else(|| ".*".to_string())).ok(),
                "t" => {
                    let items: Option<Vec<DixValue>> =
                        arguments.iter().take(6).map(|&a| try_value_to_dix(arena, a)).collect();
                    items.map(DixValue::from_tuple)
                }
                _ => None,
            }
        }

        ValueNode::Array { values, .. } | ValueNode::NestedArray { values, .. } => {
            let items: Option<Vec<DixValue>> =
                arena.value_list(values).iter().map(|&v| try_value_to_dix(arena, v)).collect();
            items.map(DixValue::from_array)
        }

        ValueNode::Object { properties, .. } => {
            let properties = arena.prop_list(properties);
            let mut map = HashMap::with_capacity(properties.len().max(MIN_CAPACITY));
            for prop in properties {
                map.insert(arena.text(prop.key).to_string(), try_value_to_dix(arena, prop.value)?);
            }
            Some(DixValue::from_object(map))
        }

        _ => None,
    }
}
