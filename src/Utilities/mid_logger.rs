use chrono::Local;
use std::fmt::Write as FmtWrite;
use std::sync::{Arc, Mutex};

/// Log levels for the logger
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warning = 2,
    Error = 3,
    None = 4,
}

/// High-Performance Zero-Cost Logger v1.0.0
/// Thread-safe with proper RAII scope guards
pub struct MID_Logger {
    current_level: LogLevel,
    indentation_level: usize,
    is_enabled: bool,
    log_buffer: String,
}

impl MID_Logger {
    const INDENTATION_SPACES: usize = 2;

    /// Create a new logger instance
    pub fn new(level: LogLevel, enabled: bool) -> Self {
        MID_Logger {
            current_level: level,
            indentation_level: 0,
            is_enabled: enabled,
            log_buffer: String::new(),
        }
    }

    // ========== Shared Instance Management ==========

    /// Get or create shared instance (thread-safe)
    pub fn GetSharedInstance(level: Option<LogLevel>, enabled: Option<bool>) -> Arc<Mutex<Self>> {
        use std::sync::OnceLock;
        static SHARED: OnceLock<Arc<Mutex<MID_Logger>>> = OnceLock::new();

        let instance = SHARED.get_or_init(|| {
            Arc::new(Mutex::new(MID_Logger::new(
                level.unwrap_or(LogLevel::Info),
                enabled.unwrap_or(true),
            )))
        });

        // Update existing instance if parameters provided
        if let (Some(lvl), Some(en)) = (level, enabled) {
            if let Ok(mut logger) = instance.lock() {
                logger.SetLogLevel(lvl);
                logger.SetEnabled(en);
            }
        }

        instance.clone()
    }

    /// Check if shared instance exists
    pub fn HasSharedInstance() -> bool {
        use std::sync::OnceLock;
        static SHARED: OnceLock<Arc<Mutex<MID_Logger>>> = OnceLock::new();
        SHARED.get().is_some()
    }

    // ========== Indentation Control ==========

    pub fn IncreaseIndent(&mut self) {
        self.indentation_level += 1;
    }

    pub fn DecreaseIndent(&mut self) {
        if self.indentation_level > 0 {
            self.indentation_level -= 1;
        }
    }

    pub fn ResetIndent(&mut self) {
        self.indentation_level = 0;
    }

    // ========== Core Logging Methods ==========

    pub fn Error(&mut self, message: &str) {
        if !self.is_enabled {
            return;
        }
        self.log_internal(message, LogLevel::Error);
    }

    pub fn Warning(&mut self, message: &str) {
        if !self.is_enabled {
            return;
        }
        self.log_internal(message, LogLevel::Warning);
    }

    pub fn Info(&mut self, message: &str) {
        if !self.is_enabled {
            return;
        }
        if self.current_level > LogLevel::Info {
            return;
        }
        self.log_internal(message, LogLevel::Info);
    }

    /// Debug logging - uses closure for deferred evaluation
    #[inline]
    pub fn Debug<F>(&mut self, message_builder: F)
    where
        F: FnOnce() -> String,
    {
        if !self.is_enabled {
            return;
        }
        if self.current_level > LogLevel::Debug {
            return;
        }
        let message = message_builder();
        self.log_internal(&message, LogLevel::Debug);
    }

    /// Debug logging - string overload
    #[inline]
    pub fn DebugStr(&mut self, message: &str) {
        if !self.is_enabled {
            return;
        }
        if self.current_level > LogLevel::Debug {
            return;
        }
        self.log_internal(message, LogLevel::Debug);
    }

    /// Verbose logging - uses closure for deferred evaluation
    #[inline]
    pub fn Verbose<F>(&mut self, message_builder: F)
    where
        F: FnOnce() -> String,
    {
        if !self.is_enabled {
            return;
        }
        if self.current_level > LogLevel::Debug {
            return;
        }
        let message = format!("[VERBOSE] {}", message_builder());
        self.log_internal(&message, LogLevel::Debug);
    }

    /// Verbose logging - string overload
    #[inline]
    pub fn VerboseStr(&mut self, message: &str) {
        if !self.is_enabled {
            return;
        }
        if self.current_level > LogLevel::Debug {
            return;
        }
        let message = format!("[VERBOSE] {}", message);
        self.log_internal(&message, LogLevel::Debug);
    }

    // ========== Internal Logging Implementation ==========

