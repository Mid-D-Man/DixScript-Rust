//! ChaCha20-Poly1305 encryption implementation (HIGH security, modern)
//! Modern authenticated encryption alternative to AES

use super::encryptor_trait::{IEncryptor, EncryptorResult};
use crate::Compiler::DLM::dlm_module_base::DLMModuleBase;
use crate::Compiler::DLM::KeyManagement::Argon2KDF;
use crate::Compiler::AST::SecuritySection;
use crate::ErrorManager::{DlmErrorType, ErrorSeverity};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use rand::RngCore;
use std::collections::HashMap;

/// ChaCha20-Poly1305 encryption implementation
pub struct Chacha20Encryptor {
    base: DLMModuleBase,
    key: Vec<u8>,
    nonce: Vec<u8>,
    security_config: Option<SecuritySection>,
    kdf: Option<Argon2KDF>,
}

impl Chacha20Encryptor {
    /// Create new ChaCha20 encryptor
    pub fn new(security_config: Option<SecuritySection>) -> Self {
        let base = DLMModuleBase::new("DEncryptor.chacha20", 3);

        Chacha20Encryptor {
            base,
            key: Vec::new(),
            nonce: Vec::new(),
            security_config,
            kdf: None,
        }
    }

    /// Generate random key
    fn generate_key(&mut self) {
        self.key = vec![0u8; 32]; // 256 bits
        OsRng.fill_bytes(&mut self.key);

        self.nonce = Self::generate_nonce();

        if self.base.debug_config().is_enabled {
            self.base.log_debug("Generated random ChaCha20 key and nonce");
        }
    }

    /// Generate random nonce
    fn generate_nonce() -> Vec<u8> {
        let mut nonce = vec![0u8; 12]; // ChaCha20 nonce size
        OsRng.fill_bytes(&mut nonce);
        nonce
    }
}

impl IEncryptor for Chacha20Encryptor {
    fn module_name(&self) -> &str {
        self.base.module_name()
    }

    fn algorithm(&self) -> &str {
        "chacha20-poly1305"
    }

    fn initialize(&mut self, config: HashMap<String, String>) {
        // Load key from metadata (decryption scenario)
        if let Some(key_data) = config.get("key_data") {
            self.key = base64::decode(key_data).unwrap_or_else(|_| {
                self.base.log_warning("Failed to decode key data, generating new key");
                let mut key = vec![0u8; 32];
                OsRng.fill_bytes(&mut key);
                key
            });

            if let Some(nonce_data) = config.get("nonce") {
                self.nonce = base64::decode(nonce_data).unwrap_or_else(|_| Self::generate_nonce());
            }

            if self.base.debug_config().is_enabled {
                self.base.log_debug("Loaded ChaCha20 key from metadata");
            }
        } else {
            // Generate new key (encryption scenario)
            self.generate_key();
            self.base.log_info("Generated new ChaCha20 encryption key (keyfile mode)");
        }

        if self.base.debug_config().is_enabled {
            self.base.log_debug("Initialized ChaCha20-Poly1305 encryptor");
        }
    }

