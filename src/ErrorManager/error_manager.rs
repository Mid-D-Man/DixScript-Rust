// src/ErrorManager/error_manager.rs

use std::sync::{Arc, Mutex, OnceLock};
use crate::ErrorManager::{
    ErrorTypes::*,
    OperationalSettings,
};
use crate::Utilities::MID_Logger;

/// Thread-safe singleton ErrorManager
/// Uses OnceLock for lazy initialization
static ERROR_MANAGER: OnceLock<Arc<Mutex<ErrorManagerInner>>> = OnceLock::new();

/// Public interface for ErrorManager
#[derive(Clone)]
pub struct ErrorManager {
    inner: Arc<Mutex<ErrorManagerInner>>,
}

/// Internal state of ErrorManager
struct ErrorManagerInner {
    // Error collections
    lexical_errors: Vec<LexicalError>,
    parse_errors: Vec<ParseError>,
    semantic_errors: Vec<SemanticError>,
    imports_resolution_errors: Vec<ImportsResolutionError>,
    ast_enhancement_errors: Vec<AstEnhancementError>,
    value_resolution_errors: Vec<ValueResolutionError>,
    dlm_errors: Vec<DlmError>,
    binary_serialization_errors: Vec<BinarySerializationError>,
    runtime_errors: Vec<RuntimeError>,
    config_errors: Vec<ConfigError>,
    general_errors: Vec<GeneralError>,

    // State
    has_errors: bool,
    operational_settings: OperationalSettings,

    // Logger reference
    logger: MID_Logger,
}

impl ErrorManager {
    /// Get the shared singleton instance
    pub fn get_shared_instance() -> Self {
        let inner = ERROR_MANAGER.get_or_init(|| {
            Arc::new(Mutex::new(ErrorManagerInner::new()))
        });

        ErrorManager {
            inner: Arc::clone(inner),
        }
    }

    /// Reset the shared instance (for testing only)
    pub fn reset_shared_instance() {
        // Note: OnceLock doesn't support reset in stable Rust
        // This is a limitation - for tests, use separate instances
        // or feature-gate with test-only code
    }

    /// Update operational settings
    pub fn update_settings(&self, settings: OperationalSettings) {
        let mut inner = self.inner.lock().unwrap();
        inner.update_settings(settings);
    }

    // ==================== LEXICAL ERRORS ====================

    /// Add a lexical error
    pub fn add_lexical_error(
        &self,
        error_type: LexicalErrorType,
        message: String,
        line: usize,
        column: usize,
        suggestion: Option<String>,
        source_line: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_lexical_error(error_type, message, line, column, suggestion, source_line);
    }

    /// Get all lexical errors (returns borrowed slice)
    pub fn get_lexical_errors(&self) -> Vec<LexicalError> {
        let inner = self.inner.lock().unwrap();
        inner.lexical_errors.clone()
    }

    // ==================== PARSE ERRORS ====================

    /// Add a parse error
    pub fn add_parse_error(
        &self,
        error_type: ParseErrorType,
        message: String,
        line: usize,
        column: usize,
        suggestion: Option<String>,
        source_line: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_parse_error(error_type, message, line, column, suggestion, source_line);
    }

    /// Get all parse errors
    pub fn get_parse_errors(&self) -> Vec<ParseError> {
        let inner = self.inner.lock().unwrap();
        inner.parse_errors.clone()
    }

