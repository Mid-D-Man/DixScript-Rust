// mdix-cli/src/commands/key.rs

use clap::{Args, Subcommand};
use serde::Serialize;
use crate::commands::{handle_error, GlobalOpts};
use crate::output::{json_output, printer};
use crate::services::key_service;

#[derive(Args)]
pub struct KeyArgs {
    #[command(subcommand)]
    pub subcommand: KeySubcommand,
}

#[derive(Subcommand)]
pub enum KeySubcommand {
    /// Generate a new .mdix.key file
    Generate(KeyGenerateArgs),
    /// Validate an existing .mdix.key file
    Validate(KeyValidateArgs),
    /// Show metadata from a .mdix.key file
    Info(KeyInfoArgs),
}

#[derive(Args)]
pub struct KeyGenerateArgs {
    /// Output path for the generated key file
    #[arg(long, default_value = "output.mdix.key")]
    pub output: String,

    /// Encryption algorithm: aes128 | aes256 | chacha20
    #[arg(long, default_value = "aes256")]
    pub algorithm: String,

    /// Generate a password-mode key file instead of a random key
    #[arg(long)]
    pub password: bool,
}

#[derive(Args)]
pub struct KeyValidateArgs {
    /// Path to the .mdix.key file
    pub keyfile: String,
}

#[derive(Args)]
pub struct KeyInfoArgs {
    /// Path to the .mdix.key file
    pub keyfile: String,
}

#[derive(Serialize)]
struct GenerateOutput {
    output_path: String,
    algorithm:   String,
    key_length:  usize,
    mode:        String,
}

#[derive(Serialize)]
struct KeyInfoOutput {
    algorithm:       String,
    key_length:      usize,
    mode:            String,
    has_compression: bool,
    created:         String,
}

pub fn run(args: KeyArgs, global: &GlobalOpts) -> i32 {
    match args.subcommand {
        KeySubcommand::Generate(a) => run_generate(a, global),
        KeySubcommand::Validate(a) => run_validate(a, global),
        KeySubcommand::Info(a)     => run_info(a, global),
    }
}

fn run_generate(args: KeyGenerateArgs, global: &GlobalOpts) -> i32 {
    match key_service::generate_key_file(&args.output, &args.algorithm, args.password) {
        Ok(result) => {
            if global.json {
                json_output::print_result(GenerateOutput {
                    output_path: result.output_path.clone(),
                    algorithm:   result.algorithm.clone(),
                    key_length:  result.key_length,
                    mode:        result.mode.clone(),
                });
                return 0;
            }
            if !global.quiet {
                printer::success(&format!("Key file generated: {}", result.output_path));
                printer::kv("algorithm",  &result.algorithm);
                printer::kv("key length", &format!("{} bytes", result.key_length));
                printer::kv("mode",       &result.mode);
            }
            0
        }
        Err(e) => handle_error(&e, global.json),
    }
}

fn run_validate(args: KeyValidateArgs, global: &GlobalOpts) -> i32 {
    match key_service::validate_key_file(&args.keyfile) {
        Ok(()) => {
            if global.json {
                json_output::print_result(serde_json::json!({
                    "valid": true,
                    "keyfile": args.keyfile
                }));
                return 0;
            }
            if !global.quiet {
                printer::success(&format!("{} is valid", args.keyfile));
            }
            0
        }
        Err(e) => handle_error(&e, global.json),
    }
}

fn run_info(args: KeyInfoArgs, global: &GlobalOpts) -> i32 {
    match key_service::get_key_info(&args.keyfile) {
        Ok(info) => {
            if global.json {
                json_output::print_result(KeyInfoOutput {
                    algorithm:       info.algorithm.clone(),
                    key_length:      info.key_length,
                    mode:            info.mode.clone(),
                    has_compression: info.has_compression,
                    created:         info.created.clone(),
                });
                return 0;
            }
            if !global.quiet {
                printer::section("Key File Info");
                printer::kv("algorithm",       &info.algorithm);
                printer::kv("key length",      &format!("{} bytes", info.key_length));
                printer::kv("mode",            &info.mode);
                printer::kv("compression",     &info.has_compression.to_string());
                printer::kv("created",         &info.created);
            }
            0
        }
        Err(e) => handle_error(&e, global.json),
    }
  }
