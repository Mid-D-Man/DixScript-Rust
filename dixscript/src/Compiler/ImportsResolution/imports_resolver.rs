// src/Compiler/ImportsResolution/imports_resolver.rs
//! Recursive import resolution with cycle detection and optional cloud download.
//!
//! Cloud download support requires the `cloud_imports` cargo feature.
//! Local file imports work without any optional features.
//!
//! ## Cycle detection
//!
//! The resolver keeps two path sets — `visiting` (currently on the call stack)
//! and `visited` (fully processed).  A cycle is detected when
//! `resolve_import_recursive` is entered for a path that is already in
//! `visiting`.
//!
//! **Critical invariant:** `parse_imported_file` must set
//! `skip_imports_resolution = true` in the settings it passes to
//! `GeneralSemanticAnalyzer`.  If that flag were `false` the semantic analyser
//! would spin up a *new* `ImportsResolver` instance that has no knowledge of
//! the outer resolver's `visiting` set.  That new instance would recurse
//! indefinitely on any cycle, overflowing the stack before the outer guard
//! ever fires.  By skipping resolution inside the semantic analyser we ensure
//! that ALL recursive resolution goes through this resolver's single
//! `resolve_import_recursive` entry-point where the cycle guard lives.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::fs;
use crate::Compiler::AST::*;
use crate::Compiler::Core::{
    ConfigSectionHandler,
    DebugMode,
    ErrorHandlingStrategy,
    GeneralAstEnhancer,
    GeneralParser,
    GeneralSemanticAnalyzer,
    OperationalSettings,
};
use crate::Compiler::Core::Tokenizer::Tokenizer;
use crate::Compiler::Utilities::{FunctionSignature, ParameterInfo, QuickFunctionInfo, SymbolTable};
use crate::ErrorManager::{DebugConfig, ErrorManager, ImportsResolutionErrorType};
use super::HashVerifier;

#[cfg(feature = "cloud_imports")]
use super::{CloudFileCache, CloudProviderFactory};

pub struct ImportsResolver<'a> {
    symbol_table: &'a mut SymbolTable,
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
    #[cfg(feature = "cloud_imports")]
    cloud_cache: CloudFileCache,
    visiting: HashSet<String>,
    visited: HashSet<String>,
    import_stack: Vec<String>,
}

impl<'a> ImportsResolver<'a> {
    pub fn new(
        symbol_table: &'a mut SymbolTable,
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let debug_config = DebugConfig::from_debug_mode(error_manager.get_debug_mode());

        #[cfg(feature = "cloud_imports")]
        let cloud_cache = CloudFileCache::new(error_manager.clone());

        ImportsResolver {
            symbol_table,
            operational_settings,
            error_manager,
            debug_config,
            #[cfg(feature = "cloud_imports")]
            cloud_cache,
            visiting: HashSet::new(),
            visited: HashSet::new(),
            import_stack: Vec::new(),
        }
    }

    // ── Primary entry point ───────────────────────────────────────────────────
    //
    // Called by GeneralSemanticAnalyzer Phase 3. Reads each import declaration
    // from the already-parsed @IMPORTS section, loads + parses the file from
    // disk (or cloud), and registers its symbols in the symbol table.
    //
    // `base_dir` is the directory containing the file that owns the @IMPORTS
    // section (used to resolve relative paths).

    pub fn resolve_from_imports_section(
        &mut self,
        imports_section: &ImportsSection,
        base_dir: &str,
    ) -> bool {
        if imports_section.imports.is_empty() {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug("[ImportsResolver] No imports to resolve");
            }
            return true;
        }

        self.error_manager.log_info(&format!(
            "[ImportsResolver] Resolving {} top-level import(s) from '{}'",
            imports_section.imports.len(),
            base_dir,
        ));

        self.visiting.clear();
        self.visited.clear();
        self.import_stack.clear();

        let mut success = true;

