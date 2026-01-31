// src/Compiler/Core/SectionAnalyzers/quickfuncs_section_analyzer.rs
//! QuickFunctions Section Analyzer - Semantic analysis for @QUICKFUNCS
//!
//! Validates:
//! - Function signatures and parameters
//! - Return types and return paths
//! - Local variable declarations and assignments
//! - Expression type checking
//! - Control flow (if/switch)
//! - Circular function calls (via CycleDetectionValidator)

use crate::Compiler::AST::*;
use crate::Compiler::AST::Visitors::{TypeInferenceVisitor, AstVisitorBase};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::Compiler::Core::Functions::{CycleDetectionValidator};
use crate::Compiler::Core::SectionAnalyzers::{
    SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo
};
use crate::Compiler::Utilities::{SymbolTable, ParameterInfo, FunctionSignature};
use crate::Builtins::Core::DixType;
use crate::Builtins::Resolver::{
    instance_method_registry, static_object_registry,
    has_instance_method, has_static_method, has_static_object,
};
use crate::Utilities::Keywords;
use crate::ErrorManager::ErrorManager;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashMap;

// ==================== CONSTANTS ====================

const MAX_VALIDATION_DEPTH: usize = 200;
const MAX_TUPLE_ARGUMENTS: usize = 6;
const MAX_ARRAY_ELEMENTS: usize = 10000;
const MAX_OBJECT_PROPERTIES: usize = 1000;
const MAX_FUNCTION_PARAMETERS: usize = 100;
const MAX_FUNCTION_BODY_STATEMENTS: usize = 1000;
const MAX_NESTING_DEPTH: usize = 50;
const MAX_METHOD_CHAIN_DEPTH: usize = 10;

// Use phf for perfect hash tables (compile-time constants)
use phf::{phf_set, Set};

static VALID_ARITHMETIC_OPERATORS: Set<&'static str> = phf_set! {
    "+", "-", "*", "/", "%", "**", "%%", "%&", "&%"
};

static VALID_BITWISE_OPERATORS: Set<&'static str> = phf_set! {
    "&", "|", "^", "<<", ">>"
};

static VALID_COMPARISON_OPERATORS: Set<&'static str> = phf_set! {
    "==", "!=", ">", "<", ">=", "<="
};

static VALID_LOGICAL_OPERATORS: Set<&'static str> = phf_set! {
    "&&", "||", "and", "or"
};

static VALID_UNARY_OPERATORS: Set<&'static str> = phf_set! {
    "!", "not", "-", "+", "~?"
};

static VALID_ARITHMETIC_ASSIGN_OPS: Set<&'static str> = phf_set! {
    "+=", "-=", "*=", "/=", "%=", "**=", "&=", "|=", "^=", "<<=", ">>="
};

static VALID_DATA_TYPES: Set<DataType> = phf_set! {
    DataType::Int, DataType::Float, DataType::Double, DataType::String,
    DataType::Bool, DataType::Array, DataType::Tuple, DataType::Hex,
    DataType::Blob, DataType::Regex, DataType::Object, DataType::Timestamp,
    DataType::Date, DataType::Enum, DataType::Any, DataType::Function,
    DataType::Range
};

// ==================== MAIN ANALYZER ====================

/// QuickFunctions section semantic analyzer
pub struct QuickFuncsSectionAnalyzer {
    operational_settings: OperationalSettings,
    error_manager: ErrorManager,
    validation_depth: usize,
}

impl QuickFuncsSectionAnalyzer {
    /// Create new analyzer
    pub fn new(operational_settings: OperationalSettings) -> Self {
        QuickFuncsSectionAnalyzer {
            operational_settings,
            error_manager: ErrorManager::get_shared_instance(),
            validation_depth: 0,
        }
    }

