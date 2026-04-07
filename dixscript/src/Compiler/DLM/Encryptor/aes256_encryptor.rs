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

    // Stored during initialize() from key file config — used for password-mode
    // decryption in the reverse pipeline when no SecuritySection is available.
    reverse_kdf_memory:      Option<u32>,
    reverse_kdf_iterations:  Option<u32>,
    reverse_kdf_parallelism: Option<u32>,
    reverse_kdf_salt:        Option<Vec<u8>>,
}

impl Aes256Encryptor {
    pub fn new(security_config: Option<SecuritySection>) -> Self {
        let base = DLMModuleBase::new("DEncryptor.aes256", 3);

        Aes256Encryptor {
            base,
            key: Vec::new(),
            iv: Vec::new(),
            security_config,
            kdf: None,
            reverse_kdf_memory:      None,
            reverse_kdf_iterations:  None,
            reverse_kdf_parallelism: None,
            reverse_kdf_salt:        None,
        }
    }

    fn generate_key(&mut self) {
        self.key = vec![0u8; 32]; // 256 bits
        OsRng.fill_bytes(&mut self.key);
        self.iv = Self::generate_iv();

        if self.base.is_debug_enabled() {
            self.base.log_debug("Generated random AES-256 key and IV");
        }
    }

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

        if let Some(key_data) = config.get("key_data") {
            // Keyfile mode: load raw key material.
            self.key = general_purpose::STANDARD.decode(key_data).unwrap_or_else(|_| {
                self.base.log_warning("Failed to decode key data, generating new key");
                let mut key = vec![0u8; 32];
                OsRng.fill_bytes(&mut key);
                key
            });

            if let Some(iv_data) = config.get("iv") {
                self.iv = general_purpose::STANDARD
                    .decode(iv_data)
                    .unwrap_or_else(|_| Self::generate_iv());
            }

            if self.base.is_debug_enabled() {
                self.base.log_debug("Loaded AES-256 key from metadata (keyfile mode)");
            }
        } else {
            // Password mode — key will be derived in set_password().
            // Load IV now so decryption uses the original nonce from the key file.
            if let Some(iv_data) = config.get("iv") {
                self.iv = general_purpose::STANDARD
                    .decode(iv_data)
                    .unwrap_or_else(|_| Self::generate_iv());
            }

            // Cache KDF parameters so set_password() can derive the key without
            // a SecuritySection (reverse pipeline path).
            self.reverse_kdf_memory = config
                .get("kdf_memory")
                .and_then(|v| v.parse::<u32>().ok());
            self.reverse_kdf_iterations = config
                .get("kdf_iterations")
                .and_then(|v| v.parse::<u32>().ok());
            self.reverse_kdf_parallelism = config
                .get("kdf_parallelism")
                .and_then(|v| v.parse::<u32>().ok());

            if let Some(salt_b64) = config.get("salt") {
                if let Ok(salt) = general_purpose::STANDARD.decode(salt_b64) {
                    self.reverse_kdf_salt = Some(salt);
                }
            }

            if self.base.is_debug_enabled() {
                self.base.log_debug("Initialized AES-256 in password mode (key pending set_password)");
            }

            // Forward pipeline with no existing key: generate a fresh one so the
            // encryptor is in a defined state before set_password() is called.
            if self.key.is_empty() {
                self.generate_key();
            }
        }

