//! MdixMerger — Python binding for AST-level multi-source merging.
//!
//! Wraps `dixscript::Runtime::merge::{MdixMerger, MdixMergeStrategy,
//! ArrayMergeStrategy}`. Because the underlying merge produces a `DixData`
//! (not a raw AST) via `merge_files_weighted`, this binding never needs to
//! expose `DixScript` AST node types to Python at all — it hands back the
//! same `MdixDatabase` wrapper every other entry point in this crate uses.

use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};
use std::io::Write;

use dixscript::Runtime::merge::{
    ArrayMergeStrategy, MdixMergeStrategy, MdixMerger as CoreMerger,
};

use crate::database::MdixDatabase;
use crate::error::to_py_err;
use crate::result::MdixResult;

// ── Strategy string parsing ─────────────────────────────────────────────────

pub(crate) fn parse_merge_strategy(s: &str) -> PyResult<MdixMergeStrategy> {
    match s {
        "weighted_priority" => Ok(MdixMergeStrategy::WeightedPriority),
        "primary_wins"      => Ok(MdixMergeStrategy::PrimaryWins),
        "secondary_wins"    => Ok(MdixMergeStrategy::SecondaryWins),
        "throw_on_conflict" => Ok(MdixMergeStrategy::ThrowOnConflict),
        other => Err(to_py_err(format!(
            "[mdix] Unknown merge strategy '{}'. Expected one of: \
             weighted_priority, primary_wins, secondary_wins, throw_on_conflict.",
            other
        ))),
    }
}

pub(crate) fn parse_array_strategy(s: &str) -> PyResult<ArrayMergeStrategy> {
    match s {
        "replace"      => Ok(ArrayMergeStrategy::Replace),
        "concat"       => Ok(ArrayMergeStrategy::Concat),
        "concat_dedup" => Ok(ArrayMergeStrategy::ConcatDedup),
        other => Err(to_py_err(format!(
            "[mdix] Unknown array merge strategy '{}'. Expected one of: \
             replace, concat, concat_dedup.",
            other
        ))),
    }
}

// ── MdixMerger ────────────────────────────────────────────────────────────────

/// Programmatic AST-level merger for two or more `.mdix` sources.
///
/// ```python
/// from midmanstudio.mdix import MdixMerger
///
/// db = (MdixMerger()
///       .with_strategy("weighted_priority")
///       .with_array_strategy("concat_dedup")
///       .merge_files_weighted([("base.mdix", 1.0), ("patch.mdix", 0.8)]))
///
/// print(db.get_string("app_name"))
/// ```
#[pyclass(module = "midmanstudio.mdix")]
pub struct MdixMerger {
    strategy:       MdixMergeStrategy,
    array_strategy: ArrayMergeStrategy,
}

// ── Non-pymethods helpers — plain impl block (same convention as database.rs) ──
impl MdixMerger {
    pub(crate) fn build_core(&self) -> CoreMerger {
        CoreMerger::new()
            .with_strategy(self.strategy)
            .with_array_strategy(self.array_strategy)
    }

    fn extract_paths(paths: &Bound<'_, PyList>) -> PyResult<Vec<String>> {
        if paths.is_empty() {
            return Err(to_py_err("[mdix] merge_files: no paths provided"));
        }
        paths.iter().map(|p| p.extract::<String>()).collect()
    }

    fn extract_weighted(paths_and_weights: &Bound<'_, PyList>) -> PyResult<Vec<(String, f64)>> {
        if paths_and_weights.is_empty() {
            return Err(to_py_err("[mdix] merge_files_weighted: no paths provided"));
        }
        paths_and_weights
            .iter()
            .map(|item| {
                let tuple = item.downcast::<PyTuple>().map_err(|_| {
                    to_py_err(
                        "[mdix] merge_files_weighted expects a list of (path, weight) tuples",
                    )
                })?;
                if tuple.len() != 2 {
                    return Err(to_py_err(
                        "[mdix] merge_files_weighted expects 2-tuples of (path, weight)",
                    ));
                }
                let path:   String = tuple.get_item(0)?.extract()?;
                let weight: f64    = tuple.get_item(1)?.extract()?;
                Ok((path, weight))
            })
            .collect()
    }

    fn merge_files_inner(&self, paths: &[String]) -> PyResult<MdixDatabase> {
        let data = self
            .build_core()
            .merge_files(paths)
            .map_err(|e| to_py_err(format!("[mdix:merge] {}", e)))?;
        Ok(MdixDatabase::from_data_pub(data))
    }

    /// Used directly by `MdixDatabase.merge_with` (via `build_core`, not
    /// this method itself — `merge_with` has no per-database weight
    /// parameter, so it goes through `merge_files` semantics: first path
    /// wins ties, not an explicit weight). Kept `pub(crate)` alongside
    /// `build_core` for that reason, even though only `merge_files_inner`
    /// calls it directly within this file.
    pub(crate) fn merge_files_weighted_inner(&self, pairs: &[(String, f64)]) -> PyResult<MdixDatabase> {
        let borrowed: Vec<(&str, f64)> = pairs.iter().map(|(p, w)| (p.as_str(), *w)).collect();
        let data = self
            .build_core()
            .merge_files_weighted(&borrowed)
            .map_err(|e| to_py_err(format!("[mdix:merge] {}", e)))?;
        Ok(MdixDatabase::from_data_pub(data))
    }
}

