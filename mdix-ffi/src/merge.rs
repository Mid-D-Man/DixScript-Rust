// C ABI merge support — thin wrapper over dixscript::Runtime::merge's
// MdixMerger, mirroring mdix-wasm/src/merge.rs (mergeSources /
// mergeSourcesWeighted) as closely as the C ABI allows. Exists so
// MidManStudio.Mdix.Core's C# MdixMerge.cs can use the *real* AST-level
// merger — weighted-priority conflict resolution, per-source conflict
// reporting, array merge strategies, full type fidelity for every
// DixScript value type (Long / Float / Double / HexColor / Blob / Regex /
// Date / Timestamp / Enum) — instead of reimplementing a much weaker
// version of it by hand in managed code via a JSON round-trip. See
// MdixMerge.cs's own doc comment for the before/after.
//
// Like mdix-wasm's binding, this takes SOURCE STRINGS, not existing loaded
// handles or file paths: MdixMerger operates on freshly-parsed DixScript
// ASTs, and an already-loaded MdixHandle only retains the resolved
// DixData, not the AST it came from. To merge two already-loaded
// databases, get each back to source text with the existing
// mdix_to_mdix() export first (mirrors mdix-wasm's MdixDatabase.mergeWith,
// which does the equivalent to_mdix() round-trip internally) — MdixMerge.cs
// does exactly this.
//
// NOTE: the #[repr(i32)] MdixMergeStrategy / ArrayMergeStrategy enum
// *definitions* live in lib.rs, not here — csbindgen's
// `.input_extern_file("src/lib.rs")` only scans lib.rs's own source text
// for the C# bindings it generates (see MdixType / MdixFormatMode, which
// are the existing precedent for this same constraint), so any enum used
// directly in an extern "C" fn signature has to be textually defined
// there. This file only holds plain Rust logic those two extern fns
// delegate into, the same role error.rs / handle.rs / string_utils.rs
// already play.

use std::os::raw::c_char;

use dixscript::Runtime::{
    ArrayMergeStrategy as CoreArrayStrategy, DixData, DixLoader, MdixMergeInput,
    MdixMergeResult, MdixMergeStrategy as CoreStrategy, MdixMerger,
};

use crate::handle::MdixHandle;
use crate::string_utils::c_str_to_str;
use crate::{ArrayMergeStrategy, MdixMergeStrategy};

impl MdixMergeStrategy {
    fn to_core(self) -> CoreStrategy {
        match self {
            MdixMergeStrategy::WeightedPriority => CoreStrategy::WeightedPriority,
            MdixMergeStrategy::PrimaryWins => CoreStrategy::PrimaryWins,
            MdixMergeStrategy::SecondaryWins => CoreStrategy::SecondaryWins,
            MdixMergeStrategy::ThrowOnConflict => CoreStrategy::ThrowOnConflict,
        }
    }
}

impl ArrayMergeStrategy {
    fn to_core(self) -> CoreArrayStrategy {
        match self {
            ArrayMergeStrategy::Replace => CoreArrayStrategy::Replace,
            ArrayMergeStrategy::Concat => CoreArrayStrategy::Concat,
            ArrayMergeStrategy::ConcatDedup => CoreArrayStrategy::ConcatDedup,
        }
    }
}

/// Reads a `*const *const c_char` / count pair into owned Rust strings.
/// Rejects (rather than skipping) any null pointer, invalid-UTF-8, or
/// empty entry — a silently-skipped source would shift every later
/// source's effective index and label, which would make conflict reports
/// point at the wrong source. Fail loudly instead.
///
/// # Safety
/// `sources` must point to `count` valid, non-dangling `*const c_char`
/// entries for the duration of this call.
pub unsafe fn read_source_array(
    sources: *const *const c_char,
    count: i32,
    fn_name: &str,
) -> Result<Vec<String>, String> {
    if sources.is_null() {
        return Err(format!("{}: sources is null", fn_name));
    }
    if count <= 0 {
        return Err(format!("{}: count must be >= 1 (got {})", fn_name, count));
    }
    let slice = std::slice::from_raw_parts(sources, count as usize);
    let mut out = Vec::with_capacity(slice.len());
    for (i, &ptr) in slice.iter().enumerate() {
        match c_str_to_str(ptr) {
            Some(s) if !s.trim().is_empty() => out.push(s.to_string()),
            Some(_) => return Err(format!("{}: sources[{}] is empty", fn_name, i)),
            None => return Err(format!(
                "{}: sources[{}] is null or invalid UTF-8", fn_name, i
            )),
        }
    }
    Ok(out)
}

/// `[{"path":"...","winningSource":0,"winningLabel":"..." | null}, ...]` —
/// same shape mdix-wasm's merge.rs produces, field for field, so any
/// consumer-side JSON parsing code (or docs) written against one is valid
/// for the other.
fn conflicts_to_json(result: &MdixMergeResult) -> Result<String, String> {
    let arr: Vec<serde_json::Value> = result.conflicts.iter().map(|c| {
        serde_json::json!({
            "path": c.path,
            "winningSource": c.winning_source,
            "winningLabel": c.winning_label,
        })
    }).collect();
    serde_json::to_string(&arr)
        .map_err(|e| format!("conflict report serialization failed: {}", e))
}

/// Shared implementation for `mdix_merge_sources` / `mdix_merge_sources_weighted`.
///
/// `weights`: `None` means auto-descending (source 0 gets weight 1.0, the
/// last source gets the lowest, only source gets 1.0) — matches
/// `MdixMerger::merge_files` / mdix-wasm's `mergeSources`. `Some(w)` must
/// be the same length as `source_strings` (checked by the caller before
/// this runs, so the mismatch error names the right fn).
///
/// Returns `(handle, conflicts_json)` on success — `conflicts_json` is
/// always valid JSON, `"[]"` when nothing conflicted. Returns `Err` with a
/// caller-ready message (already prefixed with `fn_name`) on failure; the
/// caller is responsible for routing that into `set_last_error`, matching
/// every other extern fn in lib.rs.
pub fn run_merge(
    fn_name: &str,
    source_strings: Vec<String>,
    weights: Option<Vec<f64>>,
    strategy: MdixMergeStrategy,
    array_strategy: ArrayMergeStrategy,
) -> Result<(*mut std::os::raw::c_void, String), String> {
    let n = source_strings.len();

    let loader = DixLoader::new();
    let mut inputs = Vec::with_capacity(n);
    for (i, source) in source_strings.into_iter().enumerate() {
        let weight = match &weights {
            Some(w) => w[i],
            None if n == 1 => 1.0,
            None => 1.0 - (i as f64 / (n - 1) as f64),
        };
        let label = format!("source[{}]", i);
        let ast = loader
            .compile_to_resolved_ast_from_str(&source, &label)
            .map_err(|e| format!("{}: {}: {}", fn_name, label, e))?;
        inputs.push(MdixMergeInput::new(ast).with_weight(weight).with_label(label));
    }

    let result = MdixMerger::new()
        .with_strategy(strategy.to_core())
        .with_array_strategy(array_strategy.to_core())
        .merge_all(inputs);

    if !result.is_success {
        return Err(format!("{}: {}", fn_name, result.errors.join("; ")));
    }

    let conflicts_json = conflicts_to_json(&result)
        .map_err(|e| format!("{}: {}", fn_name, e))?;

    let data = DixData::from_ast(
        result.merged_ast,
        "1.0.0".to_string(),
        chrono::Utc::now(),
        false,
        false,
        vec![],
    );
    let handle = MdixHandle::new(data) as *mut std::os::raw::c_void;
    Ok((handle, conflicts_json))
}
