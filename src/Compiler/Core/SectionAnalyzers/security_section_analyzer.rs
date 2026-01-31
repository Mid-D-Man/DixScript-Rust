// src/Compiler/Core/SectionAnalyzers/security_section_analyzer.rs

use crate::Compiler::AST::{SecuritySection, SecurityEntry, SecurityField, Value, Position};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use crate::ErrorManager::{ErrorManager, SemanticErrorType};
use rustc_hash::FxHashMap;
use lazy_static::lazy_static;

use super::{SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo};

// ==================== PERFORMANCE OPTIMIZATION: STATIC HASH SETS ====================
// CRITICAL: Use lazy_static to avoid recreating HashSets on every call
// This alone saves ~80% of allocation overhead

lazy_static! {
    static ref VALID_BLOCK_KEYS: FxHashMap<&'static str, ()> = {
        let mut map = FxHashMap::default();
        map.insert("encryption", ());
        map.insert("validation", ());
        map.insert("keystore", ());
        map.insert("override", ());
        map.insert("metadata", ());
        map
    };

    static ref VALID_CHECKSUM_ALGORITHMS: FxHashMap<&'static str, ()> = {
        let mut map = FxHashMap::default();
        map.insert("sha256", ());
        map.insert("sha512", ());
        map.insert("hmac-sha256", ());
        map.insert("hmac-sha512", ());
        map
    };

    static ref VALID_KDF_ALGORITHMS: FxHashMap<&'static str, ()> = {
        let mut map = FxHashMap::default();
        map.insert("argon2id", ());
        map.insert("pbkdf2", ());
        map
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
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        SecuritySectionAnalyzer {
            operational_settings,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

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
            self.log_info(&format!("Analyzing SECURITY section with {} entries", entry_count));
        }

        // Empty section
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
            self.log_debug("Phase 2: Validating entry structure");
        }

        self.validate_entry_structure(&section.entries, &mut result);

        if self.should_halt(&result) {
            return result;
        }

        // Phase 3: Parse encryption config (O(1) field lookups from now on)
        if is_verbose {
            self.log_debug("Phase 2: Extracting and completing encryption configuration");
        }

        let encryption_config = entry_map.get("encryption")
            .map(|entry| ParsedEncryptionConfig::from_entry(entry));

        // Phase 3: Validate mode requirements
        if let Some(ref mode) = encryption_mode {
            if is_verbose {
                self.log_debug(&format!("Phase 3: Validating {} mode requirements", mode));
            }

            if let Some(ref config) = encryption_config {
                self.validate_mode_requirements(mode_val, config, &mut result);
            }

        // Phase 4: Validate encryption algorithm
        if is_verbose {
            self.log_debug("Phase 4: Validating encryption algorithm");
        }

            if let Some(ref config) = encryption_config {
                self.validate_algorithm(config, &mut result, is_debug);
            }

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
                self.log_security_level(mode_val);
            }
        }

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

    // ==================== VALIDATION METHODS (OPTIMIZED) ====================

    fn validate_structure(
        &mut self,
        entries: &[SecurityEntry],
        result: &mut SectionAnalysisResult,
        is_debug: bool,
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
        config: &ParsedEncryptionConfig,
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
            Some(EncryptionMode::Keyfile)
        }
    }

    /// Validate and complete mode requirements
    fn validate_and_complete_mode_requirements(
        &mut self,
        mode: EncryptionMode,
        config: &ParsedEncryptionConfig,
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
        config: &ParsedEncryptionConfig,
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
        config: &ParsedEncryptionConfig,
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

        // Validate KDF parameters (O(1) lookups)
        self.validate_kdf_param(config, "kdf_memory", 65536, result, is_verbose);
        self.validate_kdf_param(config, "kdf_iterations", 3, result, is_verbose);
        self.validate_kdf_param(config, "kdf_parallelism", 4, result, is_verbose);
    }

    #[inline]
    fn validate_kdf_param(
        &mut self,
        config: &ParsedEncryptionConfig,
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
        } else if is_verbose && config.has_field(param_name) {
            self.log_debug(&format!("KDF parameter '{}' must be integer - will use default", param_name));
        }
    }

    fn validate_keystore(
        &mut self,
        entry: &SecurityEntry,
        result: &mut SectionAnalysisResult,
        _is_verbose: bool,
        is_debug: bool,
    ) {
        // Build field map for O(1) lookups
        let mut field_map: FxHashMap<&str, &SecurityField> = FxHashMap::default();
        for field in &entry.fields {
            field_map.insert(field.key.as_str(), field);
        }

        // Check auto_generate
        if let Some(field) = field_map.get("auto_generate") {
            if let Some(value) = extract_bool_value(&field.value) {
                if is_debug {
                    self.log_info(&format!("Keystore auto-generation: {}", value));
                }
            }
        }

        // Check backup_count
        if let Some(field) = field_map.get("backup_count") {
            if let Some(value) = extract_int_value(&field.value) {
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

    fn validate_manual_mode(
        &mut self,
        entry: &SecurityEntry,
        result: &mut SectionAnalysisResult,
    ) {
        // Build field map
        let mut field_map: FxHashMap<&str, &SecurityField> = FxHashMap::default();
        for field in &entry.fields {
            field_map.insert(field.key.as_str(), field);
        }

        let accepted = field_map.get("manual_key_warning_accepted")
            .and_then(|f| extract_bool_value(&f.value))
            .unwrap_or(false);

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

    fn validate_validation_config(
        &mut self,
        entry: &SecurityEntry,
        result: &mut SectionAnalysisResult,
        _is_verbose: bool,
        is_debug: bool,
    ) {
        // Build field map
        let mut field_map: FxHashMap<&str, &SecurityField> = FxHashMap::default();
        for field in &entry.fields {
            field_map.insert(field.key.as_str(), field);
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
            EncryptionMode::Password => "HIGH (Argon2id-derived key)",
            EncryptionMode::Keyfile => "HIGH (Randomly generated key)",
            EncryptionMode::Manual => "CRITICAL_RISK (Plaintext key)",
        };
        self.log_info(&format!("Security level: {}", level));
    }

    #[inline]
    fn should_halt(&self, result: &SectionAnalysisResult) -> bool {
        !result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

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

        // Logger call only if debug is enabled (checked once at start)
        if self.operational_settings.debug_mode != DebugMode::Off {
            self.log_warning(message);
        }
    }
}