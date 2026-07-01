
use std::fs;
use std::path::{Path, PathBuf};
use chrono::Utc;
use crate::Compiler::Core::Tokenizer::{Tokenizer, split_config_tokens};
use crate::Compiler::Core::Config::{ConfigSectionHandler, DebugMode, OperationalSettings};
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
use super::array_homogenizer::homogenize_data_section;

/// Internal loader for DixScript files.
///
/// Each `DixLoader` owns an isolated `ErrorManager` so loading multiple
/// files never mixes error state between calls.
pub struct DixLoader {
    error_manager: ErrorManager,
    key_resolver:  KeyFileResolver,
}

impl DixLoader {
    pub fn new() -> Self {
        DixLoader {
            error_manager: ErrorManager::new_isolated(),
            key_resolver:  KeyFileResolver::new(),
        }
    }

    // ── Public API ────────────────────────────────────────────────────────────

    pub fn load_text(
        &self,
        mdix_path: &str,
        options: &DixLoadOptions,
    ) -> Result<DixData, String> {
        self.error_manager.clear_errors();
        self.error_manager.log_info(&format!("Loading text file: {}", mdix_path));

        if !Path::new(mdix_path).exists() {
            let msg = format!("File not found: {}", mdix_path);
            self.error_manager.add_runtime_error(
                RuntimeErrorType::ResourceNotFound,
                msg.clone(),
                Some("DixLoader.load_text".to_string()),
                0, 0, vec![],
                Some("Check the file path".to_string()),
            );
            return Err(msg);
        }

        let source_text = fs::read_to_string(mdix_path).map_err(|e| {
            let msg = format!("Failed to read file {}: {}", mdix_path, e);
            self.error_manager.add_runtime_error(
                RuntimeErrorType::InvalidOperation,
                msg.clone(),
                Some("DixLoader.load_text".to_string()),
                0, 0, vec![], None,
            );
            msg
        })?;

        let compiled_ast = self.compile_source(&source_text, mdix_path)?;
        let file_gen     = self.determine_dlm_behavior(&compiled_ast, mdix_path, options)?;

        self.log_generated_files(&file_gen);
        self.error_manager.log_info("Text file loaded successfully");

        Ok(DixData::from_ast(
            file_gen.resolved_ast,
            "1.0.0".to_string(),
            Utc::now(),
            file_gen.is_encrypted,
            file_gen.is_compressed,
            file_gen.applied_modules,
        ))
    }

    pub fn load_from_str(
        &self,
        source: &str,
        options: &DixLoadOptions,
    ) -> Result<DixData, String> {
        self.error_manager.clear_errors();
        self.error_manager.log_info("Loading from string source");

        if source.trim().is_empty() {
            let msg = "Source string is empty".to_string();
            self.error_manager.add_runtime_error(
                RuntimeErrorType::InvalidArgument,
                msg.clone(),
                Some("DixLoader.load_from_str".to_string()),
                0, 0, vec![],
                Some("Provide non-empty DixScript source".to_string()),
            );
            return Err(msg);
        }

        let compiled_ast = self.compile_source(source, "<string_input>")?;

        self.error_manager.log_info("String source loaded successfully");

        Ok(DixData::from_ast(
            compiled_ast,
            "1.0.0".to_string(),
            Utc::now(),
            false,
            false,
            vec![],
        ))
    }

    pub fn load_encrypted(
        &self,
        enc_path: &str,
        options: &DixLoadOptions,
    ) -> Result<DixData, String> {
        self.error_manager.clear_errors();
        self.error_manager.log_info(&format!("Loading encrypted file: {}", enc_path));

        if !Path::new(enc_path).exists() {
            let msg = format!("Encrypted file not found: {}", enc_path);
            self.error_manager.add_runtime_error(
                RuntimeErrorType::ResourceNotFound,
                msg.clone(),
                Some("DixLoader.load_encrypted".to_string()),
                0, 0, vec![], None,
            );
            return Err(msg);
        }

        let encrypted_data = fs::read(enc_path).map_err(|e| {
            let msg = format!("Failed to read encrypted file {}: {}", enc_path, e);
            self.error_manager.add_runtime_error(
                RuntimeErrorType::InvalidOperation,
                msg.clone(),
                Some("DixLoader.load_encrypted".to_string()),
                0, 0, vec![], None,
            );
            msg
        })?;

        self.error_manager.log_info(&format!("Encrypted file size: {} bytes", encrypted_data.len()));

        let key_resolution = self.key_resolver.resolve_key_file(enc_path, options)?;
        self.error_manager.log_info(&format!("Key from: {}", key_resolution.source_description));

        self.decrypt_and_deserialize(&encrypted_data, &key_resolution, enc_path, options)
    }

