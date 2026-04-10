
//! Semantic validation of the @DLM section.

use crate::Compiler::AST::{DLMSection, DLMModule, DLMModuleType, DLMModuleSubtype, Position};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::ErrorManager::{ErrorManager, SemanticErrorType, DebugConfig};
use rustc_hash::FxHashSet;
use lazy_static::lazy_static;

use super::{SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo};

const ERROR_DUPLICATE_MODULE:      &str = "DUPLICATE_MODULE";
const ERROR_INVALID_MODULE_TYPE:   &str = "INVALID_MODULE_TYPE";
const ERROR_INVALID_MODULE_SUBTYPE: &str = "INVALID_MODULE_SUBTYPE";

const WARN_NO_SUBTYPE:             &str = "DLM_WARN001";
const WARN_SUBOPTIMAL_ORDERING:    &str = "DLM_WARN002";
const WARN_XOR_LOW_SECURITY:       &str = "DLM_WARN003";

// ═════════════════════════════════════════════════════════════════════════════
// Lazy Static Validation Sets — Zero Runtime Initialization Cost
// ═════════════════════════════════════════════════════════════════════════════

lazy_static! {
    /// Valid subtypes for DCompressor module
    static ref VALID_COMPRESSOR_SUBTYPES: FxHashSet<DLMModuleSubtype> = {
        let mut s = FxHashSet::default();
        s.insert(DLMModuleSubtype::Gzip);
        s.insert(DLMModuleSubtype::Bzip2);
        s.insert(DLMModuleSubtype::Lzma);
        s
    };
    
    /// Valid subtypes for DAuditor module
    static ref VALID_AUDITOR_SUBTYPES: FxHashSet<DLMModuleSubtype> = {
        let mut s = FxHashSet::default();
        s.insert(DLMModuleSubtype::Diy);
        s.insert(DLMModuleSubtype::Enhanced);
        s
    };
    
    /// Valid subtypes for DEncryptor module
    static ref VALID_ENCRYPTOR_SUBTYPES: FxHashSet<DLMModuleSubtype> = {
        let mut s = FxHashSet::default();
        s.insert(DLMModuleSubtype::Xor);
        s.insert(DLMModuleSubtype::Aes128);
        s.insert(DLMModuleSubtype::Aes256);
        s.insert(DLMModuleSubtype::Chacha20);
        s
    };
}

pub struct DlmSectionAnalyzer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
}

impl<'a> DlmSectionAnalyzer<'a> {
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
      Self::new_with_error_manager(operational_settings,ErrorManager::get_shared_instance())
    }
