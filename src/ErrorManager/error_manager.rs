//! ErrorManager - Singleton pattern for collecting and managing compilation errors
//!
//! CRITICAL DESIGN DECISIONS:
//! 1. Uses SINGLETON pattern (thread-safe)
//! 2. Runtime conditional logging via OperationalSettings
//! 3. Coordinated with MID_Logger (also singleton)

use super::*;
use crate::DixCore::List;
use crate::Utilities::{MID_Logger, LogLevel, Token};
use std::sync::{Arc, Mutex, OnceLock};

static SHARED_INSTANCE: OnceLock<Arc<Mutex<ErrorManager>>> = OnceLock::new();

pub struct ErrorManager {
    logger: MID_Logger,
  pub operational_settings: OperationalSettings,

    // Strongly-typed error collections
    lexical_errors: List<LexicalError>,
    parse_errors: List<ParseError>,
    semantic_errors: List<SemanticError>,
    ast_enhancement_errors: List<AstEnhancementError>,
    value_resolution_errors: List<ValueResolutionError>,
    dlm_errors: List<DLMError>,
    binary_serialization_errors: List<BinarySerializationError>,
    runtime_errors: List<RuntimeError>,
    config_errors: List<ConfigError>,
    general_errors: List<GeneralError>,

    has_errors: bool,
    max_nesting_indicator_length: usize,

    pub is_debug_enabled: bool,
    pub is_info_enabled: bool,
    pub is_warning_enabled: bool,
    pub is_error_enabled: bool,
}

impl ErrorManager {
    /// Private constructor - use get_shared_instance() instead
    fn new() -> Self {
        let logger = MID_Logger::GetSharedInstance();

        let operational_settings = OperationalSettings::default();

        logger.Debug("ErrorManager singleton instance created with default settings");

        Self {
            logger,
            operational_settings,
            lexical_errors: List::New(),
            parse_errors: List::New(),
            semantic_errors: List::New(),
            ast_enhancement_errors: List::New(),
            value_resolution_errors: List::New(),
            dlm_errors: List::New(),
            binary_serialization_errors: List::New(),
            runtime_errors: List::New(),
            config_errors: List::New(),
            general_errors: List::New(),
            has_errors: false,
            max_nesting_indicator_length: 20,
            is_debug_enabled: false,
            is_info_enabled: false,
            is_warning_enabled: false,
            is_error_enabled: false,
        }
    }

    /// Get the shared singleton instance (thread-safe)
    pub fn get_shared_instance() -> Arc<Mutex<ErrorManager>> {
        SHARED_INSTANCE
            .get_or_init(|| Arc::new(Mutex::new(ErrorManager::new())))
            .clone()
    }

    /// Reset singleton instance (for testing only)
    pub fn reset_shared_instance() {
        // Note: OnceLock doesn't support reset in stable Rust
        // This is a limitation - in tests, create new instances instead
    }

    /// Update operational settings after @CONFIG processing
    pub fn update_settings(&mut self, settings: OperationalSettings) {
        self.operational_settings = settings.clone();

        // Sync logger settings
        let log_level = match settings.debug_mode {
            DebugMode::Off => LogLevel::Error,
            DebugMode::Regular => LogLevel::Info,
            DebugMode::Verbose => LogLevel::Debug,
        };

        self.logger.SetLogLevel(log_level);

        self.is_debug_enabled = log_level == LogLevel::Debug;
        self.is_info_enabled = matches!(log_level, LogLevel::Info | LogLevel::Debug);
        self.is_warning_enabled = matches!(log_level, LogLevel::Warning | LogLevel::Info | LogLevel::Debug);
        self.is_error_enabled = true;

        self.logger.Info("ErrorManager settings updated from @CONFIG:");
        self.logger.Info(&format!("  - Error Handling: {:?}", settings.error_handling_strategy));
        self.logger.Info(&format!("  - Debug Mode: {:?}", settings.debug_mode));
        self.logger.Info(&format!("  - Compatibility: {:?}", settings.compatibility_mode));
    }

    // ========== Error Severity Determination ==========

