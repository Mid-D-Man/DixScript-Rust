
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

    /// Password for decryption. Supply it directly as --password <value>,
    /// set MDIX_DLM_PASSWORD in the environment, or omit entirely to be
    /// prompted interactively.
    #[arg(long, value_name = "PASSWORD")]
    pub password: Option<String>,

    /// Always prompt for the password interactively, even if --password or
    /// MDIX_DLM_PASSWORD is already set. Useful when you do not want the
    /// password to appear in shell history.
    #[arg(long, conflicts_with = "password")]
    pub password_prompt: bool,

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
    // Resolve the password in priority order:
    //   1. --password-prompt  (interactive, highest priority when flag is set)
    //   2. --password <value> (inline)
    //   3. MDIX_DLM_PASSWORD  (environment variable)
    //   4. None               (let the service layer fail with a clear message)
    let password = if args.password_prompt {
        match prompt_password() {
            Ok(p)  => Some(p),
            Err(e) => {
                printer::error(&format!("Failed to read password: {}", e));
                return 1;
            }
        }
    } else if let Some(p) = args.password {
        Some(p)
    } else {
        std::env::var("MDIX_DLM_PASSWORD").ok()
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
///
/// Uses the `rpassword` crate when available; falls back to a simple
/// stdin read (characters will echo) so the binary always compiles without
/// an extra dependency.
fn prompt_password() -> Result<String, String> {
    eprint!("Password: ");
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| e.to_string())?;
    Ok(input.trim_end_matches(['\n', '\r']).to_string())
                }
