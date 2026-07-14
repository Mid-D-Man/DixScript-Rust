//! Thread-safe ErrorManager singleton and per-document isolated instances.
//!
//! Two construction paths:
//! - `get_shared_instance()` — CLI / single-document pipeline (OnceLock singleton).
//! - `new_isolated()` — LSP per-document analysis (fresh Arc, no OnceLock side-effects).

use chrono::Local;
use std::fmt::Write as FmtWrite;
use std::sync::{Arc, Mutex, OnceLock};

use crate::Compiler::Core::{DebugMode, ErrorHandlingStrategy, OperationalSettings};
use crate::ErrorManager::ErrorTypes::*;
use crate::Utilities::mid_logger::LogLevel;

// =============================================================================
// LogFormat
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    #[default]
    Plain,
    Colored,
}

// =============================================================================
// DixError — unified error wrapper for flat iteration
// =============================================================================

/// Unified error type returned by `get_all_errors_flat` and `get_errors_by_severity`.
///
/// Each variant wraps the concrete per-phase error.  Use `severity()` and
/// `message()` for cross-variant access without downcasting.
#[derive(Debug, Clone)]
pub enum DixError {
    Lexical(LexicalError),
    Parse(ParseError),
    Semantic(SemanticError),
    ImportsResolution(ImportsResolutionError),
    AstEnhancement(AstEnhancementError),
    ValueResolution(ValueResolutionError),
    Dlm(DlmError),
    BinarySerialization(BinarySerializationError),
    Runtime(RuntimeError),
    Config(ConfigError),
    General(GeneralError),
}

impl DixError {
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::Lexical(e)             => e.severity,
            Self::Parse(e)               => e.severity,
            Self::Semantic(e)            => e.severity,
            Self::ImportsResolution(e)   => e.severity,
            Self::AstEnhancement(e)      => e.severity,
            Self::ValueResolution(e)     => e.severity,
            Self::Dlm(e)                 => e.severity,
            Self::BinarySerialization(e) => e.severity,
            Self::Runtime(e)             => e.severity,
            Self::Config(e)              => e.severity,
            Self::General(e)             => e.severity,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Lexical(e)             => &e.message,
            Self::Parse(e)               => &e.message,
            Self::Semantic(e)            => &e.message,
            Self::ImportsResolution(e)   => &e.message,
            Self::AstEnhancement(e)      => &e.message,
            Self::ValueResolution(e)     => &e.message,
            Self::Dlm(e)                 => &e.message,
            Self::BinarySerialization(e) => &e.message,
            Self::Runtime(e)             => &e.message,
            Self::Config(e)              => &e.message,
            Self::General(e)             => &e.message,
        }
    }
}

// =============================================================================
// DebugConfig
// =============================================================================

/// Debug and test gating flags cached from `OperationalSettings` at construction.
///
/// `is_testing` is resolved at **compile time** via `cfg!(test)`.  Any branch
/// guarded by it is eliminated entirely in release / bench / dev builds.
#[derive(Debug, Clone, Copy)]
pub struct DebugConfig {
    pub is_enabled: bool,
    pub is_verbose: bool,
    pub is_testing: bool,
}

impl DebugConfig {
    pub fn from_debug_mode(mode: DebugMode) -> Self {
        DebugConfig {
            is_enabled: matches!(mode, DebugMode::Regular | DebugMode::Verbose),
            is_verbose: matches!(mode, DebugMode::Verbose),
            is_testing: cfg!(test),
        }
    }

    pub const fn silent() -> Self {
        DebugConfig { is_enabled: false, is_verbose: false, is_testing: false }
    }

    #[cfg(test)]
    pub const fn full() -> Self {
        DebugConfig { is_enabled: true, is_verbose: true, is_testing: true }
    }
}

// =============================================================================
// Singleton storage
// =============================================================================

static ERROR_MANAGER: OnceLock<Arc<Mutex<ErrorManagerInner>>> = OnceLock::new();

// =============================================================================
// Public handle
// =============================================================================

#[derive(Clone)]
pub struct ErrorManager {
    inner: Arc<Mutex<ErrorManagerInner>>,
}

// =============================================================================
// Inner state
// =============================================================================

struct ErrorManagerInner {
    lexical_errors:              Vec<LexicalError>,
    parse_errors:                Vec<ParseError>,
    semantic_errors:             Vec<SemanticError>,
    imports_resolution_errors:   Vec<ImportsResolutionError>,
    ast_enhancement_errors:      Vec<AstEnhancementError>,
    value_resolution_errors:     Vec<ValueResolutionError>,
    dlm_errors:                  Vec<DlmError>,
    binary_serialization_errors: Vec<BinarySerializationError>,
    runtime_errors:              Vec<RuntimeError>,
    config_errors:               Vec<ConfigError>,
    general_errors:              Vec<GeneralError>,

    has_errors:           bool,
    operational_settings: OperationalSettings,

    log_buffer:       Vec<String>,
    log_level_filter: LogLevel,
    log_enabled:      bool,
    log_format:       LogFormat,
}

// =============================================================================
// ErrorManager — construction
// =============================================================================

