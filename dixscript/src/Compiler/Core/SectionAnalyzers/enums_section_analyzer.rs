
//! Semantic validation of the @ENUMS section and symbol table population.

use crate::Compiler::AST::{EnumsSection, EnumDeclaration, EnumField, Position};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::Compiler::VersionControl::VersionConstraints;
use crate::ErrorManager::{ErrorManager, SemanticErrorType, DebugConfig};
use rustc_hash::{FxHashSet, FxHashMap};

use super::{SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo};

const ERROR_DUPLICATE_ENUM_NAME:   &str = "DUPLICATE_ENUM_NAME";
const ERROR_INVALID_ENUM_NAME:     &str = "INVALID_ENUM_NAME";
const ERROR_DUPLICATE_FIELD_NAME:  &str = "DUPLICATE_FIELD_NAME";
const ERROR_INVALID_FIELD_NAME:    &str = "INVALID_FIELD_NAME";
const ERROR_DUPLICATE_FIELD_VALUE: &str = "DUPLICATE_FIELD_VALUE";
const ERROR_UNSUPPORTED_SECTION:   &str = "UNSUPPORTED_SECTION";

const WARN_EMPTY_ENUM_FIELDS: &str = "ENUM_WARN001";

// ═════════════════════════════════════════════════════════════════════════════
// Case-Insensitive Key — Stores Lowercase Once for O(1) Lookups
// ═════════════════════════════════════════════════════════════════════════════

/// Wrapper for case-insensitive string comparison.
/// 
/// Stores both original and lowercase version. The lowercase version is used
/// for hashing and equality, giving O(1) case-insensitive lookups instead of
/// the O(n) scan that `contains_ci()` required.
#[derive(Debug, Clone)]
struct CaseInsensitiveKey {
    original: String,
    lowercase: String,
}

impl CaseInsensitiveKey {
    #[inline]
    fn new(s: String) -> Self {
        let lowercase = s.to_ascii_lowercase();
        CaseInsensitiveKey { original: s, lowercase }
    }
    
    #[inline]
    fn as_str(&self) -> &str {
        &self.original
    }
}

impl PartialEq for CaseInsensitiveKey {
    fn eq(&self, other: &Self) -> bool {
        self.lowercase == other.lowercase
    }
}

impl Eq for CaseInsensitiveKey {}

impl std::hash::Hash for CaseInsensitiveKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.lowercase.hash(state);
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Enums Section Analyzer
// ═════════════════════════════════════════════════════════════════════════════

pub struct EnumsSectionAnalyzer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
}

impl<'a> EnumsSectionAnalyzer<'a> {
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        EnumsSectionAnalyzer {
            error_manager: ErrorManager::get_shared_instance(),
            debug_config: DebugConfig::from_debug_mode(operational_settings.debug_mode),
            operational_settings,
        }
    }
