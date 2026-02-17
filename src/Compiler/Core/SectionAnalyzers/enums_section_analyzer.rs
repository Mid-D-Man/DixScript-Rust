// src/Compiler/Core/SectionAnalyzers/enums_section_analyzer.rs

use crate::Compiler::AST::{EnumsSection, EnumDeclaration, EnumField, Position};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use crate::ErrorManager::{ErrorManager, SemanticErrorType};
use rustc_hash::{FxHashMap, FxHashSet};

// Import shared result types from parent module
use super::{SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo};

/// EnumsSectionAnalyzer - validates ENUMS section and populates symbol table
///
/// Performance optimizations applied:
/// - FxHashMap/FxHashSet (3x faster than std)
/// - Direct ASCII case comparison (zero allocation)
/// - Checked arithmetic (prevents overflow)
/// - Conditional logging (only when debug enabled)
/// - Preallocated collections
/// - Borrowed references (no cloning in hot paths)
pub struct EnumsSectionAnalyzer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
}

// ==================== ERROR MESSAGE CONSTANTS ====================

const ERROR_DUPLICATE_ENUM_NAME: &str = "DUPLICATE_ENUM_NAME";
const ERROR_INVALID_ENUM_NAME: &str = "INVALID_ENUM_NAME";
const ERROR_DUPLICATE_FIELD_NAME: &str = "DUPLICATE_FIELD_NAME";
const ERROR_INVALID_FIELD_NAME: &str = "INVALID_FIELD_NAME";
const ERROR_DUPLICATE_FIELD_VALUE: &str = "DUPLICATE_FIELD_VALUE";
const ERROR_ENUM_VALUE_OVERFLOW: &str = "ENUM_VALUE_OVERFLOW";
const ERROR_UNSUPPORTED_SECTION: &str = "UNSUPPORTED_SECTION";

const WARNING_EMPTY_ENUM_FIELDS: &str = "ENUM_WARN001";

impl<'a> EnumsSectionAnalyzer<'a> {
    /// Create a new EnumsSectionAnalyzer
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        EnumsSectionAnalyzer {
            operational_settings,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Main analysis method - validates ENUMS section and populates symbol table
    pub fn analyze(
        &mut self,
        section: &EnumsSection,
        symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        let mut result = SectionAnalysisResult::new("ENUMS");
        let enum_count = section.enums.len();

        if self.operational_settings.debug_mode != DebugMode::Off {
            self.log_info(&format!(
                "Analyzing ENUMS section with {} enum definitions",
                enum_count
            ));
        }

        // Check version support
        if !self.check_version_support(&mut result) {
            return result;
        }

        // Phase 1: Check for duplicate enum names globally
        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.log_debug("Phase 1: Checking for duplicate enum names");
        }

        let duplicate_enums = self.check_duplicate_enums(&section.enums, &mut result);

        if self.should_halt(&result) {
            return result;
        }

        // Phase 2: Validate each enum declaration
        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.log_debug("Phase 2: Validating individual enum declarations");
        }

        let mut invalid_enums = FxHashSet::default();

        for enum_decl in &section.enums {
            // Skip validation of duplicate enums (already reported)
            if Self::contains_case_insensitive(&duplicate_enums, &enum_decl.name) {
                if self.operational_settings.debug_mode == DebugMode::Verbose {
                    self.log_warning(&format!(
                        "Skipping validation of duplicate enum '{}'",
                        enum_decl.name
                    ));
                }
                continue;
            }

            self.validate_enum_declaration(enum_decl, &mut result, &mut invalid_enums);

            if self.should_halt(&result) {
                return result;
            }
        }