    pub fn load_from_encrypted_bytes(
        &self,
        encrypted_bytes: &[u8],
        key_file_content: &str,
        options: &DixLoadOptions,
    ) -> Result<DixData, String> {
        self.error_manager.clear_errors();
        self.error_manager.log_info(&format!(
            "Loading from encrypted bytes ({} bytes)", encrypted_bytes.len()
        ));

        if encrypted_bytes.is_empty() {
            let msg = "Encrypted bytes slice is empty".to_string();
            self.error_manager.add_runtime_error(
                RuntimeErrorType::InvalidArgument,
                msg.clone(),
                Some("DixLoader.load_from_encrypted_bytes".to_string()),
                0, 0, vec![],
                Some("Provide non-empty encrypted data".to_string()),
            );
            return Err(msg);
        }

        if key_file_content.trim().is_empty() {
            let msg = "Key file content string is empty".to_string();
            self.error_manager.add_runtime_error(
                RuntimeErrorType::InvalidArgument,
                msg.clone(),
                Some("DixLoader.load_from_encrypted_bytes".to_string()),
                0, 0, vec![],
                Some("Provide the full .mdix.key file content".to_string()),
            );
            return Err(msg);
        }

        let temp_dir  = std::env::temp_dir();
        let id        = uuid::Uuid::new_v4();
        let temp_enc  = temp_dir.join(format!("dix_enc_{}.mdix.enc", id));
        let temp_key  = temp_dir.join(format!("dix_key_{}.mdix.key", id));

        fs::write(&temp_enc, encrypted_bytes)
            .map_err(|e| format!("Failed to write temp encrypted file: {}", e))?;
        fs::write(&temp_key, key_file_content)
            .map_err(|e| format!("Failed to write temp key file: {}", e))?;

        let key_resolution = KeyFileResolution {
            source: KeyFileSource::FilePath,
            source_description: "In-memory bytes provided by caller".to_string(),
            content: key_file_content.to_string(),
            file_path: Some(temp_key.clone()),
        };

        let result = self.decrypt_and_deserialize(
            encrypted_bytes,
            &key_resolution,
            temp_enc.to_str().unwrap_or(""),
            options,
        );

        for temp in [&temp_enc, &temp_key] {
            if let Err(e) = fs::remove_file(temp) {
                self.error_manager.log_warning(&format!(
                    "Failed to remove temp file '{}': {}", temp.display(), e
                ));
            }
        }

        result
    }

    // ── Compile-only path ─────────────────────────────────────────────────────
    //
    // Runs the full tokenise → parse → semantic → enhance → value-resolve
    // pipeline and returns the resolved AST without running any DLM modules.
    // Imports are resolved using the file's own directory as the base path,
    // so multi-file projects (like the chemistry DB) resolve correctly.

    pub fn compile_to_resolved_ast(&self, file_path: &str) -> Result<DixScript, String> {
        self.error_manager.clear_errors();
        self.error_manager.log_info(&format!("compile_to_resolved_ast: {}", file_path));

        if !Path::new(file_path).exists() {
            return Err(format!("File not found: {}", file_path));
        }

        let source_text = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read {}: {}", file_path, e))?;

        self.compile_source(&source_text, file_path)
    }

