// src/Compiler/Core/SectionAnalyzers/imports_section_analyzer.rs

use std::collections::HashSet;
use std::path::Path;
use crate::Compiler::AST::{ImportsSection, ImportDeclaration, Position};
use crate::Compiler::Core::{OperationalSettings, DebugMode};
use crate::Compiler::Utilities::SymbolTable;
use crate::ErrorManager::{ErrorManager, SemanticErrorType};

/// ImportsSectionAnalyzer v1.0.0
/// 
/// Validates import declarations BEFORE resolution
/// SUPPORTS: Cloud imports with HTTP/HTTPS
/// 
/// Responsibilities:
/// 1. Validate alias uniqueness
/// 2. Check alias conflicts with built-ins, functions, enums
/// 3. Validate local import paths (file exists, .mdix extension, not self-import)
/// 4. Validate cloud import paths (HTTP/HTTPS URLs, .mdix extension)
/// 5. Validate verify hash format (sha256/sha512:hexstring)
/// 6. Report errors through ErrorManager
pub struct ImportsSectionAnalyzer<'a> {
    error_manager: ErrorManager,
    operational_settings: &'a OperationalSettings,
    symbol_table: &'a SymbolTable,
    current_file_path: &'a str,
}

impl<'a> ImportsSectionAnalyzer<'a> {
    /// Create new ImportsSectionAnalyzer
    pub fn new(
        symbol_table: &'a SymbolTable,
        operational_settings: &'a OperationalSettings,
        current_file_path: &'a str,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        
        ImportsSectionAnalyzer {
            error_manager,
            operational_settings,
            symbol_table,
            current_file_path,
        }
    }
    
    /// Analyze IMPORTS section for semantic correctness
    /// This runs BEFORE ImportsResolver to catch early errors
    pub fn analyze(&mut self, imports_section: Option<&ImportsSection>) {
        let imports_section = match imports_section {
            Some(section) => section,
            None => {
                self.log_debug("No IMPORTS section to analyze");
                return;
            }
        };
        
        self.log_debug(&format!(
            "Analyzing {} import declarations",
            imports_section.imports.len()
        ));
        
        let mut seen_aliases = HashSet::new();
        let mut seen_paths = HashSet::new();
        
        for import in &imports_section.imports {
            self.analyze_import_declaration(import, &mut seen_aliases, &mut seen_paths);
        }
        
        self.log_debug("IMPORTS section analysis complete");
    }
    
    /// Analyze a single import declaration
    fn analyze_import_declaration(
        &mut self,
        import: &ImportDeclaration,
        seen_aliases: &mut HashSet<String>,
        seen_paths: &mut HashSet<String>,
    ) {
        self.log_debug(&format!("Analyzing import '{}'", import.alias));
        
        // 1. Validate alias uniqueness
        if seen_aliases.contains(&import.alias) {
            self.add_error(
                SemanticErrorType::DuplicateDefinition,
                &format!(
                    "Duplicate import alias '{}' - each alias must be unique",
                    import.alias
                ),
                import.position,
            );
        } else {
            seen_aliases.insert(import.alias.clone());
        }
        
        // 2. Check alias doesn't conflict with built-ins
        if self.symbol_table.is_builtin_static_object(&import.alias) {
            self.add_error(
                SemanticErrorType::NameConflict,
                &format!(
                    "Import alias '{}' conflicts with built-in object",
                    import.alias
                ),
                import.position,
            );
        }
        
        // 3. Check alias doesn't conflict with existing functions
        if self.symbol_table.has_function(&import.alias) {
            self.add_error(
                SemanticErrorType::NameConflict,
                &format!(
                    "Import alias '{}' conflicts with existing function",
                    import.alias
                ),
                import.position,
            );
        }
        
        // 4. Check alias doesn't conflict with existing enums
        if self.symbol_table.has_enum(&import.alias) {
            self.add_error(
                SemanticErrorType::NameConflict,
                &format!(
                    "Import alias '{}' conflicts with existing enum",
                    import.alias
                ),
                import.position,
            );
        }
        
        // 5. Validate path based on import type
        if import.is_cloud_import {
            self.validate_cloud_import_path(import);
        } else {
            self.validate_local_import_path(import, seen_paths);
        }
        
        // 6. Validate verify hash if present
        if let Some(ref verify_hash) = import.verify_hash {
            self.validate_verify_hash(import, verify_hash);
        }
        
        self.log_debug(&format!("Import '{}' validated successfully", import.alias));
    }
    
