//! LINQ-style querying over DixScript array data.
//!
//! `DixQuery` is a small owned, chainable wrapper around `Vec<DixValue>`.
//! Build one from `DixData::query(path)` or `DixData::query_many(pattern)`,
//! then chain filter/sort/group operations before a terminal read.
//!
//! Deliberately owns its data (clones once at construction) rather than
//! borrowing `&DixData` -- this keeps the whole chain free of lifetime
//! parameters, which matters more for ergonomics here than the clone
//! cost matters for perf, given DixScript's config-sized target data.
//!
//! ## `query(path)` vs `query_many(pattern)`
//!
//! `DixData`'s flattener inserts the *whole* `DixValue::Array` at an
//! array field's base path -- for a plain `Array` literal (`tasks =
//! [...]`) **and** for a `GroupArray` (`events:: {...}, {...}`) alike
//! (`Runtime/dix_data.rs::flatten_data_section`, the `GroupArray` arm).
//! So `query("events")` already returns every item of a `GroupArray` --
//! no globbing needed for that case.
//!
//! `query_many(pattern)` is for a different shape: several *sibling*
//! paths that share structure but differ by a named key segment, e.g.
//! `servers.web1.status`, `servers.db1.status`, ... -- there `*` stands
//! in for one whole dot-segment (same syntax and limits as
//! `DixData::select_many`: a `*` segment matches any single segment
//! verbatim, it isn't a substring/prefix glob, and an array-bracket
//! index like `[0]` is part of the segment string it's attached to, not
//! a segment of its own -- so `query_many` can't select "every index of
//! this one array" the way `query(path)` already does directly).
//!
//! ```rust,ignore
//! use dixscript::Runtime::DixValue;
//!
//! let high_priority_names: Vec<Option<&str>> = {
//!     let filtered = data.query("tasks")
//!         .expect("tasks should be an array")
//!         .where_(|v| v.field("priority").and_then(DixValue::as_int) == Some(3));
//!     filtered.select(|v| v.field("name").and_then(DixValue::as_string))
//! };
//!
//! // Every GroupArray item whose `kind` is ACTIVE:
//! let active_count = data.query("events")
//!     .unwrap_or_default()
//!     .where_(|v| v.field("kind").and_then(DixValue::as_string) == Some("ACTIVE"))
//!     .count();
//!
//! // Sibling paths sharing shape, wildcarding the server name segment:
//! let statuses = data.query_many("servers.*.status");
//! ```

use std::collections::HashMap;
use std::hash::Hash;

use super::dix_data::DixData;
use super::dix_value::DixValue;

// ── DixValue field-access helpers ───────────────────────────────────────────

impl DixValue {
    /// Borrow a named field out of an `Object` value. `None` for any other
    /// variant, or if the object has no such key.
    pub fn field(&self, name: &str) -> Option<&DixValue> {
        self.as_object().and_then(|o| o.get(name))
    }

    /// Dotted-path field access through nested `Object` values --
    /// `v.field_path("owner.name")` is `v.field("owner").and_then(|o| o.field("name"))`.
    pub fn field_path(&self, path: &str) -> Option<&DixValue> {
        path.split('.').try_fold(self, |cur, segment| cur.field(segment))
    }

    /// View this value as a query over its own elements when it's an
    /// `Array`. `None` for any other variant.
    pub fn query(&self) -> Option<DixQuery> {
        self.as_array().map(DixQuery::from_slice)
    }
}

// ── DixQuery ─────────────────────────────────────────────────────────────

/// A chainable, LINQ-style query over a set of `DixValue`s.
#[derive(Debug, Clone, Default)]
pub struct DixQuery {
    items: Vec<DixValue>,
}

impl DixQuery {
    pub fn new(items: Vec<DixValue>) -> Self {
        DixQuery { items }
    }

    pub fn from_slice(items: &[DixValue]) -> Self {
        DixQuery { items: items.to_vec() }
    }

