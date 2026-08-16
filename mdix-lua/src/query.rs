// MdixQuery — LINQ-style querying over native Lua data.
//
// Host-language-native counterpart to `dixscript::Runtime::query::DixQuery`
// (see `dixscript/src/Runtime/query.rs`) — same reasoning as the Python
// binding's MdixQuery (mdix-python/src/query.rs) and the Go/C# bindings'
// equivalents: rebuilding the query engine at the binding boundary would
// mean two implementations of the same filter/sort/group logic to keep in
// sync, so this wraps plain Lua tables (the same 1-indexed sequences
// db:get()/db:to_table() already produce via value.rs's dix_to_lua) and
// every predicate/key/selector is a plain Lua function, invoked per
// element via `Function::call`.
//
// Unlike the Python binding (which routes through `json.loads(get_json(..))`
// since PyO3 has no direct DixValue -> PyObject path), this crate already
// has one — dix_to_lua, the same conversion db:get() uses — so
// from_dix_query below builds straight off DixData::query()/query_many()'s
// Vec<DixValue> with no JSON round trip at all.
//
//   local high_priority = db:query("tasks")
//       :where(function(t) return t.priority == 3 end)
//       :order_by_desc(function(t) return t.priority end)
//   local names = high_priority:select(function(t) return t.name end)
//
//   -- Sibling paths sharing shape, wildcarding one segment:
//   local statuses = db:query_many("servers.*.status")

use mlua::{
    Function as LuaFunction, Lua, MetaMethod, Result as LuaResult,
    Table as LuaTable, UserData, UserDataMethods, Value as LuaValue,
};
use dixscript::Runtime::DixQuery;
use std::cmp::Ordering;

use crate::value::dix_to_lua;

pub struct LuaMdixQuery {
    items: Vec<LuaValue>,
}

// ── Non-UserData helpers ────────────────────────────────────────────────────

impl LuaMdixQuery {
    pub fn new(items: Vec<LuaValue>) -> Self {
        LuaMdixQuery { items }
    }

    /// Build from a core DixQuery (DixData::query(path) / query_many(pattern)),
    /// converting every element via dix_to_lua.
    pub fn from_dix_query(lua: &Lua, q: DixQuery) -> LuaResult<Self> {
        let items = q
            .to_vec()
            .iter()
            .map(|v| dix_to_lua(lua, v))
            .collect::<LuaResult<Vec<_>>>()?;
        Ok(LuaMdixQuery { items })
    }

    /// Build from a plain Lua sequence table (mdix.query(table) — lets a
    /// caller query arbitrary Lua data, not just a loaded Database's
    /// fields, matching the Python binding's constructor flexibility).
    pub fn from_table(t: &LuaTable) -> LuaResult<Self> {
        let mut items = Vec::with_capacity(t.raw_len());
        for pair in t.pairs::<LuaValue, LuaValue>() {
            let (_, v) = pair?;
            items.push(v);
        }
        Ok(LuaMdixQuery { items })
    }
}

/// Cast a Lua value to an f64 key for ordering purposes. Deliberately
/// NOT `Value::as_f64()` — that only matches the `Number` variant, but
/// `dix_to_lua` (see value.rs) emits DixValue::Int/Long as
/// `Value::Integer`, which `as_f64()` silently treats as non-numeric.
/// Checked directly against mlua 0.10's Value::as_number impl before
/// relying on it, rather than assumed.
fn numeric_key(v: &LuaValue) -> Option<f64> {
    match v {
        LuaValue::Integer(i) => Some(*i as f64),
        LuaValue::Number(n) => Some(*n),
        _ => None,
    }
}

fn string_key(v: &LuaValue) -> Option<String> {
    match v {
        LuaValue::String(s) => s.to_str().ok().map(|c| c.to_string()),
        _ => None,
    }
}

/// Order two query keys: numeric if both resolve to a Lua number
/// (Integer or Number — see numeric_key), else string if both resolve
/// to a Lua string, else Equal. A pair whose keys aren't mutually
/// comparable this way is treated as equal rather than raising — keeps
/// sort_by/min/max total and panic-free, same tradeoff the Python
/// binding's sorted_by documents for its own not-mutually-comparable
/// case.
fn compare_keys(a: &LuaValue, b: &LuaValue) -> Ordering {
    if let (Some(x), Some(y)) = (numeric_key(a), numeric_key(b)) {
        return x.partial_cmp(&y).unwrap_or(Ordering::Equal);
    }
    if let (Some(x), Some(y)) = (string_key(a), string_key(b)) {
        return x.cmp(&y);
    }
    Ordering::Equal
}

/// Shared implementation for where/where_field_eq/distinct's Lua `==`
/// need — mlua's Value already implements PartialEq (checked directly:
/// mlua-0.10.5/src/value.rs), so this is just a thin readability wrapper.
fn lua_eq(a: &LuaValue, b: &LuaValue) -> bool {
    a == b
}

// ── UserData ─────────────────────────────────────────────────────────────

impl UserData for LuaMdixQuery {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {

        // ── filtering / projection ──────────────────────────────────────

