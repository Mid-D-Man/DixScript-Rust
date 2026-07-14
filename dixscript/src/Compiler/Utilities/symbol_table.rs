//! Symbol table for DixScript semantic analysis
//!
//! Stores:
//! - Imported namespaces and their symbols
//! - Enum definitions
//! - Function signatures
//! - Data section variables
//! - Builtin static objects
//! - Configuration keys

use std::collections::HashMap;
use crate::Compiler::AST::{DataType, QuickFunction};
use crate::Compiler::AST::expressions::Expression;

// ==================== MAIN SYMBOL TABLE ====================

/// Symbol table for semantic analysis
#[derive(Debug, Clone)]
pub struct SymbolTable {
    /// Imported namespaces: Alias -> ImportedNamespace
    pub namespaces: HashMap<String, ImportedNamespace>,

    /// Enum definitions: EnumName -> { FieldName -> Value }
    pub enums: HashMap<String, HashMap<String, i32>>,

    /// Function definitions: FunctionName -> FunctionSignature
    pub functions: HashMap<String, FunctionSignature>,

    /// Data section variables: VariableName -> VariableInfo
    pub data_variables: HashMap<String, VariableInfo>,

    /// Built-in static objects (Dix, Math, DateTime, etc.)
    /// CRITICAL: Must be populated via populate_builtin_objects()
    pub builtin_static_objects: Vec<String>,

    /// Built-in Dix functions (deprecated - use StaticObjectRegistry)
    pub dix_functions: HashMap<String, DixFunctionSignature>,

    /// Config keys: key -> value
    pub configs: HashMap<String, String>,

    /// Current scope stack for nested scope resolution
    scope_stack: Vec<String>,
}

impl SymbolTable {
    /// Create new empty symbol table
    pub fn new() -> Self {
        SymbolTable {
            namespaces: HashMap::new(),
            enums: HashMap::new(),
            functions: HashMap::new(),
            data_variables: HashMap::new(),
            builtin_static_objects: Vec::new(),
            dix_functions: HashMap::new(),
            configs: HashMap::new(),
            scope_stack: Vec::new(),
        }
    }

    // ==================== CRITICAL: BUILTIN POPULATION ====================

    /// CRITICAL METHOD: Populates builtin_static_objects from StaticObjectRegistry
    ///
    /// MUST be called after StaticObjectRegistry::initialize() and before semantic analysis!
    ///
    /// This bridges the gap between the static registry and the symbol table so that
    /// the semantic analyzer can recognize "Dix", "Math", "DateTime", etc. as static objects.
    pub fn populate_builtin_objects(&mut self) {
        // TODO: When StaticObjectRegistry is ported, get object names from it
        // For now, manually add known static objects
        self.builtin_static_objects = vec![
            "Dix".to_string(),
            "Math".to_string(),
            "DateTime".to_string(),
            "Array".to_string(),
            "Random".to_string(),
            "Enum".to_string(),
            "Guid".to_string(),
            "IpAddress".to_string(),
        ];
    }

    /// Check if builtin objects have been populated (for diagnostics)
    pub fn are_builtin_objects_populated(&self) -> bool {
        !self.builtin_static_objects.is_empty()
    }

    // ==================== SEED FROM OUTER TABLE ====================

    /// Seed this symbol table's namespace entries from a provided map.
    ///
    /// Only namespace entries are copied — enums, functions, data variables,
    /// and all other fields remain untouched. This is used when analyzing an
    /// imported file that has transitive dependencies: the outer ImportsResolver
    /// has already registered those dependencies; we copy only the relevant
    /// namespace slots so that QualifiedIdentifier resolution during AST
    /// enhancement can see them without polluting the outer table with this
    /// file's own symbols.
    ///
    /// Entries already present in this table are NOT overwritten, so a file's
    /// own namespace registrations (from a previous pass) are preserved.
    pub fn seed_namespaces_from_map(
        &mut self,
        source: &HashMap<String, ImportedNamespace>,
    ) {
        for (alias, namespace) in source {
            self.namespaces
                .entry(alias.clone())
                .or_insert_with(|| namespace.clone());
        }
    }

