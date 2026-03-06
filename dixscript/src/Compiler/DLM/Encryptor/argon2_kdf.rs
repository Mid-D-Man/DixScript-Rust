//! Argon2id Key Derivation Function
//! Memory-hard KDF for password-based encryption

use crate::Compiler::AST::{SecuritySection, SecurityEntry};
use crate::ErrorManager::{ErrorManager, DlmErrorType, ErrorSeverity};
use argon2::{Argon2, ParamsBuilder, Algorithm, Version};
use argon2::password_hash::{PasswordHasher, SaltString, Salt};
use rand::rngs::OsRng;
use std::collections::HashMap;

/// Argon2id KDF for deriving encryption keys from passwords
pub struct Argon2KDF {
    error_manager: ErrorManager,
    salt: Vec<u8>,
    memory_size: u32,      // KB
    iterations: u32,
    parallelism: u32,
}

impl Argon2KDF {
    /// Create new Argon2 KDF from security configuration
    pub fn new(security_config: &SecuritySection) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        
        // Extract KDF parameters from SECURITY section
        let (memory_size, iterations, parallelism) = Self::extract_kdf_params(security_config);
        
        // Generate random salt
        let salt = Self::generate_salt();
        
        if error_manager.get_debug_mode() != crate::Compiler::Core::Config::DebugMode::Off {
            error_manager.log_debug(&format!(
                "Argon2id config: memory={}KB, iterations={}, parallelism={}",
                memory_size, iterations, parallelism
            ));
        }

        Argon2KDF {
            error_manager,
            salt,
            memory_size,
            iterations,
            parallelism,
        }
    }

    /// Extract KDF parameters from security section
    fn extract_kdf_params(security_config: &SecuritySection) -> (u32, u32, u32) {
        // Find encryption block
        let encryption_block = security_config.entries.iter()
            .find(|e| e.block_key.eq_ignore_ascii_case("encryption"));

        if let Some(block) = encryption_block {
            let memory = Self::get_int_field(block, "kdf_memory").unwrap_or(65536);
            let iterations = Self::get_int_field(block, "kdf_iterations").unwrap_or(3);
            let parallelism = Self::get_int_field(block, "kdf_parallelism").unwrap_or(4);
            (memory, iterations, parallelism)
        } else {
            // Use defaults
            (65536, 3, 4)
        }
    }

    /// Get integer field from security entry
    fn get_int_field(entry: &SecurityEntry, field_name: &str) -> Option<u32> {
        entry.fields.iter()
            .find(|f| f.key.eq_ignore_ascii_case(field_name))
            .and_then(|f| {
                use crate::Compiler::AST::Value;
                match &f.value {
                    Value::Integer(i) => Some(*i as u32),
                    _ => None,
                }
            })
    }

    /// Generate random 32-byte salt
    fn generate_salt() -> Vec<u8> {
        use rand::RngCore;
        let mut salt = vec![0u8; 32];
        OsRng.fill_bytes(&mut salt);
        salt
    }

    /// Load salt from metadata (decryption scenario)
    pub fn load_salt(&mut self, existing_salt: Vec<u8>) -> Result<(), String> {
        if existing_salt.len() != 32 {
            self.error_manager.add_dlm_error(
                DlmErrorType::InvalidFunctionSignature,
                "Invalid salt - must be 32 bytes".to_string(),
                Some("Argon2KDF".to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            return Err("Invalid salt - must be 32 bytes".to_string());
        }

        self.salt = existing_salt;
        
        if self.error_manager.get_debug_mode() != crate::Compiler::Core::Config::DebugMode::Off {
            self.error_manager.log_debug("Loaded existing salt for Argon2id");
        }

        Ok(())
    }

    /// Derive encryption key from password
    pub fn derive_key(&self, password: &str, key_length: usize) -> Result<Vec<u8>, String> {
        if password.is_empty() {
            self.error_manager.add_dlm_error(
                DlmErrorType::InvalidFunctionSignature,
                "Password cannot be empty".to_string(),
                Some("Argon2KDF".to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            return Err("Password cannot be empty".to_string());
        }

        if key_length != 16 && key_length != 32 {
            self.error_manager.add_dlm_error(
                DlmErrorType::InvalidFunctionSignature,
                "Key length must be 16 (AES-128) or 32 (AES-256/ChaCha20)".to_string(),
                Some("Argon2KDF".to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            return Err("Key length must be 16 (AES-128) or 32 (AES-256/ChaCha20)".to_string());
        }

        self.error_manager.log_info(&format!(
            "Deriving {}-bit key from password using Argon2id...",
            key_length * 8
        ));
        self.error_manager.log_info("⏱️ This may take 1-2 seconds (memory-hard KDF for security)");

        let start = std::time::Instant::now();

        // Build Argon2 params
        let params = ParamsBuilder::new()
            .m_cost(self.memory_size)
            .t_cost(self.iterations)
            .p_cost(self.parallelism)
            .output_len(key_length)
            .build()
            .map_err(|e| format!("Failed to build Argon2 params: {}", e))?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        // Derive key
        let mut output_key = vec![0u8; key_length];
        argon2.hash_password_into(password.as_bytes(), &self.salt, &mut output_key)
            .map_err(|e| {
                let error_msg = format!("Key derivation failed: {}", e);
                self.error_manager.add_dlm_error(
                    DlmErrorType::InvocationFailed,
                    error_msg.clone(),
                    Some("Argon2KDF".to_string()),
                    None,
                    Some("Check password and system resources".to_string()),
                    ErrorSeverity::Error,
                );
                error_msg
            })?;

        let elapsed = start.elapsed();
        self.error_manager.log_info(&format!(
            "✅ Key derivation complete in {:.0}ms",
            elapsed.as_secs_f64() * 1000.0
        ));

        Ok(output_key)
    }

    /// Get KDF metadata for .dixscript.key file
    pub fn get_metadata(&self) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("kdf_algorithm".to_string(), "argon2id".to_string());
        metadata.insert("kdf_memory".to_string(), self.memory_size.to_string());
        metadata.insert("kdf_iterations".to_string(), self.iterations.to_string());
        metadata.insert("kdf_parallelism".to_string(), self.parallelism.to_string());
        metadata.insert("salt".to_string(), base64::encode(&self.salt));
        metadata.insert("salt_length".to_string(), self.salt.len().to_string());
        metadata
    }

    /// Get salt
    pub fn salt(&self) -> &[u8] {
        &self.salt
    }
                      }
