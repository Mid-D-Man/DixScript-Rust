//! Argon2id Key Derivation Function
//! Memory-hard KDF for password-based encryption

use crate::Compiler::AST::SecuritySection;
use crate::ErrorManager::{ErrorManager, DlmErrorType};
use std::collections::HashMap;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
    Algorithm, Version, Params,
};

/// Argon2id Key Derivation Function
/// Memory-hard KDF resistant to GPU/ASIC attacks
pub struct Argon2KDF {
    error_manager: ErrorManager,
    salt: Vec<u8>,

    // Argon2id parameters
    memory_size_kb: u32,
    iterations: u32,
    parallelism: u32,
}

impl Argon2KDF {
    /// Create new Argon2KDF with configuration from SecuritySection (forward pipeline).
    pub fn new(security_config: &SecuritySection) -> Self {
        let error_manager = ErrorManager::get_shared_instance();

        let (memory_size_kb, iterations, parallelism) = Self::load_configuration(security_config);

        let salt = Self::generate_salt();

        let debug_mode = error_manager.get_debug_mode();
        if debug_mode != crate::Compiler::Core::Config::DebugMode::Off {
            error_manager.log_debug(&format!(
                "[Argon2KDF] Initialized: memory={}KB, iterations={}, parallelism={}",
                memory_size_kb, iterations, parallelism
            ));
        }

        Argon2KDF {
            error_manager,
            salt,
            memory_size_kb,
            iterations,
            parallelism,
        }
    }

    /// Create Argon2KDF directly from params and an existing salt (reverse pipeline).
    ///
    /// Used by encryptors during decryption when no `SecuritySection` is available
    /// but the KDF parameters are already known from the `.mdix.key` file.
    pub fn from_params_with_salt(
        memory_size_kb: u32,
        iterations: u32,
        parallelism: u32,
        salt: Vec<u8>,
    ) -> Result<Self, String> {
        if salt.len() != 32 {
            return Err(format!(
                "Invalid salt length: expected 32 bytes, got {}",
                salt.len()
            ));
        }

        let error_manager = ErrorManager::get_shared_instance();

        let debug_mode = error_manager.get_debug_mode();
        if debug_mode != crate::Compiler::Core::Config::DebugMode::Off {
            error_manager.log_debug(&format!(
                "[Argon2KDF] from_params_with_salt: memory={}KB, iterations={}, parallelism={}",
                memory_size_kb, iterations, parallelism
            ));
        }

        Ok(Argon2KDF {
            error_manager,
            salt,
            memory_size_kb,
            iterations,
            parallelism,
        })
    }

    /// Load Argon2 configuration from SecuritySection
    fn load_configuration(security_config: &SecuritySection) -> (u32, u32, u32) {
        let encryption_block = security_config.entries.iter()
            .find(|e| e.block_key.eq_ignore_ascii_case("encryption"));

        if encryption_block.is_none() {
            return (65536, 3, 4); // 64 MB, 3 iterations, 4 threads
        }

        let block = encryption_block.unwrap();

        let memory = Self::get_int_field(&block.fields, "kdf_memory", 65536);
        let iterations = Self::get_int_field(&block.fields, "kdf_iterations", 3);
        let parallelism = Self::get_int_field(&block.fields, "kdf_parallelism", 4);

        (memory as u32, iterations as u32, parallelism as u32)
    }

    #[inline]
    fn get_int_field(fields: &[crate::Compiler::AST::SecurityField], field_name: &str, default: i32) -> i32 {
        fields.iter()
            .find(|f| f.key.eq_ignore_ascii_case(field_name))
            .and_then(|field| {
                if let crate::Compiler::AST::Value::Integer { value, .. } = &field.value {
                    Some(*value)
                } else {
                    None
                }
            })
            .unwrap_or(default)
    }

    #[inline]
    fn generate_salt() -> Vec<u8> {
        use rand::RngCore;
        let mut salt = vec![0u8; 32]; // 256-bit salt
        rand::thread_rng().fill_bytes(&mut salt);
        salt
    }

    /// Load existing salt (for decryption with SecuritySection available).
    pub fn load_salt(&mut self, existing_salt: Vec<u8>) -> Result<(), String> {
        if existing_salt.len() != 32 {
            return Err("Invalid salt - must be 32 bytes".to_string());
        }

        self.salt = existing_salt;

        let debug_mode = self.error_manager.get_debug_mode();
        if debug_mode != crate::Compiler::Core::Config::DebugMode::Off {
            self.error_manager.log_debug("[Argon2KDF] Loaded existing salt");
        }

        Ok(())
    }