    /// String-based sibling of `compile_to_resolved_ast` — same pipeline
    /// (tokenize -> parse -> semantic -> enhance -> value-resolve), minus
    /// the file read. `label` is used only for error messages (pass
    /// something like `"<source>"` or a caller-meaningful name) — it does
    /// not need to be a real path. Exists for targets with no real
    /// filesystem (wasm32-unknown-unknown has none at all) and for
    /// callers merging already-in-memory source text instead of files.
    pub fn compile_to_resolved_ast_from_str(
        &self,
        source: &str,
        label: &str,
    ) -> Result<DixScript, String> {
        self.error_manager.clear_errors();
        self.error_manager.log_info(&format!("compile_to_resolved_ast_from_str: {}", label));

        if source.trim().is_empty() {
            return Err(format!("'{}': source is empty", label));
        }

        self.compile_source(source, label)
    }

    // ── Shared decryption + deserialization ───────────────────────────────────

    fn decrypt_and_deserialize(
        &self,
        _encrypted_data: &[u8],
        key_resolution: &KeyFileResolution,
        enc_path: &str,
        options: &DixLoadOptions,
    ) -> Result<DixData, String> {
        let key_data = self.parse_key_file_content(&key_resolution.content)?;

        if key_data.is_password_mode && options.password.is_none() {
            let msg = "Password required for decryption. \
                       Rust API: use DixLoadOptions::with_password(). \
                       FFI callers: use mdix_load_encrypted_password() instead of mdix_load_encrypted().";
            self.error_manager.add_runtime_error(
                RuntimeErrorType::InvalidOperation,
                msg.to_string(),
                Some("DixLoader.decrypt_and_deserialize".to_string()),
                0, 0, vec![],
                Some("Provide the password used during compilation.".to_string()),
            );
            return Err(msg.to_string());
        }

        if key_data.is_password_mode {
            self.error_manager.log_info("Using password-based decryption");
        } else {
            self.error_manager.log_info("Using keyfile-based decryption");
        }

        let binary_data = self.execute_reverse_pipeline(enc_path, key_resolution, options)?;

        self.error_manager.log_info(&format!("Decrypted data size: {} bytes", binary_data.len()));

        let mut unpacker    = BinaryUnpacker::new();
        let deser_result    = unpacker.unpack(&binary_data);

        if !deser_result.is_success {
            let msg = format!("Binary deserialization failed: {:?}", deser_result.errors);
            self.error_manager.add_runtime_error(
                RuntimeErrorType::InvalidOperation,
                msg.clone(),
                Some("DixLoader.decrypt_and_deserialize".to_string()),
                0, 0, vec![], None,
            );
            return Err(msg);
        }

        let ast = deser_result
            .ast
            .ok_or_else(|| "Deserialization succeeded but AST is None".to_string())?;

        let is_encrypted  = key_data.applied_modules.iter().any(|m| m.contains("Encryptor"));
        let is_compressed = key_data.applied_modules.iter().any(|m| m.contains("Compressor"));

        self.error_manager.log_info("Encrypted data loaded successfully");

        Ok(DixData::from_ast(
            ast,
            key_data.version,
            key_data.compile_time,
            is_encrypted,
            is_compressed,
            key_data.applied_modules,
        ))
    }

    // ── Compilation pipeline (Approach B: tokenizer-first) ────────────────────

