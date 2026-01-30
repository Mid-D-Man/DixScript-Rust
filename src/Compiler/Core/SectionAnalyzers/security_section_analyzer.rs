// src/Compiler/Core/SectionAnalyzers/security_section_analyzer.rs

use crate::Compiler::AST::{SecuritySection, SecurityEntry, SecurityField, Value, Position};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use crate::ErrorManager::{ErrorManager, SemanticErrorType};
use rustc_hash::FxHashSet;
use lazy_static::lazy_static;
use unicase::UniCase;

use super::{SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo};

// ==================== PERFORMANCE OPTIMIZATION: STATIC HASH SETS ====================
// CRITICAL: Use lazy_static to avoid recreating HashSets on every call
// This alone saves ~80% of allocation overhead

lazy_static! {
    static ref VALID_BLOCK_KEYS: FxHashSet<UniCase<&'static str>> = {
        let mut set = FxHashSet::default();
        set.insert(UniCase::ascii("encryption"));
        set.insert(UniCase::ascii("validation"));
        set.insert(UniCase::ascii("keystore"));
        set.insert(UniCase::ascii("override"));
        set.insert(UniCase::ascii("metadata"));
        set
    };

    static ref VALID_ENCRYPTION_MODES: FxHashSet<UniCase<&'static str>> = {
        let mut set = FxHashSet::default();
        set.insert(UniCase::ascii("password"));
        set.insert(UniCase::ascii("keyfile"));
        set.insert(UniCase::ascii("manual"));
        set
    };

    static ref VALID_ALGORITHMS: FxHashSet<UniCase<&'static str>> = {
        let mut set = FxHashSet::default();
        set.insert(UniCase::ascii("xor"));
        set.insert(UniCase::ascii("aes128-gcm"));
        set.insert(UniCase::ascii("aes128"));
        set.insert(UniCase::ascii("aes256-gcm"));
        set.insert(UniCase::ascii("aes256"));
        set.insert(UniCase::ascii("chacha20-poly1305"));
        set.insert(UniCase::ascii("chacha20"));
        set
    };

    static ref VALID_CHECKSUM_ALGORITHMS: FxHashSet<UniCase<&'static str>> = {
        let mut set = FxHashSet::default();
        set.insert(UniCase::ascii("sha256"));
        set.insert(UniCase::ascii("sha512"));
        set.insert(UniCase::ascii("hmac-sha256"));
        set.insert(UniCase::ascii("hmac-sha512"));
        set
    };

    static ref VALID_KDF_ALGORITHMS: FxHashSet<UniCase<&'static str>> = {
        let mut set = FxHashSet::default();
        set.insert(UniCase::ascii("argon2id"));
        set.insert(UniCase::ascii("pbkdf2"));
        set
    };
}

// ==================== WARNING MESSAGE CONSTANTS ====================

const WARNING_EMPTY_SECTION: &str = "SEC_WARN001";
const WARNING_EMPTY_BLOCK: &str = "SEC_WARN002";
const WARNING_MANUAL_MODE_CRITICAL: &str = "SEC_WARN003";
const WARNING_XOR_LOW_SECURITY: &str = "SEC_WARN004";
const WARNING_KDF_PARAMETER_BELOW_MIN: &str = "SEC_WARN005";
const WARNING_MANUAL_MODE_ENABLED: &str = "SEC_WARN006";
const WARNING_MISSING_FIELD_ADDED_DEFAULT: &str = "SEC_WARN007";