    /// Validate local import path
    fn validate_local_import_path(
        &mut self,
        import: &ImportDeclaration,
        seen_paths: &mut HashSet<String>,
    ) {
        let path = &import.path;
        
        // Check for duplicate paths (same file imported multiple times)
        if seen_paths.contains(path) {
            self.add_warning(
                &format!(
                    "File '{}' is imported multiple times with different aliases",
                    path
                ),
                import.position,
            );
        } else {
            seen_paths.insert(path.clone());
        }
        
        // Resolve relative to current file
        let resolved_path = match self.resolve_local_path(path) {
            Ok(p) => p,
            Err(e) => {
                self.add_error(
                    SemanticErrorType::InvalidReference,
                    &format!("Invalid import path '{}': {}", path, e),
                    import.position,
                );
                return;
            }
        };
        
        // Check if file exists
        if !std::path::Path::new(&resolved_path).exists() {
            self.add_error(
                SemanticErrorType::InvalidReference,
                &format!(
                    "Import file not found: '{}' (resolved to: {})",
                    path, resolved_path
                ),
                import.position,
            );
            return;
        }
        
        // Warn if importing self (circular)
        if let Ok(current_canonical) = std::fs::canonicalize(self.current_file_path) {
            if let Ok(import_canonical) = std::fs::canonicalize(&resolved_path) {
                if current_canonical == import_canonical {
                    self.add_error(
                        SemanticErrorType::InvalidReference,
                        &format!(
                            "Cannot import self: '{}' resolves to current file",
                            path
                        ),
                        import.position,
                    );
                    return;
                }
            }
        }
        
        // Check file extension (already validated in parser, but double-check)
        if !path.to_lowercase().ends_with(".mdix") {
            self.add_error(
                SemanticErrorType::InvalidReference,
                &format!("Import path must have .mdix extension: '{}'", path),
                import.position,
            );
        }
        
        self.log_debug(&format!("Local import path '{}' resolved to: {}", path, resolved_path));
    }
    
    /// Validate cloud import path - supports HTTP/HTTPS URLs and query parameters
    fn validate_cloud_import_path(&mut self, import: &ImportDeclaration) {
        let path = &import.path;
        
        // Strip query parameters before validation
        let path_without_query = if let Some(query_index) = path.find('?') {
            let stripped = &path[..query_index];
            self.log_debug(&format!("Stripped query parameters from URL: {}", stripped));
            stripped
        } else {
            path.as_str()
        };
        
        // Phase 1: HTTP/HTTPS URLs (NOW SUPPORTED)
        if path.starts_with("https://") || path.starts_with("http://") {
            // Check .mdix extension (use path WITHOUT query parameters)
            if !path_without_query.to_lowercase().ends_with(".mdix") {
                self.add_error(
                    SemanticErrorType::InvalidReference,
                    &format!(
                        "Cloud import path must have .mdix extension: '{}'",
                        path_without_query
                    ),
                    import.position,
                );
                return;
            }
            
            // Validate URL format (basic check)
            match url::Url::parse(path) {
                Ok(uri) => {
                    if uri.scheme() != "http" && uri.scheme() != "https" {
                        self.add_error(
                            SemanticErrorType::InvalidReference,
                            &format!(
                                "Cloud import URL must use HTTP or HTTPS scheme: '{}'",
                                path
                            ),
                            import.position,
                        );
                        return;
                    }
                }
                Err(e) => {
                    self.add_error(
                        SemanticErrorType::InvalidReference,
                        &format!(
                            "Invalid cloud import URL format: '{}' - {}",
                            path, e
                        ),
                        import.position,
                    );
                    return;
                }
            }
            
            // Warn if using HTTP (not HTTPS)
            if path.starts_with("http://") 
                && !path.contains("localhost") 
                && !path.contains("127.0.0.1") 
            {
                self.add_warning(
                    &format!(
                        "⚠️ SECURITY WARNING: Using insecure HTTP for cloud import. \
                         Use HTTPS for production: {}",
                        path
                    ),
                    import.position,
                );
            }
            
            self.log_debug(&format!(
                "Cloud import path validated (HTTP/HTTPS): {}",
                path_without_query
            ));
            return;
        }
        
        // Phase 2+: Future cloud storage schemes (not yet supported)
        if path.starts_with("s3://") 
            || path.starts_with("azure://") 
            || path.starts_with("gs://") 
        {
            self.add_error(
                SemanticErrorType::InvalidReference,
                &format!(
                    "Cloud storage schemes (s3://, azure://, gs://) are not yet supported in v1.0.0. \
                     Use direct HTTPS URLs instead: '{}'",
                    path
                ),
                import.position,
            );
            return;
        }
        
        // Invalid cloud import format
        self.add_error(
            SemanticErrorType::InvalidReference,
            &format!(
                "Cloud import must be a valid HTTPS or HTTP URL. \
                 Expected: https://example.com/path/to/file.mdix, got: '{}'",
                path
            ),
            import.position,
        );
    }
    
