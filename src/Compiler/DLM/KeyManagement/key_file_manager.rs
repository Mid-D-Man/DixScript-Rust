//! Key File Manager - handles .dxkey file creation and reading
//! Manages encryption keys, compression metadata, and audit information

use crate::Compiler::DLM::dlm_module_base::DLMModuleBase;
use crate::ErrorManager::{DlmErrorType, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Metadata stored in .dxkey file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyFileMetadata {
    pub version: String,
    pub source_file: String,
    pub compiled_file: String,
    pub timestamp: String,
    pub compression: Option<CompressionMetadata>,
    pub encryption: Option<EncryptionMetadata>,
    pub audit: Option<AuditMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionMetadata {
    pub algorithm: String,
    pub original_size: usize,
    pub compressed_size: usize,
    pub compression_ratio: f64,
    pub module_name: String,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionMetadata {
    pub algorithm: String,
    pub key_data: Option<String>, // Base64 encoded key (only for keyfile mode)
    pub iv: Option<String>,        // Base64 encoded IV
    pub salt: Option<String>,      // Base64 encoded salt (password mode)
    pub kdf_algorithm: Option<String>,
    pub kdf_iterations: Option<u32>,
    pub kdf_memory: Option<u32>,
    pub kdf_parallelism: Option<u32>,
    pub key_length: u32,
    pub module_name: String,
    pub priority: i32,
    pub security_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditMetadata {
    pub audit_type: String,
    pub audit_file_path: String,
    pub timestamp: String,
    pub module_name: String,
}

/// Key File Manager for .dxkey files
pub struct KeyFileManager {
    base: DLMModuleBase,
    output_directory: String,
    source_file_path: String,
}

impl KeyFileManager {
    /// Create new KeyFileManager
    pub fn new(source_file_path: String, output_directory: String) -> Self {
        let base = DLMModuleBase::new("KeyFileManager", 0);

        KeyFileManager {
            base,
            output_directory,
            source_file_path,
        }
    }

    /// Create .dxkey file with metadata
    pub fn create_key_file(
        &self,
        compiled_file_path: &str,
        compression_metadata: Option<HashMap<String, String>>,
        encryption_metadata: Option<HashMap<String, String>>,
        audit_metadata: Option<HashMap<String, String>>,
    ) -> Result<String, String> {
        if self.base.is_debug_enabled() {
            self.base.log_info("Creating .dxkey file...");
        }

        // Build metadata structure
        let metadata = KeyFileMetadata {
            version: "1.0.0".to_string(),
            source_file: self.source_file_path.clone(),
            compiled_file: compiled_file_path.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            compression: compression_metadata.as_ref().map(|m| self.parse_compression_metadata(m)),
            encryption: encryption_metadata.as_ref().map(|m| self.parse_encryption_metadata(m)),
            audit: audit_metadata.as_ref().map(|m| self.parse_audit_metadata(m)),
        };

        // Serialize to JSON
        let json_content = serde_json::to_string_pretty(&metadata).map_err(|e| {
            let error_msg = format!("Failed to serialize key file metadata: {}", e);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvocationFailed,
                error_msg.clone(),
                Some(self.base.module_name().to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            error_msg
        })?;

        // Generate key file path
        let key_file_path = self.generate_key_file_path(compiled_file_path);

        // Ensure output directory exists
        if let Some(parent) = Path::new(&key_file_path).parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!("Failed to create key file directory: {}", e)
            })?;
        }

        // Write to file
        fs::write(&key_file_path, json_content).map_err(|e| {
            let error_msg = format!("Failed to write key file: {}", e);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvocationFailed,
                error_msg.clone(),
                Some(self.base.module_name().to_string()),
                None,
                Some(format!("Check write permissions for: {}", key_file_path)),
                ErrorSeverity::Error,
            );
            error_msg
        })?;

        self.base.log_info(&format!("✅ Key file created: {}", key_file_path));

        if self.base.is_verbose_enabled() {
            self.base.log_debug("Key file contents:");
            self.base.log_debug(&format!("  - Version: {}", metadata.version));
            self.base.log_debug(&format!("  - Source: {}", metadata.source_file));
            self.base.log_debug(&format!("  - Compiled: {}", metadata.compiled_file));

            if let Some(ref comp) = metadata.compression {
                self.base.log_debug(&format!("  - Compression: {} ({:.1}% reduction)",
                                             comp.algorithm, (1.0 - comp.compression_ratio) * 100.0));
            }

            if let Some(ref enc) = metadata.encryption {
                self.base.log_debug(&format!("  - Encryption: {} ({} security)",
                                             enc.algorithm, enc.security_level));
            }

            if let Some(ref audit) = metadata.audit {
                self.base.log_debug(&format!("  - Audit: {}", audit.audit_type));
            }
        }

        Ok(key_file_path)
    }

    /// Read and parse .dxkey file
    pub fn read_key_file(&self, key_file_path: &str) -> Result<KeyFileMetadata, String> {
        if self.base.is_debug_enabled() {
            self.base.log_info(&format!("Reading key file: {}", key_file_path));
        }

        // Check if file exists
        if !Path::new(key_file_path).exists() {
            let error_msg = format!("Key file not found: {}", key_file_path);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvalidFunctionSignature,
                error_msg.clone(),
                Some(self.base.module_name().to_string()),
                None,
                Some("Ensure the .dxkey file exists in the expected location".to_string()),
                ErrorSeverity::Error,
            );
            return Err(error_msg);
        }

        // Read file content
        let json_content = fs::read_to_string(key_file_path).map_err(|e| {
            let error_msg = format!("Failed to read key file: {}", e);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvocationFailed,
                error_msg.clone(),
                Some(self.base.module_name().to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            error_msg
        })?;

        // Parse JSON
        let metadata: KeyFileMetadata = serde_json::from_str(&json_content).map_err(|e| {
            let error_msg = format!("Failed to parse key file JSON: {}", e);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvocationFailed,
                error_msg.clone(),
                Some(self.base.module_name().to_string()),
                None,
                Some("Key file may be corrupted or in invalid format".to_string()),
                ErrorSeverity::Error,
            );
            error_msg
        })?;

        if self.base.is_debug_enabled() {
            self.base.log_info("✅ Key file loaded successfully");

            if self.base.is_verbose_enabled() {
                self.base.log_debug(&format!("  - Version: {}", metadata.version));
                self.base.log_debug(&format!("  - Source: {}", metadata.source_file));
                self.base.log_debug(&format!("  - Compiled: {}", metadata.compiled_file));
            }
        }

        Ok(metadata)
    }

    /// Generate key file path from compiled file path
    fn generate_key_file_path(&self, compiled_file_path: &str) -> String {
        let compiled_path = Path::new(compiled_file_path);
        let file_stem = compiled_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");

        let key_file_name = format!("{}.dxkey", file_stem);

        Path::new(&self.output_directory)
            .join(key_file_name)
            .to_string_lossy()
            .to_string()
    }

    /// Parse compression metadata from HashMap
    fn parse_compression_metadata(&self, metadata: &HashMap<String, String>) -> CompressionMetadata {
        CompressionMetadata {
            algorithm: metadata.get("algorithm").cloned().unwrap_or_default(),
            original_size: metadata.get("original_size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            compressed_size: metadata.get("compressed_size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            compression_ratio: metadata.get("compression_ratio")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0),
            module_name: metadata.get("module_name").cloned().unwrap_or_default(),
            priority: metadata.get("priority")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
        }
    }

    /// Parse encryption metadata from HashMap
    fn parse_encryption_metadata(&self, metadata: &HashMap<String, String>) -> EncryptionMetadata {
        EncryptionMetadata {
            algorithm: metadata.get("algorithm").cloned().unwrap_or_default(),
            key_data: metadata.get("key_data").cloned(),
            iv: metadata.get("iv").cloned(),
            salt: metadata.get("salt").cloned(),
            kdf_algorithm: metadata.get("kdf_algorithm").cloned(),
            kdf_iterations: metadata.get("kdf_iterations")
                .and_then(|s| s.parse().ok()),
            kdf_memory: metadata.get("kdf_memory")
                .and_then(|s| s.parse().ok()),
            kdf_parallelism: metadata.get("kdf_parallelism")
                .and_then(|s| s.parse().ok()),
            key_length: metadata.get("key_length")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            module_name: metadata.get("module_name").cloned().unwrap_or_default(),
            priority: metadata.get("priority")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            security_level: metadata.get("security_level").cloned().unwrap_or_else(|| "UNKNOWN".to_string()),
        }
    }

    /// Parse audit metadata from HashMap
    fn parse_audit_metadata(&self, metadata: &HashMap<String, String>) -> AuditMetadata {
        AuditMetadata {
            audit_type: metadata.get("audit_type").cloned().unwrap_or_default(),
            audit_file_path: metadata.get("audit_file_path").cloned().unwrap_or_default(),
            timestamp: metadata.get("timestamp").cloned().unwrap_or_default(),
            module_name: metadata.get("module_name").cloned().unwrap_or_default(),
        }
    }

    /// Extract encryption metadata as HashMap for re-initialization
    pub fn extract_encryption_config(&self, metadata: &KeyFileMetadata) -> Option<HashMap<String, String>> {
        metadata.encryption.as_ref().map(|enc| {
            let mut config = HashMap::new();

            config.insert("algorithm".to_string(), enc.algorithm.clone());
            config.insert("key_length".to_string(), enc.key_length.to_string());

            if let Some(ref key_data) = enc.key_data {
                config.insert("key_data".to_string(), key_data.clone());
            }

            if let Some(ref iv) = enc.iv {
                config.insert("iv".to_string(), iv.clone());
            }

            if let Some(ref salt) = enc.salt {
                config.insert("salt".to_string(), salt.clone());
            }

            if let Some(ref kdf_algorithm) = enc.kdf_algorithm {
                config.insert("kdf_algorithm".to_string(), kdf_algorithm.clone());
            }

            if let Some(kdf_iterations) = enc.kdf_iterations {
                config.insert("kdf_iterations".to_string(), kdf_iterations.to_string());
            }

            if let Some(kdf_memory) = enc.kdf_memory {
                config.insert("kdf_memory".to_string(), kdf_memory.to_string());
            }

            if let Some(kdf_parallelism) = enc.kdf_parallelism {
                config.insert("kdf_parallelism".to_string(), kdf_parallelism.to_string());
            }

            config
        })
    }

    /// Extract compression metadata as HashMap for re-initialization
    pub fn extract_compression_config(&self, metadata: &KeyFileMetadata) -> Option<HashMap<String, String>> {
        metadata.compression.as_ref().map(|comp| {
            let mut config = HashMap::new();
            config.insert("algorithm".to_string(), comp.algorithm.clone());
            config.insert("original_size".to_string(), comp.original_size.to_string());
            config.insert("compressed_size".to_string(), comp.compressed_size.to_string());
            config.insert("compression_ratio".to_string(), comp.compression_ratio.to_string());
            config
        })
    }

    /// Check if key file is password-protected
    pub fn is_password_protected(&self, metadata: &KeyFileMetadata) -> bool {
        metadata.encryption.as_ref()
            .map(|enc| enc.salt.is_some() && enc.kdf_algorithm.is_some())
            .unwrap_or(false)
    }

    /// Validate key file integrity
    pub fn validate_key_file(&self, metadata: &KeyFileMetadata) -> Result<(), String> {
        // Check version
        if metadata.version.is_empty() {
            return Err("Key file missing version information".to_string());
        }

        // Check source file reference
        if metadata.source_file.is_empty() {
            return Err("Key file missing source file reference".to_string());
        }

        // Check compiled file reference
        if metadata.compiled_file.is_empty() {
            return Err("Key file missing compiled file reference".to_string());
        }

        // Validate encryption metadata if present
        if let Some(ref enc) = metadata.encryption {
            if enc.algorithm.is_empty() {
                return Err("Encryption metadata missing algorithm".to_string());
            }

            if enc.key_length == 0 {
                return Err("Encryption metadata has invalid key length".to_string());
            }
        }

        if self.base.is_debug_enabled() {
            self.base.log_debug("✅ Key file validation passed");
        }

        Ok(())
    }
}