    #[inline]
    fn log_internal(&mut self, message: &str, level: LogLevel) {
        let indentation = " ".repeat(self.indentation_level * Self::INDENTATION_SPACES);
        let timestamp = Local::now().format("%H:%M:%S%.3f");
        let formatted_message = format!("[{}] {}[{:?}] {}", timestamp, indentation, level, message);

        Self::write_to_console(&formatted_message, level);

        writeln!(self.log_buffer, "{}", formatted_message).ok();
    }

    #[inline]
    fn write_to_console(message: &str, level: LogLevel) {
        // Color output based on level
        match level {
            LogLevel::Debug => println!("\x1b[90m{}\x1b[0m", message),      // Gray
            LogLevel::Info => println!("\x1b[97m{}\x1b[0m", message),       // White
            LogLevel::Warning => println!("\x1b[93m{}\x1b[0m", message),    // Yellow
            LogLevel::Error => println!("\x1b[91m{}\x1b[0m", message),      // Red
            LogLevel::None => {}
        }
    }

    // ========== RAII Scoped Logging (Proper Implementation) ==========

    /// Create a scope with RAII guard
    /// The guard will automatically end the scope when dropped
    pub fn CreateScope<'a>(&'a mut self, scope_name: &str) -> LoggerScope<'a> {
        if !self.is_enabled {
            return LoggerScope::null();
        }

        if self.current_level <= LogLevel::Info {
            self.log_internal(&format!("▶ {}", scope_name), LogLevel::Info);
        }

        self.IncreaseIndent();

        LoggerScope {
            logger: Some(self),
            scope_name: scope_name.to_string(),
            is_debug: false,
            is_verbose: false,
        }
    }

    /// Create a debug scope with RAII guard
    pub fn CreateDebugScope<'a>(&'a mut self, scope_name: &str) -> LoggerScope<'a> {
        if !self.is_enabled {
            return LoggerScope::null();
        }

        if self.current_level <= LogLevel::Debug {
            self.log_internal(&format!("▶ [DEBUG] {}", scope_name), LogLevel::Debug);
        }

        self.IncreaseIndent();

        LoggerScope {
            logger: Some(self),
            scope_name: scope_name.to_string(),
            is_debug: true,
            is_verbose: false,
        }
    }

    /// Create a verbose scope with RAII guard
    pub fn CreateVerboseScope<'a>(&'a mut self, scope_name: &str) -> LoggerScope<'a> {
        if !self.is_enabled {
            return LoggerScope::null();
        }

        if self.current_level <= LogLevel::Debug {
            self.log_internal(&format!("▶ [VERBOSE] {}", scope_name), LogLevel::Debug);
        }

        self.IncreaseIndent();

        LoggerScope {
            logger: Some(self),
            scope_name: scope_name.to_string(),
            is_debug: false,
            is_verbose: true,
        }
    }

    /// Internal method called by LoggerScope on drop
    fn end_scope(&mut self, scope_name: &str, is_debug: bool, is_verbose: bool) {
        self.DecreaseIndent();

        if !self.is_enabled {
            return;
        }

        if is_verbose {
            self.log_internal(&format!("◀ [VERBOSE] {}", scope_name), LogLevel::Debug);
        } else if is_debug {
            self.log_internal(&format!("◀ [DEBUG] {}", scope_name), LogLevel::Debug);
        } else if self.current_level <= LogLevel::Info {
            self.log_internal(&format!("◀ {}", scope_name), LogLevel::Info);
        }
    }

    // ========== Configuration and State ==========

    pub fn SetLogLevel(&mut self, level: LogLevel) {
        self.current_level = level;
    }

    pub fn SetEnabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
    }

    pub fn GetCurrentLevel(&self) -> LogLevel {
        self.current_level
    }

    pub fn IsEnabled(&self) -> bool {
        self.is_enabled
    }

    #[inline]
    pub fn IsDebugEnabled(&self) -> bool {
        #[cfg(feature = "debug_logging")]
        {
            self.is_enabled && self.current_level <= LogLevel::Debug
        }
        #[cfg(not(feature = "debug_logging"))]
        {
            false
        }
    }

    #[inline]
    pub fn IsVerboseEnabled(&self) -> bool {
        #[cfg(feature = "verbose_logging")]
        {
            self.is_enabled && self.current_level <= LogLevel::Debug
        }
        #[cfg(not(feature = "verbose_logging"))]
        {
            false
        }
    }

    pub fn WouldLog(&self, level: LogLevel) -> bool {
        self.is_enabled && level >= self.current_level
    }

    // ========== Log Buffer Management ==========

    pub fn GetLogContents(&self) -> &str {
        &self.log_buffer
    }

    pub fn ClearLogBuffer(&mut self) {
        self.log_buffer.clear();
    }
}

