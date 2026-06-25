//! Security section utilities for DixScript
//! Handles auto-generation and validation of @SECURITY sections
//!
//! IMPROVEMENTS over C# version:
//! - Uses Result<T, E> for error handling
//! - Passes Position information for better error reporting
//! - Uses ErrorManager properly for warnings/errors
//! - More idiomatic Rust patterns

use crate::Compiler::AST::*;
use crate::ErrorManager::{ErrorManager, GeneralErrorType};

/// Security utilities for validating and auto-generating security configurations
pub struct SecurityUtilities;

impl SecurityUtilities {
    /// Ensure security section is valid and complete
    /// Auto-generates defaults if section is missing or incomplete
    ///
    /// # Arguments
    /// * `existing` - Existing security section (if any)
    /// * `dlm_section` - DLM section to determine encryption requirements
    ///
    /// # Returns
    /// * `SecuritySection` - Valid and complete security section
    pub fn ensure_valid_security_section(
        existing: Option<SecuritySection>,
        dlm_section: Option<&DLMSection>,
    ) -> SecuritySection {
        let error_manager = ErrorManager::get_shared_instance();

        error_manager.log_debug("[SecurityUtilities] Ensuring valid SECURITY section...");

        // Check if encryption module is present
        let has_encryption = dlm_section.as_ref()
            .map(|dlm| Self::has_encryption_module(dlm))
            .unwrap_or(false);

        // FIXED: If no encryption, return early without consuming existing
        if !has_encryption {
            error_manager.log_debug("[SecurityUtilities] No encryption module - SECURITY section not required");
            return existing.unwrap_or_else(|| SecuritySection {
                entries: Vec::new(),
                position: Position::UNKNOWN,
            });
        }

        // Encryption module is present - determine algorithm
        let encryption_algorithm = Self::get_encryption_algorithm(dlm_section);
        error_manager.log_info(&format!(
            "[SecurityUtilities] Encryption algorithm detected: {}",
            encryption_algorithm
        ));

        match existing {
            None => {
                error_manager.log_info("[SecurityUtilities] @SECURITY section missing - auto-generating defaults");
                Self::generate_default_security_section(&encryption_algorithm)
            }
            Some(section) => {
                Self::validate_and_complete_security_section(section, &encryption_algorithm)
            }
        }
    }

    // ==================== PRIVATE HELPERS ====================

    /// Check if DLM section has encryption module
    fn has_encryption_module(dlm_section: &DLMSection) -> bool {
        dlm_section.modules.iter()
            .any(|m| matches!(m.module_type, DLMModuleType::DEncryptor))
    }

    /// Get encryption algorithm from DLM section
    fn get_encryption_algorithm(dlm_section: Option<&DLMSection>) -> String {
        let encryptor_module = dlm_section
            .and_then(|dlm| {
                dlm.modules.iter()
                    .find(|m| matches!(m.module_type, DLMModuleType::DEncryptor))
            });

        match encryptor_module.and_then(|m| m.subtype) {
            Some(DLMModuleSubtype::Xor) => "xor".to_string(),
            Some(DLMModuleSubtype::Aes128) => "aes128-gcm".to_string(),
            Some(DLMModuleSubtype::Aes256) => "aes256-gcm".to_string(),
            Some(DLMModuleSubtype::Chacha20) => "chacha20-poly1305".to_string(),
            _ => "aes256-gcm".to_string(), // Default
        }
    }

    /// Generate a complete default security section
    fn generate_default_security_section(algorithm: &str) -> SecuritySection {
        let error_manager = ErrorManager::get_shared_instance();
        error_manager.log_info(&format!(
            "[SecurityUtilities] Generating default SECURITY section for {}",
            algorithm
        ));

        let mut entries = Vec::new();

        // Add encryption entry (default to keyfile mode)
        entries.push(Self::create_encryption_entry(algorithm, "keyfile"));

        // Add validation entry
        entries.push(Self::create_validation_entry(algorithm));

        // Add keystore entry
        entries.push(Self::create_keystore_entry());

        error_manager.log_info(&format!(
            "[SecurityUtilities] ✅ Generated SECURITY section with {} entries",
            entries.len()
        ));

        SecuritySection {
            entries,
            position: Position::UNKNOWN,
        }
    }

