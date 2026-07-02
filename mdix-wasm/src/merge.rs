// mdix-wasm/src/merge.rs
//
// Merge support for JS/TS — thin bindings over dixscript::Runtime::merge.
//
// Same rationale as mdix-lua's and mdix-python's merge.rs: this does NOT
// reimplement merging the way MidManStudio.Mdix.Core's MdixMerge.cs has
// to (JSON round-trip + a hand-written deep-merge in C#, with zero
// conflict reporting) — that approach exists there only because C# can
// only reach DixScript through the C ABI. mdix-wasm links directly
// against the `dixscript` crate, so it gets the *real* AST-level merger
// for free: weighted-priority resolution, per-source labels, array merge
// strategies, and a full conflict report.
//
// Unlike the Lua/Python merge functions, these take SOURCE STRINGS, not
// file paths — wasm32-unknown-unknown has no filesystem at all, so this
// binding can never read a file itself. You (Node's fs, or a browser
// fetch()) read the .mdix files and hand the content in here.
//
// ```js
// const a = await fs.readFile("base.mdix", "utf8");
// const b = await fs.readFile("patch.mdix", "utf8");
//
// const outcome = mergeSources([a, b]);
// const outcome = mergeSources([a, b], "primary_wins");
// const outcome = mergeSourcesWeighted([[a, 1.0], [b, 0.8]], "weighted");
//
// const db        = outcome.database();   // consumes the outcome
// const conflicts = outcome.conflicts();  // [{path, winningSource, winningLabel}, ...]
// ```
//
// `MdixDatabase.mergeWith(other, strategy, arrayStrategy)` merges two
// already-loaded in-memory databases using compile_to_resolved_ast_from_str
// — no temp files anywhere, since wasm32 has no filesystem at all.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use dixscript::Runtime::{
    ArrayMergeStrategy, DixData, DixLoader, MdixMergeInput, MdixMergeResult,
    MdixMergeStrategy, MdixMerger,
};

use crate::database::MdixDatabase;
use crate::error::runtime_err;

// ── Strategy parsing ─────────────────────────────────────────────────────────

fn parse_strategy(s: Option<String>) -> Result<MdixMergeStrategy, JsValue> {
    match s.as_deref().unwrap_or("weighted") {
        "weighted"          => Ok(MdixMergeStrategy::WeightedPriority),
        "primary_wins"      => Ok(MdixMergeStrategy::PrimaryWins),
        "secondary_wins"    => Ok(MdixMergeStrategy::SecondaryWins),
        "throw_on_conflict" => Ok(MdixMergeStrategy::ThrowOnConflict),
        other => Err(runtime_err("merge", format!(
            "unknown strategy '{}' — expected \
             \"weighted\" | \"primary_wins\" | \"secondary_wins\" | \"throw_on_conflict\"",
            other
        ))),
    }
}

fn parse_array_strategy(s: Option<String>) -> Result<ArrayMergeStrategy, JsValue> {
    match s.as_deref().unwrap_or("concat_dedup") {
        "replace"      => Ok(ArrayMergeStrategy::Replace),
        "concat"       => Ok(ArrayMergeStrategy::Concat),
        "concat_dedup" => Ok(ArrayMergeStrategy::ConcatDedup),
        other => Err(runtime_err("merge", format!(
            "unknown array_strategy '{}' — expected \
             \"replace\" | \"concat\" | \"concat_dedup\"",
            other
        ))),
    }
}

// ── MdixMergeOutcome ─────────────────────────────────────────────────────────

/// Returned by `mergeSources`, `mergeSourcesWeighted`, and
/// `MdixDatabase.mergeWith`. wasm-bindgen can't return a Rust tuple
/// directly, so this small wrapper carries both results instead.
#[wasm_bindgen]
pub struct MdixMergeOutcome {
    database:       Option<MdixDatabase>,
    conflicts_json: String,
}

#[wasm_bindgen]
impl MdixMergeOutcome {
    /// Consumes and returns the merged database. Can only be called once —
    /// like other consuming methods in this crate, calling it again raises
    /// rather than silently returning something stale.
    pub fn database(&mut self) -> Result<MdixDatabase, JsValue> {
        self.database.take().ok_or_else(|| {
            JsValue::from_str("[mdix] MdixMergeOutcome.database() already consumed")
        })
    }

    /// Conflicts as a real JS array of plain objects:
    /// `{path, winningSource, winningLabel}`.
    pub fn conflicts(&self) -> Result<JsValue, JsValue> {
        js_sys::JSON::parse(&self.conflicts_json)
    }
}

fn conflicts_to_json(result: &MdixMergeResult) -> Result<String, JsValue> {
    let arr: Vec<serde_json::Value> = result.conflicts.iter().map(|c| {
        serde_json::json!({
            "path":          c.path,
            "winningSource": c.winning_source,
            "winningLabel":  c.winning_label,
        })
    }).collect();
    serde_json::to_string(&arr)
        .map_err(|e| runtime_err("merge", format!("conflict serialize failed: {}", e)))
}

// ── Shared merge_all -> MdixMergeOutcome ──────────────────────────────────────