    fn compile_source(
        &self,
        source_text: &str,
        source_file_path: &str,
    ) -> Result<DixScript, String> {
        // Stage 1: tokenize the full source with minimal initial settings.
        let initial_settings = OperationalSettings {
            source_file_path: Some(source_file_path.to_string()),
            ..OperationalSettings::default()
        };

        let tokenizer  = Tokenizer::new(source_text, &initial_settings);
        let tok_result = tokenizer.tokenize();

        // Stage 2: split @CONFIG tokens from the rest of the stream.
        let split = split_config_tokens(tok_result.tokens);

        // Stage 3: process @CONFIG to derive real operational settings.
        let mut config_handler = ConfigSectionHandler::new(None);
        let config_result      = config_handler.process_config_tokens(&split.config_tokens);

        let mut operational_settings = config_result.operational_settings;
        operational_settings.source_file_path = Some(source_file_path.to_string());
        self.error_manager.update_settings(operational_settings.clone());

        // Stage 4: parse the rest of the token stream.
        let parser = GeneralParser::new(
            split.rest_tokens,
            &config_result.config_section,
            &operational_settings,
        ).map_err(|e| format!("Parser init failed: {}", e.message()))?;

        let ast = parser.parse()
            .map_err(|e| format!("Parse failed: {}", e.message()))?;

        // Stage 5: semantic analysis.
        let semantic_analyzer = GeneralSemanticAnalyzer::new(&ast, &operational_settings);
        let semantic_result   = semantic_analyzer.analyze();

        if !semantic_result.is_success {
            let msgs: Vec<String> = semantic_result.errors.iter()
                .map(|e| e.message.clone())
                .collect();
            return Err(format!("Semantic analysis failed: {:?}", msgs));
        }

        // Stage 6: AST enhancement.
        self.error_manager.log_info("Running AST enhancement");
        let enhancer           = GeneralAstEnhancer::new(&operational_settings);
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

        // Stage 7: value resolution (only when there are functions and data).
        let has_local_functions    = resolved_ast.quick_functions.is_some();
        let has_imported_functions = semantic_result.symbol_table.as_ref()
            .map(|st| st.namespaces.values().any(|ns| !ns.functions.is_empty()))
            .unwrap_or(false);
        let has_data_section = resolved_ast.data.is_some();

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
                let msgs: Vec<String> = resolution_result.errors.iter().cloned().collect();
                return Err(format!("Value resolution failed: {:?}", msgs));
            }

            self.error_manager.log_info(&format!(
                "Value resolution complete: {} calls resolved",
                resolution_result.function_calls_resolved
            ));

