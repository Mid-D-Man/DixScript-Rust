// mdix-python/src/merge.rs
//! Merge support for Python — thin bindings over dixscript::Runtime::merge.
//!
//! Same rationale as mdix-lua's merge.rs: this does NOT reimplement
//! merging the way MidManStudio.Mdix.Core's MdixMerge.cs has to (JSON
//! round-trip + a hand-written deep-merge in C#, with zero conflict
//! reporting) — that approach exists there only because C# can only
//! reach DixScript through the C ABI. mdix-python links directly against
//! the `dixscript` crate, so it gets the *real* AST-level merger for
//! free: weighted-priority resolution, per-source labels, array merge
//! strategies, and a full conflict report. Long, Float, Double, enums,
//! and every other DixScript type survive the merge exactly as-is — no
//! information loss through JSON flattening.
//!
//! Exposed as module-level functions (registered in lib.rs), not as
//! `MdixDatabase` methods, because merging operates over files/ASTs
//! rather than already-resolved `DixData`:
//!
//! ```python
//! from midmanstudio.mdix import merge_files, merge_files_weighted
//!
//! db, conflicts = merge_files(["base.mdix", "patch.mdix"])
//! db, conflicts = merge_files(["base.mdix", "patch.mdix"], strategy="primary_wins")
//! db, conflicts = merge_files_weighted(
//!     [("base.mdix", 1.0), ("patch.mdix", 0.8)], strategy="weighted")
//! ```
//!
//! Both return `(database, conflicts)` — `conflicts` is a list of dicts
//! shaped `{"path": ..., "winning_source": ..., "winning_label": ...}`.
//!
//! `MdixDatabase.merge_with(other, strategy, array_strategy, temp_dir)`
//! merges two already-loaded in-memory databases. `DixData` does not
//! retain its source AST after resolution, so this round-trips through a
//! pair of temp files using the same `to_mdix()` serialization the rest
//! of the API already exposes, then re-parses and merges at the AST level
//! exactly like `merge_files` does. `temp_dir` defaults to
//! `std::env::temp_dir()` but can be overridden — see `merge_with`'s doc
//! comment below for why that matters on sandboxed targets.

use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use dixscript::Runtime::{
    ArrayMergeStrategy, DixData, DixLoader, MdixMergeInput, MdixMergeResult,
    MdixMergeStrategy, MdixMerger,
};

use crate::database::MdixDatabase;
use crate::error::{runtime_err, to_py_err};

// ── Strategy parsing ──────────────────────────────────────────────────────────

fn parse_strategy(s: Option<String>) -> PyResult<MdixMergeStrategy> {
    match s.as_deref().unwrap_or("weighted") {
        "weighted"          => Ok(MdixMergeStrategy::WeightedPriority),
        "primary_wins"      => Ok(MdixMergeStrategy::PrimaryWins),
        "secondary_wins"    => Ok(MdixMergeStrategy::SecondaryWins),
        "throw_on_conflict" => Ok(MdixMergeStrategy::ThrowOnConflict),
        other => Err(to_py_err(format!(
            "[mdix:merge] unknown strategy '{}' — expected \
             \"weighted\" | \"primary_wins\" | \"secondary_wins\" | \"throw_on_conflict\"",
            other
        ))),
    }
}

fn parse_array_strategy(s: Option<String>) -> PyResult<ArrayMergeStrategy> {
    match s.as_deref().unwrap_or("concat_dedup") {
        "replace"      => Ok(ArrayMergeStrategy::Replace),
        "concat"       => Ok(ArrayMergeStrategy::Concat),
        "concat_dedup" => Ok(ArrayMergeStrategy::ConcatDedup),
        other => Err(to_py_err(format!(
            "[mdix:merge] unknown array_strategy '{}' — expected \
             \"replace\" | \"concat\" | \"concat_dedup\"",
            other
        ))),
    }
}

// ── Conflict list ─────────────────────────────────────────────────────────────

fn conflicts_to_py(py: Python<'_>, result: &MdixMergeResult) -> PyResult<Vec<PyObject>> {
    let mut out = Vec::with_capacity(result.conflicts.len());
    for c in &result.conflicts {
        let d = PyDict::new_bound(py);
        d.set_item("path", &c.path)?;
        d.set_item("winning_source", c.winning_source as i64)?;
        match &c.winning_label {
            Some(label) => d.set_item("winning_label", label)?,
            None        => d.set_item("winning_label", py.None())?,
        }
        out.push(d.into_py(py));
    }
    Ok(out)
}

// ── Shared merge_all -> (Database, conflicts) ────────────────────────────────

fn merge_all_to_database(
    py: Python<'_>,
    sources: Vec<MdixMergeInput>,
    strategy: MdixMergeStrategy,
    array_strategy: ArrayMergeStrategy,
) -> PyResult<(MdixDatabase, Vec<PyObject>)> {
    let result = MdixMerger::new()
        .with_strategy(strategy)
        .with_array_strategy(array_strategy)
        .merge_all(sources);

    if !result.is_success {
        return Err(runtime_err("merge", result.errors.join("; ")));
    }

    let conflicts = conflicts_to_py(py, &result)?;
    let data = DixData::from_ast(
        result.merged_ast,
        "1.0.0".to_string(),
        chrono::Utc::now(),
        false,
        false,
        vec![],
    );
    Ok((MdixDatabase::from_data_pub(data), conflicts))
}

// ── Module-level functions (registered from lib.rs) ──────────────────────────