fn merge_all_to_outcome(
    sources:        Vec<MdixMergeInput>,
    strategy:       MdixMergeStrategy,
    array_strategy: ArrayMergeStrategy,
) -> Result<MdixMergeOutcome, JsValue> {
    let result = MdixMerger::new()
        .with_strategy(strategy)
        .with_array_strategy(array_strategy)
        .merge_all(sources);

    if !result.is_success {
        return Err(runtime_err("merge", result.errors.join("; ")));
    }

    let conflicts_json = conflicts_to_json(&result)?;
    let data = DixData::from_ast(
        result.merged_ast,
        "1.0.0".to_string(),
        chrono::Utc::now(),
        false,
        false,
        vec![],
    );
    Ok(MdixMergeOutcome {
        database: Some(MdixDatabase::from_data(data)),
        conflicts_json,
    })
}

// ── Module-level functions ────────────────────────────────────────────────────

/// Merge two or more .mdix source strings.
///
/// Sources are weighted in descending order: the first gets weight 1.0,
/// the last gets the lowest weight (only matters under "weighted" strategy).
#[wasm_bindgen(js_name = mergeSources)]
pub fn merge_sources(
    sources:        Vec<String>,
    strategy:       Option<String>,
    array_strategy: Option<String>,
) -> Result<MdixMergeOutcome, JsValue> {
    let strategy       = parse_strategy(strategy)?;
    let array_strategy = parse_array_strategy(array_strategy)?;

    if sources.is_empty() {
        return Err(runtime_err("mergeSources", "sources array is empty"));
    }
    let loader = DixLoader::new();
    let len    = sources.len();
    let mut inputs = Vec::with_capacity(len);
    for (i, source) in sources.into_iter().enumerate() {
        let weight = if len == 1 { 1.0 } else { 1.0 - (i as f64 / (len - 1) as f64) };
        let label  = format!("source[{}]", i);
        let ast    = loader.compile_to_resolved_ast_from_str(&source, &label)
            .map_err(|e| runtime_err("mergeSources", format!("{}: {}", label, e)))?;
        inputs.push(MdixMergeInput::new(ast).with_weight(weight).with_label(label));
    }
    merge_all_to_outcome(inputs, strategy, array_strategy)
}

/// Merge .mdix source strings with explicit per-source weights.
/// `entries` is a JS array of `[source, weight]` pairs.
#[wasm_bindgen(js_name = mergeSourcesWeighted)]
pub fn merge_sources_weighted(
    entries:        Vec<JsValue>,
    strategy:       Option<String>,
    array_strategy: Option<String>,
) -> Result<MdixMergeOutcome, JsValue> {
    let strategy       = parse_strategy(strategy)?;
    let array_strategy = parse_array_strategy(array_strategy)?;

    if entries.is_empty() {
        return Err(runtime_err("mergeSourcesWeighted", "entries array is empty"));
    }
    let loader = DixLoader::new();
    let mut inputs = Vec::with_capacity(entries.len());
    for (i, entry) in entries.into_iter().enumerate() {
        let pair: js_sys::Array = entry.dyn_into().map_err(|_| runtime_err(
            "mergeSourcesWeighted",
            format!("entry {} is not an array [source, weight]", i),
        ))?;
        let source: String = pair.get(0).as_string().ok_or_else(|| runtime_err(
            "mergeSourcesWeighted",
            format!("entry {}: source must be a string", i),
        ))?;
        let weight: f64 = pair.get(1).as_f64().ok_or_else(|| runtime_err(
            "mergeSourcesWeighted",
            format!("entry {}: weight must be a number", i),
        ))?;
        let label = format!("source[{}]", i);
        let ast   = loader.compile_to_resolved_ast_from_str(&source, &label)
            .map_err(|e| runtime_err("mergeSourcesWeighted", format!("{}: {}", label, e)))?;
        inputs.push(MdixMergeInput::new(ast).with_weight(weight).with_label(label));
    }
    merge_all_to_outcome(inputs, strategy, array_strategy)
}

// ── MdixDatabase.mergeWith ────────────────────────────────────────────────────

pub fn merge_with(
    primary:        &MdixDatabase,
    secondary:      &MdixDatabase,
    strategy:       Option<String>,
    array_strategy: Option<String>,
) -> Result<MdixMergeOutcome, JsValue> {
    let strategy       = parse_strategy(strategy)?;
    let array_strategy = parse_array_strategy(array_strategy)?;

    let primary_src   = primary.to_mdix()?;
    let secondary_src = secondary.to_mdix()?;

    let loader = DixLoader::new();
    let primary_ast = loader
        .compile_to_resolved_ast_from_str(&primary_src, "primary")
        .map_err(|e| runtime_err("mergeWith", e))?;
    let secondary_ast = loader
        .compile_to_resolved_ast_from_str(&secondary_src, "secondary")
        .map_err(|e| runtime_err("mergeWith", e))?;

    let sources = vec![
        MdixMergeInput::new(primary_ast).with_weight(1.0).with_label("primary"),
        MdixMergeInput::new(secondary_ast).with_weight(0.5).with_label("secondary"),
    ];
    merge_all_to_outcome(sources, strategy, array_strategy)
  }