    /// Main analysis entry point
    pub fn analyze(
        &mut self,
        section: &QuickFuncsSection,
        symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        let mut result = SectionAnalysisResult::new("QUICKFUNCS");
        let function_count = section.functions.len();

        // Ensure builtin objects are populated
        if !symbol_table.are_builtin_objects_populated() {
            symbol_table.populate_builtin_objects();
        }

        if self.operational_settings.debug_mode >= DebugMode::Regular {
            self.error_manager.log_info(&format!(
                "Analyzing QUICKFUNCS section with {} function definitions",
                function_count
            ));
        }

        // Phase 1: Check for duplicate function names
        if self.operational_settings.debug_mode >= DebugMode::Regular {
            self.error_manager.log_debug("Phase 1: Checking for duplicate function names");
        }

        let mut function_names = FxHashSet::with_capacity_and_hasher(
            function_count,
            Default::default()
        );
        let mut duplicate_functions = FxHashSet::default();

        for func in &section.functions {
            if !function_names.insert(&func.name) {
                duplicate_functions.insert(&func.name);
                self.add_error(
                    &mut result,
                    "QFUNC001",
                    "DUPLICATE_FUNCTION_NAME",
                    &format!("Function '{}' is defined multiple times", func.name),
                    "Each function must have a unique name. Remove or rename duplicate function definitions",
                    func.position,
                );

                if self.should_halt(&result) {
                    return result;
                }
            }
        }

        // Phase 2: Pre-register all functions in symbol table
        if self.operational_settings.debug_mode >= DebugMode::Regular {
            self.error_manager.log_debug("Phase 2: Pre-registering all functions in symbol table");
        }

        self.populate_symbol_table(section, symbol_table, &duplicate_functions, &mut result);

        if !result.errors.is_empty() && self.should_halt(&result) {
            return result;
        }

        // Phase 3: Validate individual function declarations
        if self.operational_settings.debug_mode >= DebugMode::Regular {
            self.error_manager.log_debug("Phase 3: Validating individual function declarations");
        }

        for func in &section.functions {
            if duplicate_functions.contains(&func.name) {
                if self.operational_settings.debug_mode >= DebugMode::Regular {
                    self.error_manager.log_debug(&format!(
                        "Skipping validation of duplicate function '{}'",
                        func.name
                    ));
                }
                continue;
            }

            self.validate_quick_function(func, symbol_table, &mut result);

            if self.should_halt(&result) {
                return result;
            }
        }

        // Phase 4: Detect circular function calls
        if self.operational_settings.debug_mode >= DebugMode::Regular {
            self.error_manager.log_debug("Phase 4: Detecting circular function calls");
        }

        let cycle_validator = CycleDetectionValidator::new(
            self.error_manager.clone(),
            self.operational_settings.clone(),
        );

        if !cycle_validator.validate_function_calls(section) {
            // Cycle detector adds errors directly to error_manager
            // We need to extract them and add to result
            // For now, just mark as having errors
            result.is_success = false;
        }

        if !result.errors.is_empty() && self.should_halt(&result) {
            return result;
        }

        result.is_success = result.errors.is_empty();

        if self.operational_settings.debug_mode >= DebugMode::Regular {
            let status = if result.is_success { "SUCCESS" } else { "FAILURE" };
            self.error_manager.log_info(&format!(
                "QUICKFUNCS analysis complete: {}",
                status
            ));
            self.error_manager.log_info(&format!(
                "  Functions validated: {}",
                function_count - duplicate_functions.len()
            ));
            self.error_manager.log_info(&format!(
                "  Errors: {}, Warnings: {}",
                result.errors.len(),
                result.warnings.len()
            ));
        }

        result
    }

    // ==================== FUNCTION VALIDATION ====================

    fn validate_quick_function(
        &mut self,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
    ) {
        if self.operational_settings.debug_mode >= DebugMode::Verbose {
            self.error_manager.log_debug(&format!("Validating function: {}", func.name));
        }

        // Validate function name
        if !Self::is_valid_identifier(&func.name) {
            self.add_error(
                result,
                "QFUNC002",
                "INVALID_FUNCTION_NAME",
                &format!("Function name '{}' is not a valid identifier", func.name),
                "Function names must start with a letter and contain only alphanumeric characters and underscores",
                func.position,
            );
            return;
        }

        // Check reserved keywords
        if Keywords::is_reserved_in_context(&func.name, "QUICKFUNCS") {
            self.add_error(
                result,
                "QFUNC002B",
                "RESERVED_KEYWORD_AS_NAME",
                &Keywords::get_keyword_usage_error(&func.name, "QUICKFUNCS"),
                &format!("Choose a different name for function '{}'", func.name),
                func.position,
            );
            return;
        }

        // Validate return type
        if func.return_type.is_none() {
            self.add_error(
                result,
                "QFUNC003",
                "MISSING_RETURN_TYPE",
                &format!("Function '{}' must specify a return type", func.name),
                &format!("Add return type: ~{}<int> or ~{}<bool>, etc.", func.name, func.name),
                func.position,
            );
            return;
        }

        self.validate_return_type(func, result);
        if self.should_halt(result) {
            return;
        }

        self.validate_parameters(func, symbol_table, result);
        if self.should_halt(result) {
            return;
        }

        self.validate_scopes(func, symbol_table, result);
        if self.should_halt(result) {
            return;
        }

        self.validate_function_body(func, symbol_table, result);
        if self.should_halt(result) {
            return;
        }

        if self.operational_settings.debug_mode >= DebugMode::Verbose {
            self.error_manager.log_debug(&format!(
                "Function '{}' validation complete",
                func.name
            ));
        }
    }

