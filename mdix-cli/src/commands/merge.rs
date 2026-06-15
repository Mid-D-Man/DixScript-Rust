use std::path::PathBuf;
use clap::Args;
use serde::Serialize;
use crate::commands::{handle_error, GlobalOpts};
use crate::output::{json_output, printer};
use crate::services::conversion::Format;
use crate::services::file_io;
use crate::services::merge_service::{self, MergeOpts};

#[derive(Args)]
pub struct MergeArgs {
    /// Input .mdix files to merge (at least 2). Order matters: it's the
    /// tie-breaker for "weighted" (when weights are equal, earlier files
    /// win), and the sole rule for "primary"/"secondary".
    #[arg(required = true, num_args = 2..)]
    pub files: Vec<PathBuf>,

    /// Output file path. Format is inferred from this extension unless
    /// --to is given. Default: "<first-file-stem>.merged.<ext>" next to
    /// the first input file.
    #[arg(short, long)]
    pub output: Option<String>,

    /// Output format: mdix | json | toml (default: inferred from --output,
    /// or "mdix" if --output is also omitted)
    #[arg(long)]
    pub to: Option<String>,

    /// Conflict resolution strategy: weighted | primary | secondary | throw
    ///
    ///   weighted  - highest --weights value wins; ties go to the earlier file (default)
    ///   primary   - the earliest file in the list always wins
    ///   secondary - the latest file in the list always wins
    ///   throw     - any conflicting key is an error
    #[arg(long, default_value = "weighted")]
    pub strategy: String,

    /// Array merge strategy for GroupArray entries (and array-valued
    /// properties) that share a path across sources:
    ///
    ///   concat-dedup - concatenate, dropping exact-duplicate primitives (default)
    ///   concat       - concatenate everything, keep duplicates
    ///   replace      - winner's array entirely replaces the loser's
    #[arg(long = "array-strategy", default_value = "concat-dedup")]
    pub array_strategy: String,

    /// Explicit priority weights, one per file, comma-separated
    /// (e.g. "1.0,0.8,0.5"). Only meaningful for the "weighted" strategy.
    /// Default: descending weights, first file = 1.0.
    #[arg(long, value_delimiter = ',')]
    pub weights: Option<Vec<f64>>,

    /// Human-readable labels for each source file, comma-separated, used
    /// in conflict reports. Default: the file paths themselves.
    #[arg(long, value_delimiter = ',')]
    pub labels: Option<Vec<String>>,

    /// Pretty-print the output (default: true)
    #[arg(long, default_value = "true")]
    pub pretty: bool,

    /// Print every resolved conflict and which source won
    #[arg(long)]
    pub show_conflicts: bool,
}

#[derive(Serialize)]
struct MergeOutput {
    input_paths:    Vec<String>,
    output_path:    String,
    conflict_count: usize,
    conflicts:      Vec<String>,
    output_size:    usize,
    elapsed_ms:     f64,
}

pub fn run(args: MergeArgs, global: &GlobalOpts) -> i32 {
    let strategy = match merge_service::parse_strategy(&args.strategy) {
        Ok(s) => s,
        Err(e) => return handle_error(&e, global.json),
    };

    let array_strategy = match merge_service::parse_array_strategy(&args.array_strategy) {
        Ok(s) => s,
        Err(e) => return handle_error(&e, global.json),
    };

    // Resolve target format: explicit --to, else infer from --output's
    // extension, else default to mdix.
    let to_format: Format = match &args.to {
        Some(s) => match s.parse() {
            Ok(f) => f,
            Err(e) => {
                let err = crate::commands::CliError::UnsupportedFormat(e);
                return handle_error(&err, global.json);
            }
        },
        None => match &args.output {
            Some(path) => {
                crate::services::conversion::detect_format(std::path::Path::new(path))
                    .unwrap_or(Format::Mdix)
            }
            None => Format::Mdix,
        },
    };

    let opts = MergeOpts {
        strategy,
        array_strategy,
        weights: args.weights.clone(),
        labels: args.labels.clone(),
        to: to_format,
        output: args.output.clone(),
        pretty: args.pretty,
    };

    match merge_service::merge_files(&args.files, &opts) {
        Ok(result) => {
            let conflict_strings: Vec<String> =
                result.conflicts.iter().map(|c| c.to_string()).collect();

            if global.json {
                json_output::print_result(MergeOutput {
                    input_paths: result.input_paths.clone(),
                    output_path: result.output_path.clone(),
                    conflict_count: result.conflicts.len(),
                    conflicts: conflict_strings,
                    output_size: result.output_size,
                    elapsed_ms: result.elapsed.as_secs_f64() * 1000.0,
                });
                return 0;
            }

            if !global.quiet {
                printer::success(&format!(
                    "Merged {} files → {}",
                    result.input_paths.len(),
                    result.output_path
                ));
                printer::kv("inputs", &result.input_paths.join(", "));
                printer::kv("output size", &file_io::format_size(result.output_size));
                printer::kv("conflicts", &result.conflicts.len().to_string());

                if (args.show_conflicts || global.verbose) && !conflict_strings.is_empty() {
                    printer::section("Conflicts");
                    for c in &conflict_strings {
                        printer::info(&format!("  {}", c));
                    }
                }

                if global.verbose {
                    printer::duration(result.elapsed);
                }
            }

            0
        }
        Err(e) => handle_error(&e, global.json),
    }
}
