// src/Compiler/Core/SectionAnalyzers/security_section_analyzer.rs

use crate::Compiler::AST::{SecuritySection, SecurityEntry, SecurityField, Value, Position};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use crate::ErrorManager::{ErrorManager, SemanticErrorType};
use rustc_hash::FxHashMap;
use lazy_static::lazy_static;

use super::{SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo};

// ==================== PERFORMANCE: USE ENUMS INSTEAD OF STRINGS ====================

/// Encryption mode (Copy + stack-based for zero-cost comparisons)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum EncryptionMode {
    Password,
    Keyfile,
    Manual,
}

impl EncryptionMode {
    #[inline]
    fn from_str(s: &str) -> Option<Self> {
        // Direct match on &str - no allocation
        match s {
            "password" => Some(Self::Password),
            "keyfile" => Some(Self::Keyfile),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }

    #[inline]
    fn as_str(&self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Keyfile => "keyfile",
            Self::Manual => "manual",
        }
    }

    #[inline]
    fn required_fields(&self) -> &'static [&'static str] {
        match self {
            Self::Password => &["mode", "algorithm", "kdf"],
            Self::Keyfile => &["mode", "algorithm"],
            Self::Manual => &["mode", "key", "iv"],
        }
    }
}

/// Encryption algorithm (Copy + stack-based)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum Algorithm {
    Xor,
    Aes128Gcm,
    Aes128,
    Aes256Gcm,
    Aes256,
    Chacha20Poly1305,
    Chacha20,
}

impl Algorithm {
    #[inline]
    fn from_str(s: &str) -> Option<Self> {
        // Direct match on &str - no allocation
        match s {
            "xor" => Some(Self::Xor),
            "aes128-gcm" => Some(Self::Aes128Gcm),
            "aes128" => Some(Self::Aes128),
            "aes256-gcm" => Some(Self::Aes256Gcm),
            "aes256" => Some(Self::Aes256),
            "chacha20-poly1305" => Some(Self::Chacha20Poly1305),
            "chacha20" => Some(Self::Chacha20),
            _ => None,
        }
    }

    #[inline]
    fn is_high_security(&self) -> bool {
        matches!(self, Self::Aes256Gcm | Self::Aes256 | Self::Chacha20Poly1305 | Self::Chacha20)
    }

    #[inline]
    fn is_low_security(&self) -> bool {
        matches!(self, Self::Xor)
    }
}

// ==================== STATIC VALIDATION SETS ====================

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

// ==================== PARSED ENCRYPTION CONFIG (CACHED) ====================

/// Pre-parsed encryption configuration for O(1) field access
struct ParsedEncryptionConfig<'a> {
    mode: Option<EncryptionMode>,
    algorithm: Option<Algorithm>,
    kdf: Option<&'a str>,
    field_map: FxHashMap<&'a str, &'a SecurityField>,
}

impl<'a> ParsedEncryptionConfig<'a> {
    /// Build from security entry - O(n) once, then O(1) lookups
    #[inline]
    fn from_entry(entry: &'a SecurityEntry) -> Self {
        // Build field map for O(1) lookups (replaces linear search)
        let mut field_map: FxHashMap<&str, &SecurityField> = FxHashMap::default();
        for field in &entry.fields {
            field_map.insert(field.key.as_str(), field);
        }

        // Extract mode - NO STRING ALLOCATION
        let mode = field_map.get("mode")
            .and_then(|f| extract_string_value(&f.value))
            .and_then(EncryptionMode::from_str);

        // Extract algorithm - NO STRING ALLOCATION
        let algorithm = field_map.get("algorithm")
            .and_then(|f| extract_string_value(&f.value))
            .and_then(Algorithm::from_str);

        // Extract KDF (just store reference, don't validate yet)
        let kdf = field_map.get("kdf")
            .and_then(|f| extract_string_value(&f.value));

        ParsedEncryptionConfig {
            mode,
            algorithm,
            kdf,
            field_map,
        }
    }

    /// Check if field exists - O(1)
    #[inline]
    fn has_field(&self, key: &str) -> bool {
        self.field_map.contains_key(key)
    }

