// src/Compiler/DLM/dlm_reverse_executor.rs
//! Reverse orchestrator for DLM pipeline during loading
//! Executes modules in reverse order: Decryptor → Decompressor

use crate::Compiler::DLM::{
    Auditor::{IAuditor, DiyAuditor},
    Compressor::{ICompressor, GzipCompressor, Bzip2Compressor, LzmaCompressor},
    Encryptor::{IEncryptor, XorEncryptor, Aes128Encryptor, Aes256Encryptor, Chacha20Encryptor},
    KeyManagement::{KeyFileManager, KeyFileMetadata},
    dlm_pipeline_result::DLMReverseResult,
};
use crate::ErrorManager::{ErrorManager, DebugConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::fs;

/// DLM reverse pipeline executor for decryption/decompression
pub struct DLMReverseExecutor {
    error_manager: ErrorManager,
    debug_config: DebugConfig,
    encrypted_file_path: PathBuf,
    key_file_path: PathBuf,
    password: Option<String>,
}

impl DLMReverseExecutor {
    /// Create new DLM reverse executor
    pub fn new(
        encrypted_file_path: impl AsRef<Path>,
        key_file_path: impl AsRef<Path>,
        password: Option<String>,
        debug_mode: crate::Compiler::Core::Config::DebugMode,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let debug_config = DebugConfig::from_debug_mode(debug_mode);

        DLMReverseExecutor {
            error_manager,
            debug_config,
            encrypted_file_path: encrypted_file_path.as_ref().to_path_buf(),
            key_file_path: key_file_path.as_ref().to_path_buf(),
            password,
        }
    }

    /// Execute the reverse pipeline
    pub fn execute(&self) -> DLMReverseResult {
        let start_time = Instant::now();

        self.error_manager
            .log_info("=== DLM REVERSE PIPELINE STARTED ===");

        // Read encrypted file
        let encrypted_data = match fs::read(&self.encrypted_file_path) {
            Ok(data) => data,
            Err(e) => {
                let mut result = DLMReverseResult::new(0);
                result
                    .errors
                    .push(format!("Failed to read encrypted file: {}", e));
                result.total_duration = start_time.elapsed();
                return result;
            }
        };

        let mut result = DLMReverseResult::new(encrypted_data.len());

        self.error_manager.log_info(&format!(
            "Read encrypted file: {} bytes",
            encrypted_data.len()
        ));

        // Parse key file
        let pipeline_metadata = match self.parse_key_file() {
            Ok(meta) => meta,
            Err(e) => {
                result.errors.push(e);
                result.total_duration = start_time.elapsed();
                return result;
            }
        };

        // Instantiate modules
        let (mut encryptor, compressor, mut auditor) =
            match self.instantiate_modules(&pipeline_metadata) {
                Ok(modules) => modules,
                Err(e) => {
                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            };

        let mut processed_data = encrypted_data;

        // Execute Decryptor (if present)
        if let Some(ref mut enc) = encryptor {
            // Set password if provided
            if let Some(ref password) = self.password {
                if let Err(e) = enc.set_password(password) {
                    result
                        .errors
                        .push(format!("Failed to set password: {}", e));
                    result.total_duration = start_time.elapsed();

                    // Log failed decryption attempt
                    if let Some(ref mut aud) = auditor {
                        aud.log_decryption_attempt(
                            false,
                            &format!("Password setup failed: {}", e),
                            result.encrypted_size,
                            0,
                            start_time.elapsed().as_millis() as f64,
                        );
                        let _ = aud.finalize_audit();
                    }

                    return result;
                }
            }

            match self.execute_decryption(enc.as_ref(), &processed_data, &mut result) {
                Ok(decrypted) => {
                    // Log successful decryption
                    if let Some(ref mut aud) = auditor {
                        aud.log_decryption_attempt(
                            true,
                            &format!("Decryption successful using {}", enc.algorithm()),
                            result.encrypted_size,
                            decrypted.len(),
                            start_time.elapsed().as_millis() as f64,
                        );
                    }
                    processed_data = decrypted;
                }
                Err(e) => {
                    // Log failed decryption attempt
                    if let Some(ref mut aud) = auditor {
                        aud.log_decryption_attempt(
                            false,
                            &format!("Decryption failed: {}", e),
                            result.encrypted_size,
                            0,
                            start_time.elapsed().as_millis() as f64,
                        );
                        let _ = aud.finalize_audit();
                    }

                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            }
        }

        // Execute Decompressor (if present)
        if let Some(ref comp) = compressor {
            match self.execute_decompression(comp.as_ref(), &processed_data, &mut result) {
                Ok(decompressed) => {
                    processed_data = decompressed;
                }
                Err(e) => {
                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            }
        }

        // Finalize auditor
        if let Some(ref mut aud) = auditor {
            if let Err(e) = aud.finalize_audit() {
                result
                    .warnings
                    .push(format!("Audit finalization warning: {}", e));
            }
        }

        result.restored_data = processed_data;
        result.restored_size = result.restored_data.len();
        result.is_success = true;
        result.total_duration = start_time.elapsed();

        self.error_manager
            .log_info("=== DLM REVERSE PIPELINE COMPLETE ===");

        result
    }

    /// Parse .mdix.key file — returns typed KeyFileMetadata
    fn parse_key_file(&self) -> Result<KeyFileMetadata, String> {
        self.error_manager.log_info("Parsing .mdix.key file...");

        // KeyFileManager needs source and output paths; use enclosing directory for both
        let dir_str = self
            .encrypted_file_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_string_lossy()
            .to_string();

        let key_manager = KeyFileManager::new(dir_str.clone(), dir_str);
        let key_path_str = self.key_file_path.to_string_lossy().to_string();
        let metadata = key_manager.read_key_file(&key_path_str)?;

        self.error_manager.log_info("Key file parsed successfully");

        Ok(metadata)
    }

    /// Instantiate modules from typed KeyFileMetadata
    fn instantiate_modules(
        &self,
        metadata: &KeyFileMetadata,
    ) -> Result
    (
        Option<Box<dyn IEncryptor>>,
        Option<Box<dyn ICompressor>>,
        Option<Box<dyn IAuditor>>,
    ),
    String,
    > {
    let dir_str = self
    .encrypted_file_path
    .parent()
    .unwrap_or_else(|| Path::new("."))
    .to_string_lossy()
    .to_string();

    let key_manager = KeyFileManager::new(dir_str.clone(), dir_str);

    let mut encryptor: Option<Box<dyn IEncryptor>> = None;
    let mut compressor: Option<Box<dyn ICompressor>> = None;
    let mut auditor: Option<Box<dyn IAuditor>> = None;

    // Create decryptor from encryption metadata
    if let Some(ref enc_meta) = metadata.encryption {
    let config = key_manager
    .extract_encryption_config(metadata)
    .unwrap_or_default();

    encryptor = Some(self.create_decryptor(&enc_meta.algorithm, &config)?);
    self.error_manager
    .log_info(&format!("Decryptor initialized: {}", enc_meta.algorithm));
    }

    // Create decompressor from compression metadata
    if let Some(ref comp_meta) = metadata.compression {
    compressor = Some(self.create_decompressor(&comp_meta.algorithm)?);
    self.error_manager
    .log_info(&format!("Decompressor initialized: {}", comp_meta.algorithm));
    }

    // Create auditor for logging decryption attempts (if audit data was present)
    if metadata.audit.is_some() {
    let source_file = self.determine_source_file_path();
    let output_dir = self
    .encrypted_file_path
    .parent()
    .unwrap_or_else(|| Path::new("."));

    auditor = Some(Box::new(DiyAuditor::new(&source_file, output_dir)));
    self.error_manager
    .log_info("Auditor initialized for decryption logging");
    }

    Ok((encryptor, compressor, auditor))
    }

    /// Create decryptor: construct with no security config, then initialize from metadata
    fn create_decryptor(
        &self,
        algorithm: &str,
        config: &HashMap<String, String>,
    ) -> Result<Box<dyn IEncryptor>, String> {
        let mut enc: Box<dyn IEncryptor> = match algorithm.to_lowercase().as_str() {
            "xor" => Box::new(XorEncryptor::new(None)),
            "aes128-gcm" | "aes128" => Box::new(Aes128Encryptor::new(None)),
            "aes256-gcm" | "aes256" => Box::new(Aes256Encryptor::new(None)),
            "chacha20-poly1305" | "chacha20" => Box::new(Chacha20Encryptor::new(None)),
            _ => return Err(format!("Unknown encryption algorithm: {}", algorithm)),
        };

        // Load key material from metadata via initialize
        enc.initialize(config.clone());

        Ok(enc)
    }

    /// Create decompressor from algorithm string
    fn create_decompressor(&self, algorithm: &str) -> Result<Box<dyn ICompressor>, String> {
        let compressor: Box<dyn ICompressor> = match algorithm.to_lowercase().as_str() {
            "gzip" => Box::new(GzipCompressor::new()),
            "bzip2" => Box::new(Bzip2Compressor::new()),
            "lzma" => Box::new(LzmaCompressor::new()),
            _ => return Err(format!("Unknown compression algorithm: {}", algorithm)),
        };

        Ok(compressor)
    }

    /// Execute decryption
    #[inline]
    fn execute_decryption(
        &self,
        encryptor: &dyn IEncryptor,
        data: &[u8],
        result: &mut DLMReverseResult,
    ) -> Result<Vec<u8>, String> {
        self.error_manager.log_info("Executing Decryption...");

        let start = Instant::now();
        let decrypted = encryptor.decrypt(data)?;
        let _duration = start.elapsed();

        result.executed_modules.push("Decryptor".to_string());

        self.error_manager.log_info(&format!(
            "Decryption complete: {} -> {} bytes",
            data.len(),
            decrypted.len()
        ));

        Ok(decrypted)
    }

    /// Execute decompression
    #[inline]
    fn execute_decompression(
        &self,
        compressor: &dyn ICompressor,
        data: &[u8],
        result: &mut DLMReverseResult,
    ) -> Result<Vec<u8>, String> {
        self.error_manager.log_info("Executing Decompression...");

        let start = Instant::now();
        let decompressed = compressor.decompress(data)?;
        let _duration = start.elapsed();

        result.executed_modules.push("Decompressor".to_string());

        self.error_manager.log_info(&format!(
            "Decompression complete: {} -> {} bytes",
            data.len(),
            decompressed.len()
        ));

        Ok(decompressed)
    }

    /// Determine source file path from encrypted file path
    fn determine_source_file_path(&self) -> PathBuf {
        let enc_dir = self
            .encrypted_file_path
            .parent()
            .unwrap_or_else(|| Path::new("."));

        let mut base_name = self
            .encrypted_file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Strip extensions: .enc → .mdix → base
        if base_name.ends_with(".enc") {
            base_name = base_name[..base_name.len() - 4].to_string();
        }
        if base_name.ends_with(".mdix") {
            base_name = base_name[..base_name.len() - 5].to_string();
        }

        let expected_source = enc_dir.join(format!("{}.mdix", base_name));

        if self.debug_config.is_enabled {
            if expected_source.exists() {
                self.error_manager.log_debug(&format!(
                    "Found source file: {}",
                    expected_source.display()
                ));
            } else {
                self.error_manager.log_debug(&format!(
                    "Source file not found, using derived path: {}",
                    expected_source.display()
                ));
            }
        }

        expected_source
    }
}