    // ==================== ENUM OPERATIONS ====================

    /// Add enum definition
    pub fn add_enum(&mut self, enum_name: String, field_mapping: HashMap<String, i32>) {
        self.enums.insert(enum_name, field_mapping);
    }

    /// Check if enum exists
    pub fn has_enum(&self, enum_name: &str) -> bool {
        self.enums.contains_key(enum_name)
    }

    /// Get enum field mapping
    pub fn try_get_enum(&self, enum_name: &str) -> Option<&HashMap<String, i32>> {
        self.enums.get(enum_name)
    }

    /// Check if enum field exists
    pub fn has_enum_field(&self, enum_name: &str, field_name: &str) -> bool {
        self.enums.get(enum_name)
            .map(|fields| fields.contains_key(field_name))
            .unwrap_or(false)
    }

    /// Get enum field value
    pub fn try_get_enum_field_value(&self, enum_name: &str, field_name: &str) -> Option<i32> {
        self.enums.get(enum_name)
            .and_then(|fields| fields.get(field_name).copied())
    }

    // ==================== CONFIG OPERATIONS ====================

    /// Add config key
    pub fn add_config_key(&mut self, config_key: String, config_entry: String) {
        self.configs.insert(config_key, config_entry);
    }

    /// Check if config key exists
    pub fn has_config(&self, config_key: &str) -> bool {
        self.configs.contains_key(config_key)
    }

    /// Get config value
    pub fn get_config(&self, config_key: &str) -> Option<&String> {
        self.configs.get(config_key)
    }

    // ==================== FUNCTION OPERATIONS ====================

    /// Add function definition
    pub fn add_function(&mut self, function_name: String, signature: FunctionSignature) {
        self.functions.insert(function_name, signature);
    }

    /// Check if function exists
    pub fn has_function(&self, function_name: &str) -> bool {
        self.functions.contains_key(function_name)
    }

    /// Get function signature
    pub fn try_get_function(&self, function_name: &str) -> Option<&FunctionSignature> {
        self.functions.get(function_name)
    }

    // ==================== DATA VARIABLE OPERATIONS ====================

    /// Add data section variable
    pub fn add_data_variable(&mut self, variable_name: String, info: VariableInfo) {
        self.data_variables.insert(variable_name, info);
    }

    /// Check if data variable exists
    pub fn has_data_variable(&self, variable_name: &str) -> bool {
        self.data_variables.contains_key(variable_name)
    }

    /// Get data variable info
    pub fn try_get_data_variable(&self, variable_name: &str) -> Option<&VariableInfo> {
        self.data_variables.get(variable_name)
    }

    // ==================== BUILTIN REGISTRY OPERATIONS ====================

    /// Add builtin static object
    pub fn add_builtin_static_object(&mut self, object_name: String) {
        if !self.builtin_static_objects.contains(&object_name) {
            self.builtin_static_objects.push(object_name);
        }
    }

    /// Check if builtin static object exists
    pub fn is_builtin_static_object(&self, object_name: &str) -> bool {
        self.builtin_static_objects.iter()
            .any(|obj| *obj == object_name)
    }

    /// Add Dix function
    pub fn add_dix_function(
        &mut self,
        function_name: String,
        return_type: String,
        parameter_types: Vec<String>,
    ) {
        self.dix_functions.insert(
            function_name.clone(),
            DixFunctionSignature {
                name: function_name,
                return_type,
                parameter_types,
            },
        );
    }

    /// Check if Dix function exists
    pub fn has_dix_function(&self, function_name: &str) -> bool {
        self.dix_functions.contains_key(function_name)
    }

