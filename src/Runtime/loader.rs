// src/Runtime/loader.rs

use std::fs;
use std::path::Path;
use chrono::Utc;
use crate::Compiler::Core::Tokenizer::Tokenizer;
use crate::Compiler::Core::Config::ConfigSectionHandler;
use crate::Compiler::Core::{GeneralParser, GeneralSemanticAnalyzer, GeneralAstEnhancer};
use crate::Compiler::Core::BinarySerialization::{BinaryPacker, BinaryUnpacker};
use crate::Compiler::Core::ValueResolution::ValueResolver;
use crate::Compiler::DLM::{DLMPipelineExecutor, DLMReverseExecutor};
use crate::Compiler::DLM::KeyManagement::KeyFileManager;
use crate::Compiler::DLM::Auditor::{IAuditor, DiyAuditor, EnhancedAuditor};
use crate::Compiler::Utilities::SecurityUtilities;
use crate::Compiler::AST::{DixScript, DLMModuleType, DLMModuleSubtype};
use crate::ErrorManager::{ErrorManager, RuntimeErrorType, ErrorSeverity};
use super::load_options::DixLoadOptions;
use super::key_resolver::{KeyFileResolver, KeyFileResolution, KeyFileSource};
use super::dix_data::DixData;

/// Internal loader for DixScript files
///
/// Handles:
/// - Text file loading (.mdix)
/// - Encrypted file loading (.mdix.enc)
/// - Full compilation pipeline
/// - DLM module execution (encryption, compression, auditing)
/// - Value resolution (QuickFunctions)
pub struct DixLoader {
    error_manager: ErrorManager,
    key_resolver: KeyFileResolver,
}

impl DixLoader {
    /// Create new loader
    pub fn new() -> Self {
        DixLoader {
            error_manager: ErrorManager::get_shared_instance(),
            key_resolver: KeyFileResolver::new(),
        }
    }

    /// Load plain text .mdix file
    pub fn load_text(
        &self,
        mdix_path: &str,
        options: &DixLoadOptions,
    ) -> Result<DixData, String> {
        self.error_manager.log_info(&format!("Loading text file: {}", mdix_path));

        // Check file exists
        if !Path::new(mdix_path).exists() {
            let error_msg = format!("File not found: {}", mdix_path);
            self.error_manager.add_runtime_error_with_severity(
                RuntimeErrorType::ResourceNotFound,
                error_msg.clone(),
                Some("DixLoader.load_text".to_string()),
                0,
                0,
                vec![],
                Some("Check file path".to_string()),
                ErrorSeverity::Error,
            );
            return Err(error_msg);
        }

        // Read source
        let source_text = fs::read_to_string(mdix_path).map_err(|e| {
            let error_msg = format!("Failed to read file {}: {}", mdix_path, e);
            self.error_manager.add_runtime_error_with_severity(
                RuntimeErrorType::InvalidOperation,
                error_msg.clone(),
                Some("DixLoader.load_text".to_string()),
                0,
                0,
                vec![],
                None,
                ErrorSeverity::Error,
            );
            error_msg
        })?;

        // Compile source
        let compilation_result = self.compile_source(&source_text, mdix_path)?;

        // Determine DLM behavior
        let file_gen_result = self.determine_dlm_behavior(
            &compilation_result,
            mdix_path,
            options,
        )?;

        // Create DixData
        let dix_data = DixData::from_ast(
            file_gen_result.resolved_ast,
            "1.0.0".to_string(),
            Utc::now(),
            file_gen_result.is_encrypted,
            file_gen_result.is_compressed,
            file_gen_result.applied_modules,
        );

        self.error_manager.log_info("Text file loaded successfully");

        if let Some(ref enc_file) = file_gen_result.generated_enc_file {
            self.error_manager.log_info(&format!("Generated encrypted file: {}", enc_file));
        }
        if let Some(ref key_file) = file_gen_result.generated_key_file {
            self.error_manager.log_info(&format!("Generated key file: {}", key_file));
        }
        if let Some(ref audit_file) = file_gen_result.generated_audit_file {
            self.error_manager.log_info(&format!("Generated audit file: {}", audit_file));
        }

        Ok(dix_data)
    }