    /// Get registry errors (built-in validation errors)
    pub fn get_registry_errors(&self) -> Vec<ParseError> {
        let inner = self.inner.lock().unwrap();
        inner.parse_errors.iter()
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

    // ==================== SEMANTIC ERRORS ====================

    /// Add a semantic error
    pub fn add_semantic_error(
        &self,
        error_type: SemanticErrorType,
        message: String,
        line: i32,
        column: i32,
        section_name: Option<String>,
        suggestion: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_semantic_error(error_type, message, line, column, section_name, suggestion);
    }

    /// Get all semantic errors
    pub fn get_semantic_errors(&self) -> Vec<SemanticError> {
        let inner = self.inner.lock().unwrap();
        inner.semantic_errors.clone()
    }

    // ==================== IMPORTS RESOLUTION ERRORS ====================

    /// Add an imports resolution error
    pub fn add_imports_resolution_error(
        &self,
        error_type: ImportsResolutionErrorType,
        message: String,
        import_alias: String,
        import_path: Option<String>,
        resolved_path: Option<String>,
        circular_chain: Option<Vec<String>>,
        line: i32,
        column: i32,
        suggestion: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_imports_resolution_error(
            error_type,
            message,
            import_alias,
            import_path,
            resolved_path,
            circular_chain,
            line,
            column,
            suggestion,
        );
    }

    /// Get all imports resolution errors
    pub fn get_imports_resolution_errors(&self) -> Vec<ImportsResolutionError> {
        let inner = self.inner.lock().unwrap();
        inner.imports_resolution_errors.clone()
    }

    // ==================== AST ENHANCEMENT ERRORS ====================

    /// Add an AST enhancement error
    pub fn add_ast_enhancement_error(
        &self,
        error_type: AstEnhancementErrorType,
        message: String,
        line: i32,
        column: i32,
        section_name: Option<String>,
        suggestion: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_ast_enhancement_error(error_type, message, line, column, section_name, suggestion);
    }

    /// Get all AST enhancement errors
    pub fn get_ast_enhancement_errors(&self) -> Vec<AstEnhancementError> {
        let inner = self.inner.lock().unwrap();
        inner.ast_enhancement_errors.clone()
    }

    // ==================== VALUE RESOLUTION ERRORS ====================

    /// Add a value resolution error
    pub fn add_value_resolution_error(
        &self,
        error_type: ValueResolutionErrorType,
        message: String,
        line: i32,
        column: i32,
        section_name: Option<String>,
        variable_name: Option<String>,
        function_name: Option<String>,
        suggestion: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_value_resolution_error(
            error_type,
            message,
            line,
            column,
            section_name,
            variable_name,
            function_name,
            suggestion,
        );
    }

    /// Get all value resolution errors
    pub fn get_value_resolution_errors(&self) -> Vec<ValueResolutionError> {
        let inner = self.inner.lock().unwrap();
        inner.value_resolution_errors.clone()
    }

    // ==================== DLM ERRORS ====================

    /// Add a DLM error
    pub fn add_dlm_error(
        &self,
        error_type: DlmErrorType,
        message: String,
        library_path: Option<String>,
        function_name: Option<String>,
        suggestion: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_dlm_error(error_type, message, library_path, function_name, suggestion);
    }

    /// Get all DLM errors
    pub fn get_dlm_errors(&self) -> Vec<DlmError> {
        let inner = self.inner.lock().unwrap();
        inner.dlm_errors.clone()
    }

    // ==================== BINARY SERIALIZATION ERRORS ====================

    /// Add a binary serialization error
    pub fn add_binary_serialization_error(
        &self,
        error_type: BinarySerializationErrorType,
        message: String,
        file_path: Option<String>,
        expected_version: Option<String>,
        actual_version: Option<String>,
        suggestion: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_binary_serialization_error(
            error_type,
            message,
            file_path,
            expected_version,
            actual_version,
            suggestion,
        );
    }

    /// Get all binary serialization errors
    pub fn get_binary_serialization_errors(&self) -> Vec<BinarySerializationError> {
        let inner = self.inner.lock().unwrap();
        inner.binary_serialization_errors.clone()
    }

    // ==================== RUNTIME ERRORS ====================

    /// Add a runtime error
    pub fn add_runtime_error(
        &self,
        error_type: RuntimeErrorType,
        message: String,
        function_name: Option<String>,
        line: i32,
        column: i32,
        stack_trace: Vec<String>,
        suggestion: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_runtime_error(
            error_type,
            message,
            function_name,
            line,
            column,
            stack_trace,
            suggestion,
        );
    }

    /// Get all runtime errors
    pub fn get_runtime_errors(&self) -> Vec<RuntimeError> {
        let inner = self.inner.lock().unwrap();
        inner.runtime_errors.clone()
    }

    // ==================== CONFIG ERRORS ====================

    /// Add a config error
    pub fn add_config_error(
        &self,
        error_type: ConfigErrorType,
        message: String,
        section_name: Option<String>,
        field_name: Option<String>,
        expected_value: Option<String>,
        actual_value: Option<String>,
        line: i32,
        column: i32,
        suggestion: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_config_error(
            error_type,
            message,
            section_name,
            field_name,
            expected_value,
            actual_value,
            line,
            column,
            suggestion,
        );
    }

    /// Get all config errors
    pub fn get_config_errors(&self) -> Vec<ConfigError> {
        let inner = self.inner.lock().unwrap();
        inner.config_errors.clone()
    }

    // ==================== GENERAL ERRORS ====================

    /// Add a general error
    pub fn add_general_error(
        &self,
        error_type: GeneralErrorType,
        message: String,
        context: Option<String>,
        source_error: Option<String>,
        suggestion: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_general_error(error_type, message, context, source_error, suggestion);
    }

    /// Get all general errors
    pub fn get_general_errors(&self) -> Vec<GeneralError> {
        let inner = self.inner.lock().unwrap();
        inner.general_errors.clone()
    }

    // ==================== STATE QUERIES ====================

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.has_errors
    }

