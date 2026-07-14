
mod commands;
mod config;
mod output;
mod services;

use clap::{Parser, Subcommand};
use commands::{
    compact::CompactArgs, compile::CompileArgs, config::ConfigArgs,
    convert::ConvertArgs, create::CreateArgs, debug_ast::DebugAstArgs,
    debug_symbols::DebugSymbolsArgs, debug_tokens::DebugTokensArgs,
    decrypt::DecryptArgs, format::FormatArgs,
    inspect::InspectArgs, key::KeyArgs, merge::MergeArgs, validate::ValidateArgs,
};

#[derive(Parser)]
#[command(
    name    = "mdix",
    version = "1.0.0",
    about   = "DixScript (.mdix) file toolchain",
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, global = true)] pub verbose:  bool,
    #[arg(long, global = true)] pub quiet:    bool,
    #[arg(long, global = true)] pub json:     bool,
    #[arg(long, global = true)] pub no_color: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Validate a .mdix file without compiling
    Validate(ValidateArgs),
    /// Compile a .mdix file through the full pipeline
    Compile(CompileArgs),
    /// Decrypt a .mdix.enc file
    Decrypt(DecryptArgs),
    /// Convert between .mdix and other formats (json, toml)
    Convert(ConvertArgs),
    /// Merge two or more .mdix databases into one
    Merge(MergeArgs),
    /// Create a new .mdix file from a template
    Create(CreateArgs),
    /// Format a .mdix file in-place
    Format(FormatArgs),
    /// Compact or minify a .mdix file
    Compact(CompactArgs),
    /// Inspect the structure of a .mdix file
    Inspect(InspectArgs),
    /// Key file management
    Key(KeyArgs),
    /// CLI configuration
    Config(ConfigArgs),

    // ── Debug commands ──────────────────────────────────────────────────────
    /// [DEBUG] Print the token stream with positions and section tags
    ///
    /// Verifies @CONFIG is NOT in the token stream, section stamps are correct,
    /// and diagnoses hover/folding/completion bugs.
    ///
    ///   mdix debug-tokens config.mdix
    ///   mdix debug-tokens config.mdix --section-filter DATA
    #[command(name = "debug-tokens")]
    DebugTokens(DebugTokensArgs),

    /// [DEBUG] Print the parsed (and optionally enhanced) AST
    ///
    ///   mdix debug-ast config.mdix
    ///   mdix debug-ast config.mdix --section DATA
    #[command(name = "debug-ast")]
    DebugAst(DebugAstArgs),

    /// [DEBUG] Print the symbol table produced by semantic analysis
    ///
    /// Shows all registered enums, QuickFuncs, DATA variables, namespaces,
    /// and builtin statics. Use to verify semantic analysis output.
    ///
    ///   mdix debug-symbols config.mdix
    ///   mdix debug-symbols config.mdix --section ENUMS
    ///   mdix debug-symbols config.mdix --section DATA --verbose
    #[command(name = "debug-symbols")]
    DebugSymbols(DebugSymbolsArgs),
}

fn main() {
    let cli = Cli::parse();

    if cli.no_color {
        colored::control::set_override(false);
    }

    let global = commands::GlobalOpts {
        verbose: cli.verbose,
        quiet:   cli.quiet,
        json:    cli.json,
    };

    let exit_code = match cli.command {
        Commands::Validate(args)     => commands::validate::run(args, &global),
        Commands::Compile(args)      => commands::compile::run(args, &global),
        Commands::Decrypt(args)      => commands::decrypt::run(args, &global),
        Commands::Convert(args)      => commands::convert::run(args, &global),
        Commands::Merge(args)        => commands::merge::run(args, &global),
        Commands::Create(args)       => commands::create::run(args, &global),
        Commands::Format(args)       => commands::format::run(args, &global),
        Commands::Compact(args)      => commands::compact::run(args, &global),
        Commands::Inspect(args)      => commands::inspect::run(args, &global),
        Commands::Key(args)          => commands::key::run(args, &global),
        Commands::Config(args)       => commands::config::run(args, &global),
        Commands::DebugTokens(args)  => commands::debug_tokens::run(args, &global),
        Commands::DebugAst(args)     => commands::debug_ast::run(args, &global),
        Commands::DebugSymbols(args) => commands::debug_symbols::run(args, &global),
    };

    std::process::exit(exit_code);
        }