    /// Create encryption entry based on algorithm and mode
    fn create_encryption_entry(algorithm: &str, mode: &str) -> SecurityEntry {
        let key_length = Self::get_key_length_for_algorithm(algorithm);

        let mut fields = vec![
            SecurityField::new(
                "mode".to_string(),
                Value::String {
                    value: mode.to_string(),
                    position: Position::UNKNOWN,
                },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "algorithm".to_string(),
                Value::String {
                    value: algorithm.to_string(),
                    position: Position::UNKNOWN,
                },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "key_length".to_string(),
                Value::Integer {
                    value: key_length,
                    position: Position::UNKNOWN,
                },
                Position::UNKNOWN,
            ),
        ];

        // Add KDF fields if password mode
        if mode.eq_ignore_ascii_case("password") {
            fields.extend(vec![
                SecurityField::new(
                    "kdf".to_string(),
                    Value::String {
                        value: "argon2id".to_string(),
                        position: Position::UNKNOWN,
                    },
                    Position::UNKNOWN,
                ),
                SecurityField::new(
                    "kdf_memory".to_string(),
                    Value::Integer {
                        value: 65536,
                        position: Position::UNKNOWN,
                    },
                    Position::UNKNOWN,
                ),
                SecurityField::new(
                    "kdf_iterations".to_string(),
                    Value::Integer {
                        value: 3,
                        position: Position::UNKNOWN,
                    },
                    Position::UNKNOWN,
                ),
                SecurityField::new(
                    "kdf_parallelism".to_string(),
                    Value::Integer {
                        value: 4,
                        position: Position::UNKNOWN,
                    },
                    Position::UNKNOWN,
                ),
            ]);
        }

        SecurityEntry::new(
            "encryption".to_string(),
            fields,
            Position::UNKNOWN,
        )
    }

    /// Create validation entry
    fn create_validation_entry(_algorithm: &str) -> SecurityEntry {
        let fields = vec![
            SecurityField::new(
                "checksum_algorithm".to_string(),
                Value::String {
                    value: "sha256".to_string(),
                    position: Position::UNKNOWN,
                },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "auth_tag_length".to_string(),
                Value::Integer {
                    value: 128,
                    position: Position::UNKNOWN,
                },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "hmac_algorithm".to_string(),
                Value::String {
                    value: "hmac-sha256".to_string(),
                    position: Position::UNKNOWN,
                },
                Position::UNKNOWN,
            ),
        ];

        SecurityEntry::new(
            "validation".to_string(),
            fields,
            Position::UNKNOWN,
        )
    }

    /// Create keystore entry
    fn create_keystore_entry() -> SecurityEntry {
        let fields = vec![
            SecurityField::new(
                "auto_generate".to_string(),
                Value::Boolean {
                    value: true,
                    position: Position::UNKNOWN,
                },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "backup_count".to_string(),
                Value::Integer {
                    value: 3,
                    position: Position::UNKNOWN,
                },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "backup_naming".to_string(),
                Value::String {
                    value: "timestamp".to_string(),
                    position: Position::UNKNOWN,
                },
                Position::UNKNOWN,
            ),
        ];

        SecurityEntry::new(
            "keystore".to_string(),
            fields,
            Position::UNKNOWN,
        )
    }

    /// Validate and complete existing security section
    fn validate_and_complete_security_section(
        mut section: SecuritySection,
        expected_algorithm: &str,
    ) -> SecuritySection {
        let error_manager = ErrorManager::get_shared_instance();
        error_manager.log_debug("[SecurityUtilities] Validating and completing SECURITY section...");

        // Check if encryption entry exists
        let has_encryption = section.entries.iter()
            .any(|e| e.block_key.eq_ignore_ascii_case("encryption"));

        if !has_encryption {
            error_manager.log_info("[SecurityUtilities] Missing 'encryption' entry - adding defaults");
            section.entries.insert(0, Self::create_encryption_entry(expected_algorithm, "keyfile"));
        } else {
            // Validate and complete existing encryption entry
            if let Some(idx) = section.entries.iter()
                .position(|e| e.block_key.eq_ignore_ascii_case("encryption"))
            {
                let entry = section.entries.remove(idx);
                let completed = Self::complete_encryption_entry(entry, expected_algorithm);
                section.entries.insert(idx, completed);
            }
        }

        // Check if validation entry exists
        if !section.entries.iter().any(|e| e.block_key.eq_ignore_ascii_case("validation")) {
            error_manager.log_info("[SecurityUtilities] Missing 'validation' entry - adding defaults");
            section.entries.push(Self::create_validation_entry(expected_algorithm));
        }

        // Check if keystore entry exists
        if !section.entries.iter().any(|e| e.block_key.eq_ignore_ascii_case("keystore")) {
            error_manager.log_info("[SecurityUtilities] Missing 'keystore' entry - adding defaults");
            section.entries.push(Self::create_keystore_entry());
        }

        error_manager.log_info(&format!(
            "[SecurityUtilities] ✅ SECURITY section validated with {} entries",
            section.entries.len()
        ));

        section
    }

