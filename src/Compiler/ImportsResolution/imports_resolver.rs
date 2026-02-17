// src/Compiler/ImportsResolution/imports_resolver.rs

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::fs;
use crate::Compiler::AST::*;
use crate::Compiler::Core::{
    OperationalSettings,
    ErrorHandlingStrategy,
    DebugMode,
    ConfigSectionHandler,
};
use crate::Compiler::Core::Tokenizer::Tokenizer;
use crate::Compiler::Core::{GeneralParser, GeneralSemanticAnalyzer, GeneralAstEnhancer};
use crate::Compiler::Utilities::{SymbolTable, QuickFunctionInfo, FunctionSignature, ParameterInfo};
use crate::ErrorManager::{ErrorManager, ImportsResolutionErrorType};
use super::{CloudFileCache, CloudProviderFactory, HashVerifier};

/// ImportsResolver v1.0.0 - CORRECTED LIFETIME MANAGEMENT
///
/// CRITICAL: This resolver BORROWS SymbolTable from GeneralSemanticAnalyzer
/// It does NOT own the symbol table - it mutates the parent's table.
///
/// Flow for each import (including nested):
/// 1. Parse imported file
/// 2. Create NEW GeneralSemanticAnalyzer with NEW SymbolTable (for that file)
/// 3. Run semantic analysis (validates, populates that file's symbol table)
/// 4. Run AST enhancement (resolves qualified identifiers)
/// 5. Extract global functions/enums from ENHANCED AST
/// 6. Register extracted symbols in PARENT's SymbolTable (via &mut reference)
///
/// This ensures:
/// - Each file's analysis is isolated
/// - Nested imports work correctly (recursion creates new analyzers)
/// - Parent gets access to all imported symbols
/// - No symbol table cloning/merging needed
pub struct ImportsResolver<'a> {
    // BORROWED from GeneralSemanticAnalyzer (not owned!)
    symbol_table: &'a mut SymbolTable,
    operational_settings: &'a OperationalSettings,
    
    // Owned state
    error_manager: ErrorManager,
    cloud_cache: CloudFileCache,

    // Cycle detection state
    visiting: HashSet<String>,
    visited: HashSet<String>,
    import_stack: Vec<String>,
}

impl<'a> ImportsResolver<'a> {
    /// Create new ImportsResolver with borrowed references
    ///
    /// # Arguments
    /// * `symbol_table` - Mutable reference to parent's symbol table
    /// * `operational_settings` - Reference to compiler settings
    pub fn new(
        symbol_table: &'a mut SymbolTable,
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let cloud_cache = CloudFileCache::new(error_manager.clone());

        ImportsResolver {
            symbol_table,
            operational_settings,
            error_manager,
            cloud_cache,
            visiting: HashSet::new(),
            visited: HashSet::new(),
            import_stack: Vec::new(),
        }
    }

    /// Resolve all imports from parsed AST
    /// Returns true if successful, false if errors occurred
    pub async fn resolve_imports(
        &mut self,
        parsed_imports: &HashMap<String, (String, DixScript)>,
    ) -> bool {
        if parsed_imports.is_empty() {
            self.log_debug("No imports to resolve");
            return true;
        }

        self.log_info(&format!("Resolving {} imports", parsed_imports.len()));

        // Log cache statistics
        if self.operational_settings.debug_mode != DebugMode::Off {
            let cache_stats = self.cloud_cache.get_statistics();
            self.log_debug(&format!("Cache statistics: {}", cache_stats));
        }

        // Clear state (in case resolver is reused)
        self.visiting.clear();
        self.visited.clear();
        self.import_stack.clear();

        // Resolve each import recursively
        let mut success = true;

        for (alias, (absolute_path, ast)) in parsed_imports {
            self.log_debug(&format!("Resolving import '{}' from '{}'", alias, absolute_path));

            if !self.resolve_import_recursive(alias, ast, absolute_path).await {
                self.log_error(&format!("Failed to resolve import '{}'", alias));

                if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
                    return false;
                }

                success = false;
            }
        }

