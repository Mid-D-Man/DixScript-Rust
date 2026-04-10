//! mdix-lsp / dixscript compiler — IMPORTS resolution phase.
//!
//! Loads `.mdix` files (local and cloud) referenced in `@IMPORTS`,
//! runs an isolated semantic pass on each, and merges the resulting
//! exported namespaces into the caller's `SymbolTable`.
//!
//! ## Error manager propagation
//! `new_with_error_manager` → LSP / isolated path: the caller's
//!   per-document `ErrorManager` is used throughout (including in the
//!   recursive sub-analyzers spawned for every imported file).
//! `new` → CLI / shared path: each sub-analyzer allocates its own
//!   handle to the process-wide singleton.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::Compiler::AST::{DixScript, ImportsSection, ImportEntry, ImportSource};
use crate::Compiler::Core::{OperationalSettings, SemanticAnalysisResult};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Utilities::symbol_table::ImportedNamespace;
use crate::Compiler::GeneralSemanticAnalyzer::GeneralSemanticAnalyzer;
use crate::ErrorManager::ErrorManager;
use crate::Parser::DixScriptParser;

// ── Resolution error types ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportResolutionErrorKind {
    FileNotFound,
    ParseFailure,
    SemanticFailure,
    CircularDependency,
    NetworkFailure,
    ChecksumMismatch,
    UnsupportedScheme,
    AliasConflict,
    MaxDepthExceeded,
}

impl fmt::Display for ImportResolutionErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileNotFound       => write!(f, "FileNotFound"),
            Self::ParseFailure       => write!(f, "ParseFailure"),
            Self::SemanticFailure    => write!(f, "SemanticFailure"),
            Self::CircularDependency => write!(f, "CircularDependency"),
            Self::NetworkFailure     => write!(f, "NetworkFailure"),
            Self::ChecksumMismatch   => write!(f, "ChecksumMismatch"),
            Self::UnsupportedScheme  => write!(f, "UnsupportedScheme"),
            Self::AliasConflict      => write!(f, "AliasConflict"),
            Self::MaxDepthExceeded   => write!(f, "MaxDepthExceeded"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportResolutionError {
    pub error_id:   String,
    pub error_type: ImportResolutionErrorKind,
    pub message:    String,
    pub suggestion: Option<String>,
    pub line:       u32,
    pub column:     u32,
    /// The import alias that triggered the error (for IDE highlighting).
    pub alias:      Option<String>,
}

impl ImportResolutionError {
    fn new(
        kind:       ImportResolutionErrorKind,
        message:    impl Into<String>,
        suggestion: Option<String>,
        alias:      Option<String>,
    ) -> Self {
        let error_id = format!("IMP_{}", kind.to_string().to_uppercase());
        Self {
            error_id,
            error_type: kind,
            message: message.into(),
            suggestion,
            line: 0,
            column: 0,
            alias,
        }
    }

    fn with_position(mut self, line: u32, column: u32) -> Self {
        self.line   = line;
        self.column = column;
        self
    }
}

// ── Resolution statistics ─────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ResolutionStatistics {
    pub total_imports:    usize,
    pub resolved_local:   usize,
    pub resolved_cloud:   usize,
    pub cache_hits:       usize,
    pub failed:           usize,
    pub skipped_circular: usize,
    pub duration:         Duration,
}

impl fmt::Display for ResolutionStatistics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "total={} local={} cloud={} cache_hits={} failed={} circular_skipped={} time={:.2}ms",
            self.total_imports,
            self.resolved_local,
            self.resolved_cloud,
            self.cache_hits,
            self.failed,
            self.skipped_circular,
            self.duration.as_secs_f64() * 1000.0,
        )
    }
}

// ── Cache entry ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct CachedNamespace {
    namespace:       ImportedNamespace,
    resolved_path:   String,
    analysis_result: SemanticAnalysisResult,
}

// ── ImportsResolver ───────────────────────────────────────────────────────────

/// Maximum import chain depth before a `MaxDepthExceeded` error is raised.
const MAX_IMPORT_DEPTH: usize = 16;

