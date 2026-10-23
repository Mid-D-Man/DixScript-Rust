// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/SectionAnalyzers/security_section_analyzer.rs"
// ============================================================================
//! Semantic validation of the `@SECURITY` section.
//!
//! The analyzer only ever warns: an unknown block, a missing field, a weak algorithm or a
//! parameter below its recommended minimum all fall back to a default at encryption time, so
//! none of them stops a compile. `result.errors` stays empty.
//!
//! The checks run over the arena form of the section:
//! [`analyze_doc`](SecuritySectionAnalyzer::analyze_doc) reads a [`DocSecurity`] and the
//! [`Arena`] its values live in, and is what `GeneralSemanticAnalyzer` calls when it has a
//! `Document`. [`analyze`](SecuritySectionAnalyzer::analyze) keeps the owned-type signature for
//! callers that only have a `DixScript`: it copies the section into a scratch arena and runs
//! the same checks, so the two entry points cannot disagree.

use crate::Compiler::AST::arena::{Arena, DocSecurity, DocSecurityEntry, DocSecurityField, ValueId, ValueNode};
use crate::Compiler::AST::{Position, SecuritySection};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::ErrorManager::{ErrorManager, DebugConfig};
use crate::Utilities::RustcHash::FxHashMap;
use crate::Utilities::LazyStatic::lazy_static;

use super::{SectionAnalysisResult, SemanticWarningInfo};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum EncryptionMode {
    Password,
    Keyfile,
    Manual,
}

impl EncryptionMode {
    #[inline]
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "password" => Some(Self::Password),
            "keyfile"  => Some(Self::Keyfile),
            "manual"   => Some(Self::Manual),
            _          => None,
        }
    }

    #[inline]
    fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Keyfile  => "keyfile",
            Self::Manual   => "manual",
        }
    }

    #[inline]
    fn required_fields(self) -> &'static [&'static str] {
        match self {
            Self::Password => &["mode", "algorithm", "kdf"],
            Self::Keyfile  => &["mode", "algorithm"],
            Self::Manual   => &["mode", "key", "iv"],
        }
    }
}

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
        match s {
            "xor"                => Some(Self::Xor),
            "aes128-gcm"         => Some(Self::Aes128Gcm),
            "aes128"             => Some(Self::Aes128),
            "aes256-gcm"         => Some(Self::Aes256Gcm),
            "aes256"             => Some(Self::Aes256),
            "chacha20-poly1305"  => Some(Self::Chacha20Poly1305),
            "chacha20"           => Some(Self::Chacha20),
            _                    => None,
        }
    }

    #[inline]
    fn is_high_security(self) -> bool {
        matches!(self, Self::Aes256Gcm | Self::Aes256 | Self::Chacha20Poly1305 | Self::Chacha20)
    }

    #[inline]
    fn is_low_security(self) -> bool {
        matches!(self, Self::Xor)
    }
}

lazy_static! {
    static ref VALID_BLOCK_KEYS: FxHashMap<&'static str, ()> = {
        let mut m = FxHashMap::default();
        m.insert("encryption", ());
        m.insert("validation", ());
        m.insert("keystore",   ());
        m.insert("override",   ());
        m.insert("metadata",   ());
        m
    };

    static ref VALID_CHECKSUM_ALGORITHMS: FxHashMap<&'static str, ()> = {
        let mut m = FxHashMap::default();
        m.insert("sha256",       ());
        m.insert("sha512",       ());
        m.insert("hmac-sha256",  ());
        m.insert("hmac-sha512",  ());
        m
    };

    static ref VALID_KDF_ALGORITHMS: FxHashMap<&'static str, ()> = {
        let mut m = FxHashMap::default();
        m.insert("argon2id", ());
        m.insert("pbkdf2",   ());
        m
    };
}

const WARN_EMPTY_SECTION:           &str = "SEC_WARN001";
const WARN_EMPTY_BLOCK:             &str = "SEC_WARN002";
const WARN_MANUAL_MODE_CRITICAL:    &str = "SEC_WARN003";
const WARN_XOR_LOW_SECURITY:        &str = "SEC_WARN004";
const WARN_KDF_PARAM_BELOW_MIN:     &str = "SEC_WARN005";
const WARN_MANUAL_MODE_ENABLED:     &str = "SEC_WARN006";
const WARN_MISSING_FIELD_DEFAULT:   &str = "SEC_WARN007";
const WARN_BACKUP_COUNT_RANGE:      &str = "SEC_WARN008";

