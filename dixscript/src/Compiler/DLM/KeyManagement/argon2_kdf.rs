//! Argon2id Key Derivation Function
//! Memory-hard KDF for password-based encryption

use crate::Compiler::AST::SecuritySection;
use crate::ErrorManager::{ErrorManager, DlmErrorType};
use std::collections::HashMap;
use argon2::{Argon2, Algorithm, Version, Params};

/// Argon2id Key Derivation Function
/// Memory-hard KDF resistant to GPU/ASIC attacks
pub struct Argon2KDF {
    error_manager:  ErrorManager,
    salt:           Vec<u8>,
    memory_size_kb: u32,
    iterations:     u32,
    parallelism:    u32,
}

impl Argon2KDF {
    /// Create new Argon2KDF with configuration from SecuritySection.
    /// Used in the forward (encryption) pipeline.
    pub fn new(security_config: &SecuritySection) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let (memory_size_kb, iterations, parallelism) =
            Self::load_configuration(security_config);
        let salt = Self::generate_salt();

        if error_manager.get_debug_mode() != crate::Compiler::Core::Config::DebugMode::Off {
            error_manager.log_debug(&format!(
                "[Argon2KDF] Initialized: memory={}KB, iterations={}, parallelism={}",
                memory_size_kb, iterations, parallelism
            ));
        }

