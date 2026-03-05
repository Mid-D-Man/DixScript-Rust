// mdix-cli/src/services/key_service.rs
//! Wraps KeyFileManager for key generation and inspection.

use std::path::Path;
use crate::commands::CliError;
use dixscript::Compiler::DLM::KeyManagement::KeyFileManager;

pub struct KeyGenResult {
    pub output_path: String,
    pub algorithm:   String,
    pub key_length:  usize,
    pub mode:        String,
}

pub struct KeyInfo {
    pub algorithm:       String,
    pub key_length:      usize,
    pub mode:            String,
    pub has_compression: bool,
    pub created:         String,
}

/// Generate a new `.mdix.key` file.
pub fn generate_key_file(
    output_path: &str,
    algorithm: &str,
    password_mode: bool,
) -> Result<KeyGenResult, CliError> {
    let dir = Path::new(output_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or(".");

    crate::services::file_io::ensure_dir(Path::new(dir))?;

    let manager = KeyFileManager::new("".to_string(), dir.to_string());

    manager
        .generate_key_file(output_path, algorithm, password_mode)
        .map_err(|e| CliError::KeyError(e))?;

    let key_length = match algorithm.to_lowercase().as_str() {
        "aes128" => 16,
        "aes256" | "chacha20" => 32,
        _ => 32,
    };

    Ok(KeyGenResult {
        output_path: output_path.to_string(),
        algorithm:   algorithm.to_string(),
        key_length,
        mode:        if password_mode { "password" } else { "keyfile" }.to_string(),
    })
}

/// Validate an existing `.mdix.key` file.
pub fn validate_key_file(key_path: &str) -> Result<(), CliError> {
    if !Path::new(key_path).exists() {
        return Err(CliError::FileNotFound(Path::new(key_path).to_path_buf()));
    }

    let dir = Path::new(key_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or(".");

    let manager = KeyFileManager::new("".to_string(), dir.to_string());
    let data = manager.read_key_file(key_path).map_err(|e| CliError::KeyError(e))?;

    data.validate()
        .map_err(|errs| CliError::KeyError(errs.join(", ")))?;

    Ok(())
}

/// Read metadata from a `.mdix.key` file without validation.
pub fn get_key_info(key_path: &str) -> Result<KeyInfo, CliError> {
    if !Path::new(key_path).exists() {
        return Err(CliError::FileNotFound(Path::new(key_path).to_path_buf()));
    }

    let dir = Path::new(key_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or(".");

    let manager = KeyFileManager::new("".to_string(), dir.to_string());
    let data = manager.read_key_file(key_path).map_err(|e| CliError::KeyError(e))?;

    let (algorithm, key_length, mode) = data
        .key_data
        .encryption
        .as_ref()
        .map(|enc| {
            let mode = if enc.kdf.is_some() { "password" } else { "keyfile" };
            (enc.algorithm.clone(), enc.key_length, mode.to_string())
        })
        .unwrap_or_else(|| ("unknown".to_string(), 0, "unknown".to_string()));

    Ok(KeyInfo {
        algorithm,
        key_length,
        mode,
        has_compression: data.key_data.compression.is_some(),
        created: data.config.created_at.clone(),
    })
}