/// `merge_files(paths, strategy=None, array_strategy=None)`
///
/// Files are weighted in descending order: the first path gets weight
/// 1.0, the last gets the lowest weight (weight only matters under the
/// "weighted" strategy). Returns `(database, conflicts)`.
#[pyfunction]
#[pyo3(signature = (paths, strategy = None, array_strategy = None))]
pub fn merge_files(
    py: Python<'_>,
    paths: Vec<String>,
    strategy: Option<String>,
    array_strategy: Option<String>,
) -> PyResult<(MdixDatabase, Vec<PyObject>)> {
    let strategy       = parse_strategy(strategy)?;
    let array_strategy = parse_array_strategy(array_strategy)?;

    if paths.is_empty() {
        return Err(runtime_err("merge_files", "paths list is empty"));
    }
    let loader = DixLoader::new();
    let len = paths.len();
    let mut sources = Vec::with_capacity(len);
    for (i, path) in paths.into_iter().enumerate() {
        let weight = if len == 1 { 1.0 } else { 1.0 - (i as f64 / (len - 1) as f64) };
        let ast = loader.compile_to_resolved_ast(&path)
            .map_err(|e| runtime_err("merge_files", format!("'{}': {}", path, e)))?;
        sources.push(MdixMergeInput::new(ast).with_weight(weight).with_label(path));
    }
    merge_all_to_database(py, sources, strategy, array_strategy)
}

/// `merge_files_weighted([(path, weight), ...], strategy=None, array_strategy=None)`
///
/// Returns `(database, conflicts)`.
#[pyfunction]
#[pyo3(signature = (entries, strategy = None, array_strategy = None))]
pub fn merge_files_weighted(
    py: Python<'_>,
    entries: Vec<(String, f64)>,
    strategy: Option<String>,
    array_strategy: Option<String>,
) -> PyResult<(MdixDatabase, Vec<PyObject>)> {
    let strategy       = parse_strategy(strategy)?;
    let array_strategy = parse_array_strategy(array_strategy)?;

    if entries.is_empty() {
        return Err(runtime_err("merge_files_weighted", "entries list is empty"));
    }
    let loader = DixLoader::new();
    let mut sources = Vec::with_capacity(entries.len());
    for (path, weight) in entries {
        let ast = loader.compile_to_resolved_ast(&path)
            .map_err(|e| runtime_err("merge_files_weighted", format!("'{}': {}", path, e)))?;
        sources.push(MdixMergeInput::new(ast).with_weight(weight).with_label(path));
    }
    merge_all_to_database(py, sources, strategy, array_strategy)
}

// ── MdixDatabase.merge_with(other, strategy, array_strategy, temp_dir) ───────

/// Merges two already-loaded in-memory databases. Returns `(database, conflicts)`.
///
/// `temp_dir` is an optional override for where the round-trip temp files
/// get written. Defaults to `std::env::temp_dir()`, which is fine on
/// desktop but is frequently NOT writable inside a sandboxed environment
/// (mobile app, restricted container, etc.) — pass a known-writable
/// directory explicitly in that case.
pub fn merge_with(
    py: Python<'_>,
    primary: &MdixDatabase,
    secondary: &MdixDatabase,
    strategy: Option<String>,
    array_strategy: Option<String>,
    temp_dir: Option<String>,
) -> PyResult<(MdixDatabase, Vec<PyObject>)> {
    let strategy       = parse_strategy(strategy)?;
    let array_strategy = parse_array_strategy(array_strategy)?;

    let primary_src   = primary.to_mdix_string()?;
    let secondary_src = secondary.to_mdix_string()?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();

    let base_dir: PathBuf = match temp_dir {
        Some(d) => PathBuf::from(d),
        None    => std::env::temp_dir(),
    };

    let primary_path: PathBuf   = base_dir.join(format!("mdix-merge-{}-{}-a.mdix", pid, stamp));
    let secondary_path: PathBuf = base_dir.join(format!("mdix-merge-{}-{}-b.mdix", pid, stamp));

    std::fs::write(&primary_path, &primary_src).map_err(|e| runtime_err(
        "merge_with",
        format!(
            "failed to write temp file at '{}': {}. If this is a sandboxed \
             environment, std::env::temp_dir() may not be writable — pass an \
             explicit temp_dir argument pointing at a writable directory.",
            primary_path.display(), e
        ),
    ))?;
    std::fs::write(&secondary_path, &secondary_src).map_err(|e| {
        let _ = std::fs::remove_file(&primary_path);
        runtime_err(
            "merge_with",
            format!("failed to write temp file at '{}': {}", secondary_path.display(), e),
        )
    })?;

    let loader = DixLoader::new();
    let result = (|| -> PyResult<(MdixDatabase, Vec<PyObject>)> {
        let primary_ast = loader
            .compile_to_resolved_ast(primary_path.to_string_lossy().as_ref())
            .map_err(|e| runtime_err("merge_with", e))?;
        let secondary_ast = loader
            .compile_to_resolved_ast(secondary_path.to_string_lossy().as_ref())
            .map_err(|e| runtime_err("merge_with", e))?;
        let sources = vec![
            MdixMergeInput::new(primary_ast).with_weight(1.0).with_label("primary"),
            MdixMergeInput::new(secondary_ast).with_weight(0.5).with_label("secondary"),
        ];
        merge_all_to_database(py, sources, strategy, array_strategy)
    })();

    let _ = std::fs::remove_file(&primary_path);
    let _ = std::fs::remove_file(&secondary_path);

    result
      }
