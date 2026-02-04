//! Main orchestrator for DLM pipeline execution during compilation
//! Executes modules in priority order: Auditor → Compressor → Encryptor

use crate::Compiler::AST::{DixScript, DLMModuleType, DLMModuleSubtype};
use crate::Compiler::DLM::{
    Auditor::{IAuditor, DiyAuditor, EnhancedAuditor},
    Compressor::{ICompressor, GzipCompressor, Bzip2Compressor, LzmaCompressor},
    Encryptor::{IEncryptor, XorEncryptor, Aes128Encryptor, Aes256Encryptor, Chacha20Encryptor},
    KeyManagement::KeyFileManager,
    dlm_module_base::DebugConfig,
    dlm_pipeline_result::DLMPipelineResult,
};
use crate::Compiler::Utilities::SecurityUtilities;
use crate::ErrorManager::{ErrorManager, DlmErrorType, DLMPipelineException};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::fs;

/// Main DLM pipeline executor for compilation
pub struct DLMPipelineExecutor {
    error_manager: ErrorManager,
    debug_config: DebugConfig,
    source_file_path: PathBuf,
    output_directory: PathBuf,
}

impl DLMPipelineExecutor {
    /// Create new DLM pipeline executor
    pub fn new(
        source_file_path: impl AsRef<Path>,
        output_directory: impl AsRef<Path>,
        debug_mode: crate::Compiler::Core::Config::DebugMode,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let debug_config = DebugConfig::from_debug_mode(debug_mode);
        
        DLMPipelineExecutor {
            error_manager,
            debug_config,
            source_file_path: source_file_path.as_ref().to_path_buf(),
            output_directory: output_directory.as_ref().to_path_buf(),
        }
    }
    