        /// Keep only elements matching predicate(item) -> bool. (LINQ Where)
        /// Named `where` (not Python's `where_`) — "where" isn't a Lua
        /// reserved word, so there's no clash to avoid here.
        methods.add_method("where", |_, this, predicate: LuaFunction| {
            let mut out = Vec::with_capacity(this.items.len());
            for item in &this.items {
                let keep: bool = predicate.call(item.clone())?;
                if keep {
                    out.push(item.clone());
                }
            }
            Ok(LuaMdixQuery { items: out })
        });

        /// Keep only elements whose field equals value. Shorthand for
        /// :where(function(v) return v[field] == value end) — items
        /// that aren't tables are simply excluded rather than erroring.
        methods.add_method(
            "where_field_eq",
            |_, this, (field, value): (String, LuaValue)| {
                let mut out = Vec::with_capacity(this.items.len());
                for item in &this.items {
                    if let LuaValue::Table(t) = item {
                        let field_val: LuaValue = t.get(field.as_str())?;
                        if lua_eq(&field_val, &value) {
                            out.push(item.clone());
                        }
                    }
                }
                Ok(LuaMdixQuery { items: out })
            },
        );

        /// Discard the first n elements. (LINQ Skip)
        methods.add_method("skip", |_, this, n: usize| {
            Ok(LuaMdixQuery { items: this.items.iter().skip(n).cloned().collect() })
        });

        /// Keep only the first n elements. (LINQ Take)
        methods.add_method("take", |_, this, n: usize| {
            Ok(LuaMdixQuery { items: this.items.iter().take(n).cloned().collect() })
        });

        /// Remove duplicate elements (by Lua ==), preserving first-seen
        /// order. O(n^2) — same config-sized-data tradeoff the Python
        /// binding's distinct() documents.
        methods.add_method("distinct", |_, this, ()| {
            let mut out: Vec<LuaValue> = Vec::with_capacity(this.items.len());
            for item in &this.items {
                if !out.iter().any(|existing| lua_eq(existing, item)) {
                    out.push(item.clone());
                }
            }
            Ok(LuaMdixQuery { items: out })
        });

        /// Project each element through map(item) -> any. (LINQ Select)
        /// Returns a plain Lua table (1-indexed), not another MdixQuery —
        /// the result of a projection isn't necessarily still
        /// query-shaped data (matches Python's select() returning a
        /// plain list for the same reason).
        methods.add_method("select", |lua, this, map: LuaFunction| {
            let t = lua.create_table()?;
            for (i, item) in this.items.iter().enumerate() {
                let mapped: LuaValue = map.call(item.clone())?;
                t.set(i + 1, mapped)?;
            }
            Ok(t)
        });

        /// Project each element through a named field. nil where the
        /// element isn't a table or lacks the field.
        methods.add_method("select_field", |lua, this, name: String| {
            let t = lua.create_table()?;
            for (i, item) in this.items.iter().enumerate() {
                let val: LuaValue = match item {
                    LuaValue::Table(tbl) => tbl.get(name.as_str())?,
                    _ => LuaValue::Nil,
                };
                t.set(i + 1, val)?;
            }
            Ok(t)
        });

        // ── ordering ──────────────────────────────────────────────────────

        /// Stable sort ascending by key(item). (LINQ OrderBy) Key must
        /// resolve to a Lua number or string for every element — see
        /// compare_keys.
        methods.add_method("order_by", |_, this, key: LuaFunction| {
            sorted_by(this, key, false)
        });

        /// Stable sort descending by key(item). (LINQ OrderByDescending)
        methods.add_method("order_by_desc", |_, this, key: LuaFunction| {
            sorted_by(this, key, true)
        });

        // ── grouping ──────────────────────────────────────────────────────

        /// Group elements by key(item), preserving first-seen key and
        /// element order. Returns a plain Lua array of {key=.., items=..}
        /// tables (LINQ GroupBy) — not a Lua table keyed by the group
        /// key itself, since Lua table keys can't hold arbitrary
        /// non-scalar key values and this keeps ordering well-defined
        /// regardless of key type.
        ///
        ///   for _, group in ipairs(q:group_by(f)) do
        ///       print(group.key, #group.items)
        ///   end
        methods.add_method("group_by", |lua, this, key: LuaFunction| {
            let mut groups: Vec<(LuaValue, LuaTable)> = Vec::new();
            for item in &this.items {
                let k: LuaValue = key.call(item.clone())?;
                let mut found = false;
                for (existing_k, existing_items) in groups.iter() {
                    if lua_eq(existing_k, &k) {
                        existing_items.set(existing_items.raw_len() + 1, item.clone())?;
                        found = true;
                        break;
                    }
                }
                if !found {
                    let items_table = lua.create_table()?;
                    items_table.set(1, item.clone())?;
                    groups.push((k, items_table));
                }
            }

            let out = lua.create_table()?;
            for (i, (k, items_table)) in groups.into_iter().enumerate() {
                let group_entry = lua.create_table()?;
                group_entry.set("key", k)?;
                group_entry.set("items", items_table)?;
                out.set(i + 1, group_entry)?;
            }
            Ok(out)
        });

