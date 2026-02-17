// src/Compiler/Core/SectionAnalyzers/data_section_analyzer.rs

use crate::Compiler::AST::{
    DataSection, DataEntry, TablePath, PropertyAssignment, Value,
    Position, DataType,
};
use crate::Compiler::AST::Visitors::TypeInferenceVisitor;
use crate::Compiler::Utilities::{SymbolTable, VariableInfo, PathBuilder};
use crate::Compiler::Core::{OperationalSettings, DebugMode};
use crate::ErrorManager::ErrorManager;
use crate::Utilities::Keywords;
use rustc_hash::{FxHashMap, FxHashSet};
use regex::Regex;
use base64::{Engine as _, engine::general_purpose};

use super::{SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo};

// ==================== ERROR MESSAGE CONSTANTS (STRING POOLING) ====================

const ERROR_ORDERING_VIOLATION: &str = "ORDERING_VIOLATION";
const ERROR_DUPLICATE_TABLE_PATH: &str = "DUPLICATE_TABLE_PATH";
const ERROR_DUPLICATE_GROUP_ARRAY_PATH: &str = "DUPLICATE_GROUP_ARRAY_PATH";
const ERROR_RESERVED_KEYWORD: &str = "RESERVED_KEYWORD";
const ERROR_TYPE_MISMATCH: &str = "TYPE_MISMATCH";
const ERROR_NULL_VALUE: &str = "NULL_VALUE";
const ERROR_NESTING_TOO_DEEP: &str = "NESTING_TOO_DEEP";
const ERROR_ARRAY_NOT_HOMOGENEOUS: &str = "ARRAY_NOT_HOMOGENEOUS";
const ERROR_DUPLICATE_PROPERTY: &str = "DUPLICATE_PROPERTY";
const ERROR_TUPLE_TOO_LARGE: &str = "TUPLE_TOO_LARGE";
const ERROR_ENUM_NOT_FOUND: &str = "ENUM_NOT_FOUND";
const ERROR_ENUM_VALUE_NOT_FOUND: &str = "ENUM_VALUE_NOT_FOUND";
const ERROR_FUNCTION_NOT_FOUND: &str = "FUNCTION_NOT_FOUND";
const ERROR_PARAMETER_COUNT_MISMATCH: &str = "PARAMETER_COUNT_MISMATCH";
const ERROR_PARAMETER_TYPE_MISMATCH: &str = "PARAMETER_TYPE_MISMATCH";
const ERROR_SCOPE_VIOLATION: &str = "SCOPE_VIOLATION";
const ERROR_NAMESPACE_NOT_FOUND: &str = "NAMESPACE_NOT_FOUND";
const ERROR_IMPORTED_FUNCTION_NOT_FOUND: &str = "IMPORTED_FUNCTION_NOT_FOUND";
const ERROR_IMPORTED_ENUM_NOT_FOUND: &str = "IMPORTED_ENUM_NOT_FOUND";
const ERROR_INVALID_EXPRESSION: &str = "INVALID_EXPRESSION";
const ERROR_UNDEFINED_VARIABLE: &str = "UNDEFINED_VARIABLE";
const ERROR_INVALID_BLOB_CONTENT: &str = "INVALID_BLOB_CONTENT";
const ERROR_INVALID_REGEX_PATTERN: &str = "INVALID_REGEX_PATTERN";
const ERROR_CALL_DEPTH_EXCEEDED: &str = "CALL_DEPTH_EXCEEDED";
const ERROR_TOO_MANY_PARAMETERS: &str = "TOO_MANY_PARAMETERS";

// ==================== LIMITS ====================

const MAX_NESTING_DEPTH: usize = 5;
const MAX_TUPLE_ELEMENTS: usize = 6;
const MAX_FUNCTION_PARAMS: usize = 100;
const MAX_CALL_DEPTH: usize = 10;

// ==================== DATA SECTION ANALYZER ====================

/// DATA Section Semantic Analyzer v1.0.0 - COMPREHENSIVE VALIDATION
pub struct DataSectionAnalyzer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,

    // Validation state (temporary - owned during analysis)
    declared_table_paths: FxHashSet<String>,
    current_nesting_depth: usize,
    current_call_depth: usize,

    // Indexes (built during analysis, returned to caller)
    short_name_to_full_paths: FxHashMap<String, Vec<String>>,
    path_to_type: FxHashMap<String, DataType>,
}

