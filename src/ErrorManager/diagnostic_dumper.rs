// src/ErrorManager/diagnostic_dumper.rs

use std::fs;
use std::path::Path;
use crate::ErrorManager::ErrorManager;

/// Diagnostic dump utility for comprehensive error and log analysis
pub struct DiagnosticDumper {
    error_manager: ErrorManager,
}

impl DiagnosticDumper {
    /// Create a new diagnostic dumper
    pub fn new() -> Self {
        DiagnosticDumper {
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Generate complete diagnostic dump to string
    pub fn generate_dump(&self) -> String {
        let mut output = String::new();

        self.write_header(&mut output);
        self.write_separator(&mut output);

        self.write_log_contents(&mut output);
        self.write_separator(&mut output);

        self.write_error_summary(&mut output);
        self.write_separator(&mut output);

        self.write_all_errors(&mut output);
        self.write_separator(&mut output);

        self.write_footer(&mut output);

        output
    }

    /// Dump diagnostics to file in project directory
    pub fn dump_to_file(&self, filename: &str) -> Result<String, String> {
        let content = self.generate_dump();

        // Get current directory
        let current_dir = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;

        // Try to find project root (where Cargo.toml is)
        let mut project_dir = current_dir.clone();
        loop {
            if project_dir.join("Cargo.toml").exists() {
                break;
            }

            match project_dir.parent() {
                Some(parent) => project_dir = parent.to_path_buf(),
                None => {
                    // Fallback to current directory
                    project_dir = current_dir;
                    break;
                }
            }
        }

        let filepath = project_dir.join(filename);

        fs::write(&filepath, &content)
            .map_err(|e| format!("Failed to write diagnostic dump: {}", e))?;

        let filepath_str = filepath.to_string_lossy().to_string();
        println!("[DiagnosticDumper] Full diagnostic dump saved to: {}", filepath_str);

        Ok(filepath_str)
    }

    // ==================== HEADER & FOOTER ====================

    fn write_header(&self, output: &mut String) {
        use std::fmt::Write;

        writeln!(output, "╔════════════════════════════════════════════════════════════════════════════╗").unwrap();
        writeln!(output, "║                    DIXSCRIPT DIAGNOSTIC DUMP                               ║").unwrap();
        writeln!(output, "╚════════════════════════════════════════════════════════════════════════════╝").unwrap();
        writeln!(output).unwrap();
        writeln!(output, "Generated: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S.%3f")).unwrap();
        writeln!(output, "Machine: {}", hostname::get().unwrap_or_default().to_string_lossy()).unwrap();
        writeln!(output, "User: {}", std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "unknown".to_string())).unwrap();
        writeln!(output, "Working Directory: {}", std::env::current_dir().unwrap_or_default().display()).unwrap();
        writeln!(output).unwrap();

        let debug_info = self.error_manager.get_debug_info();
        writeln!(output, "ErrorManager State:").unwrap();
        writeln!(output, "  Version: {}", debug_info.get("version").unwrap_or(&"unknown".to_string())).unwrap();
        writeln!(output, "  Error Handling Strategy: {}", debug_info.get("error_handling_strategy").unwrap_or(&"unknown".to_string())).unwrap();
        writeln!(output, "  Debug Mode: {}", debug_info.get("debug_mode").unwrap_or(&"unknown".to_string())).unwrap();
        writeln!(output, "  Has Errors: {}", debug_info.get("has_errors").unwrap_or(&"false".to_string())).unwrap();
        writeln!(output, "  Total Errors: {}", debug_info.get("total_errors").unwrap_or(&"0".to_string())).unwrap();
        writeln!(output).unwrap();
    }

    fn write_footer(&self, output: &mut String) {
        use std::fmt::Write;

        writeln!(output).unwrap();
        writeln!(output, "╔════════════════════════════════════════════════════════════════════════════╗").unwrap();
        writeln!(output, "║                        END OF DIAGNOSTIC DUMP                              ║").unwrap();
        writeln!(output, "╚════════════════════════════════════════════════════════════════════════════╝").unwrap();
    }

    fn write_separator(&self, output: &mut String) {
        use std::fmt::Write;

        writeln!(output).unwrap();
        writeln!(output, "================================================================================").unwrap();
        writeln!(output).unwrap();
    }

    // ==================== LOG CONTENTS ====================

    fn write_log_contents(&self, output: &mut String) {
        use std::fmt::Write;

        writeln!(output, "█████ COMPLETE LOG CONTENTS █████").unwrap();
        writeln!(output).unwrap();

        let log_contents = self.error_manager.get_log_contents();

        if log_contents.is_empty() {
            writeln!(output, "  [No log entries]").unwrap();
        } else {
            writeln!(output, "{}", log_contents).unwrap();
        }
    }