pub fn new_with_error_manager(
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
) -> Self {
    DlmSectionAnalyzer {
        error_manager,
        debug_config: DebugConfig::from_debug_mode(operational_settings.debug_mode),
        operational_settings,
    }
}
    pub fn analyze(
        &mut self,
        section: &DLMSection,
        _symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        let mut result = SectionAnalysisResult::new("DLM");
        let module_count = section.modules.len();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Analyzing DLM section with {} module definitions", module_count
            ));
        }

        let (duplicate_modules, has_encryptor) =
            self.check_duplicate_modules(&section.modules, &mut result);

        if self.should_halt(&result) {
            return result;
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug("Validating individual modules");
        }

        for module in &section.modules {
            let key = Self::module_key(module);
            if duplicate_modules.contains(&key) {
                continue;
            }
            self.validate_module(module, &mut result);
            if self.should_halt(&result) {
                return result;
            }
        }

        self.validate_ordering(&section.modules, &duplicate_modules, &mut result);

        if self.should_halt(&result) {
            return result;
        }

        self.validate_security_implications(&section.modules, &duplicate_modules, &mut result);

        result.is_success = result.errors.is_empty();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "DLM analysis complete: {} — modules: {}, encryptor: {}, errors: {}, warnings: {}",
                if result.is_success { "SUCCESS" } else { "FAILURE" },
                module_count.saturating_sub(duplicate_modules.len()),
                has_encryptor,
                result.errors.len(),
                result.warnings.len()
            ));
        }

        result
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Duplicate Detection — Uses Tuple Key (Zero Allocation!)
    // ═════════════════════════════════════════════════════════════════════════

    fn check_duplicate_modules(
        &mut self,
        modules: &[DLMModule],
        result: &mut SectionAnalysisResult,
    ) -> (FxHashSet<(DLMModuleType, Option<DLMModuleSubtype>)>, bool) {
        let mut seen = FxHashSet::default();
        let mut duplicates = FxHashSet::default();
        let mut has_encryptor = false;

        for module in modules {
            let key = Self::module_key(module);
            if !seen.insert(key) {
                duplicates.insert(key);
                // Only format String for error message (cold path)
                let display_key = Self::format_module_key(module);
                self.add_error(
                    result,
                    "DLM001",
                    ERROR_DUPLICATE_MODULE,
                    &format!("Module '{}' is defined multiple times", display_key),
                    "Each DLM module (type + subtype) can only appear once",
                    Some(module.position),
                );
            }
            if module.module_type == DLMModuleType::DEncryptor {
                has_encryptor = true;
            }
        }

        (duplicates, has_encryptor)
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Module Validation
    // ═════════════════════════════════════════════════════════════════════════

    fn validate_module(&mut self, module: &DLMModule, result: &mut SectionAnalysisResult) {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "Validating module: {:?}", module.module_type
            ));
        }

        if module.module_type == DLMModuleType::ParseError {
            self.add_error(
                result,
                "DLM002",
                ERROR_INVALID_MODULE_TYPE,
                &format!("Unknown module type: {:?}", module.module_type),
                "Valid module types: DCompressor, DAuditor, DEncryptor",
                Some(module.position),
            );
            return;
        }

        let subtype = match module.subtype {
            None => {
                self.add_warning(
                    result,
                    WARN_NO_SUBTYPE,
                    &format!(
                        "Module '{:?}' has no subtype specified - using default",
                        module.module_type
                    ),
                    Some(module.position),
                );
                return;
            }
            Some(s) => s,
        };

        if !Self::is_valid_subtype(module.module_type, subtype) {
            self.add_error(
                result,
                "DLM003",
                ERROR_INVALID_MODULE_SUBTYPE,
                &format!(
                    "Subtype '{:?}' is not valid for module type '{:?}'",
                    subtype, module.module_type
                ),
                &format!(
                    "Valid subtypes for {:?}: {}",
                    module.module_type,
                    Self::valid_subtypes_str(module.module_type)
                ),
                Some(module.position),
            );
            return;
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "Module validated: {:?}.{:?} — {}",
                module.module_type,
                subtype,
                Self::subtype_description(subtype)
            ));
        }
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Ordering Validation
    // ═════════════════════════════════════════════════════════════════════════

    fn validate_ordering(
        &mut self,
        modules: &[DLMModule],
        duplicates: &FxHashSet<(DLMModuleType, Option<DLMModuleSubtype>)>,
        result: &mut SectionAnalysisResult,
    ) {
        let compressor_idx = modules.iter()
            .enumerate()
            .filter(|(_, m)| !duplicates.contains(&Self::module_key(m)))
            .find(|(_, m)| m.module_type == DLMModuleType::DCompressor)
            .map(|(i, _)| i);

        let encryptor_idx = modules.iter()
            .enumerate()
            .filter(|(_, m)| !duplicates.contains(&Self::module_key(m)))
            .find(|(_, m)| m.module_type == DLMModuleType::DEncryptor)
            .map(|(i, _)| i);

        if let (Some(comp), Some(enc)) = (compressor_idx, encryptor_idx) {
            if enc < comp {
                self.add_warning(
                    result,
                    WARN_SUBOPTIMAL_ORDERING,
                    "DCompressor should appear before DEncryptor for optimal performance",
                    None,
                );
            }
        }
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Security Implications Validation
    // ═════════════════════════════════════════════════════════════════════════

    fn validate_security_implications(
        &mut self,
        modules: &[DLMModule],
        duplicates: &FxHashSet<(DLMModuleType, Option<DLMModuleSubtype>)>,
        result: &mut SectionAnalysisResult,
    ) {
        for module in modules.iter().filter(|m| !duplicates.contains(&Self::module_key(m))) {
            if module.module_type != DLMModuleType::DEncryptor {
                continue;
            }
            if let Some(subtype) = module.subtype {
                match subtype {
                    DLMModuleSubtype::Xor => {
                        self.add_warning(
                            result,
                            WARN_XOR_LOW_SECURITY,
                            "Encryption subtype 'Xor' provides LOW security - suitable for obfuscation only",
                            Some(module.position),
                        );
                    }
                    DLMModuleSubtype::Aes128 | DLMModuleSubtype::Aes256 | DLMModuleSubtype::Chacha20 => {
                        if self.debug_config.is_enabled {
                            self.error_manager.log_info(&format!(
                                "DEncryptor.{:?} — Security Level: {}",
                                subtype,
                                Self::encryption_security_level(subtype)
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Helper Functions — All Inline for Performance
    // ═════════════════════════════════════════════════════════════════════════

    /// Module key for deduplication — returns tuple (zero allocation!)
    #[inline]
    fn module_key(module: &DLMModule) -> (DLMModuleType, Option<DLMModuleSubtype>) {
        (module.module_type, module.subtype)
    }

    /// Format module key for display in error messages (only called on error path)
    #[inline]
    fn format_module_key(module: &DLMModule) -> String {
        match module.subtype {
            Some(s) => format!("{:?}.{:?}", module.module_type, s),
            None    => format!("{:?}", module.module_type),
        }
    }

    /// Check if subtype is valid for module type — uses lazy_static sets (O(1))
    #[inline]
    fn is_valid_subtype(module_type: DLMModuleType, subtype: DLMModuleSubtype) -> bool {
        match module_type {
            DLMModuleType::DCompressor => VALID_COMPRESSOR_SUBTYPES.contains(&subtype),
            DLMModuleType::DAuditor    => VALID_AUDITOR_SUBTYPES.contains(&subtype),
            DLMModuleType::DEncryptor  => VALID_ENCRYPTOR_SUBTYPES.contains(&subtype),
            DLMModuleType::ParseError  => false,
        }
    }

    /// Get valid subtypes as a display string (only called on error path)
    #[inline]
    fn valid_subtypes_str(module_type: DLMModuleType) -> &'static str {
        match module_type {
            DLMModuleType::DCompressor => "Gzip, Bzip2, Lzma",
            DLMModuleType::DAuditor    => "Diy, Enhanced",
            DLMModuleType::DEncryptor  => "Xor, Aes128, Aes256, Chacha20",
            DLMModuleType::ParseError  => "none",
        }
    }

    /// Get subtype description
    #[inline]
    fn subtype_description(subtype: DLMModuleSubtype) -> &'static str {
        match subtype {
            DLMModuleSubtype::Gzip      => "fast compression, moderate ratio",
            DLMModuleSubtype::Bzip2     => "better compression, slower",
            DLMModuleSubtype::Lzma      => "best compression, slowest",
            DLMModuleSubtype::Diy       => "simple text audit log",
            DLMModuleSubtype::Enhanced  => "structured comprehensive audit trail",
            DLMModuleSubtype::Xor       => "XOR cipher — obfuscation only, LOW security",
            DLMModuleSubtype::Aes128    => "AES-128-GCM — faster, MEDIUM security",
            DLMModuleSubtype::Aes256    => "AES-256-GCM — recommended, HIGH security",
            DLMModuleSubtype::Chacha20  => "ChaCha20-Poly1305 — modern, HIGH security",
            DLMModuleSubtype::ParseError => "parse error",
        }
    }

    /// Get encryption security level description
    #[inline]
    fn encryption_security_level(subtype: DLMModuleSubtype) -> &'static str {
        match subtype {
            DLMModuleSubtype::Xor      => "LOW — obfuscation only",
            DLMModuleSubtype::Aes128   => "MEDIUM — faster, suitable for most use cases",
            DLMModuleSubtype::Aes256   => "HIGH — recommended for sensitive data",
            DLMModuleSubtype::Chacha20 => "HIGH — modern, mobile-optimised",
            _                          => "UNKNOWN",
        }
    }

    /// Check if should halt based on error handling strategy
    #[inline]
    fn should_halt(&self, result: &SectionAnalysisResult) -> bool {
        !result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Error/Warning Management
    // ═════════════════════════════════════════════════════════════════════════

    fn add_error(
        &mut self,
        result: &mut SectionAnalysisResult,
        error_id: &str,
        error_type: &str,
        message: &str,
        suggestion: &str,
        position: Option<Position>,
    ) {
        result.errors.push(SemanticErrorInfo {
            error_id:     error_id.to_string(),
            error_type:   error_type.to_string(),
            message:      message.to_string(),
            section_name: "DLM".to_string(),
            suggestion:   suggestion.to_string(),
            position,
        });

        let (line, col) = position.map(|p| (p.line as i32, p.column as i32)).unwrap_or((0, 0));
        self.error_manager.add_semantic_error(
            SemanticErrorType::DuplicateDefinition,
            message.to_string(),
            line, col,
            Some("DLM".to_string()),
            Some(suggestion.to_string()),
        );
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
            section_name: "DLM".to_string(),
            position,
        });
        if self.debug_config.is_enabled {
            self.error_manager.log_warning(message);
        }
    }
        }
