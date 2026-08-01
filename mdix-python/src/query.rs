//! MdixQuery — LINQ-style querying over native Python data.
//!
//! Host-language-native counterpart to `dixscript::Runtime::query::DixQuery`
//! (see `dixscript/src/Runtime/query.rs`). Deliberately does **not** bind
//! the core `DixQuery` directly: that type operates on `DixValue` and every
//! predicate/key/selector would need a Rust closure, which Python callers
//! can't supply. Rebuilding the query engine at the FFI boundary would also
//! mean two implementations of the same filter/sort/group logic that have
//! to be kept in sync — the same reasoning `MdixDatabase::to_table()`
//! already documents for routing through `json.loads` instead of a
//! hand-written `DixValue -> PyObject` walker.
//!
//! Instead, `MdixQuery` wraps plain Python objects — the same
//! json-round-tripped dicts/lists `to_table()`/`get_json()` already
//! produce — and every predicate/key/selector is a plain Python callable
//! invoked per element via `call1`. This mirrors how the C# binding
//! handles the same gap (`MdixQueryExtensions` in
//! `mdix-csharp/src/MidManStudio.Mdix.Core/MdixQuery.cs`): fetch the data
//! natively, then query with the host language's own idioms, rather than
//! forcing Rust's closure-based API through the binding layer.
//!
//! ```python
//! high_priority = (db.query("tasks")
//!     .where_(lambda t: t["priority"] == 3)
//!     .order_by_desc(lambda t: t["priority"]))
//! names = high_priority.select(lambda t: t["name"])
//!
//! # Sibling paths sharing shape, wildcarding one segment:
//! statuses = db.query_many("servers.*.status")
//! ```

use pyo3::prelude::*;
use pyo3::types::PyList;
use std::cmp::Ordering;

#[pyclass(module = "midmanstudio.mdix")]
#[derive(Clone)]
pub struct MdixQuery {
    items: Vec<Py<PyAny>>,
}

// ── Non-pymethods helpers — must be in a plain impl block ─────────────────
// Same reasoning as MdixDatabase's own plain impl block in database.rs.
impl MdixQuery {
    pub fn new(items: Vec<Py<PyAny>>) -> Self {
        MdixQuery { items }
    }

    /// Build from a Python list (e.g. the result of `json.loads` on a
    /// `get_json(path)` string). Errors if `obj` isn't a list — mirrors
    /// the core's `query(path)` returning `None` for a non-Array path,
    /// just surfaced as an error here since this helper is only ever
    /// called right after we've already confirmed the shape upstream.
    pub fn from_any(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        let list = obj.downcast::<PyList>().map_err(|_| {
            crate::error::to_py_err("[mdix] query() target is not a JSON array")
        })?;
        Ok(MdixQuery { items: list.iter().map(|v| v.unbind()).collect() })
    }

    /// Stable sort by a Python-callable-derived key, using Python's own
    /// rich-comparison protocol (`compare`) so keys of any comparable
    /// Python type (int, float, str, tuple, ...) sort exactly as
    /// `sorted()` would. A pair whose keys aren't mutually comparable
    /// (e.g. int vs str) is treated as equal rather than raising —
    /// keeps the sort total and panic-free; the core's own `order_by`
    /// carries the same "stable sort" contract, just without this edge
    /// case, since Rust's `Ord` guarantees comparability at compile time.
    fn sorted_by(&self, py: Python<'_>, key: PyObject, descending: bool) -> PyResult<MdixQuery> {
        let mut keyed: Vec<(PyObject, Py<PyAny>)> = Vec::with_capacity(self.items.len());
        for item in &self.items {
            let k = key.call1(py, (item.clone_ref(py),))?;
            keyed.push((k, item.clone_ref(py)));
        }
        keyed.sort_by(|a, b| {
            let ord = a.0.bind(py)
                .compare(b.0.clone_ref(py).into_bound(py))
                .unwrap_or(Ordering::Equal);
            if descending { ord.reverse() } else { ord }
        });
        Ok(MdixQuery { items: keyed.into_iter().map(|(_, v)| v).collect() })
    }

    /// Shared implementation for `min_by_key`/`max_by_key`. Tie-breaking
    /// matches `std::iter::Iterator::min_by_key`/`max_by_key` exactly
    /// (what the core's own `DixQuery::min_by_key`/`max_by_key` use under
    /// the hood): on equal keys, `min_by_key` keeps the *first* element
    /// seen, `max_by_key` keeps the *last*.
    fn extreme_by(&self, py: Python<'_>, key: PyObject, want_min: bool) -> PyResult<Option<PyObject>> {
        let mut best: Option<(PyObject, Py<PyAny>)> = None;
        for item in &self.items {
            let k = key.call1(py, (item.clone_ref(py),))?;
            best = match best {
                None => Some((k, item.clone_ref(py))),
                Some((best_k, best_v)) => {
                    let ord = k.bind(py)
                        .compare(best_k.clone_ref(py).into_bound(py))
                        .unwrap_or(Ordering::Equal);
                    let replace = if want_min { ord == Ordering::Less } else { ord != Ordering::Less };
                    if replace { Some((k, item.clone_ref(py))) } else { Some((best_k, best_v)) }
                }
            };
        }
        Ok(best.map(|(_, v)| v))
    }
}

