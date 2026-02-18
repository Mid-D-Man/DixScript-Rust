// src/Runtime/key_resolver.rs
//! Key Resolver - resolves encryption keys and locates .dxkey files for .mdix loading
//!
//! Two separate concerns:
//! - `KeyFileResolver`: Finds the .dxkey file based on DixLoadOptions
//! - `KeyResolver`: Extracts/derives the actual encryption key bytes from a .dxkey file

use crate::Compiler::DLM::KeyManagement::{KeyFileManager, KeyFileMetadata, EncryptionMetadata};
use crate::ErrorManager::{ErrorManager, DlmErrorType, ErrorSeverity};
use crate::Runtime::load_options::DixLoadOptions;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use argon2::{Argon2, Algorithm, Version, Params};
use std::path::{Path, PathBuf};
use std::collections::HashMap;

// ==================== KEY FILE SOURCE ====================
// (Describes where the .dxkey file was found)

#[derive(Debug, Clone)]
pub enum KeyFileSource {
    FilePath,
    AutoDetected,
    DirectContent,
    Url,
}

// ==================== KEY FILE RESOLUTION ====================
// (The result of locating a .dxkey file: its content and origin)

#[derive(Debug, Clone)]
pub struct KeyFileResolution {
    pub source: KeyFileSource,
    pub source_description: String,
    /// Raw JSON content of the .dxkey file
    pub content: String,
    /// Filesystem path, if applicable
    pub file_path: Option<PathBuf>,
}

// ==================== KEY FILE RESOLVER ====================
// (Finds the .dxkey file given load options and an encrypted file path)

pub struct KeyFileResolver {
    error_manager: ErrorManager,
}

