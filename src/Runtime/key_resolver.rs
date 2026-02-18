//! Key Resolver - resolves encryption keys for .mdix file loading
//!
//! Handles:
//! - Reading .dxkey files to retrieve encryption metadata
//! - Password-based key derivation via Argon2
//! - Raw key extraction for keyfile mode
//! - Providing ready-to-use key bytes to the DLM reverse pipeline

use crate::Compiler::DLM::KeyManagement::{KeyFileManager, KeyFileMetadata, EncryptionMetadata, Argon2KDF};
use crate::Compiler::DLM::Encryptor::argon2_kdf::Argon2KDF;
use crate::ErrorManager::{ErrorManager, DlmErrorType, ErrorSeverity};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::collections::HashMap;
use std::path::Path;

// ==================== RESOLVED KEY ====================

/// Result of key resolution — ready-to-use key bytes + IV
#[derive(Debug, Clone)]
pub struct ResolvedKey {
    /// Raw key bytes (16 for AES-128, 32 for AES-256/ChaCha20)
    pub key_bytes: Vec<u8>,

    /// Initialization vector / nonce bytes
    pub iv_bytes: Vec<u8>,

    /// Algorithm that should be used with this key
    pub algorithm: String,

    /// Key length in bytes
    pub key_length: u32,
}

// ==================== KEY SOURCE ====================

/// How the caller wants to supply the key
#[derive(Debug, Clone)]
pub enum KeySource {
    /// Automatic — read from .dxkey file (keyfile mode, no password needed)
    KeyFile(String),

    /// Password-based — derive key from password using KDF metadata in .dxkey
    Password {
        key_file_path: String,
        password: String,
    },

    /// Raw bytes — caller provides key directly (advanced/testing use)
    RawBytes {
        key_bytes: Vec<u8>,
        iv_bytes: Vec<u8>,
        algorithm: String,
    },
}

// ==================== KEY RESOLVER ====================

/// Resolves encryption keys from .dxkey files or passwords
///
/// # Ownership note
/// `ErrorManager` is owned (not borrowed) because it wraps `Arc<Mutex<T>>`
/// internally — cloning/owning it is essentially free.
pub struct KeyResolver {
    error_manager: ErrorManager,   // ← owned, not &ErrorManager (fixes E0716)
    source_file_path: String,
}

impl KeyResolver {
    /// Create new KeyResolver for a given .mdix source path
    pub fn new(source_file_path: String) -> Self {
        KeyResolver {
            error_manager: ErrorManager::get_shared_instance(), // ← no & needed
            source_file_path,
        }
    }

    /// Resolve the key from the given source
    ///
    /// Returns `Ok(ResolvedKey)` on success, `Err(String)` with a human-readable
    /// message on failure.
    pub fn resolve(&self, source: &KeySource) -> Result<ResolvedKey, String> {
        match source {
            KeySource::KeyFile(key_file_path) => {
                self.resolve_from_key_file(key_file_path)
            }

            KeySource::Password { key_file_path, password } => {
                self.resolve_from_password(key_file_path, password)
            }

            KeySource::RawBytes { key_bytes, iv_bytes, algorithm } => {
                self.resolve_from_raw_bytes(key_bytes, iv_bytes, algorithm)
            }
        }
    }

    // ==================== KEY FILE MODE ====================

    /// Resolve key directly from .dxkey file (no password needed)
    fn resolve_from_key_file(&self, key_file_path: &str) -> Result<ResolvedKey, String> {
        self.log_debug(&format!("Resolving key from key file: {}", key_file_path));

        let manager = self.create_key_file_manager(key_file_path);
        let metadata = manager.read_key_file(key_file_path)?;

        manager.validate_key_file(&metadata)?;

        let enc = self.require_encryption_metadata(&metadata)?;

        // In keyfile mode, key_data is stored in the .dxkey file (base64)
        let key_data = enc.key_data.as_ref().ok_or_else(|| {
            let msg = format!(
                "Key file '{}' does not contain key_data. \
                 Was it created in password mode? Use KeySource::Password instead.",
                key_file_path
            );
            self.report_error(&msg);
            msg
        })?;

        let key_bytes = BASE64.decode(key_data).map_err(|e| {
            let msg = format!("Failed to decode key_data from key file: {}", e);
            self.report_error(&msg);
            msg
        })?;

        let iv_bytes = self.decode_iv(enc, key_file_path)?;

        self.log_debug(&format!(
            "Key resolved: algorithm={}, key_len={}, iv_len={}",
            enc.algorithm,
            key_bytes.len(),
            iv_bytes.len()
        ));

        Ok(ResolvedKey {
            key_bytes,
            iv_bytes,
            algorithm: enc.algorithm.clone(),
            key_length: enc.key_length,
        })
    }

    // ==================== PASSWORD MODE ====================