            resolved_ast = resolution_result
                .resolved_ast
                .ok_or_else(|| "Resolution succeeded but AST is None".to_string())?;
        } else {
            self.error_manager.log_info("Skipping value resolution (no functions or no data)");
        }

        // Stage 8: numeric array homogenization.
        //
        // QuickFunc-resolved array literals can end up with mixed numeric
        // element types (e.g. `[12.3, 4, 4.9]` where `4` stayed an Integer
        // because the original literal/expression was an int). This pass
        // promotes every element of a numeric array to the highest-precision
        // type present in that array, so the binary serializer, JSON/TOML
        // converters, and the LSP "Create Resolved" output all see a
        // consistent element type. Runs regardless of whether value
        // resolution happened — a hand-written `[12.3, 4, 4.9]` literal gets
        // the same treatment.
        homogenize_data_section(&mut resolved_ast);

        Ok(resolved_ast)
    }

    // ── DLM dispatch ──────────────────────────────────────────────────────────

    fn determine_dlm_behavior(
        &self,
        ast: &DixScript,
        source_file_path: &str,
        options: &DixLoadOptions,
    ) -> Result<DLMFileGeneration, String> {
        let mut result = DLMFileGeneration {
            resolved_ast:         ast.clone(),
            is_encrypted:         false,
            is_compressed:        false,
            applied_modules:      Vec::new(),
            generated_enc_file:   None,
            generated_key_file:   None,
            generated_audit_file: None,
        };

        let dlm_section = match ast.dlm.as_ref() {
            Some(d) if !d.modules.is_empty() => d,
            _ => {
                self.error_manager.log_info("No DLM modules - returning resolved AST only");
                return Ok(result);
            }
        };

        let has_auditor    = dlm_section.modules.iter().any(|m| m.module_type == DLMModuleType::DAuditor);
        let has_compressor = dlm_section.modules.iter().any(|m| m.module_type == DLMModuleType::DCompressor);
        let has_encryptor  = dlm_section.modules.iter().any(|m| m.module_type == DLMModuleType::DEncryptor);

        if has_auditor && !has_compressor && !has_encryptor {
            self.error_manager.log_info("DAuditor only - generating .mdix.au file");
            let audit_file = self.generate_audit_only(ast, source_file_path, options)?;
            result.applied_modules.push("DAuditor".to_string());
            result.generated_audit_file = Some(audit_file);
            return Ok(result);
        }

        if has_compressor || has_encryptor {
            self.error_manager.log_info("DCompressor/DEncryptor detected - generating binary files");

            let output_dir = options.output_directory.as_deref().unwrap_or_else(|| {
                Path::new(source_file_path)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or(".")
            });

            fs::create_dir_all(output_dir)
                .map_err(|e| format!("Failed to create output directory {}: {}", output_dir, e))?;

            let mut ast_with_security = ast.clone();
            ast_with_security.security = Some(
                SecurityUtilities::ensure_valid_security_section(
                    ast_with_security.security,
                    ast_with_security.dlm.as_ref(),
                ),
            );

            let mut packer     = BinaryPacker::new();
            let ser_result     = packer.pack(&ast_with_security);

            if !ser_result.is_success {
                return Err(format!("Binary serialization failed: {:?}", ser_result.errors));
            }

            self.error_manager.log_info(&format!(
                "Binary serialization complete: {} bytes", ser_result.binary_data.len()
            ));

            let dlm_executor = DLMPipelineExecutor::new(source_file_path, output_dir, DebugMode::Off);
            let dlm_result   = dlm_executor.execute(&mut ast_with_security, ser_result.binary_data);

            if !dlm_result.is_success {
                return Err(format!("DLM pipeline failed: {:?}", dlm_result.errors));
            }

            result.is_compressed        = has_compressor;
            result.is_encrypted         = has_encryptor;
            result.applied_modules      = dlm_result.executed_modules;
            result.generated_enc_file   = dlm_result.encrypted_file_path;
            result.generated_key_file   = dlm_result.key_file_path;
            result.generated_audit_file = dlm_result.audit_file_path;

            self.error_manager.log_info(&format!(
                "DLM pipeline complete: {} modules executed",
                result.applied_modules.len()
            ));
        }

        Ok(result)
    }

    fn generate_audit_only(
        &self,
        ast: &DixScript,
        source_file_path: &str,
        options: &DixLoadOptions,
    ) -> Result<String, String> {
        let output_dir = options.output_directory.as_deref().unwrap_or_else(|| {
            Path::new(source_file_path)
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or(".")
        });

        fs::create_dir_all(output_dir)
            .map_err(|e| format!("Failed to create output directory {}: {}", output_dir, e))?;

        let dlm_section    = ast.dlm.as_ref().unwrap();
        let auditor_module = dlm_section.modules.iter()
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
            Some(other) => return Err(format!("Unknown auditor subtype: {:?}", other)),
        };

        let audit_result = auditor
            .start_audit(ast, &[])
            .map_err(|e| format!("Failed to start audit: {}", e))?;

        auditor.finalize_audit()
            .map_err(|e| format!("Failed to finalize audit: {}", e))?;

        self.error_manager.log_info(&format!("Audit file created: {}", audit_result.audit_file_path));
        Ok(audit_result.audit_file_path)
    }

    // ── Key file parsing ──────────────────────────────────────────────────────

    fn parse_key_file_content(&self, key_content: &str) -> Result<LoaderKeyMetadata, String> {
        let temp_dir      = std::env::temp_dir();
        let temp_key_file = temp_dir.join(format!(
            "temp_mdixkey_{}.mdix.key", uuid::Uuid::new_v4()
        ));

        fs::write(&temp_key_file, key_content)
            .map_err(|e| format!("Failed to write temp key file: {}", e))?;

        let key_manager = KeyFileManager::new(
            String::new(),
            temp_dir.to_string_lossy().to_string(),
        );

        let km = key_manager
            .read_key_file(temp_key_file.to_str().unwrap_or(""))
            .map_err(|e| format!("Failed to parse key file content: {}", e))?;

        if let Err(e) = fs::remove_file(&temp_key_file) {
            self.error_manager.log_warning(&format!("Failed to clean up temp key file: {}", e));
        }

        let is_password_mode = km.key_data.encryption
            .as_ref()
            .map(|enc| enc.kdf.is_some())
            .unwrap_or(false);

        let mut applied_modules = Vec::new();
        if km.key_data.compression.is_some() {
            applied_modules.push("Compressor".to_string());
        }
        if km.key_data.encryption.is_some() {
            applied_modules.push("Encryptor".to_string());
        }
        if km.pipeline.modules_used.iter().any(|m| m.to_lowercase().contains("dauditor")) {
            applied_modules.push("Auditor".to_string());
        }

        Ok(LoaderKeyMetadata {
            version:          km.config.version,
            compile_time:     Utc::now(),
            applied_modules,
            is_password_mode,
        })
    }

    // ── Reverse pipeline ──────────────────────────────────────────────────────

    fn execute_reverse_pipeline(
        &self,
        enc_path: &str,
        resolved_key: &KeyFileResolution,
        options: &DixLoadOptions,
    ) -> Result<Vec<u8>, String> {
        let (key_file_path, using_temp) = match &resolved_key.source {
            KeyFileSource::FilePath | KeyFileSource::AutoDetected => {
                let path = resolved_key.file_path.as_ref()
                    .ok_or_else(|| "FilePath/AutoDetected source missing file_path".to_string())?;
                (path.to_string_lossy().to_string(), false)
            }
            KeyFileSource::DirectContent | KeyFileSource::Url => {
                let temp_dir      = std::env::temp_dir();
                let temp_key_path = temp_dir.join(format!(
                    "temp_key_{}.mdix.key", uuid::Uuid::new_v4()
                ));
                fs::write(&temp_key_path, &resolved_key.content)
                    .map_err(|e| format!("Failed to write temp key file: {}", e))?;
                self.error_manager.log_info(&format!(
                    "Created temporary key file: {}", temp_key_path.display()
                ));
                (temp_key_path.to_string_lossy().to_string(), true)
            }
        };

        let reverse_executor = DLMReverseExecutor::new(
            enc_path,
            &key_file_path,
            options.password.clone(),
            DebugMode::Off,
        );

        let reverse_result = reverse_executor.execute();

        if using_temp {
            if let Err(e) = fs::remove_file(&key_file_path) {
                self.error_manager.log_warning(&format!(
                    "Failed to delete temp key file '{}': {}", key_file_path, e
                ));
            } else {
                self.error_manager.log_info("Temporary key file cleaned up");
            }
        }

        if !reverse_result.is_success {
            return Err(format!("Reverse pipeline failed: {:?}", reverse_result.errors));
        }

        Ok(reverse_result.restored_data)
    }

    // ── Utility ───────────────────────────────────────────────────────────────

    fn log_generated_files(&self, file_gen: &DLMFileGeneration) {
        if let Some(ref path) = file_gen.generated_enc_file {
            self.error_manager.log_info(&format!("Generated encrypted file: {}", path));
        }
        if let Some(ref path) = file_gen.generated_key_file {
            self.error_manager.log_info(&format!("Generated key file: {}", path));
        }
        if let Some(ref path) = file_gen.generated_audit_file {
            self.error_manager.log_info(&format!("Generated audit file: {}", path));
        }
    }
}