    // ==================== ERROR SUMMARY ====================

    fn write_error_summary(&self, output: &mut String) {
        use std::fmt::Write;

        writeln!(output, "█████ ERROR SUMMARY █████").unwrap();
        writeln!(output).unwrap();

        let counts = self.error_manager.get_error_counts_by_severity();

        writeln!(output, "By Severity:").unwrap();
        writeln!(output, "  Info:    {}", counts.get(&crate::ErrorManager::ErrorSeverity::Info).unwrap_or(&0)).unwrap();
        writeln!(output, "  Warning: {}", counts.get(&crate::ErrorManager::ErrorSeverity::Warning).unwrap_or(&0)).unwrap();
        writeln!(output, "  Error:   {}", counts.get(&crate::ErrorManager::ErrorSeverity::Error).unwrap_or(&0)).unwrap();
        writeln!(output, "  Fatal:   {}", counts.get(&crate::ErrorManager::ErrorSeverity::Fatal).unwrap_or(&0)).unwrap();
        writeln!(output).unwrap();

        writeln!(output, "By Category:").unwrap();
        writeln!(output, "  Config:               {}", self.error_manager.get_config_errors().len()).unwrap();
        writeln!(output, "  Lexical:              {}", self.error_manager.get_lexical_errors().len()).unwrap();
        writeln!(output, "  Parse:                {}", self.error_manager.get_parse_errors().len()).unwrap();
        writeln!(output, "  Registry:             {}", self.error_manager.get_registry_errors().len()).unwrap();
        writeln!(output, "  ImportsResolution:    {}", self.error_manager.get_imports_resolution_errors().len()).unwrap();
        writeln!(output, "  Semantic:             {}", self.error_manager.get_semantic_errors().len()).unwrap();
        writeln!(output, "  AstEnhancement:       {}", self.error_manager.get_ast_enhancement_errors().len()).unwrap();
        writeln!(output, "  ValueResolution:      {}", self.error_manager.get_value_resolution_errors().len()).unwrap();
        writeln!(output, "  DLM:                  {}", self.error_manager.get_dlm_errors().len()).unwrap();
        writeln!(output, "  BinarySerialization:  {}", self.error_manager.get_binary_serialization_errors().len()).unwrap();
        writeln!(output, "  Runtime:              {}", self.error_manager.get_runtime_errors().len()).unwrap();
        writeln!(output, "  General:              {}", self.error_manager.get_general_errors().len()).unwrap();
    }

    // ==================== ALL ERRORS ====================

    fn write_all_errors(&self, output: &mut String) {
        use std::fmt::Write;

        writeln!(output, "█████ DETAILED ERROR BREAKDOWN █████").unwrap();
        writeln!(output).unwrap();

        self.write_config_errors(output);
        self.write_lexical_errors(output);
        self.write_parse_errors(output);
        self.write_imports_resolution_errors(output);
        self.write_semantic_errors(output);
        self.write_ast_enhancement_errors(output);
        self.write_value_resolution_errors(output);
        self.write_dlm_errors(output);
        self.write_binary_serialization_errors(output);
        self.write_runtime_errors(output);
        self.write_general_errors(output);
    }

    fn write_config_errors(&self, output: &mut String) {
        use std::fmt::Write;

        let errors = self.error_manager.get_config_errors();

        writeln!(output, "─── CONFIG ERRORS ({}) ───", errors.len()).unwrap();
        writeln!(output).unwrap();

        if errors.is_empty() {
            writeln!(output, "  [No config errors]").unwrap();
            writeln!(output).unwrap();
            return;
        }

        for error in errors {
            writeln!(output, "{}", error).unwrap();
        }
    }

    fn write_lexical_errors(&self, output: &mut String) {
        use std::fmt::Write;

        let errors = self.error_manager.get_lexical_errors();

        writeln!(output, "─── LEXICAL ERRORS ({}) ───", errors.len()).unwrap();
        writeln!(output).unwrap();

        if errors.is_empty() {
            writeln!(output, "  [No lexical errors]").unwrap();
            writeln!(output).unwrap();
            return;
        }

        for error in errors {
            writeln!(output, "{}", error).unwrap();
        }
    }

    fn write_parse_errors(&self, output: &mut String) {
        use std::fmt::Write;

        let errors = self.error_manager.get_parse_errors();

        writeln!(output, "─── PARSE ERRORS ({}) ───", errors.len()).unwrap();
        writeln!(output).unwrap();

        if errors.is_empty() {
            writeln!(output, "  [No parse errors]").unwrap();
            writeln!(output).unwrap();
            return;
        }

        for error in errors {
            writeln!(output, "{}", error).unwrap();
        }
    }