    /// Check if there are any fatal errors
    pub fn has_fatal_errors(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.has_fatal_errors()
    }

    /// Should terminate parsing based on error handling strategy
    pub fn should_terminate_parsing(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.has_errors && matches!(
            inner.operational_settings.error_handling_strategy,
            crate::ErrorManager::ErrorHandlingStrategy::Halt
        )
    }

    /// Clear all errors
    pub fn clear_errors(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.clear_errors();
    }

    /// Get error counts by severity
    pub fn get_error_counts_by_severity(&self) -> std::collections::HashMap<ErrorSeverity, usize> {
        let inner = self.inner.lock().unwrap();
        inner.get_error_counts_by_severity()
    }
}

impl ErrorManagerInner {
    fn new() -> Self {
        let logger = MID_Logger::GetSharedInstance(None, None);

        ErrorManagerInner {
            lexical_errors: Vec::new(),
            parse_errors: Vec::new(),
            semantic_errors: Vec::new(),
            imports_resolution_errors: Vec::new(),
            ast_enhancement_errors: Vec::new(),
            value_resolution_errors: Vec::new(),
            dlm_errors: Vec::new(),
            binary_serialization_errors: Vec::new(),
            runtime_errors: Vec::new(),
            config_errors: Vec::new(),
            general_errors: Vec::new(),
            has_errors: false,
            operational_settings: OperationalSettings::default(),
            logger,
        }
    }

    fn update_settings(&mut self, settings: OperationalSettings) {
        self.operational_settings = settings;
        // TODO: Sync logger settings when MID_Logger is fully ported
    }

    fn determine_severity(&self, source: ErrorSource) -> ErrorSeverity {
        use crate::ErrorManager::ErrorHandlingStrategy;

        match source {
            ErrorSource::Lexer if matches!(
                self.operational_settings.error_handling_strategy,
                ErrorHandlingStrategy::Halt
            ) => ErrorSeverity::Fatal,

            ErrorSource::Parser if matches!(
                self.operational_settings.error_handling_strategy,
                ErrorHandlingStrategy::Halt
            ) => ErrorSeverity::Fatal,

            ErrorSource::ImportsResolution if matches!(
                self.operational_settings.error_handling_strategy,
                ErrorHandlingStrategy::Halt
            ) => ErrorSeverity::Fatal,

            ErrorSource::Lexer | ErrorSource::Parser | ErrorSource::ImportsResolution
            | ErrorSource::AstEnhancement | ErrorSource::ValueResolution
            | ErrorSource::BinarySerialization | ErrorSource::DLM
            | ErrorSource::Runtime | ErrorSource::Configuration => ErrorSeverity::Error,

            ErrorSource::SemanticAnalyzer => ErrorSeverity::Warning,

            _ => ErrorSeverity::Info,
        }
    }

