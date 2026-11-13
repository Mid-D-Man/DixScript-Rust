// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/ValueResolution/identifier_resolution.rs"
// ============================================================================
//! Resolving the identifiers left in the data section (phase 5), over the arena.
//!
//! After the calls have been run, an identifier whose text is a key of the data context (a full
//! path such as `DATA.x`) is replaced by the value stored under that key. [`resolve_pass`] does
//! one pass over the data section; the caller repeats it until a pass resolves nothing, adding
//! the paths of the identifiers a pass resolved to the context between passes, so a chain of
//! identifiers resolves one link per pass.
//!
//! The data section is rewritten by allocation, as in `enum_resolution`: a value that holds no
//! resolved identifier is the same id afterwards, and an array, object or constructor that holds
//! one is a new node over a new list that shares the elements that did not change. A nested array
//! that holds a resolved identifier comes back as a plain array; the level is dropped, as it
//! always was.
//!
//! The path an identifier sits at is what the context is keyed by afterwards, so it has to be
//! exact: `DATA.t.k` for a table property, `DATA.rows[1]` for a group item, `DATA.o.y` for an
//! object property, `DATA.c.__arg0` for a constructor argument and `[i]` for an element. It is
//! built only when an identifier resolves, which is rare, instead of for every node visited.

use std::fmt::Write as _;

use super::write_back::alloc_dix_value;
use crate::Builtins::Core::DixValue;
use crate::Compiler::AST::arena::{Arena, DocData, DocDataEntry, ObjectPropNode, Str, ValueId, ValueList, ValueNode};
use crate::Compiler::Utilities::PathBuilder;
use crate::Utilities::RustcHash::FxHashMap;

/// One step below the entry's own path.
#[derive(Clone, Copy)]
enum Step {
    /// An array element or a group item: `[i]`.
    Index(usize),
    /// An object property: `.key`.
    Key(Str),
    /// A constructor argument: `.__argN`.
    Arg(usize),
}

/// Where the walk is: the segments of the entry's path, and the steps taken below it.
struct Site<'a> {
    root: &'a [&'a str],
    steps: Vec<Step>,
}

impl Site<'_> {
    fn render(&self, arena: &Arena) -> String {
        let mut path = PathBuilder::build(self.root);
        for step in &self.steps {
            match *step {
                Step::Index(i) => {
                    let _ = write!(path, "[{i}]");
                }
                Step::Key(key) => {
                    path.push('.');
                    path.push_str(arena.text(key));
                }
                Step::Arg(i) => {
                    let _ = write!(path, ".__arg{i}");
                }
            }
        }
        path
    }
}

/// What one pass resolved and left, and where the resolved identifiers sat.
struct Walk<'a> {
    ctx: &'a FxHashMap<String, DixValue>,
    newly_resolved: &'a mut Vec<(String, DixValue)>,
}

/// One pass over the data section. Returns how many identifiers were resolved and how many were
/// left because the context has no key for them.
///
/// `newly_resolved` gets the path and value of every identifier resolved, in the order they were
/// met; the caller adds them to the context after the pass, so that within one pass every
/// identifier sees the same context.
///
/// # Panics
///
/// Panics if an id in `data` is out of range for `arena`.
pub(crate) fn resolve_pass(
    arena: &mut Arena,
    data: &mut DocData,
    ctx: &FxHashMap<String, DixValue>,
    newly_resolved: &mut Vec<(String, DixValue)>,
) -> (usize, usize) {
    let mut walk = Walk { ctx, newly_resolved };
    let (mut resolved, mut skipped) = (0, 0);
    for entry in &mut data.entries {
        let (res, skip) = resolve_entry(arena, entry, &mut walk);
        resolved += res;
        skipped += skip;
    }
    (resolved, skipped)
}

