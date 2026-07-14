// Merge support for Lua — thin bindings over dixscript::Runtime::merge.
//
// This does NOT reimplement merging the way MidManStudio.Mdix.Core's
// MdixMerge.cs has to (JSON round-trip + a hand-written deep-merge in C#,
// with zero conflict reporting) — that approach exists there only because
// C# can only reach DixScript through the C ABI. mdix-lua links directly
// against the `dixscript` crate, so it gets the *real* AST-level merger for
// free: weighted-priority resolution, per-source labels, array merge
// strategies, and a full conflict report. Long, Float, Double, enums, and
// every other DixScript type survive the merge exactly as-is — no
// information loss through JSON flattening.
//
// Exposed at module level (the `mdix` table), not as a Database method,
// because merging operates over files/ASTs rather than already-resolved
// DixData:
//
//   local db, conflicts = mdix.merge_files({"base.mdix", "patch.mdix"})
//   local db, conflicts = mdix.merge_files({"base.mdix", "patch.mdix"}, "primary_wins")
//   local db, conflicts = mdix.merge_files_weighted(
//       {{"base.mdix", 1.0}, {"patch.mdix", 0.8}}, "weighted")
//
// Both return (database, conflicts) — conflicts is a Lua array of tables
// shaped { path = ..., winning_source = ..., winning_label = ... }.
//
// Database:merge_with(other [, strategy [, array_strategy [, temp_dir]]])
// merges two already-loaded in-memory databases. DixData does not retain
// its source AST after resolution, so this round-trips through a pair of
// temp files using the same to_mdix() serialization the rest of the API
// already exposes, then re-parses and merges at the AST level exactly like
// merge_files does. temp_dir defaults to std::env::temp_dir() but can be
// overridden — see merge_with's doc comment below for why that matters on
// Android/sandboxed targets.

use std::path::PathBuf;

use mlua::{
    Error as LuaError, Lua, Result as LuaResult,
    Table as LuaTable, Value as LuaValue,
};
use dixscript::Runtime::{
    ArrayMergeStrategy, DixData, DixLoader, MdixMergeInput, MdixMergeResult,
    MdixMergeStrategy, MdixMerger,
};

use crate::database::LuaMdixDatabase;
use crate::error::mdix_err;

// ── Strategy parsing ─────────────────────────────────────────────────────────

fn parse_strategy(s: Option<String>) -> LuaResult<MdixMergeStrategy> {
    match s.as_deref().unwrap_or("weighted") {
        "weighted"          => Ok(MdixMergeStrategy::WeightedPriority),
        "primary_wins"      => Ok(MdixMergeStrategy::PrimaryWins),
        "secondary_wins"    => Ok(MdixMergeStrategy::SecondaryWins),
        "throw_on_conflict" => Ok(MdixMergeStrategy::ThrowOnConflict),
        other => Err(LuaError::RuntimeError(format!(
            "[mdix:merge] unknown strategy '{}' — expected \
             \"weighted\" | \"primary_wins\" | \"secondary_wins\" | \"throw_on_conflict\"",
            other
        ))),
    }
}

fn parse_array_strategy(s: Option<String>) -> LuaResult<ArrayMergeStrategy> {
    match s.as_deref().unwrap_or("concat_dedup") {
        "replace"      => Ok(ArrayMergeStrategy::Replace),
        "concat"       => Ok(ArrayMergeStrategy::Concat),
        "concat_dedup" => Ok(ArrayMergeStrategy::ConcatDedup),
        other => Err(LuaError::RuntimeError(format!(
            "[mdix:merge] unknown array_strategy '{}' — expected \
             \"replace\" | \"concat\" | \"concat_dedup\"",
            other
        ))),
    }
}

// ── Conflict table ───────────────────────────────────────────────────────────

fn conflicts_to_lua(lua: &Lua, result: &MdixMergeResult) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    for (i, c) in result.conflicts.iter().enumerate() {
        let row = lua.create_table()?;
        row.set("path", c.path.clone())?;
        row.set("winning_source", c.winning_source as i64)?;
        match &c.winning_label {
            Some(label) => row.set("winning_label", label.clone())?,
            None        => row.set("winning_label", LuaValue::Nil)?,
        }
        t.set(i + 1, row)?;
    }
    Ok(t)
}

// ── Shared merge_all → (Database, conflicts) ────────────────────────────────

fn merge_all_to_database(
    lua: &Lua,
    sources: Vec<MdixMergeInput>,
    strategy: MdixMergeStrategy,
    array_strategy: ArrayMergeStrategy,
) -> LuaResult<(LuaMdixDatabase, LuaTable)> {
    let result = MdixMerger::new()
        .with_strategy(strategy)
        .with_array_strategy(array_strategy)
        .merge_all(sources);

    if !result.is_success {
        return Err(mdix_err("merge", result.errors.join("; ")));
    }

    let conflicts = conflicts_to_lua(lua, &result)?;
    let data = DixData::from_ast(
        result.merged_ast,
        "1.0.0".to_string(),
        chrono::Utc::now(),
        false,
        false,
        vec![],
    );
    Ok((LuaMdixDatabase::from_data(data), conflicts))
}

// ── Module-level functions (registered from lib.rs) ─────────────────────────

