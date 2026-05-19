// dixscript/src/Runtime/dix_loader.rs
//! DixLoader — compile and load `.mdix` files.
//!
//! ## Pipeline (Approach B — tokenizer-first)
//!
//!   Tokenizer (full source, no stripping)
//!       ↓
//!   split_config_tokens
//!       ├─ config_tokens → ConfigSectionHandler::process_config_tokens
//!       │       ↓  ConfigSection AST + real OperationalSettings
//!       └─ rest_tokens ──────────────────────────────────────────┐
//!                                                                 ↓
//!                                                        GeneralParser
//!                                                                 ↓
//!                                                  GeneralSemanticAnalyzer
//!                                                                 ↓
//!                                                    GeneralAstEnhancer
//!                                                                 ↓
//!                                                      ValueResolver
//!                                                                 ↓
//!                                                     DLM pipeline (opt)
//!                                                                 ↓
//!                                                    BinarySerializer (opt)
//!                                                                 ↓
//!                                                            DixData

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::Compiler::Core::{
    ConfigSectionHandler, ErrorHandlingStrategy, GeneralAstEnhancer,
    GeneralParser, GeneralSemanticAnalyzer, OperationalSettings,
};
use crate::Compiler::Core::Tokenizer::{Tokenizer, split_config_tokens};
use crate::Compiler::AST::data_types::DebugMode;
use crate::Compiler::DLM::DlmPipeline;
use crate::Compiler::Core::ValueResolution::ValueResolver;
use crate::Compiler::Core::BinarySerialization::{BinarySerializer, BinaryDeserializer};
use crate::ErrorManager::{ErrorManager, ErrorHandlingStrategy as EHStrategy};
use crate::Runtime::{
    DixData, DixLoadOptions, DixLoadOptionsMode, KeyResolver,
};

// ── Load options ──────────────────────────────────────────────────────────────

/// Configuration for a load operation.
#[derive(Debug, Clone)]
pub struct DixLoadOptions {
    pub mode:             DixLoadOptionsMode,
    pub output_directory: Option<String>,
    pub key_search_paths: Vec<String>,
}

impl DixLoadOptions {
    pub fn new() -> Self {
        DixLoadOptions {
            mode:             DixLoadOptionsMode::Plain,
            output_directory: None,
            key_search_paths: Vec::new(),
        }
    }

    pub fn with_password(password: &str) -> Self {
        DixLoadOptions {
            mode:             DixLoadOptionsMode::Password(password.to_string()),
            output_directory: None,
            key_search_paths: Vec::new(),
        }
    }

    pub fn with_key_file(path: &str) -> Self {
        DixLoadOptions {
            mode:             DixLoadOptionsMode::KeyFile(path.to_string()),
            output_directory: None,
            key_search_paths: Vec::new(),
        }
    }

    pub fn with_key_content(
        key_content: String,
        _acknowledge_security_risk: bool,
    ) -> Result<Self, String> {
        Ok(DixLoadOptions {
            mode:             DixLoadOptionsMode::KeyContent(key_content),
            output_directory: None,
            key_search_paths: Vec::new(),
        })
    }

    pub fn with_key_url(
        url: &str,
        _acknowledge_security_risk: bool,
    ) -> Result<Self, String> {
        Ok(DixLoadOptions {
            mode:             DixLoadOptionsMode::KeyUrl(url.to_string()),
            output_directory: None,
            key_search_paths: Vec::new(),
        })
    }

    pub fn with_output_directory(dir: &str) -> Self {
        DixLoadOptions {
            mode:             DixLoadOptionsMode::Plain,
            output_directory: Some(dir.to_string()),
            key_search_paths: Vec::new(),
        }
    }

    pub fn with_key_search_paths(paths: Vec<String>) -> Self {
        DixLoadOptions {
            mode:             DixLoadOptionsMode::Plain,
            output_directory: None,
            key_search_paths: paths,
        }
    }
}

impl Default for DixLoadOptions {
    fn default() -> Self { Self::new() }
}

/// How the key / password for an encrypted file is supplied.
#[derive(Debug, Clone)]
pub enum DixLoadOptionsMode {
    Plain,
    Password(String),
    KeyFile(String),
    KeyContent(String),
    KeyUrl(String),
}

// ── DixLoader ─────────────────────────────────────────────────────────────────

pub struct DixLoader {
    error_manager: ErrorManager,
}