    /// Load encrypted .mdix.enc file
    pub fn load_encrypted(
        &self,
        enc_path: &str,
        options: &DixLoadOptions,
    ) -> Result<DixData, String> {
        self.error_manager.log_info(&format!("Loading encrypted file: {}", enc_path));

        if !Path::new(enc_path).exists() {
            let error_msg = format!("Encrypted file not found: {}", enc_path);
            self.error_manager.add_runtime_error_with_severity(
                RuntimeErrorType::ResourceNotFound,
                error_msg.clone(),
                Some("DixLoader.load_encrypted".to_string()),
                0,
                0,
                vec![],
                None,
                ErrorSeverity::Error,
            );
            return Err(error_msg);
        }

        let encrypted_data = fs::read(enc_path).map_err(|e| {
            let error_msg = format!("Failed to read encrypted file {}: {}", enc_path, e);
            self.error_manager.add_runtime_error_with_severity(
                RuntimeErrorType::InvalidOperation,
                error_msg.clone(),
                Some("DixLoader.load_encrypted".to_string()),
                0,
                0,
                vec![],
                None,
                ErrorSeverity::Error,
            );
            error_msg
        })?;

        self.error_manager.log_info(&format!(
            "Encrypted file size: {} bytes",
            encrypted_data.len()
        ));

        // Locate the .dxkey file
        let key_resolution = self.key_resolver.resolve_key_file(enc_path, options)?;

        self.error_manager
            .log_info(&format!("Key source: {:?}", key_resolution.source));
        self.error_manager
            .log_info(&format!("Key from: {}", key_resolution.source_description));

        // Parse key file metadata
        let key_data = self.parse_key_file_content(&key_resolution.content)?;

        // Check password requirement
        if key_data.is_password_mode && options.password.is_none() {
            let error_msg =
                "Password required for decryption. \
                 This file was encrypted in password mode. \
                 Provide password via DixLoadOptions::with_password()";
            self.error_manager.add_runtime_error_with_severity(
                RuntimeErrorType::InvalidOperation,
                error_msg.to_string(),
                Some("DixLoader.load_encrypted".to_string()),
                0,
                0,
                vec![],
                Some("Use DixLoadOptions::with_password()".to_string()),
                ErrorSeverity::Error,
            );
            return Err(error_msg.to_string());
        }

        if key_data.is_password_mode {
            self.error_manager.log_info("Using password-based decryption");
        } else {
            self.error_manager.log_info("Using keyfile-based decryption");
        }

        // Execute reverse pipeline (decrypt + decompress)
        let binary_data = self.execute_reverse_pipeline(enc_path, &key_resolution, options, &key_data)?;

        self.error_manager
            .log_info(&format!("Decrypted data size: {} bytes", binary_data.len()));

        // Deserialize binary to AST
        let mut unpacker = BinaryUnpacker::new();
        let deser_result = unpacker.unpack(&binary_data);

        if !deser_result.is_success {
            let error_msg = format!("Binary deserialization failed: {:?}", deser_result.errors);
            self.error_manager.add_runtime_error_with_severity(
                RuntimeErrorType::InvalidOperation,
                error_msg.clone(),
                Some("DixLoader.load_encrypted".to_string()),
                0,
                0,
                vec![],
                None,
                ErrorSeverity::Error,
            );
            return Err(error_msg);
        }

        let ast = deser_result
            .ast
            .ok_or_else(|| "Deserialization succeeded but AST is None".to_string())?;

        let dix_data = DixData::from_ast(
            ast,
            key_data.version,
            key_data.compile_time,
            key_data.applied_modules.iter().any(|m| m.contains("Encryptor")),
            key_data.applied_modules.iter().any(|m| m.contains("Compressor")),
            key_data.applied_modules,
        );

        self.error_manager.log_info("Encrypted file loaded successfully");
        Ok(dix_data)
    }

    // ===== PRIVATE METHODS =====

