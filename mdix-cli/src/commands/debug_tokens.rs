//! `mdix debug-tokens <file>` — print the full token stream with positions,
//! section tags, and optional grouping by line.
//!
//! Uses Approach B (tokenizer-first): the full source is tokenized once,
//! producing ALL tokens including @CONFIG tokens with real positions. The
//! @CONFIG split is used only to extract operational settings for display.
//!
//! Use this to verify:
//!   - @CONFIG tokens ARE in the stream with SectionId::Config.
//!   - Every section's tokens carry the correct SectionId stamp.
//!   - Hover/folding/completion problems are tokeniser bugs vs parser bugs.

use std::path::PathBuf;
use clap::Args;
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, split_config_tokens, TokenType};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
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
    #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
    pub sections: bool,

    /// Show line:column position for each token
    #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
    pub positions: bool,

    /// Filter to only show tokens from this section (e.g. DATA, QUICKFUNCS, CONFIG)
    #[arg(long)]
    pub section_filter: Option<String>,

    /// Only show tokens of this type (substring match, e.g. Identifier, String)
    #[arg(long)]
    pub type_filter: Option<String>,
}

pub fn run(args: DebugTokensArgs, global: &GlobalOpts) -> i32 {
    let source = match file_io::read_file(&args.file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 2;
        }
    };

    // ── Approach B: tokenize the FULL source ──────────────────────────────
    let initial_settings = OperationalSettings {
        source_file_path: Some(args.file.to_string_lossy().to_string()),
        ..OperationalSettings::default()
    };

    let tokenizer = Tokenizer::new(&source, &initial_settings);
    let tok_result = tokenizer.tokenize();
    let total_tokens = tok_result.tokens.len();

    // Split and process @CONFIG to extract settings for display only.
    let split = split_config_tokens(tok_result.tokens.clone());
    let mut config_handler = ConfigSectionHandler::new(None);
    let config_result = config_handler.process_config_tokens(&split.config_tokens);
    let settings = &config_result.operational_settings;

    // Print config info to stderr so it doesn't mix with token output.
    eprintln!("─── @CONFIG settings ────────────────────────────────────────");
    eprintln!("Features:         {:?}", settings.enabled_features);
    eprintln!("Error handling:   {:?}", settings.error_handling_strategy);
    eprintln!("Debug mode:       {:?}", settings.debug_mode);
    eprintln!("Version:          {}", settings.version);
    eprintln!("");
    eprintln!("─── Source preview (first 20 lines) ─────────────────────────");
    for (i, line) in source.lines().take(20).enumerate() {
        eprintln!("{:3}: {}", i + 1, line);
    }
    eprintln!("─────────────────────────────────────────────────────────────");
    eprintln!("");
    eprintln!("Total tokens (including @CONFIG): {}", total_tokens);
    eprintln!("");

    // ── Build the token output ─────────────────────────────────────────────
    // Use the full token stream (tok_result.tokens was cloned above for split;
    // we re-tokenize with real settings so classification is accurate).
    let full_tokenizer = Tokenizer::new(&source, settings);
    let mut full_result = full_tokenizer.tokenize();

    // Apply filters.
    if let Some(ref section_name) = args.section_filter {
        let upper = section_name.to_uppercase();
        full_result.tokens.retain(|t| t.section.as_str().contains(&upper));
    }
    if let Some(ref type_name) = args.type_filter {
        let lower = type_name.to_lowercase();
        full_result.tokens.retain(|t| {
            format!("{:?}", t.token_type).to_lowercase().contains(&lower)
        });
    }

    let output_content = format_tokens(&full_result.tokens, &args);

    match args.output {
        Some(ref path) => {
            if let Err(e) = std::fs::write(path, &output_content) {
                eprintln!("Failed to write output: {}", e);
                return 1;
            }
            if !global.quiet {
                println!("Token debug written to: {}", path);
            }
        }
        None => {
            if !global.quiet {
                print!("{}", output_content);
            }
        }
    }

    0
}

// ── Token formatter ───────────────────────────────────────────────────────────

fn format_tokens(
    tokens: &[dixscript::Compiler::Core::Tokenizer::Token],
    args: &DebugTokensArgs,
) -> String {
    use dixscript::Compiler::Core::Tokenizer::TokenType;

    let mut out = String::new();
    let mut current_line: Option<usize> = None;

    for token in tokens {
        if matches!(token.token_type, TokenType::EndOfFile) {
            if args.by_line {
                out.push_str(&format!(
                    "L{:4}  [{}] EOF\n",
                    token.line,
                    token.section.as_str().to_lowercase()
                ));
            } else {
                out.push_str("EOF\n");
            }
            break;
        }

        if args.by_line && Some(token.line) != current_line {
            if current_line.is_some() {
                out.push('\n');
            }
            out.push_str(&format!("── Line {:4} ", token.line));
            out.push_str(&"─".repeat(60));
            out.push('\n');
            current_line = Some(token.line);
        }

        let type_str = format!("{}", token.token_type);
        let section_str = if args.sections {
            let s = token.section.as_str();
            if s.is_empty() { "       ".to_string() } else { format!("[{:<10}]", s.to_lowercase()) }
        } else {
            String::new()
        };
        let pos_str = if args.positions {
            format!("L{:4}:C{:<4} ", token.line, token.column)
        } else {
            String::new()
        };

        out.push_str(&format!("  {}{} {}\n", pos_str, section_str, type_str));
    }

    out
              }
