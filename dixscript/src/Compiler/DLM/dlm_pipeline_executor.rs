//! Main orchestrator for DLM forward pipeline execution during compilation.
//!
//! Execution order: Auditor (start) → Compressor → Encryptor → Auditor (finalize).
//! All three output files (.mdix.enc, .mdix.key, .mdix.au) are locked
//! read-only immediately after writing.

use crate::Compiler::AST::{DixScript, DLMModuleType, DLMModuleSubtype};
use crate::Compiler::DLM::{
    Auditor::{IAuditor, DiyAuditor, EnhancedAuditor},
    Compressor::{ICompressor, GzipCompressor},
    Encryptor::{IEncryptor, XorEncryptor, Aes128Encryptor, Aes256Encryptor, Chacha20Encryptor},
    KeyManagement::KeyFileManager,
    dlm_pipeline_result::DLMPipelineResult,
};
use crate::Compiler::Utilities::{SecurityUtilities, file_permissions};
use crate::ErrorManager::{ErrorManager, DebugConfig, DlmErrorType, ErrorSeverity};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::fs;

#[cfg(feature = "bzip2-support")]
use crate::Compiler::DLM::Compressor::Bzip2Compressor;
#[cfg(feature = "xz-support")]
use crate::Compiler::DLM::Compressor::LzmaCompressor;

pub struct DLMPipelineExecutor {
    error_manager:    ErrorManager,
    debug_config:     DebugConfig,
    source_file_path: PathBuf,
    output_directory: PathBuf,
    /// Password read once from MDIX_DLM_PASSWORD at construction time.
    /// Present only when the environment variable is set.
    password:         Option<String>,
}

impl DLMPipelineExecutor {
    pub fn new(
        source_file_path: impl AsRef<Path>,
        output_directory: impl AsRef<Path>,
        debug_mode:       crate::Compiler::Core::Config::DebugMode,
    ) -> Self {
      Self::new_with_error_manager(source_file_path,output_directory,debug_mode,ErrorManager::get_shared_instance())
    }
    pub fn new_with_error_manager(
        source_file_path: impl AsRef<Path>,
        output_directory: impl AsRef<Path>,
        debug_mode:       crate::Compiler::Core::Config::DebugMode,
        error_manager: ErrorManager
    ) -> Self {

        let debug_config  = DebugConfig::from_debug_mode(debug_mode);
        let password      = std::env::var("MDIX_DLM_PASSWORD").ok();

        DLMPipelineExecutor {
            error_manager,
            debug_config,
            source_file_path: source_file_path.as_ref().to_path_buf(),
            output_directory: output_directory.as_ref().to_path_buf(),
            password,
        }
    }
    // ── Main entry point ──────────────────────────────────────────────────────

