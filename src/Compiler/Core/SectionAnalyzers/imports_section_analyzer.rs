// src/Compiler/Core/SectionAnalyzers/imports_section_analyzer.rs
//! Semantic validation of the @IMPORTS section.
//!
//! Validates aliases, paths, cloud URL structure, and hash format before
//! the resolver attempts any file I/O or network access.

use std::collections::HashSet;
use crate::Compiler::AST::{ImportsSection, ImportDeclaration};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Core::OperationalSettings;
use crate::ErrorManager::ErrorManager;

pub struct ImportsSectionAnalyzer<'a> {
    symbol_table: &'a SymbolTable,
    operational_settings: &'a OperationalSettings,
    current_file_path: String,
    error_manager: ErrorManager,
    can_log_debug: bool,
    can_log_verbose: bool,
}

impl<'a> ImportsSectionAnalyzer<'a> {
    pub fn new(
        symbol_table: &'a SymbolTable,
        operational_settings: &'a OperationalSettings,
        current_file_path: &str,
    ) -> Self {
        use crate::Compiler::Core::DebugMode;
        let can_log_debug = operational_settings.debug_mode != DebugMode::Off;
        let can_log_verbose = operational_settings.debug_mode == DebugMode::Verbose;

        ImportsSectionAnalyzer {
            symbol_table,
            operational_settings,
            current_file_path: current_file_path.to_string(),
            error_manager: ErrorManager::get_shared_instance(),
            can_log_debug,
            can_log_verbose,
        }
    }

    pub fn analyze(&mut self, imports_section: Option<&ImportsSection>) {
        let section = match imports_section {
            Some(s) if !s.imports.is_empty() => s,
            _ => {
                self.log_debug("No imports to validate");
                return;
            }
        };

        self.log_debug(&format!("Validating {} import declarations", section.imports.len()));

        let mut seen_aliases: HashSet<String> = HashSet::new();
        let mut seen_paths: HashSet<String> = HashSet::new();

        for import in &section.imports {
            self.validate_alias(import, &mut seen_aliases);
            self.validate_path(import, &mut seen_paths);

            if let Some(ref hash) = import.verify_hash {
                self.validate_hash_format(import, hash);
            }
        }

        self.log_debug("IMPORTS semantic validation complete");
    }

