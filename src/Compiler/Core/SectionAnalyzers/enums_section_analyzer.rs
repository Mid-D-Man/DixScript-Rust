// src/Compiler/Core/SectionAnalyzers/enums_section_analyzer.rs

use crate::Compiler::AST::{EnumsSection, EnumDeclaration, EnumField, Position};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::ErrorManager::{ErrorManager, SemanticErrorType};
use std::collections::{HashMap, HashSet};

/// Result of analyzing the ENUMS section
#[derive(Debug, Clone)]
pub struct SectionAnalysisResult {
    pub section_name: String,
    pub is_success: bool,
    pub errors: Vec<SemanticErrorInfo>,
    pub warnings: Vec<SemanticWarningInfo>,
}

impl SectionAnalysisResult {
    pub fn new(section_name: impl Into<String>) -> Self {
        SectionAnalysisResult {
            section_name: section_name.into(),
            is_success: false,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

/// Semantic error information
#[derive(Debug, Clone)]
pub struct SemanticErrorInfo {
    pub error_id: String,
    pub error_type: String,
    pub message: String,
    pub section_name: String,
    pub suggestion: String,
    pub position: Option<Position>,
}

/// Semantic warning information
#[derive(Debug, Clone)]
pub struct SemanticWarningInfo {
    pub warning_id: String,
    pub message: String,
    pub section_name: String,
    pub position: Option<Position>,
}

/// EnumsSectionAnalyzer - validates ENUMS section and populates symbol table
///
/// Optimizations:
/// - Preallocates collections based on enum count
/// - Uses span-based identifier validation (zero allocation)
/// - Aggressive inlining on hot paths
/// - Borrowed references throughout (no cloning)
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
const ERROR_SYMBOL_TABLE_ERROR: &str = "SYMBOL_TABLE_ERROR";
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

        self.error_manager.create_scope("ENUMS Section Analysis");

        if self.error_manager.is_info_enabled() {
            self.error_manager.log_info(&format!(
                "Analyzing ENUMS section with {} enum definitions",
                enum_count
            ));
        }

        // Check version support
        if !self.check_version_support(&mut result) {
            self.error_manager.exit_scope();
            return result;
        }

        // Phase 1: Check for duplicate enum names globally
        if self.error_manager.is_debug_enabled() {
            self.error_manager.log_debug("Phase 1: Checking for duplicate enum names");
        }

        let duplicate_enums = self.check_duplicate_enums(&section.enums, &mut result);

        if self.should_halt(&result) {
            self.error_manager.exit_scope();
            return result;
        }

        // Phase 2: Validate each enum declaration
        if self.error_manager.is_debug_enabled() {
            self.error_manager.log_debug("Phase 2: Validating individual enum declarations");
        }

        for enum_decl in &section.enums {
            // Skip validation of duplicate enums (already reported)
            if duplicate_enums.contains(&enum_decl.name.to_lowercase()) {
                if self.error_manager.is_warning_enabled() {
                    self.error_manager.log_warning(&format!(
                        "Skipping validation of duplicate enum '{}'",
                        enum_decl.name
                    ));
                }
                continue;
            }

            self.validate_enum_declaration(enum_decl, &mut result);

            if self.should_halt(&result) {
                self.error_manager.exit_scope();
                return result;
            }
        }

        // Phase 3: Populate symbol table with valid enums
        if self.error_manager.is_debug_enabled() {
            self.error_manager.log_debug("Phase 3: Populating symbol table with enum definitions");
        }

        self.populate_symbol_table(section, symbol_table, &duplicate_enums, &mut result);

        // Determine overall success
        result.is_success = result.errors.is_empty();

        if self.error_manager.is_info_enabled() {
            let status = if result.is_success { "SUCCESS" } else { "FAILURE" };
            self.error_manager.log_info(&format!("ENUMS analysis complete: {}", status));
            self.error_manager.log_info(&format!(
                "  Enums validated: {}",
                enum_count - duplicate_enums.len()
            ));
            self.error_manager.log_info(&format!(
                "  Errors: {}, Warnings: {}",
                result.errors.len(),
                result.warnings.len()
            ));
        }

        self.error_manager.exit_scope();
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

    /// Check for duplicate enum names (case-insensitive)
    fn check_duplicate_enums(
        &mut self,
        enums: &[EnumDeclaration],
        result: &mut SectionAnalysisResult,
    ) -> HashSet<String> {
        let enum_count = enums.len();
        let mut enum_names = HashSet::with_capacity(enum_count);
        let mut duplicates = HashSet::new();

        for enum_decl in enums {
            let name_lower = enum_decl.name.to_lowercase();

            if !enum_names.insert(name_lower.clone()) {
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
    ) {
        if self.error_manager.is_debug_enabled() {
            self.error_manager.log_debug(&format!("Validating enum: {}", enum_decl.name));
        }

        // Check enum name is valid identifier
        if !Self::is_valid_identifier(&enum_decl.name) {
            self.add_error(
                result,
                "ENUM002",
                ERROR_INVALID_ENUM_NAME,
                &format!("Enum name '{}' is not a valid identifier", enum_decl.name),
                "Enum names must start with a letter and contain only alphanumeric characters and underscores",
                Some(enum_decl.position),
            );
            return;
        }

        // Check for empty field list
        let field_count = enum_decl.fields.len();
        if field_count == 0 {
            self.add_warning(
                result,
                WARNING_EMPTY_ENUM_FIELDS,
                &format!("Enum '{}' has no fields defined", enum_decl.name),
                Some(enum_decl.position),
            );
        }

        // Phase 2a: Check for duplicate field names
        if self.error_manager.is_debug_enabled() {
            self.error_manager.log_debug(&format!(
                "  Checking for duplicate field names in enum '{}'",
                enum_decl.name
            ));
        }

        let duplicate_field_names = self.check_duplicate_fields(&enum_decl.fields, enum_decl, result);

        // Phase 2b: Check for duplicate field values and validate field names
        if self.error_manager.is_debug_enabled() {
            self.error_manager.log_debug(&format!(
                "  Checking for duplicate field values and validating field names in enum '{}'",
                enum_decl.name
            ));
        }

        self.validate_field_values(&enum_decl.fields, &duplicate_field_names, enum_decl, result);

        if self.error_manager.is_debug_enabled() {
            self.error_manager.log_debug(&format!("Enum '{}' validation complete", enum_decl.name));
        }
    }

    /// Check for duplicate field names within an enum
    fn check_duplicate_fields(
        &mut self,
        fields: &[EnumField],
        enum_decl: &EnumDeclaration,
        result: &mut SectionAnalysisResult,
    ) -> HashSet<String> {
        let field_count = fields.len();
        let mut seen_field_names = HashSet::with_capacity(field_count);
        let mut duplicate_field_names = HashSet::new();

        for field in fields {
            let name_lower = field.name.to_lowercase();

            if !seen_field_names.insert(name_lower.clone()) {
                duplicate_field_names.insert(name_lower);

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

        duplicate_field_names
    }

    /// Validate field values and names
    fn validate_field_values(
        &mut self,
        fields: &[EnumField],
        duplicate_field_names: &HashSet<String>,
        enum_decl: &EnumDeclaration,
        result: &mut SectionAnalysisResult,
    ) {
        let mut seen_field_values: HashMap<i32, String> = HashMap::with_capacity(fields.len());
        let mut implicit_value = 0;

        for field in fields {
            let name_lower = field.name.to_lowercase();

            // Skip validation of duplicate field names (already reported)
            if duplicate_field_names.contains(&name_lower) {
                if self.error_manager.is_warning_enabled() {
                    self.error_manager.log_warning(&format!(
                        "    Skipping validation of duplicate field '{}' in enum '{}'",
                        field.name, enum_decl.name
                    ));
                }
                implicit_value += 1;
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
                    "Field names must start with a letter and contain only alphanumeric characters and underscores",
                    Some(field.position),
                );

                implicit_value += 1;
                continue;
            }

            // Determine actual field value
            let actual_value = field.value.unwrap_or(implicit_value);

            if self.error_manager.is_debug_enabled() {
                let value_type = if field.value.is_some() { "explicit" } else { "implicit" };
                self.error_manager.log_debug(&format!(
                    "    Field '{}' has {} value: {}",
                    field.name, value_type, actual_value
                ));
            }

            // Check for duplicate values
            if let Some(conflicting_field) = seen_field_values.get(&actual_value) {
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
                seen_field_values.insert(actual_value, field.name.clone());
            }

            // Update implicit counter for next field
            implicit_value = actual_value + 1;
        }
    }

    // ==================== SYMBOL TABLE POPULATION ====================

    /// Populate symbol table with valid enum definitions
    fn populate_symbol_table(
        &mut self,
        section: &EnumsSection,
        symbol_table: &mut SymbolTable,
        duplicate_enums: &HashSet<String>,
        result: &mut SectionAnalysisResult,
    ) {
        let mut success_count = 0;
        let mut skip_count = 0;

        for enum_decl in &section.enums {
            let name_lower = enum_decl.name.to_lowercase();

            // Skip duplicate enums
            if duplicate_enums.contains(&name_lower) {
                if self.error_manager.is_debug_enabled() {
                    self.error_manager.log_debug(&format!(
                        "Skipping duplicate enum '{}' for symbol table",
                        enum_decl.name
                    ));
                }
                skip_count += 1;
                continue;
            }

            // Skip if enum has no valid fields
            let valid_fields = Self::count_valid_fields(&enum_decl.fields);

            if valid_fields == 0 {
                if self.error_manager.is_warning_enabled() {
                    self.error_manager.log_warning(&format!(
                        "Skipping enum '{}' - no valid fields to register",
                        enum_decl.name
                    ));
                }
                skip_count += 1;
                continue;
            }

            // Build field mapping with computed values
            let mut field_mapping = HashMap::with_capacity(valid_fields);
            let mut implicit_value = 0;

            for field in &enum_decl.fields {
                if !Self::is_valid_identifier(&field.name) {
                    implicit_value += 1;
                    continue;
                }

                let actual_value = field.value.unwrap_or(implicit_value);
                field_mapping.insert(field.name.clone(), actual_value);
                implicit_value = actual_value + 1;

                if self.error_manager.is_debug_enabled() {
                    self.error_manager.log_debug(&format!(
                        "  Mapped field '{}.{}' = {}",
                        enum_decl.name, field.name, actual_value
                    ));
                }
            }

            // Add to symbol table
            symbol_table.add_enum(enum_decl.name.clone(), field_mapping.clone());
            success_count += 1;

            if self.error_manager.is_info_enabled() {
                self.error_manager.log_info(&format!(
                    "Added enum '{}' to symbol table with {} fields",
                    enum_decl.name,
                    field_mapping.len()
                ));
            }
        }

        if self.error_manager.is_info_enabled() {
            self.error_manager.log_info(&format!(
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

    /// Check if analysis should halt due to errors
    #[inline]
    fn should_halt(&self, result: &SectionAnalysisResult) -> bool {
        !result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
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

        // Also add to ErrorManager
        let line = position.map(|p| p.line as i32).unwrap_or(0);
        let column = position.map(|p| p.column as i32).unwrap_or(0);

        self.error_manager.add_semantic_error(
            SemanticErrorType::DuplicateDefinition,
            message.to_string(),
            Some(line),
            Some(column),
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

        if self.error_manager.is_warning_enabled() {
            self.error_manager.log_warning(message);
        }
    }
  }