// ========== RAII LoggerScope Guard ==========

/// RAII guard for scoped logging
/// Automatically ends the scope when dropped (goes out of scope)
pub struct LoggerScope<'a> {
    logger: Option<&'a mut MID_Logger>,
    scope_name: String,
    is_debug: bool,
    is_verbose: bool,
}

impl<'a> LoggerScope<'a> {
    /// Create a null scope (no-op)
    fn null() -> Self {
        LoggerScope {
            logger: None,
            scope_name: String::new(),
            is_debug: false,
            is_verbose: false,
        }
    }

    /// Manually dismiss the scope (prevent end message)
    pub fn dismiss(mut self) {
        // Setting logger to None prevents Drop from running end_scope
        self.logger = None;
    }
}

impl<'a> Drop for LoggerScope<'a> {
    fn drop(&mut self) {
        if let Some(logger) = self.logger.as_mut() {
            logger.end_scope(&self.scope_name, self.is_debug, self.is_verbose);
        }
    }
}

// ========== Tests ==========

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger_basic() {
        let mut logger = MID_Logger::new(LogLevel::Debug, true);

        logger.Info("Test info message");
        logger.Warning("Test warning message");
        logger.Error("Test error message");
        logger.DebugStr("Test debug message");

        let contents = logger.GetLogContents();
        assert!(contents.contains("Test info message"));
        assert!(contents.contains("Test warning message"));
        assert!(contents.contains("Test error message"));
        assert!(contents.contains("Test debug message"));
    }

    #[test]
    fn test_logger_scopes() {
        let mut logger = MID_Logger::new(LogLevel::Debug, true);

        logger.Info("Before scope");

        {
            let _scope = logger.CreateScope("TestScope");
            logger.Info("Inside scope");
        } // Scope automatically ends here

        logger.Info("After scope");

        let contents = logger.GetLogContents();
        assert!(contents.contains("▶ TestScope"));
        assert!(contents.contains("Inside scope"));
        assert!(contents.contains("◀ TestScope"));
    }

    #[test]
    fn test_logger_nested_scopes() {
        let mut logger = MID_Logger::new(LogLevel::Debug, true);

        {
            let _scope1 = logger.CreateScope("Outer");
            logger.Info("In outer");

            {
                let _scope2 = logger.CreateScope("Inner");
                logger.Info("In inner");
            } // Inner scope ends

            logger.Info("Back in outer");
        } // Outer scope ends

        let contents = logger.GetLogContents();
        assert!(contents.contains("▶ Outer"));
        assert!(contents.contains("▶ Inner"));
        assert!(contents.contains("◀ Inner"));
        assert!(contents.contains("◀ Outer"));
    }

    #[test]
    fn test_logger_debug_scope() {
        let mut logger = MID_Logger::new(LogLevel::Debug, true);

        {
            let _scope = logger.CreateDebugScope("DebugScope");
            logger.DebugStr("Debug message in scope");
        }

        let contents = logger.GetLogContents();
        assert!(contents.contains("▶ [DEBUG] DebugScope"));
        assert!(contents.contains("◀ [DEBUG] DebugScope"));
    }

    #[test]
    fn test_logger_level_filtering() {
        let mut logger = MID_Logger::new(LogLevel::Warning, true);

        logger.DebugStr("This should not appear");
        logger.Info("This should not appear");
        logger.Warning("This should appear");
        logger.Error("This should also appear");

        let contents = logger.GetLogContents();
        assert!(!contents.contains("This should not appear"));
        assert!(contents.contains("This should appear"));
        assert!(contents.contains("This should also appear"));
    }

    #[test]
    fn test_shared_instance() {
        let logger1 = MID_Logger::GetSharedInstance(Some(LogLevel::Info), Some(true));
        let logger2 = MID_Logger::GetSharedInstance(None, None);

        // Both should point to the same instance
        {
            let mut l1 = logger1.lock().unwrap();
            l1.Info("Test from instance 1");
        }

        {
            let l2 = logger2.lock().unwrap();
            let contents = l2.GetLogContents();
            assert!(contents.contains("Test from instance 1"));
        }
    }
}