    fn add_lexical_error(
        &mut self,
        error_type: LexicalErrorType,
        message: String,
        line: usize,
        column: usize,
        suggestion: Option<String>,
        source_line: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::Lexer);
        let error = LexicalError::new(
            error_type,
            message,
            line,
            column,
            suggestion,
            source_line,
            severity,
        );

        self.logger.Error(&format!("[Lexer] {}", error));
        self.lexical_errors.push(error);
        self.has_errors = true;
    }

    fn add_parse_error(
        &mut self,
        error_type: ParseErrorType,
        message: String,
        line: usize,
        column: usize,
        suggestion: Option<String>,
        source_line: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::Parser);
        let error = ParseError::new(
            error_type,
            message,
            line,
            column,
            suggestion,
            source_line,
            severity,
        );

        self.logger.Error(&format!("[Parser] {}", error));
        self.parse_errors.push(error);
        self.has_errors = true;
    }

    fn add_semantic_error(
        &mut self,
        error_type: SemanticErrorType,
        message: String,
        line: i32,
        column: i32,
        section_name: Option<String>,
        suggestion: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::SemanticAnalyzer);
        let error = SemanticError::new(
            error_type,
            message,
            line,
            column,
            section_name,
            suggestion,
            severity,
        );

        self.logger.Warning(&format!("[Semantic] {}", error));
        self.semantic_errors.push(error);
        self.has_errors = true;
    }

    fn add_imports_resolution_error(
        &mut self,
        error_type: ImportsResolutionErrorType,
        message: String,
        import_alias: String,
        import_path: Option<String>,
        resolved_path: Option<String>,
        circular_chain: Option<Vec<String>>,
        line: i32,
        column: i32,
        suggestion: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::ImportsResolution);
        let error = ImportsResolutionError::new(
            error_type,
            message,
            import_alias,
            import_path,
            resolved_path,
            circular_chain,
            line,
            column,
            suggestion,
            severity,
        );

        self.logger.Error(&format!("[Imports] {}", error));
        self.imports_resolution_errors.push(error);
        self.has_errors = true;
    }

    fn add_ast_enhancement_error(
        &mut self,
        error_type: AstEnhancementErrorType,
        message: String,
        line: i32,
        column: i32,
        section_name: Option<String>,
        suggestion: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::AstEnhancement);
        let error = AstEnhancementError::new(
            error_type,
            message,
            line,
            column,
            section_name,
            suggestion,
            severity,
        );

        self.logger.Error(&format!("[AstEnhancement] {}", error));
        self.ast_enhancement_errors.push(error);
        self.has_errors = true;
    }

    fn add_value_resolution_error(
        &mut self,
        error_type: ValueResolutionErrorType,
        message: String,
        line: i32,
        column: i32,
        section_name: Option<String>,
        variable_name: Option<String>,
        function_name: Option<String>,
        suggestion: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::ValueResolution);
        let error = ValueResolutionError::new(
            error_type,
            message,
            line,
            column,
            section_name,
            variable_name,
            function_name,
            suggestion,
            severity,
        );

        self.logger.Error(&format!("[ValueResolution] {}", error));
        self.value_resolution_errors.push(error);
        self.has_errors = true;
    }

    fn add_dlm_error(
        &mut self,
        error_type: DlmErrorType,
        message: String,
        library_path: Option<String>,
        function_name: Option<String>,
        suggestion: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::DLM);
        let error = DlmError::new(
            error_type,
            message,
            library_path,
            function_name,
            suggestion,
            severity,
        );

        self.logger.Error(&format!("[DLM] {}", error));
        self.dlm_errors.push(error);
        self.has_errors = true;
    }

    fn add_binary_serialization_error(
        &mut self,
        error_type: BinarySerializationErrorType,
        message: String,
        file_path: Option<String>,
        expected_version: Option<String>,
        actual_version: Option<String>,
        suggestion: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::BinarySerialization);
        let error = BinarySerializationError::new(
            error_type,
            message,
            file_path,
            expected_version,
            actual_version,
            suggestion,
            severity,
        );

        self.logger.Error(&format!("[BinarySerialization] {}", error));
        self.binary_serialization_errors.push(error);
        self.has_errors = true;
    }

    fn add_runtime_error(
        &mut self,
        error_type: RuntimeErrorType,
        message: String,
        function_name: Option<String>,
        line: i32,
        column: i32,
        stack_trace: Vec<String>,
        suggestion: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::Runtime);
        let error = RuntimeError::new(
            error_type,
            message,
            function_name,
            line,
            column,
            stack_trace,
            suggestion,
            severity,
        );

        self.logger.Error(&format!("[Runtime] {}", error));
        self.runtime_errors.push(error);
        self.has_errors = true;
    }

    fn add_config_error(
        &mut self,
        error_type: ConfigErrorType,
        message: String,
        section_name: Option<String>,
        field_name: Option<String>,
        expected_value: Option<String>,
        actual_value: Option<String>,
        line: i32,
        column: i32,
        suggestion: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::Configuration);
        let error = ConfigError::new(
            error_type,
            message,
            section_name,
            field_name,
            expected_value,
            actual_value,
            line,
            column,
            suggestion,
            severity,
        );

        self.logger.Error(&format!("[Config] {}", error));
        self.config_errors.push(error);
        self.has_errors = true;
    }

    fn add_general_error(
        &mut self,
        error_type: GeneralErrorType,
        message: String,
        context: Option<String>,
        source_error: Option<String>,
        suggestion: Option<String>,
    ) {
        let severity = self.determine_severity(ErrorSource::General);
        let error = GeneralError::new(
            error_type,
            message,
            context,
            source_error,
            suggestion,
            severity,
        );

        self.logger.Error(&format!("[General] {}", error));
        self.general_errors.push(error);
        self.has_errors = true;
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

    fn has_fatal_errors(&self) -> bool {
        self.lexical_errors.iter().any(|e| e.severity == ErrorSeverity::Fatal)
            || self.parse_errors.iter().any(|e| e.severity == ErrorSeverity::Fatal)
            || self.semantic_errors.iter().any(|e| e.severity == ErrorSeverity::Fatal)
            || self.imports_resolution_errors.iter().any(|e| e.severity == ErrorSeverity::Fatal)
            || self.ast_enhancement_errors.iter().any(|e| e.severity == ErrorSeverity::Fatal)
            || self.value_resolution_errors.iter().any(|e| e.severity == ErrorSeverity::Fatal)
            || self.dlm_errors.iter().any(|e| e.severity == ErrorSeverity::Fatal)
            || self.binary_serialization_errors.iter().any(|e| e.severity == ErrorSeverity::Fatal)
            || self.runtime_errors.iter().any(|e| e.severity == ErrorSeverity::Fatal)
            || self.config_errors.iter().any(|e| e.severity == ErrorSeverity::Fatal)
            || self.general_errors.iter().any(|e| e.severity == ErrorSeverity::Fatal)
    }

    fn get_error_counts_by_severity(&self) -> std::collections::HashMap<ErrorSeverity, usize> {
        let mut counts = std::collections::HashMap::new();
        counts.insert(ErrorSeverity::Info, 0);
        counts.insert(ErrorSeverity::Warning, 0);
        counts.insert(ErrorSeverity::Error, 0);
        counts.insert(ErrorSeverity::Fatal, 0);

        for error in &self.lexical_errors {
            *counts.entry(error.severity).or_insert(0) += 1;
        }
        for error in &self.parse_errors {
            *counts.entry(error.severity).or_insert(0) += 1;
        }
        for error in &self.semantic_errors {
            *counts.entry(error.severity).or_insert(0) += 1;
        }
        for error in &self.imports_resolution_errors {
            *counts.entry(error.severity).or_insert(0) += 1;
        }
        for error in &self.ast_enhancement_errors {
            *counts.entry(error.severity).or_insert(0) += 1;
        }
        for error in &self.value_resolution_errors {
            *counts.entry(error.severity).or_insert(0) += 1;
        }
        for error in &self.dlm_errors {
            *counts.entry(error.severity).or_insert(0) += 1;
        }
        for error in &self.binary_serialization_errors {
            *counts.entry(error.severity).or_insert(0) += 1;
        }
        for error in &self.runtime_errors {
            *counts.entry(error.severity).or_insert(0) += 1;
        }
        for error in &self.config_errors {
            *counts.entry(error.severity).or_insert(0) += 1;
        }
        for error in &self.general_errors {
            *counts.entry(error.severity).or_insert(0) += 1;
        }

        counts
    }
}

