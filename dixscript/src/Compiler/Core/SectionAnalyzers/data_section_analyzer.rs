
use crate::Compiler::AST::{
    DataSection, DataEntry, TablePath, PropertyAssignment, Value, Position, DataType,
};
use crate::Compiler::AST::Visitors::TypeInferenceVisitor;
use crate::Compiler::Utilities::{SymbolTable, VariableInfo, PathBuilder};
use crate::Compiler::Core::OperationalSettings;
use crate::ErrorManager::{ErrorManager, DebugConfig};
use crate::Utilities::Keywords;
use rustc_hash::{FxHashMap, FxHashSet};
use base64::{Engine as _, engine::general_purpose};
use lazy_static::lazy_static;
use regex::Regex;

use super::{SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo};

// ==================== ERROR TYPE CONSTANTS ====================

const ERROR_ORDERING_VIOLATION:         &str = "ORDERING_VIOLATION";
const ERROR_DUPLICATE_TABLE_PATH:       &str = "DUPLICATE_TABLE_PATH";
const ERROR_DUPLICATE_GROUP_ARRAY_PATH: &str = "DUPLICATE_GROUP_ARRAY_PATH";
const ERROR_RESERVED_KEYWORD:           &str = "RESERVED_KEYWORD";
const ERROR_TYPE_MISMATCH:              &str = "TYPE_MISMATCH";
const ERROR_NESTING_TOO_DEEP:           &str = "NESTING_TOO_DEEP";
const ERROR_ARRAY_NOT_HOMOGENEOUS:      &str = "ARRAY_NOT_HOMOGENEOUS";
const ERROR_DUPLICATE_PROPERTY:         &str = "DUPLICATE_PROPERTY";
const ERROR_TUPLE_TOO_LARGE:            &str = "TUPLE_TOO_LARGE";
const ERROR_ENUM_NOT_FOUND:             &str = "ENUM_NOT_FOUND";
const ERROR_ENUM_VALUE_NOT_FOUND:       &str = "ENUM_VALUE_NOT_FOUND";
const ERROR_FUNCTION_NOT_FOUND:         &str = "FUNCTION_NOT_FOUND";
const ERROR_INVALID_EXPRESSION:         &str = "INVALID_EXPRESSION";
const ERROR_INVALID_BLOB_CONTENT:       &str = "INVALID_BLOB_CONTENT";
const ERROR_INVALID_REGEX_PATTERN:      &str = "INVALID_REGEX_PATTERN";
const ERROR_DUPLICATE_FLAT_PROPERTY:    &str = "DUPLICATE_FLAT_PROPERTY";
const ERROR_DUPLICATE_TABLE_PROPERTY_NAME: &str = "DUPLICATE_TABLE_PROPERTY_NAME";
const MAX_NESTING_DEPTH:  usize = 5;
const MAX_TUPLE_ELEMENTS: usize = 6;

lazy_static! {
    static ref CONTEXT_QUOTE_RE: Regex =
        Regex::new(r"'([^']+)'(.*)").unwrap();

    static ref ARRAY_INDEX_RE: Regex =
        Regex::new(r"\[\d+\]").unwrap();

    static ref OBJECT_PROP_CONTEXT_RE: Regex =
        Regex::new(r"object property '([^']+)'").unwrap();
}

/// Semantic analyzer for the @DATA section.
pub struct DataSectionAnalyzer<'a> {
    operational_settings:    &'a OperationalSettings,
    error_manager:           ErrorManager,
    debug_config:            DebugConfig,

    declared_table_paths:    FxHashSet<String>,
    /// Tracks flat-tier property names (SimpleProperty + ObjectProperty) to
    /// detect duplicates within the same @DATA section.  Table paths and group
    /// array paths are tracked separately by validate_table_path_uniqueness.
    declared_flat_names:     FxHashSet<String>,
    current_nesting_depth:   usize,

    short_name_to_full_paths: FxHashMap<String, Vec<String>>,
    path_to_type:             FxHashMap<String, DataType>,
}

