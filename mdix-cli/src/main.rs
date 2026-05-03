// mdix-cli/src/main.rs
mod commands;
mod config;
mod output;
mod services;

use clap::{Parser, Subcommand};
use commands::{
    compact::CompactArgs, compile::CompileArgs, config::ConfigArgs,
    convert::ConvertArgs, create::CreateArgs, debug_ast::DebugAstArgs,
    debug_tokens::DebugTokensArgs, decrypt::DecryptArgs, format::FormatArgs,
    inspect::InspectArgs, key::KeyArgs, validate::ValidateArgs,
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
    /// Use this to verify @CONFIG is NOT in the token stream, that section
    /// stamps are correct, and to diagnose hover/folding/completion bugs.
    ///
    /// Example:
    ///   mdix debug-tokens config.mdix --by-line
    ///   mdix debug-tokens config.mdix --section-filter DATA --by-line
    ///   mdix debug-tokens config.mdix -o /tmp/tokens.txt
    #[command(name = "debug-tokens")]
    DebugTokens(DebugTokensArgs),

    /// [DEBUG] Print the parsed (and optionally enhanced) AST
    ///
    /// Use this to verify DATA entries are correctly classified, positions
    /// are sane, and enum/function declarations parsed correctly.
    ///
    /// Example:
    ///   mdix debug-ast config.mdix
    ///   mdix debug-ast config.mdix --section DATA
    ///   mdix debug-ast config.mdix --section ENUMS --positions false
    ///   mdix debug-ast config.mdix -o /tmp/ast.txt
    #[command(name = "debug-ast")]
    DebugAst(DebugAstArgs),
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
        Commands::Create(args)       => commands::create::run(args, &global),
        Commands::Format(args)       => commands::format::run(args, &global),
        Commands::Compact(args)      => commands::compact::run(args, &global),
        Commands::Inspect(args)      => commands::inspect::run(args, &global),
        Commands::Key(args)          => commands::key::run(args, &global),
        Commands::Config(args)       => commands::config::run(args, &global),
        Commands::DebugTokens(args)  => commands::debug_tokens::run(args, &global),
        Commands::DebugAst(args)     => commands::debug_ast::run(args, &global),
    };

    std::process::exit(exit_code);
}