        let errors_count = self.error_manager.get_imports_resolution_errors().len();

        if errors_count > 0 {
            self.log_warning(&format!(
                "Imports resolution completed with {} errors",
                errors_count
            ));

            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
                return false;
            }
        } else {
            self.log_info("Successfully resolved imports");
        }

        success
    }

    /// Recursively resolve a single import (DFS with cycle detection)
    async fn resolve_import_recursive(
        &mut self,
        alias: &str,
        ast: &DixScript,
        absolute_path: &str,
    ) -> bool {
        // STEP 1: Normalize path for consistent comparison
        let normalized_path = if Self::is_cloud_url(absolute_path) {
            Self::strip_query_parameters(absolute_path)
        } else {
            match std::fs::canonicalize(absolute_path) {
                Ok(path) => path.to_string_lossy().to_string(),
                Err(_) => absolute_path.to_string(),
            }
        };

        // STEP 2: Check if currently visiting this file (CYCLE!)
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
                0, 0, None,
            );

            return false;
        }

        // STEP 3: Check if already fully resolved (optimization)
        if self.visited.contains(&normalized_path) {
            self.log_debug(&format!("Import '{}' already resolved, skipping", alias));
            return true;
        }

        // STEP 4: Mark as visiting and add to stack
        self.visiting.insert(normalized_path.clone());
        self.import_stack.push(normalized_path.clone());

        self.log_debug(&format!("Processing import '{}' at '{}'", alias, normalized_path));

        // STEP 5-9: Actual resolution logic (wrapped in cleanup)
        let result = self.resolve_import_inner(alias, ast, &normalized_path).await;

        // STEP 10: Cleanup - mark as fully processed
        self.import_stack.pop();
        self.visiting.remove(&normalized_path);
        self.visited.insert(normalized_path);

        result
    }

    /// Inner resolution logic (after cycle detection)
    async fn resolve_import_inner(
        &mut self,
        alias: &str,
        ast: &DixScript,
        normalized_path: &str,
    ) -> bool {
        // STEP 5: Resolve nested imports if any
        let mut local_imports = HashMap::new();

        if let Some(ref imports_section) = ast.imports {
            let nested_count = imports_section.imports.len();
            self.log_debug(&format!("Import '{}' has {} nested imports", alias, nested_count));

            // Get base directory for nested imports
            let nested_base_dir = if Self::is_cloud_url(normalized_path) {
                Self::get_cloud_url_directory(normalized_path)
            } else {
                Path::new(normalized_path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".to_string())
            };

            for nested_import in &imports_section.imports {
                // Resolve nested import path (cloud or local)
                let nested_path = if nested_import.is_cloud_import {
                    nested_import.path.clone()
                } else {
                    Self::resolve_path(&nested_base_dir, &nested_import.path)
                };

                // Check if already resolved
                if self.visited.contains(&nested_path) {
                    if let Some(existing_ns) = self.symbol_table.try_get_namespace(&nested_import.alias) {
                        local_imports.insert(nested_import.alias.clone(), existing_ns.clone());
                        continue;
                    } else {
                        self.error_manager.add_imports_resolution_error(
                            ImportsResolutionErrorType::GeneralError,
                            format!(
                                "Internal error: Namespace '{}' marked as visited but not in symbol table",
                                nested_import.alias
                            ),
                            nested_import.alias.clone(),
                            Some(nested_import.path.clone()),
                            Some(nested_path.clone()),
                            None, 0, 0, None,
                        );
                        return false;
                    }
                }

                // Parse nested import
                let nested_ast = match self.parse_imported_file(nested_import, &nested_path).await {
                    Ok(ast) => ast,
                    Err(_) => return false,
                };

                // Recursively resolve nested import
                if !self.resolve_import_recursive(&nested_import.alias, &nested_ast, &nested_path).await {
                    return false;
                }

                // Add to local imports
                if let Some(nested_ns) = self.symbol_table.try_get_namespace(&nested_import.alias) {
                    local_imports.insert(nested_import.alias.clone(), nested_ns.clone());
                }
            }
        }

        // STEP 6: Extract GLOBAL-SCOPED functions only
        let functions = Self::extract_global_functions(ast.quick_functions.as_ref(), alias);

        if self.operational_settings.debug_mode != DebugMode::Off {
            if let Some(ref qf_section) = ast.quick_functions {
                let total_funcs = qf_section.functions.len();
                let exported_funcs = functions.len();
                let skipped_funcs = total_funcs - exported_funcs;

                self.log_debug(&format!(
                    "Extracted {}/{} functions from '{}' ({} scoped functions not exported)",
                    exported_funcs, total_funcs, alias, skipped_funcs
                ));
            }
        }

        // STEP 7: Extract all enums (enums are always exported)
        let enums = Self::extract_enums(ast.enums.as_ref());

        if self.operational_settings.debug_mode != DebugMode::Off {
            if ast.enums.is_some() {
                self.log_debug(&format!("Extracted {} enums from '{}'", enums.len(), alias));
            }
        }

        // STEP 8: Register namespace in PARENT's SymbolTable (via &mut reference)
        self.symbol_table.register_namespace(
            alias.to_string(),
            normalized_path.to_string(),
            functions.clone(),
            enums.clone(),
            local_imports.clone(),
        );

        self.log_debug(&format!(
            "Registered namespace '{}' with {} functions, {} enums, {} local imports",
            alias, functions.len(), enums.len(), local_imports.len()
        ));

        true
    }

    /// Parse an imported file - SUPPORTS BOTH LOCAL AND CLOUD
    /// ✅ Runs FULL pipeline (Parse → Semantic Analysis → AST Enhancement)
    ///
    /// CRITICAL: Each imported file gets its OWN GeneralSemanticAnalyzer with OWN SymbolTable
    /// This ensures isolation - nested imports work correctly via recursion
    async fn parse_imported_file(
        &mut self,
        import: &ImportDeclaration,
        resolved_path: &str,
    ) -> Result<DixScript, String> {
        // STEP 1: Get file content (cloud or local)
        let content = if import.is_cloud_import {
            self.log_debug(&format!("Downloading cloud import: {}", import.path));
            self.download_cloud_file(&import.path, &import.alias).await?
        } else {
            self.log_debug(&format!("Reading local import: {}", resolved_path));

            fs::read_to_string(resolved_path).map_err(|e| {
                self.error_manager.add_imports_resolution_error(
                    ImportsResolutionErrorType::FileNotFound,
                    format!("Failed to read file: {}", e),
                    import.alias.clone(),
                    Some(import.path.clone()),
                    Some(resolved_path.to_string()),
                    None, 0, 0, None,
                );
                format!("Failed to read file: {}", e)
            })?
        };

        // STEP 2: Verify hash if provided
        if let Some(ref verify_hash) = import.verify_hash {
            self.log_debug(&format!("Verifying hash for '{}': {}", import.alias, verify_hash));

            HashVerifier::verify_hash(&content, verify_hash, &import.alias, resolved_path)
                .map_err(|e| {
                    self.error_manager.add_imports_resolution_error(
                        ImportsResolutionErrorType::HashVerificationFailed,
                        e.message.clone(),
                        import.alias.clone(),
                        Some(import.path.clone()),
                        Some(resolved_path.to_string()),
                        None, 0, 0, None,
                    );
                    format!("Hash verification failed: {}", e)
                })?;

            self.log_debug(&format!("Hash verification passed for '{}'", import.alias));
        }

        if content.trim().is_empty() {
            self.log_warning(&format!("Imported file '{}' is empty", resolved_path));
            return Ok(DixScript::new());
        }

        // STEP 3: Process CONFIG section
        let config_handler = ConfigSectionHandler::new(None);
        let config_result = config_handler.process_config_section(&content);

        // STEP 4: Tokenize
        self.log_debug("Tokenizing imported file");
        let tokenizer = Tokenizer::new(config_result.cleaned_input_string.clone());
        let token_result = tokenizer.tokenize();

        if token_result.tokens.is_empty() {
            self.error_manager.add_imports_resolution_error(
                ImportsResolutionErrorType::ParseError,
                "Tokenization produced no tokens".to_string(),
                import.alias.clone(),
                Some(import.path.clone()),
                Some(resolved_path.to_string()),
                None, 0, 0, None,
            );
            return Err("Tokenization produced no tokens".to_string());
        }

        // STEP 5: Create operational settings for imported file
        // CRITICAL: skip_imports_resolution = true prevents THIS resolver from being called again
        // The imported file will create its OWN ImportsResolver if it has imports
        let mut import_operational_settings = self.operational_settings.clone();
        import_operational_settings.source_file_path = Some(resolved_path.to_string());
        import_operational_settings.skip_imports_resolution = false; // ✅ Allow nested resolution

        // STEP 6: Parse
        self.log_debug("Parsing imported file");
        let general_parser = GeneralParser::new(
            token_result.tokens,
            config_result.config_section.clone(),
            import_operational_settings.clone(),
        ).map_err(|e| {
            self.error_manager.add_imports_resolution_error(
                ImportsResolutionErrorType::ParseError,
                format!("Failed to create parser: {}", e),
                import.alias.clone(),
                Some(import.path.clone()),
                Some(resolved_path.to_string()),
                None, 0, 0, None,
            );
            format!("Failed to create parser: {}", e)
        })?;

        let mut ast = general_parser.parse().map_err(|e| {
            let parse_errors = self.error_manager.get_parse_errors();

            if !parse_errors.is_empty() {
                let first_error = &parse_errors[0];
                self.error_manager.add_imports_resolution_error(
                    ImportsResolutionErrorType::ParseError,
                    format!("Parse errors in imported file: {}", first_error.message),
                    import.alias.clone(),
                    Some(import.path.clone()),
                    Some(resolved_path.to_string()),
                    None,
                    first_error.line as i32,
                    first_error.column as i32,
                    None,
                );
            }

            format!("Parse errors in imported file: {}", e)
        })?;

        ast.config = Some(config_result.config_section);

        // ✅ STEP 7: Run semantic analysis
        // CRITICAL: Create NEW SymbolTable for this file (isolated analysis)
        // Nested imports will create their own resolvers and populate this table
        self.log_debug(&format!(
            "Running semantic analysis on imported file '{}' (with nested import resolution)",
            import.alias
        ));

        let semantic_analyzer = GeneralSemanticAnalyzer::new(&ast, &import_operational_settings);
        let semantic_result = semantic_analyzer.analyze();

        if !semantic_result.is_success {
            let error_summary = if !semantic_result.errors.is_empty() {
                let first_error = &semantic_result.errors[0];
                format!("{}: {}", first_error.error_type, first_error.message)
            } else {
                "Unknown semantic error".to_string()
            };

            self.error_manager.add_imports_resolution_error(
                ImportsResolutionErrorType::ParseError,
                format!(
                    "Semantic analysis failed for '{}': {} (total: {} errors)",
                    import.alias, error_summary, semantic_result.errors.len()
                ),
                import.alias.clone(),
                Some(import.path.clone()),
                Some(resolved_path.to_string()),
                None, 0, 0, None,
            );

            return Err(format!("Semantic analysis failed for '{}'", import.alias));
        }

        self.log_debug(&format!(
            "Semantic analysis passed for '{}' ({} warnings)",
            import.alias, semantic_result.warnings.len()
        ));

        // ✅ STEP 8: Run AST enhancement
        // This transforms QualifiedIdentifiers into concrete expression types
        self.log_debug(&format!("Running AST enhancement on imported file '{}'", import.alias));

        let ast_enhancer = GeneralAstEnhancer::new(&import_operational_settings);
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
                None, 0, 0, None,
            );

            return Err(format!("AST enhancement failed for '{}'", import.alias));
        }

        // STEP 9: Use ENHANCED AST (not raw AST)
        // ✅ FIX: enhanced_ast is DixScript, not Option<DixScript>
        let enhanced_ast = enhancement_result.enhanced_ast;

        self.log_debug(&format!(
            "✅ Successfully processed imported file '{}' (enhanced {} functions)",
            import.alias,
            enhanced_ast.quick_functions.as_ref()
                .map(|qf| qf.functions.len())
                .unwrap_or(0)
        ));

        Ok(enhanced_ast)
    }

    // ... (rest of the helper methods remain the same)
    
    /// Download cloud file with caching support
    async fn download_cloud_file(
        &mut self,
        cloud_url: &str,
        alias: &str,
    ) -> Result<String, String> {
        let url_for_cache = Self::strip_query_parameters(cloud_url);

        // Check cache first
        if self.cloud_cache.is_cached(&url_for_cache) {
            self.log_debug(&format!("Using cached version of '{}'", url_for_cache));

            if let Some(cached_content) = self.cloud_cache.get_cached_content(&url_for_cache) {
                return Ok(cached_content);
            }

            self.log_debug("Cache read failed, downloading fresh copy");
        }

        // Get cloud provider
        let provider = CloudProviderFactory::get_provider(cloud_url, &self.error_manager)
            .map_err(|e| {
                self.error_manager.add_imports_resolution_error(
                    ImportsResolutionErrorType::CloudImportNotSupported,
                    e.clone(),
                    alias.to_string(),
                    Some(cloud_url.to_string()),
                    Some(cloud_url.to_string()),
                    None, 0, 0, None,
                );
                e
            })?;

        // Download file
        let content = provider.download_file_async(cloud_url).await
            .map_err(|e| {
                self.error_manager.add_imports_resolution_error(
                    ImportsResolutionErrorType::FileNotFound,
                    format!("Cloud download failed: {}", e),
                    alias.to_string(),
                    Some(cloud_url.to_string()),
                    Some(cloud_url.to_string()),
                    None, 0, 0, None,
                );
                format!("Cloud download failed: {}", e)
            })?;

        // Cache the downloaded content
        self.cloud_cache.cache_file(&url_for_cache, &content);

        Ok(content)
    }

    /// Extract ONLY global-scoped functions (scoped functions are internal)
    fn extract_global_functions(
        section: Option<&QuickFuncsSection>,
        _namespace_name: &str,
    ) -> HashMap<String, QuickFunctionInfo> {
        let mut functions = HashMap::new();

        let section = match section {
            Some(s) => s,
            None => return functions,
        };

        if section.functions.is_empty() {
            return functions;
        }

        for func in &section.functions {
            // Check if function is globally scoped
            let is_global = match &func.scope_list {
                None => true,
                Some(scopes) if scopes.len() == 1 && scopes[0].eq_ignore_ascii_case("global") => true,
                _ => false,
            };

            if !is_global {
                continue;
            }

            // Build function signature
            let parameters: Vec<ParameterInfo> = func.parameters.iter().map(|param| {
                ParameterInfo {
                    name: param.name.clone(),
                    param_type: param.data_type,
                    has_default_value: param.default_value.is_some(),
                    default_value: param.default_value.clone(),
                }
            }).collect();

            let signature = FunctionSignature {
                name: func.name.clone(),
                return_type: func.return_type,
                parameters,
                scopes: func.scope_list.clone().unwrap_or_else(|| vec!["global".to_string()]),
                line: func.position.line as i32,
                column: func.position.column as i32,
            };

            functions.insert(func.name.clone(), QuickFunctionInfo {
                signature,
                ast: func.clone(),
            });
        }

        functions
    }

    /// Extract all enums (enums are always exported)
    fn extract_enums(section: Option<&EnumsSection>) -> HashMap<String, HashMap<String, i32>> {
        let mut enums = HashMap::new();

        let section = match section {
            Some(s) => s,
            None => return enums,
        };

        if section.enums.is_empty() {
            return enums;
        }

        for enum_decl in &section.enums {
            let mut field_map = HashMap::new();
            let mut auto_value = 0;

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

    // ==================== HELPER METHODS ====================

    #[inline]
    fn is_cloud_url(path: &str) -> bool {
        path.starts_with("http://") || path.starts_with("https://")
    }

    #[inline]
    fn strip_query_parameters(url: &str) -> String {
        if let Some(query_index) = url.find('?') {
            url[..query_index].to_string()
        } else {
            url.to_string()
        }
    }

    fn get_cloud_url_directory(cloud_url: &str) -> String {
        let url_without_query = Self::strip_query_parameters(cloud_url);

        if let Some(last_slash) = url_without_query.rfind('/') {
            url_without_query[..=last_slash].to_string()
        } else {
            url_without_query
        }
    }

    fn resolve_path(base_directory: &str, relative_path: &str) -> String {
        let combined = Path::new(base_directory).join(relative_path);
        combined.to_string_lossy().to_string()
    }

    fn build_cycle_path(&self, cycle_target: &str) -> String {
        let mut stack: Vec<String> = self.import_stack.iter().cloned().collect();
        stack.push(cycle_target.to_string());

        let readable_paths: Vec<String> = stack.iter().map(|p| {
            if Self::is_cloud_url(p) {
                url::Url::parse(p)
                    .map(|u| u.host_str().unwrap_or(p).to_string())
                    .unwrap_or_else(|_| p.clone())
            } else {
                Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(p)
                    .to_string()
            }
        }).collect();

        readable_paths.join(" → ")
    }

    fn build_cycle_chain_list(&self, cycle_target: &str) -> Vec<String> {
        let mut stack: Vec<String> = self.import_stack.iter().cloned().collect();
        stack.push(cycle_target.to_string());

        stack.iter().map(|p| {
            if Self::is_cloud_url(p) {
                url::Url::parse(p)
                    .map(|u| u.host_str().unwrap_or(p).to_string())
                    .unwrap_or_else(|_| p.clone())
            } else {
                Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(p)
                    .to_string()
            }
        }).collect()
    }

    /// Get statistics about import resolution
    pub fn get_statistics(&self) -> ImportResolutionStats {
        let total_functions: usize = self.symbol_table.namespaces.values()
            .map(|ns| ns.functions.len())
            .sum();

        let total_enums: usize = self.symbol_table.namespaces.values()
            .map(|ns| ns.enums.len())
            .sum();

        let total_local_imports: usize = self.symbol_table.namespaces.values()
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

    // ==================== LOGGING ====================

    #[inline]
    fn log_debug(&self, message: &str) {
        if self.operational_settings.debug_mode != DebugMode::Off {
            self.error_manager.log_debug(&format!("[ImportsResolver] {}", message));
        }
    }

    #[inline]
    fn log_info(&self, message: &str) {
        self.error_manager.log_info(&format!("[ImportsResolver] {}", message));
    }

    #[inline]
    fn log_warning(&self, message: &str) {
        self.error_manager.log_warning(&format!("[ImportsResolver] {}", message));
    }

    #[inline]
    fn log_error(&self, message: &str) {
        self.error_manager.log_error(&format!("[ImportsResolver] {}", message));
    }
}

/// Statistics about import resolution
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
            "Namespaces: {}, Functions: {}, Enums: {}, Nested Imports: {}, Files Visited: {}",
            self.total_namespaces,
            self.total_functions_imported,
            self.total_enums_imported,
            self.total_nested_imports,
            self.files_visited
        )
    }
            }
