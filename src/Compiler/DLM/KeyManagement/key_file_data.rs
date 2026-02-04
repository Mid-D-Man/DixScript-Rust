//! Data structures for .mdix.key file contents

use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

/// Complete key file data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyFileData {
    pub config: KeyFileConfig,
    pub pipeline: DLMPipelineInfo,
    pub key_data: KeyDataSection,
    pub file_info: FileInfoSection,
}

impl KeyFileData {
    pub fn new() -> Self {
        KeyFileData {
            config: KeyFileConfig::new(),
            pipeline: DLMPipelineInfo::new(),
            key_data: KeyDataSection::new(),
            file_info: FileInfoSection::new(),
        }
    }
}

impl Default for KeyFileData {
    fn default() -> Self {
        Self::new()
    }
}

/// @CONFIG section of .mdix.key file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyFileConfig {
    pub version: String,
    pub key_type: String, // "keyfile" or "password"
    pub generated: DateTime<Utc>,
    pub source_file: Option<String>,
}

impl KeyFileConfig {
    pub fn new() -> Self {
        KeyFileConfig {
            version: "1.0.0".to_string(),
            key_type: "keyfile".to_string(),
            generated: Utc::now(),
            source_file: None,
        }
    }
}

impl Default for KeyFileConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// @DLM_PIPELINE section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DLMPipelineInfo {
    pub modules_used: Vec<String>,
    pub execution_order: Vec<PipelineExecutionStep>,
    pub reversal_order: Vec<String>,
}

impl DLMPipelineInfo {
    pub fn new() -> Self {
        DLMPipelineInfo {
            modules_used: Vec::new(),
            execution_order: Vec::new(),
            reversal_order: Vec::new(),
        }
    }
}

impl Default for DLMPipelineInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Single pipeline execution step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineExecutionStep {
    pub step: usize,
    pub module: String,
    pub input_size: usize,
    pub output_size: usize,
    pub duration_ms: f64,
}

impl PipelineExecutionStep {
    pub fn new(step: usize, module: String, input_size: usize, output_size: usize, duration_ms: f64) -> Self {
        PipelineExecutionStep {
            step,
            module,
            input_size,
            output_size,
            duration_ms,
        }
    }
}

/// @KEY_DATA section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDataSection {
    pub encryption: Option<EncryptionKeyData>,
    pub compression: Option<CompressionKeyData>,
    pub validation: Option<ValidationData>,
}

impl KeyDataSection {
    pub fn new() -> Self {
        KeyDataSection {
            encryption: None,
            compression: None,
            validation: None,
        }
    }
}

impl Default for KeyDataSection {
    fn default() -> Self {
        Self::new()
    }
}

/// Encryption key data and parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionKeyData {
    pub algorithm: String,
    pub key_length: usize,
    pub security_level: String,
    pub key_data: Option<String>, // Base64 encoded (keyfile mode only)
    pub iv: String, // Base64 encoded IV/nonce
    pub kdf: Option<KDFParameters>, // Password mode only
    pub auth_tag: Option<String>,
}

impl EncryptionKeyData {
    pub fn new(algorithm: String) -> Self {
        EncryptionKeyData {
            algorithm,
            key_length: 32,
            security_level: "HIGH".to_string(),
            key_data: None,
            iv: String::new(),
            kdf: None,
            auth_tag: None,
        }
    }
}

/// Key Derivation Function parameters (password mode)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KDFParameters {
    pub algorithm: String,
    pub kdf_version: String,
    pub memory: u32, // KB
    pub iterations: u32,
    pub parallelism: u32,
    pub salt: String, // Base64 encoded
    pub salt_length: usize,
}

impl KDFParameters {
    pub fn new() -> Self {
        KDFParameters {
            algorithm: "argon2id".to_string(),
            kdf_version: "1.3".to_string(),
            memory: 65536, // 64 MB
            iterations: 3,
            parallelism: 4,
            salt: String::new(),
            salt_length: 32,
        }
    }
}

impl Default for KDFParameters {
    fn default() -> Self {
        Self::new()
    }
}

/// Compression key data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionKeyData {
    pub algorithm: String,
    pub compression_level: Option<String>,
    pub original_size: usize,
    pub compressed_size: usize,
    pub compression_ratio: f64,
}

impl CompressionKeyData {
    pub fn new(algorithm: String) -> Self {
        CompressionKeyData {
            algorithm,
            compression_level: None,
            original_size: 0,
            compressed_size: 0,
            compression_ratio: 0.0,
        }
    }
}

/// Validation and integrity data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationData {
    pub original_checksum: String,
    pub compressed_checksum: String,
    pub encrypted_checksum: String,
    pub checksum_algorithm: String,
    pub auth_tag_length: usize,
    pub hmac_algorithm: String,
}

impl ValidationData {
    pub fn new() -> Self {
        ValidationData {
            original_checksum: String::new(),
            compressed_checksum: String::new(),
            encrypted_checksum: String::new(),
            checksum_algorithm: "sha256".to_string(),
            auth_tag_length: 128,
            hmac_algorithm: "hmac-sha256".to_string(),
        }
    }
}

