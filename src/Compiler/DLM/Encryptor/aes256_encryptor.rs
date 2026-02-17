//! AES-256-GCM encryption implementation (HIGH security, recommended)
//! Production-ready authenticated encryption - RECOMMENDED FOR PRODUCTION

use super::encryptor_trait::{IEncryptor, EncryptorResult};
use crate::Compiler::DLM::dlm_module_base::DLMModuleBase;
use crate::Compiler::DLM::KeyManagement::Argon2KDF;
use crate::Compiler::AST::SecuritySection;
use crate::ErrorManager::{DlmErrorType, ErrorSeverity};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use rand::RngCore;
use std::collections::HashMap;

/// AES-256-GCM encryption implementation
pub struct Aes256Encryptor {
    base: DLMModuleBase,
    key: Vec<u8>,
    iv: Vec<u8>,
    security_config: Option<SecuritySection>,
    kdf: Option<Argon2KDF>,
}

impl Aes256Encryptor {
    /// Create new AES-256 encryptor
    pub fn new(security_config: Option<SecuritySection>) -> Self {
        let base = DLMModuleBase::new("DEncryptor.aes256", 3);

        Aes256Encryptor {
            base,
            key: Vec::new(),
            iv: Vec::new(),
            security_config,
            kdf: None,
        }
    }

    /// Generate random key
    fn generate_key(&mut self) {
        self.key = vec![0u8; 32]; // 256 bits
        OsRng.fill_bytes(&mut self.key);

        self.iv = Self::generate_iv();

        if self.base.is_debug_enabled() {
            self.base.log_debug("Generated random AES-256 key and IV");
        }
    }

    /// Generate random IV
    fn generate_iv() -> Vec<u8> {
        let mut iv = vec![0u8; 12]; // GCM standard IV size
        OsRng.fill_bytes(&mut iv);
        iv
    }
}

impl IEncryptor for Aes256Encryptor {
    fn module_name(&self) -> &str {
        self.base.module_name()
    }

    fn algorithm(&self) -> &str {
        "aes256-gcm"
    }

    fn initialize(&mut self, config: HashMap<String, String>) {
        use base64::{Engine as _, engine::general_purpose};

        // Load key from metadata (decryption scenario)
        if let Some(key_data) = config.get("key_data") {
            self.key = general_purpose::STANDARD.decode(key_data).unwrap_or_else(|_| {
                self.base.log_warning("Failed to decode key data, generating new key");
                let mut key = vec![0u8; 32];
                OsRng.fill_bytes(&mut key);
                key
            });

            if let Some(iv_data) = config.get("iv") {
                self.iv = general_purpose::STANDARD.decode(iv_data).unwrap_or_else(|_| Self::generate_iv());
            }

            if self.base.is_debug_enabled() {
                self.base.log_debug("Loaded AES-256 key from metadata");
            }
        } else {
            // Generate new key (encryption scenario)
            self.generate_key();
            self.base.log_info("Generated new AES-256 encryption key (keyfile mode)");
        }

        if self.base.is_debug_enabled() {
            self.base.log_debug("Initialized AES-256-GCM encryptor");
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

        self.base.log_info("Deriving AES-256 key from password using Argon2id...");
        self.base.log_warning("⏱️ Key derivation may take 1-2 seconds (this is intentional for security)");

        let kdf = Argon2KDF::new(security_config);
        self.key = kdf.derive_key(password, 32)?; // 256 bits = 32 bytes
        self.iv = Self::generate_iv();
        self.kdf = Some(kdf);

        self.base.log_info("✅ AES-256 key derived successfully");

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

        if self.base.is_debug_enabled() {
            self.base.log_info(&format!("Encrypting {} bytes with AES-256-GCM...", data.len()));
        }

        let key = Key::<Aes256Gcm>::from_slice(&self.key);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(&self.iv);

        let ciphertext = cipher.encrypt(nonce, data)
            .map_err(|e| {
                let error_msg = format!("AES-256-GCM encryption failed: {}", e);
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

        // Format: [IV][Ciphertext+Tag]
        let mut encrypted = Vec::with_capacity(self.iv.len() + ciphertext.len());
        encrypted.extend_from_slice(&self.iv);
        encrypted.extend_from_slice(&ciphertext);

        if self.base.is_debug_enabled() {
            self.base.log_info(&format!(
                "✅ AES-256-GCM encryption complete: {} → {} bytes",
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

        if self.base.is_debug_enabled() {
            self.base.log_info(&format!("Decrypting {} bytes with AES-256-GCM...", encrypted_data.len()));
        }

        // Extract IV and ciphertext
        let extracted_iv = &encrypted_data[..12];
        let ciphertext = &encrypted_data[12..];

        let key = Key::<Aes256Gcm>::from_slice(&self.key);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(extracted_iv);

        let plaintext = cipher.decrypt(nonce, ciphertext)
            .map_err(|_e| {
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

        if self.base.is_debug_enabled() {
            self.base.log_info(&format!(
                "✅ AES-256-GCM decryption complete: {} → {} bytes",
                encrypted_data.len(),
                plaintext.len()
            ));
        }

        Ok(plaintext)
    }

    fn validate(&self) -> Result<(), String> {
        if self.security_config.is_none() {
            return Err("AES-256 encryptor requires @SECURITY section".to_string());
        }
        Ok(())
    }

    fn get_metadata(&self) -> HashMap<String, String> {
        use base64::{Engine as _, engine::general_purpose};

        let mut metadata = HashMap::new();
        metadata.insert("algorithm".to_string(), "aes256-gcm".to_string());
        metadata.insert("key_length".to_string(), "32".to_string());
        metadata.insert("iv".to_string(), general_purpose::STANDARD.encode(&self.iv));
        metadata.insert("module_name".to_string(), self.module_name().to_string());
        metadata.insert("priority".to_string(), self.priority().to_string());
        metadata.insert("security_level".to_string(), "HIGH".to_string());

        // Include key if keyfile mode (NOT password mode)
        if self.kdf.is_none() && !self.key.is_empty() {
            metadata.insert("key_data".to_string(), general_purpose::STANDARD.encode(&self.key));
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