    /// Get field value - O(1)
    #[inline]
    fn get_field(&self, key: &str) -> Option<&'a SecurityField> {
        self.field_map.get(key).copied()
    }

    /// Get integer field - O(1)
    #[inline]
    fn get_int_field(&self, key: &str) -> Option<i32> {
        self.get_field(key)
            .and_then(|f| match &f.value {
                Value::Integer { value, .. } => Some(*value),
                _ => None,
            })
    }

    /// Get boolean field - O(1)
    #[inline]
    fn get_bool_field(&self, key: &str) -> Option<bool> {
        self.get_field(key)
            .and_then(|f| match &f.value {
                Value::Boolean { value, .. } => Some(*value),
                _ => None,
            })
    }
}

// ==================== HELPER FUNCTIONS ====================

/// Extract string value from Value enum (zero-copy)
#[inline]
fn extract_string_value(value: &Value) -> Option<&str> {
    match value {
        Value::String { value, .. } => Some(value.as_str()),
        _ => None,
    }
}

/// Extract integer value from Value enum
#[inline]
fn extract_int_value(value: &Value) -> Option<i32> {
    match value {
        Value::Integer { value, .. } => Some(*value),
        _ => None,
    }
}

/// Extract boolean value from Value enum
#[inline]
fn extract_bool_value(value: &Value) -> Option<bool> {
    match value {
        Value::Boolean { value, .. } => Some(*value),
        _ => None,
    }
}

/// Case-insensitive key check in static hashmap - NO ALLOCATION
#[inline]
fn is_valid_key_ci(key: &str, valid_keys: &FxHashMap<&'static str, ()>) -> bool {
    // Check lowercase version directly in hashmap (keys are already lowercase)
    valid_keys.contains_key(key) || {
        // Fallback: case-insensitive check
        valid_keys.keys().any(|k| k.eq_ignore_ascii_case(key))
    }
}