pub fn new_with_error_manager(
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
) -> Self {
    EnumsSectionAnalyzer {
        error_manager,
        debug_config: DebugConfig::from_debug_mode(operational_settings.debug_mode),
        operational_settings,
    }
                                                       }
    pub fn analyze(
        &mut self,
        section: &EnumsSection,
        symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        let mut result = SectionAnalysisResult::new("ENUMS");
        let enum_count = section.enums.len();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Analyzing ENUMS section with {} enum definitions", enum_count
            ));
        }

        // Version check — created once per analyze call
        if !VersionConstraints::new().is_valid_section_type("ENUMS") {
            self.add_error(
                &mut result,
                "ENUM000",
                ERROR_UNSUPPORTED_SECTION,
                "ENUMS section is not supported in current DixScript version",
                "Upgrade compiler to v1.0.0 or higher",
                None,
            );
            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
                return result;
            }
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug("Checking for duplicate enum names");
        }

        let duplicate_enums = self.check_duplicate_enums(&section.enums, &mut result);

        if self.should_halt(&result) {
            return result;
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug("Validating individual enum declarations");
        }

        let mut invalid_enums = FxHashSet::default();

        for enum_decl in &section.enums {
            let key = CaseInsensitiveKey::new(enum_decl.name.clone());
            if duplicate_enums.contains(&key) {
                if self.debug_config.is_verbose {
                    self.error_manager.log_warning(&format!(
                        "Skipping validation of duplicate enum '{}'", enum_decl.name
                    ));
                }
                continue;
            }
            self.validate_enum(enum_decl, &mut result, &mut invalid_enums);
            if self.should_halt(&result) {
                return result;
            }
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug("Populating symbol table with enum definitions");
        }

        self.populate_symbol_table(section, symbol_table, &duplicate_enums, &invalid_enums);

        result.is_success = result.errors.is_empty();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "ENUMS analysis complete: {} — valid: {}, errors: {}, warnings: {}",
                if result.is_success { "SUCCESS" } else { "FAILURE" },
                enum_count.saturating_sub(duplicate_enums.len()).saturating_sub(invalid_enums.len()),
                result.errors.len(),
                result.warnings.len()
            ));
        }

        result
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Duplicate Detection — O(1) Lookup with CaseInsensitiveKey
    // ═════════════════════════════════════════════════════════════════════════

    fn check_duplicate_enums(
        &mut self,
        enums: &[EnumDeclaration],
        result: &mut SectionAnalysisResult,
    ) -> FxHashSet<CaseInsensitiveKey> {
        let mut seen = FxHashSet::default();
        let mut duplicates = FxHashSet::default();

        for enum_decl in enums {
            let key = CaseInsensitiveKey::new(enum_decl.name.clone());
            if !seen.insert(key.clone()) {
                duplicates.insert(key.clone());
                self.add_error(
                    result,
                    "ENUM001",
                    ERROR_DUPLICATE_ENUM_NAME,
                    &format!("Enum '{}' is defined multiple times", key.as_str()),
                    "Each enum must have a unique name — remove or rename duplicate definitions",
                    Some(enum_decl.position),
                );
            }
        }

        duplicates
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Enum Validation
    // ═════════════════════════════════════════════════════════════════════════

    fn validate_enum(
        &mut self,
        enum_decl: &EnumDeclaration,
        result: &mut SectionAnalysisResult,
        invalid_enums: &mut FxHashSet<CaseInsensitiveKey>,
    ) {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!("Validating enum: {}", enum_decl.name));
        }

        if !Self::is_valid_identifier(&enum_decl.name) {
            let key = CaseInsensitiveKey::new(enum_decl.name.clone());
            invalid_enums.insert(key);
            self.add_error(
                result,
                "ENUM002",
                ERROR_INVALID_ENUM_NAME,
                &format!("Enum name '{}' is not a valid identifier", enum_decl.name),
                "Enum names must start with a letter or underscore, then alphanumeric or underscore only",
                Some(enum_decl.position),
            );
            return;
        }

        if enum_decl.fields.is_empty() {
            self.add_warning(
                result,
                WARN_EMPTY_ENUM_FIELDS,
                &format!("Enum '{}' has no fields defined", enum_decl.name),
                Some(enum_decl.position),
            );
        }

        let dup_field_names = self.check_duplicate_fields(&enum_decl.fields, enum_decl, result);
        self.validate_field_values(&enum_decl.fields, &dup_field_names, enum_decl, result);

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "Enum '{}' validation complete", enum_decl.name
            ));
        }
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Field Validation — Uses FxHashMap for Performance
    // ═════════════════════════════════════════════════════════════════════════

    fn check_duplicate_fields(
        &mut self,
        fields: &[EnumField],
        enum_decl: &EnumDeclaration,
        result: &mut SectionAnalysisResult,
    ) -> FxHashSet<CaseInsensitiveKey> {
        let mut seen = FxHashSet::default();
        let mut duplicates = FxHashSet::default();

        for field in fields {
            let key = CaseInsensitiveKey::new(field.name.clone());
            if !seen.insert(key.clone()) {
                duplicates.insert(key.clone());
                self.add_error(
                    result,
                    "ENUM003",
                    ERROR_DUPLICATE_FIELD_NAME,
                    &format!(
                        "Field '{}' is defined multiple times in enum '{}'",
                        key.as_str(), enum_decl.name
                    ),
                    &format!(
                        "Each field in '{}' must have a unique name",
                        enum_decl.name
                    ),
                    Some(field.position),
                );
            }
        }

        duplicates
    }

    fn validate_field_values(
        &mut self,
        fields: &[EnumField],
        dup_field_names: &FxHashSet<CaseInsensitiveKey>,
        enum_decl: &EnumDeclaration,
        result: &mut SectionAnalysisResult,
    ) {
        // Use FxHashMap instead of standard HashMap — 2x faster for small keys
        let mut seen_values: FxHashMap<i32, String> = FxHashMap::default();
        seen_values.reserve(fields.len());
        
        let mut implicit_value: i32 = 0;

        for field in fields {
            let key = CaseInsensitiveKey::new(field.name.clone());
            
            // Skip duplicate field names (already reported)
            if dup_field_names.contains(&key) {
                implicit_value = match implicit_value.checked_add(1) {
                    Some(v) => v,
                    None    => return,
                };
                continue;
            }

            // Validate field name
            if !Self::is_valid_identifier(&field.name) {
                self.add_error(
                    result,
                    "ENUM004",
                    ERROR_INVALID_FIELD_NAME,
                    &format!(
                        "Field name '{}' in enum '{}' is not a valid identifier",
                        field.name, enum_decl.name
                    ),
                    "Field names must start with a letter or underscore, then alphanumeric or underscore only",
                    Some(field.position),
                );
                implicit_value = match implicit_value.checked_add(1) {
                    Some(v) => v,
                    None    => return,
                };
                continue;
            }

            let actual = field.value.unwrap_or(implicit_value);

            // Check for duplicate values
            if let Some(conflict) = seen_values.get(&actual) {
                self.add_error(
                    result,
                    "ENUM005",
                    ERROR_DUPLICATE_FIELD_VALUE,
                    &format!(
                        "Field '{}' has value {} which is already used by '{}' in enum '{}'",
                        field.name, actual, conflict, enum_decl.name
                    ),
                    &format!(
                        "Assign a different value to '{}' in enum '{}'",
                        field.name, enum_decl.name
                    ),
                    Some(field.position),
                );
            } else {
                seen_values.insert(actual, field.name.clone());
            }

            implicit_value = match actual.checked_add(1) {
                Some(v) => v,
                None    => return,
            };
        }
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Symbol Table Population
    // ═════════════════════════════════════════════════════════════════════════

    fn populate_symbol_table(
        &mut self,
        section: &EnumsSection,
        symbol_table: &mut SymbolTable,
        duplicate_enums: &FxHashSet<CaseInsensitiveKey>,
        invalid_enums: &FxHashSet<CaseInsensitiveKey>,
    ) {
        let mut success_count: usize = 0;
        let mut skip_count:    usize = 0;

        for enum_decl in &section.enums {
            let key = CaseInsensitiveKey::new(enum_decl.name.clone());
            
            if duplicate_enums.contains(&key) || invalid_enums.contains(&key) {
                skip_count += 1;
                continue;
            }

            // Use FxHashMap for field mapping (symbol table still uses standard HashMap)
            let mut field_mapping: FxHashMap<String, i32> = FxHashMap::default();
            field_mapping.reserve(enum_decl.fields.len());
            
            let mut implicit_value: i32 = 0;

            for field in &enum_decl.fields {
                if !Self::is_valid_identifier(&field.name) {
                    implicit_value = match implicit_value.checked_add(1) {
                        Some(v) => v,
                        None    => break,
                    };
                    continue;
                }

                let actual = field.value.unwrap_or(implicit_value);
                field_mapping.insert(field.name.clone(), actual);

                implicit_value = match actual.checked_add(1) {
                    Some(v) => v,
                    None    => break,
                };
            }

            if field_mapping.is_empty() {
                skip_count += 1;
                continue;
            }

            // Convert FxHashMap to standard HashMap for symbol table
            let std_mapping: std::collections::HashMap<String, i32> = 
                field_mapping.into_iter().collect();
            
            symbol_table.add_enum(enum_decl.name.clone(), std_mapping);
            success_count += 1;
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Populated symbol table with {} enums ({} skipped)",
                success_count, skip_count
            ));
        }
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Helper Functions — All Inline for Performance
    // ═════════════════════════════════════════════════════════════════════════

    /// Validate identifier (starts with letter/underscore, then alphanumeric/underscore)
    #[inline]
    fn is_valid_identifier(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        let mut chars = name.chars();
        match chars.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
            _ => return false,
        }
        chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
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
            section_name: "ENUMS".to_string(),
            suggestion:   suggestion.to_string(),
            position,
        });

        let (line, col) = position.map(|p| (p.line as i32, p.column as i32)).unwrap_or((0, 0));
        self.error_manager.add_semantic_error(
            SemanticErrorType::DuplicateDefinition,
            message.to_string(),
            line, col,
            Some("ENUMS".to_string()),
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
            section_name: "ENUMS".to_string(),
            position,
        });
        if self.debug_config.is_enabled {
            self.error_manager.log_warning(message);
        }
    }
                                        }