    fn write_imports_resolution_errors(&self, output: &mut String) {
        use std::fmt::Write;

        let errors = self.error_manager.get_imports_resolution_errors();

        writeln!(output, "─── IMPORTS RESOLUTION ERRORS ({}) ───", errors.len()).unwrap();
        writeln!(output).unwrap();

        if errors.is_empty() {
            writeln!(output, "  [No imports resolution errors]").unwrap();
            writeln!(output).unwrap();
            return;
        }

        for error in errors {
            writeln!(output, "{}", error).unwrap();
        }
    }

    fn write_semantic_errors(&self, output: &mut String) {
        use std::fmt::Write;

        let errors = self.error_manager.get_semantic_errors();

        writeln!(output, "─── SEMANTIC ERRORS ({}) ───", errors.len()).unwrap();
        writeln!(output).unwrap();

        if errors.is_empty() {
            writeln!(output, "  [No semantic errors]").unwrap();
            writeln!(output).unwrap();
            return;
        }

        for error in errors {
            writeln!(output, "{}", error).unwrap();
        }
    }

    fn write_ast_enhancement_errors(&self, output: &mut String) {
        use std::fmt::Write;

        let errors = self.error_manager.get_ast_enhancement_errors();

        writeln!(output, "─── AST ENHANCEMENT ERRORS ({}) ───", errors.len()).unwrap();
        writeln!(output).unwrap();

        if errors.is_empty() {
            writeln!(output, "  [No AST enhancement errors]").unwrap();
            writeln!(output).unwrap();
            return;
        }

        for error in errors {
            writeln!(output, "{}", error).unwrap();
        }
    }

    fn write_value_resolution_errors(&self, output: &mut String) {
        use std::fmt::Write;

        let errors = self.error_manager.get_value_resolution_errors();

        writeln!(output, "─── VALUE RESOLUTION ERRORS ({}) ───", errors.len()).unwrap();
        writeln!(output).unwrap();

        if errors.is_empty() {
            writeln!(output, "  [No value resolution errors]").unwrap();
            writeln!(output).unwrap();
            return;
        }

        for error in errors {
            writeln!(output, "{}", error).unwrap();
        }
    }

    fn write_dlm_errors(&self, output: &mut String) {
        use std::fmt::Write;

        let errors = self.error_manager.get_dlm_errors();

        writeln!(output, "─── DLM ERRORS ({}) ───", errors.len()).unwrap();
        writeln!(output).unwrap();

        if errors.is_empty() {
            writeln!(output, "  [No DLM errors]").unwrap();
            writeln!(output).unwrap();
            return;
        }

        for error in errors {
            writeln!(output, "{}", error).unwrap();
        }
    }

    fn write_binary_serialization_errors(&self, output: &mut String) {
        use std::fmt::Write;

        let errors = self.error_manager.get_binary_serialization_errors();

        writeln!(output, "─── BINARY SERIALIZATION ERRORS ({}) ───", errors.len()).unwrap();
        writeln!(output).unwrap();

        if errors.is_empty() {
            writeln!(output, "  [No binary serialization errors]").unwrap();
            writeln!(output).unwrap();
            return;
        }

        for error in errors {
            writeln!(output, "{}", error).unwrap();
        }
    }

    fn write_runtime_errors(&self, output: &mut String) {
        use std::fmt::Write;

        let errors = self.error_manager.get_runtime_errors();

        writeln!(output, "─── RUNTIME ERRORS ({}) ───", errors.len()).unwrap();
        writeln!(output).unwrap();

        if errors.is_empty() {
            writeln!(output, "  [No runtime errors]").unwrap();
            writeln!(output).unwrap();
            return;
        }

        for error in errors {
            writeln!(output, "{}", error).unwrap();
        }
    }

    fn write_general_errors(&self, output: &mut String) {
        use std::fmt::Write;

        let errors = self.error_manager.get_general_errors();

        writeln!(output, "─── GENERAL ERRORS ({}) ───", errors.len()).unwrap();
        writeln!(output).unwrap();

        if errors.is_empty() {
            writeln!(output, "  [No general errors]").unwrap();
            writeln!(output).unwrap();
            return;
        }

        for error in errors {
            writeln!(output, "{}", error).unwrap();
        }
    }
}

impl Default for DiagnosticDumper {
    fn default() -> Self {
        Self::new()
    }
}