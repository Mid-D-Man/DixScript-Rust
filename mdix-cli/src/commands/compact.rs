
use std::path::PathBuf;
use clap::Args;
use serde::Serialize;
use crate::commands::{handle_error, GlobalOpts};
use crate::output::{json_output, printer};
use crate::services::file_io;
use dixscript::Runtime::DixCompactor;

#[derive(Args)]
pub struct CompactArgs {
    /// Path to the .mdix file
    pub file: PathBuf,

    /// Output file path (defaults to <name>.compact.mdix)
    #[arg(short, long)]
    pub output: Option<String>,

    /// compact | minify | strip-comments
    #[arg(long, default_value = "compact")]
    pub mode: String,

    /// Print compression ratio after compacting
    #[arg(long)]
    pub ratio: bool,
}

#[derive(Serialize)]
struct CompactOutput {
    input_path:    String,
    output_path:   String,
    original_size: usize,
    compacted_size: usize,
    ratio:         f64,
}

pub fn run(args: CompactArgs, global: &GlobalOpts) -> i32 {
    let content = match file_io::read_file(&args.file) {
        Ok(s)  => s,
        Err(e) => return handle_error(&e, global.json),
    };

    let compacted = match args.mode.as_str() {
        "minify"          => DixCompactor::minify(&content),
        "compact"         => DixCompactor::compact(&content),
        "strip-comments"  => DixCompactor::remove_comments(&content),
        other => {
            let err = crate::commands::CliError::InvalidArgument(format!(
                "Unknown mode '{}'. Use: compact | minify | strip-comments",
                other
            ));
            return handle_error(&err, global.json);
        }
    };

    let out_path = match args.output {
        Some(ref p) => p.clone(),
        None => file_io::suffixed_output_path(&args.file, &args.mode)
            .to_string_lossy()
            .to_string(),
    };

    if let Err(e) = file_io::write_file(std::path::Path::new(&out_path), &compacted) {
        return handle_error(&e, global.json);
    }

    let original_size  = content.len();
    let compacted_size = compacted.len();
    let ratio = DixCompactor::get_compression_ratio(&content, &compacted);

    if global.json {
        json_output::print_result(CompactOutput {
            input_path: args.file.to_string_lossy().to_string(),
            output_path: out_path,
            original_size,
            compacted_size,
            ratio,
        });
        return 0;
    }

    if !global.quiet {
        printer::success(&format!("Compacted → {}", out_path));
        printer::kv("original", &file_io::format_size(original_size));
        printer::kv("compacted", &file_io::format_size(compacted_size));
        if args.ratio || global.verbose {
            printer::kv("ratio", &format!("{:.1}% reduction", ratio * 100.0));
        }
    }

    0
}
