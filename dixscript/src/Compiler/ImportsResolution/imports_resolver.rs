
//! Recursive import resolution with cycle detection and cloud download.
//!
//! ## Cycle detection
//!
//! The resolver keeps two path sets — `visiting` (currently on the call stack)
//! and `visited` (fully processed). A cycle is detected when
//! `resolve_import_recursive` is entered for a path already in `visiting`.
//!
//! ## Two-phase import processing
//!
//! Each imported file is processed in two distinct phases:
//!
//! **Phase 1 — `read_and_parse_raw`**: read file content, verify hash,
//! tokenize, parse. Returns a raw `DixScript` AST with no semantic analysis
//! or enhancement.
//!
//! **Phase 2 — `analyze_and_enhance`**: called from `resolve_import_inner`
//! AFTER all transitive dependencies have been registered in
//! `self.symbol_table`. Seeds the analyzer's internal `SymbolTable` with
//! only the namespace entries the file actually imports, then runs
//! `GeneralSemanticAnalyzer` and `GeneralAstEnhancer`.
//!
//! **Critical invariant:** `skip_imports_resolution = true` is preserved in
//! `analyze_and_enhance` to prevent the semantic analyser from spinning up a
//! new `ImportsResolver` that has no knowledge of the outer resolver's
//! `visiting` set.

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
use crate::Compiler::Utilities::symbol_table::ImportedNamespace;
use crate::ErrorManager::{DebugConfig, ErrorManager, ImportsResolutionErrorType};
use super::{HashVerifier, CloudFileCache, CloudProviderFactory};