    fn validate_alias(&self, import: &ImportDeclaration, seen: &mut HashSet<String>) {
        let alias = &import.alias;

        if alias.is_empty() {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Import alias cannot be empty (path: '{}')",
                import.path
            ));
            return;
        }

        // Alias must be a valid identifier: starts with letter or underscore, alphanumeric/underscore only
        if !Self::is_valid_identifier(alias) {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Invalid import alias '{}': must be a valid identifier \
                 (letters, digits, underscores; must not start with a digit)",
                alias
            ));
        }

        if !seen.insert(alias.clone()) {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Duplicate import alias '{}': each alias must be unique \
                 within the file",
                alias
            ));
        }

        // Warn if alias shadows a builtin static object
        if self.symbol_table.is_builtin_static_object(alias) {
            self.error_manager.log_warning(&format!(
                "[ImportsAnalyzer] Import alias '{}' shadows a built-in static object",
                alias
            ));
        }
    }

    fn validate_path(&self, import: &ImportDeclaration, seen: &mut HashSet<String>) {
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

        // Warn on duplicate paths (same file imported under multiple aliases)
        if !seen.insert(path.clone()) {
            self.error_manager.log_warning(&format!(
                "[ImportsAnalyzer] Path '{}' is imported more than once \
                 (aliases may diverge in future refactors)",
                path
            ));
        }
    }

    fn validate_cloud_url(&self, import: &ImportDeclaration) {
        let url = &import.path;

        // Use string-based checks — the `url` crate is optional (cloud_imports feature).
        let lower = url.to_lowercase();

        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Cloud import '{}' must use http:// or https://. \
                 Got: '{}'",
                import.alias, url
            ));
            return;
        }

        if lower.starts_with("http://")
            && !Self::is_local_address(url)
        {
            self.error_manager.log_warning(&format!(
                "[ImportsAnalyzer] Cloud import '{}' uses insecure HTTP. \
                 Use HTTPS for non-local URLs.",
                import.alias
            ));
        }

        // Must have at least a hostname after the scheme
        let after_scheme = if lower.starts_with("https://") {
            &url[8..]
        } else {
            &url[7..]
        };

        if after_scheme.is_empty() || after_scheme.starts_with('/') {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Cloud import '{}' has no hostname in URL '{}'",
                import.alias, url
            ));
            return;
        }

        // Hostname must not be empty (catches "http:///path")
        let hostname = after_scheme.split('/').next().unwrap_or("");
        if hostname.is_empty() {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Cloud import '{}' has an empty hostname in URL '{}'",
                import.alias, url
            ));
            return;
        }

        // Path component must be present and end in .mdix
        if let Some(path_start) = after_scheme.find('/') {
            let path_part = &after_scheme[path_start..];
            // Strip query string for extension check
            let path_no_query = path_part.split('?').next().unwrap_or(path_part);
            if !path_no_query.to_lowercase().ends_with(".mdix") {
                self.error_manager.log_warning(&format!(
                    "[ImportsAnalyzer] Cloud import '{}' URL '{}' does not end in .mdix. \
                     Ensure the URL points directly to a .mdix file.",
                    import.alias, url
                ));
            }
        } else {
            self.error_manager.log_warning(&format!(
                "[ImportsAnalyzer] Cloud import '{}' URL '{}' has no path component. \
                 Ensure the URL points to a specific .mdix file.",
                import.alias, url
            ));
        }

        self.log_verbose(&format!(
            "Cloud import '{}' URL basic structure OK: {}",
            import.alias, url
        ));
    }

    fn validate_local_path(&self, import: &ImportDeclaration) {
        let path = &import.path;

        // Reject paths that look like URLs but weren't flagged as cloud imports
        let lower = path.to_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Import '{}' path '{}' looks like a URL but was not \
                 declared with 'from_cloud'. Use 'from_cloud' for HTTP/HTTPS imports.",
                import.alias, path
            ));
            return;
        }

        // Path must end in .mdix
        if !lower.ends_with(".mdix") {
            self.error_manager.log_warning(&format!(
                "[ImportsAnalyzer] Import '{}' path '{}' does not end in .mdix",
                import.alias, path
            ));
        }

        // Reject null bytes and other clearly invalid characters
        if path.contains('\0') {
            self.error_manager.log_error(&format!(
                "[ImportsAnalyzer] Import '{}' path '{}' contains a null byte",
                import.alias, path
            ));
        }

        // Warn on absolute paths that might break portability
        let is_absolute = path.starts_with('/') || (path.len() >= 2 && path.as_bytes()[1] == b':');
        if is_absolute {
            self.error_manager.log_warning(&format!(
                "[ImportsAnalyzer] Import '{}' uses an absolute path '{}'. \
                 Relative paths are preferred for portability.",
                import.alias, path
            ));
        }

        self.log_verbose(&format!(
            "Local import '{}' path basic structure OK: {}",
            import.alias, path
        ));
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

        let algorithm = parts[0].to_lowercase();
        let hex = parts[1];

        match algorithm.as_str() {
            "sha256" => {
                if hex.len() != 64 {
                    self.error_manager.log_error(&format!(
                        "[ImportsAnalyzer] Import '{}' sha256 hash must be 64 hex characters, \
                         got {}",
                        import.alias,
                        hex.len()
                    ));
                }
            }
            "sha512" => {
                if hex.len() != 128 {
                    self.error_manager.log_error(&format!(
                        "[ImportsAnalyzer] Import '{}' sha512 hash must be 128 hex characters, \
                         got {}",
                        import.alias,
                        hex.len()
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

    fn is_valid_identifier(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        let mut chars = s.chars();
        let first = chars.next().unwrap();
        if !first.is_alphabetic() && first != '_' {
            return false;
        }
        chars.all(|c| c.is_alphanumeric() || c == '_')
    }

    fn is_local_address(url: &str) -> bool {
        let lower = url.to_lowercase();
        lower.contains("localhost")
            || lower.contains("127.0.0.1")
            || lower.contains("::1")
            || lower.contains("0.0.0.0")
    }

    #[inline]
    fn log_debug(&self, message: &str) {
        if self.can_log_debug {
            self.error_manager.log_debug(message);
        }
    }

    #[inline]
    fn log_verbose(&self, message: &str) {
        if self.can_log_verbose {
            self.error_manager.log_debug(message);
        }
    }
}