        for import in &imports_section.imports {
            let resolved_path = if import.is_cloud_import {
                import.path.clone()
            } else {
                Self::resolve_path(base_dir, &import.path)
            };

            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[ImportsResolver] Loading '{}' from '{}'",
                    import.alias, resolved_path,
                ));
            }

            // Parse the file (tokenise → parse → semantic → enhance).
            let ast = match self.parse_imported_file(import, &resolved_path) {
                Ok(a)  => a,
                Err(e) => {
                    self.error_manager.log_error(&format!(
                        "[ImportsResolver] Failed to load '{}': {}",
                        import.alias, e,
                    ));
                    if self.operational_settings.error_handling_strategy
                        == ErrorHandlingStrategy::Halt
                    {
                        return false;
                    }
                    success = false;
                    continue;
                }
            };

            // Recursively resolve (handles transitive imports + cycle detection).
            if !self.resolve_import_recursive(&import.alias, &ast, &resolved_path) {
                self.error_manager.log_error(&format!(
                    "[ImportsResolver] Failed to resolve '{}'",
                    import.alias,
                ));
                if self.operational_settings.error_handling_strategy
                    == ErrorHandlingStrategy::Halt
                {
                    return false;
                }
                success = false;
            }
        }

        let error_count = self.error_manager.get_imports_resolution_errors().len();
        if error_count > 0 {
            self.error_manager.log_warning(&format!(
                "[ImportsResolver] Resolution completed with {} error(s)",
                error_count,
            ));
            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
                return false;
            }
        } else if self.debug_config.is_enabled {
            self.error_manager
                .log_info("[ImportsResolver] All top-level imports resolved successfully");
        }

        success
    }

    // ── Legacy entry point (kept for backward compatibility) ──────────────────
    //
    // Accepts a map of already-parsed ASTs. Still used by tests that build the
    // map manually; the semantic analyzer now uses resolve_from_imports_section.

    pub fn resolve_imports(
        &mut self,
        parsed_imports: &HashMap<String, (String, DixScript)>,
    ) -> bool {
        if parsed_imports.is_empty() {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug("[ImportsResolver] No pre-parsed imports to resolve");
            }
            return true;
        }

        self.error_manager.log_info(&format!(
            "[ImportsResolver] Resolving {} pre-parsed import(s)",
            parsed_imports.len()
        ));

        #[cfg(feature = "cloud_imports")]
        if self.debug_config.is_enabled {
            let stats = self.cloud_cache.get_statistics();
            self.error_manager.log_debug(&format!("[ImportsResolver] Cache statistics: {}", stats));
        }

        self.visiting.clear();
        self.visited.clear();
        self.import_stack.clear();

        let mut success = true;

        for (alias, (absolute_path, ast)) in parsed_imports {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[ImportsResolver] Resolving import '{}' from '{}'",
                    alias, absolute_path
                ));
            }

            if !self.resolve_import_recursive(alias, ast, absolute_path) {
                self.error_manager.log_error(&format!(
                    "[ImportsResolver] Failed to resolve import '{}'",
                    alias
                ));

                if self.operational_settings.error_handling_strategy
                    == ErrorHandlingStrategy::Halt
                {
                    return false;
                }
                success = false;
            }
        }

        let error_count = self.error_manager.get_imports_resolution_errors().len();
        if error_count > 0 {
            self.error_manager.log_warning(&format!(
                "[ImportsResolver] Resolution completed with {} errors",
                error_count
            ));
            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
                return false;
            }
        } else {
            self.error_manager
                .log_info("[ImportsResolver] Successfully resolved imports");
        }

        success
    }

    fn resolve_import_recursive(
        &mut self,
        alias: &str,
        ast: &DixScript,
        absolute_path: &str,
    ) -> bool {
        let normalized_path = if Self::is_cloud_url(absolute_path) {
            Self::strip_query_parameters(absolute_path)
        } else {
            match std::fs::canonicalize(absolute_path) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(_) => absolute_path.to_string(),
            }
        };

        // ── Cycle guard ───────────────────────────────────────────────────────
        // If this path is already on the call stack we have a circular
        // dependency.  Report it and return false immediately — no further
        // recursion.
        if self.visiting.contains(&normalized_path) {
            let cycle_path = self.build_cycle_path(&normalized_path);
            let cycle_chain = self.build_cycle_chain_list(&normalized_path);

            self.error_manager.add_imports_resolution_error(
                ImportsResolutionErrorType::CircularDependency,
                format!("Circular dependency detected: {}", cycle_path),
                alias.to_string(),
                Some(absolute_path.to_string()),
                Some(normalized_path.clone()),
                Some(cycle_chain),
                0,
                0,
                None,
            );
            return false;
        }

        // Already fully processed in a prior branch — skip without re-doing work.
        if self.visited.contains(&normalized_path) {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[ImportsResolver] Import '{}' already resolved, skipping",
                    alias
                ));
            }
            return true;
        }

        // Mark as in-progress BEFORE recursing so nested calls see it.
        self.visiting.insert(normalized_path.clone());
        self.import_stack.push(normalized_path.clone());

        let result = self.resolve_import_inner(alias, ast, &normalized_path);

        self.import_stack.pop();
        self.visiting.remove(&normalized_path);
        self.visited.insert(normalized_path);

        result
    }

    fn resolve_import_inner(
        &mut self,
        alias: &str,
        ast: &DixScript,
        normalized_path: &str,
    ) -> bool {
        let mut local_imports = HashMap::new();

        if let Some(ref imports_section) = ast.imports {
            let nested_base_dir = if Self::is_cloud_url(normalized_path) {
                Self::get_cloud_url_directory(normalized_path)
            } else {
                Path::new(normalized_path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".to_string())
            };

            let nested_imports: Vec<ImportDeclaration> = imports_section.imports.clone();

            for nested_import in &nested_imports {
                let nested_path = if nested_import.is_cloud_import {
                    nested_import.path.clone()
                } else {
                    Self::resolve_path(&nested_base_dir, &nested_import.path)
                };

                if self.visited.contains(&nested_path) {
                    if let Some(existing_ns) =
                        self.symbol_table.try_get_namespace(&nested_import.alias)
                    {
                        local_imports
                            .insert(nested_import.alias.clone(), existing_ns.clone());
                        continue;
                    } else {
                        self.error_manager.add_imports_resolution_error(
                            ImportsResolutionErrorType::GeneralError,
                            format!(
                                "Internal: namespace '{}' marked visited but absent from symbol table",
                                nested_import.alias
                            ),
                            nested_import.alias.clone(),
                            Some(nested_import.path.clone()),
                            Some(nested_path.clone()),
                            None,
                            0,
                            0,
                            None,
                        );
                        return false;
                    }
                }

                // ── Early cycle check before parsing ─────────────────────────
                // Normalise the nested path and check the visiting set *before*
                // calling parse_imported_file.  This is the second line of
                // defence: parse_imported_file itself does not resolve imports
                // (skip_imports_resolution = true), but if for any reason the
                // path is already on the stack we catch it here without even
                // opening the file.
                let normalized_nested = if Self::is_cloud_url(&nested_path) {
                    Self::strip_query_parameters(&nested_path)
                } else {
                    match std::fs::canonicalize(&nested_path) {
                        Ok(p) => p.to_string_lossy().to_string(),
                        Err(_) => nested_path.clone(),
                    }
                };

                if self.visiting.contains(&normalized_nested) {
                    let cycle_path = self.build_cycle_path(&normalized_nested);
                    let cycle_chain = self.build_cycle_chain_list(&normalized_nested);
                    self.error_manager.add_imports_resolution_error(
                        ImportsResolutionErrorType::CircularDependency,
                        format!("Circular dependency detected: {}", cycle_path),
                        nested_import.alias.clone(),
                        Some(nested_import.path.clone()),
                        Some(normalized_nested.clone()),
                        Some(cycle_chain),
                        0,
                        0,
                        None,
                    );
                    return false;
                }

                let nested_ast =
                    match self.parse_imported_file(nested_import, &nested_path) {
                        Ok(a) => a,
                        Err(_) => return false,
                    };

                if !self.resolve_import_recursive(
                    &nested_import.alias,
                    &nested_ast,
                    &nested_path,
                ) {
                    return false;
                }

                if let Some(ns) =
                    self.symbol_table.try_get_namespace(&nested_import.alias)
                {
                    local_imports.insert(nested_import.alias.clone(), ns.clone());
                }
            }
        }

        let functions = Self::extract_global_functions(ast.quick_functions.as_ref(), alias);
        let enums = Self::extract_enums(ast.enums.as_ref());

        if self.debug_config.is_enabled {
            if let Some(ref qf) = ast.quick_functions {
                let skipped = qf.functions.len().saturating_sub(functions.len());
                self.error_manager.log_debug(&format!(
                    "[ImportsResolver] Extracted {}/{} functions from '{}' ({} scoped, not exported)",
                    functions.len(),
                    qf.functions.len(),
                    alias,
                    skipped
                ));
            }
        }

        self.symbol_table.register_namespace(
            alias.to_string(),
            normalized_path.to_string(),
            functions.clone(),
            enums.clone(),
            local_imports.clone(),
        );

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[ImportsResolver] Registered namespace '{}' with {} functions, {} enums, {} local imports",
                alias,
                functions.len(),
                enums.len(),
                local_imports.len()
            ));
        }

        true
    }

    fn parse_imported_file(
        &mut self,
        import: &ImportDeclaration,
        resolved_path: &str,
    ) -> Result<DixScript, String> {
        let content = self.read_import_content(import, resolved_path)?;

        if let Some(ref verify_hash) = import.verify_hash {
            HashVerifier::verify_hash(&content, verify_hash, &import.alias, resolved_path)
                .map_err(|e| {
                    self.error_manager.add_imports_resolution_error(
                        ImportsResolutionErrorType::HashVerificationFailed,
                        e.message.clone(),
                        import.alias.clone(),
                        Some(import.path.clone()),
                        Some(resolved_path.to_string()),
                        None,
                        0,
                        0,
                        None,
                    );
                    format!("Hash verification failed: {}", e)
                })?;

            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[ImportsResolver] Hash verification passed for '{}'",
                    import.alias
                ));
            }
        }

        if content.trim().is_empty() {
            self.error_manager.log_warning(&format!(
                "[ImportsResolver] Imported file '{}' is empty",
                resolved_path
            ));
            return Ok(DixScript::new());
        }

        let mut config_handler = ConfigSectionHandler::new(None);
        let config_result = config_handler.process_config_section(&content);

        if self.debug_config.is_enabled {
            self.error_manager
                .log_debug("[ImportsResolver] Tokenizing imported file");
        }

        let tokenizer = Tokenizer::new(
            &config_result.cleaned_input_string,
            self.operational_settings,
        );
        let token_result = tokenizer.tokenize();

        if token_result.tokens.is_empty() {
            self.error_manager.add_imports_resolution_error(
                ImportsResolutionErrorType::ParseError,
                "Tokenization produced no tokens".to_string(),
                import.alias.clone(),
                Some(import.path.clone()),
                Some(resolved_path.to_string()),
                None,
                0,
                0,
                None,
            );
            return Err("Tokenization produced no tokens".to_string());
        }

        let mut import_settings = self.operational_settings.clone();
        import_settings.source_file_path = Some(resolved_path.to_string());
        // ── CRITICAL ─────────────────────────────────────────────────────────
        // Do NOT allow the semantic analyser called below to start its own
        // ImportsResolver.  That new resolver would have empty `visiting` /
        // `visited` sets and would recurse infinitely on any cycle before the
        // outer resolver's cycle guard fires, causing a stack overflow.
        //
        // All recursive import resolution for this file is handled by the
        // outer resolver's resolve_import_inner, which already iterates over
        // ast.imports and calls resolve_import_recursive (with the shared
        // `visiting` set) for every nested dependency.
        import_settings.skip_imports_resolution = true;

        if self.debug_config.is_enabled {
            self.error_manager
                .log_debug("[ImportsResolver] Parsing imported file");
        }

        let general_parser = GeneralParser::new(
            token_result.tokens,
            &config_result.config_section,
            &import_settings,
        )
            .map_err(|e| {
                self.error_manager.add_imports_resolution_error(
                    ImportsResolutionErrorType::ParseError,
                    format!("Failed to create parser: {}", e),
                    import.alias.clone(),
                    Some(import.path.clone()),
                    Some(resolved_path.to_string()),
                    None,
                    0,
                    0,
                    None,
                );
                format!("Failed to create parser: {}", e)
            })?;

        let mut ast = general_parser.parse().map_err(|e| {
            let parse_errors = self.error_manager.get_parse_errors();
            if let Some(first) = parse_errors.first() {
                self.error_manager.add_imports_resolution_error(
                    ImportsResolutionErrorType::ParseError,
                    format!("Parse errors in imported file: {}", first.message),
                    import.alias.clone(),
                    Some(import.path.clone()),
                    Some(resolved_path.to_string()),
                    None,
                    first.line as i32,
                    first.column as i32,
                    None,
                );
            }
            format!("Parse errors in imported file: {}", e)
        })?;

        ast.config = Some(config_result.config_section);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[ImportsResolver] Running semantic analysis on imported file '{}'",
                import.alias
            ));
        }

        // import_settings already has skip_imports_resolution = true, so the
        // analyser will parse and validate the file's own content but will NOT
        // try to load transitive imports.  Those are handled by
        // resolve_import_inner above.
        let semantic_analyzer = GeneralSemanticAnalyzer::new(&ast, &import_settings);
        let semantic_result = semantic_analyzer.analyze();

        if !semantic_result.is_success {
            let summary = semantic_result
                .errors
                .first()
                .map(|e| format!("{}: {}", e.error_type, e.message))
                .unwrap_or_else(|| "Unknown semantic error".to_string());

            self.error_manager.add_imports_resolution_error(
                ImportsResolutionErrorType::ParseError,
                format!(
                    "Semantic analysis failed for '{}': {} (total: {} errors)",
                    import.alias,
                    summary,
                    semantic_result.errors.len()
                ),
                import.alias.clone(),
                Some(import.path.clone()),
                Some(resolved_path.to_string()),
                None,
                0,
                0,
                None,
            );
            return Err(format!("Semantic analysis failed for '{}'", import.alias));
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[ImportsResolver] Running AST enhancement on imported file '{}'",
                import.alias
            ));
        }

        let ast_enhancer = GeneralAstEnhancer::new(&import_settings);
        let enhancement_result = ast_enhancer.enhance(&ast, Some(&semantic_result));

        if !enhancement_result.is_success {
            self.error_manager.add_imports_resolution_error(
                ImportsResolutionErrorType::ParseError,
                format!(
                    "AST enhancement failed for '{}': {} errors, {} warnings",
                    import.alias,
                    enhancement_result.errors.len(),
                    enhancement_result.warnings.len()
                ),
                import.alias.clone(),
                Some(import.path.clone()),
                Some(resolved_path.to_string()),
                None,
                0,
                0,
                None,
            );
            return Err(format!("AST enhancement failed for '{}'", import.alias));
        }

        let enhanced_ast = enhancement_result.enhanced_ast;

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[ImportsResolver] Processed imported file '{}' ({} functions)",
                import.alias,
                enhanced_ast
                    .quick_functions
                    .as_ref()
                    .map(|qf| qf.functions.len())
                    .unwrap_or(0)
            ));
        }

        Ok(enhanced_ast)
    }

    fn read_import_content(
        &mut self,
        import: &ImportDeclaration,
        resolved_path: &str,
    ) -> Result<String, String> {
        if import.is_cloud_import {
            self.read_cloud_content(import)
        } else {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[ImportsResolver] Reading local import: {}",
                    resolved_path
                ));
            }
            fs::read_to_string(resolved_path).map_err(|e| {
                self.error_manager.add_imports_resolution_error(
                    ImportsResolutionErrorType::FileNotFound,
                    format!("Failed to read file: {}", e),
                    import.alias.clone(),
                    Some(import.path.clone()),
                    Some(resolved_path.to_string()),
                    None,
                    0,
                    0,
                    None,
                );
                format!("Failed to read file: {}", e)
            })
        }
    }

    #[cfg(feature = "cloud_imports")]
    fn read_cloud_content(&mut self, import: &ImportDeclaration) -> Result<String, String> {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[ImportsResolver] Downloading cloud import: {}",
                import.path
            ));
        }
        self.download_cloud_file_sync(&import.path, &import.alias)
    }

    #[cfg(not(feature = "cloud_imports"))]
    fn read_cloud_content(&mut self, import: &ImportDeclaration) -> Result<String, String> {
        Err(format!(
            "Cloud imports require the 'cloud_imports' cargo feature. \
             Import '{}' at '{}' cannot be resolved.",
            import.alias, import.path
        ))
    }

    #[cfg(feature = "cloud_imports")]
    fn download_cloud_file_sync(
        &mut self,
        cloud_url: &str,
        alias: &str,
    ) -> Result<String, String> {
        let url_for_cache = Self::strip_query_parameters(cloud_url);

        if self.cloud_cache.is_cached(&url_for_cache) {
            if let Some(content) = self.cloud_cache.get_cached_content(&url_for_cache) {
                return Ok(content);
            }
            if self.debug_config.is_enabled {
                self.error_manager
                    .log_debug("[ImportsResolver] Cache read failed, downloading fresh copy");
            }
        }

        let provider = CloudProviderFactory::get_provider(cloud_url, &self.error_manager)
            .map_err(|e| {
                self.error_manager.add_imports_resolution_error(
                    ImportsResolutionErrorType::CloudImportNotSupported,
                    e.clone(),
                    alias.to_string(),
                    Some(cloud_url.to_string()),
                    Some(cloud_url.to_string()),
                    None,
                    0,
                    0,
                    None,
                );
                e
            })?;

        let content = tokio::runtime::Runtime::new()
            .map_err(|e| format!("Failed to create async runtime: {}", e))?
            .block_on(provider.download_file_async(cloud_url))
            .map_err(|e| {
                self.error_manager.add_imports_resolution_error(
                    ImportsResolutionErrorType::FileNotFound,
                    format!("Cloud download failed: {}", e),
                    alias.to_string(),
                    Some(cloud_url.to_string()),
                    Some(cloud_url.to_string()),
                    None,
                    0,
                    0,
                    None,
                );
                format!("Cloud download failed: {}", e)
            })?;

        self.cloud_cache.cache_file(&url_for_cache, &content);
        Ok(content)
    }

    fn extract_global_functions(
        section: Option<&QuickFuncsSection>,
        _namespace_name: &str,
    ) -> HashMap<String, QuickFunctionInfo> {
        let mut functions = HashMap::new();

        let section = match section {
            Some(s) => s,
            None => return functions,
        };

        for func in &section.functions {
            let is_global = match &func.scope_list {
                None => true,
                Some(scopes)
                if scopes.len() == 1
                    && scopes[0].eq_ignore_ascii_case("global") =>
                    {
                        true
                    }
                _ => false,
            };

            if !is_global {
                continue;
            }

            let parameters: Vec<ParameterInfo> = func
                .parameters
                .iter()
                .map(|p| ParameterInfo {
                    name: p.name.clone(),
                    param_type: p.data_type,
                    has_default_value: p.default_value.is_some(),
                    default_value: p.default_value.clone(),
                })
                .collect();

            let signature = FunctionSignature {
                name: func.name.clone(),
                return_type: func.return_type,
                parameters,
                scopes: func
                    .scope_list
                    .clone()
                    .unwrap_or_else(|| vec!["global".to_string()]),
                line: func.position.line as i32,
                column: func.position.column as i32,
            };

            functions.insert(
                func.name.clone(),
                QuickFunctionInfo { signature, ast: func.clone() },
            );
        }

        functions
    }

    fn extract_enums(
        section: Option<&EnumsSection>,
    ) -> HashMap<String, HashMap<String, i32>> {
        let mut enums = HashMap::new();

        let section = match section {
            Some(s) => s,
            None => return enums,
        };

        for enum_decl in &section.enums {
            let mut field_map = HashMap::new();
            let mut auto_value = 0i32;

            for field in &enum_decl.fields {
                if let Some(value) = field.value {
                    field_map.insert(field.name.clone(), value);
                    auto_value = value + 1;
                } else {
                    field_map.insert(field.name.clone(), auto_value);
                    auto_value += 1;
                }
            }

            enums.insert(enum_decl.name.clone(), field_map);
        }

        enums
    }

    #[inline]
    fn is_cloud_url(path: &str) -> bool {
        path.starts_with("http://") || path.starts_with("https://")
    }

    #[inline]
    fn strip_query_parameters(url: &str) -> String {
        match url.find('?') {
            Some(idx) => url[..idx].to_string(),
            None => url.to_string(),
        }
    }

    fn get_cloud_url_directory(cloud_url: &str) -> String {
        let without_query = Self::strip_query_parameters(cloud_url);
        match without_query.rfind('/') {
            Some(idx) => without_query[..=idx].to_string(),
            None => without_query,
        }
    }

    fn resolve_path(base_directory: &str, relative_path: &str) -> String {
        Path::new(base_directory)
            .join(relative_path)
            .to_string_lossy()
            .to_string()
    }

    fn extract_readable_path(path: &str) -> String {
        if Self::is_cloud_url(path) {
            path.split("://")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .unwrap_or(path)
                .to_string()
        } else {
            Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path)
                .to_string()
        }
    }

    fn build_cycle_path(&self, cycle_target: &str) -> String {
        let mut chain: Vec<String> = self
            .import_stack
            .iter()
            .map(|p| Self::extract_readable_path(p))
            .collect();
        chain.push(Self::extract_readable_path(cycle_target));
        chain.join(" -> ")
    }

    fn build_cycle_chain_list(&self, cycle_target: &str) -> Vec<String> {
        let mut chain: Vec<String> = self
            .import_stack
            .iter()
            .map(|p| Self::extract_readable_path(p))
            .collect();
        chain.push(Self::extract_readable_path(cycle_target));
        chain
    }

    pub fn get_statistics(&self) -> ImportResolutionStats {
        let total_functions: usize = self
            .symbol_table
            .namespaces
            .values()
            .map(|ns| ns.functions.len())
            .sum();

        let total_enums: usize = self
            .symbol_table
            .namespaces
            .values()
            .map(|ns| ns.enums.len())
            .sum();

        let total_local_imports: usize = self
            .symbol_table
            .namespaces
            .values()
            .map(|ns| ns.local_imports.len())
            .sum();

        ImportResolutionStats {
            total_namespaces: self.symbol_table.namespaces.len(),
            total_functions_imported: total_functions,
            total_enums_imported: total_enums,
            total_nested_imports: total_local_imports,
            files_visited: self.visited.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportResolutionStats {
    pub total_namespaces: usize,
    pub total_functions_imported: usize,
    pub total_enums_imported: usize,
    pub total_nested_imports: usize,
    pub files_visited: usize,
}

impl std::fmt::Display for ImportResolutionStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Namespaces: {}, Functions: {}, Enums: {}, Nested: {}, Files: {}",
            self.total_namespaces,
            self.total_functions_imported,
            self.total_enums_imported,
            self.total_nested_imports,
            self.files_visited
        )
    }
}