    /// Compile source text to resolved AST
    ///
    /// Pipeline:
    /// 1. CONFIG processing
    /// 2. Tokenization
    /// 3. Parsing
    /// 4. Semantic analysis
    /// 4.5. AST Enhancement (post-semantic — required by Rust borrow rules)
    /// 5. Value resolution (QuickFunctions)
    fn compile_source(
        &self,
        source_text: &str,
        source_file_path: &str,
    ) -> Result<DixScript, String> {
        // Step 1: Process CONFIG section
        let config_handler = ConfigSectionHandler::new(None);
        let config_result = config_handler.process_config_section(source_text);

        let mut operational_settings = config_result.operational_settings;
        operational_settings.source_file_path = Some(source_file_path.to_string());

        self.error_manager.update_settings(operational_settings.clone());

        // Step 2: Tokenization
        let tokenizer = Tokenizer::new(config_result.cleaned_input_string.clone());
        let tok_result = tokenizer.tokenize();

        if !tok_result.tokens.is_empty()
            && tok_result.tokens.iter().any(|t| {
            matches!(
                    t.token_type,
                    crate::Compiler::Core::Tokenizer::TokenType::Error(_)
                )
        })
        {
            return Err("Tokenization failed - invalid tokens found".to_string());
        }

        // Step 3: Parsing
        let parser = GeneralParser::new(
            tok_result.tokens,
            config_result.config_section.clone(),
            operational_settings.clone(),
        )
            .map_err(|e| format!("Parser init failed: {}", e.message()))?;

        let ast = parser
            .parse()
            .map_err(|e| format!("Parse failed: {}", e.message()))?;

        // Step 4: Semantic analysis
        let semantic_analyzer = GeneralSemanticAnalyzer::new(&ast, &operational_settings);
        let semantic_result = semantic_analyzer.analyze();

        if !semantic_result.is_success {
            let error_messages: Vec<String> =
                semantic_result.errors.iter().map(|e| e.message.clone()).collect();
            return Err(format!("Semantic analysis failed: {:?}", error_messages));
        }

        // Step 4.5: AST Enhancement
        self.error_manager
            .log_info("Running AST Enhancement (post-semantic)");
        let enhancer = GeneralAstEnhancer::new(&operational_settings);
        let enhancement_result = enhancer.enhance(&ast, Some(&semantic_result));

        if !enhancement_result.is_success {
            self.error_manager.log_warning(&format!(
                "AST enhancement had {} issue(s) - continuing with best-effort result",
                enhancement_result.errors.len()
            ));
        } else {
            self.error_manager.log_info(&format!(
                "AST enhancement complete: {} enhancements applied",
                enhancement_result.total_enhancements
            ));
        }

        let mut resolved_ast = enhancement_result.enhanced_ast;

        // Step 5: Value resolution (if QuickFunctions exist)
        let has_local_functions = resolved_ast.quick_functions.is_some();
        let has_imported_functions = semantic_result
            .symbol_table
            .as_ref()
            .map(|st| st.namespaces.values().any(|ns| !ns.functions.is_empty()))
            .unwrap_or(false);
        let has_data_section = resolved_ast.data.is_some();

        self.error_manager.log_info(&format!(
            "Value resolution check: local_funcs={}, imported_funcs={}, data={}",
            has_local_functions, has_imported_functions, has_data_section
        ));

        if (has_local_functions || has_imported_functions)
            && has_data_section
            && semantic_result.symbol_table.is_some()
        {
            self.error_manager.log_info("Starting value resolution");

            let value_resolver = ValueResolver::new(
                resolved_ast,
                semantic_result.symbol_table.as_ref().unwrap(),
                operational_settings.debug_mode,
            );

            let resolution_result = value_resolver.resolve();

            if !resolution_result.is_success {
                let error_messages: Vec<String> =
                    resolution_result.errors.iter().map(|e| e.clone()).collect();
                return Err(format!("Value resolution failed: {:?}", error_messages));
            }

            self.error_manager.log_info(&format!(
                "Value resolution complete: {} calls resolved",
                resolution_result.function_calls_resolved
            ));

            resolved_ast = resolution_result
                .resolved_ast
                .ok_or_else(|| "Resolution succeeded but AST is None".to_string())?;
        } else {
            self.error_manager
                .log_info("Skipping value resolution (no functions or no data)");
        }

        Ok(resolved_ast)
    }

