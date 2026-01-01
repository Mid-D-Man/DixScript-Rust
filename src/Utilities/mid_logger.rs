//! MID_Logger - High-performance logger with C# style API
//! Zero-cost abstractions with compile-time conditionals

use std::sync::{Arc, Mutex};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warning = 2,
    Error = 3,
    None = 4,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warning => write!(f, "WARNING"),
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::None => write!(f, "NONE"),
        }
    }
}

/// MID_Logger - Thread-safe logger with indentation support
#[derive(Clone)]
pub struct MID_Logger {
    current_level: Arc<Mutex<LogLevel>>,
    indentation_level: Arc<Mutex<usize>>,
    is_enabled: Arc<Mutex<bool>>,
    log_buffer: Arc<Mutex<Vec<String>>>,
}

impl MID_Logger {
    const INDENTATION_SPACES: usize = 2;

    /// Creates a new logger with specified level
    pub fn New(level: LogLevel, enabled: bool) -> Self {
        Self {
            current_level: Arc::new(Mutex::new(level)),
            indentation_level: Arc::new(Mutex::new(0)),
            is_enabled: Arc::new(Mutex::new(enabled)),
            log_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Gets a shared instance (singleton pattern)
    pub fn GetSharedInstance() -> Self {
        // For now, create a new instance each time
        // In production, you'd want a true singleton
        Self::New(LogLevel::Info, true)
    }

    // ========== Core Logging Methods ==========

    /// Logs an error message (always logged)
    pub fn Error(&self, message: &str) {
        self.LogInternal(message, LogLevel::Error);
    }

    /// Logs a warning message
    pub fn Warning(&self, message: &str) {
        if !self.IsEnabled() {
            return;
        }
        if self.GetCurrentLevel() <= LogLevel::Warning {
            self.LogInternal(message, LogLevel::Warning);
        }
    }

    /// Logs an info message
    pub fn Info(&self, message: &str) {
        if !self.IsEnabled() {
            return;
        }
        if self.GetCurrentLevel() <= LogLevel::Info {
            self.LogInternal(message, LogLevel::Info);
        }
    }

    /// Logs a debug message
    pub fn Debug(&self, message: &str) {
        if !self.IsEnabled() {
            return;
        }
        if self.GetCurrentLevel() <= LogLevel::Debug {
            self.LogInternal(message, LogLevel::Debug);
        }
    }

    /// Verbose debug logging (compile-time conditional)
    #[cfg(feature = "verbose_logging")]
    pub fn Verbose(&self, message: &str) {
        if !self.IsEnabled() {
            return;
        }
        if self.GetCurrentLevel() <= LogLevel::Debug {
            self.LogInternal(&format!("[VERBOSE] {}", message), LogLevel::Debug);
        }
    }

    #[cfg(not(feature = "verbose_logging"))]
    pub fn Verbose(&self, _message: &str) {
        // No-op when verbose logging is disabled
    }

    // ========== Indentation Control ==========

    /// Increases indentation level
    pub fn IncreaseIndent(&self) {
        let mut level = self.indentation_level.lock().unwrap();
        *level += 1;
    }

    /// Decreases indentation level
    pub fn DecreaseIndent(&self) {
        let mut level = self.indentation_level.lock().unwrap();
        if *level > 0 {
            *level -= 1;
        }
    }

    /// Resets indentation to zero
    pub fn ResetIndent(&self) {
        let mut level = self.indentation_level.lock().unwrap();
        *level = 0;
    }

    // ========== Configuration ==========

    /// Sets the log level
    pub fn SetLogLevel(&self, level: LogLevel) {
        let mut current = self.current_level.lock().unwrap();
        *current = level;
    }

    /// Gets the current log level
    pub fn GetCurrentLevel(&self) -> LogLevel {
        *self.current_level.lock().unwrap()
    }

    /// Enables/disables logging
    pub fn SetEnabled(&self, enabled: bool) {
        let mut is_enabled = self.is_enabled.lock().unwrap();
        *is_enabled = enabled;
    }

    /// Returns true if logging is enabled
    pub fn IsEnabled(&self) -> bool {
        *self.is_enabled.lock().unwrap()
    }

    /// Checks if debug logging is enabled
    pub fn IsDebugEnabled(&self) -> bool {
        self.IsEnabled() && self.GetCurrentLevel() <= LogLevel::Debug
    }

    /// Checks if verbose logging is enabled
    #[cfg(feature = "verbose_logging")]
    pub fn IsVerboseEnabled(&self) -> bool {
        self.IsEnabled() && self.GetCurrentLevel() <= LogLevel::Debug
    }

    #[cfg(not(feature = "verbose_logging"))]
    pub fn IsVerboseEnabled(&self) -> bool {
        false
    }

    /// Returns true if a given level would be logged
    pub fn WouldLog(&self, level: LogLevel) -> bool {
        self.IsEnabled() && level >= self.GetCurrentLevel()
    }

    // ========== Log Buffer Management ==========

    /// Gets all logged content as a string
    pub fn GetLogContents(&self) -> String {
        let buffer = self.log_buffer.lock().unwrap();
        buffer.join("\n")
    }

    /// Clears the log buffer
    pub fn ClearLogBuffer(&self) {
        let mut buffer = self.log_buffer.lock().unwrap();
        buffer.clear();
    }

    // ========== Scoped Logging ==========

    /// Creates a logging scope (C# using pattern)
    pub fn CreateScope(&self, scope_name: &str) -> LoggerScope {
        if self.IsEnabled() && self.GetCurrentLevel() <= LogLevel::Info {
            self.LogInternal(&format!("▶ {}", scope_name), LogLevel::Info);
        }
        self.IncreaseIndent();
        LoggerScope::new(self.clone(), scope_name.to_string(), false, false)
    }

    /// Creates a debug scope
    pub fn CreateDebugScope(&self, scope_name: &str) -> LoggerScope {
        if self.IsEnabled() && self.GetCurrentLevel() <= LogLevel::Debug {
            self.LogInternal(&format!("▶ [DEBUG] {}", scope_name), LogLevel::Debug);
        }
        self.IncreaseIndent();
        LoggerScope::new(self.clone(), scope_name.to_string(), true, false)
    }

    /// Creates a verbose scope
    #[cfg(feature = "verbose_logging")]
    pub fn CreateVerboseScope(&self, scope_name: &str) -> LoggerScope {
        if self.IsEnabled() && self.GetCurrentLevel() <= LogLevel::Debug {
            self.LogInternal(&format!("▶ [VERBOSE] {}", scope_name), LogLevel::Debug);
        }
        self.IncreaseIndent();
        LoggerScope::new(self.clone(), scope_name.to_string(), false, true)
    }

    #[cfg(not(feature = "verbose_logging"))]
    pub fn CreateVerboseScope(&self, scope_name: &str) -> LoggerScope {
        // Create a no-op scope
        LoggerScope::new(self.clone(), scope_name.to_string(), false, true)
    }

    // ========== Internal Implementation ==========

    fn LogInternal(&self, message: &str, level: LogLevel) {
        if !self.IsEnabled() {
            return;
        }

        let indentation = self.indentation_level.lock().unwrap();
        let indent_str = " ".repeat(*indentation * Self::INDENTATION_SPACES);

        // Get timestamp
        let timestamp = Self::GetTimestamp();

        // Format message
        let formatted = format!(
            "[{}] {}[{}] {}",
            timestamp,
            indent_str,
            level,
            message
        );

        // Store in buffer
        let mut buffer = self.log_buffer.lock().unwrap();
        buffer.push(formatted.clone());

        // Print to console with color
        Self::WriteToConsole(&formatted, level);
    }

    fn WriteToConsole(message: &str, level: LogLevel) {
        let color = match level {
            LogLevel::Debug => "\x1b[90m",      // Gray
            LogLevel::Info => "\x1b[97m",       // White
            LogLevel::Warning => "\x1b[93m",    // Yellow
            LogLevel::Error => "\x1b[91m",      // Red
            LogLevel::None => "\x1b[0m",        // Reset
        };

        println!("{}{}\x1b[0m", color, message);
    }

    fn GetTimestamp() -> String {
        // Simple timestamp (you might want to use chrono crate for better formatting)
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap();
        let millis = now.as_millis() % 1000;
        let secs = now.as_secs() % 86400; // Seconds in a day
        let hours = (secs / 3600) % 24;
        let minutes = (secs / 60) % 60;
        let seconds = secs % 60;

        format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
    }
}

impl Default for MID_Logger {
    fn default() -> Self {
        Self::New(LogLevel::Info, true)
    }
}

// ========== LoggerScope (RAII Pattern) ==========

/// Logger scope - automatically decreases indent when dropped
pub struct LoggerScope {
    logger: MID_Logger,
    scope_name: String,
    is_debug: bool,
    is_verbose: bool,
}

impl LoggerScope {
    fn new(logger: MID_Logger, scope_name: String, is_debug: bool, is_verbose: bool) -> Self {
        Self {
            logger,
            scope_name,
            is_debug,
            is_verbose,
        }
    }
}

impl Drop for LoggerScope {
    fn drop(&mut self) {
        self.logger.DecreaseIndent();

        if !self.logger.IsEnabled() {
            return;
        }

        let message = if self.is_verbose {
            format!("◀ [VERBOSE] {}", self.scope_name)
        } else if self.is_debug {
            format!("◀ [DEBUG] {}", self.scope_name)
        } else {
            format!("◀ {}", self.scope_name)
        };

        let level = if self.is_debug || self.is_verbose {
            LogLevel::Debug
        } else {
            LogLevel::Info
        };

        if self.logger.GetCurrentLevel() <= level {
            self.logger.LogInternal(&message, level);
        }
    }
}