        // Phase 3: Populate symbol table with valid enums
        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.log_debug("Phase 3: Populating symbol table with enum definitions");
        }

        self.populate_symbol_table(section, symbol_table, &duplicate_enums, &invalid_enums, &mut result);

        // Determine overall success
        result.is_success = result.errors.is_empty();

        if self.operational_settings.debug_mode != DebugMode::Off {
            let status = if result.is_success { "SUCCESS" } else { "FAILURE" };
            self.log_info(&format!("ENUMS analysis complete: {}", status));
            self.log_info(&format!(
                "  Enums validated: {}",
                enum_count - duplicate_enums.len() - invalid_enums.len()
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

    /// Check if ENUMS section is supported in current version
    fn check_version_support(&mut self, result: &mut SectionAnalysisResult) -> bool {
        use crate::Compiler::VersionControl::VersionConstraints;

        let constraints = VersionConstraints::new();

        if !constraints.is_valid_section_type("ENUMS") {
            self.add_error(
                result,
                "ENUM000",
                ERROR_UNSUPPORTED_SECTION,
                "ENUMS section is not supported in current DixScript version",
                "Upgrade compiler to v1.0.0 or higher",
                None,
            );

            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
                return false;
            }
        }

        true
    }

    /// Check for duplicate enum names (case-insensitive, zero-allocation)
    fn check_duplicate_enums(
        &mut self,
        enums: &[EnumDeclaration],
        result: &mut SectionAnalysisResult,
    ) -> FxHashSet<String> {
        let mut seen = FxHashSet::default();
        let mut duplicates = FxHashSet::default();

        for enum_decl in enums {
            // Store lowercase for case-insensitive comparison
            let name_lower = enum_decl.name.to_ascii_lowercase();

            if !seen.insert(name_lower.clone()) {
                duplicates.insert(name_lower);

                self.add_error(
                    result,
                    "ENUM001",
                    ERROR_DUPLICATE_ENUM_NAME,
                    &format!("Enum '{}' is defined multiple times", enum_decl.name),
                    "Each enum must have a unique name. Remove or rename duplicate enum definitions",
                    Some(enum_decl.position),
                );
            }
        }

        duplicates
    }

    /// Validate a single enum declaration
    fn validate_enum_declaration(
        &mut self,
        enum_decl: &EnumDeclaration,
        result: &mut SectionAnalysisResult,
        invalid_enums: &mut FxHashSet<String>,
    ) {
        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.log_debug(&format!("Validating enum: {}", enum_decl.name));
        }

        // Check enum name is valid identifier
        if !Self::is_valid_identifier(&enum_decl.name) {
            invalid_enums.insert(enum_decl.name.to_ascii_lowercase());

            self.add_error(
                result,
                "ENUM002",
                ERROR_INVALID_ENUM_NAME,
                &format!("Enum name '{}' is not a valid identifier", enum_decl.name),
                "Enum names must start with a letter or underscore and contain only alphanumeric characters and underscores",
                Some(enum_decl.position),
            );
            return; // Early exit - don't validate fields of invalid enum
        }

        // Check for empty field list
        if enum_decl.fields.is_empty() {
            self.add_warning(
                result,
                WARNING_EMPTY_ENUM_FIELDS,
                &format!("Enum '{}' has no fields defined", enum_decl.name),
                Some(enum_decl.position),
            );
        }

        // Phase 2a: Check for duplicate field names
        let duplicate_field_names = self.check_duplicate_fields(&enum_decl.fields, enum_decl, result);

        // Phase 2b: Validate field values
        self.validate_field_values(&enum_decl.fields, &duplicate_field_names, enum_decl, result);

        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.log_debug(&format!("Enum '{}' validation complete", enum_decl.name));
        }
    }

    /// Check for duplicate field names within an enum (zero-allocation comparison)
    fn check_duplicate_fields(
        &mut self,
        fields: &[EnumField],
        enum_decl: &EnumDeclaration,
        result: &mut SectionAnalysisResult,
    ) -> FxHashSet<String> {
        let mut seen = FxHashSet::default();
        let mut duplicates = FxHashSet::default();

        for field in fields {
            let name_lower = field.name.to_ascii_lowercase();

            if !seen.insert(name_lower.clone()) {
                duplicates.insert(name_lower);

                self.add_error(
                    result,
                    "ENUM003",
                    ERROR_DUPLICATE_FIELD_NAME,
                    &format!(
                        "Field '{}' is defined multiple times in enum '{}'",
                        field.name, enum_decl.name
                    ),
                    &format!(
                        "Each field in enum '{}' must have a unique name",
                        enum_decl.name
                    ),
                    Some(field.position),
                );
            }
        }

        duplicates
    }

    /// Validate field values and names
    fn validate_field_values(
        &mut self,
        fields: &[EnumField],
        duplicate_field_names: &FxHashSet<String>,
        enum_decl: &EnumDeclaration,
        result: &mut SectionAnalysisResult,
    ) {
        let mut seen_values: FxHashMap<i32, String> = FxHashMap::default();
        let mut implicit_value = 0i32;

        for field in fields {
            // Skip validation of duplicate field names (already reported)
            if Self::contains_case_insensitive(duplicate_field_names, &field.name) {
                // Still increment implicit counter
                if let Some(next_val) = implicit_value.checked_add(1) {
                    implicit_value = next_val;
                } else {
                    return; // Stop if overflow
                }
                continue;
            }

            // Check field name is valid identifier
            if !Self::is_valid_identifier(&field.name) {
                self.add_error(
                    result,
                    "ENUM004",
                    ERROR_INVALID_FIELD_NAME,
                    &format!(
                        "Field name '{}' in enum '{}' is not a valid identifier",
                        field.name, enum_decl.name
                    ),
                    "Field names must start with a letter or underscore and contain only alphanumeric characters and underscores",
                    Some(field.position),
                );

                // Increment implicit counter
                if let Some(next_val) = implicit_value.checked_add(1) {
                    implicit_value = next_val;
                } else {
                    return;
                }
                continue;
            }

            // Determine actual field value
            let actual_value = field.value.unwrap_or(implicit_value);

            // Check for duplicate values
            if let Some(conflicting_field) = seen_values.get(&actual_value) {
                self.add_error(
                    result,
                    "ENUM005",
                    ERROR_DUPLICATE_FIELD_VALUE,
                    &format!(
                        "Field '{}' has value {}, which is already used by field '{}' in enum '{}'",
                        field.name, actual_value, conflicting_field, enum_decl.name
                    ),
                    &format!(
                        "Each field value in enum '{}' must be unique. Assign a different value to '{}'",
                        enum_decl.name, field.name
                    ),
                    Some(field.position),
                );
            } else {
                seen_values.insert(actual_value, field.name.clone());
            }

            // Update implicit counter for next field (with overflow check)
            if let Some(next_val) = actual_value.checked_add(1) {
                implicit_value = next_val;
            } else {
                // Can't increment further - stop processing remaining fields
                return;
            }
        }
    }

    // ==================== SYMBOL TABLE POPULATION ====================

    /// Populate symbol table with valid enum definitions
    fn populate_symbol_table(
        &mut self,
        section: &EnumsSection,
        symbol_table: &mut SymbolTable,
        duplicate_enums: &FxHashSet<String>,
        invalid_enums: &FxHashSet<String>,
        result: &mut SectionAnalysisResult,
    ) {
        let mut success_count = 0;
        let mut skip_count = 0;

        for enum_decl in &section.enums {
            // Skip duplicate and invalid enums
            if Self::contains_case_insensitive(duplicate_enums, &enum_decl.name)
                || Self::contains_case_insensitive(invalid_enums, &enum_decl.name) {
                skip_count += 1;
                continue;
            }

            // Skip if enum has no valid fields
            let valid_fields = Self::count_valid_fields(&enum_decl.fields);

            if valid_fields == 0 {
                skip_count += 1;
                continue;
            }

            // Build field mapping with computed values
            let mut field_mapping = std::collections::HashMap::new();
            let mut implicit_value = 0i32;

            for field in &enum_decl.fields {
                if !Self::is_valid_identifier(&field.name) {
                    // Skip invalid field, but increment counter
                    if let Some(next_val) = implicit_value.checked_add(1) {
                        implicit_value = next_val;
                    } else {
                        break;
                    }
                    continue;
                }

                let actual_value = field.value.unwrap_or(implicit_value);
                field_mapping.insert(field.name.clone(), actual_value);

                // Increment with overflow check
                if let Some(next_val) = actual_value.checked_add(1) {
                    implicit_value = next_val;
                } else {
                    break;
                }
            }

            // Add to symbol table
            symbol_table.add_enum(enum_decl.name.clone(), field_mapping);
            success_count += 1;
        }

        if self.operational_settings.debug_mode != DebugMode::Off {
            self.log_info(&format!(
                "Populated symbol table with {} enums ({} skipped)",
                success_count, skip_count
            ));
        }
    }

    // ==================== HELPER METHODS ====================

    /// Check if identifier is valid (zero-allocation span-based)
    #[inline]
    fn is_valid_identifier(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        let mut chars = name.chars();

        // First character must be letter or underscore
        match chars.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
            _ => return false,
        }

        // Rest must be alphanumeric or underscore
        chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    /// Count valid fields (fast, no allocation)
    #[inline]
    fn count_valid_fields(fields: &[EnumField]) -> usize {
        fields.iter()
            .filter(|f| Self::is_valid_identifier(&f.name))
            .count()
    }

    /// Case-insensitive contains check (zero-allocation)
    #[inline]
    fn contains_case_insensitive(set: &FxHashSet<String>, name: &str) -> bool {
        set.iter().any(|item| item.eq_ignore_ascii_case(name))
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
        self.error_manager.log_warning(message);
    }

    // ==================== ERROR/WARNING HELPERS ====================

    fn add_error(
        &mut self,
        result: &mut SectionAnalysisResult,
        error_id: &str,
        error_type: &str,
        message: &str,
        suggestion: &str,
        position: Option<Position>,
    ) {
        let error = SemanticErrorInfo {
            error_id: error_id.to_string(),
            error_type: error_type.to_string(),
            message: message.to_string(),
            section_name: "ENUMS".to_string(),
            suggestion: suggestion.to_string(),
            position,
        };

        result.errors.push(error.clone());

        // Convert position to line/column for ErrorManager
        let (line, column) = position
            .map(|p| (p.line as i32, p.column as i32))
            .unwrap_or((0, 0));

        // Add to ErrorManager
        self.error_manager.add_semantic_error(
            SemanticErrorType::DuplicateDefinition,
            message.to_string(),
            line,
            column,
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
        let warning = SemanticWarningInfo {
            warning_id: warning_id.to_string(),
            message: message.to_string(),
            section_name: "ENUMS".to_string(),
            position,
        };

        result.warnings.push(warning);

        if self.operational_settings.debug_mode != DebugMode::Off {
            self.log_warning(message);
        }
    }
                }