fn resolve_entry(arena: &mut Arena, entry: &mut DocDataEntry, walk: &mut Walk) -> (usize, usize) {
    match entry {
        DocDataEntry::SimpleProperty { name, value, .. } => {
            let root = [name.as_str()];
            let mut site = Site { root: &root, steps: Vec::new() };
            let (new, res, skip) = resolve_value(arena, *value, &mut site, walk);
            *value = new;
            (res, skip)
        }
        DocDataEntry::ObjectProperty { name, object, .. } => {
            let root = [name.as_str()];
            let mut site = Site { root: &root, steps: Vec::new() };
            let (new, res, skip) = resolve_value(arena, *object, &mut site, walk);
            *object = new;
            (res, skip)
        }
        DocDataEntry::TableProperty { path, properties, .. } => {
            // The path of a property is the table's segments and the property's name.
            let mut root: Vec<&str> = path.segments.iter().map(String::as_str).collect();
            root.push("");
            let (mut total_res, mut total_skip) = (0, 0);
            for prop in properties.iter_mut() {
                *root.last_mut().expect("the root has the property's name") = prop.name.as_str();
                let mut site = Site { root: &root, steps: Vec::new() };
                let (new, res, skip) = resolve_value(arena, prop.value, &mut site, walk);
                prop.value = new;
                total_res += res;
                total_skip += skip;
            }
            (total_res, total_skip)
        }
        DocDataEntry::GroupArray { path, items, .. } => {
            let root: Vec<&str> = path.segments.iter().map(String::as_str).collect();
            let mut site = Site { root: &root, steps: Vec::new() };
            let (list, res, skip) = resolve_list(arena, *items, &mut site, walk, Step::Index);
            *items = list;
            (res, skip)
        }
    }
}

/// The value with its identifiers resolved, and the counts. The id is unchanged when nothing in
/// the value was resolved.
fn resolve_value(arena: &mut Arena, id: ValueId, site: &mut Site, walk: &mut Walk) -> (ValueId, usize, usize) {
    let node = *arena.value(id);
    match node {
        ValueNode::Identifier { value, position } => {
            let Some(dix) = walk.ctx.get(arena.text(value)) else {
                return (id, 0, 1);
            };
            let resolved = alloc_dix_value(arena, dix, position);
            walk.newly_resolved.push((site.render(arena), dix.clone()));
            (resolved, 1, 0)
        }
        ValueNode::Array { values, position } | ValueNode::NestedArray { values, position, .. } => {
            let (list, res, skip) = resolve_list(arena, values, site, walk, Step::Index);
            if res > 0 {
                (arena.alloc_value(ValueNode::Array { values: list, position }), res, skip)
            } else {
                (id, 0, skip)
            }
        }
        ValueNode::Object { properties, position } => {
            let mut props: Option<Vec<ObjectPropNode>> = None;
            let (mut res, mut skip) = (0, 0);
            for i in 0..properties.len() {
                let prop = arena.prop_list(properties)[i];
                site.steps.push(Step::Key(prop.key));
                let (new, r, s) = resolve_value(arena, prop.value, site, walk);
                site.steps.pop();
                res += r;
                skip += s;
                if r > 0 {
                    props.get_or_insert_with(|| arena.prop_list(properties).to_vec())[i].value = new;
                }
            }
            match props {
                Some(props) => {
                    let properties = arena.alloc_prop_list(&props);
                    (arena.alloc_value(ValueNode::Object { properties, position }), res, skip)
                }
                None => (id, 0, skip),
            }
        }
        ValueNode::PrefixedConstructor { prefix, arguments, position } => {
            let (list, res, skip) = resolve_list(arena, arguments, site, walk, Step::Arg);
            if res > 0 {
                (arena.alloc_value(ValueNode::PrefixedConstructor { prefix, arguments: list, position }), res, skip)
            } else {
                (id, 0, skip)
            }
        }
        _ => (id, 0, 0),
    }
}

/// The list with its identifiers resolved; the same list when nothing was.
fn resolve_list(
    arena: &mut Arena,
    list: ValueList,
    site: &mut Site,
    walk: &mut Walk,
    step: fn(usize) -> Step,
) -> (ValueList, usize, usize) {
    let mut ids: Option<Vec<ValueId>> = None;
    let (mut res, mut skip) = (0, 0);
    for i in 0..list.len() {
        let item = arena.value_list(list)[i];
        site.steps.push(step(i));
        let (new, r, s) = resolve_value(arena, item, site, walk);
        site.steps.pop();
        res += r;
        skip += s;
        if r > 0 {
            ids.get_or_insert_with(|| arena.value_list(list).to_vec())[i] = new;
        }
    }
    match ids {
        Some(ids) => (arena.alloc_value_list(&ids), res, skip),
        None => (list, 0, skip),
    }
}