impl ErrorManager {
    /// Returns the process-wide singleton.  Creates it on first call.
    ///
    /// Use this for CLI compilation and single-document pipelines.
    pub fn get_shared_instance() -> Self {
        let inner = ERROR_MANAGER.get_or_init(|| {
            Arc::new(Mutex::new(ErrorManagerInner::new()))
        });
        ErrorManager { inner: Arc::clone(inner) }
    }

    /// Creates a completely independent instance that does **not** touch the
    /// OnceLock singleton.  Use this for LSP per-document analysis so that
    /// parallel document pipelines never share error state.
    ///
    /// Call `force_strategy(Continue)` immediately after to ensure all
    /// diagnostics are collected regardless of what the file's @CONFIG says.
    pub fn new_isolated() -> Self {
        ErrorManager {
            inner: Arc::new(Mutex::new(ErrorManagerInner::new())),
        }
    }

    /// Same as `new_isolated`, but with `eprintln!` log output disabled from
    /// construction — no Info/Warning/Error line is ever written to stderr.
    ///
    /// Error state is still collected exactly as normal (`get_log_contents()`,
    /// `has_errors()`, the `*_errors()` getters all behave identically); this
    /// only silences the `write_log` side effect.
    ///
    /// Intended for hot-loop / high-frequency callers — fuzzing harnesses,
    /// benchmarks, or any embedding context calling `load_from_str` thousands
    /// of times a second — where unbuffered per-call `eprintln!` would
    /// dominate wall-clock time and flood the surrounding log capture. A
    /// fresh `ErrorManagerInner` otherwise defaults `log_level_filter` to
    /// `LogLevel::Info`, so even `DebugMode::Off` still prints Info-and-above
    /// lines; this is the only way to fully silence output short of that.
    pub fn new_isolated_silent() -> Self {
        let manager = Self::new_isolated();
        manager.set_log_enabled(false);
        manager
    }

    /// Stub retained for API compatibility.
    ///
    /// `OnceLock` cannot be reset in stable Rust.  For test isolation use
    /// `new_isolated()` rather than relying on this method.
    pub fn reset_shared_instance() {}
}

// =============================================================================
// ErrorManager — settings
// =============================================================================

impl ErrorManager {
    pub fn update_settings(&self, settings: OperationalSettings) {
        let mut inner = self.inner.lock().unwrap();
        inner.update_settings(settings);
    }

    /// Overrides the error handling strategy regardless of what `update_settings`
    /// stored.  Intended for LSP mode where `Continue` must always be enforced
    /// so all diagnostics are collected.
    pub fn force_strategy(&self, strategy: ErrorHandlingStrategy) {
        let mut inner = self.inner.lock().unwrap();
        inner.operational_settings.error_handling_strategy = strategy;
    }

    pub fn get_debug_mode(&self) -> DebugMode {
        let inner = self.inner.lock().unwrap();
        inner.operational_settings.debug_mode
    }
}

// =============================================================================
// ErrorManager — add errors
// =============================================================================

impl ErrorManager {
    pub fn add_lexical_error(
        &self,
        error_type:  LexicalErrorType,
        message:     String,
        line:        usize,
        column:      usize,
        suggestion:  Option<String>,
        source_line: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_lexical_error(error_type, message, line, column, suggestion, source_line);
    }

    pub fn add_parse_error(
        &self,
        error_type:  ParseErrorType,
        message:     String,
        line:        usize,
        column:      usize,
        suggestion:  Option<String>,
        source_line: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_parse_error(error_type, message, line, column, suggestion, source_line);
    }

    pub fn add_semantic_error(
        &self,
        error_type:   SemanticErrorType,
        message:      String,
        line:         i32,
        column:       i32,
        section_name: Option<String>,
        suggestion:   Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_semantic_error(error_type, message, line, column, section_name, suggestion);
    }

    pub fn add_imports_resolution_error(
        &self,
        error_type:     ImportsResolutionErrorType,
        message:        String,
        import_alias:   String,
        import_path:    Option<String>,
        resolved_path:  Option<String>,
        circular_chain: Option<Vec<String>>,
        line:           i32,
        column:         i32,
        suggestion:     Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_imports_resolution_error(
            error_type, message, import_alias, import_path,
            resolved_path, circular_chain, line, column, suggestion,
        );
    }

    pub fn add_ast_enhancement_error(
        &self,
        error_type:   AstEnhancementErrorType,
        message:      String,
        line:         i32,
        column:       i32,
        section_name: Option<String>,
        suggestion:   Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_ast_enhancement_error(
            error_type, message, line, column, section_name, suggestion,
        );
    }

    pub fn add_value_resolution_error(
        &self,
        error_type:    ValueResolutionErrorType,
        message:       String,
        line:          i32,
        column:        i32,
        section_name:  Option<String>,
        variable_name: Option<String>,
        function_name: Option<String>,
        suggestion:    Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_value_resolution_error(
            error_type, message, line, column,
            section_name, variable_name, function_name, suggestion,
        );
    }

    pub fn add_dlm_error(
        &self,
        error_type:    DlmErrorType,
        message:       String,
        library_path:  Option<String>,
        function_name: Option<String>,
        suggestion:    Option<String>,
        severity:      ErrorSeverity,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_dlm_error(error_type, message, library_path, function_name, suggestion, severity);
    }