    fn validate_return_type(
        &self,
        func: &QuickFunction,
        result: &mut SectionAnalysisResult,
    ) {
        if let Some(return_type) = func.return_type {
            if !VALID_DATA_TYPES.contains(&return_type) {
                self.add_error(
                    result,
                    "QFUNC003B",
                    "INVALID_RETURN_TYPE",
                    &format!(
                        "Function '{}' has invalid return type: {:?}",
                        func.name, return_type
                    ),
                    "Use a valid data type: int, float, double, string, bool, array, tuple, object, etc.",
                    func.position,
                );
            }
        }
    }

    fn validate_parameters(
        &self,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
    ) {
        let param_count = func.parameters.len();

        if param_count > MAX_FUNCTION_PARAMETERS {
            self.add_error(
                result,
                "QFUNC004",
                "TOO_MANY_PARAMETERS",
                &format!(
                    "Function '{}' has {} parameters, exceeds limit of {}",
                    func.name, param_count, MAX_FUNCTION_PARAMETERS
                ),
                &format!(
                    "Reduce the number of parameters to {} or fewer",
                    MAX_FUNCTION_PARAMETERS
                ),
                func.position,
            );
            return;
        }

        let mut param_names = FxHashSet::with_capacity_and_hasher(
            param_count,
            Default::default()
        );
        let mut duplicate_params = FxHashSet::default();

        // Check for duplicate parameter names
        for param in &func.parameters {
            if !param_names.insert(&param.name) {
                duplicate_params.insert(&param.name);
                self.add_error(
                    result,
                    "QFUNC005",
                    "DUPLICATE_PARAMETER_NAME",
                    &format!(
                        "Parameter '{}' is defined multiple times in function '{}'",
                        param.name, func.name
                    ),
                    "Each parameter must have a unique name",
                    param.position,
                );

                if self.should_halt(result) {
                    return;
                }
            }
        }

        let mut seen_default = false;

        // Validate each parameter
        for param in &func.parameters {
            if duplicate_params.contains(&param.name) {
                if self.operational_settings.debug_mode >= DebugMode::Verbose {
                    self.error_manager.log_debug(&format!(
                        "  Skipping validation of duplicate parameter '{}'",
                        param.name
                    ));
                }
                continue;
            }

            // Check valid identifier
            if !Self::is_valid_identifier(&param.name) {
                self.add_error(
                    result,
                    "QFUNC006",
                    "INVALID_PARAMETER_NAME",
                    &format!(
                        "Parameter '{}' in function '{}' is not a valid identifier",
                        param.name, func.name
                    ),
                    "Parameter names must start with a letter and contain only alphanumeric characters and underscores",
                    param.position,
                );

                if self.should_halt(result) {
                    return;
                }
                continue;
            }

            // Check if parameter name is a data type keyword
            if Keywords::is_data_type_keyword(&param.name) {
                let suggestion = format!(
                    "Use a different name like 'my{}{}' or '{}Value'",
                    param.name.chars().next().unwrap().to_uppercase(),
                    &param.name[1..],
                    param.name
                );

                self.add_error(
                    result,
                    "QFUNC006C",
                    "DATA_TYPE_KEYWORD_AS_PARAMETER",
                    &format!(
                        "Parameter '{}' in function '{}' cannot use data type keyword as name",
                        param.name, func.name
                    ),
                    &suggestion,
                    param.position,
                );

                if self.should_halt(result) {
                    return;
                }
                continue;
            }

            // Check reserved keywords
            if Keywords::is_reserved_in_context(&param.name, "QUICKFUNCS") {
                self.add_error(
                    result,
                    "QFUNC006B",
                    "RESERVED_KEYWORD_AS_PARAMETER",
                    &Keywords::get_keyword_usage_error(&param.name, "QUICKFUNCS"),
                    &format!(
                        "Choose a different name for parameter '{}' in function '{}'",
                        param.name, func.name
                    ),
                    param.position,
                );

                if self.should_halt(result) {
                    return;
                }
                continue;
            }

            // Validate parameter type
            if let Some(param_type) = param.data_type {
                if !VALID_DATA_TYPES.contains(&param_type) {
                    self.add_error(
                        result,
                        "QFUNC007",
                        "INVALID_PARAMETER_TYPE",
                        &format!(
                            "Parameter '{}' in function '{}' has invalid type: {:?}",
                            param.name, func.name, param_type
                        ),
                        "Use a valid data type: int, float, double, string, bool, array, tuple, object, etc.",
                        param.position,
                    );

                    if self.should_halt(result) {
                        return;
                    }
                }
            }

            // Validate default value type if both type and default are present
            if let (Some(_), Some(ref default_value)) = (param.data_type, &param.default_value) {
                self.validate_default_value_type_strict(
                    param,
                    &func.name,
                    symbol_table,
                    result,
                );
            }

            // Check parameter ordering (defaults must come last)
            if param.default_value.is_some() {
                seen_default = true;
            } else if seen_default {
                self.add_error(
                    result,
                    "QFUNC008",
                    "PARAMETER_ORDER_VIOLATION",
                    &format!(
                        "Non-default parameter '{}' cannot follow default parameters in function '{}'",
                        param.name, func.name
                    ),
                    "Place all parameters with default values at the end of the parameter list",
                    param.position,
                );

                if self.should_halt(result) {
                    return;
                }
            }
        }
    }