impl KeyFileResolver {
    pub fn new() -> Self {
        KeyFileResolver {
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Locate and read the .dxkey file based on the provided load options.
    ///
    /// Priority order:
    /// 1. Direct content (from vault / secret manager)
    /// 2. Explicit key file path
    /// 3. Key file URL (HTTPS only)
    /// 4. Auto-detect from same directory as encrypted file
    pub fn resolve_key_file(
        &self,
        enc_path: &str,
        options: &DixLoadOptions,
    ) -> Result<KeyFileResolution, String> {
        // 1. Direct content
        if let Some(ref content) = options.key_file_content {
            if !options.allow_direct_key_content {
                return Err(
                    "Direct key content loading is disabled for security. \
                     Set allow_direct_key_content = true."
                        .to_string(),
                );
            }
            return Ok(KeyFileResolution {
                source: KeyFileSource::DirectContent,
                source_description: "Direct content provided by caller".to_string(),
                content: content.clone(),
                file_path: None,
            });
        }

        // 2. Explicit key file path
        if let Some(ref key_path) = options.key_file_path {
            let path = Path::new(key_path);
            if !path.exists() {
                return Err(format!("Explicit key file not found: {}", key_path));
            }
            let content = std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read key file '{}': {}", key_path, e))?;
            return Ok(KeyFileResolution {
                source: KeyFileSource::FilePath,
                source_description: format!("Explicit path: {}", key_path),
                content,
                file_path: Some(path.to_path_buf()),
            });
        }

        // 3. URL (async not supported here — return descriptive error)
        if let Some(ref url) = options.key_file_url {
            if !options.allow_url_key_loading {
                return Err("URL key loading is disabled. Set allow_url_key_loading = true.".to_string());
            }
            if !url.starts_with("https://") {
                return Err("Key file URL must use HTTPS protocol.".to_string());
            }
            return Err(
                "URL-based key loading requires an async runtime. \
                 Load the key file content manually and use DixLoadOptions::with_key_content() instead."
                    .to_string(),
            );
        }

        // 4. Auto-detect from same directory as .enc file
        self.auto_detect_key_file(enc_path, options)
    }

    fn auto_detect_key_file(
        &self,
        enc_path: &str,
        options: &DixLoadOptions,
    ) -> Result<KeyFileResolution, String> {
        let enc_path_buf = Path::new(enc_path);
        let dir = enc_path_buf.parent().unwrap_or_else(|| Path::new("."));

        // Derive base name by stripping .enc and .mdix extensions
        let file_name = enc_path_buf
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("output");

        let base_stem = file_name
            .strip_suffix(".enc")
            .unwrap_or(file_name)
            .strip_suffix(".mdix")
            .unwrap_or(file_name);

        // Build search dirs
        let mut search_dirs: Vec<PathBuf> = vec![dir.to_path_buf()];
        if let Some(ref paths) = options.key_file_search_paths {
            for p in paths {
                search_dirs.push(PathBuf::from(p));
            }
        }

        for search_dir in &search_dirs {
            let candidate = search_dir.join(format!("{}.dxkey", base_stem));
            if candidate.exists() {
                let content = std::fs::read_to_string(&candidate)
                    .map_err(|e| format!("Failed to read key file '{}': {}", candidate.display(), e))?;
                return Ok(KeyFileResolution {
                    source: KeyFileSource::AutoDetected,
                    source_description: format!("Auto-detected: {}", candidate.display()),
                    content,
                    file_path: Some(candidate),
                });
            }
        }

        Err(format!(
            "Key file '{}.dxkey' not found. Searched in: {}",
            base_stem,
            search_dirs
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

impl Default for KeyFileResolver {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== RESOLVED KEY ====================
// (Actual encryption key bytes, ready to hand to a cipher)

#[derive(Debug, Clone)]
pub struct ResolvedKey {
    /// Raw key bytes (16 for AES-128, 32 for AES-256 / ChaCha20)
    pub key_bytes: Vec<u8>,
    /// Initialization vector / nonce bytes
    pub iv_bytes: Vec<u8>,
    /// Algorithm name (e.g. "aes256-gcm")
    pub algorithm: String,
    /// Key length in bytes
    pub key_length: u32,
}

// ==================== KEY SOURCE ====================
// (How the caller wants to supply the encryption key)

#[derive(Debug, Clone)]
pub enum KeySource {
    /// Read key_data directly from .dxkey file (keyfile mode — no password needed)
    KeyFile(String),
    /// Derive key from password using KDF metadata stored in .dxkey
    Password {
        key_file_path: String,
        password: String,
    },
    /// Caller provides raw key bytes directly (advanced / testing only)
    RawBytes {
        key_bytes: Vec<u8>,
        iv_bytes: Vec<u8>,
        algorithm: String,
    },
}

// ==================== KEY RESOLVER ====================
// (Resolves the actual encryption key from a KeySource)

pub struct KeyResolver {
    error_manager: ErrorManager,
    source_file_path: String,
}

impl KeyResolver {
    pub fn new(source_file_path: String) -> Self {
        KeyResolver {
            error_manager: ErrorManager::get_shared_instance(),
            source_file_path,
        }
    }

    pub fn resolve(&self, source: &KeySource) -> Result<ResolvedKey, String> {
        match source {
            KeySource::KeyFile(key_file_path) => self.resolve_from_key_file(key_file_path),
            KeySource::Password { key_file_path, password } => {
                self.resolve_from_password(key_file_path, password)
            }
            KeySource::RawBytes { key_bytes, iv_bytes, algorithm } => {
                self.resolve_from_raw_bytes(key_bytes, iv_bytes, algorithm)
            }
        }
    }

    // ---- Keyfile mode ----

    fn resolve_from_key_file(&self, key_file_path: &str) -> Result<ResolvedKey, String> {
        self.log_debug(&format!("Resolving key from key file: {}", key_file_path));

        let manager = self.make_key_file_manager(key_file_path);
        let metadata = manager.read_key_file(key_file_path)?;
        manager.validate_key_file(&metadata)?;

        let enc = self.require_enc_meta(&metadata)?;

        let key_data = enc.key_data.as_ref().ok_or_else(|| {
            let msg = format!(
                "Key file '{}' has no key_data. Was it created in password mode? \
                 Use KeySource::Password instead.",
                key_file_path
            );
            self.report_error(&msg);
            msg
        })?;

        let key_bytes = BASE64.decode(key_data).map_err(|e| {
            let msg = format!("Failed to base64-decode key_data: {}", e);
            self.report_error(&msg);
            msg
        })?;

        let iv_bytes = self.decode_iv(enc, key_file_path)?;

        Ok(ResolvedKey {
            key_bytes,
            iv_bytes,
            algorithm: enc.algorithm.clone(),
            key_length: enc.key_length,
        })
    }

    // ---- Password mode (Argon2id KDF) ----

    fn resolve_from_password(
        &self,
        key_file_path: &str,
        password: &str,
    ) -> Result<ResolvedKey, String> {
        if password.is_empty() {
            let msg = "Password cannot be empty".to_string();
            self.report_error(&msg);
            return Err(msg);
        }

        self.log_debug(&format!(
            "Deriving key from password using key file: {}",
            key_file_path
        ));

        let manager = self.make_key_file_manager(key_file_path);
        let metadata = manager.read_key_file(key_file_path)?;
        manager.validate_key_file(&metadata)?;

        if !manager.is_password_protected(&metadata) {
            let msg = format!(
                "Key file '{}' is not password-protected. Use KeySource::KeyFile instead.",
                key_file_path
            );
            self.report_error(&msg);
            return Err(msg);
        }

        let enc = self.require_enc_meta(&metadata)?;

        // Decode salt
        let salt_b64 = enc.salt.as_ref().ok_or_else(|| {
            let msg = "Password-protected key file is missing the salt field".to_string();
            self.report_error(&msg);
            msg
        })?;
        let salt = BASE64.decode(salt_b64).map_err(|e| {
            let msg = format!("Failed to base64-decode salt: {}", e);
            self.report_error(&msg);
            msg
        })?;

        // KDF parameters from key file metadata (with sane defaults)
        let key_length = enc.key_length as usize;
        let t_cost = enc.kdf_iterations.unwrap_or(3);
        let m_cost = enc.kdf_memory.unwrap_or(65536);
        let p_cost = enc.kdf_parallelism.unwrap_or(4);

        self.log_debug(&format!(
            "Argon2id KDF: memory={}KB, iterations={}, parallelism={}, key_len={}",
            m_cost, t_cost, p_cost, key_length
        ));

        let params = Params::new(m_cost, t_cost, p_cost, Some(key_length)).map_err(|e| {
            let msg = format!("Invalid Argon2 parameters: {}", e);
            self.report_error(&msg);
            msg
        })?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut key_bytes = vec![0u8; key_length];
        argon2
            .hash_password_into(password.as_bytes(), &salt, &mut key_bytes)
            .map_err(|e| {
                let msg = format!("Key derivation failed: {}", e);
                self.report_error(&msg);
                msg
            })?;

        let iv_bytes = self.decode_iv(enc, key_file_path)?;

        Ok(ResolvedKey {
            key_bytes,
            iv_bytes,
            algorithm: enc.algorithm.clone(),
            key_length: enc.key_length,
        })
    }

    // ---- Raw bytes mode ----

    fn resolve_from_raw_bytes(
        &self,
        key_bytes: &[u8],
        iv_bytes: &[u8],
        algorithm: &str,
    ) -> Result<ResolvedKey, String> {
        if key_bytes.is_empty() {
            let msg = "Raw key bytes cannot be empty".to_string();
            self.report_error(&msg);
            return Err(msg);
        }
        if iv_bytes.is_empty() {
            let msg = "Raw IV bytes cannot be empty".to_string();
            self.report_error(&msg);
            return Err(msg);
        }

        let key_length = key_bytes.len() as u32;
        self.validate_key_length(key_length, algorithm)?;

        Ok(ResolvedKey {
            key_bytes: key_bytes.to_vec(),
            iv_bytes: iv_bytes.to_vec(),
            algorithm: algorithm.to_string(),
            key_length,
        })
    }

    // ---- Helpers ----

    fn make_key_file_manager(&self, key_file_path: &str) -> KeyFileManager {
        let output_dir = Path::new(key_file_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".")
            .to_string();
        KeyFileManager::new(self.source_file_path.clone(), output_dir)
    }

    fn require_enc_meta<'a>(
        &self,
        metadata: &'a KeyFileMetadata,
    ) -> Result<&'a EncryptionMetadata, String> {
        metadata.encryption.as_ref().ok_or_else(|| {
            let msg = format!(
                "Key file for '{}' contains no encryption metadata. \
                 The file may not have been compiled with an encryptor module.",
                self.source_file_path
            );
            self.report_error(&msg);
            msg
        })
    }

    fn decode_iv(&self, enc: &EncryptionMetadata, key_file_path: &str) -> Result<Vec<u8>, String> {
        let iv_b64 = enc.iv.as_ref().ok_or_else(|| {
            let msg = format!("Key file '{}' is missing IV/nonce field", key_file_path);
            self.report_error(&msg);
            msg
        })?;
        BASE64.decode(iv_b64).map_err(|e| {
            let msg = format!("Failed to base64-decode IV: {}", e);
            self.report_error(&msg);
            msg
        })
    }

    fn validate_key_length(&self, key_length: u32, algorithm: &str) -> Result<(), String> {
        let expected: Option<u32> = match algorithm.to_lowercase().as_str() {
            "aes128" | "aes-128-gcm" | "aes128-gcm" => Some(16),
            "aes256" | "aes-256-gcm" | "aes256-gcm" => Some(32),
            "chacha20" | "chacha20poly1305" | "chacha20-poly1305" => Some(32),
            "xor" => None,
            _ => None,
        };
        if let Some(exp) = expected {
            if key_length != exp {
                let msg = format!(
                    "Key length {} bytes does not match algorithm '{}' (expected {} bytes)",
                    key_length, algorithm, exp
                );
                self.report_error(&msg);
                return Err(msg);
            }
        }
        Ok(())
    }

    #[inline]
    fn report_error(&self, message: &str) {
        self.error_manager.add_dlm_error(
            DlmErrorType::InvocationFailed,
            message.to_string(),
            Some("KeyResolver".to_string()),
            None,
            None,
            ErrorSeverity::Error,
        );
    }

    #[inline]
    fn log_debug(&self, message: &str) {
        self.error_manager.log_debug(message);
    }
}

// ==================== TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_bytes_empty_key_fails() {
        let resolver = KeyResolver::new("test.mdix".to_string());
        let result = resolver.resolve(&KeySource::RawBytes {
            key_bytes: vec![],
            iv_bytes: vec![0u8; 12],
            algorithm: "aes256".to_string(),
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_raw_bytes_wrong_key_length_for_aes128() {
        let resolver = KeyResolver::new("test.mdix".to_string());
        let result = resolver.resolve(&KeySource::RawBytes {
            key_bytes: vec![0u8; 32],
            iv_bytes: vec![0u8; 12],
            algorithm: "aes128".to_string(),
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected 16 bytes"));
    }

    #[test]
    fn test_raw_bytes_correct_aes256() {
        let resolver = KeyResolver::new("test.mdix".to_string());
        let result = resolver.resolve(&KeySource::RawBytes {
            key_bytes: vec![0u8; 32],
            iv_bytes: vec![0u8; 12],
            algorithm: "aes256".to_string(),
        });
        assert!(result.is_ok());
        let key = result.unwrap();
        assert_eq!(key.key_length, 32);
        assert_eq!(key.algorithm, "aes256");
    }

    #[test]
    fn test_empty_password_fails() {
        let resolver = KeyResolver::new("test.mdix".to_string());
        let result = resolver.resolve(&KeySource::Password {
            key_file_path: "nonexistent.dxkey".to_string(),
            password: "".to_string(),
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }
}