        Argon2KDF { error_manager, salt, memory_size_kb, iterations, parallelism }
    }

    /// Create Argon2KDF directly from stored params and an existing salt.
    ///
    /// Used in the reverse (decryption) pipeline when no `SecuritySection` is
    /// available. The params and salt come from the `.mdix.key` file written
    /// during encryption, guaranteeing the derived key matches the original.
    pub fn from_params_with_salt(
        memory_size_kb: u32,
        iterations:     u32,
        parallelism:    u32,
        salt:           Vec<u8>,
    ) -> Result<Self, String> {
        if salt.len() != 32 {
            return Err(format!(
                "Invalid salt length: expected 32 bytes, got {}",
                salt.len()
            ));
        }

        let error_manager = ErrorManager::get_shared_instance();

        if error_manager.get_debug_mode() != crate::Compiler::Core::Config::DebugMode::Off {
            error_manager.log_debug(&format!(
                "[Argon2KDF] from_params_with_salt: memory={}KB, iterations={}, parallelism={}",
                memory_size_kb, iterations, parallelism
            ));
        }

        Ok(Argon2KDF { error_manager, salt, memory_size_kb, iterations, parallelism })
    }

    // ── Private ───────────────────────────────────────────────────────────────

    fn load_configuration(security_config: &SecuritySection) -> (u32, u32, u32) {
        let encryption_block = security_config.entries.iter()
            .find(|e| e.block_key.eq_ignore_ascii_case("encryption"));

        let Some(block) = encryption_block else {
            return (65536, 3, 4);
        };

        let memory      = Self::get_int_field(&block.fields, "kdf_memory",      65536);
        let iterations  = Self::get_int_field(&block.fields, "kdf_iterations",  3);
        let parallelism = Self::get_int_field(&block.fields, "kdf_parallelism", 4);

        (memory as u32, iterations as u32, parallelism as u32)
    }

    #[inline]
    fn get_int_field(
        fields:     &[crate::Compiler::AST::SecurityField],
        field_name: &str,
        default:    i32,
    ) -> i32 {
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
        let mut salt = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut salt);
        salt
    }

    // ── Public ────────────────────────────────────────────────────────────────

    /// Load existing salt (for decryption when a `SecuritySection` is available
    /// but the salt needs to be overridden from the key file).
    pub fn load_salt(&mut self, existing_salt: Vec<u8>) -> Result<(), String> {
        if existing_salt.len() != 32 {
            return Err("Invalid salt — must be 32 bytes".to_string());
        }
        self.salt = existing_salt;
        if self.error_manager.get_debug_mode() != crate::Compiler::Core::Config::DebugMode::Off {
            self.error_manager.log_debug("[Argon2KDF] Loaded existing salt");
        }
        Ok(())
    }

    /// Derive an encryption key of `key_length` bytes from `password`.
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

        if self.error_manager.get_debug_mode() != crate::Compiler::Core::Config::DebugMode::Off {
            self.error_manager.log_info(
                "[Argon2KDF] ⏱️ This may take 1-2 seconds (memory-hard KDF for security)",
            );
        }

        let start = std::time::Instant::now();

        let params = Params::new(
            self.memory_size_kb,
            self.iterations,
            self.parallelism,
            Some(key_length),
        ).map_err(|e| {
            let msg = format!("Invalid Argon2 parameters: {}", e);
            self.error_manager.add_dlm_error(
                DlmErrorType::KeyGenerationFailed,
                msg.clone(),
                Some("Argon2KDF".to_string()),
                None,
                None,
                crate::ErrorManager::ErrorSeverity::Error,
            );
            msg
        })?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut key = vec![0u8; key_length];

        argon2
            .hash_password_into(password.as_bytes(), &self.salt, &mut key)
            .map_err(|e| {
                let msg = format!("Key derivation failed: {}", e);
                self.error_manager.add_dlm_error(
                    DlmErrorType::KeyGenerationFailed,
                    msg.clone(),
                    Some("Argon2KDF".to_string()),
                    None,
                    Some("Check password and system resources".to_string()),
                    crate::ErrorManager::ErrorSeverity::Error,
                );
                msg
            })?;

        self.error_manager.log_info(&format!(
            "[Argon2KDF] ✅ Key derivation complete in {:.0}ms",
            start.elapsed().as_millis()
        ));

        Ok(key)
    }

    /// Return all KDF parameters as a metadata map for writing to `.mdix.key`.
    /// The reverse pipeline reads these back via `from_params_with_salt`.
    pub fn get_metadata(&self) -> HashMap<String, String> {
        use base64::{Engine as _, engine::general_purpose};

        let mut m = HashMap::new();
        m.insert("kdf_algorithm".to_string(),   "argon2id".to_string());
        m.insert("kdf_version".to_string(),     "1.3".to_string());
        m.insert("kdf_memory".to_string(),      self.memory_size_kb.to_string());
        m.insert("kdf_iterations".to_string(),  self.iterations.to_string());
        m.insert("kdf_parallelism".to_string(), self.parallelism.to_string());
        m.insert("salt".to_string(),            general_purpose::STANDARD.encode(&self.salt));
        m.insert("salt_length".to_string(),     self.salt.len().to_string());
        m
    }

    pub fn salt(&self) -> &[u8] {
        &self.salt
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler::AST::{SecuritySection, SecurityEntry, SecurityField, Value, Position};

    fn make_security(memory: i32, iterations: i32, parallelism: i32) -> SecuritySection {
        SecuritySection {
            entries: vec![SecurityEntry::new(
                "encryption".to_string(),
                vec![
                    SecurityField::new(
                        "kdf_memory".to_string(),
                        Value::Integer { value: memory, position: Position::UNKNOWN },
                        Position::UNKNOWN,
                    ),
                    SecurityField::new(
                        "kdf_iterations".to_string(),
                        Value::Integer { value: iterations, position: Position::UNKNOWN },
                        Position::UNKNOWN,
                    ),
                    SecurityField::new(
                        "kdf_parallelism".to_string(),
                        Value::Integer { value: parallelism, position: Position::UNKNOWN },
                        Position::UNKNOWN,
                    ),
                ],
                Position::UNKNOWN,
            )],
            position: Position::UNKNOWN,
        }
    }

    #[test]
    fn test_argon2_key_derivation() {
        let kdf  = Argon2KDF::new(&make_security(65536, 3, 4));
        let key  = kdf.derive_key("test_password", 32).unwrap();
        assert_eq!(key.len(), 32);
        let key2 = kdf.derive_key("test_password", 32).unwrap();
        assert_eq!(key, key2);
    }

    #[test]
    fn test_salt_loading() {
        let mut kdf  = Argon2KDF::new(&make_security(65536, 3, 4));
        let original = kdf.salt().to_vec();
        let new_salt = vec![42u8; 32];
        kdf.load_salt(new_salt.clone()).unwrap();
        assert_eq!(kdf.salt(), &new_salt[..]);
        assert_ne!(kdf.salt(), &original[..]);
    }

    #[test]
    fn test_load_salt_rejects_wrong_length() {
        let mut kdf = Argon2KDF::new(&make_security(65536, 3, 4));
        assert!(kdf.load_salt(vec![0u8; 16]).is_err());
    }

    #[test]
    fn test_from_params_with_salt_roundtrip() {
        use rand::RngCore;
        let mut salt = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut salt);

        let kdf1 = Argon2KDF::from_params_with_salt(65536, 3, 4, salt.clone()).unwrap();
        let key1 = kdf1.derive_key("roundtrip_password", 32).unwrap();

        let kdf2 = Argon2KDF::from_params_with_salt(65536, 3, 4, salt).unwrap();
        let key2 = kdf2.derive_key("roundtrip_password", 32).unwrap();

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_from_params_with_salt_rejects_bad_length() {
        assert!(Argon2KDF::from_params_with_salt(65536, 3, 4, vec![0u8; 16]).is_err());
    }

    #[test]
    fn test_forward_reverse_key_match() {
        let fwd_kdf = Argon2KDF::new(&make_security(65536, 3, 4));
        let fwd_key = fwd_kdf.derive_key("shared_password", 32).unwrap();
        let salt    = fwd_kdf.salt().to_vec();

        let rev_kdf = Argon2KDF::from_params_with_salt(65536, 3, 4, salt).unwrap();
        let rev_key = rev_kdf.derive_key("shared_password", 32).unwrap();

        assert_eq!(fwd_key, rev_key, "forward and reverse keys must match");
    }

    #[test]
    fn test_metadata_round_trip() {
        use base64::{Engine as _, engine::general_purpose};

        let kdf      = Argon2KDF::new(&make_security(65536, 3, 4));
        let metadata = kdf.get_metadata();

        assert_eq!(metadata["kdf_algorithm"],   "argon2id");
        assert_eq!(metadata["kdf_memory"],      "65536");
        assert_eq!(metadata["kdf_iterations"],  "3");
        assert_eq!(metadata["kdf_parallelism"], "4");
        assert_eq!(metadata["salt_length"],     "32");

        let decoded = general_purpose::STANDARD
            .decode(&metadata["salt"])
            .unwrap();
        assert_eq!(decoded, kdf.salt());
    }
            }