    pub fn execute(&self, ast: &mut DixScript, binary_data: Vec<u8>) -> DLMPipelineResult {
        let original_size = binary_data.len();
        let mut result    = DLMPipelineResult::new(original_size);
        let start_time    = Instant::now();

        self.error_manager.log_info("DLM pipeline execution started");

        let dlm_is_empty = ast.dlm.as_ref()
            .map(|d| d.modules.is_empty())
            .unwrap_or(true);

        if dlm_is_empty {
            self.error_manager.log_info("No DLM modules specified — skipping pipeline");
            result.is_success     = true;
            result.processed_size = binary_data.len();
            result.processed_data = binary_data;
            result.total_duration = start_time.elapsed();
            return result;
        }

        let (auditor, compressor, encryptor) = match self.parse_dlm_section(ast) {
            Ok(modules) => modules,
            Err(e) => {
                self.error_manager.add_dlm_error(
                    DlmErrorType::ModuleExecutionFailed, e.clone(),
                    Some(self.base_name()), None, None, ErrorSeverity::Fatal,
                );
                result.errors.push(e);
                result.total_duration = start_time.elapsed();
                return result;
            }
        };

        let mut processed_data                             = binary_data;
        let mut active_auditor: Option<Box<dyn IAuditor>> = None;

        // Phase 1: start auditor
        if let Some(mut aud) = auditor {
            match aud.start_audit(ast, &processed_data) {
                Ok(_)  => { active_auditor = Some(aud); }
                Err(e) => {
                    self.error_manager.add_dlm_error(
                        DlmErrorType::ModuleExecutionFailed, e.clone(),
                        Some(self.base_name()), Some("DAuditor".to_string()),
                        None, ErrorSeverity::Warning,
                    );
                    result.warnings.push(e);
                }
            }
        }

        // Phase 2: compress
        let pre_compress_size = processed_data.len();
        if let Some(comp) = compressor {
            let phase_start = Instant::now();
            match comp.compress(&processed_data) {
                Ok(compressed) => {
                    let duration_ms = phase_start.elapsed().as_secs_f64() * 1000.0;
                    let out_size    = compressed.len();

                    result.executed_modules.push(comp.module_name().to_string());
                    result.metadata.insert("compressor".to_string(), comp.get_metadata());

                    if let Some(ref mut aud) = active_auditor {
                        aud.log_step(
                            comp.module_name(),
                            &format!("Compressed with {}", comp.algorithm()),
                            pre_compress_size,
                            out_size,
                            duration_ms,
                        );
                    }

                    let ratio = 1.0 - (out_size as f64 / pre_compress_size as f64);
                    self.error_manager.log_info(&format!(
                        "Compression: {} -> {} bytes ({:.1}% reduction)",
                        pre_compress_size, out_size, ratio * 100.0,
                    ));

                    processed_data = compressed;
                }
                Err(e) => {
                    self.error_manager.add_dlm_error(
                        DlmErrorType::ModuleExecutionFailed, e.clone(),
                        Some(self.base_name()), Some(comp.module_name().to_string()),
                        None, ErrorSeverity::Fatal,
                    );
                    if let Some(ref mut aud) = active_auditor {
                        let _ = aud.finalize_audit();
                    }
                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            }
        }

        // Phase 3: encrypt
        let pre_encrypt_size = processed_data.len();
        if let Some(enc) = encryptor {
            let phase_start = Instant::now();
            match enc.encrypt(&processed_data) {
                Ok(encrypted) => {
                    let duration_ms = phase_start.elapsed().as_secs_f64() * 1000.0;
                    let out_size    = encrypted.len();

                    result.executed_modules.push(enc.module_name().to_string());
                    result.metadata.insert("encryptor".to_string(), enc.get_metadata());

                    if let Some(ref mut aud) = active_auditor {
                        aud.log_step(
                            enc.module_name(),
                            &format!("Encrypted with {}", enc.algorithm()),
                            pre_encrypt_size,
                            out_size,
                            duration_ms,
                        );
                    }

                    self.error_manager.log_info(&format!(
                        "Encryption: {} -> {} bytes",
                        pre_encrypt_size, out_size,
                    ));

                    processed_data = encrypted;
                }
                Err(e) => {
                    self.error_manager.add_dlm_error(
                        DlmErrorType::ModuleExecutionFailed, e.clone(),
                        Some(self.base_name()), Some(enc.module_name().to_string()),
                        None, ErrorSeverity::Fatal,
                    );
                    if let Some(ref mut aud) = active_auditor {
                        let _ = aud.finalize_audit();
                    }
                    result.errors.push(e);
                    result.total_duration = start_time.elapsed();
                    return result;
                }
            }
        }

        result.processed_size    = processed_data.len();
        result.compression_ratio = 1.0 - (result.processed_size as f64 / original_size as f64);
        result.processed_data    = processed_data;

        // Phase 4: finalize auditor
        if let Some(ref mut aud) = active_auditor {
            if let Err(e) = aud.finalize_audit() {
                self.error_manager.add_dlm_error(
                    DlmErrorType::ModuleExecutionFailed, e.clone(),
                    Some(self.base_name()), Some("DAuditor".to_string()),
                    None, ErrorSeverity::Warning,
                );
                result.warnings.push(format!("Audit finalization warning: {}", e));
            }
        }

        // Phase 5: write .mdix.enc and .mdix.key
        if let Err(e) = self.generate_output_files(&mut result, original_size) {
            self.error_manager.add_dlm_error(
                DlmErrorType::ModuleExecutionFailed, e.clone(),
                Some(self.base_name()), None,
                Some("Check output directory write permissions".to_string()),
                ErrorSeverity::Warning,
            );
            result.warnings.push(e);
        }

        result.is_success     = true;
        result.total_duration = start_time.elapsed();

        self.error_manager.log_info(&format!(
            "DLM pipeline complete: {} modules, {} -> {} bytes, {:.2}ms",
            result.executed_modules.len(),
            original_size,
            result.processed_size,
            result.total_duration.as_secs_f64() * 1000.0,
        ));

        result
    }

    // ── Module creation ───────────────────────────────────────────────────────

    fn parse_dlm_section(
        &self,
        ast: &mut DixScript,
    ) -> Result<(
        Option<Box<dyn IAuditor>>,
        Option<Box<dyn ICompressor>>,
        Option<Box<dyn IEncryptor>>,
    ), String> {
        let dlm = ast.dlm.as_ref().unwrap();

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[DLMPipelineExecutor] Parsing {} module(s)",
                dlm.modules.len(),
            ));
        }

        let mut auditor:    Option<Box<dyn IAuditor>>    = None;
        let mut compressor: Option<Box<dyn ICompressor>> = None;
        let mut encryptor:  Option<Box<dyn IEncryptor>>  = None;

        let modules: Vec<_> = dlm.modules.iter().cloned().collect();

        for module in &modules {
            match module.module_type {
                DLMModuleType::DAuditor => {
                    auditor = Some(self.create_auditor(module.subtype, ast)?);
                }
                DLMModuleType::DCompressor => {
                    compressor = Some(self.create_compressor(module.subtype)?);
                }
                DLMModuleType::DEncryptor => {
                    encryptor = Some(self.create_encryptor(module.subtype, ast)?);
                }
                DLMModuleType::ParseError => {
                    return Err(
                        "DLM section contains a parse error — check @DLM syntax".to_string()
                    );
                }
            }
        }

        Ok((auditor, compressor, encryptor))
    }

    fn create_auditor(
        &self,
        subtype: Option<DLMModuleSubtype>,
        ast:     &DixScript,
    ) -> Result<Box<dyn IAuditor>, String> {
        let aud: Box<dyn IAuditor> = match subtype {
            Some(DLMModuleSubtype::Diy) | None => {
                Box::new(DiyAuditor::new(&self.source_file_path, &self.output_directory))
            }
            Some(DLMModuleSubtype::Enhanced) => {
                Box::new(EnhancedAuditor::new(
                    self.source_file_path.to_string_lossy().to_string(),
                    self.output_directory.to_string_lossy().to_string(),
                    ast.clone(),
                ))
            }
            Some(other) => return Err(format!("Unknown auditor subtype: {:?}", other)),
        };
        Ok(aud)
    }

    fn create_compressor(
        &self,
        subtype: Option<DLMModuleSubtype>,
    ) -> Result<Box<dyn ICompressor>, String> {
        match subtype {
            Some(DLMModuleSubtype::Gzip) | None => Ok(Box::new(GzipCompressor::new())),

            #[cfg(feature = "bzip2-support")]
            Some(DLMModuleSubtype::Bzip2) => Ok(Box::new(Bzip2Compressor::new())),
            #[cfg(not(feature = "bzip2-support"))]
            Some(DLMModuleSubtype::Bzip2) => Err(
                "This file requires bzip2 compression (DCompressor.bzip2), but \
                 this build of dixscript was compiled without the \
                 'bzip2-support' feature. Rebuild with `--features \
                 bzip2-support` (or default features) to read this file, or \
                 use DCompressor.gzip in files you control.".to_string()
            ),

            #[cfg(feature = "xz-support")]
            Some(DLMModuleSubtype::Lzma) => Ok(Box::new(LzmaCompressor::new())),
            #[cfg(not(feature = "xz-support"))]
            Some(DLMModuleSubtype::Lzma) => Err(
                "This file requires XZ/LZMA compression (DCompressor.lzma), but \
                 this build of dixscript was compiled without the 'xz-support' \
                 feature. Rebuild with `--features xz-support` (or default \
                 features) to read this file, or use DCompressor.gzip in \
                 files you control.".to_string()
            ),

            Some(other) => Err(format!("Unknown compressor subtype: {:?}", other)),
        }
    }

    fn create_encryptor(
        &self,
        subtype: Option<DLMModuleSubtype>,
        ast:     &mut DixScript,
    ) -> Result<Box<dyn IEncryptor>, String> {
        ast.security = Some(SecurityUtilities::ensure_valid_security_section(
            ast.security.take(),
            ast.dlm.as_ref(),
        ));

        if let Err(errors) = SecurityUtilities::is_valid_for_encryption(
            ast.security.as_ref().unwrap(),
        ) {
            return Err(format!("SECURITY section validation failed: {:?}", errors));
        }

        let security = ast.security.as_ref().unwrap();

        let mut enc: Box<dyn IEncryptor> = match subtype {
            Some(DLMModuleSubtype::Xor)                => Box::new(XorEncryptor::new(Some(security.clone()))),
            Some(DLMModuleSubtype::Aes128)             => Box::new(Aes128Encryptor::new(Some(security.clone()))),
            Some(DLMModuleSubtype::Aes256) | None      => Box::new(Aes256Encryptor::new(Some(security.clone()))),
            Some(DLMModuleSubtype::Chacha20)           => Box::new(Chacha20Encryptor::new(Some(security.clone()))),
            Some(other) => return Err(format!("Unknown encryptor subtype: {:?}", other)),
        };

        // In password mode, the encryptor derives the key from a password via Argon2.
        // Supply the password so the encryptor can do that derivation before encrypt() is called.
        let mode = SecurityUtilities::get_encryption_mode(security);
        if mode.eq_ignore_ascii_case("password") {
            let password = self.password.as_deref().ok_or_else(|| {
                "Encryption mode is 'password' but no password was provided. \
                 Pass --password <pw> to the compile command or set MDIX_DLM_PASSWORD.".to_string()
            })?;

            if self.debug_config.is_enabled {
                self.error_manager.log_debug(
                    "[DLMPipelineExecutor] Supplying password to encryptor for key derivation"
                );
            }

            enc.set_password(password)
                .map_err(|e| format!("Failed to initialise encryption password: {}", e))?;
        }

        Ok(enc)
    }

    // ── Output file generation ────────────────────────────────────────────────

    fn generate_output_files(
        &self,
        result:        &mut DLMPipelineResult,
        original_size: usize,
    ) -> Result<(), String> {
        if result.metadata.is_empty() {
            return Ok(());
        }

        let base_name = self.source_file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid source file name")?;

        let enc_path = self.output_directory.join(format!("{}.mdix.enc", base_name));

        self.write_enc_file(&enc_path, &result.processed_data)?;

        let enc_path_str = enc_path.to_string_lossy().to_string();
        self.error_manager.log_info(&format!("Output file: {}", enc_path.display()));

        let compressed_size = result.metadata
            .get("compressor")
            .and_then(|m| m.get("compressed_size"))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(original_size);

        let key_manager = KeyFileManager::new(
            self.source_file_path.to_string_lossy().to_string(),
            self.output_directory.to_string_lossy().to_string(),
        );

        let key_file_path = key_manager.create_key_file(
            &enc_path_str,
            result.metadata.get("compressor").cloned(),
            result.metadata.get("encryptor").cloned(),
            result.metadata.get("auditor").cloned(),
            (original_size, compressed_size, result.processed_size),
        )?;

        self.error_manager.log_info(&format!("Key file: {}", key_file_path));

        result.encrypted_file_path = Some(enc_path_str);
        result.key_file_path       = Some(key_file_path);

        Ok(())
    }

    fn write_enc_file(&self, path: &Path, data: &[u8]) -> Result<(), String> {
        if path.exists() {
            file_permissions::set_writable(path)
                .map_err(|e| format!("Cannot unlock .mdix.enc for writing: {}", e))?;
        }

        let result = fs::write(path, data)
            .map_err(|e| format!("Failed to write encrypted file: {}", e));

        if let Err(e) = file_permissions::set_readonly(path) {
            self.error_manager.log_warning(&format!("Could not lock .mdix.enc read-only: {}", e));
        }

        result
    }

    // ── Utility ───────────────────────────────────────────────────────────────

    #[inline]
    fn base_name(&self) -> String {
        self.source_file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }
            }
