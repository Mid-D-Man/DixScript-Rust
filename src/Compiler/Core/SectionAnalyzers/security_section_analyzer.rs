// src/Compiler/Core/SectionAnalyzers/security_section_analyzer.rs

use crate::Compiler::AST::{SecuritySection, SecurityEntry, SecurityField, Value, Position};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use crate::ErrorManager::{ErrorManager, SemanticErrorType};
use rustc_hash::FxHashSet;

use super::{SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo};

/// SecuritySectionAnalyzer - validates SECURITY section
///
/// This analyzer validates security configuration but does NOT fail hard when
/// fields are missing - instead it warns and relies on SecurityUtilities to
/// fill in defaults later during processing.
///
/// Performance optimizations applied:
/// - FxHashSet for O(1) lookups
/// - Case-insensitive comparisons with zero allocation
/// - Conditional logging
/// - Borrowed references throughout
pub struct SecuritySectionAnalyzer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
}

// ==================== WARNING MESSAGE CONSTANTS ====================

const WARNING_EMPTY_SECTION: &str = "SEC_WARN001";
const WARNING_EMPTY_BLOCK: &str = "SEC_WARN002";
const WARNING_MANUAL_MODE_CRITICAL: &str = "SEC_WARN003";
const WARNING_XOR_LOW_SECURITY: &str = "SEC_WARN004";
const WARNING_KDF_PARAMETER_BELOW_MIN: &str = "SEC_WARN005";
const WARNING_MANUAL_MODE_ENABLED: &str = "SEC_WARN006";
const WARNING_MISSING_FIELD_ADDED_DEFAULT: &str = "SEC_WARN007";

// ==================== VALID VALUES (STATIC SETS) ====================

fn get_valid_block_keys() -> FxHashSet<&'static str> {
    let mut set = FxHashSet::default();
    set.insert("encryption");
    set.insert("validation");
    set.insert("keystore");
    set.insert("override");
    set.insert("metadata");
    set
}

fn get_valid_encryption_modes() -> FxHashSet<&'static str> {
    let mut set = FxHashSet::default();
    set.insert("password");
    set.insert("keyfile");
    set.insert("manual");
    set
}

fn get_valid_algorithms() -> FxHashSet<&'static str> {
    let mut set = FxHashSet::default();
    set.insert("xor");
    set.insert("aes128-gcm");
    set.insert("aes128");
    set.insert("aes256-gcm");
    set.insert("aes256");
    set.insert("chacha20-poly1305");
    set.insert("chacha20");
    set
}

fn get_valid_checksum_algorithms() -> FxHashSet<&'static str> {
    let mut set = FxHashSet::default();
    set.insert("sha256");
    set.insert("sha512");
    set.insert("hmac-sha256");
    set.insert("hmac-sha512");
    set
}

fn get_valid_kdf_algorithms() -> FxHashSet<&'static str> {
    let mut set = FxHashSet::default();
    set.insert("argon2id");
    set.insert("pbkdf2");
    set
}

impl<'a> SecuritySectionAnalyzer<'a> {
    /// Create a new SecuritySectionAnalyzer
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        SecuritySectionAnalyzer {
            operational_settings,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Main analysis method - validates SECURITY section
    pub fn analyze(
        &mut self,
        section: &SecuritySection,
        _symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        let mut result = SectionAnalysisResult::new("SECURITY");
        let entry_count = section.entries.len();

        if self.operational_settings.debug_mode != DebugMode::Off {
            self.log_info(&format!(
                "Analyzing SECURITY section with {} entries",
                entry_count
            ));
        }

        // Empty section is valid - defaults will be used
        if entry_count == 0 {
            self.add_warning(
                &mut result,
                WARNING_EMPTY_SECTION,
                "SECURITY section is empty - default settings will be used",
                None,
            );
            result.is_success = true;
            return result;
        }

        // Phase 1: Validate entry structure
        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.log_debug("Phase 1: Validating entry structure");
        }

        self.validate_entry_structure(&section.entries, &mut result);

        if self.should_halt(&result) {
            return result;
        }

        // Phase 2: Extract and complete encryption configuration
        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.log_debug("Phase 2: Extracting and completing encryption configuration");
        }

