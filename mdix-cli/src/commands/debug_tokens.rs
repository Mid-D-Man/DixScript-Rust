// mdix-cli/src/commands/debug_tokens.rs
//! `mdix debug-tokens <file>` — print the token stream with positions,
//! section tags, and optional grouping by line.
//!
//! Use this to verify:
//!   - That @CONFIG tokens are NOT in the stream (they should be blank lines).
//!   - That every section's tokens carry the correct SectionId stamp.
//!   - That folding/completion/hover problems are tokeniser bugs vs parser bugs.

use std::path::PathBuf;
use clap::Args;
use dixscript::Compiler::Core::Config::ConfigSectionHandler;
use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Utilities::TokenDebugPrinter;
use crate::commands::{CliError, GlobalOpts};
use crate::services::file_io;

#[derive(Args)]
pub struct DebugTokensArgs {
    /// Path to the .mdix file
    pub file: PathBuf,

    /// Group tokens by line (easier to read)
    #[arg(long)]
    pub by_line: bool,

    /// Write output to this file instead of stdout
    #[arg(short, long)]
    pub output: Option<String>,

    /// Show which section each token belongs to (SectionId stamp)
    #[arg(long, default_value = "true")]
    pub sections: bool,

    /// Show line:column position for each token
    #[arg(long, default_value = "true")]
    pub positions: bool,

    /// Filter to only show tokens from this section (e.g. DATA, QUICKFUNCS)
    #[arg(long)]
    pub section_filter: Option<String>,

    /// Only show tokens of this type (substring match, e.g. Identifier, String)
    #[arg(long)]
    pub type_filter: Option<String>,
}

pub fn run(args: DebugTokensArgs, _global: &GlobalOpts) -> i32 {
    let source = match file_io::read_file(&args.file) {
        Ok(s)  => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 2;
        }
    };

    // Strip @CONFIG (same as the real pipeline).
    let mut config_handler = ConfigSectionHandler::new(None);
    let config_result = config_handler.process_config_section(&source);
    let settings = &config_result.operational_settings;

    // Show what @CONFIG was detected as.
    eprintln!("─── @CONFIG detection ───────────────────────────────────────");
    eprintln!("Features:         {:?}", settings.enabled_features);
    eprintln!("Error handling:   {:?}", settings.error_handling_strategy);
    eprintln!("Debug mode:       {:?}", settings.debug_mode);
    eprintln!("");
    eprintln!("─── Cleaned source preview (first 20 lines) ─────────────────");
    for (i, line) in config_result.cleaned_input_string.lines().take(20).enumerate() {
        eprintln!("{:3}: {}", i + 1, line);
    }
    eprintln!("─────────────────────────────────────────────────────────────");
    eprintln!("");

    // Tokenize.
    let tokenizer = Tokenizer::new(&config_result.cleaned_input_string, settings);
    let mut tok_result = tokenizer.tokenize();

    // Apply filters.
    if let Some(ref section_name) = args.section_filter {
        let upper = section_name.to_uppercase();
        tok_result.tokens.retain(|t| {
            t.section.as_str().contains(&upper)
        });
    }
    if let Some(ref type_name) = args.type_filter {
        let lower = type_name.to_lowercase();
        tok_result.tokens.retain(|t| {
            format!("{:?}", t.token_type).to_lowercase().contains(&lower)
        });
    }

    // Print.
    let mut printer = TokenDebugPrinter::new(args.positions, args.sections, args.by_line);

    let output_content = printer.print(&tok_result);

    match args.output {
        Some(ref path) => {
            if let Err(e) = std::fs::write(path, &output_content) {
                eprintln!("Failed to write output: {}", e);
                return 1;
            }
            println!("Token debug written to: {}", path);
        }
        None => {
            print!("{}", output_content);
        }
    }

    0
}
