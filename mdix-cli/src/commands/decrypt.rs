// mdix-cli/src/commands/decrypt.rs

use std::path::PathBuf;
use clap::Args;
use serde::Serialize;
use crate::commands::{handle_error, GlobalOpts};
use crate::output::{json_output, printer};
use crate::services::compilation::{self, DecryptOpts};
use crate::services::file_io;

#[derive(Args)]
pub struct DecryptArgs {
    /// Path to the .mdix.enc file
    pub file: PathBuf,

    /// Explicit key file path (auto-detected if omitted)
    #[arg(long)]
    pub key: Option<String>,

    /// Prompt for a password instead of using a key file
    #[arg(long)]
    pub password: bool,

    /// Output directory
    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Serialize)]
struct DecryptOutput {
    source_path:    String,
    output_path:    String,
    encrypted_size: String,
    elapsed_ms:     f64,
}

pub fn run(args: DecryptArgs, global: &GlobalOpts) -> i32 {
    let password = if args.password {
        match rpassword_prompt() {
            Ok(p)  => Some(p),
            Err(e) => {
                printer::error(&format!("Failed to read password: {}", e));
                return 1;
            }
        }
    } else {
        None
    };

    let opts = DecryptOpts {
        key_file_path: args.key.clone(),
        password,
        output_dir: args.output.clone(),
    };

    match compilation::decrypt(&args.file, &opts) {
        Ok(result) => {
            if global.json {
                json_output::print_result(DecryptOutput {
                    source_path:    result.source_path.clone(),
                    output_path:    result.output_path.clone(),
                    encrypted_size: file_io::format_size(result.encrypted_size),
                    elapsed_ms:     result.elapsed.as_secs_f64() * 1000.0,
                });
                return 0;
            }

            if !global.quiet {
                printer::success("Decryption successful");
                printer::kv("source",         &result.source_path);
                printer::kv("output",         &result.output_path);
                printer::kv("encrypted size", &file_io::format_size(result.encrypted_size));
                printer::duration(result.elapsed);
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

/// Read a password from the terminal without echoing characters.
fn rpassword_prompt() -> Result<String, String> {
    eprint!("Password: ");
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| e.to_string())?;
    Ok(input.trim_end_matches('\n').to_string())
  }