pub struct ImportsResolver<'a> {
    symbol_table:            &'a mut SymbolTable,
    operational_settings:    &'a OperationalSettings,
    error_manager:           ErrorManager,
    propagate_error_manager: bool,

    errors:     Vec<ImportResolutionError>,
    stats:      ResolutionStatistics,

    /// Canonical paths currently on the resolution stack — detects cycles.
    resolution_stack: Vec<String>,
    /// Simple in-process namespace cache keyed by canonical path / URL.
    namespace_cache:  HashMap<String, CachedNamespace>,
    /// Aliases already registered in this resolution pass — detects conflicts.
    registered_aliases: HashSet<String>,
}

impl<'a> ImportsResolver<'a> {
    // ── Constructors ──────────────────────────────────────────────────────────

    /// LSP / isolated path — receives a per-document `ErrorManager`.
    /// All recursive sub-resolutions also receive the same manager clone.
    pub fn new_with_error_manager(
        symbol_table:         &'a mut SymbolTable,
        operational_settings: &'a OperationalSettings,
        error_manager:        ErrorManager,
    ) -> Self {
        Self::build(symbol_table, operational_settings, error_manager, true)
    }

    /// CLI / shared path — uses the process-wide shared singleton.
    pub fn new(
        symbol_table:         &'a mut SymbolTable,
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        Self::build(
            symbol_table,
            operational_settings,
            ErrorManager::get_shared_instance(),
            false,
        )
    }

