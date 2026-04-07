
use std::path::PathBuf;
use clap::Args;
use serde::Serialize;
use crate::commands::{handle_error, GlobalOpts};
use crate::output::{json_output, printer};
use crate::services::{file_io, validation};
use dixscript::Runtime::{DixConverter, DixFormatOptions, DixLoader, DixLoadOptions};

#[derive(Args)]
pub struct FormatArgs {
    /// Path to the .mdix file
    pub file: PathBuf,

    /// Write formatted output to this path instead of overwriting input
    #[arg(short, long)]
    pub output: Option<String>,

    /// Spaces per indent level
    #[arg(long, default_value = "2")]
    pub indent: usize,

    /// Use tabs instead of spaces
    #[arg(long)]
    pub tabs: bool,

    /// Exit 1 if file is not already formatted; do not write output
    #[arg(long)]
    pub check: bool,
}

#[derive(Serialize)]
struct FormatOutput {
    file_path:  String,
    already_formatted: bool,
}

pub fn run(args: FormatArgs, global: &GlobalOpts) -> i32 {
    if let Err(e) = validation::validate_file(&args.file, false) {
        return handle_error(&e, global.json);
    }

    let original = match file_io::read_file(&args.file) {
        Ok(s)  => s,
        Err(e) => return handle_error(&e, global.json),
    };

    let loader = DixLoader::new();
    let dix_data = match loader.load_text(
        args.file.to_str().unwrap_or(""),
        &DixLoadOptions::new(),
    ) {
        Ok(d)  => d,
        Err(e) => {
            let err = crate::commands::CliError::CompileError(e);
            return handle_error(&err, global.json);
        }
    };

    let mut fmt_opts = DixFormatOptions::new();
    fmt_opts.indent_size = args.indent;
    fmt_opts.use_tabs    = args.tabs;

    let converter = DixConverter::new();

    // Reconstruct a minimal AST from the loaded data for re-serialisation.
    let map = dix_data.to_hashmap();
    let ast = match converter.from_hashmap(map) {
        Ok(a)  => a,
        Err(e) => {
            let err = crate::commands::CliError::CompileError(e);
            return handle_error(&err, global.json);
        }
    };

    let formatted = match converter.to_mdix(&ast, Some(&fmt_opts)) {
        Ok(s)  => s,
        Err(e) => {
            let err = crate::commands::CliError::CompileError(e);
            return handle_error(&err, global.json);
        }
    };

    let already_formatted = formatted == original;

    if args.check {
        if global.json {
            json_output::print_result(FormatOutput {
                file_path: args.file.to_string_lossy().to_string(),
                already_formatted,
            });
        } else if already_formatted {
            printer::success(&format!("{} is already formatted", args.file.display()));
        } else {
            printer::error(&format!(
                "{} is not formatted — run without --check to fix",
                args.file.display()
            ));
        }
        return if already_formatted { 0 } else { 1 };
    }

    let out_path_str = args.output.clone().unwrap_or_else(|| {
        args.file.to_string_lossy().to_string()
    });

    if let Err(e) = file_io::write_file(std::path::Path::new(&out_path_str), &formatted) {
        return handle_error(&e, global.json);
    }

    if global.json {
        json_output::print_result(FormatOutput {
            file_path: out_path_str,
            already_formatted,
        });
    } else if !global.quiet {
        printer::success(&format!("Formatted {}", args.file.display()));
    }

    0
  }