struct ParsedEncryptionConfig<'a> {
    arena:     &'a Arena,
    mode:      Option<EncryptionMode>,
    algorithm: Option<Algorithm>,
    kdf:       Option<&'a str>,
    field_map: FxHashMap<&'a str, &'a DocSecurityField>,
}

impl<'a> ParsedEncryptionConfig<'a> {
    fn from_entry(entry: &'a DocSecurityEntry, arena: &'a Arena) -> Self {
        // A repeated key keeps its last field here. `validate_manual_mode` takes the first
        // instead, as it always has; both behaviours are pinned by tests.
        let mut field_map: FxHashMap<&str, &DocSecurityField> = FxHashMap::default();
        for field in &entry.fields {
            field_map.insert(field.key.as_str(), field);
        }

        let mode = field_map.get("mode")
            .and_then(|f| extract_str(arena, f.value))
            .and_then(EncryptionMode::from_str);

        let algorithm = field_map.get("algorithm")
            .and_then(|f| extract_str(arena, f.value))
            .and_then(Algorithm::from_str);

        let kdf = field_map.get("kdf")
            .and_then(|f| extract_str(arena, f.value));

        ParsedEncryptionConfig { arena, mode, algorithm, kdf, field_map }
    }

    #[inline] fn has_field(&self, key: &str) -> bool { self.field_map.contains_key(key) }

    #[inline] fn get_field(&self, key: &str) -> Option<&'a DocSecurityField> {
        self.field_map.get(key).copied()
    }

    #[inline] fn get_int_field(&self, key: &str) -> Option<i32> {
        self.get_field(key).and_then(|f| match self.arena.value(f.value) {
            ValueNode::Integer { value, .. } => Some(*value),
            _ => None,
        })
    }
    #[inline] fn get_long_field(&self, key: &str) -> Option<i64> {
        self.get_field(key).and_then(|f| match self.arena.value(f.value) {
            ValueNode::Long { value, .. } => Some(*value),
            _ => None,
        })
    }
    #[inline] fn get_bool_field(&self, key: &str) -> Option<bool> {
        self.get_field(key).and_then(|f| match self.arena.value(f.value) {
            ValueNode::Boolean { value, .. } => Some(*value),
            _ => None,
        })
    }
}

#[inline]
fn extract_str<'a>(arena: &'a Arena, id: ValueId) -> Option<&'a str> {
    match arena.value(id) {
        ValueNode::String { value, .. } => Some(arena.text(*value)),
        _ => None,
    }
}

#[inline]
fn extract_int(arena: &Arena, id: ValueId) -> Option<i32> {
    match arena.value(id) {
        ValueNode::Integer { value, .. } => Some(*value),
        _ => None,
    }
}

#[inline]
fn extract_bool(arena: &Arena, id: ValueId) -> Option<bool> {
    match arena.value(id) {
        ValueNode::Boolean { value, .. } => Some(*value),
        _ => None,
    }
}

/// Case-insensitive lookup against a pre-built lowercase key map.
#[inline]
fn is_valid_key_ci(key: &str, valid: &FxHashMap<&'static str, ()>) -> bool {
    valid.contains_key(key) || valid.keys().any(|k| k.eq_ignore_ascii_case(key))
}

pub struct SecuritySectionAnalyzer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
}

impl<'a> SecuritySectionAnalyzer<'a> {
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
      Self::new_with_error_manager(operational_settings,ErrorManager::get_shared_instance())
    }