#[pymethods]
impl MdixQuery {
    #[new]
    fn py_new(items: Vec<Py<PyAny>>) -> Self {
        MdixQuery { items }
    }

    // ── filtering / projection ──────────────────────────────────────────

    /// Keep only elements matching `predicate(item) -> bool`. (LINQ `Where`)
    fn where_(&self, py: Python<'_>, predicate: PyObject) -> PyResult<MdixQuery> {
        let mut out = Vec::with_capacity(self.items.len());
        for item in &self.items {
            let keep: bool = predicate.call1(py, (item.clone_ref(py),))?.extract(py)?;
            if keep { out.push(item.clone_ref(py)); }
        }
        Ok(MdixQuery { items: out })
    }

    /// Keep only elements whose `field` equals `value`. Shorthand for
    /// `.where_(lambda v: v.get(field) == value)` — items that aren't
    /// subscriptable, or don't have `field`, are simply excluded rather
    /// than raising.
    fn where_field_eq(&self, py: Python<'_>, field: &str, value: PyObject) -> PyResult<MdixQuery> {
        let mut out = Vec::with_capacity(self.items.len());
        for item in &self.items {
            if let Ok(field_val) = item.bind(py).get_item(field) {
                if field_val.eq(value.bind(py))? {
                    out.push(item.clone_ref(py));
                }
            }
        }
        Ok(MdixQuery { items: out })
    }

    /// Discard the first `n` elements. (LINQ `Skip`)
    fn skip(&self, py: Python<'_>, n: usize) -> MdixQuery {
        MdixQuery { items: self.items.iter().skip(n).map(|v| v.clone_ref(py)).collect() }
    }

    /// Keep only the first `n` elements. (LINQ `Take`)
    fn take(&self, py: Python<'_>, n: usize) -> MdixQuery {
        MdixQuery { items: self.items.iter().take(n).map(|v| v.clone_ref(py)).collect() }
    }

    /// Remove duplicate elements (by Python `==`), preserving first-seen
    /// order. O(n^2) — matches the core's own documented tradeoff: fine
    /// for the config-sized arrays DixScript targets, not huge datasets.
    fn distinct(&self, py: Python<'_>) -> PyResult<MdixQuery> {
        let mut out: Vec<Py<PyAny>> = Vec::with_capacity(self.items.len());
        for item in &self.items {
            let mut seen = false;
            for existing in &out {
                if item.bind(py).eq(existing.bind(py))? { seen = true; break; }
            }
            if !seen { out.push(item.clone_ref(py)); }
        }
        Ok(MdixQuery { items: out })
    }

    /// Project each element through `map(item) -> Any`. (LINQ `Select`)
    fn select(&self, py: Python<'_>, map: PyObject) -> PyResult<Vec<PyObject>> {
        self.items.iter().map(|item| map.call1(py, (item.clone_ref(py),))).collect()
    }

    /// Project each element through a named field/key. `None` where the
    /// element isn't subscriptable or lacks the field — shorthand for
    /// `.select(lambda v: v.get(name))`.
    fn select_field(&self, py: Python<'_>, name: &str) -> Vec<PyObject> {
        self.items.iter().map(|item| {
            item.bind(py).get_item(name).map(|v| v.unbind()).unwrap_or_else(|_| py.None())
        }).collect()
    }

    // ── ordering ─────────────────────────────────────────────────────────

    /// Sort ascending by `key(item)`. (LINQ `OrderBy`) Stable.
    fn order_by(&self, py: Python<'_>, key: PyObject) -> PyResult<MdixQuery> {
        self.sorted_by(py, key, false)
    }

    /// Sort descending by `key(item)`. (LINQ `OrderByDescending`) Stable.
    fn order_by_desc(&self, py: Python<'_>, key: PyObject) -> PyResult<MdixQuery> {
        self.sorted_by(py, key, true)
    }

    // ── grouping ─────────────────────────────────────────────────────────