impl Default for DixLoader {
    fn default() -> Self {
        Self::new()
    }
}

// ── Internal result types ─────────────────────────────────────────────────────

struct DLMFileGeneration {
    resolved_ast:         DixScript,
    is_encrypted:         bool,
    is_compressed:        bool,
    applied_modules:      Vec<String>,
    generated_enc_file:   Option<String>,
    generated_key_file:   Option<String>,
    generated_audit_file: Option<String>,
}

struct LoaderKeyMetadata {
    version:          String,
    compile_time:     chrono::DateTime<Utc>,
    applied_modules:  Vec<String>,
    is_password_mode: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loader_creation() {
        let loader = DixLoader::new();
        assert!(!loader.error_manager.has_errors());
    }

    #[test]
    fn test_load_from_str_empty_fails() {
        let loader = DixLoader::new();
        let result = loader.load_from_str("", &DixLoadOptions::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_encrypted_bytes_empty_fails() {
        let loader = DixLoader::new();
        let result = loader.load_from_encrypted_bytes(
            &[], "some_key_content", &DixLoadOptions::new()
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_sequential_loads_dont_accumulate_errors() {
        let loader = DixLoader::new();
        let _ = loader.load_text("nonexistent_a.mdix", &DixLoadOptions::new());
        let _ = loader.load_text("nonexistent_b.mdix", &DixLoadOptions::new());
        let errors = loader.error_manager.get_runtime_errors();
        assert_eq!(errors.len(), 1, "only the most recent load's error should remain");
    }
           }