    /// Execute the DLM pipeline
    pub fn execute(&self, ast: &mut DixScript, binary_data: Vec<u8>) -> DLMPipelineResult {
        let mut result = DLMPipelineResult::new(binary_data.len());
        let start_time = Instant::now();
        
        self.error_manager.log_info("=== DLM PIPELINE EXECUTION STARTED ===");
        
        // Check if DLM section exists
        if ast.dlm.is_none() || ast.dlm.as_ref().unwrap().modules.is_empty() {
            self.error_manager.log_info("No DLM modules specified - skipping pipeline");
            result.is_success = true;
            result.processed_data = binary_data;
            result.processed_size = binary_data.len();
            result.total_duration = start_time.elapsed();
            return result;
        }
        
        // Parse DLM section and create modules
        let (auditor, compressor, encryptor) = match self.parse_dlm_section(ast) {
            Ok(modules) => modules,
            Err(e) => {
                result.errors.push(e);
                result.total_duration = start_time.elapsed();
                return result;
            }
        };
        
        let mut processed_data = binary_data;
        
        // Execute Auditor (if present)
        if let Some(mut auditor) = auditor {
            match self.execute_auditor(&mut *auditor, ast, &processed_data, &mut result) {
                Ok(_) => {},
                Err(e) => {
                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            }
        }
        
        // Execute Compressor (if present)
        if let Some(compressor) = compressor {
            match self.execute_compressor(&*compressor, &processed_data, &mut result) {
                Ok(compressed) => {
                    processed_data = compressed;
                },
                Err(e) => {
                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            }
        }
        
        // Execute Encryptor (if present)
        if let Some(encryptor) = encryptor {
            match self.execute_encryptor(&*encryptor, &processed_data, &mut result) {
                Ok(encrypted) => {
                    processed_data = encrypted;
                },
                Err(e) => {
                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            }
        }
        
        result.processed_data = processed_data.clone();
        result.processed_size = processed_data.len();
        result.compression_ratio = 1.0 - (result.processed_size as f64 / result.original_size as f64);
        
        // Finalize audit (if present)
        if let Some(auditor) = auditor {
            auditor.finalize_audit();
        }
        
        // Generate output files
        if let Err(e) = self.generate_output_files(&mut result, ast) {
            result.warnings.push(format!("Failed to generate output files: {}", e));
        }
        
        result.is_success = true;
        result.total_duration = start_time.elapsed();
        
        self.error_manager.log_info("=== DLM PIPELINE EXECUTION COMPLETE ===");
        
        result
    }
    
    /// Parse DLM section and instantiate modules
    fn parse_dlm_section(
        &self,
        ast: &mut DixScript,
    ) -> Result<(Option<Box<dyn IAuditor>>, Option<Box<dyn ICompressor>>, Option<Box<dyn IEncryptor>>), String> {
        let dlm = ast.dlm.as_ref().unwrap();
        
        self.error_manager.log_info(&format!("Parsing {} DLM module(s)", dlm.modules.len()));
        
        let mut auditor: Option<Box<dyn IAuditor>> = None;
        let mut compressor: Option<Box<dyn ICompressor>> = None;
        let mut encryptor: Option<Box<dyn IEncryptor>> = None;
        
        for module in &dlm.modules {
            match module.module_type {
                DLMModuleType::DAuditor => {
                    auditor = Some(self.create_auditor(module.subtype, ast)?);
                },
                DLMModuleType::DCompressor => {
                    compressor = Some(self.create_compressor(module.subtype)?);
                },
                DLMModuleType::DEncryptor => {
                    encryptor = Some(self.create_encryptor(module.subtype, ast)?);
                },
            }
        }
        
        Ok((auditor, compressor, encryptor))
    }
    
    /// Create auditor module
    fn create_auditor(
        &self,
        subtype: Option<DLMModuleSubtype>,
        ast: &DixScript,
    ) -> Result<Box<dyn IAuditor>, String> {
        let auditor: Box<dyn IAuditor> = match subtype {
            Some(DLMModuleSubtype::Diy) | None => {
                Box::new(DiyAuditor::new(
                    &self.source_file_path,
                    &self.output_directory,
                    self.debug_config.into(),
                ))
            },
            Some(DLMModuleSubtype::Enhanced) => {
                Box::new(EnhancedAuditor::new(
                    &self.source_file_path,
                    &self.output_directory,
                    ast.clone(),
                    self.debug_config.into(),
                ))
            },
            _ => return Err(format!("Unknown auditor subtype: {:?}", subtype)),
        };
        
        Ok(auditor)
    }
    
    /// Create compressor module
    fn create_compressor(
        &self,
        subtype: Option<DLMModuleSubtype>,
    ) -> Result<Box<dyn ICompressor>, String> {
        let compressor: Box<dyn ICompressor> = match subtype {
            Some(DLMModuleSubtype::Gzip) | None => {
                Box::new(GzipCompressor::new(self.debug_config.into()))
            },
            Some(DLMModuleSubtype::Bzip2) => {
                Box::new(Bzip2Compressor::new(self.debug_config.into()))
            },
            Some(DLMModuleSubtype::Lzma) => {
                Box::new(LzmaCompressor::new(self.debug_config.into()))
            },
            _ => return Err(format!("Unknown compressor subtype: {:?}", subtype)),
        };
        
        Ok(compressor)
    }
    
    /// Create encryptor module
    fn create_encryptor(
        &self,
        subtype: Option<DLMModuleSubtype>,
        ast: &mut DixScript,
    ) -> Result<Box<dyn IEncryptor>, String> {
        // Ensure valid security section
        ast.security = Some(SecurityUtilities::ensure_valid_security_section(
            ast.security.take(),
            ast.dlm.as_ref(),
        ));
        
        // Validate security section
        if let Err(errors) = SecurityUtilities::is_valid_for_encryption(ast.security.as_ref().unwrap()) {
            return Err(format!("SECURITY section validation failed: {:?}", errors));
        }
        
        self.error_manager.log_info("SECURITY section validated and ready for encryption");
        
        let security = ast.security.as_ref().unwrap();
        
        let encryptor: Box<dyn IEncryptor> = match subtype {
            Some(DLMModuleSubtype::Xor) => {
                Box::new(XorEncryptor::new(security.clone(), self.debug_config.into()))
            },
            Some(DLMModuleSubtype::Aes128) => {
                Box::new(Aes128Encryptor::new(security.clone(), self.debug_config.into()))
            },
            Some(DLMModuleSubtype::Aes256) | None => {
                Box::new(Aes256Encryptor::new(security.clone(), self.debug_config.into()))
            },
            Some(DLMModuleSubtype::Chacha20) => {
                Box::new(Chacha20Encryptor::new(security.clone(), self.debug_config.into()))
            },
            _ => return Err(format!("Unknown encryptor subtype: {:?}", subtype)),
        };
        
        Ok(encryptor)
    }
    
    /// Execute auditor module
    #[inline]
    fn execute_auditor(
        &self,
        auditor: &mut dyn IAuditor,
        ast: &DixScript,
        data: &[u8],
        result: &mut DLMPipelineResult,
    ) -> Result<(), String> {
        self.error_manager.log_info("Executing Auditor module...");
        
        let audit_result = auditor.start_audit(ast, data)?;
        
        result.executed_modules.push(auditor.module_name().to_string());
        result.metadata.insert("auditor".to_string(), auditor.get_metadata());
        result.audit_file_path = Some(audit_result.audit_file_path);
        
        self.error_manager.log_info(&format!("Auditor started: {}", audit_result.audit_file_path));
        
        Ok(())
    }
    
    /// Execute compressor module
    #[inline]
    fn execute_compressor(
        &self,
        compressor: &dyn ICompressor,
        data: &[u8],
        result: &mut DLMPipelineResult,
    ) -> Result<Vec<u8>, String> {
        self.error_manager.log_info("Executing Compressor module...");
        
        let start = Instant::now();
        let compressed = compressor.compress(data)?;
        let duration = start.elapsed();
        
        result.executed_modules.push(compressor.module_name().to_string());
        result.metadata.insert("compressor".to_string(), compressor.get_metadata());
        
        let ratio = 1.0 - (compressed.len() as f64 / data.len() as f64);
        self.error_manager.log_info(&format!(
            "Compression complete: {} -> {} bytes ({:.1}% reduction)",
            data.len(),
            compressed.len(),
            ratio * 100.0
        ));
        
        Ok(compressed)
    }
    
    /// Execute encryptor module
    #[inline]
    fn execute_encryptor(
        &self,
        encryptor: &dyn IEncryptor,
        data: &[u8],
        result: &mut DLMPipelineResult,
    ) -> Result<Vec<u8>, String> {
        self.error_manager.log_info("Executing Encryptor module...");
        
        let start = Instant::now();
        let encrypted = encryptor.encrypt(data)?;
        let duration = start.elapsed();
        
        result.executed_modules.push(encryptor.module_name().to_string());
        result.metadata.insert("encryptor".to_string(), encryptor.get_metadata());
        
        self.error_manager.log_info(&format!(
            "Encryption complete: {} -> {} bytes",
            data.len(),
            encrypted.len()
        ));
        
        Ok(encrypted)
    }
    
    /// Generate output files (.mdix.enc, .mdix.key)
    fn generate_output_files(
        &self,
        result: &mut DLMPipelineResult,
        ast: &DixScript,
    ) -> Result<(), String> {
        let base_name = self.source_file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid source file name")?;
        
        // Generate .mdix.enc file (if encrypted or compressed)
        if !result.metadata.is_empty() {
            let enc_path = self.output_directory.join(format!("{}.mdix.enc", base_name));
            fs::write(&enc_path, &result.processed_data)
                .map_err(|e| format!("Failed to write encrypted file: {}", e))?;
            
            result.encrypted_file_path = Some(enc_path.to_string_lossy().to_string());
            self.error_manager.log_info(&format!("Generated: {}", enc_path.display()));
        }
        
        // Generate .mdix.key file (if encrypted or compressed)
        if !result.metadata.is_empty() {
            let key_path = self.output_directory.join(format!("{}.mdix.key", base_name));
            let key_manager = KeyFileManager::new(self.debug_config.into());
            key_manager.generate_key_file(&key_path, &result.metadata, ast)?;
            
            result.key_file_path = Some(key_path.to_string_lossy().to_string());
            self.error_manager.log_info(&format!("Generated: {}", key_path.display()));
        }
        
        Ok(())
    }
                  }