    // ── filtering / projection ──────────────────────────────────────────

    /// Keep only elements matching `predicate`. (LINQ `Where`)
    pub fn where_(mut self, predicate: impl Fn(&DixValue) -> bool) -> Self {
        self.items.retain(|v| predicate(v));
        self
    }

    /// Keep only elements whose `field` equals `value`. Shorthand for the
    /// common `.where_(|v| v.field(name) == Some(value))` case -- works for
    /// every `DixValue` variant via its own `PartialEq`, including `Enum`
    /// once a value carries real enum identity.
    pub fn where_field_eq(self, field: &str, value: &DixValue) -> Self {
        self.where_(move |v| v.field(field) == Some(value))
    }

    /// Discard the first `n` elements. (LINQ `Skip`)
    pub fn skip(mut self, n: usize) -> Self {
        if n >= self.items.len() {
            self.items.clear();
        } else {
            self.items.drain(0..n);
        }
        self
    }

    /// Keep only the first `n` elements. (LINQ `Take`)
    pub fn take(mut self, n: usize) -> Self {
        self.items.truncate(n);
        self
    }

    /// Remove duplicate elements (by `DixValue`'s own `PartialEq`),
    /// preserving first-seen order. O(n^2) -- fine for the config-sized
    /// arrays DixScript targets, not intended for huge datasets.
    pub fn distinct(mut self) -> Self {
        let mut out: Vec<DixValue> = Vec::with_capacity(self.items.len());
        for v in self.items.drain(..) {
            if !out.contains(&v) {
                out.push(v);
            }
        }
        self.items = out;
        self
    }

    /// Project each element to a new value. (LINQ `Select`) -- borrows
    /// rather than consumes, specifically so `map` can return data
    /// borrowed from the query itself (e.g. `.select(|v|
    /// v.field("name").and_then(DixValue::as_string))` returning `&str`).
    /// The named lifetime `'q` (rather than a plain `impl Fn(&DixValue) ->
    /// T`) is what makes that legal -- without it, `T` would have to work
    /// for *any* input lifetime, which rules out `T` containing a
    /// reference at all.
    pub fn select<'q, T>(&'q self, map: impl Fn(&'q DixValue) -> T) -> Vec<T> {
        self.items.iter().map(map).collect()
    }

    /// Project each element through a named field. Shorthand for
    /// `.select(|v| v.field(name).cloned())`.
    pub fn select_field(&self, name: &str) -> Vec<Option<DixValue>> {
        self.items.iter().map(|v| v.field(name).cloned()).collect()
    }

    // ── ordering ─────────────────────────────────────────────────────────

    /// Sort ascending by a derived key. (LINQ `OrderBy`) Stable sort.
    pub fn order_by<K: Ord>(mut self, key: impl Fn(&DixValue) -> K) -> Self {
        self.items.sort_by_key(key);
        self
    }

    /// Sort descending by a derived key. (LINQ `OrderByDescending`)
    pub fn order_by_desc<K: Ord>(mut self, key: impl Fn(&DixValue) -> K) -> Self {
        self.items.sort_by(|a, b| key(b).cmp(&key(a)));
        self
    }

    // ── grouping ─────────────────────────────────────────────────────────

    /// Group elements by a derived key, preserving first-seen key order.
    /// (LINQ `GroupBy`)
    pub fn group_by<K: Eq + Hash + Clone>(
        self,
        key: impl Fn(&DixValue) -> K,
    ) -> Vec<(K, Vec<DixValue>)> {
        let mut order: Vec<K> = Vec::new();
        let mut groups: HashMap<K, Vec<DixValue>> = HashMap::new();
        for v in self.items {
            let k = key(&v);
            if !groups.contains_key(&k) {
                order.push(k.clone());
            }
            groups.entry(k).or_default().push(v);
        }
        order
            .into_iter()
            .map(|k| {
                let vs = groups.remove(&k).unwrap_or_default();
                (k, vs)
            })
            .collect()
    }