    /// Derive encryption key from password
    pub fn derive_key(&self, password: &str, key_length: usize) -> Result<Vec<u8>, String> {
        if password.is_empty() {
            self.error_manager.add_dlm_error(
                DlmErrorType::KeyGenerationFailed,
                "Password cannot be empty".to_string(),
                Some("Argon2KDF".to_string()),
                None,
                None,
                crate::ErrorManager::ErrorSeverity::Error,
            );
            return Err("Password cannot be empty".to_string());
        }

        if key_length != 16 && key_length != 32 {
            self.error_manager.add_dlm_error(
                DlmErrorType::KeyGenerationFailed,
                "Key length must be 16 (AES-128) or 32 (AES-256/ChaCha20)".to_string(),
                Some("Argon2KDF".to_string()),
                None,
                None,
                crate::ErrorManager::ErrorSeverity::Error,
            );
            return Err("Key length must be 16 or 32 bytes".to_string());
        }

        self.error_manager.log_info(&format!(
            "[Argon2KDF] Deriving {}-bit key from password using Argon2id...",
            key_length * 8
        ));

        let debug_mode = self.error_manager.get_debug_mode();
        if debug_mode != crate::Compiler::Core::Config::DebugMode::Off {
            self.error_manager.log_info("[Argon2KDF] ⏱️ This may take 1-2 seconds (memory-hard KDF for security)");
        }

        let start = std::time::Instant::now();

        let params = match Params::new(
            self.memory_size_kb,
            self.iterations,
            self.parallelism,
            Some(key_length),
        ) {
            Ok(p) => p,
            Err(e) => {
                self.error_manager.add_dlm_error(
                    DlmErrorType::KeyGenerationFailed,
                    format!("Invalid Argon2 parameters: {}", e),
                    Some("Argon2KDF".to_string()),
                    None,
                    None,
                    crate::ErrorManager::ErrorSeverity::Error,
                );
                return Err(format!("Invalid Argon2 parameters: {}", e));
            }
        };

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut key = vec![0u8; key_length];

        if let Err(e) = argon2.hash_password_into(password.as_bytes(), &self.salt, &mut key) {
            self.error_manager.add_dlm_error(
                DlmErrorType::KeyGenerationFailed,
                format!("Key derivation failed: {}", e),
                Some("Argon2KDF".to_string()),
                None,
                Some("Check password and system resources".to_string()),
                crate::ErrorManager::ErrorSeverity::Error,
            );
            return Err(format!("Key derivation failed: {}", e));
        }

        let elapsed = start.elapsed();
        self.error_manager.log_info(&format!(
            "[Argon2KDF] ✅ Key derivation complete in {:.0}ms",
            elapsed.as_millis()
        ));

        Ok(key)
    }

    /// Get KDF metadata for .mdix.key file
    pub fn get_metadata(&self) -> HashMap<String, String> {
        use base64::{Engine as _, engine::general_purpose};

        let mut metadata = HashMap::new();
        metadata.insert("kdf_algorithm".to_string(), "argon2id".to_string());
        metadata.insert("kdf_version".to_string(), "1.3".to_string());
        metadata.insert("kdf_memory".to_string(), self.memory_size_kb.to_string());
        metadata.insert("kdf_iterations".to_string(), self.iterations.to_string());
        metadata.insert("kdf_parallelism".to_string(), self.parallelism.to_string());
        metadata.insert("salt".to_string(), general_purpose::STANDARD.encode(&self.salt));
        metadata.insert("salt_length".to_string(), self.salt.len().to_string());
        metadata
    }

    /// Get salt reference
    pub fn salt(&self) -> &[u8] {
        &self.salt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler::AST::{SecuritySection, SecurityEntry, SecurityField, Value, Position};

    #[test]
    fn test_argon2_key_derivation() {
        let security = SecuritySection {
            entries: vec![
                SecurityEntry::new(
                    "encryption".to_string(),
                    vec![
                        SecurityField::new(
                            "kdf_memory".to_string(),
                            Value::Integer { value: 65536, position: Position::UNKNOWN },
                            Position::UNKNOWN,
                        ),
                        SecurityField::new(
                            "kdf_iterations".to_string(),
                            Value::Integer { value: 3, position: Position::UNKNOWN },
                            Position::UNKNOWN,
                        ),
                        SecurityField::new(
                            "kdf_parallelism".to_string(),
                            Value::Integer { value: 4, position: Position::UNKNOWN },
                            Position::UNKNOWN,
                        ),
                    ],
                    Position::UNKNOWN,
                )
            ],
            position: Position::UNKNOWN,
        };

        let kdf = Argon2KDF::new(&security);
        let key = kdf.derive_key("test_password", 32).unwrap();

        assert_eq!(key.len(), 32);

        let key2 = kdf.derive_key("test_password", 32).unwrap();
        assert_eq!(key, key2);
    }

    #[test]
    fn test_salt_loading() {
        let security = SecuritySection {
            entries: vec![],
            position: Position::UNKNOWN,
        };

        let mut kdf = Argon2KDF::new(&security);
        let original_salt = kdf.salt().to_vec();

        let new_salt = vec![0u8; 32];
        kdf.load_salt(new_salt.clone()).unwrap();

        assert_eq!(kdf.salt(), &new_salt[..]);
        assert_ne!(kdf.salt(), &original_salt[..]);
    }

    #[test]
    fn test_from_params_with_salt_roundtrip() {
        use rand::RngCore;
        let mut salt = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut salt);

        let kdf = Argon2KDF::from_params_with_salt(65536, 3, 4, salt.clone()).unwrap();
        let key = kdf.derive_key("roundtrip_password", 32).unwrap();
        assert_eq!(key.len(), 32);

        // Same params + salt + password must reproduce the same key
        let kdf2 = Argon2KDF::from_params_with_salt(65536, 3, 4, salt).unwrap();
        let key2 = kdf2.derive_key("roundtrip_password", 32).unwrap();
        assert_eq!(key, key2);
    }

    #[test]
    fn test_from_params_with_salt_rejects_bad_length() {
        let bad_salt = vec![0u8; 16]; // wrong length
        assert!(Argon2KDF::from_params_with_salt(65536, 3, 4, bad_salt).is_err());
    }
            }