pub struct ImportsResolver<'a> {
    symbol_table: &'a mut SymbolTable,
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
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
       Self::new_with_error_manager(symbol_table,operational_settings,ErrorManager::get_shared_instance())
    }
    pub fn new_with_error_manager(
        symbol_table: &'a mut SymbolTable,
        operational_settings: &'a OperationalSettings,
        error_manager: ErrorManager
    ) -> Self {

        let debug_config = DebugConfig::from_debug_mode(error_manager.get_debug_mode());
        let cloud_cache = CloudFileCache::new(error_manager.clone());

        ImportsResolver {
            symbol_table,
            operational_settings,
            error_manager,
            debug_config,
            cloud_cache,
            visiting: HashSet::new(),
            visited: HashSet::new(),
            import_stack: Vec::new(),
        }
    }
    // ── Primary entry point ───────────────────────────────────────────────────
    //
    // Called by GeneralSemanticAnalyzer Phase 3. Reads each import declaration
    // from the already-parsed @IMPORTS section, loads + parses the raw AST from
    // disk (or cloud), and then resolves it recursively.

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

            let raw_ast = match self.read_and_parse_raw(import, &resolved_path) {
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

            if !self.resolve_import_recursive(&import.alias, &raw_ast, &resolved_path) {
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

    // ── Legacy entry point ────────────────────────────────────────────────────

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
        raw_ast: &DixScript,
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
        if self.visiting.contains(&normalized_path) {
            let cycle_path  = self.build_cycle_path(&normalized_path);
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

        if self.visited.contains(&normalized_path) {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[ImportsResolver] Import '{}' already resolved, skipping",
                    alias
                ));
            }
            return true;
        }

        self.visiting.insert(normalized_path.clone());
        self.import_stack.push(normalized_path.clone());

        let result = self.resolve_import_inner(alias, raw_ast, &normalized_path);

        self.import_stack.pop();
        self.visiting.remove(&normalized_path);
        self.visited.insert(normalized_path);

        result
    }

    fn resolve_import_inner(
        &mut self,
        alias: &str,
        raw_ast: &DixScript,
        normalized_path: &str,
    ) -> bool {
        let mut local_imports = HashMap::new();

        // ── Step 1: Resolve all transitive dependencies FIRST ─────────────────
        if let Some(ref imports_section) = raw_ast.imports {
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

                let normalized_nested = if Self::is_cloud_url(&nested_path) {
                    Self::strip_query_parameters(&nested_path)
                } else {
                    match std::fs::canonicalize(&nested_path) {
                        Ok(p) => p.to_string_lossy().to_string(),
                        Err(_) => nested_path.clone(),
                    }
                };

                if self.visiting.contains(&normalized_nested) {
                    let cycle_path  = self.build_cycle_path(&normalized_nested);
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

                let nested_raw =
                    match self.read_and_parse_raw(nested_import, &nested_path) {
                        Ok(a) => a,
                        Err(_) => return false,
                    };

                if !self.resolve_import_recursive(
                    &nested_import.alias,
                    &nested_raw,
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

        // ── Step 2: Seed namespaces for enhancement ───────────────────────────
        let seed_namespaces: HashMap<String, ImportedNamespace> =
            if let Some(ref imports_section) = raw_ast.imports {
                imports_section
                    .imports
                    .iter()
                    .filter_map(|imp| {
                        self.symbol_table
                            .namespaces
                            .get(&imp.alias)
                            .map(|ns| (imp.alias.clone(), ns.clone()))
                    })
                    .collect()
            } else {
                HashMap::new()
            };

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[ImportsResolver] Seeding '{}' enhancement with {} namespace(s): [{}]",
                alias,
                seed_namespaces.len(),
                seed_namespaces.keys().cloned().collect::<Vec<_>>().join(", ")
            ));
        }

        let enhanced_ast =
            match self.analyze_and_enhance(raw_ast, normalized_path, alias, &seed_namespaces) {
                Ok(a)  => a,
                Err(_) => return false,
            };

        // ── Step 3: Extract symbols ───────────────────────────────────────────
        let functions =
            Self::extract_global_functions(enhanced_ast.quick_functions.as_ref(), alias);
        let enums = Self::extract_enums(enhanced_ast.enums.as_ref());

        if self.debug_config.is_enabled {
            if let Some(ref qf) = enhanced_ast.quick_functions {
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

    // ── Phase 1: Read and parse raw AST ──────────────────────────────────────

    fn read_and_parse_raw(
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

        let mut import_settings = self.operational_settings.clone();
        import_settings.source_file_path = Some(resolved_path.to_string());

        let tokenizer = Tokenizer::new(
            &config_result.cleaned_input_string,
            &import_settings,
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

        Ok(ast)
    }

    // ── Phase 2: Semantic analysis and AST enhancement ────────────────────────

    fn analyze_and_enhance(
        &mut self,
        raw_ast: &DixScript,
        resolved_path: &str,
        alias: &str,
        seed_namespaces: &HashMap<String, ImportedNamespace>,
    ) -> Result<DixScript, String> {
        let mut import_settings = self.operational_settings.clone();
        import_settings.source_file_path = Some(resolved_path.to_string());
        // CRITICAL: prevent a nested ImportsResolver from being created.
        import_settings.skip_imports_resolution = true;

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[ImportsResolver] Running semantic analysis on '{}' with {} seeded namespace(s)",
                alias,
                seed_namespaces.len()
            ));
        }

        let semantic_analyzer = GeneralSemanticAnalyzer::new_with_seed_namespaces(
            raw_ast,
            &import_settings,
            self.error_manager.clone(),
            seed_namespaces,
        );
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
                    alias,
                    summary,
                    semantic_result.errors.len()
                ),
                alias.to_string(),
                Some(resolved_path.to_string()),
                Some(resolved_path.to_string()),
                None,
                0,
                0,
                None,
            );
            return Err(format!("Semantic analysis failed for '{}'", alias));
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[ImportsResolver] Running AST enhancement on '{}'",
                alias
            ));
        }

        let ast_enhancer = GeneralAstEnhancer::new(&import_settings);
        let enhancement_result = ast_enhancer.enhance(raw_ast, Some(&semantic_result));

        if !enhancement_result.is_success {
            self.error_manager.add_imports_resolution_error(
                ImportsResolutionErrorType::ParseError,
                format!(
                    "AST enhancement failed for '{}': {} errors, {} warnings",
                    alias,
                    enhancement_result.errors.len(),
                    enhancement_result.warnings.len()
                ),
                alias.to_string(),
                Some(resolved_path.to_string()),
                Some(resolved_path.to_string()),
                None,
                0,
                0,
                None,
            );
            return Err(format!("AST enhancement failed for '{}'", alias));
        }

        let enhanced_ast = enhancement_result.enhanced_ast;

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[ImportsResolver] Processed '{}' ({} functions)",
                alias,
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
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[ImportsResolver] Downloading cloud import: {}",
                    import.path
                ));
            }
            self.download_cloud_file_sync(&import.path, &import.alias)
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
