// src/Compiler/Utilities/security_utilities.rs
//! Security section utilities for DixScript
//! Handles auto-generation and validation of @SECURITY sections

use crate::Compiler::AST::*;

pub struct SecurityUtilities;

impl SecurityUtilities {
    /// Ensure security section is valid and complete
    /// Auto-generates defaults if section is missing or incomplete
    pub fn ensure_valid_security_section(
        existing: Option<SecuritySection>,
        dlm_section: Option<&DLMSection>,
    ) -> SecuritySection {
        // Check if DEncryptor is present
        let has_encryptor = dlm_section
            .map(|dlm| dlm.modules.iter().any(|m| matches!(m.module_type, DLMModuleType::DEncryptor)))
            .unwrap_or(false);
        
        if !has_encryptor {
            // No encryptor, return existing or empty section
            return existing.unwrap_or_else(|| SecuritySection {
                entries: Vec::new(),
                position: Position::UNKNOWN,
            });
        }
        
        // Get encryptor subtype
        let encryptor_subtype = dlm_section
            .and_then(|dlm| {
                dlm.modules
                    .iter()
                    .find(|m| matches!(m.module_type, DLMModuleType::DEncryptor))
                    .and_then(|m| m.subtype)
            });
        
        match existing {
            Some(mut section) => {
                // Validate and complete existing section
                Self::ensure_encryption_entry(&mut section, encryptor_subtype);
                Self::ensure_keystore_entry(&mut section);
                section
            }
            None => {
                // Generate complete default section
                Self::generate_default_security_section(encryptor_subtype)
            }
        }
    }
    
    /// Generate a complete default security section
    fn generate_default_security_section(encryptor_subtype: Option<DLMModuleSubtype>) -> SecuritySection {
        let mut entries = Vec::new();
        
        // Add encryption entry
        entries.push(Self::create_encryption_entry(encryptor_subtype));
        
        // Add keystore entry
        entries.push(Self::create_keystore_entry());
        
        SecuritySection {
            entries,
            position: Position::UNKNOWN,
        }
    }
    
    /// Ensure encryption entry exists
    fn ensure_encryption_entry(section: &mut SecuritySection, encryptor_subtype: Option<DLMModuleSubtype>) {
        // Check if encryption entry exists
        let has_encryption = section.entries.iter().any(|e| e.block_key == "encryption");
        
        if !has_encryption {
            section.entries.insert(0, Self::create_encryption_entry(encryptor_subtype));
        } else {
            // Validate existing encryption entry
            if let Some(entry) = section.entries.iter_mut().find(|e| e.block_key == "encryption") {
                Self::validate_encryption_fields(entry, encryptor_subtype);
            }
        }
    }
    
    /// Ensure keystore entry exists
    fn ensure_keystore_entry(section: &mut SecuritySection) {
        let has_keystore = section.entries.iter().any(|e| e.block_key == "keystore");
        
        if !has_keystore {
            section.entries.push(Self::create_keystore_entry());
        }
    }
    
    /// Create encryption entry based on encryptor type
    fn create_encryption_entry(encryptor_subtype: Option<DLMModuleSubtype>) -> SecurityEntry {
        let (algorithm, key_size) = match encryptor_subtype {
            Some(DLMModuleSubtype::Aes256) => ("aes256", 256),
            Some(DLMModuleSubtype::Aes128) => ("aes128", 128),
            Some(DLMModuleSubtype::Chacha20) => ("chacha20", 256),
            Some(DLMModuleSubtype::Xor) => ("xor", 128),
            _ => ("aes256", 256), // Default
        };
        
        let fields = vec![
            SecurityField {
                key: "algorithm".to_string(),
                value: Value::String {
                    value: algorithm.to_string(),
                    position: Position::UNKNOWN,
                },
                position: Position::UNKNOWN,
            },
            SecurityField {
                key: "key_size".to_string(),
                value: Value::Integer {
                    value: key_size,
                    position: Position::UNKNOWN,
                },
                position: Position::UNKNOWN,
            },
            SecurityField {
                key: "mode".to_string(),
                value: Value::String {
                    value: "cbc".to_string(),
                    position: Position::UNKNOWN,
                },
                position: Position::UNKNOWN,
            },
        ];
        
        SecurityEntry {
            block_key: "encryption".to_string(),
            fields,
            position: Position::UNKNOWN,
        }
    }
    
    /// Create keystore entry
    fn create_keystore_entry() -> SecurityEntry {
        let fields = vec![
            SecurityField {
                key: "path".to_string(),
                value: Value::Identifier {
                    value: "auto".to_string(),
                    position: Position::UNKNOWN,
                },
                position: Position::UNKNOWN,
            },
            SecurityField {
                key: "auto_generate".to_string(),
                value: Value::Boolean {
                    value: true,
                    position: Position::UNKNOWN,
                },
                position: Position::UNKNOWN,
            },
        ];
        
        SecurityEntry {
            block_key: "keystore".to_string(),
            fields,
            position: Position::UNKNOWN,
        }
    }
    
    /// Validate encryption entry fields
    fn validate_encryption_fields(entry: &mut SecurityEntry, encryptor_subtype: Option<DLMModuleSubtype>) {
        // Ensure required fields exist
        let has_algorithm = entry.fields.iter().any(|f| f.key == "algorithm");
        let has_key_size = entry.fields.iter().any(|f| f.key == "key_size");
        
        if !has_algorithm {
            let algorithm = match encryptor_subtype {
                Some(DLMModuleSubtype::Aes256) => "aes256",
                Some(DLMModuleSubtype::Aes128) => "aes128",
                Some(DLMModuleSubtype::Chacha20) => "chacha20",
                Some(DLMModuleSubtype::Xor) => "xor",
                _ => "aes256",
            };
            
            entry.fields.push(SecurityField {
                key: "algorithm".to_string(),
                value: Value::String {
                    value: algorithm.to_string(),
                    position: Position::UNKNOWN,
                },
                position: Position::UNKNOWN,
            });
        }
        
        if !has_key_size {
            let key_size = match encryptor_subtype {
                Some(DLMModuleSubtype::Aes256) => 256,
                Some(DLMModuleSubtype::Aes128) => 128,
                Some(DLMModuleSubtype::Chacha20) => 256,
                Some(DLMModuleSubtype::Xor) => 128,
                _ => 256,
            };
            
            entry.fields.push(SecurityField {
                key: "key_size".to_string(),
                value: Value::Integer {
                    value: key_size,
                    position: Position::UNKNOWN,
                },
                position: Position::UNKNOWN,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_default_security() {
        let section = SecurityUtilities::generate_default_security_section(Some(DLMModuleSubtype::Aes256));
        
        assert_eq!(section.entries.len(), 2);
        assert_eq!(section.entries[0].block_key, "encryption");
        assert_eq!(section.entries[1].block_key, "keystore");
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
        let encryption = section.entries.iter().find(|e| e.block_key == "encryption").unwrap();
        
        // Check algorithm field
        let algorithm_field = encryption.fields.iter().find(|f| f.key == "algorithm").unwrap();
        if let Value::String { value, .. } = &algorithm_field.value {
            assert_eq!(value, "aes128");
        } else {
            panic!("Algorithm should be a string");
        }
    }
      }
