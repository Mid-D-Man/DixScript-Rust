// dixscript-cli/src/commands/compile.rs

use std::path::PathBuf;
use clap::Args;
use serde::Serialize;
use crate::commands::{handle_error, GlobalOpts};
use crate::output::{json_output, printer};
use crate::services::compilation::{self, CompileOpts};

#[derive(Args)]
pub struct CompileArgs {
    /// Path to the .dixscript file
    pub file: PathBuf,

    /// Output directory for generated files
    #[arg(short, long)]
    pub output: Option<String>,

    /// Skip the DLM pipeline
    #[arg(long)]
    pub skip_dlm: bool,
}

#[derive(Serialize)]
struct CompileOutput {
    source_path:     String,
    generated_files: Vec<String>,
    original_size:   usize,
    modules_applied: Vec<String>,
    elapsed_ms:      f64,
}

pub fn run(args: CompileArgs, global: &GlobalOpts) -> i32 {
    let opts = CompileOpts {
        output_dir: args.output.clone(),
        skip_dlm:   args.skip_dlm,
    };

    match compilation::compile(&args.file, &opts) {
        Ok(result) => {
            if global.json {
                json_output::print_result(CompileOutput {
                    source_path:     result.source_path.clone(),
                    generated_files: result.generated_files.clone(),
                    original_size:   result.original_size,
                    modules_applied: result.modules_applied.clone(),
                    elapsed_ms:      result.elapsed.as_secs_f64() * 1000.0,
                });
                return 0;
            }

            if !global.quiet {
                printer::success(&format!("Compiled {}", result.source_path));
                printer::kv("original size",
                    &crate::services::file_io::format_size(result.original_size));

                if !result.modules_applied.is_empty() {
                    printer::kv("modules", &result.modules_applied.join(", "));
                }
                if !result.generated_files.is_empty() {
                    printer::section("Generated files");
                    printer::file_list(&result.generated_files);
                }
                if global.verbose {
                    printer::duration(result.elapsed);
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