    /// Determine DLM behavior and execute if needed
    fn determine_dlm_behavior(
        &self,
        ast: &DixScript,
        source_file_path: &str,
        options: &DixLoadOptions,
    ) -> Result<DLMFileGeneration, String> {
        let mut result = DLMFileGeneration {
            resolved_ast: ast.clone(),
            is_encrypted: false,
            is_compressed: false,
            applied_modules: Vec::new(),
            generated_enc_file: None,
            generated_key_file: None,
            generated_audit_file: None,
        };

        if ast.dlm.is_none() {
            self.error_manager
                .log_info("No DLM modules - returning resolved AST only");
            return Ok(result);
        }

        let dlm_section = ast.dlm.as_ref().unwrap();

        let has_auditor = dlm_section
            .modules
            .iter()
            .any(|m| m.module_type == DLMModuleType::DAuditor);
        let has_compressor = dlm_section
            .modules
            .iter()
            .any(|m| m.module_type == DLMModuleType::DCompressor);
        let has_encryptor = dlm_section
            .modules
            .iter()
            .any(|m| m.module_type == DLMModuleType::DEncryptor);

        // Auditor only — generate .mdix.au file, no binary output
        if has_auditor && !has_compressor && !has_encryptor {
            self.error_manager
                .log_info("DAuditor only - generating .mdix.au file");
            let audit_file = self.generate_audit_only(ast, source_file_path, options)?;
            result.applied_modules.push("DAuditor".to_string());
            result.generated_audit_file = Some(audit_file);
            return Ok(result);
        }

        // Compressor / Encryptor — generate binary files
        if has_compressor || has_encryptor {
            self.error_manager
                .log_info("DCompressor/DEncryptor detected - generating binary files");

            let output_dir = options
                .output_directory
                .as_deref()
                .unwrap_or_else(|| {
                    Path::new(source_file_path)
                        .parent()
                        .and_then(|p| p.to_str())
                        .unwrap_or(".")
                });

            fs::create_dir_all(output_dir).map_err(|e| {
                format!("Failed to create output directory {}: {}", output_dir, e)
            })?;

            // Ensure valid SECURITY section
            let mut ast_with_security = ast.clone();
            ast_with_security.security = Some(
                SecurityUtilities::ensure_valid_security_section(
                    ast_with_security.security,
                    ast_with_security.dlm.as_ref(),
                ),
            );

            // Binary serialization
            let mut packer = BinaryPacker::new();
            let ser_result = packer.pack(&ast_with_security);

            if !ser_result.is_success {
                return Err(format!(
                    "Binary serialization failed: {:?}",
                    ser_result.errors
                ));
            }

            let binary_data = ser_result.binary_data;

            self.error_manager.log_info(&format!(
                "Binary serialization complete: {} bytes",
                binary_data.len()
            ));

            // Execute DLM pipeline
            let debug_mode = crate::Compiler::Core::DebugMode::Off;
            let dlm_executor =
                DLMPipelineExecutor::new(source_file_path, output_dir, debug_mode);

            let dlm_result = dlm_executor.execute(&mut ast_with_security, binary_data);

            if !dlm_result.is_success {
                return Err(format!("DLM pipeline failed: {:?}", dlm_result.errors));
            }

            result.is_compressed = has_compressor;
            result.is_encrypted = has_encryptor;
            result.applied_modules = dlm_result.executed_modules;
            result.generated_enc_file = dlm_result.encrypted_file_path;
            result.generated_key_file = dlm_result.key_file_path;
            result.generated_audit_file = dlm_result.audit_file_path;

            self.error_manager.log_info(&format!(
                "DLM pipeline complete: {} modules executed",
                result.applied_modules.len()
            ));
        }

        Ok(result)
    }

    /// Generate audit file only (DAuditor without encryption/compression)
    fn generate_audit_only(
        &self,
        ast: &DixScript,
        source_file_path: &str,
        options: &DixLoadOptions,
    ) -> Result<String, String> {
        let output_dir = options
            .output_directory
            .as_deref()
            .unwrap_or_else(|| {
                Path::new(source_file_path)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or(".")
            });

        fs::create_dir_all(output_dir).map_err(|e| {
            format!("Failed to create output directory {}: {}", output_dir, e)
        })?;

        let dlm_section = ast.dlm.as_ref().unwrap();
        let auditor_module = dlm_section
            .modules
            .iter()
            .find(|m| m.module_type == DLMModuleType::DAuditor)
            .ok_or_else(|| "DAuditor module not found in DLM section".to_string())?;

        let mut auditor: Box<dyn IAuditor> = match auditor_module.subtype {
            Some(DLMModuleSubtype::Diy) | None => {
                Box::new(DiyAuditor::new(source_file_path, output_dir))
            }
            Some(DLMModuleSubtype::Enhanced) => Box::new(EnhancedAuditor::new(
                source_file_path.to_string(),
                output_dir.to_string(),
                ast.clone(),
            )),
            _ => {
                return Err(format!(
                    "Unknown auditor subtype: {:?}",
                    auditor_module.subtype
                ))
            }
        };

        let audit_result = auditor
            .start_audit(ast, &[])
            .map_err(|e| format!("Failed to start audit: {}", e))?;

        auditor
            .finalize_audit()
            .map_err(|e| format!("Failed to finalize audit: {}", e))?;

        self.error_manager.log_info(&format!(
            "Audit file created: {}",
            audit_result.audit_file_path
        ));

        Ok(audit_result.audit_file_path)
    }