/// `mdix.merge_files({"a.mdix", "b.mdix", ...} [, strategy [, array_strategy]])`
///
/// Files are weighted in descending order: the first path gets weight 1.0,
/// the last gets the lowest weight (weight only matters under the
/// "weighted" strategy). Returns `(database, conflicts)`.
pub fn merge_files(
    lua: &Lua,
    (paths, strategy, array_strategy): (LuaTable, Option<String>, Option<String>),
) -> LuaResult<(LuaMdixDatabase, LuaTable)> {
    let strategy       = parse_strategy(strategy)?;
    let array_strategy = parse_array_strategy(array_strategy)?;

    let len = paths.len()? as i64;
    if len == 0 {
        return Err(mdix_err("merge_files", "paths table is empty"));
    }
    let loader = DixLoader::new();
    let mut sources = Vec::with_capacity(len as usize);
    for i in 1..=len {
        let path: String = paths.get(i)?;
        let weight = if len == 1 { 1.0 } else { 1.0 - ((i - 1) as f64 / (len - 1) as f64) };
        let ast = loader.compile_to_resolved_ast(&path)
            .map_err(|e| mdix_err("merge_files", format!("'{}': {}", path, e)))?;
        sources.push(MdixMergeInput::new(ast).with_weight(weight).with_label(path));
    }
    merge_all_to_database(lua, sources, strategy, array_strategy)
}

/// `mdix.merge_files_weighted({{"a.mdix", 1.0}, {"b.mdix", 0.8}, ...} [, strategy [, array_strategy]])`
///
/// Returns `(database, conflicts)`.
pub fn merge_files_weighted(
    lua: &Lua,
    (entries, strategy, array_strategy): (LuaTable, Option<String>, Option<String>),
) -> LuaResult<(LuaMdixDatabase, LuaTable)> {
    let strategy       = parse_strategy(strategy)?;
    let array_strategy = parse_array_strategy(array_strategy)?;

    let len = entries.len()? as i64;
    if len == 0 {
        return Err(mdix_err("merge_files_weighted", "entries table is empty"));
    }
    let loader = DixLoader::new();
    let mut sources = Vec::with_capacity(len as usize);
    for i in 1..=len {
        let pair: LuaTable = entries.get(i)?;
        let path: String   = pair.get(1)?;
        let weight: f64    = pair.get(2)?;
        let ast = loader.compile_to_resolved_ast(&path)
            .map_err(|e| mdix_err("merge_files_weighted", format!("'{}': {}", path, e)))?;
        sources.push(MdixMergeInput::new(ast).with_weight(weight).with_label(path));
    }
    merge_all_to_database(lua, sources, strategy, array_strategy)
}

// ── Database:merge_with(other, strategy, array_strategy, temp_dir) ───────────

/// Merges two already-loaded in-memory databases. Returns `(database, conflicts)`.
///
/// `temp_dir` is an optional override for where the round-trip temp files
/// get written. Defaults to `std::env::temp_dir()`, which is fine on
/// desktop but is frequently NOT writable inside a sandboxed mobile app —
/// Android apps generally have no usable `/tmp` and no `TMPDIR` set, so
/// `std::env::temp_dir()` resolves to a path the process can't write to.
/// A host embedding this module (e.g. a game's modding layer running on
/// `mlua` + `vendored` + `lua54`) knows its own writable cache directory
/// and can pass it explicitly:
///
///   local db, conflicts = primary:merge_with(secondary, "weighted", nil, app_cache_dir)
pub fn merge_with(
    lua: &Lua,
    primary: &LuaMdixDatabase,
    secondary: &LuaMdixDatabase,
    strategy: Option<String>,
    array_strategy: Option<String>,
    temp_dir: Option<String>,
) -> LuaResult<(LuaMdixDatabase, LuaTable)> {
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

    std::fs::write(&primary_path, &primary_src).map_err(|e| mdix_err(
        "merge_with",
        format!(
            "failed to write temp file at '{}': {}. On Android/sandboxed \
             targets, std::env::temp_dir() is often not writable — pass an \
             explicit temp_dir (your app's cache directory) as the 4th argument.",
            primary_path.display(), e
        ),
    ))?;
    std::fs::write(&secondary_path, &secondary_src).map_err(|e| {
        let _ = std::fs::remove_file(&primary_path);
        mdix_err(
            "merge_with",
            format!("failed to write temp file at '{}': {}", secondary_path.display(), e),
        )
    })?;

    let loader = DixLoader::new();
    let result = (|| -> LuaResult<(LuaMdixDatabase, LuaTable)> {
        let primary_ast = loader
            .compile_to_resolved_ast(primary_path.to_string_lossy().as_ref())
            .map_err(|e| mdix_err("merge_with", e))?;
        let secondary_ast = loader
            .compile_to_resolved_ast(secondary_path.to_string_lossy().as_ref())
            .map_err(|e| mdix_err("merge_with", e))?;
        let sources = vec![
            MdixMergeInput::new(primary_ast).with_weight(1.0).with_label("primary"),
            MdixMergeInput::new(secondary_ast).with_weight(0.5).with_label("secondary"),
        ];
        merge_all_to_database(lua, sources, strategy, array_strategy)
    })();

    let _ = std::fs::remove_file(&primary_path);
    let _ = std::fs::remove_file(&secondary_path);

    result
}
