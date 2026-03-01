// src/Compiler/Core/SectionAnalyzers/imports_section_analyzer.rs
//! Semantic validation of the @IMPORTS section.
//!
//! Validates aliases, paths, cloud URL structure, and hash format before
//! the resolver attempts any file I/O or network access.

use crate::Compiler::AST::{ImportsSection, ImportDeclaration};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Core::OperationalSettings;
use crate::ErrorManager::{ErrorManager, DebugConfig};
use rustc_hash::FxHashSet;

pub struct ImportsSectionAnalyzer<'a> {
    symbol_table:         &'a SymbolTable,
    operational_settings: &'a OperationalSettings,
    current_file_path:    String,
    error_manager:        ErrorManager,
    debug_config:         DebugConfig,
}

impl<'a> ImportsSectionAnalyzer<'a> {
    pub fn new(
        symbol_table:         &'a SymbolTable,
        operational_settings: &'a OperationalSettings,
        current_file_path:    &str,
    ) -> Self {
        ImportsSectionAnalyzer {
            error_manager:     ErrorManager::get_shared_instance(),
            debug_config:      DebugConfig::from_debug_mode(operational_settings.debug_mode),
            symbol_table,
            operational_settings,
            current_file_path: current_file_path.to_string(),
        }
    }

    pub fn analyze(&mut self, imports_section: Option<&ImportsSection>) {
        let section = match imports_section {
            Some(s) if !s.imports.is_empty() => s,
            _ => {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug("No imports to validate");
                }
                return;
            }
        };

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Validating {} import declarations", section.imports.len()
            ));
        }

        let mut seen_aliases: FxHashSet<String> = FxHashSet::default();
        let mut seen_paths:   FxHashSet<String> = FxHashSet::default();

        for import in &section.imports {
            self.validate_alias(import, &mut seen_aliases);
            self.validate_path(import, &mut seen_paths);

            if let Some(ref hash) = import.verify_hash {
                self.validate_hash_format(import, hash);
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug("IMPORTS semantic validation complete");
        }
    }

    fn validate_alias(
        &self,
        import: &ImportDeclaration,
        seen: &mut FxHashSet<String>,
    ) {
        let alias = &import.alias;

        if alias.is_empty() {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Import alias cannot be empty (path: '{}')",
                import.path
            ));
            return;
        }

        if !Self::is_valid_identifier(alias) {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Invalid import alias '{}': must be a valid identifier \
                 (letters, digits, underscores; must not start with a digit)",
                alias
            ));
        }

        if !seen.insert(alias.clone()) {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Duplicate import alias '{}': each alias must be unique",
                alias
            ));
        }

        if self.symbol_table.is_builtin_static_object(alias) {
            self.error_manager.log_warning(&format!(
                "[ImportsAnalyzer] Import alias '{}' shadows a built-in static object",
                alias
            ));
        }
    }

    fn validate_path(
        &self,
        import: &ImportDeclaration,
        seen: &mut FxHashSet<String>,
    ) {
        let path = &import.path;

        if path.is_empty() {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Import path for alias '{}' cannot be empty",
                import.alias
            ));
            return;
        }

        if import.is_cloud_import {
            self.validate_cloud_url(import);
        } else {
            self.validate_local_path(import);
        }

        if !seen.insert(path.clone()) {
            self.error_manager.log_warning(&format!(
                "[ImportsAnalyzer] Path '{}' is imported more than once",
                path
            ));
        }
    }

    fn validate_cloud_url(&self, import: &ImportDeclaration) {
        let url   = &import.path;
        let lower = url.to_ascii_lowercase();

        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Cloud import '{}' must use http:// or https://. Got: '{}'",
                import.alias, url
            ));
            return;
        }

        if lower.starts_with("http://") && !Self::is_local_address(url) {
            self.error_manager.log_warning(&format!(
                "[ImportsAnalyzer] Cloud import '{}' uses insecure HTTP. Use HTTPS for non-local URLs.",
                import.alias
            ));
        }

        let after_scheme = if lower.starts_with("https://") { &url[8..] } else { &url[7..] };

        if after_scheme.is_empty() || after_scheme.starts_with('/') {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Cloud import '{}' has no hostname in URL '{}'",
                import.alias, url
            ));
            return;
        }

        let hostname = after_scheme.split('/').next().unwrap_or("");
        if hostname.is_empty() {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Cloud import '{}' has an empty hostname in URL '{}'",
                import.alias, url
            ));
            return;
        }

        if let Some(path_start) = after_scheme.find('/') {
            let path_part    = &after_scheme[path_start..];
            let path_no_qs   = path_part.split('?').next().unwrap_or(path_part);
            if !path_no_qs.to_ascii_lowercase().ends_with(".mdix") {
                self.error_manager.log_warning(&format!(
                    "[ImportsAnalyzer] Cloud import '{}' URL '{}' does not end in .mdix",
                    import.alias, url
                ));
            }
        } else {
            self.error_manager.log_warning(&format!(
                "[ImportsAnalyzer] Cloud import '{}' URL '{}' has no path component",
                import.alias, url
            ));
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "Cloud import '{}' URL basic structure OK: {}", import.alias, url
            ));
        }
    }

    fn validate_local_path(&self, import: &ImportDeclaration) {
        let path  = &import.path;
        let lower = path.to_ascii_lowercase();

        if lower.starts_with("http://") || lower.starts_with("https://") {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Import '{}' path '{}' looks like a URL but was not \
                 declared with 'from_cloud'. Use 'from_cloud' for HTTP/HTTPS imports.",
                import.alias, path
            ));
            return;
        }

        if !lower.ends_with(".mdix") {
            self.error_manager.log_warning(&format!(
                "[ImportsAnalyzer] Import '{}' path '{}' does not end in .mdix",
                import.alias, path
            ));
        }

        if path.contains('\0') {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Import '{}' path '{}' contains a null byte",
                import.alias, path
            ));
        }

        let is_absolute = path.starts_with('/')
            || (path.len() >= 2 && path.as_bytes()[1] == b':');
        if is_absolute {
            self.error_manager.log_warning(&format!(
                "[ImportsAnalyzer] Import '{}' uses an absolute path '{}'. \
                 Relative paths are preferred for portability.",
                import.alias, path
            ));
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "Local import '{}' path basic structure OK: {}", import.alias, path
            ));
        }
    }

    fn validate_hash_format(&self, import: &ImportDeclaration, hash: &str) {
        if hash.is_empty() {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Import '{}' has an empty verify hash. \
                 Remove the 'verify' clause or provide a valid hash.",
                import.alias
            ));
            return;
        }

        let parts: Vec<&str> = hash.splitn(2, ':').collect();
        if parts.len() != 2 {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Import '{}' verify hash '{}' is malformed. \
                 Expected format: 'algorithm:hexstring' (e.g. 'sha256:abc123...')",
                import.alias, hash
            ));
            return;
        }

        let algorithm = parts[0].to_ascii_lowercase();
        let hex       = parts[1];

        match algorithm.as_str() {
            "sha256" => {
                if hex.len() != 64 {
                    self.error_manager.log_error(&format!(
                        "[ImportsAnalyzer] Import '{}' sha256 hash must be 64 hex characters, got {}",
                        import.alias, hex.len()
                    ));
                }
            }
            "sha512" => {
                if hex.len() != 128 {
                    self.error_manager.log_error(&format!(
                        "[ImportsAnalyzer] Import '{}' sha512 hash must be 128 hex characters, got {}",
                        import.alias, hex.len()
                    ));
                }
            }
            other => {
                self.error_manager.log_error(&format!(
                    "[ImportsAnalyzer] Import '{}' uses unsupported hash algorithm '{}'. \
                     Supported: sha256, sha512",
                    import.alias, other
                ));
                return;
            }
        }

        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Import '{}' verify hash '{}' contains non-hex characters",
                import.alias, hash
            ));
        }
    }

    #[inline]
    fn is_valid_identifier(s: &str) -> bool {
        if s.is_empty() { return false; }
        let mut chars = s.chars();
        let first = chars.next().unwrap();
        if !first.is_alphabetic() && first != '_' { return false; }
        chars.all(|c| c.is_alphanumeric() || c == '_')
    }

    #[inline]
    fn is_local_address(url: &str) -> bool {
        let lower = url.to_ascii_lowercase();
        lower.contains("localhost")
            || lower.contains("127.0.0.1")
            || lower.contains("::1")
            || lower.contains("0.0.0.0")
    }
    }
