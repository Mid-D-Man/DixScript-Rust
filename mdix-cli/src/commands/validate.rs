
use std::path::PathBuf;
use clap::Args;
use serde::Serialize;
use crate::commands::{handle_error, GlobalOpts};
use crate::output::{json_output, printer};
use crate::services::validation;

#[derive(Args)]
pub struct ValidateArgs {
    /// Path to the .mdix file
    pub file: PathBuf,

    /// Treat warnings as errors
    #[arg(long)]
    pub strict: bool,
}

#[derive(Serialize)]
struct ValidateOutput {
    file:          String,
    token_count:   usize,
    warning_count: usize,
    warnings:      Vec<String>,
    elapsed_ms:    f64,
}

pub fn run(args: ValidateArgs, global: &GlobalOpts) -> i32 {
    match validation::validate_file(&args.file, args.strict) {
        Ok(result) => {
            if global.json {
                json_output::print_result(ValidateOutput {
                    file:          result.file_path.clone(),
                    token_count:   result.token_count,
                    warning_count: result.warning_count,
                    warnings:      result.warnings.clone(),
                    elapsed_ms:    result.elapsed.as_secs_f64() * 1000.0,
                });
                return 0;
            }

            if !global.quiet {
                printer::success(&format!("{} is valid", result.file_path));
                printer::kv("tokens",   &result.token_count.to_string());
                printer::kv("warnings", &result.warning_count.to_string());
                printer::duration(result.elapsed);

                if !result.warnings.is_empty() {
                    printer::section("Warnings");
                    for w in &result.warnings {
                        printer::warning(w);
                    }
                }
            }

            0
        }
        Err(e) => {
            if global.json {
                json_output::print_error(&e.to_string());
                return e.exit_code();
            }
            handle_error(&e, false)
        }
    }
  }