    pub fn add_binary_serialization_error(
        &self,
        error_type:       BinarySerializationErrorType,
        message:          String,
        file_path:        Option<String>,
        expected_version: Option<String>,
        actual_version:   Option<String>,
        suggestion:       Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_binary_serialization_error(
            error_type, message, file_path, expected_version, actual_version, suggestion,
        );
    }

    pub fn add_runtime_error(
        &self,
        error_type:    RuntimeErrorType,
        message:       String,
        function_name: Option<String>,
        line:          i32,
        column:        i32,
        stack_trace:   Vec<String>,
        suggestion:    Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_runtime_error(
            error_type, message, function_name, line, column, stack_trace, suggestion,
        );
    }

    pub fn add_runtime_error_with_severity(
        &self,
        error_type:    RuntimeErrorType,
        message:       String,
        function_name: Option<String>,
        line:          i32,
        column:        i32,
        stack_trace:   Vec<String>,
        suggestion:    Option<String>,
        severity:      ErrorSeverity,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_runtime_error_with_severity(
            error_type, message, function_name, line, column, stack_trace, suggestion, severity,
        );
    }

    pub fn add_config_error(
        &self,
        error_type:     ConfigErrorType,
        message:        String,
        section_name:   Option<String>,
        field_name:     Option<String>,
        expected_value: Option<String>,
        actual_value:   Option<String>,
        line:           i32,
        column:         i32,
        suggestion:     Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_config_error(
            error_type, message, section_name, field_name,
            expected_value, actual_value, line, column, suggestion,
        );
    }

    pub fn add_general_error(
        &self,
        error_type:   GeneralErrorType,
        message:      String,
        context:      Option<String>,
        source_error: Option<String>,
        suggestion:   Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_general_error(error_type, message, context, source_error, suggestion);
    }
}

// =============================================================================
// ErrorManager — typed getters (Vec<T>)
// =============================================================================

impl ErrorManager {
    pub fn get_lexical_errors(&self) -> Vec<LexicalError> {
        self.inner.lock().unwrap().lexical_errors.clone()
    }

    pub fn get_parse_errors(&self) -> Vec<ParseError> {
        self.inner.lock().unwrap().parse_errors.clone()
    }

    pub fn get_registry_errors(&self) -> Vec<ParseError> {
        self.inner.lock().unwrap().parse_errors.iter()
            .filter(|e| matches!(
                e.error_type,
                ParseErrorType::UnknownStaticObject
                | ParseErrorType::UnknownStaticMethod
                | ParseErrorType::UnknownInstanceMethod
                | ParseErrorType::InvalidMethodSignature
                | ParseErrorType::InvalidBuiltinCall
            ))
            .cloned()
            .collect()
    }

    pub fn get_semantic_errors(&self) -> Vec<SemanticError> {
        self.inner.lock().unwrap().semantic_errors.clone()
    }

    pub fn get_imports_resolution_errors(&self) -> Vec<ImportsResolutionError> {
        self.inner.lock().unwrap().imports_resolution_errors.clone()
    }

    pub fn get_ast_enhancement_errors(&self) -> Vec<AstEnhancementError> {
        self.inner.lock().unwrap().ast_enhancement_errors.clone()
    }

    pub fn get_value_resolution_errors(&self) -> Vec<ValueResolutionError> {
        self.inner.lock().unwrap().value_resolution_errors.clone()
    }

    pub fn get_dlm_errors(&self) -> Vec<DlmError> {
        self.inner.lock().unwrap().dlm_errors.clone()
    }

    pub fn get_binary_serialization_errors(&self) -> Vec<BinarySerializationError> {
        self.inner.lock().unwrap().binary_serialization_errors.clone()
    }

    pub fn get_runtime_errors(&self) -> Vec<RuntimeError> {
        self.inner.lock().unwrap().runtime_errors.clone()
    }

    pub fn get_config_errors(&self) -> Vec<ConfigError> {
        self.inner.lock().unwrap().config_errors.clone()
    }

    pub fn get_general_errors(&self) -> Vec<GeneralError> {
        self.inner.lock().unwrap().general_errors.clone()
    }
}

// =============================================================================
// ErrorManager — typed getters (_as_string variants)
// =============================================================================

