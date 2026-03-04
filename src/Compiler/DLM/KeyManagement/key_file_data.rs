//! In-memory data model for `.mdix.key` file contents.

use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Complete key file data — the canonical in-memory representation.
#[derive(Debug, Clone)]
pub struct KeyFileData {
    pub config:    KeyFileConfig,
    pub pipeline:  DLMPipelineInfo,
    pub key_data:  KeyDataSection,
    pub file_info: FileInfoSection,
}

impl KeyFileData {
    pub fn new() -> Self {
        KeyFileData {
            config:    KeyFileConfig::new(),
            pipeline:  DLMPipelineInfo::new(),
            key_data:  KeyDataSection::new(),
            file_info: FileInfoSection::new(),
        }
    }

    pub fn is_password_mode(&self) -> bool {
        self.config.key_type.eq_ignore_ascii_case("password")
    }

    pub fn is_keyfile_mode(&self) -> bool {
        self.config.key_type.eq_ignore_ascii_case("keyfile")
    }

    pub fn has_encryption(&self) -> bool {
        self.key_data.encryption.is_some()
    }

    pub fn has_compression(&self) -> bool {
        self.key_data.compression.is_some()
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.config.version.is_empty() {
            errors.push("Missing version in config".to_string());
        }
        if self.pipeline.modules_used.is_empty() {
            errors.push("No modules specified in pipeline".to_string());
        }
        if let Some(ref enc) = self.key_data.encryption {
            if enc.algorithm.is_empty() {
                errors.push("Missing encryption algorithm".to_string());
            }
            if enc.iv.is_empty() {
                errors.push("Missing IV/nonce".to_string());
            }
            if self.is_keyfile_mode() && enc.key_data.is_none() {
                errors.push("Keyfile mode requires key_data".to_string());
            }
            if self.is_password_mode() && enc.kdf.is_none() {
                errors.push("Password mode requires KDF parameters".to_string());
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl Default for KeyFileData {
    fn default() -> Self { Self::new() }
}

/// @KEY_CONFIG section.
#[derive(Debug, Clone)]
pub struct KeyFileConfig {
    pub version:     String,
    pub key_type:    String,
    pub generated:   DateTime<Utc>,
    pub source_file: Option<String>,
}

impl KeyFileConfig {
    pub fn new() -> Self {
        KeyFileConfig {
            version:     "1.0.0".to_string(),
            key_type:    "keyfile".to_string(),
            generated:   Utc::now(),
            source_file: None,
        }
    }
}

impl Default for KeyFileConfig {
    fn default() -> Self { Self::new() }
}

/// @KEY_PIPELINE section.
#[derive(Debug, Clone)]
pub struct DLMPipelineInfo {
    pub modules_used:    Vec<String>,
    pub execution_steps: Vec<PipelineExecutionStep>,
    pub reversal_order:  Vec<String>,
}

impl DLMPipelineInfo {
    pub fn new() -> Self {
        DLMPipelineInfo {
            modules_used:    Vec::new(),
            execution_steps: Vec::new(),
            reversal_order:  Vec::new(),
        }
    }
}

impl Default for DLMPipelineInfo {
    fn default() -> Self { Self::new() }
}

/// A single recorded pipeline execution step.
#[derive(Debug, Clone)]
pub struct PipelineExecutionStep {
    pub step:        usize,
    pub module:      String,
    pub input_size:  usize,
    pub output_size: usize,
    pub duration_ms: f64,
}

impl PipelineExecutionStep {
    pub fn new(
        step: usize,
        module: String,
        input_size: usize,
        output_size: usize,
        duration_ms: f64,
    ) -> Self {
        PipelineExecutionStep { step, module, input_size, output_size, duration_ms }
    }
}

/// @KEY_DATA section grouping encryption, compression, and validation info.
#[derive(Debug, Clone)]
pub struct KeyDataSection {
    pub encryption:  Option<EncryptionKeyData>,
    pub compression: Option<CompressionKeyData>,
    pub validation:  Option<ValidationData>,
}

impl KeyDataSection {
    pub fn new() -> Self {
        KeyDataSection { encryption: None, compression: None, validation: None }
    }
}

impl Default for KeyDataSection {
    fn default() -> Self { Self::new() }
}

/// Encryption key material and algorithm parameters.
#[derive(Debug, Clone)]
pub struct EncryptionKeyData {
    pub algorithm:      String,
    pub key_length:     usize,
    pub security_level: String,
    /// Base64-encoded key (keyfile mode only — absent in password mode).
    pub key_data:       Option<String>,
    /// Base64-encoded IV / nonce.
    pub iv:             String,
    /// KDF parameters (password mode only).
    pub kdf:            Option<KDFParameters>,
}

impl EncryptionKeyData {
    pub fn new(algorithm: String) -> Self {
        EncryptionKeyData {
            algorithm,
            key_length:     32,
            security_level: "HIGH".to_string(),
            key_data:       None,
            iv:             String::new(),
            kdf:            None,
        }
    }
}

/// Argon2id key derivation parameters (password mode).
#[derive(Debug, Clone)]
pub struct KDFParameters {
    pub algorithm:    String,
    pub kdf_version:  String,
    pub memory:       u32,
    pub iterations:   u32,
    pub parallelism:  u32,
    /// Base64-encoded salt.
    pub salt:         String,
    pub salt_length:  usize,
}

impl KDFParameters {
    pub fn new() -> Self {
        KDFParameters {
            algorithm:   "argon2id".to_string(),
            kdf_version: "1.3".to_string(),
            memory:      65536,
            iterations:  3,
            parallelism: 4,
            salt:        String::new(),
            salt_length: 32,
        }
    }
}

impl Default for KDFParameters {
    fn default() -> Self { Self::new() }
}

/// Compression algorithm and size statistics.
#[derive(Debug, Clone)]
pub struct CompressionKeyData {
    pub algorithm:         String,
    pub compression_level: Option<String>,
    pub original_size:     usize,
    pub compressed_size:   usize,
}

impl CompressionKeyData {
    pub fn new(algorithm: String) -> Self {
        CompressionKeyData {
            algorithm,
            compression_level: None,
            original_size:     0,
            compressed_size:   0,
        }
    }
}

/// Integrity checksums recorded during the pipeline.
#[derive(Debug, Clone)]
pub struct ValidationData {
    pub original_checksum:   String,
    pub compressed_checksum: String,
    pub encrypted_checksum:  String,
    pub checksum_algorithm:  String,
}

impl ValidationData {
    pub fn new() -> Self {
        ValidationData {
            original_checksum:   String::new(),
            compressed_checksum: String::new(),
            encrypted_checksum:  String::new(),
            checksum_algorithm:  "sha256".to_string(),
        }
    }
}

impl Default for ValidationData {
    fn default() -> Self { Self::new() }
}

/// @KEY_FILE_INFO section — size accounting.
#[derive(Debug, Clone)]
pub struct FileInfoSection {
    pub original_size:   usize,
    pub compressed_size: usize,
    pub encrypted_size:  usize,
    pub created:         DateTime<Utc>,
    pub source_file:     Option<String>,
    pub output_file:     Option<String>,
}

impl FileInfoSection {
    pub fn new() -> Self {
        FileInfoSection {
            original_size:   0,
            compressed_size: 0,
            encrypted_size:  0,
            created:         Utc::now(),
            source_file:     None,
            output_file:     None,
        }
    }
}

impl Default for FileInfoSection {
    fn default() -> Self { Self::new() }
}

/// Builder for `KeyFileData`.
pub struct KeyFileDataBuilder {
    data: KeyFileData,
}

impl KeyFileDataBuilder {
    pub fn new() -> Self {
        KeyFileDataBuilder { data: KeyFileData::new() }
    }

    pub fn with_source_file(mut self, source_file: String) -> Self {
        self.data.config.source_file        = Some(source_file.clone());
        self.data.file_info.source_file     = Some(source_file);
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
        duration_ms: f64,
    ) -> Self {
        self.data.pipeline.execution_steps.push(
            PipelineExecutionStep::new(step, module, input_size, output_size, duration_ms),
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

    pub fn with_file_sizes(
        mut self,
        original: usize,
        compressed: usize,
        encrypted: usize,
    ) -> Self {
        self.data.file_info.original_size   = original;
        self.data.file_info.compressed_size = compressed;
        self.data.file_info.encrypted_size  = encrypted;
        self
    }

    pub fn build(mut self) -> KeyFileData {
        self.data.pipeline.reversal_order = self.data.pipeline.modules_used
            .iter()
            .rev()
            .cloned()
            .collect();
        self.data
    }
}

impl Default for KeyFileDataBuilder {
    fn default() -> Self { Self::new() }
}