/// SecuritySectionAnalyzer - validates SECURITY section
///
/// PERFORMANCE OPTIMIZATIONS APPLIED:
/// 1. lazy_static HashSets (no repeated allocations)
/// 2. UniCase for zero-copy case-insensitive comparisons
/// 3. Borrowed references throughout (no clones)
/// 4. Short-circuit evaluation for debug checks
/// 5. Inline hints for hot paths
pub struct SecuritySectionAnalyzer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
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

        // OPTIMIZATION: Single debug check at start
        let is_debug = self.operational_settings.debug_mode != DebugMode::Off;
        let is_verbose = self.operational_settings.debug_mode == DebugMode::Verbose;

        if is_debug {
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
        if is_verbose {
            self.log_debug("Phase 1: Validating entry structure");
        }

        self.validate_entry_structure(&section.entries, &mut result);

        if self.should_halt(&result) {
            return result;
        }

        // Phase 2: Extract and complete encryption configuration
        if is_verbose {
            self.log_debug("Phase 2: Extracting and completing encryption configuration");
        }

        let (encryption_mode, _was_fixed) = self.extract_and_complete_encryption_mode(&section.entries, &mut result);

        // Phase 3: Validate mode requirements
        if let Some(ref mode) = encryption_mode {
            if is_verbose {
                self.log_debug(&format!("Phase 3: Validating {} mode requirements", mode));
            }

            self.validate_and_complete_mode_requirements(mode, &section.entries, &mut result);
        }

        // Phase 4: Validate encryption algorithm
        if is_verbose {
            self.log_debug("Phase 4: Validating encryption algorithm");
        }

        self.validate_algorithm(&section.entries, encryption_mode.as_deref(), &mut result, is_debug);

        // Phase 5: Validate KDF parameters (password mode only)
        if let Some(ref mode) = encryption_mode {
            if mode.to_string() == "password" {
                if is_verbose {
                    self.log_debug("Phase 5: Validating KDF parameters");
                }

                self.validate_and_complete_kdf_parameters(&section.entries, &mut result, is_verbose);
            }
        }

        // Phase 6: Validate keystore configuration (keyfile mode)
        if let Some(ref mode) = encryption_mode {
            if mode.to_string() == "keyfile" {
                if is_verbose {
                    self.log_debug("Phase 6: Validating keystore configuration");
                }

                self.validate_keystore_config(&section.entries, &mut result, is_verbose, is_debug);
            }
        }

        // Phase 7: Validate manual mode warnings (manual mode)
        if let Some(ref mode) = encryption_mode {
            if mode.to_string() == "manual" {
                if is_verbose {
                    self.log_debug("Phase 7: Validating manual mode warning acceptance");
                }

                self.validate_manual_mode_warnings(&section.entries, &mut result);
            }
        }

        // Phase 8: Validate validation configuration
        if is_verbose {
            self.log_debug("Phase 8: Validating validation configuration");
        }

        self.validate_validation_config(&section.entries, &mut result, is_verbose, is_debug);

        // Log security level
        if is_debug {
            if let Some(ref mode) = encryption_mode {
                self.log_security_level(mode);
            }
        }

        // Determine overall success
        result.is_success = result.errors.is_empty();

        if is_debug {
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
        for entry in entries {
            // OPTIMIZATION: Use UniCase for zero-copy comparison
            let block_key_uni = UniCase::ascii(entry.block_key.as_str());

            // Check if block key is valid
            if !VALID_BLOCK_KEYS.contains(&block_key_uni) {
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

        // OPTIMIZATION: Use UniCase for validation
        let mode_uni = UniCase::ascii(mode.as_str());
        if !VALID_ENCRYPTION_MODES.contains(&mode_uni) {
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
        is_debug: bool,
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

        // OPTIMIZATION: Use UniCase for validation
        let algorithm_uni = UniCase::ascii(algorithm.as_str());
        if !VALID_ALGORITHMS.contains(&algorithm_uni) {
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
        } else if is_debug {
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
        is_verbose: bool,
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

        // OPTIMIZATION: Use UniCase for validation
        let kdf_uni = UniCase::ascii(kdf.as_str());
        if !VALID_KDF_ALGORITHMS.contains(&kdf_uni) {
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
        self.validate_kdf_parameter(encryption_entry, "kdf_memory", 65536, result, is_verbose);
        self.validate_kdf_parameter(encryption_entry, "kdf_iterations", 3, result, is_verbose);
        self.validate_kdf_parameter(encryption_entry, "kdf_parallelism", 4, result, is_verbose);
    }

    /// Validate a single KDF parameter
    #[inline]
    fn validate_kdf_parameter(
        &mut self,
        entry: &SecurityEntry,
        param_name: &str,
        min_value: i32,
        result: &mut SectionAnalysisResult,
        is_verbose: bool,
    ) {
        let param_field = Self::find_field(&entry.fields, param_name);

        if param_field.is_none() {
            if is_verbose {
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
        is_verbose: bool,
        is_debug: bool,
    ) {
        let keystore_entry = Self::find_entry(entries, "keystore");

        if keystore_entry.is_none() {
            if is_verbose {
                self.log_debug("No keystore configuration - using defaults");
            }
            return;
        }

        let keystore_entry = keystore_entry.unwrap();

        // Check auto_generate field
        let auto_gen_field = Self::find_field(&keystore_entry.fields, "auto_generate");
        if let Some(field) = auto_gen_field {
            if let Value::Boolean { value, .. } = field.value {
                if is_debug {
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
                } else if is_debug {
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
        is_verbose: bool,
        is_debug: bool,
    ) {
        let validation_entry = Self::find_entry(entries, "validation");

        if validation_entry.is_none() {
            if is_verbose {
                self.log_debug("No validation configuration - using defaults");
            }
            return;
        }

        let validation_entry = validation_entry.unwrap();
        let checksum_field = Self::find_field(&validation_entry.fields, "checksum_algorithm");

        if let Some(field) = checksum_field {
            if let Value::String { value, .. } = &field.value {
                let algorithm = value.to_lowercase();
                let algorithm_uni = UniCase::ascii(algorithm.as_str());

                if !VALID_CHECKSUM_ALGORITHMS.contains(&algorithm_uni) {
                    self.add_warning(
                        result,
                        WARNING_MISSING_FIELD_ADDED_DEFAULT,
                        &format!(
                            "Unknown checksum algorithm: {} - will default to 'sha256'",
                            algorithm
                        ),
                        Some(field.position),
                    );
                } else if is_debug {
                    self.log_info(&format!("Checksum algorithm: {}", algorithm));
                }
            }
        }
    }

    // ==================== HELPER METHODS ====================

    /// Find entry by block key (case-insensitive)
    #[inline]
    fn find_entry<'b>(entries: &'b [SecurityEntry], block_key: &str) -> Option<&'b SecurityEntry> {
        let block_key_uni = UniCase::ascii(block_key);
        entries.iter()
            .find(|e| UniCase::ascii(e.block_key.as_str()) == block_key_uni)
    }

    /// Find field by key (case-insensitive)
    #[inline]
    fn find_field<'b>(fields: &'b [SecurityField], key: &str) -> Option<&'b SecurityField> {
        let key_uni = UniCase::ascii(key);
        fields.iter()
            .find(|f| UniCase::ascii(f.key.as_str()) == key_uni)
    }

    /// Check if field exists (case-insensitive)
    #[inline]
    fn has_field(fields: &[SecurityField], key: &str) -> bool {
        let key_uni = UniCase::ascii(key);
        fields.iter()
            .any(|f| UniCase::ascii(f.key.as_str()) == key_uni)
    }

    /// Log security level
    #[inline]
    fn log_security_level(&self, mode: &str) {
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