    /// Complete encryption entry with missing fields
    fn complete_encryption_entry(
        mut entry: SecurityEntry,
        expected_algorithm: &str,
    ) -> SecurityEntry {
        let error_manager = ErrorManager::get_shared_instance();

        // Get current mode
        let mode = Self::get_string_field_value(&entry.fields, "mode")
            .unwrap_or_else(|| "keyfile".to_string());

        // Ensure mode field exists
        if !Self::has_field(&entry.fields, "mode") {
            error_manager.log_info("[SecurityUtilities] Missing 'mode' field - defaulting to 'keyfile'");
            entry.fields.push(SecurityField::new(
                "mode".to_string(),
                Value::String {
                    value: "keyfile".to_string(),
                    position: Position::UNKNOWN,
                },
                Position::UNKNOWN,
            ));
        }

        // Ensure algorithm field exists
        if !Self::has_field(&entry.fields, "algorithm") {
            error_manager.log_info(&format!(
                "[SecurityUtilities] Missing 'algorithm' field - defaulting to '{}'",
                expected_algorithm
            ));
            entry.fields.push(SecurityField::new(
                "algorithm".to_string(),
                Value::String {
                    value: expected_algorithm.to_string(),
                    position: Position::UNKNOWN,
                },
                Position::UNKNOWN,
            ));
        }

        let algorithm = Self::get_string_field_value(&entry.fields, "algorithm")
            .unwrap_or_else(|| expected_algorithm.to_string());

        // Ensure key_length field exists
        if !Self::has_field(&entry.fields, "key_length") {
            let key_length = Self::get_key_length_for_algorithm(&algorithm);
            error_manager.log_info(&format!(
                "[SecurityUtilities] Missing 'key_length' field - defaulting to {}",
                key_length
            ));
            entry.fields.push(SecurityField::new(
                "key_length".to_string(),
                Value::Integer {
                    value: key_length,
                    position: Position::UNKNOWN,
                },
                Position::UNKNOWN,
            ));
        }

        // Add KDF fields for password mode
        if mode.eq_ignore_ascii_case("password") {
            if !Self::has_field(&entry.fields, "kdf") {
                error_manager.log_info("[SecurityUtilities] Missing 'kdf' field for password mode - defaulting to 'argon2id'");
                entry.fields.push(SecurityField::new(
                    "kdf".to_string(),
                    Value::String {
                        value: "argon2id".to_string(),
                        position: Position::UNKNOWN,
                    },
                    Position::UNKNOWN,
                ));
            }

            if !Self::has_field(&entry.fields, "kdf_memory") {
                error_manager.log_info("[SecurityUtilities] Missing 'kdf_memory' field - defaulting to 65536");
                entry.fields.push(SecurityField::new(
                    "kdf_memory".to_string(),
                    Value::Integer {
                        value: 65536,
                        position: Position::UNKNOWN,
                    },
                    Position::UNKNOWN,
                ));
            }

            if !Self::has_field(&entry.fields, "kdf_iterations") {
                error_manager.log_info("[SecurityUtilities] Missing 'kdf_iterations' field - defaulting to 3");
                entry.fields.push(SecurityField::new(
                    "kdf_iterations".to_string(),
                    Value::Integer {
                        value: 3,
                        position: Position::UNKNOWN,
                    },
                    Position::UNKNOWN,
                ));
            }

            if !Self::has_field(&entry.fields, "kdf_parallelism") {
                error_manager.log_info("[SecurityUtilities] Missing 'kdf_parallelism' field - defaulting to 4");
                entry.fields.push(SecurityField::new(
                    "kdf_parallelism".to_string(),
                    Value::Integer {
                        value: 4,
                        position: Position::UNKNOWN,
                    },
                    Position::UNKNOWN,
                ));
            }
        }

        entry
    }

    // ==================== FIELD HELPERS ====================

    /// Check if field exists in fields list
    fn has_field(fields: &[SecurityField], field_name: &str) -> bool {
        fields.iter()
            .any(|f| f.key.eq_ignore_ascii_case(field_name))
    }

    /// Get string field value
    fn get_string_field_value(fields: &[SecurityField], field_name: &str) -> Option<String> {
        fields.iter()
            .find(|f| f.key.eq_ignore_ascii_case(field_name))
            .and_then(|field| match &field.value {
                Value::String { value, .. } => Some(value.clone()),
                _ => None,
            })
    }

    /// Get key length for algorithm
    fn get_key_length_for_algorithm(algorithm: &str) -> i32 {
        match algorithm.to_lowercase().as_str() {
            "xor" => 32,
            "aes128-gcm" | "aes128" => 16,
            "aes256-gcm" | "aes256" => 32,
            "chacha20-poly1305" | "chacha20" => 32,
            _ => 32, // Default
        }
    }

    // ==================== PUBLIC QUERY METHODS ====================