pub fn new_with_error_manager(
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
) -> Self {
    SecuritySectionAnalyzer {
        error_manager,
        debug_config: DebugConfig::from_debug_mode(operational_settings.debug_mode),
        operational_settings,
    }
}
    /// Analyzes a `@SECURITY` section held as the owned type.
    ///
    /// This is the entry point for callers that only have a `DixScript`. It copies the section
    /// into a scratch arena and calls [`analyze_doc`](Self::analyze_doc), so the diagnostics
    /// are the ones `analyze_doc` gives for the same section.
    ///
    /// # Panics
    ///
    /// Panics if the section needs more nodes or text than an arena can address (more than
    /// `u32::MAX` of either).
    pub fn analyze(
        &mut self,
        section: &SecuritySection,
        symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        let mut arena = Arena::new();
        let doc_section = DocSecurity::from_owned(&mut arena, section);
        self.analyze_doc(&doc_section, &arena, symbol_table)
    }

    /// Analyzes the `@SECURITY` section of a `Document`.
    ///
    /// `section` and `arena` must come from the same `Document` (its `security` field and its
    /// `arena` field): the section holds ids into the arena and has no meaning without it.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, DocSecurity};
    /// use dixscript::Compiler::AST::{Position, SecurityEntry, SecurityField, SecuritySection, Value};
    /// use dixscript::Compiler::Core::Config::OperationalSettings;
    /// use dixscript::Compiler::Core::SectionAnalyzers::SecuritySectionAnalyzer;
    /// use dixscript::Compiler::Utilities::SymbolTable;
    ///
    /// let pos = Position::new(1, 1);
    /// let field = |key: &str, text: &str| SecurityField::new(
    ///     key.to_string(),
    ///     Value::String { value: text.to_string(), position: pos },
    ///     pos,
    /// );
    /// let owned = SecuritySection::new(
    ///     vec![SecurityEntry::new(
    ///         "encryption".to_string(),
    ///         vec![field("mode", "keyfile"), field("algorithm", "xor")],
    ///         pos,
    ///     )],
    ///     pos,
    /// );
    ///
    /// let mut arena = Arena::new();
    /// let section = DocSecurity::from_owned(&mut arena, &owned);
    ///
    /// let settings = OperationalSettings::default();
    /// let mut analyzer = SecuritySectionAnalyzer::new(&settings);
    /// let result = analyzer.analyze_doc(&section, &arena, &mut SymbolTable::new());
    ///
    /// assert!(result.is_success);
    /// assert_eq!(result.warnings[0].warning_id, "SEC_WARN004");
    /// ```
    pub fn analyze_doc(
        &mut self,
        section: &DocSecurity,
        arena: &Arena,
        _symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        let mut result = SectionAnalysisResult::new("SECURITY");

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Analyzing SECURITY section with {} entries",
                section.entries.len()
            ));
        }

        if section.entries.is_empty() {
            self.add_warning(&mut result, WARN_EMPTY_SECTION,
                "SECURITY section is empty - default settings will be used", None);
            result.is_success = true;
            return result;
        }

        let entry_map = Self::build_entry_map(&section.entries);

        self.validate_structure(&section.entries, &mut result);

        if self.should_halt(&result) {
            return result;
        }

        let encryption_config = entry_map.get("encryption")
            .map(|e| ParsedEncryptionConfig::from_entry(e, arena));

        let mode = match encryption_config {
            Some(ref cfg) => self.validate_encryption_mode(cfg, &mut result),
            None => {
                self.add_warning(&mut result, WARN_MISSING_FIELD_DEFAULT,
                    "No encryption configuration found - defaults will be applied", None);
                Some(EncryptionMode::Keyfile)
            }
        };

        if let Some(m) = mode {
            if let Some(ref cfg) = encryption_config {
                self.validate_mode_requirements(m, cfg, &mut result);
                self.validate_algorithm(cfg, &mut result);
            }

            match m {
                EncryptionMode::Password => {
                    if let Some(ref cfg) = encryption_config {
                        self.validate_kdf_parameters(cfg, &mut result);
                    }
                }
                EncryptionMode::Keyfile => {
                    if let Some(entry) = entry_map.get("keystore") {
                        self.validate_keystore(entry, arena, &mut result);
                    }
                }
                EncryptionMode::Manual => {
                    if let Some(entry) = entry_map.get("override") {
                        self.validate_manual_mode(entry, arena, &mut result);
                    }
                }
            }

            if let Some(entry) = entry_map.get("validation") {
                self.validate_validation_config(entry, arena, &mut result);
            }

            if self.debug_config.is_enabled {
                self.log_security_level(m);
            }
        }

        result.is_success = result.errors.is_empty();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "SECURITY analysis complete: {} — errors: {}, warnings: {}",
                if result.is_success { "SUCCESS" } else { "FAILURE" },
                result.errors.len(),
                result.warnings.len()
            ));
        }

        result
    }

    fn build_entry_map<'e>(entries: &'e [DocSecurityEntry]) -> FxHashMap<&'static str, &'e DocSecurityEntry> {
        let mut map: FxHashMap<&'static str, &'e DocSecurityEntry> = FxHashMap::default();
        for entry in entries {
            let key = entry.block_key.as_str();
            let canonical: Option<&'static str> = if key.eq_ignore_ascii_case("encryption") {
                Some("encryption")
            } else if key.eq_ignore_ascii_case("validation") {
                Some("validation")
            } else if key.eq_ignore_ascii_case("keystore") {
                Some("keystore")
            } else if key.eq_ignore_ascii_case("override") {
                Some("override")
            } else if key.eq_ignore_ascii_case("metadata") {
                Some("metadata")
            } else {
                None
            };
            if let Some(k) = canonical {
                map.insert(k, entry);
            }
        }
        map
    }

    fn validate_structure(
        &mut self,
        entries: &[DocSecurityEntry],
        result: &mut SectionAnalysisResult,
    ) {
        for entry in entries {
            if !is_valid_key_ci(entry.block_key.as_str(), &VALID_BLOCK_KEYS) {
                if self.debug_config.is_enabled {
                    self.error_manager.log_warning(&format!(
                        "Unknown security block key: {} (ignored)", entry.block_key
                    ));
                }
                self.add_warning(result, WARN_EMPTY_BLOCK,
                    &format!("Unknown security block key: {} (ignored)", entry.block_key),
                    Some(entry.position));
            }
            if entry.fields.is_empty() {
                self.add_warning(result, WARN_EMPTY_BLOCK,
                    &format!("Security block '{}' is empty", entry.block_key),
                    Some(entry.position));
            }
        }
    }

    fn validate_encryption_mode(
        &mut self,
        config: &ParsedEncryptionConfig,
        result: &mut SectionAnalysisResult,
    ) -> Option<EncryptionMode> {
        if let Some(mode) = config.mode {
            if self.debug_config.is_enabled {
                self.error_manager.log_info(&format!("Encryption mode: {}", mode.as_str()));
            }
            Some(mode)
        } else {
            let pos = config.get_field("mode").map(|f| f.position);
            let msg = if config.has_field("mode") {
                "Encryption mode must be a string - defaulting to 'keyfile'"
            } else {
                "Encryption mode not specified - defaulting to 'keyfile'"
            };
            self.add_warning(result, WARN_MISSING_FIELD_DEFAULT, msg, pos);
            Some(EncryptionMode::Keyfile)
        }
    }

    fn validate_mode_requirements(
        &mut self,
        mode: EncryptionMode,
        config: &ParsedEncryptionConfig,
        result: &mut SectionAnalysisResult,
    ) {
        for &field in mode.required_fields() {
            if !config.has_field(field) {
                self.add_warning(result, WARN_MISSING_FIELD_DEFAULT,
                    &format!("Required field '{}' missing for mode '{}' - will use default",
                        field, mode.as_str()),
                    None);
            }
        }
        if mode == EncryptionMode::Manual {
            self.add_warning(result, WARN_MANUAL_MODE_CRITICAL,
                "CRITICAL: Manual mode stores the encryption key in PLAINTEXT in source", None);
        }
    }

    fn validate_algorithm(
        &mut self,
        config: &ParsedEncryptionConfig,
        result: &mut SectionAnalysisResult,
    ) {
        if let Some(alg) = config.algorithm {
            if alg.is_low_security() {
                self.add_warning(result, WARN_XOR_LOW_SECURITY,
                    "Algorithm 'xor' provides LOW security - obfuscation only", None);
            } else if self.debug_config.is_enabled {
                self.error_manager.log_info(&format!(
                    "Encryption algorithm: {:?} ({} security)",
                    alg,
                    if alg.is_high_security() { "HIGH" } else { "MEDIUM" }
                ));
            }
        } else {
            let pos = config.get_field("algorithm").map(|f| f.position);
            let msg = if config.has_field("algorithm") {
                "Algorithm must be a string - will default to 'aes256-gcm'"
            } else {
                "Encryption algorithm not specified - will default to 'aes256-gcm'"
            };
            self.add_warning(result, WARN_MISSING_FIELD_DEFAULT, msg, pos);
        }
    }

    fn validate_kdf_parameters(
        &mut self,
        config: &ParsedEncryptionConfig,
        result: &mut SectionAnalysisResult,
    ) {
        if let Some(kdf) = config.kdf {
            if !is_valid_key_ci(kdf, &VALID_KDF_ALGORITHMS) {
                self.add_warning(result, WARN_MISSING_FIELD_DEFAULT,
                    &format!("Unknown KDF algorithm: {} - will default to 'argon2id'", kdf), None);
            } else if self.debug_config.is_verbose {
                self.error_manager.log_info(&format!("Key derivation function: {}", kdf));
            }
        } else {
            self.add_warning(result, WARN_MISSING_FIELD_DEFAULT,
                "Key derivation function not specified - will default to 'argon2id'", None);
        }

        self.validate_kdf_param(config, "kdf_memory",      65536, result);
        self.validate_kdf_param(config, "kdf_iterations",      3, result);
        self.validate_kdf_param(config, "kdf_parallelism",     4, result);
    }

    #[inline]
    fn validate_kdf_param(
        &mut self,
        config: &ParsedEncryptionConfig,
        param: &str,
        min: i32,
        result: &mut SectionAnalysisResult,
    ) {
        if let Some(v) = config.get_int_field(param) {
            if v < min {
                self.add_warning(result, WARN_KDF_PARAM_BELOW_MIN,
                    &format!("KDF parameter '{}' value {} below recommended minimum {}",
                        param, v, min),
                    None);
            }
        } else if self.debug_config.is_verbose && config.has_field(param) {
            self.error_manager.log_debug(&format!(
                "KDF parameter '{}' must be integer - will use default", param
            ));
        }
    }

    fn validate_keystore(
        &mut self,
        entry: &DocSecurityEntry,
        arena: &Arena,
        result: &mut SectionAnalysisResult,
    ) {
        let mut field_map: FxHashMap<&str, &DocSecurityField> = FxHashMap::default();
        for f in &entry.fields { field_map.insert(f.key.as_str(), f); }

        if let Some(f) = field_map.get("auto_generate") {
            if let Some(v) = extract_bool(arena, f.value) {
                if self.debug_config.is_enabled {
                    self.error_manager.log_info(&format!("Keystore auto-generation: {}", v));
                }
            }
        }

        if let Some(f) = field_map.get("backup_count") {
            if let Some(v) = extract_int(arena, f.value) {
                if !(0..=10).contains(&v) {
                    self.add_warning(result, WARN_BACKUP_COUNT_RANGE,
                        &format!("Backup count {} out of range (0-10) - will use default 3", v),
                        Some(f.position));
                } else if self.debug_config.is_enabled {
                    self.error_manager.log_info(&format!("Keystore backup count: {}", v));
                }
            }
        }
    }

    fn validate_manual_mode(
        &mut self,
        entry: &DocSecurityEntry,
        arena: &Arena,
        result: &mut SectionAnalysisResult,
    ) {
        let accepted = entry.fields.iter()
            .find(|f| f.key == "manual_key_warning_accepted")
            .and_then(|f| extract_bool(arena, f.value))
            .unwrap_or(false);

        let msg = if accepted {
            "Manual mode enabled - encryption key stored in PLAINTEXT"
        } else {
            "Manual mode key warning not explicitly accepted - encryption may fail"
        };
        self.add_warning(result, WARN_MANUAL_MODE_ENABLED, msg, Some(entry.position));
    }

    fn validate_validation_config(
        &mut self,
        entry: &DocSecurityEntry,
        arena: &Arena,
        result: &mut SectionAnalysisResult,
    ) {
        let mut field_map: FxHashMap<&str, &DocSecurityField> = FxHashMap::default();
        for f in &entry.fields { field_map.insert(f.key.as_str(), f); }

        if let Some(f) = field_map.get("checksum_algorithm") {
            if let Some(alg) = extract_str(arena, f.value) {
                if !is_valid_key_ci(alg, &VALID_CHECKSUM_ALGORITHMS) {
                    self.add_warning(result, WARN_MISSING_FIELD_DEFAULT,
                        &format!("Unknown checksum algorithm: {} - will default to 'sha256'", alg),
                        Some(f.position));
                } else if self.debug_config.is_enabled {
                    self.error_manager.log_info(&format!("Checksum algorithm: {}", alg));
                }
            }
        }
    }

    #[inline]
    fn log_security_level(&self, mode: EncryptionMode) {
        let level = match mode {
            EncryptionMode::Password => "HIGH (Argon2id-derived key)",
            EncryptionMode::Keyfile  => "HIGH (randomly generated key)",
            EncryptionMode::Manual   => "CRITICAL_RISK (plaintext key)",
        };
        self.error_manager.log_info(&format!("Security level: {}", level));
    }

    #[inline]
    fn should_halt(&self, result: &SectionAnalysisResult) -> bool {
        !result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

    fn add_warning(
        &mut self,
        result: &mut SectionAnalysisResult,
        warning_id: &str,
        message: &str,
        position: Option<Position>,
    ) {
        result.warnings.push(SemanticWarningInfo {
            warning_id:   warning_id.to_string(),
            message:      message.to_string(),
            section_name: "SECURITY".to_string(),
            position,
        });
        if self.debug_config.is_enabled {
            self.error_manager.log_warning(message);
        }
    }
}