impl ErrorManager {
    pub fn get_lexical_errors_as_string(&self) -> String {
        self.inner.lock().unwrap().lexical_errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn get_parse_errors_as_string(&self) -> String {
        self.inner.lock().unwrap().parse_errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn get_semantic_errors_as_string(&self) -> String {
        self.inner.lock().unwrap().semantic_errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn get_config_errors_as_string(&self) -> String {
        self.inner.lock().unwrap().config_errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn get_runtime_errors_as_string(&self) -> String {
        self.inner.lock().unwrap().runtime_errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn get_imports_resolution_errors_as_string(&self) -> String {
        self.inner.lock().unwrap().imports_resolution_errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn get_value_resolution_errors_as_string(&self) -> String {
        self.inner.lock().unwrap().value_resolution_errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn get_dlm_errors_as_string(&self) -> String {
        self.inner.lock().unwrap().dlm_errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn get_binary_serialization_errors_as_string(&self) -> String {
        self.inner.lock().unwrap().binary_serialization_errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// =============================================================================
// ErrorManager — flat iteration and severity filtering
// =============================================================================

impl ErrorManager {
    /// Returns every error from every category as a single flat `Vec<DixError>`.
    ///
    /// Errors are ordered: Config → Lexical → Parse → ImportsResolution →
    /// Semantic → AstEnhancement → ValueResolution → DLM → BinarySerialization
    /// → Runtime → General.
    pub fn get_all_errors_flat(&self) -> Vec<DixError> {
        let inner = self.inner.lock().unwrap();
        let capacity = inner.lexical_errors.len()
            + inner.parse_errors.len()
            + inner.semantic_errors.len()
            + inner.imports_resolution_errors.len()
            + inner.ast_enhancement_errors.len()
            + inner.value_resolution_errors.len()
            + inner.dlm_errors.len()
            + inner.binary_serialization_errors.len()
            + inner.runtime_errors.len()
            + inner.config_errors.len()
            + inner.general_errors.len();

        let mut all = Vec::with_capacity(capacity);
        all.extend(inner.config_errors.iter().cloned().map(DixError::Config));
        all.extend(inner.lexical_errors.iter().cloned().map(DixError::Lexical));
        all.extend(inner.parse_errors.iter().cloned().map(DixError::Parse));
        all.extend(inner.imports_resolution_errors.iter().cloned().map(DixError::ImportsResolution));
        all.extend(inner.semantic_errors.iter().cloned().map(DixError::Semantic));
        all.extend(inner.ast_enhancement_errors.iter().cloned().map(DixError::AstEnhancement));
        all.extend(inner.value_resolution_errors.iter().cloned().map(DixError::ValueResolution));
        all.extend(inner.dlm_errors.iter().cloned().map(DixError::Dlm));
        all.extend(inner.binary_serialization_errors.iter().cloned().map(DixError::BinarySerialization));
        all.extend(inner.runtime_errors.iter().cloned().map(DixError::Runtime));
        all.extend(inner.general_errors.iter().cloned().map(DixError::General));
        all
    }

    /// Returns all errors whose severity matches the given level.
    pub fn get_errors_by_severity(&self, severity: ErrorSeverity) -> Vec<DixError> {
        self.get_all_errors_flat()
            .into_iter()
            .filter(|e| e.severity() == severity)
            .collect()
    }
}

// =============================================================================
// ErrorManager — state queries
// =============================================================================

impl ErrorManager {
    pub fn has_errors(&self) -> bool {
        self.inner.lock().unwrap().has_errors
    }

    pub fn has_fatal_errors(&self) -> bool {
        self.inner.lock().unwrap().has_fatal_errors()
    }

    pub fn should_terminate_parsing(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.has_errors && matches!(
            inner.operational_settings.error_handling_strategy,
            ErrorHandlingStrategy::Halt
        )
    }

    pub fn clear_errors(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.clear_errors();
    }

    pub fn get_error_counts_by_severity(&self) -> std::collections::HashMap<ErrorSeverity, usize> {
        self.inner.lock().unwrap().get_error_counts_by_severity()
    }
}

// =============================================================================
// ErrorManager — reporting
// =============================================================================

impl ErrorManager {
    pub fn generate_error_report(&self) -> String {
        self.inner.lock().unwrap().generate_error_report()
    }

    pub fn get_all_errors_as_json(&self, pretty_print: bool) -> Result<String, String> {
        self.inner.lock().unwrap().get_all_errors_as_json(pretty_print)
    }

    pub fn get_log_contents(&self) -> String {
        self.inner.lock().unwrap().log_buffer.join("\n")
    }

    pub fn get_debug_info(&self) -> std::collections::HashMap<String, String> {
        let inner = self.inner.lock().unwrap();
        let mut info = std::collections::HashMap::new();
        info.insert("version".to_string(), "1.0.0".to_string());
        info.insert("has_errors".to_string(), inner.has_errors.to_string());
        info.insert(
            "error_handling_strategy".to_string(),
            format!("{:?}", inner.operational_settings.error_handling_strategy),
        );
        info.insert(
            "debug_mode".to_string(),
            format!("{:?}", inner.operational_settings.debug_mode),
        );

        let total = inner.lexical_errors.len()
            + inner.parse_errors.len()
            + inner.semantic_errors.len()
            + inner.imports_resolution_errors.len()
            + inner.ast_enhancement_errors.len()
            + inner.value_resolution_errors.len()
            + inner.dlm_errors.len()
            + inner.binary_serialization_errors.len()
            + inner.runtime_errors.len()
            + inner.config_errors.len()
            + inner.general_errors.len();

        info.insert("total_errors".to_string(), total.to_string());
        info.insert("lexical_errors".to_string(), inner.lexical_errors.len().to_string());
        info.insert("parse_errors".to_string(), inner.parse_errors.len().to_string());
        info.insert("semantic_errors".to_string(), inner.semantic_errors.len().to_string());
        info.insert("imports_errors".to_string(), inner.imports_resolution_errors.len().to_string());
        info
    }
}

// =============================================================================
// ErrorManager — logging delegation
// =============================================================================

impl ErrorManager {
    pub fn log_debug(&self, message: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.write_log(LogLevel::Debug, message);
    }

    pub fn log_info(&self, message: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.write_log(LogLevel::Info, message);
    }

    pub fn log_warning(&self, message: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.write_log(LogLevel::Warning, message);
    }

    pub fn log_error(&self, message: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.write_log(LogLevel::Error, message);
    }

    /// Turns `eprintln!` log output on/off without touching error collection
    /// (`log_buffer` still accumulates lines regardless — this only gates the
    /// `write_log` side effect). See `new_isolated_silent` for the common
    /// case of wanting this off from construction.
    pub fn set_log_enabled(&self, enabled: bool) {
        let mut inner = self.inner.lock().unwrap();
        inner.log_enabled = enabled;
    }
}

// =============================================================================
// ErrorManagerInner — construction and settings
// =============================================================================

impl ErrorManagerInner {
    fn new() -> Self {
        ErrorManagerInner {
            lexical_errors:              Vec::new(),
            parse_errors:                Vec::new(),
            semantic_errors:             Vec::new(),
            imports_resolution_errors:   Vec::new(),
            ast_enhancement_errors:      Vec::new(),
            value_resolution_errors:     Vec::new(),
            dlm_errors:                  Vec::new(),
            binary_serialization_errors: Vec::new(),
            runtime_errors:              Vec::new(),
            config_errors:               Vec::new(),
            general_errors:              Vec::new(),
            has_errors:                  false,
            operational_settings:        OperationalSettings::default(),
            log_buffer:                  Vec::new(),
            log_level_filter:            LogLevel::Info,
            log_enabled:                 true,
            log_format:                  LogFormat::Colored,
        }
    }

    fn update_settings(&mut self, settings: OperationalSettings) {
        self.log_level_filter = match settings.debug_mode {
            DebugMode::Off              => LogLevel::Info,
            DebugMode::Regular
            | DebugMode::Verbose        => LogLevel::Debug,
        };
        self.operational_settings = settings;
    }
}

// =============================================================================
// ErrorManagerInner — inlined logging
// =============================================================================

impl ErrorManagerInner {
    /// Writes one formatted log line.
    ///
    /// FIX (Group A — CLI stdout pollution): both branches previously used
    /// `println!`, sending every Info/Warning/Error log line — including the
    /// ANSI color codes from `write_to_console_colored` — to **stdout**. This
    /// silently broke `--json` (the JSON envelope was no longer the first
    /// thing on stdout, so `serde_json::from_str` failed at line 1, col 1) and
    /// `--quiet` (stdout was never actually empty).
    ///
    /// Diagnostics/logs now go to stderr, which is where they belong per Unix
    /// convention — stdout stays reserved for the command's actual result
    /// (JSON envelope, formatted file body, etc.). `log_buffer` still captures
    /// every line regardless, so programmatic log access (LSP output channel,
    /// `get_log_contents()`) is unchanged.
    fn write_log(&mut self, level: LogLevel, message: &str) {
        if !self.log_enabled { return; }
        if level < self.log_level_filter { return; }

        let line = self.format_log_line(level, message);

        match self.log_format {
            LogFormat::Colored => Self::write_to_console_colored(&line, level),
            LogFormat::Plain   => eprintln!("{}", line),
        }

        self.log_buffer.push(line);
    }

    fn format_log_line(&self, level: LogLevel, message: &str) -> String {
        let timestamp = Local::now().format("%H:%M:%S%.3f");
        format!("[{}] [{:?}] {}", timestamp, level, message)
    }

    /// See `write_log` — routed to stderr so CLI stdout (JSON / `--quiet`)
    /// stays clean.
    fn write_to_console_colored(line: &str, level: LogLevel) {
        match level {
            LogLevel::Debug   => eprintln!("\x1b[90m{}\x1b[0m", line),
            LogLevel::Info    => eprintln!("\x1b[97m{}\x1b[0m", line),
            LogLevel::Warning => eprintln!("\x1b[93m{}\x1b[0m", line),
            LogLevel::Error   => eprintln!("\x1b[91m{}\x1b[0m", line),
            LogLevel::None    => {}
        }
    }
}

// =============================================================================
// ErrorManagerInner — severity determination
// =============================================================================

impl ErrorManagerInner {
    fn determine_severity(&self, source: ErrorSource) -> ErrorSeverity {
        let is_halt = matches!(
            self.operational_settings.error_handling_strategy,
            ErrorHandlingStrategy::Halt
        );

        match source {
            ErrorSource::Lexer
            | ErrorSource::Parser
            | ErrorSource::ImportsResolution if is_halt => ErrorSeverity::Fatal,

            ErrorSource::Lexer
            | ErrorSource::Parser
            | ErrorSource::ImportsResolution
            | ErrorSource::AstEnhancement
            | ErrorSource::ValueResolution
            | ErrorSource::BinarySerialization
            | ErrorSource::DLM
            | ErrorSource::Runtime
            | ErrorSource::Configuration => ErrorSeverity::Error,

            ErrorSource::SemanticAnalyzer => ErrorSeverity::Warning,

            _ => ErrorSeverity::Info,
        }
    }
}

// =============================================================================
// ErrorManagerInner — add error methods
// =============================================================================

impl ErrorManagerInner {
    fn add_lexical_error(
        &mut self,
        error_type:  LexicalErrorType,
        message:     String,
        line:        usize,
        column:      usize,
        suggestion:  Option<String>,
        source_line: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::Lexer);
        let error = LexicalError::new(
            error_type, message, line, column, suggestion, source_line, severity,
        );
        self.write_log(LogLevel::Error, &format!("[Lexer] {}", error));
        self.lexical_errors.push(error);
        self.has_errors = true;
    }

    fn add_parse_error(
        &mut self,
        error_type:  ParseErrorType,
        message:     String,
        line:        usize,
        column:      usize,
        suggestion:  Option<String>,
        source_line: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::Parser);
        let error = ParseError::new(
            error_type, message, line, column, suggestion, source_line, severity,
        );
        self.write_log(LogLevel::Error, &format!("[Parser] {}", error));
        self.parse_errors.push(error);
        self.has_errors = true;
    }

    fn add_semantic_error(
        &mut self,
        error_type:   SemanticErrorType,
        message:      String,
        line:         i32,
        column:       i32,
        section_name: Option<String>,
        suggestion:   Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::SemanticAnalyzer);
        let error = SemanticError::new(
            error_type, message, line, column, section_name, suggestion, severity,
        );
        self.write_log(LogLevel::Warning, &format!("[Semantic] {}", error));
        self.semantic_errors.push(error);
        self.has_errors = true;
    }

    fn add_imports_resolution_error(
        &mut self,
        error_type:     ImportsResolutionErrorType,
        message:        String,
        import_alias:   String,
        import_path:    Option<String>,
        resolved_path:  Option<String>,
        circular_chain: Option<Vec<String>>,
        line:           i32,
        column:         i32,
        suggestion:     Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::ImportsResolution);
        let error = ImportsResolutionError::new(
            error_type, message, import_alias, import_path, resolved_path,
            circular_chain, line, column, suggestion, severity,
        );
        self.write_log(LogLevel::Error, &format!("[Imports] {}", error));
        self.imports_resolution_errors.push(error);
        self.has_errors = true;
    }

    fn add_ast_enhancement_error(
        &mut self,
        error_type:   AstEnhancementErrorType,
        message:      String,
        line:         i32,
        column:       i32,
        section_name: Option<String>,
        suggestion:   Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::AstEnhancement);
        let error = AstEnhancementError::new(
            error_type, message, line, column, section_name, suggestion, severity,
        );
        self.write_log(LogLevel::Error, &format!("[AstEnhancement] {}", error));
        self.ast_enhancement_errors.push(error);
        self.has_errors = true;
    }

    fn add_value_resolution_error(
        &mut self,
        error_type:    ValueResolutionErrorType,
        message:       String,
        line:          i32,
        column:        i32,
        section_name:  Option<String>,
        variable_name: Option<String>,
        function_name: Option<String>,
        suggestion:    Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::ValueResolution);
        let error = ValueResolutionError::new(
            error_type, message, line, column,
            section_name, variable_name, function_name, suggestion, severity,
        );
        self.write_log(LogLevel::Error, &format!("[ValueResolution] {}", error));
        self.value_resolution_errors.push(error);
        self.has_errors = true;
    }

    fn add_dlm_error(
        &mut self,
        error_type:    DlmErrorType,
        message:       String,
        library_path:  Option<String>,
        function_name: Option<String>,
        suggestion:    Option<String>,
        severity:      ErrorSeverity,
    ) {
        let error = DlmError::new(
            error_type, message, library_path, function_name, suggestion, severity,
        );
        self.write_log(LogLevel::Error, &format!("[DLM] {}", error));
        self.dlm_errors.push(error);
        self.has_errors = true;
    }

    fn add_binary_serialization_error(
        &mut self,
        error_type:       BinarySerializationErrorType,
        message:          String,
        file_path:        Option<String>,
        expected_version: Option<String>,
        actual_version:   Option<String>,
        suggestion:       Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::BinarySerialization);
        let error = BinarySerializationError::new(
            error_type, message, file_path, expected_version, actual_version, suggestion, severity,
        );
        self.write_log(LogLevel::Error, &format!("[BinarySerialization] {}", error));
        self.binary_serialization_errors.push(error);
        self.has_errors = true;
    }

    fn add_runtime_error(
        &mut self,
        error_type:    RuntimeErrorType,
        message:       String,
        function_name: Option<String>,
        line:          i32,
        column:        i32,
        stack_trace:   Vec<String>,
        suggestion:    Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::Runtime);
        let error = RuntimeError::new(
            error_type, message, function_name, line, column, stack_trace, suggestion, severity,
        );
        self.write_log(LogLevel::Error, &format!("[Runtime] {}", error));
        self.runtime_errors.push(error);
        self.has_errors = true;
    }

    fn add_runtime_error_with_severity(
        &mut self,
        error_type:    RuntimeErrorType,
        message:       String,
        function_name: Option<String>,
        line:          i32,
        column:        i32,
        stack_trace:   Vec<String>,
        suggestion:    Option<String>,
        severity:      ErrorSeverity,
    ) {
        let error = RuntimeError::new(
            error_type, message, function_name, line, column, stack_trace, suggestion, severity,
        );
        self.write_log(LogLevel::Error, &format!("[Runtime] {}", error));
        self.runtime_errors.push(error);
        self.has_errors = true;
    }

    fn add_config_error(
        &mut self,
        error_type:     ConfigErrorType,
        message:        String,
        section_name:   Option<String>,
        field_name:     Option<String>,
        expected_value: Option<String>,
        actual_value:   Option<String>,
        line:           i32,
        column:         i32,
        suggestion:     Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::Configuration);
        let error = ConfigError::new(
            error_type, message, section_name, field_name,
            expected_value, actual_value, line, column, suggestion, severity,
        );
        self.write_log(LogLevel::Error, &format!("[Config] {}", error));
        self.config_errors.push(error);
        self.has_errors = true;
    }

    fn add_general_error(
        &mut self,
        error_type:   GeneralErrorType,
        message:      String,
        context:      Option<String>,
        source_error: Option<String>,
        suggestion:   Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::General);
        let error = GeneralError::new(
            error_type, message, context, source_error, suggestion, severity,
        );
        self.write_log(LogLevel::Error, &format!("[General] {}", error));
        self.general_errors.push(error);
        self.has_errors = true;
    }
}