        let (encryption_mode, _was_fixed) = self.extract_and_complete_encryption_mode(&section.entries, &mut result);

        // Phase 3: Validate mode requirements
        if let Some(ref mode) = encryption_mode {
            if self.operational_settings.debug_mode == DebugMode::Verbose {
                self.log_debug(&format!("Phase 3: Validating {} mode requirements", mode));
            }

            self.validate_and_complete_mode_requirements(mode, &section.entries, &mut result);
        }

        // Phase 4: Validate encryption algorithm
        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.log_debug("Phase 4: Validating encryption algorithm");
        }

        self.validate_algorithm(&section.entries, encryption_mode.as_deref(), &mut result);

        // Phase 5: Validate KDF parameters (password mode only)
        if let Some(ref mode) = encryption_mode {
            if mode == "password" {
                if self.operational_settings.debug_mode == DebugMode::Verbose {
                    self.log_debug("Phase 5: Validating KDF parameters");
                }

                self.validate_and_complete_kdf_parameters(&section.entries, &mut result);
            }
        }

        // Phase 6: Validate keystore configuration (keyfile mode)
        if let Some(ref mode) = encryption_mode {
            if mode == "keyfile" {
                if self.operational_settings.debug_mode == DebugMode::Verbose {
                    self.log_debug("Phase 6: Validating keystore configuration");
                }

                self.validate_keystore_config(&section.entries, &mut result);
            }
        }

        // Phase 7: Validate manual mode warnings (manual mode)
        if let Some(ref mode) = encryption_mode {
            if mode == "manual" {
                if self.operational_settings.debug_mode == DebugMode::Verbose {
                    self.log_debug("Phase 7: Validating manual mode warning acceptance");
                }

                self.validate_manual_mode_warnings(&section.entries, &mut result);
            }
        }

        // Phase 8: Validate validation configuration
        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.log_debug("Phase 8: Validating validation configuration");
        }

        self.validate_validation_config(&section.entries, &mut result);

        // Log security level
        if let Some(ref mode) = encryption_mode {
            self.log_security_level(mode);
        }

        // Determine overall success
        result.is_success = result.errors.is_empty();

        if self.operational_settings.debug_mode != DebugMode::Off {
            let status = if result.is_success { "SUCCESS" } else { "FAILURE" };
            self.log_info(&format!("SECURITY analysis complete: {}", status));
            self.log_info(&format!(
                "  Encryption mode: {}",
                encryption_mode.as_deref().unwrap_or("UNKNOWN")
            ));
            self.log_info(&format!(
                "  Errors: {}, Warnings: {}",
                result.errors.len(),
                result.warnings.len()
            ));
        }

        result
    }

    // ==================== VALIDATION METHODS ====================

    /// Validate entry structure (block keys and empty blocks)
    fn validate_entry_structure(
        &mut self,
        entries: &[SecurityEntry],
        result: &mut SectionAnalysisResult,
    ) {
        let valid_block_keys = get_valid_block_keys();

        for entry in entries {
            // Check if block key is valid
            if !Self::contains_case_insensitive(&valid_block_keys, &entry.block_key) {
                self.add_warning(
                    result,
                    WARNING_EMPTY_BLOCK,
                    &format!("Unknown security block key: {} (will be ignored)", entry.block_key),
                    Some(entry.position),
                );
            }

            // Check if block has fields
            if entry.fields.is_empty() {
                self.add_warning(
                    result,
                    WARNING_EMPTY_BLOCK,
                    &format!("Security block '{}' is empty", entry.block_key),
                    Some(entry.position),
                );
            }
        }
    }

    /// Extract and complete encryption mode (with defaults)
    fn extract_and_complete_encryption_mode(
        &mut self,
        entries: &[SecurityEntry],
        result: &mut SectionAnalysisResult,
    ) -> (Option<String>, bool) {
        let encryption_entry = Self::find_entry(entries, "encryption");

        if encryption_entry.is_none() {
            self.add_warning(
                result,
                WARNING_MISSING_FIELD_ADDED_DEFAULT,
                "No encryption configuration found - defaults will be applied during processing",
                None,
            );
            return (Some("keyfile".to_string()), true);
        }

        let encryption_entry = encryption_entry.unwrap();
        let mode_field = Self::find_field(&encryption_entry.fields, "mode");

        if mode_field.is_none() {
            self.add_warning(
                result,
                WARNING_MISSING_FIELD_ADDED_DEFAULT,
                "Encryption mode not specified - defaulting to 'keyfile'",
                Some(encryption_entry.position),
            );
            return (Some("keyfile".to_string()), true);
        }

        let mode_field = mode_field.unwrap();

        // Extract string value
        let mode = match &mode_field.value {
            Value::String { value, .. } => value.to_lowercase(),
            _ => {
                self.add_warning(
                    result,
                    WARNING_MISSING_FIELD_ADDED_DEFAULT,
                    "Encryption mode must be a string - defaulting to 'keyfile'",
                    Some(mode_field.position),
                );
                return (Some("keyfile".to_string()), true);
            }
        };

        // Validate mode
        let valid_modes = get_valid_encryption_modes();
        if !valid_modes.contains(mode.as_str()) {
            self.add_warning(
                result,
                WARNING_MISSING_FIELD_ADDED_DEFAULT,
                &format!("Invalid encryption mode: {} - defaulting to 'keyfile'", mode),
                Some(mode_field.position),
            );
            return (Some("keyfile".to_string()), true);
        }

        if self.operational_settings.debug_mode != DebugMode::Off {
            self.log_info(&format!("Encryption mode: {}", mode));
        }

        (Some(mode), false)
    }

    /// Validate and complete mode requirements
    fn validate_and_complete_mode_requirements(
        &mut self,
        mode: &str,
        entries: &[SecurityEntry],
        result: &mut SectionAnalysisResult,
    ) {
        let required_fields: &[&str] = match mode {
            "password" => &["mode", "algorithm", "kdf"],
            "keyfile" => &["mode", "algorithm"],
            "manual" => &["mode", "key", "iv"],
            _ => &[],
        };

        let encryption_entry = Self::find_entry(entries, "encryption");
        if encryption_entry.is_none() {
            return;
        }

        let encryption_entry = encryption_entry.unwrap();

        // Check each required field
        for &field_name in required_fields {
            if !Self::has_field(&encryption_entry.fields, field_name) {
                self.add_warning(
                    result,
                    WARNING_MISSING_FIELD_ADDED_DEFAULT,
                    &format!(
                        "Required field '{}' missing for mode '{}' - will use default",
                        field_name, mode
                    ),
                    Some(encryption_entry.position),
                );
            }
        }

        // Special warning for manual mode
        if mode == "manual" {
            self.add_warning(
                result,
                WARNING_MANUAL_MODE_CRITICAL,
                "CRITICAL: Manual mode stores encryption key in PLAINTEXT in source file",
                Some(encryption_entry.position),
            );
        }
    }

    /// Validate algorithm
    fn validate_algorithm(
        &mut self,
        entries: &[SecurityEntry],
        mode: Option<&str>,
        result: &mut SectionAnalysisResult,
    ) {
        let encryption_entry = Self::find_entry(entries, "encryption");
        if encryption_entry.is_none() {
            return;
        }

        let encryption_entry = encryption_entry.unwrap();
        let algorithm_field = Self::find_field(&encryption_entry.fields, "algorithm");

        if algorithm_field.is_none() {
            self.add_warning(
                result,
                WARNING_MISSING_FIELD_ADDED_DEFAULT,
                "Encryption algorithm not specified - will default to 'aes256-gcm'",
                Some(encryption_entry.position),
            );
            return;
        }

        let algorithm_field = algorithm_field.unwrap();

        // Extract string value
        let algorithm = match &algorithm_field.value {
            Value::String { value, .. } => value.to_lowercase(),
            _ => {
                self.add_warning(
                    result,
                    WARNING_MISSING_FIELD_ADDED_DEFAULT,
                    "Algorithm must be a string - will default to 'aes256-gcm'",
                    Some(algorithm_field.position),
                );
                return;
            }
        };

        // Validate algorithm
        let valid_algorithms = get_valid_algorithms();
        if !valid_algorithms.contains(algorithm.as_str()) {
            self.add_warning(
                result,
                WARNING_MISSING_FIELD_ADDED_DEFAULT,
                &format!("Unknown algorithm: {} - will default to 'aes256-gcm'", algorithm),
                Some(algorithm_field.position),
            );
            return;
        }

        // Warn about XOR
        if algorithm == "xor" {
            self.add_warning(
                result,
                WARNING_XOR_LOW_SECURITY,
                "Algorithm 'xor' provides LOW security - obfuscation only",
                Some(algorithm_field.position),
            );
        } else if self.operational_settings.debug_mode != DebugMode::Off {
            let security = if algorithm.starts_with("aes256") || algorithm.starts_with("chacha20") {
                "HIGH"
            } else {
                "MEDIUM"
            };
            self.log_info(&format!("Encryption algorithm: {} ({} security)", algorithm, security));
        }
    }

    /// Validate and complete KDF parameters
    fn validate_and_complete_kdf_parameters(
        &mut self,
        entries: &[SecurityEntry],
        result: &mut SectionAnalysisResult,
    ) {
        let encryption_entry = Self::find_entry(entries, "encryption");
        if encryption_entry.is_none() {
            return;
        }

        let encryption_entry = encryption_entry.unwrap();
        let kdf_field = Self::find_field(&encryption_entry.fields, "kdf");

        if kdf_field.is_none() {
            self.add_warning(
                result,
                WARNING_MISSING_FIELD_ADDED_DEFAULT,
                "Key derivation function not specified - will default to 'argon2id'",
                Some(encryption_entry.position),
            );
            return;
        }

        let kdf_field = kdf_field.unwrap();

        // Extract string value
        let kdf = match &kdf_field.value {
            Value::String { value, .. } => value.to_lowercase(),
            _ => {
                self.add_warning(
                    result,
                    WARNING_MISSING_FIELD_ADDED_DEFAULT,
                    "KDF must be a string - will default to 'argon2id'",
                    Some(kdf_field.position),
                );
                return;
            }
        };

        // Validate KDF algorithm
        let valid_kdfs = get_valid_kdf_algorithms();
        if !valid_kdfs.contains(kdf.as_str()) {
            self.add_warning(
                result,
                WARNING_MISSING_FIELD_ADDED_DEFAULT,
                &format!("Unknown KDF algorithm: {} - will default to 'argon2id'", kdf),
                Some(kdf_field.position),
            );
        } else if self.operational_settings.debug_mode != DebugMode::Off {
            self.log_info(&format!("Key derivation function: {}", kdf));
        }

        // Validate KDF parameters
        self.validate_kdf_parameter(encryption_entry, "kdf_memory", 65536, result);
        self.validate_kdf_parameter(encryption_entry, "kdf_iterations", 3, result);
        self.validate_kdf_parameter(encryption_entry, "kdf_parallelism", 4, result);
    }

    /// Validate a single KDF parameter
    #[inline]
    fn validate_kdf_parameter(
        &mut self,
        entry: &SecurityEntry,
        param_name: &str,
        min_value: i32,
        result: &mut SectionAnalysisResult,
    ) {
        let param_field = Self::find_field(&entry.fields, param_name);

        if param_field.is_none() {
            if self.operational_settings.debug_mode == DebugMode::Verbose {
                self.log_debug(&format!(
                    "KDF parameter '{}' not specified - will use default",
                    param_name
                ));
            }
            return;
        }

        let param_field = param_field.unwrap();

        // Extract integer value
        match &param_field.value {
            Value::Integer { value, .. } => {
                if *value < min_value {
                    self.add_warning(
                        result,
                        WARNING_KDF_PARAMETER_BELOW_MIN,
                        &format!(
                            "KDF parameter '{}' value {} below recommended minimum {}",
                            param_name, value, min_value
                        ),
                        Some(param_field.position),
                    );
                }
            }
            _ => {
                self.add_warning(
                    result,
                    WARNING_MISSING_FIELD_ADDED_DEFAULT,
                    &format!("KDF parameter '{}' must be integer - will use default", param_name),
                    Some(param_field.position),
                );
            }
        }
    }

    /// Validate keystore configuration
    fn validate_keystore_config(
        &mut self,
        entries: &[SecurityEntry],
        result: &mut SectionAnalysisResult,
    ) {
        let keystore_entry = Self::find_entry(entries, "keystore");

        if keystore_entry.is_none() {
            if self.operational_settings.debug_mode == DebugMode::Verbose {
                self.log_debug("No keystore configuration - using defaults");
            }
            return;
        }

        let keystore_entry = keystore_entry.unwrap();

        // Check auto_generate field
        let auto_gen_field = Self::find_field(&keystore_entry.fields, "auto_generate");
        if let Some(field) = auto_gen_field {
            if let Value::Boolean { value, .. } = field.value {
                if self.operational_settings.debug_mode != DebugMode::Off {
                    self.log_info(&format!("Keystore auto-generation: {}", value));
                }
            }
        }

        // Check backup_count field
        let backup_count_field = Self::find_field(&keystore_entry.fields, "backup_count");
        if let Some(field) = backup_count_field {
            if let Value::Integer { value, .. } = field.value {
                if value < 0 || value > 10 {
                    self.add_warning(
                        result,
                        "SEC_WARN008",
                        &format!(
                            "Backup count {} out of range (0-10) - will use default 3",
                            value
                        ),
                        Some(field.position),
                    );
                } else if self.operational_settings.debug_mode != DebugMode::Off {
                    self.log_info(&format!("Keystore backup count: {}", value));
                }
            }
        }
    }

    /// Validate manual mode warnings
    fn validate_manual_mode_warnings(
        &mut self,
        entries: &[SecurityEntry],
        result: &mut SectionAnalysisResult,
    ) {
        let override_entry = Self::find_entry(entries, "override");

        if override_entry.is_none() {
            self.add_warning(
                result,
                WARNING_MISSING_FIELD_ADDED_DEFAULT,
                "Manual mode requires explicit warning acceptance - will add default (rejected)",
                None,
            );
            return;
        }

        let override_entry = override_entry.unwrap();
        let warning_field = Self::find_field(&override_entry.fields, "manual_key_warning_accepted");

        let accepted = if let Some(field) = warning_field {
            matches!(field.value, Value::Boolean { value: true, .. })
        } else {
            false
        };

        if !accepted {
            self.add_warning(
                result,
                WARNING_MANUAL_MODE_ENABLED,
                "Manual mode key warning not explicitly accepted - encryption may fail",
                Some(override_entry.position),
            );
        } else {
            self.add_warning(
                result,
                WARNING_MANUAL_MODE_ENABLED,
                "Manual mode enabled - encryption key stored in PLAINTEXT",
                Some(override_entry.position),
            );
        }
    }

    /// Validate validation configuration
    fn validate_validation_config(
        &mut self,
        entries: &[SecurityEntry],
        result: &mut SectionAnalysisResult,
    ) {
        let validation_entry = Self::find_entry(entries, "validation");

        if validation_entry.is_none() {
            if self.operational_settings.debug_mode == DebugMode::Verbose {
                self.log_debug("No validation configuration - using defaults");
            }
            return;
        }

        let validation_entry = validation_entry.unwrap();
        let checksum_field = Self::find_field(&validation_entry.fields, "checksum_algorithm");

        if let Some(field) = checksum_field {
            if let Value::String { value, .. } = &field.value {
                let algorithm = value.to_lowercase();
                let valid_checksums = get_valid_checksum_algorithms();

                if !valid_checksums.contains(algorithm.as_str()) {
                    self.add_warning(
                        result,
                        WARNING_MISSING_FIELD_ADDED_DEFAULT,
                        &format!(
                            "Unknown checksum algorithm: {} - will default to 'sha256'",
                            algorithm
                        ),
                        Some(field.position),
                    );
                } else if self.operational_settings.debug_mode != DebugMode::Off {
                    self.log_info(&format!("Checksum algorithm: {}", algorithm));
                }
            }
        }
    }

    // ==================== HELPER METHODS ====================

    /// Find entry by block key (case-insensitive)
    #[inline]
    fn find_entry(entries: &[SecurityEntry], block_key: &str) -> Option<&'a SecurityEntry> {
        entries.iter()
            .find(|e| e.block_key.eq_ignore_ascii_case(block_key))
    }

    /// Find field by key (case-insensitive)
    #[inline]
    fn find_field(fields: &[SecurityField], key: &str) -> Option<&'a SecurityField> {
        fields.iter()
            .find(|f| f.key.eq_ignore_ascii_case(key))
    }

    /// Check if field exists (case-insensitive)
    #[inline]
    fn has_field(fields: &[SecurityField], key: &str) -> bool {
        fields.iter()
            .any(|f| f.key.eq_ignore_ascii_case(key))
    }

    /// Case-insensitive contains check (zero-allocation)
    #[inline]
    fn contains_case_insensitive(set: &FxHashSet<&str>, value: &str) -> bool {
        set.iter().any(|item| item.eq_ignore_ascii_case(value))
    }

    /// Log security level
    #[inline]
    fn log_security_level(&self, mode: &str) {
        if self.operational_settings.debug_mode == DebugMode::Off {
            return;
        }

        let level = match mode {
            "password" => "HIGH (Argon2id-derived key)",
            "keyfile" => "HIGH (Randomly generated key)",
            "manual" => "CRITICAL_RISK (Plaintext key)",
            _ => "UNKNOWN",
        };

        self.log_info(&format!("Security level: {}", level));
    }

    /// Check if analysis should halt due to errors
    #[inline]
    fn should_halt(&self, result: &SectionAnalysisResult) -> bool {
        !result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

    // ==================== LOGGING HELPERS ====================

    #[inline]
    fn log_debug(&self, message: &str) {
        self.error_manager.log_debug(message);
    }

    #[inline]
    fn log_info(&self, message: &str) {
        self.error_manager.log_info(message);
    }

    #[inline]
    fn log_warning(&self, message: &str) {
        self.error_manager.log_Warning(message);
    }

    // ==================== ERROR/WARNING HELPERS ====================

    fn add_warning(
        &mut self,
        result: &mut SectionAnalysisResult,
        warning_id: &str,
        message: &str,
        position: Option<Position>,
    ) {
        let warning = SemanticWarningInfo {
            warning_id: warning_id.to_string(),
            message: message.to_string(),
            section_name: "SECURITY".to_string(),
            position,
        };

        result.warnings.push(warning);

        if self.operational_settings.debug_mode != DebugMode::Off {
            self.log_warning(message);
        }
    }
}