    /// Validate verify hash format
    fn validate_verify_hash(
        &mut self,
        import: &ImportDeclaration,
        hash: &str,
    ) {
        if hash.trim().is_empty() {
            self.add_warning(
                "Empty verify hash - hash verification will be skipped",
                import.position,
            );
            return;
        }
        
        // Expected format: "sha256:HEXSTRING" or "sha512:HEXSTRING"
        let parts: Vec<&str> = hash.splitn(2, ':').collect();
        if parts.len() != 2 {
            self.add_error(
                SemanticErrorType::InvalidLiteral,
                &format!(
                    "Verify hash must be in format 'algorithm:hash', got: '{}'",
                    hash
                ),
                import.position,
            );
            return;
        }
        
        let algorithm = parts[0].to_lowercase();
        let hash_value = parts[1];
        
        // Validate algorithm
        if algorithm != "sha256" && algorithm != "sha512" {
            self.add_error(
                SemanticErrorType::InvalidLiteral,
                &format!(
                    "Unsupported hash algorithm '{}' - supported: sha256, sha512",
                    algorithm
                ),
                import.position,
            );
            return;
        }
        
        // Validate hash format (hex string)
        let expected_length = if algorithm == "sha256" { 64 } else { 128 };
        if hash_value.len() != expected_length {
            self.add_error(
                SemanticErrorType::InvalidLiteral,
                &format!(
                    "Invalid {} hash length: expected {} hex chars, got {}",
                    algorithm, expected_length, hash_value.len()
                ),
                import.position,
            );
            return;
        }
        
        if !Self::is_hex_string(hash_value) {
            self.add_error(
                SemanticErrorType::InvalidLiteral,
                "Invalid hash format: must be hexadecimal string",
                import.position,
            );
            return;
        }
        
        self.log_debug(&format!(
            "Verify hash validated: {}:{}...",
            algorithm,
            &hash_value[..8]
        ));
    }
    
    // ==================== HELPER METHODS ====================
    
    /// Resolve local import path relative to current file
    fn resolve_local_path(&self, relative_path: &str) -> Result<String, String> {
        let current_dir = Path::new(self.current_file_path)
            .parent()
            .ok_or_else(|| "Cannot determine current file directory".to_string())?;
        
        let combined = current_dir.join(relative_path);
        
        combined
            .to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| "Invalid path characters".to_string())
    }
    
    /// Check if string is valid hexadecimal
    fn is_hex_string(s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
    }
    
    // ==================== ERROR HANDLING ====================
    
    fn add_error(
        &mut self,
        error_type: SemanticErrorType,
        message: &str,
        position: Position,
    ) {
        self.error_manager.add_semantic_error(
            error_type,
            message.to_string(),
            position.line as i32,
            position.column as i32,
            Some("IMPORTS".to_string()),
            None,
        );
    }
    
    fn add_warning(&self, message: &str, position: Position) {
        if self.operational_settings.debug_mode != DebugMode::Off {
            self.error_manager.log_Warning(&format!(
                "[Line {}:{}] {}",
                position.line,
                position.column,
                message
            ));
        }
    }
    
    // ==================== LOGGING ====================
    
    fn log_debug(&self, message: &str) {
        if self.operational_settings.debug_mode >= DebugMode::Regular {
            self.error_manager.log_debug(&format!("[IMPORTS Analyzer] {}", message));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_hex_string() {
        assert!(ImportsSectionAnalyzer::is_hex_string("0123456789abcdef"));
        assert!(ImportsSectionAnalyzer::is_hex_string("ABCDEF"));
        assert!(!ImportsSectionAnalyzer::is_hex_string("xyz"));
        assert!(!ImportsSectionAnalyzer::is_hex_string(""));
        assert!(!ImportsSectionAnalyzer::is_hex_string("12 34"));
    }
  }
