//! Argon2id Key Derivation Function
//! Memory-hard KDF for password-based encryption

use crate::Compiler::AST::SecuritySection;
use crate::Compiler::DLM::dlm_module_base::DebugConfig;
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
    debug_config: DebugConfig,
    salt: Vec<u8>,
    
    // Argon2id parameters
    memory_size_kb: u32,
    iterations: u32,
    parallelism: u32,
}

impl Argon2KDF {
    /// Create new Argon2KDF with configuration from SecuritySection
    pub fn new(security_config: &SecuritySection, debug_mode: crate::Compiler::Core::Config::DebugMode) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let debug_config = DebugConfig::from_debug_mode(debug_mode);
        
        // Load parameters from security config
        let (memory_size_kb, iterations, parallelism) = Self::load_configuration(security_config);
        
        // Generate random salt
        let salt = Self::generate_salt();
        
        if debug_config.is_enabled {
            error_manager.log_debug(&format!(
                "[Argon2KDF] Initialized: memory={}KB, iterations={}, parallelism={}",
                memory_size_kb, iterations, parallelism
            ));
        }
        
        Argon2KDF {
            error_manager,
            debug_config,
            salt,
            memory_size_kb,
            iterations,
            parallelism,
        }
    }
    
    /// Load Argon2 configuration from SecuritySection
    fn load_configuration(security_config: &SecuritySection) -> (u32, u32, u32) {
        // Find encryption block
        let encryption_block = security_config.entries.iter()
            .find(|e| e.block_key.eq_ignore_ascii_case("encryption"));
        
        if encryption_block.is_none() {
            // Use defaults
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
    
    /// Load existing salt (for decryption)
    pub fn load_salt(&mut self, existing_salt: Vec<u8>) -> Result<(), String> {
        if existing_salt.len() != 32 {
            return Err("Invalid salt - must be 32 bytes".to_string());
        }
        
        self.salt = existing_salt;
        
        if self.debug_config.is_enabled {
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

            );
            return Err("Key length must be 16 or 32 bytes".to_string());
        }
        
        self.error_manager.log_info(&format!(
            "[Argon2KDF] Deriving {}-bit key from password using Argon2id...",
            key_length * 8
        ));
        
        if self.debug_config.is_enabled {
            self.error_manager.log_info("[Argon2KDF] ⏱️ This may take 1-2 seconds (memory-hard KDF for security)");
        }
        
        let start = std::time::Instant::now();
        
        // Create Argon2 parameters
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

                );
                return Err(format!("Invalid Argon2 parameters: {}", e));
            }
        };
        
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        
        // Derive key
        let mut key = vec![0u8; key_length];
        
        if let Err(e) = argon2.hash_password_into(password.as_bytes(), &self.salt, &mut key) {
            self.error_manager.add_dlm_error(
                DlmErrorType::KeyGenerationFailed,
                format!("Key derivation failed: {}", e),
                Some("Argon2KDF".to_string()),
                None,
                Some("Check password and system resources".to_string()),

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
        let mut metadata = HashMap::new();
        metadata.insert("kdf_algorithm".to_string(), "argon2id".to_string());
        metadata.insert("kdf_version".to_string(), "1.3".to_string());
        metadata.insert("kdf_memory".to_string(), self.memory_size_kb.to_string());
        metadata.insert("kdf_iterations".to_string(), self.iterations.to_string());
        metadata.insert("kdf_parallelism".to_string(), self.parallelism.to_string());
        metadata.insert("salt".to_string(), base64::encode(&self.salt));
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
        // Create minimal security config
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
        
        let kdf = Argon2KDF::new(&security, crate::Compiler::Core::Config::DebugMode::Off);
        let key = kdf.derive_key("test_password", 32).unwrap();
        
        assert_eq!(key.len(), 32);
        
        // Same password should produce same key with same salt
        let key2 = kdf.derive_key("test_password", 32).unwrap();
        assert_eq!(key, key2);
    }
    
    #[test]
    fn test_salt_loading() {
        let security = SecuritySection {
            entries: vec![],
            position: Position::UNKNOWN,
        };
        
        let mut kdf = Argon2KDF::new(&security, crate::Compiler::Core::Config::DebugMode::Off);
        let original_salt = kdf.salt().to_vec();
        
        // Load a different salt
        let new_salt = vec![0u8; 32];
        kdf.load_salt(new_salt.clone()).unwrap();
        
        assert_eq!(kdf.salt(), &new_salt[..]);
        assert_ne!(kdf.salt(), &original_salt[..]);
    }
  }