// Add to ErrorManager impl block (public interface)
impl ErrorManager {
    // ==================== ERROR REPORTING ====================

    /// Generate a comprehensive error report
    pub fn generate_error_report(&self) -> String {
        let inner = self.inner.lock().unwrap();
        inner.generate_error_report()
    }

    /// Get all errors as JSON string
    pub fn get_all_errors_as_json(&self, pretty_print: bool) -> Result<String, String> {
        let inner = self.inner.lock().unwrap();
        inner.get_all_errors_as_json(pretty_print)
    }

    /// Get log contents from logger
    pub fn get_log_contents(&self) -> String {
        let inner = self.inner.lock().unwrap();
        inner.logger.GetLogContents()
    }

    // ==================== LOGGING DELEGATION ====================

    /// Log debug message
    pub fn log_debug(&self, message: &str) {
        let inner = self.inner.lock().unwrap();
        inner.logger.Debug(message);
    }

    /// Log info message
    pub fn log_info(&self, message: &str) {
        let inner = self.inner.lock().unwrap();
        inner.logger.Info(message);
    }

    /// Log Warning message
    pub fn log_Warning(&self, message: &str) {
        let inner = self.inner.lock().unwrap();
        inner.logger.Warning(message);
    }

    /// Log error message
    pub fn log_error(&self, message: &str) {
        let inner = self.inner.lock().unwrap();
        inner.logger.Error(message);
    }

