//! Reverse orchestrator for DLM pipeline during loading.
//!
//! Execution order: Decryptor → Decompressor.

use crate::Compiler::DLM::{
    Auditor::{IAuditor, DiyAuditor},
    Compressor::{ICompressor, GzipCompressor},
    Encryptor::{IEncryptor, XorEncryptor, Aes128Encryptor, Aes256Encryptor, Chacha20Encryptor},
    KeyManagement::{KeyFileManager, KeyFileData},
    dlm_pipeline_result::DLMReverseResult,
};
use crate::ErrorManager::{ErrorManager, DebugConfig, DlmErrorType, ErrorSeverity};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::fs;

#[cfg(not(target_arch = "wasm32"))]
use crate::Compiler::DLM::Compressor::{Bzip2Compressor, LzmaCompressor};

pub struct DLMReverseExecutor {
    error_manager:       ErrorManager,
    debug_config:        DebugConfig,
    encrypted_file_path: PathBuf,
    key_file_path:       PathBuf,
    password:            Option<String>,
}

impl DLMReverseExecutor {
    pub fn new(
        encrypted_file_path: impl AsRef<Path>,
        key_file_path: impl AsRef<Path>,
        password: Option<String>,
        debug_mode: crate::Compiler::Core::Config::DebugMode,
    ) -> Self {
        Self::new_with_error_manager(encrypted_file_path,key_file_path,password,debug_mode,ErrorManager::get_shared_instance())
    }
    pub fn new_with_error_manager(
        encrypted_file_path: impl AsRef<Path>,
        key_file_path: impl AsRef<Path>,
        password: Option<String>,
        debug_mode: crate::Compiler::Core::Config::DebugMode,
        error_manager: ErrorManager
    ) -> Self {

        let debug_config  = DebugConfig::from_debug_mode(debug_mode);

        DLMReverseExecutor {
            error_manager,
            debug_config,
            encrypted_file_path: encrypted_file_path.as_ref().to_path_buf(),
            key_file_path:       key_file_path.as_ref().to_path_buf(),
            password,
        }
    }
    // ── Main entry point ──────────────────────────────────────────────────────