    fn determine_error_severity(&self, source: ErrorSource) -> ErrorSeverity {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            match source {
                ErrorSource::Lexer
                | ErrorSource::Parser
                | ErrorSource::AstEnhancement
                | ErrorSource::ValueResolution
                | ErrorSource::DLM
                | ErrorSource::BinarySerialization
                | ErrorSource::Runtime
                | ErrorSource::Configuration => ErrorSeverity::Fatal,
                ErrorSource::SemanticAnalyzer => ErrorSeverity::Warning,
                _ => ErrorSeverity::Info,
            }
        } else {
            match source {
                ErrorSource::SemanticAnalyzer => ErrorSeverity::Warning,
                _ => ErrorSeverity::Error,
            }
        }
    }

    fn handle_error_based_on_strategy(&self, error_message: &str, _source: ErrorSource) {
        match self.operational_settings.error_handling_strategy {
            ErrorHandlingStrategy::Halt => {
                self.logger.Error(&format!("🛑 HALT: {}", error_message));
            }
            ErrorHandlingStrategy::Continue => {
                self.logger.Warning(&format!("⚠️  CONTINUE: {}", error_message));
            }
            ErrorHandlingStrategy::Recover => {
                self.logger.Warning(&format!("🔄 RECOVER: {}", error_message));
            }
        }
    }

    // ========== Lexical Errors ==========

    pub fn add_lexical_error(
        &mut self,
        error_type: LexicalErrorType,
        message: String,
        line: usize,
        column: usize,
        suggestion: Option<String>,
        source_line: Option<String>,
    ) {
        let severity = self.determine_error_severity(ErrorSource::Lexer);
        let error = LexicalError::new(error_type, message, line, column, suggestion, source_line, severity);

        self.handle_error_based_on_strategy(&format!("[Lexer] {}", error), ErrorSource::Lexer);

        self.lexical_errors.Add(error);
        self.has_errors = true;
    }

    pub fn get_lexical_errors(&self) -> List<LexicalError> {
        self.lexical_errors.clone()
    }

    // ========== Parse Errors ==========

    pub fn add_parse_error(
        &mut self,
        error_type: ParseErrorType,
        message: String,
        line: usize,
        column: usize,
        suggestion: Option<String>,
        source_line: Option<String>,
    ) {
        let severity = self.determine_error_severity(ErrorSource::Parser);
        let error = ParseError::new(error_type, message, line, column, suggestion, source_line, severity);

        self.handle_error_based_on_strategy(&format!("[Parser] {}", error), ErrorSource::Parser);

        self.parse_errors.Add(error);
        self.has_errors = true;
    }

    pub fn add_parse_error_from_token(
        &mut self,
        error_type: ParseErrorType,
        token: &Token,
        message: String,
        context_tokens: Option<&List<Token>>,
        source_line: Option<String>,
    ) {
        let suggestion = ParseError::generate_suggestion(&error_type, token, context_tokens);
        self.add_parse_error(
            error_type,
            message,
            token.Line,
            token.Column,
            Some(suggestion),
            source_line,
        );
    }

    pub fn add_registry_error(
        &mut self,
        error_type: ParseErrorType,
        object_name: &str,
        method_name: &str,
        line: usize,
        column: usize,
        source_line: Option<String>,
    ) {
        let error = ParseError::create_registry_error(
            error_type,
            object_name,
            method_name,
            line,
            column,
            source_line,
        );

        self.handle_error_based_on_strategy(&format!("[Registry] {}", error), ErrorSource::Parser);

        self.parse_errors.Add(error);
        self.has_errors = true;
    }

    pub fn get_parse_errors(&self) -> List<ParseError> {
        self.parse_errors.clone()
    }

    pub fn get_registry_errors(&self) -> List<ParseError> {
        self.parse_errors.Where(|e| {
            matches!(
                e.error_type,
                ParseErrorType::UnknownStaticObject
                    | ParseErrorType::UnknownStaticMethod
                    | ParseErrorType::UnknownInstanceMethod
                    | ParseErrorType::InvalidMethodSignature
                    | ParseErrorType::InvalidBuiltinCall
            )
        })
    }

    // ========== Semantic Errors ==========

    pub fn add_semantic_error(
        &mut self,
        error_type: SemanticErrorType,
        message: String,
        line: i32,
        column: i32,
        section_name: Option<String>,
        suggestion: Option<String>,
    ) {
        let severity = self.determine_error_severity(ErrorSource::SemanticAnalyzer);
        let error = SemanticError::new(error_type, message, line, column, section_name, suggestion, severity);

        self.handle_error_based_on_strategy(&format!("[Semantic] {}", error), ErrorSource::SemanticAnalyzer);

        self.semantic_errors.Add(error);
        self.has_errors = true;
    }

    pub fn get_semantic_errors(&self) -> List<SemanticError> {
        self.semantic_errors.clone()
    }

    // ========== Value Resolution Errors ==========

    pub fn add_value_resolution_error(
        &mut self,
        error_type: ValueResolutionErrorType,
        message: String,
        line: usize,
        column: usize,
        suggestion: Option<String>,
        function_name: Option<String>,
        variable_name: Option<String>,
        location: Option<String>,
    ) {
        let severity = self.determine_error_severity(ErrorSource::ValueResolution);
        let error = ValueResolutionError::new(
            error_type,
            message,
            line,
            column,
            suggestion,
            function_name,
            variable_name,
            location,
            severity,
        );

        self.handle_error_based_on_strategy(&format!("[ValueResolution] {}", error), ErrorSource::ValueResolution);

        self.value_resolution_errors.Add(error);
        self.has_errors = true;
    }

    pub fn get_value_resolution_errors(&self) -> List<ValueResolutionError> {
        self.value_resolution_errors.clone()
    }

    // ========== Public API Methods ==========

    pub fn has_errors(&self) -> bool {
        self.has_errors
    }

    pub fn should_terminate_parsing(&self) -> bool {
        self.has_errors && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

    pub fn supports_recovery(&self) -> bool {
        self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover
    }

    pub fn should_continue_parsing(&self) -> bool {
        self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Halt
    }

    pub fn can_continue(&self) -> bool {
        !self.should_terminate_parsing()
    }

    pub fn clear_errors(&mut self) {
        self.lexical_errors.Clear();
        self.parse_errors.Clear();
        self.semantic_errors.Clear();
        self.ast_enhancement_errors.Clear();
        self.value_resolution_errors.Clear();
        self.dlm_errors.Clear();
        self.binary_serialization_errors.Clear();
        self.runtime_errors.Clear();
        self.config_errors.Clear();
        self.general_errors.Clear();
        self.has_errors = false;
        self.logger.ClearLogBuffer();
        self.logger.Debug("All errors cleared");
    }

    pub fn generate_error_report(&self) -> String {
        if !self.has_errors {
            return "No errors detected.".to_string();
        }
//un comment after downloading chrono
        let mut report = String::new();
        report.push_str("=== DixScript Error Report v2.0.0 ===\n");
      //  report.push_str(&format!("Generated: {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
        report.push_str(&format!("Error Handling Strategy: {:?}\n", self.operational_settings.error_handling_strategy));
        report.push_str(&format!("Debug Mode: {:?}\n", self.operational_settings.debug_mode));
        report.push_str("\n");

        if !self.config_errors.IsEmpty() {
            report.push_str("=== Configuration Errors ===\n");
            for error in self.config_errors.Iter() {
                report.push_str(&format!("{}\n", error));
            }
        }

        if !self.lexical_errors.IsEmpty() {
            report.push_str("=== Lexical Errors ===\n");
            for error in self.lexical_errors.Iter() {
                report.push_str(&format!("{}\n", error));
            }
        }

        if !self.parse_errors.IsEmpty() {
            report.push_str("=== Parse Errors ===\n");
            for error in self.parse_errors.Iter() {
                report.push_str(&format!("{}\n", error));
            }
        }

        let total_errors = self.lexical_errors.Count()
            + self.parse_errors.Count()
            + self.semantic_errors.Count()
            + self.value_resolution_errors.Count();

        report.push_str("=== Summary ===\n");
        report.push_str(&format!("Total errors: {}\n", total_errors));
        report.push_str(&format!("Lexical: {}\n", self.lexical_errors.Count()));
        report.push_str(&format!("Parse: {}\n", self.parse_errors.Count()));
        report.push_str(&format!("Semantic: {}\n", self.semantic_errors.Count()));

        report
    }

    // ========== Logging Methods ==========

    fn get_nesting_indicator(&self, nesting_level: usize) -> String {
        if nesting_level == 0 {
            return String::new();
        }

        let level = nesting_level.min(self.max_nesting_indicator_length);
        format!("{}>", "-".repeat(level))
    }

    pub fn log_debug(&self, message: &str, nesting_level: usize) {
        if self.operational_settings.debug_mode >= DebugMode::Regular {
            self.logger.Debug(&format!("{} {}", self.get_nesting_indicator(nesting_level), message));
        }
    }

    pub fn log_info(&self, message: &str, nesting_level: usize) {
        self.logger.Info(&format!("{} {}", self.get_nesting_indicator(nesting_level), message));
    }
}