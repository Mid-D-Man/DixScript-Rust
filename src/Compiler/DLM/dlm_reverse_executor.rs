//! Reverse orchestrator for DLM pipeline during loading
//! Executes modules in reverse order: Decryptor → Decompressor

use crate::Compiler::DLM::{
    Auditor::{IAuditor, DiyAuditor},
    Compressor::{ICompressor, GzipCompressor, Bzip2Compressor, LzmaCompressor},
    Encryptor::{IEncryptor, XorEncryptor, Aes128Encryptor, Aes256Encryptor, Chacha20Encryptor},
    KeyManagement::KeyFileManager,
    dlm_module_base::DebugConfig,
    dlm_pipeline_result::DLMReverseResult,
};
use crate::ErrorManager::{ErrorManager, DlmErrorType};
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
        
        self.error_manager.log_info("=== DLM REVERSE PIPELINE STARTED ===");
        
        // Read encrypted file
        let encrypted_data = match fs::read(&self.encrypted_file_path) {
            Ok(data) => data,
            Err(e) => {
                let mut result = DLMReverseResult::new(0);
                result.errors.push(format!("Failed to read encrypted file: {}", e));
                result.total_duration = start_time.elapsed();
                return result;
            }
        };
        
        let mut result = DLMReverseResult::new(encrypted_data.len());
        
        self.error_manager.log_info(&format!("Read encrypted file: {} bytes", encrypted_data.len()));
        
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
        let (encryptor, compressor, auditor) = match self.instantiate_modules(&pipeline_metadata) {
            Ok(modules) => modules,
            Err(e) => {
                result.errors.push(e);
                result.total_duration = start_time.elapsed();
                return result;
            }
        };
        
        let mut processed_data = encrypted_data;
        
        // Execute Decryptor (if present)
        if let Some(mut encryptor) = encryptor {
            // Set password if provided
            if let Some(ref password) = self.password {
                if let Err(e) = encryptor.set_password(password) {
                    result.errors.push(format!("Failed to set password: {}", e));
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            }
            
            match self.execute_decryption(&*encryptor, &processed_data, &mut result) {
                Ok(decrypted) => {
                    processed_data = decrypted;
                },
                Err(e) => {
                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    
                    // Log failed decryption attempt
                    if let Some(mut auditor) = auditor {
                        auditor.log_decryption_attempt(
                            false,
                            &format!("Decryption failed: {}", result.errors.last().unwrap()),
                            result.encrypted_size,
                            0,
                            start_time.elapsed().as_millis() as f64,
                        );
                        auditor.finalize_audit();
                    }
                    
                    return result;
                }
            }
        }
        
        // Execute Decompressor (if present)
        if let Some(compressor) = compressor {
            match self.execute_decompression(&*compressor, &processed_data, &mut result) {
                Ok(decompressed) => {
                    processed_data = decompressed;
                },
                Err(e) => {
                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            }
        }
        
        result.restored_data = processed_data;
        result.restored_size = result.restored_data.len();
        result.is_success = true;
        result.total_duration = start_time.elapsed();
        
        self.error_manager.log_info("=== DLM REVERSE PIPELINE COMPLETE ===");
        
        result
    }
    
    /// Parse .mdix.key file
    fn parse_key_file(&self) -> Result<HashMap<String, HashMap<String, String>>, String> {
        self.error_manager.log_info("Parsing .mdix.key file...");
        
        let key_manager = KeyFileManager::new(self.debug_config.into());
        let metadata = key_manager.read_key_file(&self.key_file_path)?;
        
        self.error_manager.log_info("Key file parsed successfully");
        
        Ok(metadata)
    }
    
    /// Instantiate modules from key file metadata
    fn instantiate_modules(
        &self,
        metadata: &HashMap<String, HashMap<String, String>>,
    ) -> Result<(Option<Box<dyn IEncryptor>>, Option<Box<dyn ICompressor>>, Option<Box<dyn IAuditor>>), String> {
        let mut encryptor: Option<Box<dyn IEncryptor>> = None;
        let mut compressor: Option<Box<dyn ICompressor>> = None;
        let mut auditor: Option<Box<dyn IAuditor>> = None;
        
        // Create decryptor
        if let Some(enc_meta) = metadata.get("encryptor") {
            if let Some(algorithm) = enc_meta.get("algorithm") {
                encryptor = Some(self.create_decryptor(algorithm, enc_meta)?);
                self.error_manager.log_info(&format!("Decryptor initialized: {}", algorithm));
            }
        }
        
        // Create decompressor
        if let Some(comp_meta) = metadata.get("compressor") {
            if let Some(algorithm) = comp_meta.get("algorithm") {
                compressor = Some(self.create_decompressor(algorithm)?);
                self.error_manager.log_info(&format!("Decompressor initialized: {}", algorithm));
            }
        }
        
        // Create auditor (for logging decryption attempts)
        if let Some(aud_meta) = metadata.get("auditor") {
            let source_file = self.determine_source_file_path();
            let output_dir = self.encrypted_file_path.parent()
                .unwrap_or_else(|| Path::new("."));
            
            auditor = Some(Box::new(DiyAuditor::new(
                &source_file,
                output_dir,
                self.debug_config.into(),
            )));
            
            self.error_manager.log_info("Auditor initialized for decryption logging");
        }
        
        Ok((encryptor, compressor, auditor))
    }
    
    /// Create decryptor from algorithm string
    fn create_decryptor(
        &self,
        algorithm: &str,
        metadata: &HashMap<String, String>,
    ) -> Result<Box<dyn IEncryptor>, String> {
        let encryptor: Box<dyn IEncryptor> = match algorithm.to_lowercase().as_str() {
            "xor" => Box::new(XorEncryptor::new_with_metadata(metadata.clone(), self.debug_config.into())),
            "aes128-gcm" | "aes128" => Box::new(Aes128Encryptor::new_with_metadata(metadata.clone(), self.debug_config.into())),
            "aes256-gcm" | "aes256" => Box::new(Aes256Encryptor::new_with_metadata(metadata.clone(), self.debug_config.into())),
            "chacha20-poly1305" | "chacha20" => Box::new(Chacha20Encryptor::new_with_metadata(metadata.clone(), self.debug_config.into())),
            _ => return Err(format!("Unknown encryption algorithm: {}", algorithm)),
        };
        
        Ok(encryptor)
    }
    
    /// Create decompressor from algorithm string
    fn create_decompressor(&self, algorithm: &str) -> Result<Box<dyn ICompressor>, String> {
        let compressor: Box<dyn ICompressor> = match algorithm.to_lowercase().as_str() {
            "gzip" => Box::new(GzipCompressor::new(self.debug_config.into())),
            "bzip2" => Box::new(Bzip2Compressor::new(self.debug_config.into())),
            "lzma" => Box::new(LzmaCompressor::new(self.debug_config.into())),
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
        let duration = start.elapsed();
        
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
        let duration = start.elapsed();
        
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
        let enc_dir = self.encrypted_file_path.parent()
            .unwrap_or_else(|| Path::new("."));
        
        let mut base_name = self.encrypted_file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        
        // Remove .enc extension
        if base_name.ends_with(".enc") {
            base_name = base_name[..base_name.len() - 4].to_string();
        }
        
        // Remove .mdix extension
        if base_name.ends_with(".mdix") {
            base_name = base_name[..base_name.len() - 5].to_string();
        }
        
        let expected_source = enc_dir.join(format!("{}.mdix", base_name));
        
        if expected_source.exists() {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!("Found source file: {}", expected_source.display()));
            }
            expected_source
        } else {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!("Source file not found, using derived path: {}", expected_source.display()));
            }
            expected_source
        }
    }
              }
