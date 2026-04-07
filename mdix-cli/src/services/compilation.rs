
//! Wraps the full dixscript compilation and load pipeline.

use std::path::Path;
use std::time::Instant;
use dixscript::Runtime::{DixLoader, DixLoadOptions};
use crate::commands::CliError;

#[derive(Debug)]
pub struct CompileResult {
    pub source_path:     String,
    pub generated_files: Vec<String>,
    pub original_size:   usize,
    pub elapsed:         std::time::Duration,
    pub modules_applied: Vec<String>,
}

pub struct CompileOpts {
    pub output_dir: Option<String>,
    pub skip_dlm:   bool,
    /// Password for password-mode DLM encryption.
    pub password:   Option<String>,
}

pub fn compile(path: &Path, opts: &CompileOpts) -> Result<CompileResult, CliError> {
    if !path.exists() {
        return Err(CliError::FileNotFound(path.to_path_buf()));
    }

    let source_text = std::fs::read_to_string(path).map_err(CliError::IoError)?;
    let original_size = source_text.len();

    let t = Instant::now();

    let mut load_opts = DixLoadOptions::new();
    if let Some(ref dir) = opts.output_dir {
        load_opts.output_directory = Some(dir.clone());
    }
    // Thread the password through so the DLM pipeline can access it via
    // DixLoadOptions. The encryptors read MDIX_DLM_PASSWORD from env —
    // set it here so password-mode encryption works in non-interactive contexts.
    if let Some(ref pw) = opts.password {
        load_opts.password = Some(pw.clone());
        // Set as env var so the encryptor layer can read it without changing
        // the DLMPipelineExecutor signature. This is intentionally scoped to
        // the current process only (not a child process).
        std::env::set_var("MDIX_DLM_PASSWORD", pw);
    }

    let loader = DixLoader::new();
    let dix_data = loader
        .load_text(path.to_str().unwrap_or(""), &load_opts)
        .map_err(CliError::CompileError)?;

    // Clear the env var after compilation so it doesn't leak to later commands
    // in the same process invocation.
    if opts.password.is_some() {
        std::env::remove_var("MDIX_DLM_PASSWORD");
    }

    let elapsed = t.elapsed();

    Ok(CompileResult {
        source_path:     path.to_string_lossy().to_string(),
        generated_files: dix_data.applied_modules.clone(),
        original_size,
        elapsed,
        modules_applied: dix_data.applied_modules,
    })
}

pub struct DecryptResult {
    pub source_path:    String,
    pub output_path:    String,
    pub encrypted_size: usize,
    pub elapsed:        std::time::Duration,
}

pub struct DecryptOpts {
    pub key_file_path: Option<String>,
    pub password:      Option<String>,
    pub output_dir:    Option<String>,
}

pub fn decrypt(path: &Path, opts: &DecryptOpts) -> Result<DecryptResult, CliError> {
    if !path.exists() {
        return Err(CliError::FileNotFound(path.to_path_buf()));
    }

    let encrypted_size = std::fs::metadata(path)
        .map(|m| m.len() as usize)
        .unwrap_or(0);

    let t = Instant::now();

    let mut load_opts = DixLoadOptions::new();

    if let Some(ref kp) = opts.key_file_path {
        // Explicit key path supplied by the caller.
        load_opts.key_file_path = Some(kp.clone());
    } else {
        // Auto-detect: strip .mdix.enc (or .enc) from the encrypted filename
        // and append .mdix.key.
        //
        // Without this, DixLoader would naively append .mdix.key to the full
        // encrypted filename, producing nonsense like
        // "foo.mdix.enc.mdix.key" instead of "foo.mdix.key".
        let enc_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let base = if enc_name.ends_with(".mdix.enc") {
            &enc_name[..enc_name.len() - ".mdix.enc".len()]
        } else if enc_name.ends_with(".enc") {
            &enc_name[..enc_name.len() - ".enc".len()]
        } else {
            enc_name
        };

        let key_filename = format!("{}.mdix.key", base);
        let parent = path.parent().unwrap_or(Path::new("."));
        let key_candidate = parent.join(&key_filename);

        if key_candidate.exists() {
            load_opts.key_file_path = Some(key_candidate.to_string_lossy().to_string());
        } else {
            // Surface a clear error rather than letting DixLoader produce a
            // confusing "file not found" message with a mangled filename.
            return Err(CliError::KeyError(format!(
                "Key file '{}' not found. Searched in: {}",
                key_filename,
                parent.display()
            )));
        }
    }

    if let Some(ref pw) = opts.password {
        load_opts.password = Some(pw.clone());
        std::env::set_var("MDIX_DLM_PASSWORD", pw);
    }

    if let Some(ref dir) = opts.output_dir {
        load_opts.output_directory = Some(dir.clone());
    }

    let loader = DixLoader::new();
    loader
        .load_encrypted(path.to_str().unwrap_or(""), &load_opts)
        .map_err(CliError::CompileError)?;

    if opts.password.is_some() {
        std::env::remove_var("MDIX_DLM_PASSWORD");
    }

    let output_dir = opts.output_dir.as_deref().unwrap_or(".");
    let stem = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output")
        .trim_end_matches(".enc");

    let output_path = format!("{}/{}", output_dir, stem);

    Ok(DecryptResult {
        source_path: path.to_string_lossy().to_string(),
        output_path,
        encrypted_size,
        elapsed: t.elapsed(),
    })
        }
