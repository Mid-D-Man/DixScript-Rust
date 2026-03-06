// dixscript-cli/src/main.rs
//! Entry point — parses global flags, dispatches to subcommands.

mod commands;
mod config;
mod output;
mod services;

use clap::{Parser, Subcommand};
use commands::{
    compact::CompactArgs, compile::CompileArgs, config::ConfigArgs, convert::ConvertArgs,
    create::CreateArgs, decrypt::DecryptArgs, format::FormatArgs, inspect::InspectArgs,
    key::KeyArgs, validate::ValidateArgs,
};

#[derive(Parser)]
#[command(
    name = "dixscript",
    version = "1.0.0",
    about = "DixScript (.dixscript) file toolchain",
    long_about = None,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Detailed output including per-stage timing
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Suppress all non-error output
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Machine-readable JSON output
    #[arg(long, global = true)]
    pub json: bool,

    /// Disable colored output
    #[arg(long, global = true)]
    pub no_color: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Validate a .dixscript file without compiling
    Validate(ValidateArgs),
    /// Compile a .dixscript file through the full pipeline
    Compile(CompileArgs),
    /// Decrypt a .dixscript.enc file
    Decrypt(DecryptArgs),
    /// Convert between .dixscript and other formats (json, toml, yaml)
    Convert(ConvertArgs),
    /// Create a new .dixscript file from a template
    Create(CreateArgs),
    /// Format a .dixscript file in-place
    Format(FormatArgs),
    /// Compact or minify a .dixscript file
    Compact(CompactArgs),
    /// Inspect the structure of a .dixscript file
    Inspect(InspectArgs),
    /// Key file management
    Key(KeyArgs),
    /// CLI configuration
    Config(ConfigArgs),
}

fn main() {
    let cli = Cli::parse();

    if cli.no_color {
        colored::control::set_override(false);
    }

    let global = commands::GlobalOpts {
        verbose: cli.verbose,
        quiet: cli.quiet,
        json: cli.json,
    };

    let exit_code = match cli.command {
        Commands::Validate(args)  => commands::validate::run(args, &global),
        Commands::Compile(args)   => commands::compile::run(args, &global),
        Commands::Decrypt(args)   => commands::decrypt::run(args, &global),
        Commands::Convert(args)   => commands::convert::run(args, &global),
        Commands::Create(args)    => commands::create::run(args, &global),
        Commands::Format(args)    => commands::format::run(args, &global),
        Commands::Compact(args)   => commands::compact::run(args, &global),
        Commands::Inspect(args)   => commands::inspect::run(args, &global),
        Commands::Key(args)       => commands::key::run(args, &global),
        Commands::Config(args)    => commands::config::run(args, &global),
    };

    std::process::exit(exit_code);
      }