        // ── terminal predicates / aggregates ────────────────────────────

        methods.add_method("any", |_, this, predicate: LuaFunction| {
            for item in &this.items {
                if predicate.call(item.clone())? {
                    return Ok(true);
                }
            }
            Ok(false)
        });

        methods.add_method("all", |_, this, predicate: LuaFunction| {
            for item in &this.items {
                let keep: bool = predicate.call(item.clone())?;
                if !keep {
                    return Ok(false);
                }
            }
            Ok(true)
        });

        methods.add_method("count", |_, this, ()| Ok(this.items.len()));

        methods.add_method("is_empty", |_, this, ()| Ok(this.items.is_empty()));

        methods.add_method("first", |_, this, ()| {
            Ok(this.items.first().cloned().unwrap_or(LuaValue::Nil))
        });

        methods.add_method("first_or", |_, this, default: LuaValue| {
            Ok(this.items.first().cloned().unwrap_or(default))
        });

        methods.add_method("last", |_, this, ()| {
            Ok(this.items.last().cloned().unwrap_or(LuaValue::Nil))
        });

        /// 1-indexed, matching Lua convention (unlike the Rust core's
        /// and Python binding's 0-indexed nth).
        methods.add_method("nth", |_, this, index: usize| {
            if index < 1 {
                return Ok(LuaValue::Nil);
            }
            Ok(this.items.get(index - 1).cloned().unwrap_or(LuaValue::Nil))
        });

        /// Sum of every element's numeric value, widened to a Lua
        /// integer. Non-numeric elements contribute nothing (skipped,
        /// not an error) — matches the core's filter_map behavior.
        methods.add_method("sum_int", |_, this, ()| {
            let sum: i64 = this.items.iter().filter_map(numeric_key).map(|f| f as i64).sum();
            Ok(sum)
        });

        methods.add_method("sum_float", |_, this, ()| {
            let sum: f64 = this.items.iter().filter_map(numeric_key).sum();
            Ok(sum)
        });

        /// Average of every numeric element. nil on an empty query or
        /// one with no numeric elements.
        methods.add_method("avg_float", |_, this, ()| {
            let (sum, n) = this
                .items
                .iter()
                .filter_map(numeric_key)
                .fold((0.0_f64, 0usize), |(s, n), v| (s + v, n + 1));
            if n == 0 {
                Ok(LuaValue::Nil)
            } else {
                Ok(LuaValue::Number(sum / n as f64))
            }
        });

        /// Element with the minimum key(item). On ties, the first such
        /// element wins (matches Rust's Iterator::min_by_key, same as
        /// the core's own DixQuery::min_by_key).
        methods.add_method("min_by_key", |_, this, key: LuaFunction| {
            extreme_by(this, key, true)
        });

        /// Element with the maximum key(item). On ties, the last such
        /// element wins (matches Iterator::max_by_key).
        methods.add_method("max_by_key", |_, this, key: LuaFunction| {
            extreme_by(this, key, false)
        });

        /// Current result set as a plain 1-indexed Lua table.
        methods.add_method("to_table", |lua, this, ()| {
            let t = lua.create_table()?;
            for (i, item) in this.items.iter().enumerate() {
                t.set(i + 1, item.clone())?;
            }
            Ok(t)
        });

        // ── Metamethods ──────────────────────────────────────────────────

        methods.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.items.len()));

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("MdixQuery(count={})", this.items.len()))
        });
    }
}

// ── Shared helpers (need &LuaMdixQuery, so plain functions rather than
//    inline closures duplicated across order_by/order_by_desc and
//    min_by_key/max_by_key) ─────────────────────────────────────────────

fn sorted_by(this: &LuaMdixQuery, key: LuaFunction, descending: bool) -> LuaResult<LuaMdixQuery> {
    let mut keyed: Vec<(LuaValue, LuaValue)> = Vec::with_capacity(this.items.len());
    for item in &this.items {
        let k: LuaValue = key.call(item.clone())?;
        keyed.push((k, item.clone()));
    }
    keyed.sort_by(|a, b| {
        let ord = compare_keys(&a.0, &b.0);
        if descending { ord.reverse() } else { ord }
    });
    Ok(LuaMdixQuery { items: keyed.into_iter().map(|(_, v)| v).collect() })
}

fn extreme_by(this: &LuaMdixQuery, key: LuaFunction, want_min: bool) -> LuaResult<LuaValue> {
    let mut best: Option<(LuaValue, LuaValue)> = None;
    for item in &this.items {
        let k: LuaValue = key.call(item.clone())?;
        best = match best {
            None => Some((k, item.clone())),
            Some((best_k, best_v)) => {
                let ord = compare_keys(&k, &best_k);
                let replace = if want_min { ord == Ordering::Less } else { ord != Ordering::Less };
                if replace { Some((k, item.clone())) } else { Some((best_k, best_v)) }
            }
        };
    }
    Ok(best.map(|(_, v)| v).unwrap_or(LuaValue::Nil))
}