    // ==================== DEBUG INFO ====================

    /// Get debug information about ErrorManager state
    pub fn get_debug_info(&self) -> std::collections::HashMap<String, String> {
        let inner = self.inner.lock().unwrap();

        let mut info = std::collections::HashMap::new();
        info.insert("version".to_string(), "1.0.0".to_string());
        info.insert("has_errors".to_string(), inner.has_errors.to_string());
        info.insert("error_handling_strategy".to_string(),
                    format!("{:?}", inner.operational_settings.error_handling_strategy));
        info.insert("debug_mode".to_string(),
                    format!("{:?}", inner.operational_settings.debug_mode));

        let total_errors = inner.lexical_errors.len()
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

        info.insert("total_errors".to_string(), total_errors.to_string());
        info.insert("lexical_errors".to_string(), inner.lexical_errors.len().to_string());
        info.insert("parse_errors".to_string(), inner.parse_errors.len().to_string());
        info.insert("semantic_errors".to_string(), inner.semantic_errors.len().to_string());
        info.insert("imports_errors".to_string(), inner.imports_resolution_errors.len().to_string());

        info
    }
}

// Add to ErrorManagerInner impl block (internal methods)
impl ErrorManagerInner {
    fn generate_error_report(&self) -> String {
        use std::fmt::Write;

        let mut report = String::new();

        writeln!(report, "=== DixScript Error Report v1.0.0 ===").unwrap();
        writeln!(report, "Generated: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")).unwrap();
        writeln!(report, "Error Handling Strategy: {:?}", self.operational_settings.error_handling_strategy).unwrap();
        writeln!(report, "Debug Mode: {:?}", self.operational_settings.debug_mode).unwrap();
        writeln!(report).unwrap();