// =============================================================================
// ErrorManagerInner — state queries
// =============================================================================

impl ErrorManagerInner {
    fn has_fatal_errors(&self) -> bool {
        macro_rules! any_fatal {
            ($($coll:ident),+) => {
                $(self.$coll.iter().any(|e| e.severity == ErrorSeverity::Fatal))||+
            }
        }
        any_fatal!(
            lexical_errors,
            parse_errors,
            semantic_errors,
            imports_resolution_errors,
            ast_enhancement_errors,
            value_resolution_errors,
            dlm_errors,
            binary_serialization_errors,
            runtime_errors,
            config_errors,
            general_errors
        )
    }

    fn clear_errors(&mut self) {
        self.lexical_errors.clear();
        self.parse_errors.clear();
        self.semantic_errors.clear();
        self.imports_resolution_errors.clear();
        self.ast_enhancement_errors.clear();
        self.value_resolution_errors.clear();
        self.dlm_errors.clear();
        self.binary_serialization_errors.clear();
        self.runtime_errors.clear();
        self.config_errors.clear();
        self.general_errors.clear();
        self.has_errors = false;
    }

    fn get_error_counts_by_severity(&self) -> std::collections::HashMap<ErrorSeverity, usize> {
        let mut counts = std::collections::HashMap::new();
        counts.insert(ErrorSeverity::Info, 0);
        counts.insert(ErrorSeverity::Warning, 0);
        counts.insert(ErrorSeverity::Error, 0);
        counts.insert(ErrorSeverity::Fatal, 0);

        macro_rules! count_coll {
            ($($coll:ident),+) => {
                $(for e in &self.$coll {
                    *counts.entry(e.severity).or_insert(0) += 1;
                })+
            }
        }
        count_coll!(
            lexical_errors, parse_errors, semantic_errors,
            imports_resolution_errors, ast_enhancement_errors,
            value_resolution_errors, dlm_errors, binary_serialization_errors,
            runtime_errors, config_errors, general_errors
        );

        counts
    }
}