    // ── terminal predicates / aggregates ────────────────────────────────

    pub fn any(&self, predicate: impl Fn(&DixValue) -> bool) -> bool {
        self.items.iter().any(|v| predicate(v))
    }

    pub fn all(&self, predicate: impl Fn(&DixValue) -> bool) -> bool {
        self.items.iter().all(|v| predicate(v))
    }

    pub fn count(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn first(&self) -> Option<&DixValue> {
        self.items.first()
    }

    pub fn first_or<'d>(&'d self, default: &'d DixValue) -> &'d DixValue {
        self.first().unwrap_or(default)
    }

    pub fn last(&self) -> Option<&DixValue> {
        self.items.last()
    }

    pub fn nth(&self, index: usize) -> Option<&DixValue> {
        self.items.get(index)
    }

    /// Sum of every element's numeric value, widened to `i64`.
    /// Non-numeric elements contribute nothing.
    pub fn sum_int(&self) -> i64 {
        self.items.iter().filter_map(DixValue::as_long).sum()
    }

    /// Sum of every element's numeric value, widened to `f64`.
    /// Non-numeric elements contribute nothing.
    pub fn sum_float(&self) -> f64 {
        self.items.iter().filter_map(DixValue::as_float).sum()
    }

    /// Average of every numeric element's `f64` value. `None` on an
    /// empty query or one with no numeric elements.
    pub fn avg_float(&self) -> Option<f64> {
        let (sum, n) = self.items.iter().filter_map(DixValue::as_float)
            .fold((0.0_f64, 0usize), |(s, n), v| (s + v, n + 1));
        if n == 0 { None } else { Some(sum / n as f64) }
    }

    pub fn min_by_key<K: Ord>(&self, key: impl Fn(&DixValue) -> K) -> Option<&DixValue> {
        self.items.iter().min_by_key(|v| key(v))
    }

    pub fn max_by_key<K: Ord>(&self, key: impl Fn(&DixValue) -> K) -> Option<&DixValue> {
        self.items.iter().max_by_key(|v| key(v))
    }

    /// Consume the query, returning the current result set as an owned
    /// `Vec<DixValue>`.
    pub fn to_vec(self) -> Vec<DixValue> {
        self.items
    }

    /// Borrowed view of the current result set.
    pub fn as_slice(&self) -> &[DixValue] {
        &self.items
    }

    pub fn iter(&self) -> std::slice::Iter<'_, DixValue> {
        self.items.iter()
    }
}

impl IntoIterator for DixQuery {
    type Item = DixValue;
    type IntoIter = std::vec::IntoIter<DixValue>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a> IntoIterator for &'a DixQuery {
    type Item = &'a DixValue;
    type IntoIter = std::slice::Iter<'a, DixValue>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl FromIterator<DixValue> for DixQuery {
    fn from_iter<I: IntoIterator<Item = DixValue>>(iter: I) -> Self {
        DixQuery { items: iter.into_iter().collect() }
    }
}

// ── DixData entry points ────────────────────────────────────────────────

impl DixData {
    /// Query the array at `path` -- `None` if the path doesn't exist or
    /// isn't an `Array` value.
    pub fn query(&self, path: &str) -> Option<DixQuery> {
        self.get_value(path).and_then(DixValue::query)
    }

    /// Query across every *sibling path* matched by a glob `pattern`
    /// (same whole-segment `*` syntax as `select_many` -- see the module
    /// doc for exactly what that does and doesn't match), gathered into
    /// a single queryable set. For iterating a `GroupArray`'s own items,
    /// use `query(path)` instead -- the flattener already stores the
    /// full array there directly.
    pub fn query_many(&self, pattern: &str) -> DixQuery {
        DixQuery::new(self.select_many::<DixValue>(pattern))
    }
}