impl<'a> DataSectionAnalyzer<'a> {

    
    //Depricated left regular new in sub moduls for sake of bakwards compatability
//use new with error manager instead from here on out.
pub fn new(operational_settings: &'a OperationalSettings) -> Self {
    Self::new_with_error_manager(operational_settings, ErrorManager::get_shared_instance())
}

pub fn new_with_error_manager(
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
) -> Self {
    DataSectionAnalyzer {
        debug_config:  DebugConfig::from_debug_mode(operational_settings.debug_mode),
        error_manager,
        operational_settings,
        declared_table_paths:     FxHashSet::default(),
        declared_flat_names:      FxHashSet::default(),
        current_nesting_depth:    0,
        short_name_to_full_paths: FxHashMap::default(),
        path_to_type:             FxHashMap::default(),
    }
}
    pub fn analyze(
    &mut self,
    section: &DataSection,
    symbol_table: &mut SymbolTable,
) -> SectionAnalysisResult {
    let mut result = SectionAnalysisResult::new("DATA");
    let entry_count = section.entries.len();

    self.declared_table_paths = FxHashSet::with_capacity_and_hasher(
        entry_count / 2,
        Default::default(),
    );
    self.declared_flat_names = FxHashSet::with_capacity_and_hasher(
        entry_count,
        Default::default(),
    );
    self.current_nesting_depth = 0;

    if self.debug_config.is_enabled {
        self.error_manager.log_info(&format!(
            "Analyzing DATA section with {} entries", entry_count
        ));
    }

    if self.debug_config.is_verbose {
        self.error_manager.log_debug("Phase 1: Validating two-tier ordering");
    }
    self.validate_two_tier_ordering(section, &mut result);

    if self.debug_config.is_verbose {
        self.error_manager.log_debug("Phase 2: Validating table path uniqueness");
    }
    self.validate_table_path_uniqueness(section, &mut result);

    if self.debug_config.is_verbose {
        self.error_manager.log_debug("Phase 3: Validating entries and building indexes");
    }
    for entry in &section.entries {
        self.validate_data_entry(entry, symbol_table, &mut result);
        if self.error_manager.should_terminate_parsing() {
            break;
        }
    }

    result.is_success = result.errors.is_empty();

    if self.debug_config.is_enabled {
        self.error_manager.log_info(&format!(
            "DATA analysis {}: {} entries, {} short names, {} types, {} errors, {} warnings",
            if result.is_success { "SUCCESS" } else { "FAILED" },
            entry_count,
            self.short_name_to_full_paths.len(),
            self.path_to_type.len(),
            result.errors.len(),
            result.warnings.len(),
        ));
    }

    result
}

    #[inline]
    pub fn get_indexes(
        &self,
    ) -> (&FxHashMap<String, Vec<String>>, &FxHashMap<String, DataType>) {
        (&self.short_name_to_full_paths, &self.path_to_type)
    }

    // ==================== PHASE 1: TWO-TIER ORDERING ====================

    fn validate_two_tier_ordering(
        &self,
        section: &DataSection,
        result: &mut SectionAnalysisResult,
    ) {
        let mut has_seen_grouped = false;

        for entry in &section.entries {
            match entry {
                DataEntry::SimpleProperty { name, position, .. }
                | DataEntry::ObjectProperty { name, position, .. } => {
                    if has_seen_grouped {
                        self.add_error(
                            result,
                            ERROR_ORDERING_VIOLATION,
                            &format!(
                                "Flat property '{}' appears after grouped data. \
                                All flat properties must come before table properties and group arrays.",
                                name
                            ),
                            *position,
                            Some("Move this property before any table properties (path:) or group arrays (path::)"),
                        );
                    }
                }
                DataEntry::TableProperty { .. } | DataEntry::GroupArray { .. } => {
                    has_seen_grouped = true;
                }
            }
        }
    }

    // ==================== PHASE 2: TABLE PATH UNIQUENESS ====================