    /// Derive key from password using KDF metadata stored in .dxkey
    fn resolve_from_password(
        &self,
        key_file_path: &str,
        password: &str,
    ) -> Result<ResolvedKey, String> {
        self.log_debug(&format!(
            "Deriving key from password using key file: {}",
            key_file_path
        ));

        if password.is_empty() {
            let msg = "Password cannot be empty".to_string();
            self.report_error(&msg);
            return Err(msg);
        }

        let manager = self.create_key_file_manager(key_file_path);
        let metadata = manager.read_key_file(key_file_path)?;

        manager.validate_key_file(&metadata)?;

        if !manager.is_password_protected(&metadata) {
            let msg = format!(
                "Key file '{}' is not password-protected. \
                 Use KeySource::KeyFile instead.",
                key_file_path
            );
            self.report_error(&msg);
            return Err(msg);
        }

        let enc = self.require_encryption_metadata(&metadata)?;

        // Decode salt
        let salt_b64 = enc.salt.as_ref().ok_or_else(|| {
            let msg = "Password-protected key file missing salt".to_string();
            self.report_error(&msg);
            msg
        })?;

        let salt = BASE64.decode(salt_b64).map_err(|e| {
            let msg = format!("Failed to decode salt: {}", e);
            self.report_error(&msg);
            msg
        })?;

        // Build KDF config from metadata
        let kdf_config = self.build_kdf_config(enc);

        self.log_debug(&format!(
            "Running Argon2 KDF: iterations={:?}, memory={:?}, parallelism={:?}",
            enc.kdf_iterations, enc.kdf_memory, enc.kdf_parallelism
        ));

        // Derive key via Argon2
        let kdf = Argon2KDF::new();
        let key_bytes = kdf
            .derive_key(password, &salt, enc.key_length as usize, &kdf_config)
            .map_err(|e| {
                let msg = format!("Key derivation failed: {}", e);
                self.report_error(&msg);
                msg
            })?;

        let iv_bytes = self.decode_iv(enc, key_file_path)?;

        self.log_debug(&format!(
            "Key derived successfully: algorithm={}, key_len={}",
            enc.algorithm,
            key_bytes.len()
        ));

        Ok(ResolvedKey {
            key_bytes,
            iv_bytes,
            algorithm: enc.algorithm.clone(),
            key_length: enc.key_length,
        })
    }

    // ==================== RAW BYTES MODE ====================

    /// Use caller-supplied raw key bytes directly
    fn resolve_from_raw_bytes(
        &self,
        key_bytes: &[u8],
        iv_bytes: &[u8],
        algorithm: &str,
    ) -> Result<ResolvedKey, String> {
        self.log_debug("Using raw key bytes (advanced/testing mode)");

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

        self.validate_key_length_for_algorithm(key_length, algorithm)?;

        Ok(ResolvedKey {
            key_bytes: key_bytes.to_vec(),
            iv_bytes: iv_bytes.to_vec(),
            algorithm: algorithm.to_string(),
            key_length,
        })
    }

    // ==================== HELPERS ====================

    /// Create KeyFileManager scoped to the output directory of the source file
    fn create_key_file_manager(&self, key_file_path: &str) -> KeyFileManager {
        let output_dir = Path::new(key_file_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".")
            .to_string();

        KeyFileManager::new(self.source_file_path.clone(), output_dir)
    }

    /// Unwrap encryption metadata or return descriptive error
    fn require_encryption_metadata<'a>(
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

    /// Decode IV bytes from encryption metadata
    fn decode_iv(
        &self,
        enc: &EncryptionMetadata,
        key_file_path: &str,
    ) -> Result<Vec<u8>, String> {
        let iv_b64 = enc.iv.as_ref().ok_or_else(|| {
            let msg = format!(
                "Key file '{}' is missing IV/nonce data",
                key_file_path
            );
            self.report_error(&msg);
            msg
        })?;

        BASE64.decode(iv_b64).map_err(|e| {
            let msg = format!("Failed to decode IV from key file: {}", e);
            self.report_error(&msg);
            msg
        })
    }

    /// Build KDF config HashMap from encryption metadata
    fn build_kdf_config(&self, enc: &EncryptionMetadata) -> HashMap<String, String> {
        let mut config = HashMap::new();

        if let Some(ref alg) = enc.kdf_algorithm {
            config.insert("algorithm".to_string(), alg.clone());
        }
        if let Some(iterations) = enc.kdf_iterations {
            config.insert("iterations".to_string(), iterations.to_string());
        }
        if let Some(memory) = enc.kdf_memory {
            config.insert("memory_kib".to_string(), memory.to_string());
        }
        if let Some(parallelism) = enc.kdf_parallelism {
            config.insert("parallelism".to_string(), parallelism.to_string());
        }

        config
    }

    /// Validate key length is appropriate for the given algorithm
    fn validate_key_length_for_algorithm(
        &self,
        key_length: u32,
        algorithm: &str,
    ) -> Result<(), String> {
        let expected: Option<u32> = match algorithm.to_lowercase().as_str() {
            "aes128" | "aes-128-gcm" => Some(16),
            "aes256" | "aes-256-gcm" => Some(32),
            "chacha20" | "chacha20poly1305" => Some(32),
            "xor" => None, // XOR accepts any length
            _ => None,
        };

        if let Some(expected_len) = expected {
            if key_length != expected_len {
                let msg = format!(
                    "Key length {} bytes does not match algorithm '{}' (expected {} bytes)",
                    key_length, algorithm, expected_len
                );
                self.report_error(&msg);
                return Err(msg);
            }
        }

        Ok(())
    }

    /// Report error to ErrorManager
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

    /// Log debug message via ErrorManager
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
            key_bytes: vec![0u8; 32], // 32 bytes but algorithm says aes128
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