    fn build(
        symbol_table:            &'a mut SymbolTable,
        operational_settings:    &'a OperationalSettings,
        error_manager:           ErrorManager,
        propagate_error_manager: bool,
    ) -> Self {
        ImportsResolver {
            symbol_table,
            operational_settings,
            error_manager,
            propagate_error_manager,
            errors:             Vec::new(),
            stats:              ResolutionStatistics::default(),
            resolution_stack:   Vec::new(),
            namespace_cache:    HashMap::new(),
            registered_aliases: HashSet::new(),
        }
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Resolve every import in `imports_section` relative to `base_dir`.
    /// Returns `true` if all imports resolved without errors.
    pub fn resolve_from_imports_section(
        &mut self,
        imports_section: &ImportsSection,
        base_dir:        &str,
    ) -> bool {
        let started = Instant::now();
        self.stats.total_imports = imports_section.imports.len();

        if self.stats.total_imports == 0 {
            self.error_manager.log_debug("[ImportsResolver] No imports to resolve");
            return true;
        }

        self.error_manager.log_info(&format!(
            "[ImportsResolver] Resolving {} import(s) — base_dir='{}'",
            self.stats.total_imports, base_dir
        ));

        for entry in &imports_section.imports {
            self.resolve_single_entry(entry, base_dir, 0);
        }

        self.stats.duration = started.elapsed();
        self.error_manager.log_info(&format!(
            "[ImportsResolver] Done: {}", self.stats
        ));

        self.errors.is_empty()
    }

    /// Returns all resolution errors accumulated so far.
    pub fn get_errors(&self) -> &[ImportResolutionError] {
        &self.errors
    }

    /// Returns aggregate statistics for the resolution pass.
    pub fn get_statistics(&self) -> &ResolutionStatistics {
        &self.stats
    }

    /// Returns `true` if the resolution pass produced any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    // ── Single entry dispatch ─────────────────────────────────────────────────

    fn resolve_single_entry(
        &mut self,
        entry:    &ImportEntry,
        base_dir: &str,
        depth:    usize,
    ) {
        if depth > MAX_IMPORT_DEPTH {
            self.push_error(ImportResolutionError::new(
                ImportResolutionErrorKind::MaxDepthExceeded,
                format!("Import depth exceeded {} levels (alias '{}')", MAX_IMPORT_DEPTH, entry.alias),
                Some("Check for deep or circular import chains".to_string()),
                Some(entry.alias.clone()),
            ));
            self.stats.failed += 1;
            return;
        }

        // Alias conflict check
        if self.registered_aliases.contains(&entry.alias) {
            self.push_error(ImportResolutionError::new(
                ImportResolutionErrorKind::AliasConflict,
                format!("Duplicate import alias '{}'", entry.alias),
                Some(format!("Use a unique alias for each import. '{}' already exists.", entry.alias)),
                Some(entry.alias.clone()),
            ));
            self.stats.failed += 1;
            return;
        }

        match &entry.source {
            ImportSource::Local(rel_path) => {
                self.resolve_local(entry, rel_path, base_dir, depth);
            }
            ImportSource::Cloud { url, checksum } => {
                self.resolve_cloud(entry, url, checksum.as_deref(), depth);
            }
        }
    }

    // ── Local resolution ──────────────────────────────────────────────────────

    fn resolve_local(
        &mut self,
        entry:    &ImportEntry,
        rel_path: &str,
        base_dir: &str,
        depth:    usize,
    ) {
        let canonical = match self.canonicalize_local(rel_path, base_dir) {
            Ok(p)  => p,
            Err(e) => {
                self.push_error(ImportResolutionError::new(
                    ImportResolutionErrorKind::FileNotFound,
                    format!("Cannot resolve '{}': {}", rel_path, e),
                    Some(format!("Ensure the file exists relative to '{}'", base_dir)),
                    Some(entry.alias.clone()),
                ));
                self.stats.failed += 1;
                return;
            }
        };

        let canonical_str = canonical.to_string_lossy().to_string();

        self.error_manager.log_debug(&format!(
            "[ImportsResolver] local '{}' → '{}'", rel_path, canonical_str
        ));

        // Cache hit
        if let Some(cached) = self.namespace_cache.get(&canonical_str) {
            self.error_manager.log_debug(&format!(
                "[ImportsResolver] cache hit for '{}'", canonical_str
            ));
            let ns = cached.namespace.clone();
            self.register_namespace(&entry.alias, ns);
            self.stats.cache_hits += 1;
            return;
        }

        // Circular dependency check
        if self.resolution_stack.contains(&canonical_str) {
            let cycle_path = self.resolution_stack.join(" → ");
            self.push_error(ImportResolutionError::new(
                ImportResolutionErrorKind::CircularDependency,
                format!("Circular import detected: {} → {}", cycle_path, canonical_str),
                Some("Remove or break the circular import chain.".to_string()),
                Some(entry.alias.clone()),
            ));
            self.stats.skipped_circular += 1;
            return;
        }

        // Read source
        let source = match std::fs::read_to_string(&canonical) {
            Ok(s)  => s,
            Err(e) => {
                self.push_error(ImportResolutionError::new(
                    ImportResolutionErrorKind::FileNotFound,
                    format!("Cannot read '{}': {}", canonical_str, e),
                    Some("Check file permissions and path".to_string()),
                    Some(entry.alias.clone()),
                ));
                self.stats.failed += 1;
                return;
            }
        };

        self.resolution_stack.push(canonical_str.clone());
        let result = self.parse_and_analyze(&source, &canonical_str, depth);
        self.resolution_stack.pop();

        match result {
            Ok((namespace, analysis)) => {
                self.namespace_cache.insert(canonical_str.clone(), CachedNamespace {
                    namespace:       namespace.clone(),
                    resolved_path:   canonical_str,
                    analysis_result: analysis,
                });
                self.register_namespace(&entry.alias, namespace);
                self.registered_aliases.insert(entry.alias.clone());
                self.stats.resolved_local += 1;
            }
            Err(err) => {
                self.errors.push(err);
                self.stats.failed += 1;
            }
        }
    }

    // ── Cloud resolution ──────────────────────────────────────────────────────

    fn resolve_cloud(
        &mut self,
        entry:    &ImportEntry,
        url:      &str,
        checksum: Option<&str>,
        depth:    usize,
    ) {
        self.error_manager.log_debug(&format!(
            "[ImportsResolver] cloud '{}' (checksum: {})",
            url, checksum.unwrap_or("none")
        ));

        // Scheme check
        if !url.starts_with("https://") && !url.starts_with("http://") {
            self.push_error(ImportResolutionError::new(
                ImportResolutionErrorKind::UnsupportedScheme,
                format!("Unsupported URL scheme for import '{}': '{}'", entry.alias, url),
                Some("Use 'https://' for cloud imports.".to_string()),
                Some(entry.alias.clone()),
            ));
            self.stats.failed += 1;
            return;
        }

        // Warn on plain HTTP
        if url.starts_with("http://") {
            self.error_manager.log_warning(&format!(
                "[ImportsResolver] Insecure URL (http://) for import '{}' — prefer https://", entry.alias
            ));
        }

        // Cache hit
        if let Some(cached) = self.namespace_cache.get(url) {
            self.error_manager.log_debug(&format!(
                "[ImportsResolver] cache hit for cloud '{}'", url
            ));
            let ns = cached.namespace.clone();
            self.register_namespace(&entry.alias, ns);
            self.stats.cache_hits += 1;
            return;
        }

        // Circular check (URLs can also participate in import cycles)
        if self.resolution_stack.contains(&url.to_string()) {
            self.push_error(ImportResolutionError::new(
                ImportResolutionErrorKind::CircularDependency,
                format!("Circular cloud import detected for '{}'", url),
                Some("Two imports reference each other transitively.".to_string()),
                Some(entry.alias.clone()),
            ));
            self.stats.skipped_circular += 1;
            return;
        }

        // Fetch
        let source = match self.fetch_url(url) {
            Ok(s)  => s,
            Err(e) => {
                self.push_error(ImportResolutionError::new(
                    ImportResolutionErrorKind::NetworkFailure,
                    format!("Failed to fetch '{}': {}", url, e),
                    Some("Check network connectivity or use a local copy.".to_string()),
                    Some(entry.alias.clone()),
                ));
                self.stats.failed += 1;
                return;
            }
        };

        // Checksum verification
        if let Some(expected) = checksum {
            let actual = self.sha256_hex(source.as_bytes());
            if !actual.starts_with(expected) {
                self.push_error(ImportResolutionError::new(
                    ImportResolutionErrorKind::ChecksumMismatch,
                    format!(
                        "Checksum mismatch for '{}': expected prefix '{}', got '{}'",
                        url, expected, &actual[..expected.len().min(actual.len())]
                    ),
                    Some("Re-fetch the file and update the checksum in @IMPORTS.".to_string()),
                    Some(entry.alias.clone()),
                ));
                self.stats.failed += 1;
                return;
            }
            self.error_manager.log_debug(&format!(
                "[ImportsResolver] checksum OK for '{}'", url
            ));
        }

        self.resolution_stack.push(url.to_string());
        let result = self.parse_and_analyze(&source, url, depth);
        self.resolution_stack.pop();

        match result {
            Ok((namespace, analysis)) => {
                self.namespace_cache.insert(url.to_string(), CachedNamespace {
                    namespace:       namespace.clone(),
                    resolved_path:   url.to_string(),
                    analysis_result: analysis,
                });
                self.register_namespace(&entry.alias, namespace);
                self.registered_aliases.insert(entry.alias.clone());
                self.stats.resolved_cloud += 1;
            }
            Err(err) => {
                self.errors.push(err);
                self.stats.failed += 1;
            }
        }
    }

    // ── Parse + analyze an imported file ─────────────────────────────────────

    fn parse_and_analyze(
        &mut self,
        source:      &str,
        source_path: &str,
        depth:       usize,
    ) -> Result<(ImportedNamespace, SemanticAnalysisResult), ImportResolutionError> {
        // Parse
        let ast: DixScript = DixScriptParser::parse(source).map_err(|parse_err| {
            ImportResolutionError::new(
                ImportResolutionErrorKind::ParseFailure,
                format!("Parse error in '{}': {}", source_path, parse_err),
                Some("Fix syntax errors in the imported file.".to_string()),
                None,
            )
        })?;

        // Build per-import OperationalSettings that skips re-resolving imports
        // of the imported file (we handle that recursively ourselves).
        let mut import_settings = self.operational_settings.clone();
        import_settings.source_file_path         = Some(source_path.to_string());
        import_settings.skip_imports_resolution  = true; // prevent double-resolution

        // Determine the base directory for any transitive local imports.
        let transitive_base = if source_path.starts_with("http") {
            // For cloud sources, transitive local imports are not supported
            // without an explicit base — we pass empty so resolution fails
            // clearly rather than silently resolving relative to cwd.
            String::new()
        } else {
            Path::new(source_path)
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or(".")
                .to_string()
        };

        // Resolve transitive imports first, seeding a symbol table
        let mut transitive_symbol_table = SymbolTable::new();
        let transitive_namespaces = if let Some(imports_section) = &ast.imports {
            if !imports_section.imports.is_empty() && !transitive_base.is_empty() {
                let mut transitive_resolver = if self.propagate_error_manager {
                    ImportsResolver::new_with_error_manager(
                        &mut transitive_symbol_table,
                        self.operational_settings,
                        self.error_manager.clone(),
                    )
                } else {
                    ImportsResolver::new(
                        &mut transitive_symbol_table,
                        self.operational_settings,
                    )
                };

                // Pass the current resolution stack so cycles are detected
                transitive_resolver.resolution_stack = self.resolution_stack.clone();
                transitive_resolver.namespace_cache  = self.namespace_cache.clone();

                let ok = transitive_resolver
                    .resolve_from_imports_section(imports_section, &transitive_base);

                // Merge transitive errors up
                for err in transitive_resolver.errors {
                    self.errors.push(err);
                }
                // Merge cache back so subsequent imports benefit
                self.namespace_cache.extend(transitive_resolver.namespace_cache);

                if !ok {
                    self.error_manager.log_warning(&format!(
                        "[ImportsResolver] transitive import errors in '{}'", source_path
                    ));
                }

                transitive_resolver.namespace_cache
                    .into_values()
                    .map(|c| {
                        // Build alias → namespace map from cache for seeding
                        // We can't reconstruct the alias here, but the symbol
                        // table is already seeded by the transitive resolver.
                        (c.resolved_path.clone(), c.namespace)
                    })
                    .collect::<HashMap<_, _>>()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        // Run semantic analysis on the imported file with the seeded namespaces
        let analysis_result = if self.propagate_error_manager {
            GeneralSemanticAnalyzer::new_with_seed_namespaces(
                &ast,
                &import_settings,
                self.error_manager.clone(),
                &transitive_namespaces,
            )
            .analyze()
        } else {
            GeneralSemanticAnalyzer::new_with_seed_namespaces(
                &ast,
                &import_settings,
                ErrorManager::get_shared_instance(),
                &transitive_namespaces,
            )
            .analyze()
        };

        if !analysis_result.is_success {
            let error_count = analysis_result.errors.len();
            return Err(ImportResolutionError::new(
                ImportResolutionErrorKind::SemanticFailure,
                format!(
                    "Semantic analysis of '{}' failed with {} error(s)",
                    source_path, error_count
                ),
                Some("Fix semantic errors in the imported file.".to_string()),
                None,
            ));
        }

        // Extract exported symbols from the analysis result
        let namespace = self.extract_namespace_from_result(&analysis_result, source_path, depth);

        Ok((namespace, analysis_result))
    }

    // ── Namespace extraction ──────────────────────────────────────────────────

    /// Builds an `ImportedNamespace` from a successfully-analyzed imported AST.
    fn extract_namespace_from_result(
        &self,
        result:      &SemanticAnalysisResult,
        source_path: &str,
        _depth:      usize,
    ) -> ImportedNamespace {
        let symbol_table = match &result.symbol_table {
            Some(st) => st,
            None     => return ImportedNamespace::empty(source_path.to_string()),
        };

        let mut namespace = ImportedNamespace::new(source_path.to_string());

        // Export DATA properties
        for (name, data_type) in &symbol_table.data_properties {
            namespace.add_property(name.clone(), data_type.clone());
        }

        // Export QUICKFUNCS
        for (fn_name, fn_info) in &symbol_table.quick_functions {
            namespace.add_function(fn_name.clone(), fn_info.clone());
        }

        // Export ENUMS (the whole enum, not just names)
        for (enum_name, fields) in &symbol_table.enums {
            namespace.add_enum(enum_name.clone(), fields.clone());
        }

        // Export DLM modules visible to the host
        for module in &symbol_table.dlm_modules {
            namespace.add_dlm_module(module.clone());
        }

        // Export short-name index for inlay hints in the host file
        if let Some(short_index) = &result.short_name_index {
            for (k, v) in short_index {
                namespace.add_short_name(k.clone(), v.clone());
            }
        }

        self.error_manager.log_debug(&format!(
            "[ImportsResolver] namespace from '{}': {} props, {} fns, {} enums",
            source_path,
            namespace.property_count(),
            namespace.function_count(),
            namespace.enum_count(),
        ));

        namespace
    }

    // ── Symbol table registration ─────────────────────────────────────────────

    fn register_namespace(&mut self, alias: &str, namespace: ImportedNamespace) {
        self.symbol_table.register_imported_namespace(alias.to_string(), namespace);
        self.error_manager.log_debug(&format!(
            "[ImportsResolver] registered namespace under alias '{}'", alias
        ));
    }

    // ── Utilities ─────────────────────────────────────────────────────────────

    fn canonicalize_local(&self, rel_path: &str, base_dir: &str) -> Result<PathBuf, String> {
        let raw = if Path::new(rel_path).is_absolute() {
            PathBuf::from(rel_path)
        } else {
            PathBuf::from(base_dir).join(rel_path)
        };

        // Normalize without requiring the file to exist on the host (useful in
        // test / sandbox environments).  We do a best-effort lexical
        // normalization here; the actual existence check happens in
        // `resolve_local` via `read_to_string`.
        Ok(self.lexical_canonicalize(&raw))
    }

    /// Lexical path normalization — resolves `.` and `..` components without
    /// hitting the filesystem (so tests and sandboxes work).
    fn lexical_canonicalize(&self, path: &Path) -> PathBuf {
        let mut out = PathBuf::new();
        for component in path.components() {
            use std::path::Component::*;
            match component {
                CurDir        => {}
                ParentDir     => { out.pop(); }
                other         => out.push(other),
            }
        }
        out
    }

    /// Synchronous HTTP(S) fetch.  In a real async context the LSP layer
    /// should pre-fetch cloud imports and pass them as local temp files;
    /// this fallback uses `ureq` (or `reqwest` blocking) for the CLI.
    fn fetch_url(&self, url: &str) -> Result<String, String> {
        #[cfg(feature = "cloud_imports")]
        {
            ureq::get(url)
                .call()
                .map_err(|e| format!("HTTP error: {}", e))?
                .into_string()
                .map_err(|e| format!("Decode error: {}", e))
        }
        #[cfg(not(feature = "cloud_imports"))]
        {
            Err(format!(
                "Cloud imports not compiled in (feature 'cloud_imports' is off). \
                 Cannot fetch '{}'.", url
            ))
        }
    }

    /// Minimal SHA-256 hex digest without pulling in a heavy dep.
    /// Delegates to the `sha2` crate if available, otherwise returns a stub.
    fn sha256_hex(&self, bytes: &[u8]) -> String {
        #[cfg(feature = "checksum_verify")]
        {
            use sha2::{Sha256, Digest};
            let mut h = Sha256::new();
            h.update(bytes);
            format!("{:x}", h.finalize())
        }
        #[cfg(not(feature = "checksum_verify"))]
        {
            // Stub — verification is skipped; log a warning.
            self.error_manager.log_warning(
                "[ImportsResolver] checksum_verify feature disabled — skipping SHA-256 check"
            );
            String::from("00000000")
        }
    }

    fn push_error(&mut self, err: ImportResolutionError) {
        self.error_manager.log_error(&format!(
            "[ImportsResolver] {}: {}", err.error_type, err.message
        ));
        self.errors.push(err);
    }
    }
