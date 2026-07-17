//! Backs `mdix diff` by reusing `MdixMerger`'s conflict *detection* — same
//! machinery `mdix merge` uses, minus writing a merged file. Lets you
//! preview what a merge would need to resolve before committing to a
//! `--strategy`.

use std::path::PathBuf;
use std::time::Instant;

use dixscript::Compiler::AST::DixScript;
use dixscript::Runtime::{
    DixLoader, MdixMergeInput, MdixMergeResult, MdixMergeStrategy, MdixMerger, MergeConflict,
};

use crate::commands::CliError;

pub struct DiffResult {
    pub input_paths: Vec<String>,
    pub conflicts: Vec<MergeConflict>,
    pub elapsed: std::time::Duration,
}

pub fn diff_files(files: &[PathBuf], labels: Option<Vec<String>>) -> Result<DiffResult, CliError> {
    if files.len() < 2 {
        return Err(CliError::InvalidArgument(
            "diff requires at least 2 input files".to_string(),
        ));
    }
    for f in files {
        if !f.exists() {
            return Err(CliError::FileNotFound(f.clone()));
        }
    }
    if let Some(ref l) = labels {
        if l.len() != files.len() {
            return Err(CliError::InvalidArgument(format!(
                "--labels has {} value(s) but {} file(s) were provided",
                l.len(),
                files.len()
            )));
        }
    }

    let t = Instant::now();
    let loader = DixLoader::new();
    let n = files.len();

    // Equal weights on purpose, unlike merge_service's descending default:
    // diff isn't picking a winner, it's surfacing every path where sources
    // disagree, so nobody should "lose" a comparison just because of
    // argument order the way merge's default weighting would imply.
    let mut sources: Vec<MdixMergeInput> = Vec::with_capacity(n);
    for (i, file) in files.iter().enumerate() {
        let ast: DixScript = loader
            .compile_to_resolved_ast(file.to_str().unwrap_or(""))
            .map_err(CliError::CompileError)?;

        let label = labels
            .as_ref()
            .map(|l| l[i].clone())
            .unwrap_or_else(|| file.to_string_lossy().to_string());

        sources.push(MdixMergeInput::new(ast).with_weight(1.0).with_label(label));
    }

    // ThrowOnConflict surfaces every disagreement as a reportable conflict
    // instead of silently resolving it the way merge's default
    // (WeightedPriority) would. `result.errors`/`is_success` reflect that
    // ThrowOnConflict refuses to pick a winner when conflicts exist — that's
    // expected and fine here: diff wants the conflict *list*
    // (`result.conflicts`), not a successfully-merged AST, so a "failed"
    // merge with a populated conflicts Vec is the useful case, not an
    // error to bubble up.
    let merger = MdixMerger::new().with_strategy(MdixMergeStrategy::ThrowOnConflict);
    let result: MdixMergeResult = merger.merge_all(sources);

    Ok(DiffResult {
        input_paths: files.iter().map(|f| f.to_string_lossy().to_string()).collect(),
        conflicts: result.conflicts,
        elapsed: t.elapsed(),
    })
}
