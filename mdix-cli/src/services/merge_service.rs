//! Wraps `MdixMerger` for the `mdix merge` CLI command.

use std::path::{Path, PathBuf};
use std::time::Instant;

use dixscript::Compiler::AST::DixScript;
use dixscript::Runtime::{
    ArrayMergeStrategy, DixConverter, DixFormatOptions, DixLoader,
    MdixMergeInput, MdixMergeResult, MdixMergeStrategy, MdixMerger, MergeConflict,
};

use crate::commands::CliError;
use crate::services::conversion::Format;
use crate::services::file_io;

pub struct MergeOpts {
    pub strategy:       MdixMergeStrategy,
    pub array_strategy: ArrayMergeStrategy,
    pub weights:        Option<Vec<f64>>,
    pub labels:         Option<Vec<String>>,
    pub to:             Format,
    pub output:         Option<String>,
    pub pretty:         bool,
}

pub struct MergeResult {
    pub input_paths: Vec<String>,
    pub output_path: String,
    pub conflicts:   Vec<MergeConflict>,
    pub output_size: usize,
    pub elapsed:     std::time::Duration,
}

/// Parse a merge conflict-resolution strategy name from CLI input.
pub fn parse_strategy(s: &str) -> Result<MdixMergeStrategy, CliError> {
    match s.to_lowercase().replace('_', "-").as_str() {
        "weighted" | "weighted-priority" => Ok(MdixMergeStrategy::WeightedPriority),
        "primary" | "primary-wins"       => Ok(MdixMergeStrategy::PrimaryWins),
        "secondary" | "secondary-wins"   => Ok(MdixMergeStrategy::SecondaryWins),
        "throw" | "throw-on-conflict"    => Ok(MdixMergeStrategy::ThrowOnConflict),
        other => Err(CliError::InvalidArgument(format!(
            "Unknown merge strategy '{}'. Use: weighted, primary, secondary, throw",
            other
        ))),
    }
}

/// Parse an array-merge strategy name from CLI input.
pub fn parse_array_strategy(s: &str) -> Result<ArrayMergeStrategy, CliError> {
    match s.to_lowercase().replace('_', "-").as_str() {
        "concat-dedup" | "concatdedup" | "dedup" => Ok(ArrayMergeStrategy::ConcatDedup),
        "concat"                                  => Ok(ArrayMergeStrategy::Concat),
        "replace"                                  => Ok(ArrayMergeStrategy::Replace),
        other => Err(CliError::InvalidArgument(format!(
            "Unknown array merge strategy '{}'. Use: concat-dedup, concat, replace",
            other
        ))),
    }
}

/// Compile each input file to a resolved AST (tokenize → parse → semantic →
/// enhance → value-resolve, via `DixLoader::compile_to_resolved_ast`), merge
/// them with `MdixMerger`, and write the merged AST to `opts.output` in the
/// requested format.
///
/// Weight assignment: if `opts.weights` is `None`, files are assigned
/// descending weights (`files[0]` = 1.0, `files[last]` approaching 0.0),
/// matching `MdixMerger::merge_files`'s default — so by default earlier
/// files in the argument list win ties under `WeightedPriority`.
pub fn merge_files(files: &[PathBuf], opts: &MergeOpts) -> Result<MergeResult, CliError> {
    if files.len() < 2 {
        return Err(CliError::InvalidArgument(
            "merge requires at least 2 input files".to_string(),
        ));
    }

    for f in files {
        if !f.exists() {
            return Err(CliError::FileNotFound(f.clone()));
        }
    }

    if let Some(ref weights) = opts.weights {
        if weights.len() != files.len() {
            return Err(CliError::InvalidArgument(format!(
                "--weights has {} value(s) but {} file(s) were provided",
                weights.len(),
                files.len()
            )));
        }
    }

    if let Some(ref labels) = opts.labels {
        if labels.len() != files.len() {
            return Err(CliError::InvalidArgument(format!(
                "--labels has {} value(s) but {} file(s) were provided",
                labels.len(),
                files.len()
            )));
        }
    }

    let t = Instant::now();

    let loader = DixLoader::new();

    // Default weights: descending from 1.0 to 0.0, first file highest —
    // mirrors MdixMerger::merge_files's default weighting scheme.
    let n = files.len();
    let default_weights: Vec<f64> = (0..n)
        .map(|i| if n == 1 { 1.0 } else { 1.0 - (i as f64 / (n - 1) as f64) })
        .collect();

    let mut sources: Vec<MdixMergeInput> = Vec::with_capacity(n);
    for (i, file) in files.iter().enumerate() {
        let ast: DixScript = loader
            .compile_to_resolved_ast(file.to_str().unwrap_or(""))
            .map_err(CliError::CompileError)?;

        let weight = opts.weights.as_ref().map(|w| w[i]).unwrap_or(default_weights[i]);
        let label = opts
            .labels
            .as_ref()
            .map(|l| l[i].clone())
            .unwrap_or_else(|| file.to_string_lossy().to_string());

        sources.push(
            MdixMergeInput::new(ast)
                .with_weight(weight)
                .with_label(label),
        );
    }

    let merger = MdixMerger::new()
        .with_strategy(opts.strategy)
        .with_array_strategy(opts.array_strategy);

    let result: MdixMergeResult = merger.merge_all(sources);

    if !result.is_success {
        return Err(CliError::CompileError(format!(
            "Merge failed: {}",
            result.errors.join("; ")
        )));
    }

    let MdixMergeResult { merged_ast, conflicts, .. } = result;

    let converter = DixConverter::new();

    let output_content = match opts.to {
        Format::Mdix => {
            let fmt_opts = if opts.pretty {
                DixFormatOptions::pretty()
            } else {
                DixFormatOptions::new()
            };
            converter
                .to_mdix(&merged_ast, Some(&fmt_opts))
                .map_err(CliError::ConversionError)?
        }
        Format::Json => converter
            .to_json(&merged_ast, opts.pretty)
            .map_err(CliError::ConversionError)?,
        Format::Toml => converter
            .to_toml(&merged_ast)
            .map_err(CliError::ConversionError)?,
    };

    let output_path = match &opts.output {
        Some(p) => p.clone(),
        None => {
            let stem = files[0]
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("merged");
            let parent = files[0].parent().unwrap_or(Path::new("."));
            parent
                .join(format!("{}.merged.{}", stem, opts.to.extension()))
                .to_string_lossy()
                .to_string()
        }
    };

    let output_size = output_content.len();
    file_io::write_file(Path::new(&output_path), &output_content)?;

    Ok(MergeResult {
        input_paths: files.iter().map(|f| f.to_string_lossy().to_string()).collect(),
        output_path,
        conflicts,
        output_size,
        elapsed: t.elapsed(),
    })
  }