    fn validate_table_path_uniqueness(
        &mut self,
        section: &DataSection,
        result: &mut SectionAnalysisResult,
    ) {
        let estimated = section.entries.len() / 3;
        let mut table_paths =
            FxHashSet::with_capacity_and_hasher(estimated, Default::default());
        let mut array_paths =
            FxHashSet::with_capacity_and_hasher(estimated, Default::default());

        for entry in &section.entries {
            match entry {
                DataEntry::TableProperty { path, position, .. } => {
                    let path_str = Self::join_path(&path.segments);
                    if !table_paths.insert(path_str.clone()) {
                        self.add_error(
                            result,
                            ERROR_DUPLICATE_TABLE_PATH,
                            &format!("Table property path '{}' is defined multiple times", path_str),
                            *position,
                            Some("Combine assignments into a single table property or use different paths"),
                        );
                    } else {
                        self.declared_table_paths.insert(path_str);
                    }
                }
                DataEntry::GroupArray { path, position, .. } => {
                    let path_str = Self::join_path(&path.segments);
                    if !array_paths.insert(path_str.clone()) {
                        self.add_error(
                            result,
                            ERROR_DUPLICATE_GROUP_ARRAY_PATH,
                            &format!("Group array path '{}' is defined multiple times", path_str),
                            *position,
                            Some("Combine items into a single group array or use different paths"),
                        );
                    } else {
                        self.declared_table_paths.insert(path_str);
                    }
                }
                _ => {}
            }
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "Table path uniqueness: {} unique paths",
                table_paths.len() + array_paths.len()
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
    ) {
        match entry {
            DataEntry::SimpleProperty { name, data_type, value, position } => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!("  Validating simple property: {}", name));
                }
                self.validate_simple_property(
                    name, *data_type, value, *position, symbol_table, result,
                );
            }
            DataEntry::TableProperty { path, properties, position } => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "  Validating table property: {}", Self::join_path(&path.segments)
                    ));
                }
                self.validate_table_property(path, properties, *position, symbol_table, result);
            }
            DataEntry::GroupArray { path, items, position } => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "  Validating group array: {}", Self::join_path(&path.segments)
                    ));
                }
                self.validate_group_array(path, items, *position, symbol_table, result);
            }
            DataEntry::ObjectProperty { name, data_type, object, position } => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!("  Validating object property: {}", name));
                }
                self.validate_object_property(
                    name, *data_type, object.as_ref(), *position, symbol_table, result,
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
) {
    if Keywords::is_reserved_in_context(name, "DATA") {
        self.add_error(
            result,
            ERROR_RESERVED_KEYWORD,
            &Keywords::get_keyword_usage_error(name, "DATA"),
            position,
            Some(&format!("Choose a different name for property '{}'", name)),
        );
        return;
    }

    // Duplicate flat-tier property check.
    // Only flat properties (SimpleProperty / ObjectProperty) are tracked here;
    // table paths and group arrays are checked separately.
    if !self.declared_flat_names.insert(name.to_string()) {
        self.add_error(
            result,
            ERROR_DUPLICATE_FLAT_PROPERTY,
            &format!(
                "Flat property '{}' is defined more than once in @DATA. \
                 Each flat property name must be unique.",
                name
            ),
            position,
            Some(&format!(
                "Remove or rename the duplicate '{}' property. \
                 Note: table properties (path:) and group arrays (path::) \
                 may share this name.",
                name
            )),
        );
        return;
    }

    let context = format!("property '{}'", name);
    let inferred_type = {
        let visitor = TypeInferenceVisitor::new(symbol_table, None);
        self.validate_value(value, &context, symbol_table, &visitor, result)
    };

    let full_path = PathBuilder::build(&[name]);

    self.short_name_to_full_paths
        .entry(name.to_string())
        .or_insert_with(Vec::new)
        .push(full_path.clone());

    if let Some(inf) = inferred_type {
        self.path_to_type.insert(full_path.clone(), inf);
    } else if let Some(decl) = declared_type {
        self.path_to_type.insert(full_path.clone(), decl);
    }

    if self.debug_config.is_verbose {
        self.error_manager.log_debug(&format!(
            "  Indexed: {} -> {} ({:?})", name, full_path, inferred_type.or(declared_type)
        ));
    }

    if let (Some(decl), Some(mut inf)) = (declared_type, inferred_type) {
        // Widening coercions: allow numeric promotion in the declared direction.
        if decl == DataType::Float
            && matches!(inf, DataType::Int | DataType::Long | DataType::Double)
        {
            inf = DataType::Float;
            self.path_to_type.insert(full_path.clone(), DataType::Float);
        }
        if decl == DataType::Long && inf == DataType::Int {
            inf = DataType::Long;
            self.path_to_type.insert(full_path.clone(), DataType::Long);
        }
        if decl == DataType::Double
            && matches!(inf, DataType::Int | DataType::Long | DataType::Float)
        {
            inf = DataType::Double;
            self.path_to_type.insert(full_path.clone(), DataType::Double);
        }

        if !Self::is_type_compatible(decl, inf) {
            self.add_error(
                result,
                ERROR_TYPE_MISMATCH,
                &format!(
                    "Property '{}' is declared as <{}> but the value is <{}>. \
                     These types are not compatible.",
                    name, decl, inf
                ),
                position,
                Some(&format!(
                    "Either change the type annotation to <{}> or \
                     provide a value of type <{}>.",
                    inf, decl
                )),
            );
        }
    }

    symbol_table.add_data_variable(name.to_string(), VariableInfo {
        name: name.to_string(),
        declared_type,
        inferred_type,
        is_inferred: declared_type.is_none(),
        scope: "global".to_string(),
        line: position.line as i32,
        column: position.column as i32,
    });
}

    fn validate_table_property(
    &mut self,
    path: &TablePath,
    properties: &[PropertyAssignment],
    _position: Position,
    symbol_table: &mut SymbolTable,
    result: &mut SectionAnalysisResult,
) {
    let full_path = Self::join_path(&path.segments);

    // Track seen names within this single table path call to catch duplicates
    let mut seen_names: FxHashSet<&str> =
        FxHashSet::with_capacity_and_hasher(properties.len(), Default::default());

    for assignment in properties {
        // ── Duplicate property name within this table path ────────────────
        if !seen_names.insert(assignment.name.as_str()) {
            self.add_error(
                result,
                ERROR_DUPLICATE_TABLE_PROPERTY_NAME,
                &format!(
                    "Property '{}' is defined more than once in table path '{}'",
                    assignment.name, full_path
                ),
                assignment.position,
                Some(&format!(
                    "Remove or rename the duplicate '{}' property in table '{}'",
                    assignment.name, full_path
                )),
            );
            continue; // Skip further validation for this duplicate
        }

        if Keywords::is_reserved_in_context(&assignment.name, "DATA") {
            self.add_error(
                result,
                ERROR_RESERVED_KEYWORD,
                &Keywords::get_keyword_usage_error(&assignment.name, "DATA"),
                assignment.position,
                Some(&format!(
                    "Choose a different name for property '{}.{}'",
                    full_path, assignment.name
                )),
            );
            continue;
        }

        let context = format!("table property '{}.{}'", full_path, assignment.name);
        let inferred_type = {
            let visitor = TypeInferenceVisitor::new(symbol_table, None);
            self.validate_value(&assignment.value, &context, symbol_table, &visitor, result)
        };

        if let (Some(decl), Some(mut inf)) = (assignment.data_type, inferred_type) {
            if decl == DataType::Float
                && matches!(inf, DataType::Int | DataType::Long | DataType::Double)
            {
                inf = DataType::Float;
            }
            if decl == DataType::Long && inf == DataType::Int {
                inf = DataType::Long;
            }
            if decl == DataType::Double
                && matches!(inf, DataType::Int | DataType::Long | DataType::Float)
            {
                inf = DataType::Double;
            }
            if !Self::is_type_compatible(decl, inf) {
                self.add_error(
                    result,
                    ERROR_TYPE_MISMATCH,
                    &format!(
                        "Property '{}.{}' declared as <{}> but value is <{}>. \
                         These types are not compatible.",
                        full_path, assignment.name, decl, inf
                    ),
                    assignment.position,
                    Some(&format!(
                        "Either change the type annotation to <{}> or \
                         provide a value of type <{}>.",
                        inf, decl
                    )),
                );
            }
        }

        let prop_path = PathBuilder::build_from(&full_path, &[&assignment.name]);

        self.short_name_to_full_paths
            .entry(assignment.name.clone())
            .or_insert_with(Vec::new)
            .push(prop_path.clone());

        if let Some(inf) = inferred_type {
            self.path_to_type.insert(prop_path.clone(), inf);
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "  Indexed: {} -> {}", assignment.name, prop_path
            ));
        }

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
    ) {
        let full_path = Self::join_path(&path.segments);

        if items.is_empty() {
            self.add_warning(result, &format!("Group array '{}' is empty", full_path), position);
            return;
        }

        let mut first_item_type: Option<DataType> = None;

        {
            let visitor = TypeInferenceVisitor::new(symbol_table, None);

            for (i, item) in items.iter().enumerate() {
                let context = format!("group array '{}[{}]'", full_path, i);
                let item_type =
                    self.validate_value(item, &context, symbol_table, &visitor, result);

                if first_item_type.is_none() {
                    first_item_type = item_type;
                } else if let (Some(first), Some(current)) = (first_item_type, item_type) {
                    if !(first == DataType::Object && current == DataType::Object)
                        && first != current
                    {
                        self.add_error(
                            result,
                            ERROR_ARRAY_NOT_HOMOGENEOUS,
                            &format!(
                                "Group array '{}' contains mixed types: {:?} and {:?}",
                                full_path, first, current
                            ),
                            item.position(),
                            Some("All items in a group array must be the same type."),
                        );
                    }
                }

                if let Value::Object { properties, .. } = item {
                    for prop in properties {
                        let item_path =
                            PathBuilder::build_array_item_property(&full_path, i, &prop.key);
                        self.short_name_to_full_paths
                            .entry(prop.key.clone())
                            .or_insert_with(Vec::new)
                            .push(item_path.clone());

                        if self.debug_config.is_verbose {
                            self.error_manager.log_debug(&format!(
                                "  Indexed: {} -> {}", prop.key, item_path
                            ));
                        }
                    }
                }
            }
        }

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
) {
    // ObjectProperty is also in the flat tier — check for duplicates.
    if !self.declared_flat_names.insert(name.to_string()) {
        self.add_error(
            result,
            ERROR_DUPLICATE_FLAT_PROPERTY,
            &format!(
                "Flat property '{}' is defined more than once in @DATA. \
                 Each flat property name must be unique.",
                name
            ),
            position,
            Some(&format!(
                "Remove or rename the duplicate '{}' property.",
                name
            )),
        );
        return;
    }

    self.current_nesting_depth = 0;
    let context = format!("object property '{}'", name);

    {
        let visitor = TypeInferenceVisitor::new(symbol_table, None);
        self.validate_object_literal(object, &context, symbol_table, &visitor, result);
    }

    let full_path = PathBuilder::build(&[name]);
    self.short_name_to_full_paths
        .entry(name.to_string())
        .or_insert_with(Vec::new)
        .push(full_path.clone());

    let object_type = declared_type.unwrap_or(DataType::Object);
    self.path_to_type.insert(full_path.clone(), object_type);

    if self.debug_config.is_verbose {
        self.error_manager.log_debug(&format!(
            "  Indexed object: {} -> {} ({:?})", name, full_path, object_type
        ));
    }

    symbol_table.add_data_variable(name.to_string(), VariableInfo {
        name: name.to_string(),
        declared_type: Some(declared_type.unwrap_or(DataType::Object)),
        inferred_type: Some(DataType::Object),
        is_inferred: declared_type.is_none(),
        scope: "global".to_string(),
        line: position.line as i32,
        column: position.column as i32,
    });
}

    #[inline]
    fn join_path(segments: &[String]) -> String {
        segments.join(".")
    }

    // ==================== VALUE VALIDATION ====================

    #[inline]
    fn validate_value(
        &mut self,
        value: &Value,
        context: &str,
        symbol_table: &SymbolTable,
        visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
    ) -> Option<DataType> {
        self.current_nesting_depth = 0;
        self.validate_value_recursive(value, context, symbol_table, visitor, result)
    }

    fn validate_value_recursive(
        &mut self,
        value: &Value,
        context: &str,
        symbol_table: &SymbolTable,
        visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
    ) -> Option<DataType> {
        if self.current_nesting_depth > MAX_NESTING_DEPTH {
            self.add_error(
                result,
                ERROR_NESTING_TOO_DEEP,
                &format!(
                    "Nesting depth exceeds maximum of {} levels in {}",
                    MAX_NESTING_DEPTH, context
                ),
                value.position(),
                Some("Flatten your data structure or break it into multiple properties"),
            );
            return None;
        }

        let inferred_type = visitor.infer_type_from_value(value);

        match value {
            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                return self.validate_array_value(values, context, symbol_table, visitor, result);
            }
            Value::Object { .. } => {
                return Some(self.validate_object_literal(
                    value, context, symbol_table, visitor, result,
                ));
            }
            Value::PrefixedConstructor { prefix, arguments, position, .. } => {
                return self.validate_prefixed_constructor(
                    prefix, arguments, *position, context, symbol_table, visitor, result,
                );
            }
            Value::EnumValue { enum_name, value: enum_val, position, .. } => {
                return Some(self.validate_enum_value(
                    enum_name, enum_val, *position, context, symbol_table, result,
                ));
            }
            Value::QuickFuncCall { function_name, arguments, position, .. } => {
                return self.validate_function_call_value(
                    function_name, arguments, *position, context, symbol_table, visitor, result,
                );
            }
            Value::Expression { .. } => {
                self.add_error(
                    result,
                    ERROR_INVALID_EXPRESSION,
                    &format!("Expression values not allowed in DATA section at {}", context),
                    value.position(),
                    Some("Use only literals, function calls, or enum values"),
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
        visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
    ) -> Option<DataType> {
        if values.is_empty() {
            return Some(DataType::Array);
        }

        self.current_nesting_depth += 1;
        let mut first_type: Option<DataType> = None;

        for (i, element) in values.iter().enumerate() {
            let elem_context = format!("{}[{}]", context, i);
            let element_type = self.validate_value_recursive(
                element, &elem_context, symbol_table, visitor, result,
            );

            if first_type.is_none() {
                first_type = element_type;
            } else if let (Some(first), Some(current)) = (first_type, element_type) {
                if !(first == DataType::Object && current == DataType::Object)
                    && first != current
                {
                    self.add_error(
                        result,
                        ERROR_ARRAY_NOT_HOMOGENEOUS,
                        &format!(
                            "Array in {} contains mixed types: {:?} and {:?}",
                            context, first, current
                        ),
                        element.position(),
                        Some("All array elements must be the same type."),
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
        visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
    ) -> DataType {
        let properties = match value {
            Value::Object { properties, .. } => properties,
            _ => return DataType::Object,
        };

        let mut seen_keys: FxHashSet<&str> =
            FxHashSet::with_capacity_and_hasher(properties.len(), Default::default());
        self.current_nesting_depth += 1;

        let object_path = Self::extract_table_path_from_context(context);

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "    Object literal in '{}', extracted path: '{}'", context, object_path
            ));
        }

        for prop in properties {
            if !seen_keys.insert(prop.key.as_str()) {
                self.add_error(
                    result,
                    ERROR_DUPLICATE_PROPERTY,
                    &format!("Duplicate property '{}' in object at {}", prop.key, context),
                    prop.position,
                    None,
                );
            }

            let prop_context = format!("{}.{}", context, prop.key);
            let property_type = self.validate_value_recursive(
                &prop.value, &prop_context, symbol_table, visitor, result,
            );

            let opt_path: Option<String> =
                if let Some(caps) = OBJECT_PROP_CONTEXT_RE.captures(context) {
                    Some(PathBuilder::build(&[&caps[1], &prop.key]))
                } else if !object_path.is_empty() {
                    Some(PathBuilder::build_from(
                        &PathBuilder::ensure_root(&object_path),
                        &[&prop.key],
                    ))
                } else {
                    if self.debug_config.is_verbose {
                        self.error_manager.log_debug(&format!(
                            "    Skipping index for anonymous object property: {}", prop.key
                        ));
                    }
                    None
                };

            if let Some(full_path) = opt_path {
                self.short_name_to_full_paths
                    .entry(prop.key.clone())
                    .or_insert_with(Vec::new)
                    .push(full_path.clone());

                if let Some(pt) = property_type {
                    self.path_to_type.insert(full_path.clone(), pt);
                }

                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "    Indexed: {} -> {} ({:?})", prop.key, full_path, property_type
                    ));
                }
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
        visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
    ) -> Option<DataType> {
        match prefix.to_lowercase().as_str() {
            "t" => self.validate_tuple_constructor(
                arguments, position, context, symbol_table, visitor, result,
            ),
            "b" => self.validate_blob_constructor(arguments, position, context, result),
            "r" => self.validate_regex_constructor(arguments, position, context, result),
            _ => Some(DataType::Any),
        }
    }

    fn validate_tuple_constructor(
        &mut self,
        arguments: &[Value],
        position: Position,
        context: &str,
        symbol_table: &SymbolTable,
        visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
    ) -> Option<DataType> {
        if arguments.len() > MAX_TUPLE_ELEMENTS {
            self.add_error(
                result,
                ERROR_TUPLE_TOO_LARGE,
                &format!(
                    "Tuple in {} has {} elements (maximum is {})",
                    context, arguments.len(), MAX_TUPLE_ELEMENTS
                ),
                position,
                Some(&format!(
                    "Use an array or object for more than {} elements", MAX_TUPLE_ELEMENTS
                )),
            );
        }

        for (i, arg) in arguments.iter().enumerate() {
            let elem_context = format!("{}[{}]", context, i);
            self.validate_value_recursive(arg, &elem_context, symbol_table, visitor, result);
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
                    "Blob constructor in {} must have exactly 1 argument (base64 string)", context
                ),
                position,
                Some("Use format: b:(\"base64EncodedData\")"),
            );
            return Some(DataType::Blob);
        }

        let arg = &arguments[0];
        if let Value::String { value: str_val, .. } = arg {
            if general_purpose::STANDARD.decode(str_val).is_err() {
                self.add_error(
                    result,
                    ERROR_INVALID_BLOB_CONTENT,
                    &format!("Invalid base64 content in blob at {}", context),
                    arg.position(),
                    Some("Ensure the string is valid base64 encoded data"),
                );
            }
        } else {
            self.add_error(
                result,
                ERROR_INVALID_BLOB_CONTENT,
                &format!("Blob constructor in {} requires a string literal", context),
                arg.position(),
                Some("Use format: b:(\"base64EncodedData\")"),
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
                    "Regex constructor in {} must have exactly 1 argument (pattern string)", context
                ),
                position,
                Some("Use format: r:(\"pattern\")"),
            );
            return Some(DataType::Regex);
        }

        let arg = &arguments[0];
        if let Value::String { value: str_val, .. } = arg {
            if Regex::new(str_val).is_err() {
                self.add_error(
                    result,
                    ERROR_INVALID_REGEX_PATTERN,
                    &format!("Invalid regex pattern in {}", context),
                    arg.position(),
                    Some("Fix the regular expression syntax"),
                );
            }
        } else {
            self.add_error(
                result,
                ERROR_INVALID_REGEX_PATTERN,
                &format!("Regex constructor in {} requires a string literal", context),
                arg.position(),
                Some("Use format: r:(\"pattern\")"),
            );
        }

        Some(DataType::Regex)
    }

    // ==================== ENUM VALUE VALIDATION ====================
    //
    // Handles three cases:
    //   1. Local enum:              Rarity.COMMON          → enum_name="Rarity",  value="COMMON"
    //   2. Namespaced enum (2-part): Base.Rarity           → treated as namespace lookup
    //   3. Namespaced enum (3-part): Base.Rarity.COMMON    → namespace="Base", enum="Rarity", value="COMMON"
    //
    // The parser may emit the full dotted string as `enum_name` when it cannot
    // resolve the namespace at parse time, so we split on '.' here to handle all
    // three forms.

    fn validate_enum_value(
        &self,
        enum_name: &str,
        enum_value: &str,
        position: Position,
        context: &str,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
    ) -> DataType {
        // Fast path — local enum (no dot in name).
        if !enum_name.contains('.') {
            return self.validate_local_enum(
                enum_name, enum_value, position, context, symbol_table, result,
            );
        }

        // Dotted name — split and treat as namespaced enum.
        // Supported forms after splitting:
        //   ["Namespace", "EnumName"]          — enum_value already supplied separately
        //   ["Namespace", "EnumName", "VALUE"] — value embedded in the name string
        let parts: Vec<&str> = enum_name.splitn(3, '.').collect();

        match parts.as_slice() {
            [namespace, enum_nm] => {
                // Form: Namespace.EnumName  with value supplied separately.
                self.validate_namespaced_enum(
                    namespace, enum_nm, enum_value, position, context, symbol_table, result,
                )
            }
            [namespace, enum_nm, embedded_value] => {
                // Form: Namespace.EnumName.VALUE — value was embedded in the name string.
                self.validate_namespaced_enum(
                    namespace, enum_nm, embedded_value, position, context, symbol_table, result,
                )
            }
            _ => {
                // Unexpected shape — fall back to a local lookup.
                self.validate_local_enum(
                    enum_name, enum_value, position, context, symbol_table, result,
                )
            }
        }
    }

    /// Validate a plain (non-namespaced) enum access.
    fn validate_local_enum(
        &self,
        enum_name: &str,
        enum_value: &str,
        position: Position,
        context: &str,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
    ) -> DataType {
        if !symbol_table.has_enum(enum_name) {
            self.add_error(
                result,
                ERROR_ENUM_NOT_FOUND,
                &format!("Enum '{}' not found in {}", enum_name, context),
                position,
                Some("Check @ENUMS section for available enums"),
            );
            return DataType::Enum;
        }

        if !symbol_table.has_enum_field(enum_name, enum_value) {
            self.add_error(
                result,
                ERROR_ENUM_VALUE_NOT_FOUND,
                &format!(
                    "Enum value '{}.{}' not found in {}", enum_name, enum_value, context
                ),
                position,
                Some(&format!("Check available values in enum '{}'", enum_name)),
            );
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "  Validated local enum value: {}.{}", enum_name, enum_value
            ));
        }

        DataType::Enum
    }

    /// Validate a namespaced enum access: `Namespace.EnumName.VALUE`.
    fn validate_namespaced_enum(
        &self,
        namespace: &str,
        enum_name: &str,
        enum_value: &str,
        position: Position,
        context: &str,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
    ) -> DataType {
        match symbol_table.get_namespaced_enum(namespace, enum_name) {
            None => {
                // Namespace may not be loaded yet — check if namespace exists at all.
                if !symbol_table.is_imported_namespace(namespace) {
                    self.add_error(
                        result,
                        ERROR_ENUM_NOT_FOUND,
                        &format!(
                            "Namespace '{}' not found — import it with @IMPORTS in {}",
                            namespace, context
                        ),
                        position,
                        Some(&format!(
                            "Add: {} from \"path/to/file.mdix\" in @IMPORTS",
                            namespace
                        )),
                    );
                } else {
                    self.add_error(
                        result,
                        ERROR_ENUM_NOT_FOUND,
                        &format!(
                            "Enum '{}' not found in namespace '{}' in {}",
                            enum_name, namespace, context
                        ),
                        position,
                        Some(&format!(
                            "Check that '{}' exports enum '{}'",
                            namespace, enum_name
                        )),
                    );
                }
            }
            Some(fields) => {
                if !fields.contains_key(enum_value) {
                    let valid: Vec<&String> = fields.keys().collect();
                    self.add_error(
                        result,
                        ERROR_ENUM_VALUE_NOT_FOUND,
                        &format!(
                            "Enum value '{}.{}.{}' not found in {}",
                            namespace, enum_name, enum_value, context
                        ),
                        position,
                        Some(&format!(
                            "Valid values: {}",
                            valid.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        )),
                    );
                } else if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "  Validated namespaced enum: {}.{}.{}",
                        namespace, enum_name, enum_value
                    ));
                }
            }
        }

        DataType::Enum
    }

    // ==================== FUNCTION CALL VALIDATION ====================
    //
    // Handles two cases:
    //   1. Local function:      makeItem(...)         → function_name="makeItem"
    //   2. Namespaced function: Base.makeItem(...)    → function_name="Base.makeItem"
    //
    // The parser may emit the full dotted string as `function_name`.

    fn validate_function_call_value(
        &mut self,
        function_name: &str,
        _arguments: &[crate::Compiler::AST::Expression],
        position: Position,
        context: &str,
        symbol_table: &SymbolTable,
        _visitor: &TypeInferenceVisitor,
        result: &mut SectionAnalysisResult,
    ) -> Option<DataType> {
        // Namespaced call: "Namespace.FunctionName"
        if let Some(dot_pos) = function_name.find('.') {
            let namespace     = &function_name[..dot_pos];
            let func_name     = &function_name[dot_pos + 1..];
            return self.validate_namespaced_function_call(
                namespace, func_name, position, context, symbol_table, result,
            );
        }

        // Local function call.
        if !symbol_table.has_function(function_name) {
            self.add_error(
                result,
                ERROR_FUNCTION_NOT_FOUND,
                &format!("Function '{}' not found in {}", function_name, context),
                position,
                Some("Check @QUICKFUNCS section or imports for available functions"),
            );
            return None;
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "  Validated local function call: {}()", function_name
            ));
        }

        symbol_table
            .try_get_function(function_name)
            .and_then(|sig| sig.return_type)
    }

    /// Validate a namespaced function call: `Namespace.FunctionName(...)`.
    fn validate_namespaced_function_call(
        &self,
        namespace: &str,
        func_name: &str,
        position: Position,
        context: &str,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
    ) -> Option<DataType> {
        if !symbol_table.is_imported_namespace(namespace) {
            self.add_error(
                result,
                ERROR_FUNCTION_NOT_FOUND,
                &format!(
                    "Namespace '{}' not found — import it with @IMPORTS in {}",
                    namespace, context
                ),
                position,
                Some(&format!(
                    "Add: {} from \"path/to/file.mdix\" in @IMPORTS",
                    namespace
                )),
            );
            return None;
        }

        match symbol_table.get_namespaced_function(namespace, func_name) {
            None => {
                self.add_error(
                    result,
                    ERROR_FUNCTION_NOT_FOUND,
                    &format!(
                        "Function '{}.{}' not found in {}",
                        namespace, func_name, context
                    ),
                    position,
                    Some(&format!(
                        "Check that '{}' exports function '{}'",
                        namespace, func_name
                    )),
                );
                None
            }
            Some(info) => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "  Validated namespaced function call: {}.{}()",
                        namespace, func_name
                    ));
                }
                info.signature.return_type
            }
        }
    }

    // ==================== TYPE SYSTEM ====================