impl<'a> DataSectionAnalyzer<'a> {
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        DataSectionAnalyzer {
            operational_settings,
            error_manager: ErrorManager::get_shared_instance(),
            declared_table_paths: FxHashSet::default(),
            current_nesting_depth: 0,
            current_call_depth: 0,
            short_name_to_full_paths: FxHashMap::default(),
            path_to_type: FxHashMap::default(),
        }
    }

    pub fn analyze(
        &mut self,
        section: &DataSection,
        symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        let mut result = SectionAnalysisResult::new("DATA");
        let entry_count = section.entries.len();

        // Preallocate collections
        self.declared_table_paths = FxHashSet::with_capacity_and_hasher(
            entry_count / 2,
            Default::default()
        );
        self.current_nesting_depth = 0;
        self.current_call_depth = 0;

        // Check debug mode ONCE
        let is_info = self.operational_settings.debug_mode != DebugMode::Off;
        let is_debug = self.operational_settings.debug_mode == DebugMode::Verbose;

        if is_info {
            self.error_manager.log_info(&format!(
                "Analyzing DATA section with {} entries",
                entry_count
            ));
        }

        // Phase 1: Validate two-tier ordering
        if is_debug {
            self.error_manager.log_debug("Phase 1: Validating two-tier ordering system");
        }
        self.validate_two_tier_ordering(section, &mut result);

        // Phase 2: Validate table path uniqueness
        if is_debug {
            self.error_manager.log_debug("Phase 2: Validating table path uniqueness");
        }
        self.validate_table_path_uniqueness(section, &mut result);

        // Phase 3: Validate individual entries AND build indexes
        if is_debug {
            self.error_manager.log_debug(
                "Phase 3: Validating individual data entries and building indexes"
            );
        }

        for entry in &section.entries {
            self.validate_data_entry(
                entry,
                symbol_table,
                &mut result,
                is_debug
            );
        }

        result.is_success = result.errors.is_empty();

        if is_info {
            let status = if result.is_success { "SUCCESS" } else { "FAILED" };
            self.error_manager.log_info(&format!("DATA analysis complete: {}", status));
            self.error_manager.log_info(&format!("  Entries validated: {}", entry_count));
            self.error_manager.log_info(&format!(
                "  Indexes built: {} short names, {} type mappings",
                self.short_name_to_full_paths.len(),
                self.path_to_type.len()
            ));
            self.error_manager.log_info(&format!(
                "  Errors: {}, Warnings: {}",
                result.errors.len(),
                result.warnings.len()
            ));
        }

        result
    }

    /// Get the built indexes (short name → full paths, path → type)
    #[inline]
    pub fn get_indexes(
        &self
    ) -> (&FxHashMap<String, Vec<String>>, &FxHashMap<String, DataType>) {
        (&self.short_name_to_full_paths, &self.path_to_type)
    }

    // ==================== PHASE 1: TWO-TIER ORDERING VALIDATION ====================

    fn validate_two_tier_ordering(
        &self,
        section: &DataSection,
        result: &mut SectionAnalysisResult,
    ) {
        let mut has_seen_grouped_data = false;
        let mut flat_props_count = 0;
        let mut grouped_data_count = 0;

        for entry in &section.entries {
            match entry {
                DataEntry::SimpleProperty { name, position, .. } => {
                    flat_props_count += 1;

                    if has_seen_grouped_data {
                        self.add_error(
                            result,
                            ERROR_ORDERING_VIOLATION,
                            &format!(
                                "Flat property '{}' appears after grouped data. \
                                All flat properties must come before table properties and group arrays.",
                                name
                            ),
                            *position,
                            Some("Move this property before any table properties (path:) or group arrays (path::)")
                        );
                    }
                }
                DataEntry::TableProperty { .. } | DataEntry::GroupArray { .. } => {
                    grouped_data_count += 1;
                    has_seen_grouped_data = true;
                }
                DataEntry::ObjectProperty { .. } => {
                    flat_props_count += 1;

                    if has_seen_grouped_data {
                        // Object properties are treated like simple properties
                        // (they can cause ordering violations too)
                    }
                }
            }
        }

        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.error_manager.log_debug(&format!(
                "Two-tier ordering validation complete: Flat → Grouped"
            ));
            self.error_manager.log_debug(&format!("  Flat properties: {}", flat_props_count));
            self.error_manager.log_debug(&format!("  Grouped data entries: {}", grouped_data_count));
        }
    }

    // ==================== PHASE 2: TABLE PATH UNIQUENESS ====================

    fn validate_table_path_uniqueness(
        &mut self,
        section: &DataSection,
        result: &mut SectionAnalysisResult,
    ) {
        let estimated_tables = section.entries.len() / 3;
        let mut table_property_paths = FxHashSet::with_capacity_and_hasher(
            estimated_tables,
            Default::default()
        );
        let mut group_array_paths = FxHashSet::with_capacity_and_hasher(
            estimated_tables,
            Default::default()
        );

        for entry in &section.entries {
            match entry {
                DataEntry::TableProperty { path, position, .. } => {
                    let path_str = Self::join_table_path(&path.segments);

                    if !table_property_paths.insert(path_str.clone()) {
                        self.add_error(
                            result,
                            ERROR_DUPLICATE_TABLE_PATH,
                            &format!("Table property path '{}' is defined multiple times", path_str),
                            *position,
                            Some("Combine assignments into a single table property or use different paths")
                        );
                    } else {
                        self.declared_table_paths.insert(path_str);
                    }
                }
                DataEntry::GroupArray { path, position, .. } => {
                    let path_str = Self::join_table_path(&path.segments);

                    if !group_array_paths.insert(path_str.clone()) {
                        self.add_error(
                            result,
                            ERROR_DUPLICATE_GROUP_ARRAY_PATH,
                            &format!("Group array path '{}' is defined multiple times", path_str),
                            *position,
                            Some("Combine items into a single group array or use different paths")
                        );
                    } else {
                        self.declared_table_paths.insert(path_str);
                    }
                }
                _ => {}
            }
        }

        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.error_manager.log_debug(&format!(
                "Table path uniqueness validated: {} unique paths",
                table_property_paths.len() + group_array_paths.len()
            ));
        }
    }

    // ==================== PHASE 3: ENTRY VALIDATION + INDEX BUILDING ====================

    #[inline]
    fn validate_data_entry(
        &mut self,
        entry: &DataEntry,
        symbol_table: &mut SymbolTable,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) {
        match entry {
            DataEntry::SimpleProperty { name, data_type, value, position } => {
                if is_debug {
                    self.error_manager.log_debug(&format!("  Validating simple property: {}", name));
                }
                self.validate_simple_property(
                    name,
                    *data_type,
                    value,
                    *position,
                    symbol_table,
                    result,
                    is_debug
                );
            }
            DataEntry::TableProperty { path, properties, position } => {
                if is_debug {
                    let path_str = Self::join_table_path(&path.segments);
                    self.error_manager.log_debug(&format!("  Validating table property: {}", path_str));
                }
                self.validate_table_property(
                    path,
                    properties,
                    *position,
                    symbol_table,
                    result,
                    is_debug
                );
            }
            DataEntry::GroupArray { path, items, position } => {
                if is_debug {
                    let path_str = Self::join_table_path(&path.segments);
                    self.error_manager.log_debug(&format!("  Validating group array: {}", path_str));
                }
                self.validate_group_array(
                    path,
                    items,
                    *position,
                    symbol_table,
                    result,
                    is_debug
                );
            }
            DataEntry::ObjectProperty { name, data_type, object, position } => {
                if is_debug {
                    self.error_manager.log_debug(&format!("  Validating object property: {}", name));
                }
                self.validate_object_property(
                    name,
                    *data_type,
                    object.as_ref(),
                    *position,
                    symbol_table,
                    result,
                    is_debug
                );
            }
        }
    }

    fn validate_simple_property(
        &mut self,
        name: &str,
        declared_type: Option<DataType>,
        value: &Value,
        position: Position,
        symbol_table: &mut SymbolTable,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) {
        // Check reserved keywords
        if Keywords::is_reserved_in_context(name, "DATA") {
            self.add_error(
                result,
                ERROR_RESERVED_KEYWORD,
                &Keywords::get_keyword_usage_error(name, "DATA"),
                position,
                Some(&format!("Choose a different name for property '{}'", name))
            );
            return;
        }

        // Validate type annotation if present
        if let Some(dt) = declared_type {
            self.validate_type_annotation(dt, name, is_debug);
        }

        // Validate value and infer type (create TypeInferenceVisitor locally)
        let context = format!("property '{}'", name);
        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
        let inferred_type = self.validate_value(
            value,
            &context,
            symbol_table,
            &type_inference_visitor,
            result,
            is_debug
        );

        // Build index IMMEDIATELY
        let full_path = PathBuilder::build(&[name]);

        self.short_name_to_full_paths
            .entry(name.to_string())
            .or_insert_with(Vec::new)
            .push(full_path.clone());

        if let Some(inf_type) = inferred_type {
            self.path_to_type.insert(full_path.clone(), inf_type);
        } else if let Some(decl_type) = declared_type {
            self.path_to_type.insert(full_path.clone(), decl_type);
        }

        if is_debug {
            self.error_manager.log_debug(&format!(
                "  Indexed flat property: {} -> {} ({:?})",
                name, full_path, inferred_type.or(declared_type)
            ));
        }

        // Type compatibility check
        if let (Some(decl), Some(mut inf)) = (declared_type, inferred_type) {
            // Float conversion
            if decl == DataType::Float && inf != DataType::Float {
                if inf == DataType::Int || inf == DataType::Double {
                    if is_debug {
                        self.error_manager.log_debug(&format!(
                            "  Auto-converting {:?} to <float> for property '{}'",
                            inf, name
                        ));
                    }
                    inf = DataType::Float;
                    self.path_to_type.insert(full_path.clone(), DataType::Float);
                }
            }

            if !Self::is_type_compatible(decl, inf) {
                self.add_error(
                    result,
                    ERROR_TYPE_MISMATCH,
                    &format!(
                        "Property '{}' declared as <{:?}> but value is {:?}",
                        name, decl, inf
                    ),
                    position,
                    Some(&format!(
                        "Change type annotation to <{:?}> or provide a compatible value",
                        inf
                    ))
                );
            }
        }

        // Add to symbol table
        symbol_table.add_data_variable(name.to_string(), VariableInfo {
            name: name.to_string(),
            declared_type,
            inferred_type,
            is_inferred: declared_type.is_none(),
            scope: "global".to_string(),
            line: position.line as i32,
            column: position.column as i32,
        });

        if is_debug {
            self.error_manager.log_debug(&format!(
                "  Added to symbol table: {} (type: {:?})",
                name, inferred_type.or(declared_type)
            ));
        }
    }

    fn validate_table_property(
        &mut self,
        path: &TablePath,
        properties: &[PropertyAssignment],
        _position: Position,
        symbol_table: &mut SymbolTable,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) {
        let full_path = Self::join_table_path(&path.segments);

        for assignment in properties {
            // Check reserved keywords
            if Keywords::is_reserved_in_context(&assignment.name, "DATA") {
                self.add_error(
                    result,
                    ERROR_RESERVED_KEYWORD,
                    &Keywords::get_keyword_usage_error(&assignment.name, "DATA"),
                    assignment.position,
                    Some(&format!(
                        "Choose a different name for property '{}.{}'",
                        full_path, assignment.name
                    ))
                );
                continue;
            }

            // Validate type annotation
            if let Some(dt) = assignment.data_type {
                self.validate_type_annotation(dt, &assignment.name, is_debug);
            }

            // Validate value (create TypeInferenceVisitor locally)
            let context = format!("table property '{}.{}'", full_path, assignment.name);
            let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
            let inferred_type = self.validate_value(
                &assignment.value,
                &context,
                symbol_table,
                &type_inference_visitor,
                result,
                is_debug
            );

            // Type compatibility check
            if let (Some(decl), Some(mut inf)) = (assignment.data_type, inferred_type) {
                if decl == DataType::Float && inf != DataType::Float {
                    if inf == DataType::Int || inf == DataType::Double {
                        if is_debug {
                            self.error_manager.log_debug(&format!(
                                "  Auto-converting {:?} to <float> for property '{}.{}'",
                                inf, full_path, assignment.name
                            ));
                        }
                        inf = DataType::Float;
                    }
                }

                if !Self::is_type_compatible(decl, inf) {
                    self.add_error(
                        result,
                        ERROR_TYPE_MISMATCH,
                        &format!(
                            "Property '{}.{}' declared as <{:?}> but value is {:?}",
                            full_path, assignment.name, decl, inf
                        ),
                        assignment.position,
                        None
                    );
                }
            }

            // Build index
            let property_full_path = PathBuilder::build_from(&full_path, &[&assignment.name]);

            self.short_name_to_full_paths
                .entry(assignment.name.clone())
                .or_insert_with(Vec::new)
                .push(property_full_path.clone());

            if let Some(inf) = inferred_type {
                self.path_to_type.insert(property_full_path.clone(), inf);
            }

            if is_debug {
                self.error_manager.log_debug(&format!(
                    "  Indexed: {} -> {}",
                    assignment.name, property_full_path
                ));
            }

            // Add to symbol table
            let var_name = format!("{}.{}", full_path, assignment.name);
            symbol_table.add_data_variable(var_name.clone(), VariableInfo {
                name: var_name,
                declared_type: assignment.data_type,
                inferred_type,
                is_inferred: assignment.data_type.is_none(),
                scope: full_path.clone(),
                line: assignment.position.line as i32,
                column: assignment.position.column as i32,
            });
        }
    }

    fn validate_group_array(
        &mut self,
        path: &TablePath,
        items: &[Value],
        position: Position,
        symbol_table: &mut SymbolTable,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) {
        let full_path = Self::join_table_path(&path.segments);

        if items.is_empty() {
            self.add_warning(
                result,
                &format!("Group array '{}' is empty", full_path),
                position
            );
            return;
        }

        // Validate array homogeneity
        let mut first_item_type: Option<DataType> = None;
        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);

        for (i, item) in items.iter().enumerate() {
            let context = format!("group array '{}[{}]'", full_path, i);
            let item_type = self.validate_value(
                item,
                &context,
                symbol_table,
                &type_inference_visitor,
                result,
                is_debug
            );

            if first_item_type.is_none() {
                first_item_type = item_type;
            } else if let (Some(first), Some(current)) = (first_item_type, item_type) {
                // Allow objects with different properties
                if first == DataType::Object && current == DataType::Object {
                    continue;
                }

                if first != current {
                    self.add_error(
                        result,
                        ERROR_ARRAY_NOT_HOMOGENEOUS,
                        &format!(
                            "Group array '{}' contains mixed types: {:?} and {:?}",
                            full_path, first, current
                        ),
                        item.position(),
                        Some("All items in a group array must be the same type. Use tuples for mixed types.")
                    );
                }
            }

            // Index array item properties
            if let Value::Object { properties, .. } = item {
                for prop in properties {
                    let item_path = PathBuilder::build_array_item_property(&full_path, i, &prop.key);

                    self.short_name_to_full_paths
                        .entry(prop.key.clone())
                        .or_insert_with(Vec::new)
                        .push(item_path.clone());

                    if is_debug {
                        self.error_manager.log_debug(&format!(
                            "  Indexed: {} -> {}",
                            prop.key, item_path
                        ));
                    }
                }
            }
        }

        // Add to symbol table
        symbol_table.add_data_variable(full_path.clone(), VariableInfo {
            name: full_path,
            declared_type: Some(DataType::Array),
            inferred_type: Some(DataType::Array),
            is_inferred: false,
            scope: "global".to_string(),
            line: position.line as i32,
            column: position.column as i32,
        });
    }

    fn validate_object_property(
        &mut self,
        name: &str,
        declared_type: Option<DataType>,
        object: &Value,
        position: Position,
        symbol_table: &mut SymbolTable,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) {
        // Validate type annotation
        if let Some(dt) = declared_type {
            self.validate_type_annotation(dt, name, is_debug);
        }

        // Validate object literal
        self.current_nesting_depth = 0;
        let context = format!("object property '{}'", name);
        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
        self.validate_object_literal(
            object,
            &context,
            symbol_table,
            &type_inference_visitor,
            result,
            is_debug
        );

        // Index the object itself
        let full_path = PathBuilder::build(&[name]);

        self.short_name_to_full_paths
            .entry(name.to_string())
            .or_insert_with(Vec::new)
            .push(full_path.clone());

        let object_type = declared_type.unwrap_or(DataType::Object);
        self.path_to_type.insert(full_path.clone(), object_type);

        if is_debug {
            self.error_manager.log_debug(&format!(
                "  Indexed object: {} -> {} ({:?})",
                name, full_path, object_type
            ));
        }

        // Add to symbol table
        symbol_table.add_data_variable(name.to_string(), VariableInfo {
            name: name.to_string(),
            declared_type: Some(declared_type.unwrap_or(DataType::Object)),
            inferred_type: Some(DataType::Object),
            is_inferred: declared_type.is_none(),
            scope: "global".to_string(),
            line: position.line as i32,
            column: position.column as i32,
        });

        if is_debug {
            self.error_manager.log_debug(&format!("  Added object to symbol table: {}", name));
        }
    }

    // Helper: Join table path segments
    #[inline]
    fn join_table_path(segments: &[String]) -> String {
        segments.join(".")
    }

    // ==================== VALUE VALIDATION ====================

    #[inline]
    fn validate_value(
        &mut self,
        value: &Value,
        context: &str,
        symbol_table: &SymbolTable,
        type_inference_visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) -> Option<DataType> {
        self.current_nesting_depth = 0;
        self.validate_value_recursive(
            value,
            context,
            symbol_table,
            type_inference_visitor,
            result,
            is_debug
        )
    }

    fn validate_value_recursive(
        &mut self,
        value: &Value,
        context: &str,
        symbol_table: &SymbolTable,
        type_inference_visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) -> Option<DataType> {
        // Check nesting depth
        if self.current_nesting_depth > MAX_NESTING_DEPTH {
            self.add_error(
                result,
                ERROR_NESTING_TOO_DEEP,
                &format!(
                    "Nesting depth exceeds maximum of {} levels in {}",
                    MAX_NESTING_DEPTH, context
                ),
                value.position(),
                Some("Flatten your data structure or break it into multiple properties")
            );
            return None;
        }

        // Use type inference visitor for basic type inference
        let inferred_type = type_inference_visitor.infer_type_from_value(value);

        // Additional validation for complex types
        match value {
            Value::Array { values, .. } => {
                return self.validate_array_value(
                    values,
                    context,
                    symbol_table,
                    type_inference_visitor,
                    result,
                    is_debug
                );
            }
            Value::NestedArray { values, .. } => {
                return self.validate_array_value(
                    values,
                    context,
                    symbol_table,
                    type_inference_visitor,
                    result,
                    is_debug
                );
            }
            Value::Object { .. } => {
                return Some(self.validate_object_literal(
                    value,
                    context,
                    symbol_table,
                    type_inference_visitor,
                    result,
                    is_debug
                ));
            }
            Value::PrefixedConstructor { prefix, arguments, position, .. } => {
                return self.validate_prefixed_constructor(
                    prefix,
                    arguments,
                    *position,
                    context,
                    symbol_table,
                    type_inference_visitor,
                    result,
                    is_debug
                );
            }
            Value::EnumValue { enum_name, value: enum_val, position, .. } => {
                return Some(self.validate_enum_value(
                    enum_name,
                    enum_val,
                    *position,
                    context,
                    symbol_table,
                    result,
                    is_debug
                ));
            }
            Value::QuickFuncCall { function_name, arguments, position, .. } => {
                return self.validate_function_call_value(
                    function_name,
                    arguments,
                    *position,
                    context,
                    symbol_table,
                    type_inference_visitor,
                    result,
                    is_debug
                );
            }
            Value::Expression { .. } => {
                self.add_error(
                    result,
                    ERROR_INVALID_EXPRESSION,
                    &format!("Expression values not allowed in DATA section at {}", context),
                    value.position(),
                    Some("Use only literals, function calls, or enum values")
                );
                return None;
            }
            _ => {}
        }

        inferred_type
    }

    fn validate_array_value(
        &mut self,
        values: &[Value],
        context: &str,
        symbol_table: &SymbolTable,
        type_inference_visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) -> Option<DataType> {
        if values.is_empty() {
            return Some(DataType::Array);
        }

        self.current_nesting_depth += 1;

        let mut first_type: Option<DataType> = None;
        for (i, element) in values.iter().enumerate() {
            let elem_context = format!("{}[{}]", context, i);
            let element_type = self.validate_value_recursive(
                element,
                &elem_context,
                symbol_table,
                type_inference_visitor,
                result,
                is_debug
            );

            if first_type.is_none() {
                first_type = element_type;
            } else if let (Some(first), Some(current)) = (first_type, element_type) {
                // Allow object arrays with different properties
                if first == DataType::Object && current == DataType::Object {
                    continue;
                }

                if first != current {
                    self.add_error(
                        result,
                        ERROR_ARRAY_NOT_HOMOGENEOUS,
                        &format!(
                            "Array in {} contains mixed types: {:?} and {:?}",
                            context, first, current
                        ),
                        element.position(),
                        Some("All array elements must be the same type. Use tuples for mixed types.")
                    );
                }
            }
        }

        self.current_nesting_depth -= 1;
        Some(DataType::Array)
    }

    fn validate_object_literal(
        &mut self,
        value: &Value,
        context: &str,
        symbol_table: &SymbolTable,
        type_inference_visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) -> DataType {
        let properties = match value {
            Value::Object { properties, .. } => properties,
            _ => return DataType::Object,
        };

        let mut property_names = FxHashSet::with_capacity_and_hasher(
            properties.len(),
            Default::default()
        );

        self.current_nesting_depth += 1;

        // Extract table path from context
        let object_path = Self::extract_table_path_from_context(context);

        if is_debug {
            self.error_manager.log_debug(&format!(
                "    Validating object literal in context: {}",
                context
            ));
            self.error_manager.log_debug(&format!(
                "    Extracted object path: '{}' (empty = flat property)",
                object_path
            ));
        }

        for prop in properties {
            // Check for duplicate property names
            if !property_names.insert(prop.key.clone()) {
                self.add_error(
                    result,
                    ERROR_DUPLICATE_PROPERTY,
                    &format!("Duplicate property '{}' in object at {}", prop.key, context),
                    prop.position,
                    None
                );
            }

            // Validate property value
            let prop_context = format!("{}.{}", context, prop.key);
            let property_type = self.validate_value_recursive(
                &prop.value,
                &prop_context,
                symbol_table,
                type_inference_visitor,
                result,
                is_debug
            );

            // Index object properties
            let property_full_path = if context.contains("object property") {
                // Extract object name
                if let Some(captures) = Regex::new(r"object property '([^']+)'")
                    .ok()
                    .and_then(|re| re.captures(context))
                {
                    let object_name = &captures[1];
                    PathBuilder::build(&[object_name, &prop.key])
                } else {
                    continue;
                }
            } else if !object_path.is_empty() {
                PathBuilder::build_from(
                    &PathBuilder::ensure_root(&object_path),
                    &[&prop.key]
                )
            } else {
                if is_debug {
                    self.error_manager.log_debug(&format!(
                        "    Skipping index for anonymous object property: {}",
                        prop.key
                    ));
                }
                continue;
            };

            self.short_name_to_full_paths
                .entry(prop.key.clone())
                .or_insert_with(Vec::new)
                .push(property_full_path.clone());

            if let Some(pt) = property_type {
                self.path_to_type.insert(property_full_path.clone(), pt);
            }

            if is_debug {
                self.error_manager.log_debug(&format!(
                    "    Indexed object property: {} -> {} ({:?})",
                    prop.key, property_full_path, property_type
                ));
            }
        }

        self.current_nesting_depth -= 1;
        DataType::Object
    }

    #[inline]
    fn validate_prefixed_constructor(
        &mut self,
        prefix: &str,
        arguments: &[Value],
        position: Position,
        context: &str,
        symbol_table: &SymbolTable,
        type_inference_visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) -> Option<DataType> {
        match prefix.to_lowercase().as_str() {
            "t" => self.validate_tuple_constructor(
                arguments,
                position,
                context,
                symbol_table,
                type_inference_visitor,
                result,
                is_debug
            ),
            "b" => self.validate_blob_constructor(arguments, position, context, result),
            "r" => self.validate_regex_constructor(arguments, position, context, result),
            _ => {
                if is_debug {
                    self.error_manager.log_debug(&format!(
                        "Unknown prefixed constructor: {}",
                        prefix
                    ));
                }
                Some(DataType::Any)
            }
        }
    }

    fn validate_tuple_constructor(
        &mut self,
        arguments: &[Value],
        position: Position,
        context: &str,
        symbol_table: &SymbolTable,
        type_inference_visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) -> Option<DataType> {
        let arg_count = arguments.len();

        if arg_count > MAX_TUPLE_ELEMENTS {
            self.add_error(
                result,
                ERROR_TUPLE_TOO_LARGE,
                &format!(
                    "Tuple in {} has {} elements (maximum is {})",
                    context, arg_count, MAX_TUPLE_ELEMENTS
                ),
                position,
                Some(&format!(
                    "Use an array or object for more than {} elements",
                    MAX_TUPLE_ELEMENTS
                ))
            );
        }

        // Validate each element (tuples can have mixed types)
        for (i, arg) in arguments.iter().enumerate() {
            let elem_context = format!("{}[{}]", context, i);
            self.validate_value_recursive(
                arg,
                &elem_context,
                symbol_table,
                type_inference_visitor,
                result,
                is_debug
            );
        }

        Some(DataType::Tuple)
    }

    fn validate_blob_constructor(
        &mut self,
        arguments: &[Value],
        position: Position,
        context: &str,
        result: &mut SectionAnalysisResult,
    ) -> Option<DataType> {
        if arguments.len() != 1 {
            self.add_error(
                result,
                ERROR_INVALID_BLOB_CONTENT,
                &format!(
                    "Blob constructor in {} must have exactly 1 argument (base64 string)",
                    context
                ),
                position,
                Some("Use format: b:(\"base64EncodedData\")")
            );
            return Some(DataType::Blob);
        }

        let arg = &arguments[0];
        if let Value::String { value: str_val, .. } = arg {
            // Validate base64 content using new Engine API
            if general_purpose::STANDARD.decode(str_val).is_err() {
                self.add_error(
                    result,
                    ERROR_INVALID_BLOB_CONTENT,
                    &format!("Invalid base64 content in blob at {}", context),
                    arg.position(),
                    Some("Ensure the string is valid base64 encoded data")
                );
            }
        } else {
            self.add_error(
                result,
                ERROR_INVALID_BLOB_CONTENT,
                &format!("Blob constructor in {} requires a string literal", context),
                arg.position(),
                Some("Use format: b:(\"base64EncodedData\")")
            );
        }

        Some(DataType::Blob)
    }

    fn validate_regex_constructor(
        &mut self,
        arguments: &[Value],
        position: Position,
        context: &str,
        result: &mut SectionAnalysisResult,
    ) -> Option<DataType> {
        if arguments.len() != 1 {
            self.add_error(
                result,
                ERROR_INVALID_REGEX_PATTERN,
                &format!(
                    "Regex constructor in {} must have exactly 1 argument (pattern string)",
                    context
                ),
                position,
                Some("Use format: r:(\"pattern\")")
            );
            return Some(DataType::Regex);
        }

        let arg = &arguments[0];
        if let Value::String { value: str_val, .. } = arg {
            // Validate regex pattern syntax
            if Regex::new(str_val).is_err() {
                self.add_error(
                    result,
                    ERROR_INVALID_REGEX_PATTERN,
                    &format!("Invalid regex pattern in {}", context),
                    arg.position(),
                    Some("Fix the regular expression syntax")
                );
            }
        } else {
            self.add_error(
                result,
                ERROR_INVALID_REGEX_PATTERN,
                &format!("Regex constructor in {} requires a string literal", context),
                arg.position(),
                Some("Use format: r:(\"pattern\")")
            );
        }

        Some(DataType::Regex)
    }

    // ==================== NEW: MISSING VALIDATION METHODS ====================

    fn validate_enum_value(
        &self,
        enum_name: &str,
        enum_value: &str,
        position: Position,
        context: &str,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) -> DataType {
        // Check if enum exists
        if !symbol_table.has_enum(enum_name) {
            self.add_error(
                result,
                ERROR_ENUM_NOT_FOUND,
                &format!("Enum '{}' not found in {}", enum_name, context),
                position,
                Some("Check @ENUMS section for available enums")
            );
            return DataType::Enum;
        }

        // Check if enum value exists
        if !symbol_table.has_enum_field(enum_name, enum_value) {
            self.add_error(
                result,
                ERROR_ENUM_VALUE_NOT_FOUND,
                &format!(
                    "Enum value '{}.{}' not found in {}",
                    enum_name, enum_value, context
                ),
                position,
                Some(&format!("Check available values in enum '{}'", enum_name))
            );
        }

        if is_debug {
            self.error_manager.log_debug(&format!(
                "  Validated enum value: {}.{}",
                enum_name, enum_value
            ));
        }

        DataType::Enum
    }

    fn validate_function_call_value(
        &mut self,
        function_name: &str,
        _arguments: &[crate::Compiler::AST::Expression],
        position: Position,
        context: &str,
        symbol_table: &SymbolTable,
        _type_inference_visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
        is_debug: bool,
    ) -> Option<DataType> {
        // Check if function exists
        if !symbol_table.has_function(function_name) {
            self.add_error(
                result,
                ERROR_FUNCTION_NOT_FOUND,
                &format!("Function '{}' not found in {}", function_name, context),
                position,
                Some("Check @QUICKFUNCS section or imports for available functions")
            );
            return None;
        }

        // Get function signature
        let func_sig = symbol_table.try_get_function(function_name);

        if is_debug {
            self.error_manager.log_debug(&format!(
                "  Validated function call: {}()",
                function_name
            ));
        }

        // Return function's return type
        func_sig.and_then(|sig| sig.return_type)
    }

    // ==================== TYPE SYSTEM HELPERS ====================

    #[inline]
    fn validate_type_annotation(&self, data_type: DataType, name: &str, is_debug: bool) {
        if is_debug {
            self.error_manager.log_debug(&format!(
                "      Type annotation <{:?}> is valid for '{}'",
                data_type, name
            ));
        }
    }

    #[inline]
    fn is_type_compatible(expected: DataType, actual: DataType) -> bool {
        // Exact match
        if expected == actual {
            return true;
        }

        // Any type accepts everything
        if expected == DataType::Any || actual == DataType::Any {
            return true;
        }

        // Hex type accepts integers
        if expected == DataType::Hex {
            return actual == DataType::Int || actual == DataType::Hex;
        }

        // Numeric type promotions
        if expected == DataType::Double {
            return matches!(actual, DataType::Int | DataType::Float | DataType::Double);
        }

        if expected == DataType::Float {
            return matches!(actual, DataType::Int | DataType::Float | DataType::Double);
        }

        if expected == DataType::Int {
            return actual == DataType::Int;
        }

        // String conversions (anything can be converted to string)
        if expected == DataType::String {
            return true;
        }

        false
    }

    // Helper: Extract table path from context string
    fn extract_table_path_from_context(context: &str) -> String {
        // Extract the quoted path
        let re = Regex::new(r"'([^']+)'(.*)").ok();
        let Some(regex) = re else {
            return String::new();
        };

        let Some(captures) = regex.captures(context) else {
            return String::new();
        };

        let path_in_quotes = &captures[1];
        let after_quotes = captures.get(2).map(|m| m.as_str().trim()).unwrap_or("");

        // Remove array indices
        let path_in_quotes = Regex::new(r"\[\d+\]")
            .ok()
            .map(|re| re.replace_all(path_in_quotes, "").to_string())
            .unwrap_or_else(|| path_in_quotes.to_string());

        // If there's content after quotes, use full path
        if !after_quotes.is_empty() && after_quotes.starts_with('.') {
            return path_in_quotes;
        }

        // Otherwise, get table path
        let property_path = PathBuilder::ensure_root(&path_in_quotes);
        let table_path = PathBuilder::get_table_path(&property_path);
        PathBuilder::strip_root(&table_path)
    }

    // ==================== ERROR/WARNING HELPERS ====================

    #[inline]
    fn add_error(
        &self,
        result: &mut SectionAnalysisResult,
        error_type: &str,
        message: &str,
        position: Position,
        suggestion: Option<&str>,
    ) {
        let error = SemanticErrorInfo {
            error_id: format!("DATA{:X}", error_type.as_bytes().iter().fold(0u32, |acc, &b| acc.wrapping_add(b as u32))),
            error_type: error_type.to_string(),
            message: message.to_string(),
            section_name: "DATA".to_string(),
            suggestion: suggestion.map(|s| s.to_string()).unwrap_or_default(),
            position: Some(position),
        };

        result.errors.push(error);

        if self.operational_settings.debug_mode != DebugMode::Off {
            self.error_manager.log_error(&format!("  [{}] {}", error_type, message));
            if let Some(sugg) = suggestion {
                self.error_manager.log_error(&format!("    Suggestion: {}", sugg));
            }
        }
    }

    #[inline]
    fn add_warning(
        &self,
        result: &mut SectionAnalysisResult,
        message: &str,
        position: Position,
    ) {
        let warning = SemanticWarningInfo {
            warning_id: format!("DATAW{:X}", message.as_bytes().iter().fold(0u32, |acc, &b| acc.wrapping_add(b as u32))),
            message: message.to_string(),
            section_name: "DATA".to_string(),
            position: Some(position),
        };

        result.warnings.push(warning);

        if self.operational_settings.debug_mode != DebugMode::Off {
            self.error_manager.log_warning(message);
        }
    }
}