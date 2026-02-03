//! XOR cipher implementation (LOW security - obfuscation only)
//! For testing purposes only - DO NOT use in production

use super::encryptor_trait::{IEncryptor, EncryptorResult};
use crate::Compiler::DLM::dlm_module_base::DLMModuleBase;
use crate::Compiler::AST::SecuritySection;
use crate::ErrorManager::{DlmErrorType, ErrorSeverity};
use sha2::{Sha256, Digest};
use rand::RngCore;
use std::collections::HashMap;

/// XOR cipher implementation
pub struct XorEncryptor {
    base: DLMModuleBase,
    key: Vec<u8>,
    security_config: Option<SecuritySection>,
}

impl XorEncryptor {
    /// Create new XOR encryptor
    pub fn new(security_config: Option<SecuritySection>) -> Self {
        let base = DLMModuleBase::new("DEncryptor.xor", 3);

        XorEncryptor {
            base,
            key: Vec::new(),
            security_config,
        }
    }

    /// Generate random key
    fn generate_key(&mut self) {
        self.key = vec![0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut self.key);

        if self.base.debug_config().is_enabled {
            self.base.log_debug("Generated random XOR key");
        }
    }
}

impl IEncryptor for XorEncryptor {
    fn module_name(&self) -> &str {
        self.base.module_name()
    }

    fn algorithm(&self) -> &str {
        "xor"
    }

    fn initialize(&mut self, config: HashMap<String, String>) {
        self.base.log_warning("⚠️ XOR cipher provides LOW security - use only for testing!");

        // Load key from metadata or generate new one
        if let Some(key_data) = config.get("key_data") {
            self.key = base64::decode(key_data).unwrap_or_else(|_| {
                self.base.log_warning("Failed to decode key data, generating new key");
                let mut key = vec![0u8; 32];
                rand::rngs::OsRng.fill_bytes(&mut key);
                key
            });

            if self.base.debug_config().is_enabled {
                self.base.log_debug("Loaded XOR key from metadata");
            }
        } else {
            self.generate_key();
            self.base.log_info("Generated new XOR encryption key (keyfile mode)");
        }

        if self.base.debug_config().is_enabled {
            self.base.log_debug(&format!("Initialized XOR encryptor with {}-byte key", self.key.len()));
        }
    }

    fn set_password(&mut self, password: &str) -> EncryptorResult<()> {
        if self.base.debug_config().is_enabled {
            self.base.log_debug("Setting password for XOR encryption");
        }

        // Use password hash as key
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        self.key = hasher.finalize().to_vec();

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
            self.base.log_info(&format!("Encrypting {} bytes with XOR cipher...", data.len()));
        }

        let mut encrypted = vec![0u8; data.len()];
        for (i, &byte) in data.iter().enumerate() {
            encrypted[i] = byte ^ self.key[i % self.key.len()];
        }

        if self.base.debug_config().is_enabled {
            self.base.log_info(&format!("✅ XOR encryption complete: {} bytes", encrypted.len()));
        }

        Ok(encrypted)
    }

    fn decrypt(&self, encrypted_data: &[u8]) -> EncryptorResult<Vec<u8>> {
        // XOR encryption is symmetric - decrypt = encrypt
        self.encrypt(encrypted_data)
    }

    fn validate(&self) -> Result<(), String> {
        if self.security_config.is_none() {
            return Err("XOR encryptor requires @SECURITY section".to_string());
        }
        Ok(())
    }

    fn get_metadata(&self) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("algorithm".to_string(), "xor".to_string());
        metadata.insert("key_data".to_string(), base64::encode(&self.key));
        metadata.insert("key_length".to_string(), self.key.len().to_string());
        metadata.insert("module_name".to_string(), self.module_name().to_string());
        metadata.insert("priority".to_string(), self.priority().to_string());
        metadata.insert("security_level".to_string(), "LOW".to_string());
        metadata
    }

    fn priority(&self) -> i32 {
        self.base.priority()
    }
          }