        if !self.has_errors {
            writeln!(report, "No errors detected.").unwrap();
            return report;
        }

        // Config errors
        if !self.config_errors.is_empty() {
            writeln!(report, "=== Configuration Errors ===").unwrap();
            for error in &self.config_errors {
                writeln!(report, "{}", error).unwrap();
                writeln!(report).unwrap();
            }
        }

        // Lexical errors
        if !self.lexical_errors.is_empty() {
            writeln!(report, "=== Lexical Errors ===").unwrap();
            for error in &self.lexical_errors {
                writeln!(report, "{}", error).unwrap();
                writeln!(report).unwrap();
            }
        }

        // Parse errors
        if !self.parse_errors.is_empty() {
            writeln!(report, "=== Parse Errors ===").unwrap();
            for error in &self.parse_errors {
                writeln!(report, "{}", error).unwrap();
                writeln!(report).unwrap();
            }
        }

        // Imports resolution errors
        if !self.imports_resolution_errors.is_empty() {
            writeln!(report, "=== Imports Resolution Errors ===").unwrap();
            for error in &self.imports_resolution_errors {
                writeln!(report, "{}", error).unwrap();
                writeln!(report).unwrap();
            }
        }

        // Semantic errors
        if !self.semantic_errors.is_empty() {
            writeln!(report, "=== Semantic Errors ===").unwrap();
            for error in &self.semantic_errors {
                writeln!(report, "{}", error).unwrap();
                writeln!(report).unwrap();
            }
        }

        // AST enhancement errors
        if !self.ast_enhancement_errors.is_empty() {
            writeln!(report, "=== AST Enhancement Errors ===").unwrap();
            for error in &self.ast_enhancement_errors {
                writeln!(report, "{}", error).unwrap();
                writeln!(report).unwrap();
            }
        }

        // Value resolution errors
        if !self.value_resolution_errors.is_empty() {
            writeln!(report, "=== Value Resolution Errors ===").unwrap();
            for error in &self.value_resolution_errors {
                writeln!(report, "{}", error).unwrap();
                writeln!(report).unwrap();
            }
        }

        // DLM errors
        if !self.dlm_errors.is_empty() {
            writeln!(report, "=== DLM Errors ===").unwrap();
            for error in &self.dlm_errors {
                writeln!(report, "{}", error).unwrap();
                writeln!(report).unwrap();
            }
        }

        // Binary serialization errors
        if !self.binary_serialization_errors.is_empty() {
            writeln!(report, "=== Binary Serialization Errors ===").unwrap();
            for error in &self.binary_serialization_errors {
                writeln!(report, "{}", error).unwrap();
                writeln!(report).unwrap();
            }
        }

        // Runtime errors
        if !self.runtime_errors.is_empty() {
            writeln!(report, "=== Runtime Errors ===").unwrap();
            for error in &self.runtime_errors {
                writeln!(report, "{}", error).unwrap();
                writeln!(report).unwrap();
            }
        }

        // General errors
        if !self.general_errors.is_empty() {
            writeln!(report, "=== General Errors ===").unwrap();
            for error in &self.general_errors {
                writeln!(report, "{}", error).unwrap();
                writeln!(report).unwrap();
            }
        }