impl DixLoader {
    pub fn new() -> Self {
        DixLoader {
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Create a loader with an isolated ErrorManager — useful in tests or when
    /// running multiple loaders concurrently without shared error state.
    pub fn new_isolated() -> Self {
        DixLoader {
            error_manager: ErrorManager::new_isolated(),
        }
    }

    // ── Public load API ───────────────────────────────────────────────────────

    /// Load a plain `.mdix` file from disk.
    pub fn load_text(
        &self,
        path: &str,
        options: &DixLoadOptions,
    ) -> Result<DixData, String> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read '{}': {}", path, e))?;
        self.compile_source(&source, Some(path), options)
    }

    /// Load a `.mdix` source from a string — useful for Unity TextAssets,
    /// embedded configs, or tests.
    pub fn load_from_str(
        &self,
        source: &str,
        options: &DixLoadOptions,
    ) -> Result<DixData, String> {
        self.compile_source(source, None, options)
    }

    /// Load and decrypt an `.mdix.enc` file from disk.
    pub fn load_encrypted(
        &self,
        path: &str,
        options: &DixLoadOptions,
    ) -> Result<DixData, String> {
        let encrypted_bytes = std::fs::read(path)
            .map_err(|e| format!("Failed to read encrypted file '{}': {}", path, e))?;

        let key_bytes = self.resolve_key_for_file(path, options)?;
        let source    = self.decrypt_bytes(&encrypted_bytes, &key_bytes)?;

        self.compile_source(&source, Some(path), options)
    }

    /// Load from encrypted bytes already in memory — for platforms without
    /// filesystem access (Android, WASM asset bundles, etc.).
    pub fn load_from_encrypted_bytes(
        &self,
        encrypted_bytes: &[u8],
        key_content:     &str,
        options:         &DixLoadOptions,
    ) -> Result<DixData, String> {
        let key_bytes = KeyResolver::resolve_from_key_content(key_content)?;
        let source    = self.decrypt_bytes(encrypted_bytes, &key_bytes)?;
        self.compile_source(&source, None, options)
    }

    // ── Core pipeline (Approach B) ────────────────────────────────────────────

    /// Full compilation pipeline — tokenizer-first, config tokens split before
    /// the parser so @CONFIG positions are accurate with no source stripping.
    fn compile_source(
        &self,
        source:      &str,
        source_path: Option<&str>,
        options:     &DixLoadOptions,
    ) -> Result<DixData, String> {
        // Each load gets an isolated error manager so concurrent loads never
        // share error state.
        let em = ErrorManager::new_isolated();

        // Force Continue for the initial pass; will be updated by config
        // processing below.
        em.force_strategy(ErrorHandlingStrategy::Continue);

        // ── Stage 1: tokenize full source ─────────────────────────────────
        // Safe defaults for the tokenizer pass — real settings come from config.
        let initial_settings = OperationalSettings {
            error_handling_strategy: ErrorHandlingStrategy::Continue,
            debug_mode:              DebugMode::Off,
            source_file_path:        source_path.map(|p| p.to_string()),
            ..OperationalSettings::default()
        };

        let tokenizer = Tokenizer::new_with_error_manager(
            source,
            &initial_settings,
            em.clone(),
        );
        let token_result = tokenizer.tokenize();

        if em.has_fatal_errors() {
            return Err(format!(
                "Lexer errors:\n{}",
                em.get_lexical_errors_as_string()
            ));
        }

        // ── Stage 2: extract @CONFIG and derive real OpSettings ───────────
        let split = split_config_tokens(token_result.tokens);

        let config_result = {
            let mut handler =
                ConfigSectionHandler::new_with_error_manager(None, em.clone());
            handler.process_config_tokens(&split.config_tokens)
        };

        let mut operational_settings = config_result.operational_settings;
        if let Some(path) = source_path {
            operational_settings.source_file_path = Some(path.to_string());
        }

        // Now that we have real settings, re-apply them to the error manager.
        em.update_settings(operational_settings.clone());

        // ── Stage 3: parse ────────────────────────────────────────────────
        let parser = GeneralParser::new(
            split.rest_tokens,
            &config_result.config_section,
            &operational_settings,
        )
        .map_err(|e| format!("Parser initialisation error: {}", e.message()))?;

        let ast = parser
            .parse()
            .map_err(|e| format!("Parse error: {}", e.message()))?;

        if em.has_fatal_errors() {
            return Err(format!(
                "Parse errors:\n{}",
                em.get_parse_errors_as_string()
            ));
        }

        // ── Stage 4: semantic analysis ────────────────────────────────────
        let analyzer =
            GeneralSemanticAnalyzer::new(&ast, &operational_settings);
        let semantic_result = analyzer.analyze();

        if !semantic_result.is_success
            && operational_settings.error_handling_strategy
                == ErrorHandlingStrategy::Halt
        {
            let errors: Vec<String> =
                semantic_result.errors.iter().map(|e| e.message.clone()).collect();
            return Err(format!(
                "Semantic analysis failed:\n{}",
                errors.join("\n")
            ));
        }

        // ── Stage 5: AST enhancement ──────────────────────────────────────
        let enhancer =
            GeneralAstEnhancer::new(&operational_settings);
        let enhancement_result =
            enhancer.enhance(&ast, Some(&semantic_result));

        // ── Stage 6: value resolution ─────────────────────────────────────
        let mut resolver = ValueResolver::new(
            &enhancement_result.enhanced_ast,
            &operational_settings,
        );
        let resolved = resolver
            .resolve()
            .map_err(|e| format!("Value resolution failed: {}", e))?;

        if em.has_fatal_errors() {
            return Err(format!(
                "Value resolution errors:\n{}",
                em.get_value_resolution_errors_as_string()
            ));
        }

        // ── Stage 7: build DixData ────────────────────────────────────────
        let config_map: Option<HashMap<String, String>> =
            enhancement_result.enhanced_ast.config.as_ref().map(|cfg| {
                cfg.entries
                    .iter()
                    .map(|e| (e.key.clone(), e.value.to_string()))
                    .collect()
            });

        let data = DixData::from_resolved(
            resolved,
            config_map,
            false, // is_encrypted
            false, // is_compressed
            semantic_result.symbol_table,
        );

        Ok(data)
    }

    // ── Compile to encrypted binary (.mdix.enc) ───────────────────────────────
    //
    // Used by `mdix compile` CLI and any caller that wants a DLM-processed
    // binary output rather than a runtime DixData.

    /// Compile a `.mdix` source to an encrypted binary file.
    ///
    /// Runs the full Approach B pipeline through value resolution, then passes
    /// the enhanced AST through the DLM pipeline (compression + encryption)
    /// and writes `.mdix.enc` + `.mdix.key` to the output directory.
    pub fn compile_to_binary(
        &self,
        source:      &str,
        source_path: Option<&str>,
        options:     &DixLoadOptions,
    ) -> Result<CompilationOutput, String> {
        let em = ErrorManager::new_isolated();
        em.force_strategy(ErrorHandlingStrategy::Continue);

        let initial_settings = OperationalSettings {
            error_handling_strategy: ErrorHandlingStrategy::Continue,
            debug_mode:              DebugMode::Off,
            source_file_path:        source_path.map(|p| p.to_string()),
            ..OperationalSettings::default()
        };

        // ── Tokenize → split → config ─────────────────────────────────────
        let tokenizer = Tokenizer::new_with_error_manager(
            source,
            &initial_settings,
            em.clone(),
        );
        let token_result = tokenizer.tokenize();

        let split = split_config_tokens(token_result.tokens);

        let config_result = {
            let mut handler =
                ConfigSectionHandler::new_with_error_manager(None, em.clone());
            handler.process_config_tokens(&split.config_tokens)
        };

        let mut operational_settings = config_result.operational_settings;
        if let Some(path) = source_path {
            operational_settings.source_file_path = Some(path.to_string());
        }
        em.update_settings(operational_settings.clone());

        // ── Parse → semantic → enhance ────────────────────────────────────
        let parser = GeneralParser::new(
            split.rest_tokens,
            &config_result.config_section,
            &operational_settings,
        )
        .map_err(|e| format!("Parser error: {}", e.message()))?;

        let ast = parser
            .parse()
            .map_err(|e| format!("Parse error: {}", e.message()))?;

        let analyzer =
            GeneralSemanticAnalyzer::new(&ast, &operational_settings);
        let semantic_result = analyzer.analyze();

        if !semantic_result.is_success
            && operational_settings.error_handling_strategy
                == ErrorHandlingStrategy::Halt
        {
            let errors: Vec<String> =
                semantic_result.errors.iter().map(|e| e.message.clone()).collect();
            return Err(format!(
                "Semantic errors:\n{}",
                errors.join("\n")
            ));
        }

        let enhancer = GeneralAstEnhancer::new(&operational_settings);
        let enhancement_result =
            enhancer.enhance(&ast, Some(&semantic_result));

        // ── Value resolution ──────────────────────────────────────────────
        let mut resolver = ValueResolver::new(
            &enhancement_result.enhanced_ast,
            &operational_settings,
        );
        let resolved = resolver
            .resolve()
            .map_err(|e| format!("Value resolution failed: {}", e))?;

        // ── DLM pipeline (compression + encryption) ───────────────────────
        let dlm_result = if let Some(dlm) = enhancement_result.enhanced_ast.dlm.as_ref() {
            let mut pipeline =
                DlmPipeline::new(dlm, &operational_settings, em.clone());
            Some(
                pipeline
                    .process(&resolved)
                    .map_err(|e| format!("DLM pipeline error: {}", e))?,
            )
        } else {
            None
        };

        // ── Binary serialization ──────────────────────────────────────────
        let output_dir = options
            .output_directory
            .as_deref()
            .unwrap_or(".");

        let serializer = BinarySerializer::new(&operational_settings, em.clone());
        let serialized = serializer
            .serialize(
                &enhancement_result.enhanced_ast,
                &resolved,
                dlm_result.as_ref(),
            )
            .map_err(|e| format!("Serialization error: {}", e))?;

        // Determine output paths
        let base_name = source_path
            .and_then(|p| Path::new(p).file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("output");

        let enc_path = format!("{}/{}.mdix.enc", output_dir, base_name);
        let key_path = dlm_result
            .as_ref()
            .filter(|_| serialized.key_bytes.is_some())
            .map(|_| format!("{}/{}.mdix.key", output_dir, base_name));

        std::fs::write(&enc_path, &serialized.enc_bytes)
            .map_err(|e| format!("Failed to write '{}': {}", enc_path, e))?;

        if let (Some(ref kp), Some(ref kb)) = (&key_path, &serialized.key_bytes) {
            std::fs::write(kp, kb)
                .map_err(|e| format!("Failed to write '{}': {}", kp, e))?;
        }

        Ok(CompilationOutput {
            enc_path,
            key_path,
            warnings: em
                .get_all_errors_flat()
                .iter()
                .map(|e| e.message().to_string())
                .collect(),
        })
    }

    // ── Decryption helpers ────────────────────────────────────────────────────

    fn resolve_key_for_file(
        &self,
        enc_path: &str,
        options:  &DixLoadOptions,
    ) -> Result<Vec<u8>, String> {
        match &options.mode {
            DixLoadOptionsMode::Plain => {
                // Auto-detect: look for <name>.mdix.key alongside the .enc file.
                let key_path = enc_path.replace(".mdix.enc", ".mdix.key");
                if Path::new(&key_path).exists() {
                    let key_content = std::fs::read_to_string(&key_path)
                        .map_err(|e| format!("Failed to read key file '{}': {}", key_path, e))?;
                    return KeyResolver::resolve_from_key_content(&key_content);
                }
                // Search additional paths.
                for search_dir in &options.key_search_paths {
                    let base = Path::new(enc_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("output");
                    let candidate = format!(
                        "{}/{}.key",
                        search_dir,
                        base.replace(".mdix.enc", "")
                    );
                    if Path::new(&candidate).exists() {
                        let key_content = std::fs::read_to_string(&candidate)
                            .map_err(|e| format!("Key read error: {}", e))?;
                        return KeyResolver::resolve_from_key_content(&key_content);
                    }
                }
                Err(format!(
                    "No key file found for '{}'. Use DixLoadOptions::with_key_file or with_password.",
                    enc_path
                ))
            }
            DixLoadOptionsMode::Password(pw) => {
                KeyResolver::derive_from_password(pw)
            }
            DixLoadOptionsMode::KeyFile(kf) => {
                let key_content = std::fs::read_to_string(kf)
                    .map_err(|e| format!("Failed to read key file '{}': {}", kf, e))?;
                KeyResolver::resolve_from_key_content(&key_content)
            }
            DixLoadOptionsMode::KeyContent(kc) => {
                KeyResolver::resolve_from_key_content(kc)
            }
            DixLoadOptionsMode::KeyUrl(url) => {
                KeyResolver::resolve_from_url(url)
            }
        }
    }

    fn decrypt_bytes(
        &self,
        encrypted_bytes: &[u8],
        key_bytes:       &[u8],
    ) -> Result<String, String> {
        let deserializer = BinaryDeserializer::new();
        deserializer
            .decrypt_to_source(encrypted_bytes, key_bytes)
            .map_err(|e| format!("Decryption failed: {}", e))
    }
}

impl Default for DixLoader {
    fn default() -> Self { Self::new() }
}

// ── Output type ───────────────────────────────────────────────────────────────

/// Result of a `compile_to_binary` call.
#[derive(Debug, Clone)]
pub struct CompilationOutput {
    /// Path to the written `.mdix.enc` file.
    pub enc_path: String,
    /// Path to the written `.mdix.key` file, if encryption was applied.
    pub key_path: Option<String>,
    /// Non-fatal warnings collected during compilation.
    pub warnings: Vec<String>,
}