    pub fn execute(&self) -> DLMReverseResult {
        let start_time = Instant::now();

        self.error_manager.log_info("DLM reverse pipeline started");

        let encrypted_data = match fs::read(&self.encrypted_file_path) {
            Ok(data) => data,
            Err(e) => {
                let msg = format!("Failed to read encrypted file: {}", e);
                self.error_manager.add_dlm_error(
                    DlmErrorType::ModuleExecutionFailed,
                    msg.clone(),
                    Some(self.file_label()),
                    None,
                    Some("Ensure the .mdix.enc file exists".to_string()),
                    ErrorSeverity::Fatal,
                );
                let mut result = DLMReverseResult::new(0);
                result.errors.push(msg);
                result.total_duration = start_time.elapsed();
                return result;
            }
        };

        let mut result = DLMReverseResult::new(encrypted_data.len());

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[DLMReverseExecutor] Read {} bytes from encrypted file",
                encrypted_data.len(),
            ));
        }

        let key_data = match self.load_key_file() {
            Ok(kd) => kd,
            Err(e) => {
                result.errors.push(e);
                result.total_duration = start_time.elapsed();
                return result;
            }
        };

        let (mut encryptor, compressor, mut auditor) =
            match self.instantiate_modules(&key_data) {
                Ok(modules) => modules,
                Err(e) => {
                    self.error_manager.add_dlm_error(
                        DlmErrorType::ModuleExecutionFailed,
                        e.clone(),
                        Some(self.file_label()),
                        None,
                        None,
                        ErrorSeverity::Fatal,
                    );
                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            };

        let mut processed_data = encrypted_data;

        // Phase 1: decrypt
        if let Some(ref mut enc) = encryptor {
            if let Some(ref password) = self.password {
                if let Err(e) = enc.set_password(password) {
                    self.error_manager.add_dlm_error(
                        DlmErrorType::ModuleExecutionFailed,
                        e.clone(),
                        Some(self.file_label()),
                        Some(enc.module_name().to_string()),
                        Some("Check the password is correct".to_string()),
                        ErrorSeverity::Fatal,
                    );
                    if let Some(ref mut aud) = auditor {
                        aud.log_decryption_attempt(
                            false,
                            &format!("Password setup failed: {}", e),
                            result.encrypted_size,
                            0,
                            start_time.elapsed().as_secs_f64() * 1000.0,
                        );
                        let _ = aud.finalize_audit();
                    }
                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            }

            let phase_start = Instant::now();
            match enc.decrypt(&processed_data) {
                Ok(decrypted) => {
                    let duration_ms = phase_start.elapsed().as_secs_f64() * 1000.0;

                    if let Some(ref mut aud) = auditor {
                        aud.log_decryption_attempt(
                            true,
                            &format!("Decrypted with {}", enc.algorithm()),
                            result.encrypted_size,
                            decrypted.len(),
                            duration_ms,
                        );
                    }

                    result.executed_modules.push(enc.module_name().to_string());

                    self.error_manager.log_info(&format!(
                        "Decryption complete: {} -> {} bytes",
                        result.encrypted_size,
                        decrypted.len(),
                    ));

                    processed_data = decrypted;
                }
                Err(e) => {
                    let duration_ms = phase_start.elapsed().as_secs_f64() * 1000.0;

                    self.error_manager.add_dlm_error(
                        DlmErrorType::ModuleExecutionFailed,
                        e.clone(),
                        Some(self.file_label()),
                        Some(enc.module_name().to_string()),
                        Some("Verify the password or key file is correct".to_string()),
                        ErrorSeverity::Fatal,
                    );

                    if let Some(ref mut aud) = auditor {
                        aud.log_decryption_attempt(
                            false,
                            &format!("Decryption failed: {}", e),
                            result.encrypted_size,
                            0,
                            duration_ms,
                        );
                        let _ = aud.finalize_audit();
                    }

                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            }
        }

        // Phase 2: decompress
        if let Some(ref comp) = compressor {
            let pre_size    = processed_data.len();
            let phase_start = Instant::now();
            match comp.decompress(&processed_data) {
                Ok(decompressed) => {
                    let duration_ms = phase_start.elapsed().as_secs_f64() * 1000.0;

                    result.executed_modules.push(comp.module_name().to_string());

                    self.error_manager.log_info(&format!(
                        "Decompression complete: {} -> {} bytes",
                        pre_size,
                        decompressed.len(),
                    ));

                    if let Some(ref mut aud) = auditor {
                        aud.log_step(
                            comp.module_name(),
                            &format!("Decompressed with {}", comp.algorithm()),
                            pre_size,
                            decompressed.len(),
                            duration_ms,
                        );
                    }

                    processed_data = decompressed;
                }
                Err(e) => {
                    self.error_manager.add_dlm_error(
                        DlmErrorType::ModuleExecutionFailed,
                        e.clone(),
                        Some(self.file_label()),
                        Some(comp.module_name().to_string()),
                        Some(
                            "The data may be corrupted or use an algorithm unavailable \
                             on this platform.".to_string()
                        ),
                        ErrorSeverity::Fatal,
                    );
                    if let Some(ref mut aud) = auditor {
                        let _ = aud.finalize_audit();
                    }
                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            }
        }

        // Finalize auditor
        if let Some(ref mut aud) = auditor {
            if let Err(e) = aud.finalize_audit() {
                self.error_manager.add_dlm_error(
                    DlmErrorType::ModuleExecutionFailed,
                    e.clone(),
                    Some(self.file_label()),
                    Some("DAuditor".to_string()),
                    None,
                    ErrorSeverity::Warning,
                );
                result.warnings.push(format!("Audit finalization warning: {}", e));
            }
        }

        result.restored_data  = processed_data;
        result.restored_size  = result.restored_data.len();
        result.is_success     = true;
        result.total_duration = start_time.elapsed();

        self.error_manager.log_info(&format!(
            "DLM reverse pipeline complete: {} modules, {} -> {} bytes, {:.2}ms",
            result.executed_modules.len(),
            result.encrypted_size,
            result.restored_size,
            result.total_duration.as_secs_f64() * 1000.0,
        ));

        result
    }

    // ── Key file loading ──────────────────────────────────────────────────────

    fn load_key_file(&self) -> Result<KeyFileData, String> {
        let dir = self.encrypted_file_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_string_lossy()
            .to_string();

        let key_manager  = KeyFileManager::new(dir.clone(), dir);
        let key_path_str = self.key_file_path.to_string_lossy().to_string();
        let data         = key_manager.read_key_file(&key_path_str)?;

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[DLMReverseExecutor] Key file loaded: mode={}, modules={}",
                data.config.key_type,
                data.pipeline.modules_used.join(","),
            ));
        }

        Ok(data)
    }

    // ── Module instantiation ──────────────────────────────────────────────────

    fn instantiate_modules(
        &self,
        key_data: &KeyFileData,
    ) -> Result<(
        Option<Box<dyn IEncryptor>>,
        Option<Box<dyn ICompressor>>,
        Option<Box<dyn IAuditor>>,
    ), String> {
        let dir = self.encrypted_file_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_string_lossy()
            .to_string();

        let key_manager = KeyFileManager::new(dir.clone(), dir.clone());

        let mut encryptor:  Option<Box<dyn IEncryptor>>  = None;
        let mut compressor: Option<Box<dyn ICompressor>> = None;
        let mut auditor:    Option<Box<dyn IAuditor>>    = None;

        if let Some(ref enc_data) = key_data.key_data.encryption {
            let config = key_manager
                .extract_encryption_config(key_data)
                .unwrap_or_default();
            encryptor = Some(self.create_decryptor(&enc_data.algorithm, &config)?);

            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[DLMReverseExecutor] Decryptor: {}",
                    enc_data.algorithm,
                ));
            }
        }

        if let Some(ref comp_data) = key_data.key_data.compression {
            compressor = Some(self.create_decompressor(&comp_data.algorithm)?);

            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[DLMReverseExecutor] Decompressor: {}",
                    comp_data.algorithm,
                ));
            }
        }

        let had_auditor = key_data.pipeline.modules_used.iter()
            .any(|m| m.to_lowercase().contains("dauditor"));

        if had_auditor {
            let source = self.derive_source_path();
            let output = self.encrypted_file_path
                .parent()
                .unwrap_or_else(|| Path::new("."));
            auditor = Some(Box::new(DiyAuditor::new(&source, output)));

            if self.debug_config.is_enabled {
                self.error_manager.log_debug(
                    "[DLMReverseExecutor] Auditor created for decryption logging",
                );
            }
        }

        Ok((encryptor, compressor, auditor))
    }

    fn create_decryptor(
        &self,
        algorithm: &str,
        config: &HashMap<String, String>,
    ) -> Result<Box<dyn IEncryptor>, String> {
        let mut enc: Box<dyn IEncryptor> = match algorithm.to_lowercase().as_str() {
            "xor"                              => Box::new(XorEncryptor::new(None)),
            "aes128-gcm" | "aes128"            => Box::new(Aes128Encryptor::new(None)),
            "aes256-gcm" | "aes256"            => Box::new(Aes256Encryptor::new(None)),
            "chacha20-poly1305" | "chacha20"   => Box::new(Chacha20Encryptor::new(None)),
            _ => {
                let msg = format!(
                    "Unknown encryption algorithm in key file: '{}'", algorithm
                );
                self.error_manager.add_dlm_error(
                    DlmErrorType::ModuleExecutionFailed,
                    msg.clone(),
                    Some(self.file_label()),
                    None,
                    None,
                    ErrorSeverity::Fatal,
                );
                return Err(msg);
            }
        };

        enc.initialize(config.clone());
        Ok(enc)
    }

    fn create_decompressor(
        &self,
        algorithm: &str,
    ) -> Result<Box<dyn ICompressor>, String> {
        match algorithm.to_lowercase().as_str() {
            "gzip" => Ok(Box::new(GzipCompressor::new())),

            #[cfg(not(target_arch = "wasm32"))]
            "bzip2" => Ok(Box::new(Bzip2Compressor::new())),

            #[cfg(not(target_arch = "wasm32"))]
            "lzma" => Ok(Box::new(LzmaCompressor::new())),

            #[cfg(target_arch = "wasm32")]
            "bzip2" | "lzma" => {
                Err(format!(
                    "Cannot decompress '{}' format in a WebAssembly context — \
                     this .mdix.enc file was compressed with a C-based algorithm \
                     unavailable in WASM builds. Decompress it using the native \
                     @dixscript/cli or the .NET library first, then load the \
                     resulting plain .mdix file.",
                    algorithm
                ))
            }

            _ => {
                let msg = format!(
                    "Unknown compression algorithm in key file: '{}'", algorithm
                );
                self.error_manager.add_dlm_error(
                    DlmErrorType::ModuleExecutionFailed,
                    msg.clone(),
                    Some(self.file_label()),
                    None,
                    None,
                    ErrorSeverity::Fatal,
                );
                Err(msg)
            }
        }
    }

    // ── Utility ───────────────────────────────────────────────────────────────

    fn derive_source_path(&self) -> PathBuf {
        let dir = self.encrypted_file_path
            .parent()
            .unwrap_or_else(|| Path::new("."));

        let mut name = self.encrypted_file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Strip compound suffix first, then single extension.
        // Previously this stripped ".dixscript" — must be ".mdix".
        if let Some(stripped) = name.strip_suffix(".enc")  { name = stripped.to_string(); }
        if let Some(stripped) = name.strip_suffix(".mdix") { name = stripped.to_string(); }

        let candidate = dir.join(format!("{}.mdix", name));

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[DLMReverseExecutor] Derived source path: {}",
                candidate.display(),
            ));
        }

        candidate
    }

    #[inline]
    fn file_label(&self) -> String {
        self.encrypted_file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }
}