// =============================================================================
// ErrorManagerInner — reporting
// =============================================================================

impl ErrorManagerInner {
    fn generate_error_report(&self) -> String {
        let mut report = String::new();

        writeln!(report, "=== DixScript Error Report v1.0.0 ===").unwrap();
        writeln!(
            report,
            "Generated: {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ).unwrap();
        writeln!(
            report,
            "Error Handling Strategy: {:?}",
            self.operational_settings.error_handling_strategy
        ).unwrap();
        writeln!(
            report,
            "Debug Mode: {:?}",
            self.operational_settings.debug_mode
        ).unwrap();
        writeln!(report).unwrap();

        if !self.has_errors {
            writeln!(report, "No errors detected.").unwrap();
            return report;
        }

        macro_rules! write_section {
            ($coll:expr, $header:expr) => {
                if !$coll.is_empty() {
                    writeln!(report, "=== {} ===", $header).unwrap();
                    for e in &$coll {
                        writeln!(report, "{}", e).unwrap();
                        writeln!(report).unwrap();
                    }
                }
            }
        }

        write_section!(self.config_errors,               "Configuration Errors");
        write_section!(self.lexical_errors,              "Lexical Errors");
        write_section!(self.parse_errors,                "Parse Errors");
        write_section!(self.imports_resolution_errors,   "Imports Resolution Errors");
        write_section!(self.semantic_errors,             "Semantic Errors");
        write_section!(self.ast_enhancement_errors,      "AST Enhancement Errors");
        write_section!(self.value_resolution_errors,     "Value Resolution Errors");
        write_section!(self.dlm_errors,                  "DLM Errors");
        write_section!(self.binary_serialization_errors, "Binary Serialization Errors");
        write_section!(self.runtime_errors,              "Runtime Errors");
        write_section!(self.general_errors,              "General Errors");

        let total = self.lexical_errors.len()
            + self.parse_errors.len()
            + self.semantic_errors.len()
            + self.imports_resolution_errors.len()
            + self.ast_enhancement_errors.len()
            + self.value_resolution_errors.len()
            + self.dlm_errors.len()
            + self.binary_serialization_errors.len()
            + self.runtime_errors.len()
            + self.config_errors.len()
            + self.general_errors.len();

        writeln!(report, "=== Summary ===").unwrap();
        writeln!(report, "Total errors:          {}", total).unwrap();
        writeln!(report, "Config:                {}", self.config_errors.len()).unwrap();
        writeln!(report, "Lexical:               {}", self.lexical_errors.len()).unwrap();
        writeln!(report, "Parse:                 {}", self.parse_errors.len()).unwrap();
        writeln!(report, "ImportsResolution:     {}", self.imports_resolution_errors.len()).unwrap();
        writeln!(report, "Semantic:              {}", self.semantic_errors.len()).unwrap();
        writeln!(report, "AstEnhancement:        {}", self.ast_enhancement_errors.len()).unwrap();
        writeln!(report, "ValueResolution:       {}", self.value_resolution_errors.len()).unwrap();
        writeln!(report, "DLM:                   {}", self.dlm_errors.len()).unwrap();
        writeln!(report, "BinarySerialization:   {}", self.binary_serialization_errors.len()).unwrap();
        writeln!(report, "Runtime:               {}", self.runtime_errors.len()).unwrap();
        writeln!(report, "General:               {}", self.general_errors.len()).unwrap();

        report
    }

    fn get_all_errors_as_json(&self, pretty_print: bool) -> Result<String, String> {
        use serde_json::json;

        let total = self.lexical_errors.len()
            + self.parse_errors.len()
            + self.semantic_errors.len()
            + self.imports_resolution_errors.len()
            + self.ast_enhancement_errors.len()
            + self.value_resolution_errors.len()
            + self.dlm_errors.len()
            + self.binary_serialization_errors.len()
            + self.runtime_errors.len()
            + self.config_errors.len()
            + self.general_errors.len();

        let payload = json!({
            "timestamp": chrono::Local::now().to_rfc3339(),
            "error_handling_strategy": format!("{:?}", self.operational_settings.error_handling_strategy),
            "debug_mode": format!("{:?}", self.operational_settings.debug_mode),
            "summary": {
                "total_errors":          total,
                "config":                self.config_errors.len(),
                "lexical":               self.lexical_errors.len(),
                "parse":                 self.parse_errors.len(),
                "imports_resolution":    self.imports_resolution_errors.len(),
                "semantic":              self.semantic_errors.len(),
                "ast_enhancement":       self.ast_enhancement_errors.len(),
                "value_resolution":      self.value_resolution_errors.len(),
                "dlm":                   self.dlm_errors.len(),
                "binary_serialization":  self.binary_serialization_errors.len(),
                "runtime":               self.runtime_errors.len(),
                "general":               self.general_errors.len(),
            },
            "errors": {
                "config": self.config_errors.iter().map(|e| json!({
                    "error_id":    e.error_id,
                    "type":        format!("{:?}", e.error_type),
                    "severity":    format!("{:?}", e.severity),
                    "message":     e.message,
                    "section_name": e.section_name,
                    "field_name":  e.field_name,
                    "line":        e.line,
                    "column":      e.column,
                    "suggestion":  e.suggestion,
                })).collect::<Vec<_>>(),

                "lexical": self.lexical_errors.iter().map(|e| json!({
                    "error_id": e.error_id,
                    "type":     format!("{:?}", e.error_type),
                    "severity": format!("{:?}", e.severity),
                    "message":  e.message,
                    "line":     e.line,
                    "column":   e.column,
                    "suggestion": e.suggestion,
                })).collect::<Vec<_>>(),

                "parse": self.parse_errors.iter().map(|e| json!({
                    "error_id":   e.error_id,
                    "type":       format!("{:?}", e.error_type),
                    "severity":   format!("{:?}", e.severity),
                    "message":    e.message,
                    "line":       e.line,
                    "column":     e.column,
                    "suggestion": e.suggestion,
                    "quick_fixes": e.quick_fixes,
                })).collect::<Vec<_>>(),

                "imports_resolution": self.imports_resolution_errors.iter().map(|e| json!({
                    "error_id":      e.error_id,
                    "type":          format!("{:?}", e.error_type),
                    "severity":      format!("{:?}", e.severity),
                    "message":       e.message,
                    "import_alias":  e.import_alias,
                    "import_path":   e.import_path,
                    "resolved_path": e.resolved_path,
                    "circular_chain": e.circular_chain,
                    "line":          e.line,
                    "column":        e.column,
                    "suggestion":    e.suggestion,
                })).collect::<Vec<_>>(),

                "semantic": self.semantic_errors.iter().map(|e| json!({
                    "error_id":    e.error_id,
                    "type":        format!("{:?}", e.error_type),
                    "severity":    format!("{:?}", e.severity),
                    "message":     e.message,
                    "line":        e.line,
                    "column":      e.column,
                    "suggestion":  e.suggestion,
                })).collect::<Vec<_>>(),

                "runtime": self.runtime_errors.iter().map(|e| json!({
                    "error_id":    e.error_id,
                    "type":        format!("{:?}", e.error_type),
                    "severity":    format!("{:?}", e.severity),
                    "message":     e.message,
                    "line":        e.line,
                    "column":      e.column,
                    "suggestion":  e.suggestion,
                    "stack_trace": e.stack_trace,
                })).collect::<Vec<_>>(),
            }
        });

        if pretty_print {
            serde_json::to_string_pretty(&payload)
                .map_err(|e| format!("JSON serialization error: {}", e))
        } else {
            serde_json::to_string(&payload)
                .map_err(|e| format!("JSON serialization error: {}", e))
        }
    }
    }