// ==================== MAIN ANALYZER ====================

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

        // OPTIMIZATION: Hoist debug checks ONCE (not in every validation call)
        let is_debug = self.operational_settings.debug_mode != DebugMode::Off;
        let is_verbose = self.operational_settings.debug_mode == DebugMode::Verbose;

        if is_debug {
            self.log_info(&format!("Analyzing SECURITY section with {} entries", entry_count));
        }

        // Empty section
        if entry_count == 0 {
            self.add_warning(&mut result, WARNING_EMPTY_SECTION, 
                "SECURITY section is empty - default settings will be used", None);
            result.is_success = true;
            return result;
        }

        // Phase 1: Build entry lookup map (O(n) once)
        if is_verbose {
            self.log_debug("Phase 1: Building entry lookup map");
        }
        let entry_map = Self::build_entry_map(&section.entries);

        // Phase 2: Validate structure
        if is_verbose {
            self.log_debug("Phase 2: Validating entry structure");
        }
        self.validate_structure(&section.entries, &entry_map, &mut result, is_debug);

        if self.should_halt(&result) {
            return result;
        }

        // Phase 3: Parse encryption config (O(1) field lookups from now on)
        if is_verbose {
            self.log_debug("Phase 3: Parsing encryption configuration");
        }

        let encryption_config = entry_map.get("encryption")
            .map(|entry| ParsedEncryptionConfig::from_entry(entry));

        let mode = if let Some(ref config) = encryption_config {
            self.validate_encryption_mode(config, &mut result, is_debug)
        } else {
            self.add_warning(&mut result, WARNING_MISSING_FIELD_ADDED_DEFAULT,
                "No encryption configuration found - defaults will be applied", None);
            Some(EncryptionMode::Keyfile)
        };

        // Phase 4: Validate mode-specific requirements
        if let Some(mode_val) = mode {
            if is_verbose {
                self.log_debug(&format!("Phase 4: Validating {} mode requirements", mode_val.as_str()));
            }

            if let Some(ref config) = encryption_config {
                self.validate_mode_requirements(mode_val, config, &mut result);
            }

            // Phase 5: Validate algorithm
            if is_verbose {
                self.log_debug("Phase 5: Validating encryption algorithm");
            }

            if let Some(ref config) = encryption_config {
                self.validate_algorithm(config, &mut result, is_debug);
            }

            // Phase 6: Mode-specific validations (ENUM MATCHING - NO STRING ALLOCATION)
            match mode_val {
                EncryptionMode::Password => {
                    if is_verbose {
                        self.log_debug("Phase 6: Validating KDF parameters");
                    }
                    if let Some(ref config) = encryption_config {
                        self.validate_kdf_parameters(config, &mut result, is_verbose);
                    }
                }
                EncryptionMode::Keyfile => {
                    if is_verbose {
                        self.log_debug("Phase 7: Validating keystore configuration");
                    }
                    if let Some(entry) = entry_map.get("keystore") {
                        self.validate_keystore(entry, &mut result, is_verbose, is_debug);
                    }
                }
                EncryptionMode::Manual => {
                    if is_verbose {
                        self.log_debug("Phase 8: Validating manual mode warnings");
                    }
                    if let Some(entry) = entry_map.get("override") {
                        self.validate_manual_mode(entry, &mut result);
                    }
                }
            }

            // Phase 7: Validate validation config
            if is_verbose {
                self.log_debug("Phase 9: Validating validation configuration");
            }
            if let Some(entry) = entry_map.get("validation") {
                self.validate_validation_config(entry, &mut result, is_verbose, is_debug);
            }

            // Log security level
            if is_debug {
                self.log_security_level(mode_val);
            }
        }

        result.is_success = result.errors.is_empty();

        if is_debug {
            let status = if result.is_success { "SUCCESS" } else { "FAILURE" };
            self.log_info(&format!("SECURITY analysis complete: {}", status));
            if let Some(m) = mode {
                self.log_info(&format!("  Encryption mode: {}", m.as_str()));
            }
            self.log_info(&format!("  Errors: {}, Warnings: {}", 
                result.errors.len(), result.warnings.len()));
        }

        result
    }

    // ==================== OPTIMIZATION: BUILD ENTRY MAP ONCE ====================

    /// Build entry lookup map - O(n) once instead of O(n) per lookup
    /// NO STRING ALLOCATION - uses &str directly
    #[inline]
    fn build_entry_map(entries: &[SecurityEntry]) -> FxHashMap<&str, &SecurityEntry> {
        let mut map = FxHashMap::default();
        for entry in entries {
            let key = entry.block_key.as_str();
            // Case-insensitive by checking directly (no .to_lowercase())
            if key.eq_ignore_ascii_case("encryption") {
                map.insert("encryption", entry);
            } else if key.eq_ignore_ascii_case("validation") {
                map.insert("validation", entry);
            } else if key.eq_ignore_ascii_case("keystore") {
                map.insert("keystore", entry);
            } else if key.eq_ignore_ascii_case("override") {
                map.insert("override", entry);
            } else if key.eq_ignore_ascii_case("metadata") {
                map.insert("metadata", entry);
            }
        }
        map
    }

    // ==================== VALIDATION METHODS (OPTIMIZED) ====================

    fn validate_structure(
        &mut self,
        entries: &[SecurityEntry],
        _entry_map: &FxHashMap<&str, &SecurityEntry>,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) {
        for entry in entries {
            // NO STRING ALLOCATION - use &str directly
            if !is_valid_key_ci(entry.block_key.as_str(), &VALID_BLOCK_KEYS) {
                if is_debug {
                    self.log_warning(&format!("Unknown security block key: {} (will be ignored)", entry.block_key));
                }
                self.add_warning(result, WARNING_EMPTY_BLOCK,
                    &format!("Unknown security block key: {} (will be ignored)", entry.block_key),
                    Some(entry.position));
            }

            if entry.fields.is_empty() {
                self.add_warning(result, WARNING_EMPTY_BLOCK,
                    &format!("Security block '{}' is empty", entry.block_key),
                    Some(entry.position));
            }
        }
    }

    /// Validate encryption mode (ENUM MATCHING - NO STRING ALLOCATION)
    fn validate_encryption_mode(
        &mut self,
        config: &ParsedEncryptionConfig,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) -> Option<EncryptionMode> {
        if let Some(mode) = config.mode {
            if is_debug {
                self.log_info(&format!("Encryption mode: {}", mode.as_str()));
            }
            Some(mode)
        } else {
            // Check if mode field exists but has wrong type
            if config.has_field("mode") {
                self.add_warning(result, WARNING_MISSING_FIELD_ADDED_DEFAULT,
                    "Encryption mode must be a string - defaulting to 'keyfile'",
                    config.get_field("mode").map(|f| f.position));
            } else {
                self.add_warning(result, WARNING_MISSING_FIELD_ADDED_DEFAULT,
                    "Encryption mode not specified - defaulting to 'keyfile'", None);
            }
            Some(EncryptionMode::Keyfile)
        }
    }

    /// Validate mode requirements (ENUM MATCHING - NO STRING ALLOCATION)
    fn validate_mode_requirements(
        &mut self,
        mode: EncryptionMode,
        config: &ParsedEncryptionConfig,
        result: &mut SectionAnalysisResult,
    ) {
        // Get required fields for this mode (zero-cost enum match)
        for &field_name in mode.required_fields() {
            if !config.has_field(field_name) {
                self.add_warning(result, WARNING_MISSING_FIELD_ADDED_DEFAULT,
                    &format!("Required field '{}' missing for mode '{}' - will use default",
                        field_name, mode.as_str()),
                    None);
            }
        }

        // Special warning for manual mode (ENUM MATCH instead of string comparison)
        if mode == EncryptionMode::Manual {
            self.add_warning(result, WARNING_MANUAL_MODE_CRITICAL,
                "CRITICAL: Manual mode stores encryption key in PLAINTEXT in source file", None);
        }
    }

    /// Validate algorithm (ENUM MATCHING - NO STRING ALLOCATION)
    fn validate_algorithm(
        &mut self,
        config: &ParsedEncryptionConfig,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) {
        if let Some(algorithm) = config.algorithm {
            // ENUM-BASED CHECKS (zero-cost)
            if algorithm.is_low_security() {
                self.add_warning(result, WARNING_XOR_LOW_SECURITY,
                    "Algorithm 'xor' provides LOW security - obfuscation only", None);
            } else if is_debug {
                let security = if algorithm.is_high_security() { "HIGH" } else { "MEDIUM" };
                self.log_info(&format!("Encryption algorithm: {:?} ({} security)", algorithm, security));
            }
        } else {
            if config.has_field("algorithm") {
                self.add_warning(result, WARNING_MISSING_FIELD_ADDED_DEFAULT,
                    "Algorithm must be a string - will default to 'aes256-gcm'",
                    config.get_field("algorithm").map(|f| f.position));
            } else {
                self.add_warning(result, WARNING_MISSING_FIELD_ADDED_DEFAULT,
                    "Encryption algorithm not specified - will default to 'aes256-gcm'", None);
            }
        }
    }

    /// Validate KDF parameters (uses O(1) field lookups, NO STRING ALLOCATION)
    fn validate_kdf_parameters(
        &mut self,
        config: &ParsedEncryptionConfig,
        result: &mut SectionAnalysisResult,
        is_verbose: bool,
    ) {
        // Validate KDF algorithm - NO STRING ALLOCATION
        if let Some(kdf) = config.kdf {
            if !is_valid_key_ci(kdf, &VALID_KDF_ALGORITHMS) {
                self.add_warning(result, WARNING_MISSING_FIELD_ADDED_DEFAULT,
                    &format!("Unknown KDF algorithm: {} - will default to 'argon2id'", kdf), None);
            } else if is_verbose {
                self.log_info(&format!("Key derivation function: {}", kdf));
            }
        } else {
            self.add_warning(result, WARNING_MISSING_FIELD_ADDED_DEFAULT,
                "Key derivation function not specified - will default to 'argon2id'", None);
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
        if let Some(value) = config.get_int_field(param_name) {
            if value < min_value {
                self.add_warning(result, WARNING_KDF_PARAMETER_BELOW_MIN,
                    &format!("KDF parameter '{}' value {} below recommended minimum {}",
                        param_name, value, min_value), None);
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
                    self.add_warning(result, "SEC_WARN008",
                        &format!("Backup count {} out of range (0-10) - will use default 3", value),
                        Some(field.position));
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
            self.add_warning(result, WARNING_MANUAL_MODE_ENABLED,
                "Manual mode key warning not explicitly accepted - encryption may fail",
                Some(entry.position));
        } else {
            self.add_warning(result, WARNING_MANUAL_MODE_ENABLED,
                "Manual mode enabled - encryption key stored in PLAINTEXT",
                Some(entry.position));
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

        if let Some(field) = field_map.get("checksum_algorithm") {
            if let Some(algorithm) = extract_string_value(&field.value) {
                // NO STRING ALLOCATION - use &str directly
                if !is_valid_key_ci(algorithm, &VALID_CHECKSUM_ALGORITHMS) {
                    self.add_warning(result, WARNING_MISSING_FIELD_ADDED_DEFAULT,
                        &format!("Unknown checksum algorithm: {} - will default to 'sha256'", algorithm),
                        Some(field.position));
                } else if is_debug {
                    self.log_info(&format!("Checksum algorithm: {}", algorithm));
                }
            }
        }
    }

    // ==================== HELPER METHODS ====================

    #[inline]
    fn log_security_level(&self, mode: EncryptionMode) {
        // ENUM MATCH - NO STRING ALLOCATION
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
        self.error_manager.log_warning(message);
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
