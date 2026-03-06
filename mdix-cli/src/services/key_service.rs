// dixscript-cli/src/services/key_service.rs
//! Wraps KeyFileManager for key generation and inspection.

use std::path::Path;
use crate::commands::CliError;
use dixscript::Compiler::DLM::KeyManagement::{
    KeyFileManager, KeyFileDataBuilder, EncryptionKeyData, MdixKeyWriter,
};

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

/// Generate a new `.dixscript.key` file with a random key and IV.
pub fn generate_key_file(
    output_path: &str,
    algorithm: &str,
    password_mode: bool,
) -> Result<KeyGenResult, CliError> {
    use rand::RngCore;
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    let dir = Path::new(output_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or(".");

    crate::services::file_io::ensure_dir(Path::new(dir))?;

    let key_length: usize = match algorithm.to_lowercase().as_str() {
        "aes128" => 16,
        "aes256" | "chacha20" => 32,
        _ => 32,
    };

    // AES-GCM and ChaCha20-Poly1305 both use a 12-byte nonce.
    let iv_length: usize = 12;

    let mut key_bytes = vec![0u8; key_length];
    let mut iv_bytes  = vec![0u8; iv_length];
    rand::thread_rng().fill_bytes(&mut key_bytes);
    rand::thread_rng().fill_bytes(&mut iv_bytes);

    let algo_label = match algorithm.to_lowercase().as_str() {
        "aes128"   => "aes128-gcm",
        "aes256"   => "aes256-gcm",
        "chacha20" => "chacha20-poly1305",
        other      => other,
    };

    let mut enc = EncryptionKeyData::new(algo_label.to_string());
    enc.key_length     = key_length;
    enc.iv             = BASE64.encode(&iv_bytes);
    enc.security_level = "HIGH".to_string();

    if !password_mode {
        enc.key_data = Some(BASE64.encode(&key_bytes));
    }

    let mode_str = if password_mode { "password" } else { "keyfile" };

    let data = KeyFileDataBuilder::new()
        .with_source_file(output_path.to_string())
        .with_encryption_mode(mode_str.to_string())
        .with_module(format!("DEncryptor.{}", algorithm))
        .with_encryption(enc)
        .build();

    let content = MdixKeyWriter::write(&data);
    std::fs::write(output_path, &content).map_err(CliError::IoError)?;

    Ok(KeyGenResult {
        output_path: output_path.to_string(),
        algorithm:   algorithm.to_string(),
        key_length,
        mode:        mode_str.to_string(),
    })
}

/// Validate an existing `.dixscript.key` file.
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

/// Read metadata from a `.dixscript.key` file without full validation.
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

    // `generated` is a DateTime<Utc>; format it as an RFC 3339 string.
    let created = data.config.generated.to_rfc3339();

    Ok(KeyInfo {
        algorithm,
        key_length,
        mode,
        has_compression: data.key_data.compression.is_some(),
        created,
    })
}