impl Default for ValidationData {
    fn default() -> Self {
        Self::new()
    }
}

/// @FILE_INFO section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfoSection {
    pub original_size: usize,
    pub compressed_size: usize,
    pub encrypted_size: usize,
    pub compression_ratio: f64,
    pub created: DateTime<Utc>,
    pub source_file: Option<String>,
    pub output_file: Option<String>,
}

impl FileInfoSection {
    pub fn new() -> Self {
        FileInfoSection {
            original_size: 0,
            compressed_size: 0,
            encrypted_size: 0,
            compression_ratio: 0.0,
            created: Utc::now(),
            source_file: None,
            output_file: None,
        }
    }
}

impl Default for FileInfoSection {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for KeyFileData
pub struct KeyFileDataBuilder {
    data: KeyFileData,
}

impl KeyFileDataBuilder {
    pub fn new() -> Self {
        KeyFileDataBuilder {
            data: KeyFileData::new(),
        }
    }
    
    pub fn with_source_file(mut self, source_file: String) -> Self {
        self.data.config.source_file = Some(source_file.clone());
        self.data.file_info.source_file = Some(source_file);
        self
    }
    
    pub fn with_encryption_mode(mut self, mode: String) -> Self {
        self.data.config.key_type = mode;
        self
    }
    
    pub fn with_module(mut self, module_name: String) -> Self {
        self.data.pipeline.modules_used.push(module_name);
        self
    }
    
    pub fn with_execution_step(
        mut self,
        step: usize,
        module: String,
        input_size: usize,
        output_size: usize,
        duration_ms: f64
    ) -> Self {
        self.data.pipeline.execution_order.push(
            PipelineExecutionStep::new(step, module, input_size, output_size, duration_ms)
        );
        self
    }
    
    pub fn with_encryption(mut self, encryption: EncryptionKeyData) -> Self {
        self.data.key_data.encryption = Some(encryption);
        self
    }
    
    pub fn with_compression(mut self, compression: CompressionKeyData) -> Self {
        self.data.key_data.compression = Some(compression);
        self
    }
    
    pub fn with_validation(mut self, validation: ValidationData) -> Self {
        self.data.key_data.validation = Some(validation);
        self
    }
    
    pub fn with_file_sizes(mut self, original: usize, compressed: usize, encrypted: usize) -> Self {
        self.data.file_info.original_size = original;
        self.data.file_info.compressed_size = compressed;
        self.data.file_info.encrypted_size = encrypted;
        
        if original > 0 && encrypted > 0 {
            self.data.file_info.compression_ratio = 1.0 - (encrypted as f64 / original as f64);
        }
        
        self
    }
    
    pub fn build(mut self) -> KeyFileData {
        // Calculate reversal order (reverse of execution order)
        self.data.pipeline.reversal_order = self.data.pipeline.modules_used
            .iter()
            .rev()
            .cloned()
            .collect();
        
        self.data
    }
}

impl Default for KeyFileDataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension methods for KeyFileData
impl KeyFileData {
    /// Check if using password mode
    pub fn is_password_mode(&self) -> bool {
        self.config.key_type.eq_ignore_ascii_case("password")
    }
    
    /// Check if using keyfile mode
    pub fn is_keyfile_mode(&self) -> bool {
        self.config.key_type.eq_ignore_ascii_case("keyfile")
    }
    
    /// Check if encryption was used
    pub fn has_encryption(&self) -> bool {
        self.key_data.encryption.is_some()
    }
    
    /// Check if compression was used
    pub fn has_compression(&self) -> bool {
        self.key_data.compression.is_some()
    }
    
    /// Get human-readable summary
    pub fn get_summary(&self) -> String {
        let modules = self.pipeline.modules_used.join(" → ");
        let mode = if self.is_password_mode() { "Password Mode" } else { "Keyfile Mode" };
        format!("{}: {}", mode, modules)
    }
    
    /// Validate key file data integrity
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        
        // Check version
        if self.config.version.is_empty() {
            errors.push("Missing version in config".to_string());
        }
        
        // Check modules
        if self.pipeline.modules_used.is_empty() {
            errors.push("No modules specified in pipeline".to_string());
        }
        
        // Check encryption data
        if let Some(ref enc) = self.key_data.encryption {
            if enc.algorithm.is_empty() {
                errors.push("Missing encryption algorithm".to_string());
            }
            
            if enc.iv.is_empty() {
                errors.push("Missing IV/nonce".to_string());
            }
            
            // Keyfile mode requires key data
            if self.is_keyfile_mode() && enc.key_data.is_none() {
                errors.push("Keyfile mode requires key_data".to_string());
            }
            
            // Password mode requires KDF parameters
            if self.is_password_mode() && enc.kdf.is_none() {
                errors.push("Password mode requires KDF parameters".to_string());
            }
        }
        
        // Check compression data
        if let Some(ref comp) = self.key_data.compression {
            if comp.algorithm.is_empty() {
                errors.push("Missing compression algorithm".to_string());
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