    /// Get security level for algorithm
    pub fn get_security_level(algorithm: &str) -> &'static str {
        match algorithm.to_lowercase().as_str() {
            "xor" => "LOW",
            "aes128-gcm" | "aes128" => "MEDIUM",
            "aes256-gcm" | "aes256" => "HIGH",
            "chacha20-poly1305" | "chacha20" => "HIGH",
            _ => "UNKNOWN",
        }
    }

    /// Get encryption mode from security section
    pub fn get_encryption_mode(security_section: &SecuritySection) -> String {
        security_section.entries.iter()
            .find(|e| e.block_key.eq_ignore_ascii_case("encryption"))
            .and_then(|entry| Self::get_string_field_value(&entry.fields, "mode"))
            .unwrap_or_else(|| "keyfile".to_string())
    }

    /// Get algorithm from security section
    pub fn get_algorithm(security_section: &SecuritySection) -> String {
        security_section.entries.iter()
            .find(|e| e.block_key.eq_ignore_ascii_case("encryption"))
            .and_then(|entry| Self::get_string_field_value(&entry.fields, "algorithm"))
            .unwrap_or_else(|| "aes256-gcm".to_string())
    }

    /// Validate security section for encryption
    ///
    /// # Returns
    /// * `Ok(())` - If valid
    /// * `Err(Vec<String>)` - List of validation errors
    pub fn is_valid_for_encryption(security_section: &SecuritySection) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        let encryption_entry = security_section.entries.iter()
            .find(|e| e.block_key.eq_ignore_ascii_case("encryption"));

        if encryption_entry.is_none() {
            errors.push("Missing 'encryption' entry in SECURITY section".to_string());
            return Err(errors);
        }

        let entry = encryption_entry.unwrap();

        if !Self::has_field(&entry.fields, "mode") {
            errors.push("Missing 'mode' field in encryption entry".to_string());
        }

        if !Self::has_field(&entry.fields, "algorithm") {
            errors.push("Missing 'algorithm' field in encryption entry".to_string());
        }

        if !Self::has_field(&entry.fields, "key_length") {
            errors.push("Missing 'key_length' field in encryption entry".to_string());
        }

        let mode = Self::get_string_field_value(&entry.fields, "mode");
        if let Some(m) = mode {
            if m.eq_ignore_ascii_case("password") && !Self::has_field(&entry.fields, "kdf") {
                errors.push("Missing 'kdf' field for password mode".to_string());
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// ==================== TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_default_security() {
        let section = SecurityUtilities::generate_default_security_section("aes256-gcm");

        assert_eq!(section.entries.len(), 3);
        assert_eq!(section.entries[0].block_key, "encryption");
        assert_eq!(section.entries[1].block_key, "validation");
        assert_eq!(section.entries[2].block_key, "keystore");
    }

    #[test]
    fn test_ensure_valid_security_with_aes128() {
        let dlm = DLMSection {
            modules: vec![DLMModule {
                module_type: DLMModuleType::DEncryptor,
                subtype: Some(DLMModuleSubtype::Aes128),
                position: Position::UNKNOWN,
            }],
            position: Position::UNKNOWN,
        };

        let section = SecurityUtilities::ensure_valid_security_section(None, Some(&dlm));

        assert!(!section.entries.is_empty());
        let encryption = section.entries.iter()
            .find(|e| e.block_key == "encryption")
            .unwrap();

        // Check algorithm field
        let algorithm_field = encryption.fields.iter()
            .find(|f| f.key == "algorithm")
            .unwrap();

        if let Value::String { value, .. } = &algorithm_field.value {
            assert_eq!(value.to_string(), "aes128-gcm");
        } else {
            panic!("Algorithm should be a string");
        }
    }

    #[test]
    fn test_password_mode_kdf_fields() {
        let entry = SecurityUtilities::create_encryption_entry("aes256-gcm", "password");

        assert!(SecurityUtilities::has_field(&entry.fields, "kdf"));
        assert!(SecurityUtilities::has_field(&entry.fields, "kdf_memory"));
        assert!(SecurityUtilities::has_field(&entry.fields, "kdf_iterations"));
        assert!(SecurityUtilities::has_field(&entry.fields, "kdf_parallelism"));
    }

    #[test]
    fn test_no_encryption_returns_empty_or_existing() {
        // No DLM section
        let result = SecurityUtilities::ensure_valid_security_section(None, None);
        assert_eq!(result.entries.len(), 0);

        // Existing section should be preserved
        let existing = SecuritySection {
            entries: vec![],
            position: Position::UNKNOWN,
        };
        let result = SecurityUtilities::ensure_valid_security_section(Some(existing), None);
        assert_eq!(result.entries.len(), 0);
    }
}