    /// Group elements by `key(item)`, preserving first-seen key order.
    /// Returns a list of `(key, items)` pairs — O(n^2) grouping (Python
    /// keys aren't assumed hashable), same config-sized-data tradeoff as
    /// `distinct()`. (LINQ `GroupBy`)
    fn group_by(&self, py: Python<'_>, key: PyObject) -> PyResult<Vec<(PyObject, Vec<PyObject>)>> {
        let mut groups: Vec<(PyObject, Vec<PyObject>)> = Vec::new();
        for item in &self.items {
            let k = key.call1(py, (item.clone_ref(py),))?;
            let mut found = false;
            for (existing_k, existing_items) in groups.iter_mut() {
                if k.bind(py).eq(existing_k.bind(py))? {
                    existing_items.push(item.clone_ref(py));
                    found = true;
                    break;
                }
            }
            if !found {
                groups.push((k, vec![item.clone_ref(py)]));
            }
        }
        Ok(groups)
    }

    // ── terminal predicates / aggregates ────────────────────────────────

    fn any(&self, py: Python<'_>, predicate: PyObject) -> PyResult<bool> {
        for item in &self.items {
            if predicate.call1(py, (item.clone_ref(py),))?.extract::<bool>(py)? { return Ok(true); }
        }
        Ok(false)
    }

    fn all(&self, py: Python<'_>, predicate: PyObject) -> PyResult<bool> {
        for item in &self.items {
            if !predicate.call1(py, (item.clone_ref(py),))?.extract::<bool>(py)? { return Ok(false); }
        }
        Ok(true)
    }

    fn count(&self) -> usize { self.items.len() }

    #[getter]
    fn is_empty(&self) -> bool { self.items.is_empty() }

    fn first(&self, py: Python<'_>) -> Option<PyObject> {
        self.items.first().map(|v| v.clone_ref(py))
    }

    fn first_or(&self, py: Python<'_>, default: PyObject) -> PyObject {
        self.items.first().map(|v| v.clone_ref(py)).unwrap_or(default)
    }

    fn last(&self, py: Python<'_>) -> Option<PyObject> {
        self.items.last().map(|v| v.clone_ref(py))
    }

    fn nth(&self, py: Python<'_>, index: usize) -> Option<PyObject> {
        self.items.get(index).map(|v| v.clone_ref(py))
    }

    /// Sum of every element's numeric value, widened to `int`.
    /// Non-numeric elements contribute nothing (extraction failure is
    /// skipped, not raised — matches the core's `filter_map` behaviour).
    fn sum_int(&self, py: Python<'_>) -> i64 {
        self.items.iter().filter_map(|v| v.bind(py).extract::<i64>().ok()).sum()
    }

    /// Sum of every element's numeric value, widened to `float`.
    fn sum_float(&self, py: Python<'_>) -> f64 {
        self.items.iter().filter_map(|v| v.bind(py).extract::<f64>().ok()).sum()
    }

    /// Average of every numeric element's `float` value. `None` on an
    /// empty query or one with no numeric elements.
    fn avg_float(&self, py: Python<'_>) -> Option<f64> {
        let (sum, n) = self.items.iter()
            .filter_map(|v| v.bind(py).extract::<f64>().ok())
            .fold((0.0_f64, 0usize), |(s, n), v| (s + v, n + 1));
        if n == 0 { None } else { Some(sum / n as f64) }
    }

    /// Element with the minimum `key(item)`. On ties, the *first* such
    /// element wins (matches Rust's `Iterator::min_by_key`).
    fn min_by_key(&self, py: Python<'_>, key: PyObject) -> PyResult<Option<PyObject>> {
        self.extreme_by(py, key, true)
    }

    /// Element with the maximum `key(item)`. On ties, the *last* such
    /// element wins (matches Rust's `Iterator::max_by_key`).
    fn max_by_key(&self, py: Python<'_>, key: PyObject) -> PyResult<Option<PyObject>> {
        self.extreme_by(py, key, false)
    }

    /// Consume the query, returning the current result set as a plain
    /// `list`.
    fn to_list(&self, py: Python<'_>) -> Vec<PyObject> {
        self.items.iter().map(|v| v.clone_ref(py)).collect()
    }

    // ── Dunder — sequence protocol ─────────────────────────────────────
    // Deliberately no explicit `__iter__`: defining `__len__` +
    // `__getitem__` is enough for CPython's own sequence-iteration
    // fallback (calls `__getitem__(0, 1, 2, ...)` until `IndexError`),
    // so `for x in q:` / `list(q)` work without constructing a Python
    // list/iterator object by hand here.

    fn __len__(&self) -> usize { self.items.len() }

    fn __getitem__(&self, py: Python<'_>, index: usize) -> PyResult<PyObject> {
        self.items.get(index).map(|v| v.clone_ref(py)).ok_or_else(|| {
            pyo3::exceptions::PyIndexError::new_err("MdixQuery index out of range")
        })
    }

    fn __bool__(&self) -> bool { !self.items.is_empty() }

    fn __repr__(&self) -> String {
        format!("MdixQuery(count={})", self.items.len())
    }
}