    fn set_password(&mut self, password: &str) -> EncryptorResult<()> {
        let security_config = self.security_config.as_ref().ok_or_else(|| {
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvalidFunctionSignature,
                "Security configuration not available".to_string(),
                Some(self.module_name().to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            "Security configuration not available".to_string()
        })?;

        self.base.log_info("Deriving ChaCha20 key from password using Argon2id...");

        let kdf = Argon2KDF::new(security_config);
        self.key = kdf.derive_key(password, 32)?; // 256 bits = 32 bytes
        self.nonce = Self::generate_nonce();
        self.kdf = Some(kdf);

        self.base.log_info("ChaCha20 key derived successfully");

        Ok(())
    }

    fn encrypt(&self, data: &[u8]) -> EncryptorResult<Vec<u8>> {
        if data.is_empty() {
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvalidFunctionSignature,
                "Cannot encrypt null or empty data".to_string(),
                Some(self.module_name().to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            return Err("Cannot encrypt null or empty data".to_string());
        }

        if self.key.is_empty() {
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvalidFunctionSignature,
                "Encryption key not set".to_string(),
                Some(self.module_name().to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            return Err("Encryption key not set".to_string());
        }

        if self.base.debug_config().is_enabled {
            self.base.log_info(&format!("Encrypting {} bytes with ChaCha20-Poly1305...", data.len()));
        }

        let key = Key::from_slice(&self.key);
        let cipher = ChaCha20Poly1305::new(key);
        let nonce = Nonce::from_slice(&self.nonce);

        let ciphertext = cipher.encrypt(nonce, data)
            .map_err(|e| {
                let error_msg = format!("ChaCha20-Poly1305 encryption failed: {}", e);
                self.base.error_manager().add_dlm_error(
                    DlmErrorType::InvocationFailed,
                    error_msg.clone(),
                    Some(self.module_name().to_string()),
                    None,
                    Some("Check key validity and available memory".to_string()),
                    ErrorSeverity::Error,
                );
                error_msg
            })?;

        // Format: [Nonce][Ciphertext+Tag]
        let mut encrypted = Vec::with_capacity(self.nonce.len() + ciphertext.len());
        encrypted.extend_from_slice(&self.nonce);
        encrypted.extend_from_slice(&ciphertext);

        if self.base.debug_config().is_enabled {
            self.base.log_info(&format!(
                "✅ ChaCha20-Poly1305 encryption complete: {} → {} bytes",
                data.len(),
                encrypted.len()
            ));
        }

        Ok(encrypted)
    }

    fn decrypt(&self, encrypted_data: &[u8]) -> EncryptorResult<Vec<u8>> {
        if encrypted_data.is_empty() {
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvalidFunctionSignature,
                "Cannot decrypt null or empty data".to_string(),
                Some(self.module_name().to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            return Err("Cannot decrypt null or empty data".to_string());
        }

        if self.key.is_empty() {
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvalidFunctionSignature,
                "Decryption key not set".to_string(),
                Some(self.module_name().to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            return Err("Decryption key not set".to_string());
        }

        if encrypted_data.len() < 12 {
            return Err("Encrypted data too short".to_string());
        }

        if self.base.debug_config().is_enabled {
            self.base.log_info(&format!("Decrypting {} bytes with ChaCha20-Poly1305...", encrypted_data.len()));
        }

        // Extract nonce and ciphertext
        let extracted_nonce = &encrypted_data[..12];
        let ciphertext = &encrypted_data[12..];

        let key = Key::from_slice(&self.key);
        let cipher = ChaCha20Poly1305::new(key);
        let nonce = Nonce::from_slice(extracted_nonce);

        let plaintext = cipher.decrypt(nonce, ciphertext)
            .map_err(|e| {
                let error_msg = "Decryption failed - invalid password or corrupted data".to_string();
                self.base.error_manager().add_dlm_error(
                    DlmErrorType::InvocationFailed,
                    error_msg.clone(),
                    Some(self.module_name().to_string()),
                    None,
                    Some("Verify password and data integrity".to_string()),
                    ErrorSeverity::Error,
                );
                error_msg
            })?;

        if self.base.debug_config().is_enabled {
            self.base.log_info(&format!(
                "✅ ChaCha20-Poly1305 decryption complete: {} → {} bytes",
                encrypted_data.len(),
                plaintext.len()
            ));
        }

        Ok(plaintext)
    }

    fn validate(&self) -> Result<(), String> {
        if self.security_config.is_none() {
            return Err("ChaCha20 encryptor requires @SECURITY section".to_string());
        }
        Ok(())
    }

    fn get_metadata(&self) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("algorithm".to_string(), "chacha20-poly1305".to_string());
        metadata.insert("key_length".to_string(), "32".to_string());
        metadata.insert("nonce".to_string(), base64::encode(&self.nonce));
        metadata.insert("module_name".to_string(), self.module_name().to_string());
        metadata.insert("priority".to_string(), self.priority().to_string());
        metadata.insert("security_level".to_string(), "HIGH".to_string());

        // Include key if keyfile mode (NOT password mode)
        if self.kdf.is_none() && !self.key.is_empty() {
            metadata.insert("key_data".to_string(), base64::encode(&self.key));
        }

        // Include KDF parameters if password mode
        if let Some(ref kdf) = self.kdf {
            let kdf_metadata = kdf.get_metadata();
            metadata.extend(kdf_metadata);
        }

        metadata
    }

    fn priority(&self) -> i32 {
        self.base.priority()
    }
}