        // Summary
        let total_errors = self.lexical_errors.len()
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
        writeln!(report, "Total errors: {}", total_errors).unwrap();
        writeln!(report, "Config: {}", self.config_errors.len()).unwrap();
        writeln!(report, "Lexical: {}", self.lexical_errors.len()).unwrap();
        writeln!(report, "Parse: {}", self.parse_errors.len()).unwrap();
        writeln!(report, "ImportsResolution: {}", self.imports_resolution_errors.len()).unwrap();
        writeln!(report, "Semantic: {}", self.semantic_errors.len()).unwrap();
        writeln!(report, "AstEnhancement: {}", self.ast_enhancement_errors.len()).unwrap();
        writeln!(report, "ValueResolution: {}", self.value_resolution_errors.len()).unwrap();
        writeln!(report, "DLM: {}", self.dlm_errors.len()).unwrap();
        writeln!(report, "BinarySerialization: {}", self.binary_serialization_errors.len()).unwrap();
        writeln!(report, "Runtime: {}", self.runtime_errors.len()).unwrap();
        writeln!(report, "General: {}", self.general_errors.len()).unwrap();

        report
    }

    fn get_all_errors_as_json(&self, pretty_print: bool) -> Result<String, String> {
        use serde_json::json;

        let total_errors = self.lexical_errors.len()
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

        let error_data = json!({
            "timestamp": chrono::Local::now().to_rfc3339(),
            "error_handling_strategy": format!("{:?}", self.operational_settings.error_handling_strategy),
            "debug_mode": format!("{:?}", self.operational_settings.debug_mode),
            "summary": {
                "total_errors": total_errors,
                "config": self.config_errors.len(),
                "lexical": self.lexical_errors.len(),
                "parse": self.parse_errors.len(),
                "imports_resolution": self.imports_resolution_errors.len(),
                "semantic": self.semantic_errors.len(),
                "ast_enhancement": self.ast_enhancement_errors.len(),
                "value_resolution": self.value_resolution_errors.len(),
                "dlm": self.dlm_errors.len(),
                "binary_serialization": self.binary_serialization_errors.len(),
                "runtime": self.runtime_errors.len(),
                "general": self.general_errors.len(),
            },
            "errors": {
                "config": self.config_errors.iter().map(|e| json!({
                    "error_id": e.error_id,
                    "type": format!("{:?}", e.error_type),
                    "severity": format!("{:?}", e.severity),
                    "message": e.message,
                    "section_name": e.section_name,
                    "field_name": e.field_name,
                    "line": e.line,
                    "column": e.column,
                    "suggestion": e.suggestion,
                })).collect::<Vec<_>>(),

                "lexical": self.lexical_errors.iter().map(|e| json!({
                    "error_id": e.error_id,
                    "type": format!("{:?}", e.error_type),
                    "severity": format!("{:?}", e.severity),
                    "message": e.message,
                    "line": e.line,
                    "column": e.column,
                    "suggestion": e.suggestion,
                })).collect::<Vec<_>>(),

                "parse": self.parse_errors.iter().map(|e| json!({
                    "error_id": e.error_id,
                    "type": format!("{:?}", e.error_type),
                    "severity": format!("{:?}", e.severity),
                    "message": e.message,
                    "line": e.line,
                    "column": e.column,
                    "suggestion": e.suggestion,
                    "quick_fixes": e.quick_fixes,
                })).collect::<Vec<_>>(),

                "imports_resolution": self.imports_resolution_errors.iter().map(|e| json!({
                    "error_id": e.error_id,
                    "type": format!("{:?}", e.error_type),
                    "severity": format!("{:?}", e.severity),
                    "message": e.message,
                    "import_alias": e.import_alias,
                    "import_path": e.import_path,
                    "resolved_path": e.resolved_path,
                    "circular_chain": e.circular_chain,
                    "line": e.line,
                    "column": e.column,
                    "suggestion": e.suggestion,
                })).collect::<Vec<_>>(),
            }
        });

        if pretty_print {
            serde_json::to_string_pretty(&error_data)
                .map_err(|e| format!("JSON serialization error: {}", e))
        } else {
            serde_json::to_string(&error_data)
                .map_err(|e| format!("JSON serialization error: {}", e))
        }
    }
}