    /// Parse .dxkey JSON content into our internal metadata type.
    ///
    /// Writes to a temp file, delegates to KeyFileManager for parsing,
    /// then maps the structured result to what the loader needs.
    fn parse_key_file_content(&self, key_content: &str) -> Result<LoaderKeyMetadata, String> {
        let temp_dir = std::env::temp_dir();
        let temp_key_file =
            temp_dir.join(format!("temp_dxkey_{}.dxkey", uuid::Uuid::new_v4()));

        fs::write(&temp_key_file, key_content)
            .map_err(|e| format!("Failed to write temp key file: {}", e))?;

        let key_manager = KeyFileManager::new(
            String::new(),
            temp_dir.to_string_lossy().to_string(),
        );

        let km_metadata = key_manager
            .read_key_file(temp_key_file.to_str().unwrap_or(""))
            .map_err(|e| format!("Failed to parse key file content: {}", e))?;

        if let Err(e) = fs::remove_file(&temp_key_file) {
            self.error_manager
                .log_warning(&format!("Failed to clean up temp key file: {}", e));
        }

        let is_password_mode = km_metadata
            .encryption
            .as_ref()
            .map(|enc| enc.salt.is_some() && enc.kdf_algorithm.is_some())
            .unwrap_or(false);

        let mut applied_modules = Vec::new();
        if km_metadata.compression.is_some() {
            applied_modules.push("Compressor".to_string());
        }
        if km_metadata.encryption.is_some() {
            applied_modules.push("Encryptor".to_string());
        }
        if km_metadata.audit.is_some() {
            applied_modules.push("Auditor".to_string());
        }

        Ok(LoaderKeyMetadata {
            version: km_metadata.version,
            compile_time: Utc::now(),
            applied_modules,
            is_password_mode,
        })
    }

    /// Execute reverse DLM pipeline (decrypt + decompress).
    ///
    /// For non-file key sources (DirectContent / Url), the key content is
    /// written to a temp file so DLMReverseExecutor can read it normally.
    fn execute_reverse_pipeline(
        &self,
        enc_path: &str,
        resolved_key: &KeyFileResolution,
        options: &DixLoadOptions,
        _key_data: &LoaderKeyMetadata,
    ) -> Result<Vec<u8>, String> {
        let (key_file_path, using_temp) = match &resolved_key.source {
            KeyFileSource::FilePath | KeyFileSource::AutoDetected => {
                let path = resolved_key
                    .file_path
                    .as_ref()
                    .ok_or_else(|| "FilePath/AutoDetected source missing file_path".to_string())?;
                (path.to_string_lossy().to_string(), false)
            }
            KeyFileSource::DirectContent | KeyFileSource::Url => {
                let temp_dir = std::env::temp_dir();
                let temp_key_path =
                    temp_dir.join(format!("temp_key_{}.dxkey", uuid::Uuid::new_v4()));

                fs::write(&temp_key_path, &resolved_key.content)
                    .map_err(|e| format!("Failed to write temp key file: {}", e))?;

                self.error_manager.log_info(&format!(
                    "Created temporary key file: {}",
                    temp_key_path.display()
                ));

                (temp_key_path.to_string_lossy().to_string(), true)
            }
        };

        let debug_mode = crate::Compiler::Core::DebugMode::Off;
        let reverse_executor = DLMReverseExecutor::new(
            enc_path,
            &key_file_path,
            options.password.clone(),
            debug_mode,
        );

        let reverse_result = reverse_executor.execute();

        // Cleanup temp key file
        if using_temp {
            if let Err(e) = fs::remove_file(&key_file_path) {
                self.error_manager.log_warning(&format!(
                    "Failed to delete temp key file '{}': {}",
                    key_file_path, e
                ));
            } else {
                self.error_manager.log_info("Temporary key file cleaned up");
            }
        }

        if !reverse_result.is_success {
            return Err(format!(
                "Reverse pipeline failed: {:?}",
                reverse_result.errors
            ));
        }

        Ok(reverse_result.restored_data)
    }
}

impl Default for DixLoader {
    fn default() -> Self {
        Self::new()
    }
}

// ===== INTERNAL RESULT TYPES =====

struct DLMFileGeneration {
    resolved_ast: DixScript,
    is_encrypted: bool,
    is_compressed: bool,
    applied_modules: Vec<String>,
    generated_enc_file: Option<String>,
    generated_key_file: Option<String>,
    generated_audit_file: Option<String>,
}

struct LoaderKeyMetadata {
    version: String,
    compile_time: chrono::DateTime<Utc>,
    applied_modules: Vec<String>,
    is_password_mode: bool,
}

// ===== TESTS =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loader_creation() {
        let loader = DixLoader::new();
        assert!(!loader.error_manager.has_errors());
    }
}