#[pymethods]
impl MdixMerger {
    #[new]
    fn new() -> Self {
        MdixMerger {
            strategy:       MdixMergeStrategy::default(),
            array_strategy: ArrayMergeStrategy::default(),
        }
    }

    /// Set the conflict resolution strategy.
    /// One of: "weighted_priority" (default) | "primary_wins" |
    /// "secondary_wins" | "throw_on_conflict".
    fn with_strategy(mut slf: PyRefMut<'_, Self>, strategy: &str) -> PyResult<Py<Self>> {
        slf.strategy = parse_merge_strategy(strategy)?;
        Ok(slf.into())
    }

    /// Set the array merge strategy.
    /// One of: "replace" | "concat" | "concat_dedup" (default).
    fn with_array_strategy(mut slf: PyRefMut<'_, Self>, strategy: &str) -> PyResult<Py<Self>> {
        slf.array_strategy = parse_array_strategy(strategy)?;
        Ok(slf.into())
    }

    /// Load and merge `.mdix` files from disk. Weights are assigned in
    /// descending order — the first path gets the highest priority (1.0),
    /// the last gets the lowest (approaching 0.0). Use
    /// `merge_files_weighted` for explicit weights.
    fn merge_files(&self, paths: &Bound<'_, PyList>) -> PyResult<MdixDatabase> {
        let paths = Self::extract_paths(paths)?;
        self.merge_files_inner(&paths)
    }

    /// Load and merge `.mdix` files from disk with explicit
    /// `(path, weight)` pairs. Higher weight wins under
    /// `"weighted_priority"`.
    fn merge_files_weighted(&self, paths_and_weights: &Bound<'_, PyList>) -> PyResult<MdixDatabase> {
        let pairs = Self::extract_weighted(paths_and_weights)?;
        self.merge_files_weighted_inner(&pairs)
    }

    /// Railway-style variant of `merge_files` — never raises, returns a
    /// failed `MdixResult` instead.
    fn try_merge_files(&self, py: Python<'_>, paths: &Bound<'_, PyList>) -> MdixResult {
        match Self::extract_paths(paths).and_then(|p| self.merge_files_inner(&p)) {
            Ok(db) => MdixResult::ok(py, db),
            Err(e) => MdixResult::err(e.to_string()),
        }
    }

    /// Railway-style variant of `merge_files_weighted`.
    fn try_merge_files_weighted(
        &self,
        py: Python<'_>,
        paths_and_weights: &Bound<'_, PyList>,
    ) -> MdixResult {
        match Self::extract_weighted(paths_and_weights)
            .and_then(|p| self.merge_files_weighted_inner(&p))
        {
            Ok(db) => MdixResult::ok(py, db),
            Err(e) => MdixResult::err(e.to_string()),
        }
    }

    /// Merge `.mdix` *source text* directly, without touching disk.
    ///
    /// Binding-layer convenience, not a core-crate feature: the core
    /// `compile_to_resolved_ast` is path-based only, so each source is
    /// written to a short-lived temp file and cleaned up immediately after
    /// compiling. `sources` is a list of `(label, source_text, weight)`
    /// triples — `label` is used only for conflict-report readability.
    fn merge_strings(&self, sources: &Bound<'_, PyList>) -> PyResult<MdixDatabase> {
        if sources.is_empty() {
            return Err(to_py_err("[mdix] merge_strings: no sources provided"));
        }

        let mut temp_files = Vec::with_capacity(sources.len());
        let mut weighted: Vec<(String, f64)> = Vec::with_capacity(sources.len());

        for item in sources.iter() {
            let tuple = item.downcast::<PyTuple>().map_err(|_| {
                to_py_err("[mdix] merge_strings expects a list of (label, source, weight) tuples")
            })?;
            if tuple.len() != 3 {
                return Err(to_py_err(
                    "[mdix] merge_strings expects 3-tuples of (label, source, weight)",
                ));
            }
            let _label: String = tuple.get_item(0)?.extract()?;
            let source: String = tuple.get_item(1)?.extract()?;
            let weight: f64    = tuple.get_item(2)?.extract()?;

            let mut tmp = tempfile::Builder::new()
                .suffix(".mdix")
                .tempfile()
                .map_err(|e| to_py_err(format!("[mdix] failed to create temp file: {}", e)))?;
            tmp.write_all(source.as_bytes())
                .map_err(|e| to_py_err(format!("[mdix] failed to write temp file: {}", e)))?;
            let path = tmp.path().to_string_lossy().to_string();
            weighted.push((path, weight));
            temp_files.push(tmp); // kept alive until merge completes below
        }

        let result = self.merge_files_weighted_inner(&weighted);
        drop(temp_files); // explicit: temp files are deleted here, after compiling
        result
    }

    fn __repr__(&self) -> String {
        format!(
            "MdixMerger(strategy={:?}, array_strategy={:?})",
            self.strategy, self.array_strategy
        )
    }
                               }