    /// Get Dix function signature
    pub fn try_get_dix_function(&self, function_name: &str) -> Option<&DixFunctionSignature> {
        self.dix_functions.get(function_name)
    }

    // ==================== SCOPE OPERATIONS ====================

    /// Enter a new scope
    pub fn enter_scope(&mut self, scope_name: String) {
        self.scope_stack.push(scope_name);
    }

    /// Exit current scope
    pub fn exit_scope(&mut self) {
        self.scope_stack.pop();
    }

    /// Get current scope name
    pub fn get_current_scope(&self) -> String {
        self.scope_stack.last()
            .cloned()
            .unwrap_or_else(|| "global".to_string())
    }

    /// Get full scope stack
    pub fn get_scope_stack(&self) -> Vec<String> {
        self.scope_stack.clone()
    }

    // ==================== NAMESPACE OPERATIONS ====================

    /// Register imported namespace
    pub fn register_namespace(
        &mut self,
        alias: String,
        file_path: String,
        functions: HashMap<String, QuickFunctionInfo>,
        enums: HashMap<String, HashMap<String, i32>>,
        local_imports: HashMap<String, ImportedNamespace>,
    ) {
        let ns = ImportedNamespace {
            alias: alias.clone(),
            file_path,
            functions,
            enums,
            local_imports,
        };

        self.namespaces.insert(alias, ns);
    }

    /// Check if namespace is imported
    pub fn is_imported_namespace(&self, alias: &str) -> bool {
        self.namespaces.contains_key(alias)
    }

    /// Get imported namespace
    pub fn try_get_namespace(&self, alias: &str) -> Option<&ImportedNamespace> {
        self.namespaces.get(alias)
    }

    /// Get namespaced function
    pub fn get_namespaced_function(
        &self,
        namespace_name: &str,
        function_name: &str,
    ) -> Option<&QuickFunctionInfo> {
        self.namespaces.get(namespace_name)
            .and_then(|ns| ns.functions.get(function_name))
    }

    /// Get namespaced enum
    pub fn get_namespaced_enum(
        &self,
        namespace_name: &str,
        enum_name: &str,
    ) -> Option<&HashMap<String, i32>> {
        self.namespaces.get(namespace_name)
            .and_then(|ns| ns.enums.get(enum_name))
    }

    /// Check if namespace has local import
    pub fn is_imported_by_namespace(
        &self,
        namespace_name: &str,
        import_alias: &str,
    ) -> bool {
        self.namespaces.get(namespace_name)
            .map(|ns| ns.local_imports.contains_key(import_alias))
            .unwrap_or(false)
    }

    /// Get namespace's local import
    pub fn get_namespace_local_import(
        &self,
        namespace_name: &str,
        import_alias: &str,
    ) -> Option<&ImportedNamespace> {
        self.namespaces.get(namespace_name)
            .and_then(|ns| ns.local_imports.get(import_alias))
    }

    // ==================== UTILITY OPERATIONS ====================

    /// Clear all symbol table data
    pub fn clear(&mut self) {
        self.enums.clear();
        self.functions.clear();
        self.data_variables.clear();
        self.builtin_static_objects.clear();
        self.dix_functions.clear();
        self.namespaces.clear();
        self.configs.clear();
        self.scope_stack.clear();
    }

    /// Get total number of symbols
    pub fn get_total_symbols(&self) -> usize {
        let namespace_symbols: usize = self.namespaces.values()
            .map(|ns| ns.functions.len() + ns.enums.len())
            .sum();

        self.enums.len()
            + self.functions.len()
            + self.data_variables.len()
            + self.builtin_static_objects.len()
            + self.dix_functions.len()
            + namespace_symbols
    }

