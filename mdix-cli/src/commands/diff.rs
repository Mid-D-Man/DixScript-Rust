use std::path::PathBuf;
use clap::Args;
use serde::Serialize;
use crate::commands::{handle_error, GlobalOpts};
use crate::output::{json_output, printer};
use crate::services::diff_service;

#[derive(Args)]
pub struct DiffArgs {
    /// The two (or more) .mdix files to compare
    #[arg(required = true, num_args = 2..)]
    pub files: Vec<PathBuf>,

    /// Human-readable labels for each file in the report, comma-separated
    /// (default: the file paths themselves)
    #[arg(long, value_delimiter = ',')]
    pub labels: Option<Vec<String>>,

    /// Exit with a non-zero status if any conflicts are found (for CI)
    #[arg(long)]
    pub fail_on_conflict: bool,
}

#[derive(Serialize)]
struct DiffOutput {
    input_paths: Vec<String>,
    conflict_count: usize,
    conflicts: Vec<String>,
    elapsed_ms: f64,
}

pub fn run(args: DiffArgs, global: &GlobalOpts) -> i32 {
    match diff_service::diff_files(&args.files, args.labels.clone()) {
        Ok(result) => {
            let conflict_strings: Vec<String> =
                result.conflicts.iter().map(|c| c.to_string()).collect();

            if global.json {
                json_output::print_result(DiffOutput {
                    input_paths: result.input_paths.clone(),
                    conflict_count: conflict_strings.len(),
                    conflicts: conflict_strings.clone(),
                    elapsed_ms: result.elapsed.as_secs_f64() * 1000.0,
                });
            } else if !global.quiet {
                if conflict_strings.is_empty() {
                    printer::success(&format!(
                        "No conflicts across {} file(s) — a merge would be clean",
                        result.input_paths.len()
                    ));
                } else {
                    printer::section(&format!("{} potential conflict(s)", conflict_strings.len()));
                    for c in &conflict_strings {
                        printer::warning(&format!("  {}", c));
                    }
                    printer::info(
                        "These are what `mdix merge` would need to resolve — run merge \
                         with --strategy to pick a winner, or --show-conflicts to see \
                         this same list alongside the merged output.",
                    );
                }
                if global.verbose {
                    printer::duration(result.elapsed);
                }
            }

            if args.fail_on_conflict && !conflict_strings.is_empty() {
                1
            } else {
                0
            }
        }
        Err(e) => handle_error(&e, global.json),
    }
}
