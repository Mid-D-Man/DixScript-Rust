
use std::path::PathBuf;
use clap::Args;
use serde::Serialize;
use crate::commands::{handle_error, GlobalOpts};
use crate::output::{json_output, printer};
use crate::services::conversion::{self, ConvertOpts, Format};
use crate::services::file_io;

#[derive(Args)]
pub struct ConvertArgs {
    /// Input file path
    pub file: PathBuf,

    /// Target format: json, toml, mdix
    #[arg(long)]
    pub to: String,

    /// Source format override (auto-detected from extension if omitted)
    #[arg(long)]
    pub from: Option<String>,

    /// Output file path
    #[arg(short, long)]
    pub output: Option<String>,

    /// Pretty-print the output (default true)
    #[arg(long, default_value = "true")]
    pub pretty: bool,
}

#[derive(Serialize)]
struct ConvertOutput {
    input_path:   String,
    output_path:  String,
    input_size:   String,
    output_size:  String,
    size_ratio:   String,
    elapsed_ms:   f64,
}

pub fn run(args: ConvertArgs, global: &GlobalOpts) -> i32 {
    let to_format: Format = match args.to.parse() {
        Ok(f)  => f,
        Err(e) => {
            let err = crate::commands::CliError::UnsupportedFormat(e);
            return handle_error(&err, global.json);
        }
    };

    let from_format: Option<Format> = match args.from.as_deref() {
        Some(s) => match s.parse() {
            Ok(f)  => Some(f),
            Err(e) => {
                let err = crate::commands::CliError::UnsupportedFormat(e);
                return handle_error(&err, global.json);
            }
        },
        None => None,
    };

    let opts = ConvertOpts {
        from:   from_format,
        to:     to_format,
        output: args.output.clone(),
        pretty: args.pretty,
    };

    match conversion::convert_file(&args.file, &opts) {
        Ok(result) => {
            let ratio = if result.input_size > 0 {
                format!(
                    "{:.1}%",
                    (result.output_size as f64 / result.input_size as f64) * 100.0
                )
            } else {
                "N/A".to_string()
            };

            if global.json {
                json_output::print_result(ConvertOutput {
                    input_path:  result.input_path.clone(),
                    output_path: result.output_path.clone(),
                    input_size:  file_io::format_size(result.input_size),
                    output_size: file_io::format_size(result.output_size),
                    size_ratio:  ratio,
                    elapsed_ms:  result.elapsed.as_secs_f64() * 1000.0,
                });
                return 0;
            }

            if !global.quiet {
                printer::success(&format!("Converted to {}", result.output_path));
                printer::kv("input",  &file_io::format_size(result.input_size));
                printer::kv("output", &file_io::format_size(result.output_size));
                if global.verbose {
                    printer::duration(result.elapsed);
                }
            }
            0
        }
        Err(e) => handle_error(&e, global.json),
    }
  }