    fn validate_default_value_type_strict(
        &self,
        param: &QuickFuncParam,
        func_name: &str,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
    ) {
        let default_value = match &param.default_value {
            Some(v) => v,
            None => return,
        };

        let expected_type = match param.data_type {
            Some(t) => t,
            None => return,
        };

        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
        let inferred_type = type_inference_visitor.infer_type_from_expression(default_value);

        if let Some(inferred) = inferred_type {
            if !Self::are_types_compatible_strict(inferred, expected_type) {
                self.add_error(
                    result,
                    "QFUNC009",
                    "DEFAULT_VALUE_TYPE_MISMATCH",
                    &format!(
                        "Default value type ({:?}) does not match parameter type ({:?}) for '{}' in function '{}'",
                        inferred, expected_type, param.name, func_name
                    ),
                    &format!(
                        "Change default value to match type {:?} or remove type annotation",
                        expected_type
                    ),
                    param.position,
                );
            }
        } else {
            self.add_warning(
                result,
                "QFUNC_WARN003",
                &format!(
                    "Cannot infer type for default value of parameter '{}' in function '{}'",
                    param.name, func_name
                ),
                "QUICKFUNCS",
                param.position,
            );
        }
    }

    fn validate_scopes(
        &self,
        func: &QuickFunction,
        _symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
    ) {
        let scope_list = match &func.scope_list {
            Some(scopes) => scopes,
            None => {
                if self.operational_settings.debug_mode >= DebugMode::Verbose {
                    self.error_manager.log_debug(&format!(
                        "  Function '{}' has no explicit scope declaration",
                        func.name
                    ));
                }
                self.add_warning(
                    result,
                    "QFUNC_WARN006",
                    &format!(
                        "Function '{}' has no scope declaration - will only be callable within its definition context",
                        func.name
                    ),
                    "QUICKFUNCS",
                    func.position,
                );
                return;
            }
        };

        for scope in scope_list {
            if scope.eq_ignore_ascii_case("global") {
                if self.operational_settings.debug_mode >= DebugMode::Verbose {
                    self.error_manager.log_debug(
                        "    Scope 'global' is valid - function callable from anywhere"
                    );
                }
                continue;
            }

            if !Self::is_valid_data_path(scope) {
                self.add_error(
                    result,
                    "QFUNC010",
                    "INVALID_SCOPE_SYNTAX",
                    &format!(
                        "Function '{}' has invalid scope syntax: '{}'",
                        func.name, scope
                    ),
                    "Scope must be 'global' or a valid dotted path (e.g., 'user.profile', 'server.config')",
                    func.position,
                );

                if self.should_halt(result) {
                    return;
                }
                continue;
            }

            if self.operational_settings.debug_mode >= DebugMode::Verbose {
                self.error_manager.log_debug(&format!(
                    "    Scope '{}' has valid syntax (existence will be verified in DATA section)",
                    scope
                ));
            }
        }

        if self.operational_settings.debug_mode >= DebugMode::Verbose {
            self.error_manager.log_debug(&format!(
                "  Function '{}' scope validation complete: {} scope(s) declared",
                func.name,
                scope_list.len()
            ));
        }
    }

    // ... continues in Part 2