        if self.base.is_debug_enabled() {
            self.base.log_debug("Initialized AES-256-GCM encryptor");
        }
    }

    fn set_password(&mut self, password: &str) -> EncryptorResult<()> {
        if let Some(ref security_config) = self.security_config {
            // ── Forward pipeline ────────────────────────────────────────────
            self.base.log_info("Deriving AES-256 key from password using Argon2id...");
            self.base.log_warning("⏱️ Key derivation may take 1-2 seconds (this is intentional for security)");

            let kdf = Argon2KDF::new(security_config);
            self.key = kdf.derive_key(password, 32)?;
            self.iv  = Self::generate_iv(); // fresh IV for new encryption
            self.kdf = Some(kdf);
        } else if let (Some(memory), Some(iterations), Some(parallelism), Some(ref salt)) = (
            self.reverse_kdf_memory,
            self.reverse_kdf_iterations,
            self.reverse_kdf_parallelism,
            self.reverse_kdf_salt.clone(),
        ) {
            // ── Reverse pipeline (decryption from key file) ─────────────────
            self.base.log_info("Deriving AES-256 key from password using stored Argon2id params...");

            let kdf = Argon2KDF::from_params_with_salt(memory, iterations, parallelism, salt.clone())
                .map_err(|e| format!("Failed to build Argon2 KDF for decryption: {}", e))?;

            self.key = kdf.derive_key(password, 32)?;
            // IV was already loaded from the key file in initialize() — do NOT overwrite it.
            self.kdf = Some(kdf);
        } else {
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvalidFunctionSignature,
                "Security configuration not available and no KDF params in key file".to_string(),
                Some(self.module_name().to_string()),
                None,
                Some("Provide a @SECURITY section or a valid .mdix.key file with KDF parameters".to_string()),
                ErrorSeverity::Error,
            );
            return Err(
                "Cannot derive key: no security config and no KDF params from key file".to_string()
            );
        }

        self.base.log_info(" AES-256 key derived successfully");
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

        let key    = Key::<Aes256Gcm>::from_slice(&self.key);
        let cipher = Aes256Gcm::new(key);
        let nonce  = Nonce::from_slice(&self.iv);

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

        // Format: [IV (12 bytes)][Ciphertext+GCM Tag]
        let mut encrypted = Vec::with_capacity(self.iv.len() + ciphertext.len());
        encrypted.extend_from_slice(&self.iv);
        encrypted.extend_from_slice(&ciphertext);

        if self.base.is_debug_enabled() {
            self.base.log_info(&format!(
                " AES-256-GCM encryption complete: {} → {} bytes",
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
            return Err("Encrypted data too short (minimum 12 bytes for IV)".to_string());
        }

        if self.base.is_debug_enabled() {
            self.base.log_info(&format!(
                "Decrypting {} bytes with AES-256-GCM...",
                encrypted_data.len()
            ));
        }

        // The IV is prepended to the ciphertext during encryption.
        let extracted_iv = &encrypted_data[..12];
        let ciphertext   = &encrypted_data[12..];

        let key    = Key::<Aes256Gcm>::from_slice(&self.key);
        let cipher = Aes256Gcm::new(key);
        let nonce  = Nonce::from_slice(extracted_iv);

        let plaintext = cipher.decrypt(nonce, ciphertext)
            .map_err(|_| {
                let error_msg =
                    "Decryption failed — invalid password or corrupted data".to_string();
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
                " AES-256-GCM decryption complete: {} → {} bytes",
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
        metadata.insert("algorithm".to_string(),      "aes256-gcm".to_string());
        metadata.insert("key_length".to_string(),     "32".to_string());
        metadata.insert("iv".to_string(),             general_purpose::STANDARD.encode(&self.iv));
        metadata.insert("module_name".to_string(),    self.module_name().to_string());
        metadata.insert("priority".to_string(),       self.priority().to_string());
        metadata.insert("security_level".to_string(), "HIGH".to_string());

        // Include raw key only in keyfile mode (never in password mode).
        if self.kdf.is_none() && !self.key.is_empty() {
            metadata.insert(
                "key_data".to_string(),
                general_purpose::STANDARD.encode(&self.key),
            );
        }

        // Include KDF parameters so the reverse pipeline can re-derive the key.
        if let Some(ref kdf) = self.kdf {
            metadata.extend(kdf.get_metadata());
        }

        metadata
    }

    fn priority(&self) -> i32 {
        self.base.priority()
    }
}