#[inline]
fn is_type_compatible(expected: DataType, actual: DataType) -> bool {
    if expected == actual {
        return true;
    }
    if expected == DataType::Any || actual == DataType::Any {
        return true;
    }

    match expected {
        // Hex accepts integer hex literals (0xFF) and plain integers.
        DataType::Hex => matches!(actual, DataType::Int | DataType::Long | DataType::Hex),

        // Long is wider than Int; widening is safe.
        DataType::Long => matches!(actual, DataType::Int | DataType::Long),

        // Float and Double accept all numeric types (widening).
        DataType::Float | DataType::Double => matches!(
            actual,
            DataType::Int | DataType::Long | DataType::Float | DataType::Double
        ),

        // String only accepts String (InterpolatedString infers as String).
        // DO NOT fall through to true here — this was a bug.
        DataType::String => actual == DataType::String,

        // Bool only accepts Bool.
        DataType::Bool => actual == DataType::Bool,

        // Int does NOT accept Float/Double (narrowing).
        DataType::Int => matches!(actual, DataType::Int),

        // Object accepts Object only.
        DataType::Object => actual == DataType::Object,

        // Strict single-type matches for everything else.
        DataType::Array     => actual == DataType::Array,
        DataType::Tuple     => actual == DataType::Tuple,
        DataType::Blob      => actual == DataType::Blob,
        DataType::Regex     => actual == DataType::Regex,
        DataType::Date      => actual == DataType::Date,
        DataType::Timestamp => actual == DataType::Timestamp,
        DataType::Enum      => actual == DataType::Enum,
        DataType::Function  => actual == DataType::Function,
        DataType::Range     => actual == DataType::Range,

        // Any was handled above; this arm is unreachable.
        DataType::Any => true,
    }
}

    fn extract_table_path_from_context(context: &str) -> String {
        let Some(captures) = CONTEXT_QUOTE_RE.captures(context) else {
            return String::new();
        };

        let path_in_quotes = &captures[1];
        let after_quotes = captures.get(2).map(|m| m.as_str().trim()).unwrap_or("");

        let path_stripped = ARRAY_INDEX_RE.replace_all(path_in_quotes, "").to_string();

        if !after_quotes.is_empty() && after_quotes.starts_with('.') {
            return path_stripped;
        }

        let property_path = PathBuilder::ensure_root(&path_stripped);
        let table_path = PathBuilder::get_table_path(&property_path);
        PathBuilder::strip_root(&table_path)
    }

    // ==================== ERROR / WARNING HELPERS ====================

    #[inline]
    fn add_error(
        &self,
        result: &mut SectionAnalysisResult,
        error_type: &str,
        message: &str,
        position: Position,
        suggestion: Option<&str>,
    ) {
        result.errors.push(SemanticErrorInfo {
            error_id: format!(
                "DATA{:X}",
                error_type
                    .as_bytes()
                    .iter()
                    .fold(0u32, |acc, &b| acc.wrapping_add(b as u32))
            ),
            error_type:   error_type.to_string(),
            message:      message.to_string(),
            section_name: "DATA".to_string(),
            suggestion:   suggestion.map(|s| s.to_string()).unwrap_or_default(),
            position:     Some(position),
        });

        if self.debug_config.is_enabled {
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
        result.warnings.push(SemanticWarningInfo {
            warning_id: format!(
                "DATAW{:X}",
                message
                    .as_bytes()
                    .iter()
                    .fold(0u32, |acc, &b| acc.wrapping_add(b as u32))
            ),
            message:      message.to_string(),
            section_name: "DATA".to_string(),
            position:     Some(position),
        });

        if self.debug_config.is_enabled {
            self.error_manager.log_warning(message);
        }
    }
}