    /// Get symbol counts by category
    pub fn get_symbol_counts(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        counts.insert("Enums".to_string(), self.enums.len());
        counts.insert("Functions".to_string(), self.functions.len());
        counts.insert("DataVariables".to_string(), self.data_variables.len());
        counts.insert("BuiltinStaticObjects".to_string(), self.builtin_static_objects.len());
        counts.insert("DixFunctions".to_string(), self.dix_functions.len());
        counts.insert("Namespaces".to_string(), self.namespaces.len());
        counts
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SymbolTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SymbolTable: {} symbols (Enums: {}, Functions: {}, DataVars: {}, Builtins: {}, DixFuncs: {}, Namespaces: {})",
            self.get_total_symbols(),
            self.enums.len(),
            self.functions.len(),
            self.data_variables.len(),
            self.builtin_static_objects.len(),
            self.dix_functions.len(),
            self.namespaces.len()
        )
    }
}

// ==================== SUPPORTING DATA STRUCTURES ====================

/// Function signature information
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: String,
    pub return_type: Option<DataType>,
    pub parameters: Vec<ParameterInfo>,
    pub scopes: Vec<String>,
    pub line: i32,
    pub column: i32,
}

impl std::fmt::Display for FunctionSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<String> = self.parameters.iter()
            .map(|p| p.to_string())
            .collect();
        let scope_str = if !self.scopes.is_empty() {
            format!(" => {}", self.scopes.join(","))
        } else {
            String::new()
        };

        write!(
            f,
            "~{}<{:?}>{}({})",
            self.name,
            self.return_type,
            scope_str,
            params.join(", ")
        )
    }
}

/// Parameter information
#[derive(Debug, Clone)]
pub struct ParameterInfo {
    pub name: String,
    pub param_type: Option<DataType>,
    pub has_default_value: bool,
    pub default_value: Option<Expression>,
}

impl std::fmt::Display for ParameterInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let default_str = if self.has_default_value {
            " = <default>".to_string()
        } else {
            String::new()
        };

        write!(f, "{}<{:?}>{}", self.name, self.param_type, default_str)
    }
}

/// Variable information
#[derive(Debug, Clone)]
pub struct VariableInfo {
    pub name: String,
    pub declared_type: Option<DataType>,
    pub inferred_type: Option<DataType>,
    pub is_inferred: bool,
    pub scope: String,
    pub line: i32,
    pub column: i32,
}

impl VariableInfo {
    /// Get effective type (declared or inferred)
    pub fn effective_type(&self) -> Option<DataType> {
        self.declared_type.or(self.inferred_type)
    }
}

impl std::fmt::Display for VariableInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let type_str = if self.is_inferred {
            format!("<{:?}> (inferred)", self.inferred_type)
        } else {
            format!("<{:?}>", self.declared_type)
        };

        write!(f, "{}{} in {}", self.name, type_str, self.scope)
    }
}

/// Dix function signature (deprecated)
#[derive(Debug, Clone)]
pub struct DixFunctionSignature {
    pub name: String,
    pub return_type: String,
    pub parameter_types: Vec<String>,
}

impl std::fmt::Display for DixFunctionSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Dix.{}({}) -> {}",
            self.name,
            self.parameter_types.join(", "),
            self.return_type
        )
    }
}

/// Imported namespace
#[derive(Debug, Clone)]
pub struct ImportedNamespace {
    pub alias: String,
    pub file_path: String,
    pub functions: HashMap<String, QuickFunctionInfo>,
    pub enums: HashMap<String, HashMap<String, i32>>,
    pub local_imports: HashMap<String, ImportedNamespace>,
}

impl std::fmt::Display for ImportedNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Namespace '{}' ({} functions, {} enums, {} imports)",
            self.alias,
            self.functions.len(),
            self.enums.len(),
            self.local_imports.len()
        )
    }
}

/// QuickFunction information for imported namespaces
#[derive(Debug, Clone)]
pub struct QuickFunctionInfo {
    pub signature: FunctionSignature,
    pub ast: QuickFunction,
}

impl std::fmt::Display for QuickFunctionInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.signature)
    }
}
