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
use crate::Compiler::AST::Visitors::{TypeInferenceVisitor};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use crate::Compiler::Core::Functions::CycleDetectionValidator;
use crate::Compiler::Core::SectionAnalyzers::{
    SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo
};
use crate::Compiler::Utilities::{SymbolTable, ParameterInfo, FunctionSignature};
use crate::Builtins::Core::DixType;
use crate::Builtins::Resolver::{
    has_instance_method, has_static_method, has_static_object,
};
use crate::Utilities::Keywords;
use crate::ErrorManager::ErrorManager;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashMap;

// ==================== CONSTANTS ====================

const MAX_ABSOLUTE_VALIDATION_DEPTH: usize = 500;
const BASE_VALIDATION_DEPTH: usize = 100;
const MAX_TUPLE_ARGUMENTS: usize = 6;
const MAX_ARRAY_ELEMENTS: usize = 10000;
const MAX_OBJECT_PROPERTIES: usize = 1000;
const MAX_FUNCTION_PARAMETERS: usize = 100;
const MAX_FUNCTION_BODY_STATEMENTS: usize = 1000;
const MAX_NESTING_DEPTH: usize = 50;
const MAX_METHOD_CHAIN_DEPTH: usize = 10;

// ==================== RUNTIME VALIDATION SETS (NOT PHF) ====================

/// Check if operator is valid arithmetic operator
#[inline]
fn is_valid_arithmetic_operator(op: &str) -> bool {
    matches!(op, "+" | "-" | "*" | "/" | "%" | "**" | "%%" | "%&" | "&%")
}

/// Check if operator is valid bitwise operator
#[inline]
fn is_valid_bitwise_operator(op: &str) -> bool {
    matches!(op, "&" | "|" | "^" | "<<" | ">>")
}

/// Check if operator is valid comparison operator
#[inline]
fn is_valid_comparison_operator(op: &str) -> bool {
    matches!(op, "==" | "!=" | ">" | "<" | ">=" | "<=")
}

/// Check if operator is valid logical operator
#[inline]
fn is_valid_logical_operator(op: &str) -> bool {
    matches!(op, "&&" | "||" | "and" | "or")
}

/// Check if operator is valid unary operator
#[inline]
fn is_valid_unary_operator(op: &str) -> bool {
    matches!(op, "!" | "not" | "-" | "+" | "~?")
}

/// Check if operator is valid arithmetic assignment operator
#[inline]
fn is_valid_arithmetic_assign_op(op: &str) -> bool {
    matches!(op, "+=" | "-=" | "*=" | "/=" | "%=" | "**=" | "&=" | "|=" | "^=" | "<<=" | ">>=")
}

/// Check if data type is valid
#[inline]
fn is_valid_data_type(data_type: DataType) -> bool {
    matches!(
        data_type,
        DataType::Int | DataType::Float | DataType::Double | DataType::String |
        DataType::Bool | DataType::Array | DataType::Tuple | DataType::Hex |
        DataType::Blob | DataType::Regex | DataType::Object | DataType::Timestamp |
        DataType::Date | DataType::Enum | DataType::Any | DataType::Function |
        DataType::Range
    )
}

// ==================== MAIN ANALYZER ====================

/// QuickFunctions section semantic analyzer
pub struct QuickFuncsSectionAnalyzer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    // Cached logging flags (set once, checked many times)
    is_debug: bool,
    is_verbose: bool,
}

impl<'a> QuickFuncsSectionAnalyzer<'a> {
    /// Create new analyzer
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        let is_debug = operational_settings.debug_mode >= DebugMode::Regular;
        let is_verbose = operational_settings.debug_mode >= DebugMode::Verbose;

        QuickFuncsSectionAnalyzer {
            operational_settings,
            error_manager: ErrorManager::get_shared_instance(),
            is_debug,
            is_verbose,
        }
    }

    /// Calculate max validation depth based on AST size
    #[inline]
    fn calculate_max_depth(ast_size: usize) -> usize {
        let dynamic_depth = BASE_VALIDATION_DEPTH + (ast_size / 10);
        dynamic_depth.min(MAX_ABSOLUTE_VALIDATION_DEPTH)
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

        if self.is_debug {
            self.error_manager.log_info(&format!(
                "Analyzing QUICKFUNCS section with {} function definitions",
                function_count
            ));
        }

        // Phase 1: Check for duplicate function names
        if self.is_debug {
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
        if self.is_debug {
            self.error_manager.log_debug("Phase 2: Pre-registering all functions in symbol table");
        }

        self.populate_symbol_table(section, symbol_table, &duplicate_functions, &mut result);

        if !result.errors.is_empty() && self.should_halt(&result) {
            return result;
        }

        // Phase 3: Validate individual function declarations
        if self.is_debug {
            self.error_manager.log_debug("Phase 3: Validating individual function declarations");
        }

        for func in &section.functions {
            if duplicate_functions.contains(&func.name) {
                if self.is_verbose {
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
        if self.is_debug {
            self.error_manager.log_debug("Phase 4: Detecting circular function calls");
        }

        let cycle_validator = CycleDetectionValidator::new(
            self.error_manager.clone(),
            self.operational_settings.clone(),
        );

        if !cycle_validator.validate_function_calls(section) {
            result.is_success = false;
        }

        if !result.errors.is_empty() && self.should_halt(&result) {
            return result;
        }

        result.is_success = result.errors.is_empty();

        if self.is_debug {
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
        &self,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
    ) {
        if self.is_verbose {
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

        self.validate_scopes(func, result);
        if self.should_halt(result) {
            return;
        }

        self.validate_function_body(func, symbol_table, result);
        if self.should_halt(result) {
            return;
        }

        if self.is_verbose {
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
            if !is_valid_data_type(return_type) {
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
                if self.is_verbose {
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
                if !is_valid_data_type(param_type) {
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
            if param.data_type.is_some() && param.default_value.is_some() {
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
        result: &mut SectionAnalysisResult,
    ) {
        let scope_list = match &func.scope_list {
            Some(scopes) => scopes,
            None => {
                if self.is_verbose {
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
                if self.is_verbose {
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

            if self.is_verbose {
                self.error_manager.log_debug(&format!(
                    "    Scope '{}' has valid syntax (existence will be verified in DATA section)",
                    scope
                ));
            }
        }

        if self.is_verbose {
            self.error_manager.log_debug(&format!(
                "  Function '{}' scope validation complete: {} scope(s) declared",
                func.name,
                scope_list.len()
            ));
        }
    }

    fn validate_function_body(
        &self,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
    ) {
        let body_length = func.body.len();

        if body_length == 0 {
            self.add_error(
                result,
                "QFUNC011",
                "EMPTY_FUNCTION_BODY",
                &format!(
                    "Function '{}' has an empty body but declares return type {:?}",
                    func.name,
                    func.return_type.unwrap()
                ),
                "Add function body with return statement or remove function",
                func.position,
            );
            return;
        }

        if body_length > MAX_FUNCTION_BODY_STATEMENTS {
            self.add_error(
                result,
                "QFUNC012",
                "FUNCTION_BODY_TOO_LARGE",
                &format!(
                    "Function '{}' has {} statements, exceeds limit of {}",
                    func.name, body_length, MAX_FUNCTION_BODY_STATEMENTS
                ),
                &format!(
                    "Reduce the function body to {} or fewer statements",
                    MAX_FUNCTION_BODY_STATEMENTS
                ),
                func.position,
            );
            return;
        }

        let mut local_scope = LocalScopeTracker::new(&func.parameters);
        let mut return_path_analyzer = ReturnPathAnalyzer::new(func.return_type.unwrap());

        // Calculate max depth based on function body size
        let max_depth = Self::calculate_max_depth(body_length);

        for statement in &func.body {
            self.validate_statement(
                statement,
                func,
                symbol_table,
                &mut local_scope,
                result,
                0,
                max_depth,
                &mut return_path_analyzer,
            );

            if self.should_halt(result) {
                return;
            }
        }

        if !return_path_analyzer.all_paths_return() {
            self.add_error(
                result,
                "QFUNC013",
                "NOT_ALL_PATHS_RETURN",
                &format!(
                    "Function '{}' with return type {:?} does not return a value on all code paths",
                    func.name,
                    func.return_type.unwrap()
                ),
                "Ensure all branches (if/else, switch cases) have return statements",
                func.position,
            );
        }

        self.check_for_unused_variables(func, &local_scope, result);
    }

    fn validate_statement(
        &self,
        statement: &QuickFuncStatement,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &mut LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        nesting_depth: usize,
        max_depth: usize,
        return_path_analyzer: &mut ReturnPathAnalyzer,
    ) {
        if nesting_depth > max_depth {
            self.add_error(
                result,
                "QFUNC073",
                "VALIDATION_DEPTH_EXCEEDED",
                &format!(
                    "Maximum validation depth ({}) exceeded in function '{}'",
                    max_depth, func.name
                ),
                "This indicates very deep nesting - please simplify your code structure",
                statement.position(),
            );
            return;
        }

        if nesting_depth > MAX_NESTING_DEPTH {
            self.add_error(
                result,
                "QFUNC014",
                "NESTING_TOO_DEEP",
                &format!(
                    "Function '{}' has nesting depth exceeding {}",
                    func.name, MAX_NESTING_DEPTH
                ),
                "Reduce nesting depth by extracting code into separate functions",
                statement.position(),
            );
            return;
        }

        match statement {
            QuickFuncStatement::Return { value, .. } => {
                self.validate_return_statement(value, func, symbol_table, local_scope, result);
                return_path_analyzer.add_return();
            }

            QuickFuncStatement::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.validate_if_statement(
                    condition,
                    then_branch,
                    else_branch.as_ref(),
                    func,
                    symbol_table,
                    local_scope,
                    result,
                    nesting_depth,
                    max_depth,
                    return_path_analyzer,
                );
            }

            QuickFuncStatement::Switch {
                expression,
                cases,
                default_case,
                ..
            } => {
                self.validate_switch_statement(
                    expression,
                    cases,
                    default_case.as_ref(),
                    func,
                    symbol_table,
                    local_scope,
                    result,
                    nesting_depth,
                    max_depth,
                    return_path_analyzer,
                );
            }

            QuickFuncStatement::Assignment { variable, value, .. } => {
                self.validate_assignment_statement(variable, value, func, symbol_table, local_scope, result);
            }

            QuickFuncStatement::ArithmeticAssignment {
                variable,
                operator,
                value,
                ..
            } => {
                self.validate_arithmetic_assignment_statement(
                    variable,
                    operator,
                    value,
                    func,
                    symbol_table,
                    local_scope,
                    result,
                );
            }

            QuickFuncStatement::ObjectCreation { variable, object, .. } => {
                self.validate_object_creation_statement(variable, object, func, symbol_table, local_scope, result);
            }

            QuickFuncStatement::Log { value, .. } => {
                self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);
            }

            QuickFuncStatement::ExpressionStatement { expression, .. } => {
                self.validate_expression(expression, func, symbol_table, local_scope, result, max_depth);
            }

            QuickFuncStatement::VariableDeclaration { .. } => {
                self.validate_variable_declaration_statement(statement, func, symbol_table, local_scope, result);
            }
        }
    }

    fn validate_return_statement(
        &self,
        value: &Expression,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
    ) {
        let max_depth = Self::calculate_max_depth(100);
        self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);

        let local_variable_types = local_scope.get_all_variable_types();
        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, Some(local_variable_types));

        let return_value_type = type_inference_visitor.infer_type_from_expression(value);
        let expected_return_type = func.return_type.unwrap();

        if let Some(actual_type) = return_value_type {
            if !Self::are_types_compatible_strict(actual_type, expected_return_type) {
                self.add_error(
                    result,
                    "QFUNC015",
                    "RETURN_TYPE_MISMATCH",
                    &format!(
                        "Function '{}' returns {:?} but declared return type is {:?}",
                        func.name, actual_type, expected_return_type
                    ),
                    &format!(
                        "Change return value to match {:?} or update function return type",
                        expected_return_type
                    ),
                    value.position(),
                );
            } else if self.is_verbose {
                self.error_manager.log_debug(&format!(
                    "    Return type {:?} matches expected {:?}",
                    actual_type, expected_return_type
                ));
            }
        } else {
            self.add_warning(
                result,
                "QFUNC_WARN004",
                &format!(
                    "Unable to infer return type in function '{}'. Expected type: {:?}",
                    func.name, expected_return_type
                ),
                "QUICKFUNCS",
                value.position(),
            );
        }
    }

    fn validate_variable_declaration_statement(
        &self,
        statement: &QuickFuncStatement,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &mut LocalScopeTracker,
        result: &mut SectionAnalysisResult,
    ) {
        let (declaration_type, is_mutable, variable_name, data_type, value, position) = match statement {
            QuickFuncStatement::VariableDeclaration {
                declaration_type,
                is_mutable,
                variable_name,
                data_type,
                value,
                position,
            } => (declaration_type, is_mutable, variable_name, data_type, value, position),
            _ => return,
        };

        // Validate variable name
        if !Self::is_valid_identifier(variable_name) {
            self.add_error(
                result,
                "QFUNC067",
                "INVALID_VARIABLE_NAME",
                &format!(
                    "Invalid variable name '{}' in function '{}'",
                    variable_name, func.name
                ),
                "Variable names must start with a letter and contain only alphanumeric characters and underscores",
                *position,
            );
            return;
        }

        // Check if variable name is a data type keyword
        if Keywords::is_data_type_keyword(variable_name) {
            let suggestion = format!(
                "Use a different name like 'my{}{}' or '{}Value'",
                variable_name.chars().next().unwrap().to_uppercase(),
                &variable_name[1..],
                variable_name
            );

            self.add_error(
                result,
                "QFUNC067B",
                "DATA_TYPE_KEYWORD_AS_VARIABLE",
                &format!(
                    "Variable '{}' in function '{}' cannot use data type keyword as name",
                    variable_name, func.name
                ),
                &suggestion,
                *position,
            );
            return;
        }

        // Check reserved keywords
        if Keywords::is_reserved_in_context(variable_name, "QUICKFUNCS") {
            self.add_error(
                result,
                "QFUNC068",
                "RESERVED_KEYWORD_AS_VARIABLE",
                &Keywords::get_keyword_usage_error(variable_name, "QUICKFUNCS"),
                &format!("Choose a different name for variable '{}'", variable_name),
                *position,
            );
            return;
        }

        // Check for redeclaration
        if local_scope.has_variable(variable_name) {
            self.add_error(
                result,
                "QFUNC069",
                "VARIABLE_REDECLARATION",
                &format!(
                    "Variable '{}' already declared in function '{}'",
                    variable_name, func.name
                ),
                "Each variable must be declared only once. Use assignment to change its value.",
                *position,
            );
            return;
        }

        // Validate value expression
        if !Self::is_valid_value_expression(value) {
            self.add_error(
                result,
                "QFUNC070",
                "INVALID_VARIABLE_VALUE",
                &format!("Invalid expression in variable declaration for '{}'", variable_name),
                "Variable declarations cannot contain assignment operations like +=, -=, etc.",
                *position,
            );
            return;
        }

        let max_depth = Self::calculate_max_depth(100);
        self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);

        // Type inference
        let local_variable_types = local_scope.get_all_variable_types();
        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, Some(local_variable_types));
        let inferred_type = type_inference_visitor.infer_type_from_expression(value);

        // Check type compatibility if both declared and inferred
        if let (Some(declared), Some(inferred)) = (data_type, inferred_type) {
            if !Self::are_types_compatible_strict(inferred, *declared) {
                self.add_error(
                    result,
                    "QFUNC071",
                    "VARIABLE_TYPE_MISMATCH",
                    &format!(
                        "Variable '{}' declared as {:?} but assigned value of type {:?}",
                        variable_name, declared, inferred
                    ),
                    &format!(
                        "Change the value to match type {:?} or remove type annotation",
                        declared
                    ),
                    *position,
                );
            }
        }

        let is_const = matches!(declaration_type, DeclarationType::Const) || !is_mutable;
        let effective_type = data_type.or(inferred_type);

        local_scope.add_variable(variable_name.clone(), effective_type, is_const);

        if self.is_verbose {
            let mutability = if is_const { "immutable" } else { "mutable" };
            let type_str = if let Some(t) = effective_type {
                format!("{:?}", t)
            } else {
                "inferred".to_string()
            };

            self.error_manager.log_debug(&format!(
                "    Declared {} variable '{}' with type {}",
                mutability, variable_name, type_str
            ));
        }
    }

    fn validate_if_statement(
        &self,
        condition: &Expression,
        then_branch: &[QuickFuncStatement],
        else_branch: Option<&Vec<QuickFuncStatement>>,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &mut LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        nesting_depth: usize,
        max_depth: usize,
        return_path_analyzer: &mut ReturnPathAnalyzer,
    ) {
        self.validate_expression(condition, func, symbol_table, local_scope, result, max_depth);

        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
        let condition_type = type_inference_visitor.infer_type_from_expression(condition);

        if let Some(cond_type) = condition_type {
            if cond_type != DataType::Bool {
                self.add_error(
                    result,
                    "QFUNC016",
                    "NON_BOOLEAN_CONDITION",
                    &format!("If statement condition must be boolean, got {:?}", cond_type),
                    "Use comparison operators (==, !=, >, <, etc.) to create boolean conditions",
                    condition.position(),
                );
            }
        }

        let mut then_returns = ReturnPathAnalyzer::new(func.return_type.unwrap());
        let mut else_returns = ReturnPathAnalyzer::new(func.return_type.unwrap());

        for stmt in then_branch {
            self.validate_statement(
                stmt,
                func,
                symbol_table,
                local_scope,
                result,
                nesting_depth + 1,
                max_depth,
                &mut then_returns,
            );
        }

        if let Some(else_stmts) = else_branch {
            for stmt in else_stmts {
                self.validate_statement(
                    stmt,
                    func,
                    symbol_table,
                    local_scope,
                    result,
                    nesting_depth + 1,
                    max_depth,
                    &mut else_returns,
                );
            }

            if then_returns.all_paths_return() && else_returns.all_paths_return() {
                return_path_analyzer.add_return();
            }
        }
    }

    fn validate_switch_statement(
        &self,
        expression: &Expression,
        cases: &[SwitchCase],
        default_case: Option<&SwitchCase>,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &mut LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        nesting_depth: usize,
        max_depth: usize,
        return_path_analyzer: &mut ReturnPathAnalyzer,
    ) {
        self.validate_expression(expression, func, symbol_table, local_scope, result, max_depth);

        let mut case_returns = Vec::new();
        let has_default = default_case.is_some();

        for case in cases {
            let mut case_analyzer = ReturnPathAnalyzer::new(func.return_type.unwrap());

            for stmt in &case.statements {
                self.validate_statement(
                    stmt,
                    func,
                    symbol_table,
                    local_scope,
                    result,
                    nesting_depth + 1,
                    max_depth,
                    &mut case_analyzer,
                );
            }

            case_returns.push(case_analyzer);
        }

        let mut default_analyzer = None;
        if let Some(default) = default_case {
            let mut analyzer = ReturnPathAnalyzer::new(func.return_type.unwrap());

            for stmt in &default.statements {
                self.validate_statement(
                    stmt,
                    func,
                    symbol_table,
                    local_scope,
                    result,
                    nesting_depth + 1,
                    max_depth,
                    &mut analyzer,
                );
            }

            default_analyzer = Some(analyzer);
        }

        let all_cases_return = case_returns.iter().all(|r| r.all_paths_return());
        let default_returns = has_default && default_analyzer.as_ref().unwrap().all_paths_return();

        if all_cases_return && default_returns {
            return_path_analyzer.add_return();
        }
    }

    fn validate_assignment_statement(
        &self,
        variable: &str,
        value: &Expression,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &mut LocalScopeTracker,
        result: &mut SectionAnalysisResult,
    ) {
        if !Self::is_valid_identifier(variable) {
            self.add_error(
                result,
                "QFUNC017",
                "INVALID_VARIABLE_NAME",
                &format!(
                    "Invalid variable name '{}' in function '{}'",
                    variable, func.name
                ),
                "Variable names must start with a letter and contain only alphanumeric characters and underscores",
                value.position(),
            );
            return;
        }

        if !local_scope.has_variable(variable) {
            self.add_error(
                result,
                "QFUNC072",
                "UNDECLARED_VARIABLE",
                &format!(
                    "Variable '{}' used before declaration in function '{}'",
                    variable, func.name
                ),
                &format!(
                    "Declare variable first: let {} = ...; or const {} = ...;",
                    variable, variable
                ),
                value.position(),
            );
            return;
        }

        if local_scope.is_const(variable) {
            self.add_error(
                result,
                "QFUNC018",
                "CONST_REASSIGNMENT",
                &format!(
                    "Cannot reassign const variable '{}' in function '{}'",
                    variable, func.name
                ),
                "Use 'let mut' instead of 'const' or 'let' to make variable mutable",
                value.position(),
            );
            return;
        }

        let max_depth = Self::calculate_max_depth(100);
        self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);

        let local_variable_types = local_scope.get_all_variable_types();
        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, Some(local_variable_types));

        let existing_type = local_scope.get_variable_type(variable);
        let new_type = type_inference_visitor.infer_type_from_expression(value);

        if let (Some(existing), Some(new)) = (existing_type, new_type) {
            if !Self::are_types_compatible_strict(new, existing) {
                self.add_error(
                    result,
                    "QFUNC019",
                    "TYPE_MISMATCH_REASSIGNMENT",
                    &format!(
                        "Cannot assign {:?} to variable '{}' of type {:?}",
                        new, variable, existing
                    ),
                    "Variable types cannot change once assigned (unless type is 'any')",
                    value.position(),
                );
            }
        } else if existing_type.is_none() {
            if let Some(new) = new_type {
                local_scope.update_variable_type(variable, new);

                if self.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "    Inferred type {:?} for variable '{}'",
                        new, variable
                    ));
                }
            }
        }
    }

    fn validate_arithmetic_assignment_statement(
        &self,
        variable: &str,
        operator: &str,
        value: &Expression,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
    ) {
        if !local_scope.has_variable(variable) {
            self.add_error(
                result,
                "QFUNC020",
                "UNDEFINED_VARIABLE",
                &format!(
                    "Variable '{}' used before assignment in function '{}'",
                    variable, func.name
                ),
                "Declare variable before using in arithmetic assignment",
                value.position(),
            );
            return;
        }

        if local_scope.is_const(variable) {
            self.add_error(
                result,
                "QFUNC021",
                "CONST_REASSIGNMENT",
                &format!(
                    "Cannot modify const variable '{}' with {}",
                    variable, operator
                ),
                "Remove 'const' keyword to make variable mutable",
                value.position(),
            );
            return;
        }

        if !is_valid_arithmetic_assign_op(operator) {
            self.add_error(
                result,
                "QFUNC022",
                "INVALID_ARITHMETIC_ASSIGN_OP",
                &format!("Invalid arithmetic assignment operator '{}'", operator),
                "Valid operators: +=, -=, *=, /=, %=, **=, &=, |=, ^=, <<=, >>=",
                value.position(),
            );
            return;
        }

        let max_depth = Self::calculate_max_depth(100);
        self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);

        let var_type = local_scope.get_variable_type(variable);
        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
        let value_type = type_inference_visitor.infer_type_from_expression(value);

        if let (Some(var_t), Some(val_t)) = (var_type, value_type) {
            self.validate_arithmetic_operation(operator, var_t, val_t, &func.name, result, value.position());
        }
    }

    fn validate_object_creation_statement(
        &self,
        variable: &str,
        object: &Value,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &mut LocalScopeTracker,
        result: &mut SectionAnalysisResult,
    ) {
        if !Self::is_valid_identifier(variable) {
            self.add_error(
                result,
                "QFUNC023",
                "INVALID_VARIABLE_NAME",
                &format!(
                    "Invalid variable name '{}' in function '{}'",
                    variable, func.name
                ),
                "Variable names must start with a letter and contain only alphanumeric characters and underscores",
                object.position(),
            );
            return;
        }

        if local_scope.is_const(variable) {
            self.add_error(
                result,
                "QFUNC024",
                "CONST_REASSIGNMENT",
                &format!(
                    "Cannot reassign const variable '{}' in function '{}'",
                    variable, func.name
                ),
                "Remove 'const' keyword to make variable mutable",
                object.position(),
            );
            return;
        }

        self.validate_object_literal_keys(object, &func.name, result);
        self.validate_value(object, func, symbol_table, local_scope, result);

        if !local_scope.has_variable(variable) {
            local_scope.add_variable(variable.to_string(), Some(DataType::Object), false);
        }
    }

    fn check_for_unused_variables(
        &self,
        func: &QuickFunction,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
    ) {
        let mut collector = VariableReferenceCollector::new(&func.parameters);
        let referenced_variables = collector.collect_from_function(func);

        for var_name in local_scope.get_declared_variable_names() {
            if !referenced_variables.contains(var_name) {
                self.add_warning(
                    result,
                    "QFUNC_WARN005",
                    &format!(
                        "Variable '{}' declared but never used in function '{}'",
                        var_name, func.name
                    ),
                    "QUICKFUNCS",
                    func.position,
                );

                if self.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "    Unused variable detected: '{}'",
                        var_name
                    ));
                }
            }
        }

        if self.is_verbose {
            let declared_count = local_scope.get_declared_variable_names().count();
            let used_count = referenced_variables.len();
            self.error_manager.log_debug(&format!(
                "  Variable usage: {}/{} variables used",
                used_count, declared_count
            ));
        }
    }

    // ==================== EXPRESSION VALIDATION ====================

    fn validate_expression(
        &self,
        expr: &Expression,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if max_depth == 0 {
            self.add_error(
                result,
                "QFUNC074",
                "EXPRESSION_DEPTH_EXCEEDED",
                &format!(
                    "Maximum expression depth exceeded in function '{}'",
                    func.name
                ),
                "This indicates a circular expression - please simplify your expressions",
                expr.position(),
            );
            return;
        }

        match expr {
            Expression::Identifier { name, .. } => {
                self.validate_identifier(name, &func.name, local_scope, symbol_table, result, expr.position());
            }

            Expression::QualifiedIdentifier { parts, arguments, .. } => {
                self.validate_qualified_identifier(parts, arguments.as_ref(), func, symbol_table, local_scope, result, max_depth);
            }

            Expression::QuickFuncCall { name, arguments, .. } => {
                self.validate_quick_func_call(name, arguments, func, symbol_table, local_scope, result, max_depth);
            }

            Expression::ImportedFunctionCall {
                namespace_name,
                function_name,
                arguments,
                ..
            } => {
                self.validate_imported_function_call(
                    namespace_name,
                    function_name,
                    arguments,
                    func,
                    symbol_table,
                    local_scope,
                    result,
                    max_depth,
                );
            }

            Expression::InstanceMethodCall {
                instance,
                method_name,
                arguments,
                ..
            } => {
                self.validate_instance_method_call(instance, method_name, arguments, func, symbol_table, local_scope, result, max_depth);
            }

            Expression::StaticMethodCall {
                object_name,
                method_name,
                arguments,
                ..
            } => {
                self.validate_static_method_call(object_name, method_name, arguments, func, symbol_table, local_scope, result, max_depth);
            }

            Expression::EnumAccess {
                namespace_name,
                enum_name,
                value,
                position,
            } => {
                self.validate_enum_access(namespace_name.as_deref(), enum_name, value, &func.name, symbol_table, result, *position);
            }

            Expression::ArithmeticOp { left, right, operator, .. } => {
                self.validate_arithmetic_op_expression(left, right, operator, func, symbol_table, local_scope, result, max_depth);
            }

            Expression::BitwiseOp { left, right, operator, .. } => {
                self.validate_bitwise_op_expression(left, right, operator, func, symbol_table, local_scope, result, max_depth);
            }

            Expression::ComparisonOp { left, right, operator, .. } => {
                self.validate_comparison_op_expression(left, right, operator, func, symbol_table, local_scope, result, max_depth);
            }

            Expression::LogicalOp { left, right, operator, .. } => {
                self.validate_logical_op_expression(left, right, operator, func, symbol_table, local_scope, result, max_depth);
            }

            Expression::UnaryOp { operand, operator, .. } => {
                self.validate_unary_op_expression(operand, operator, func, symbol_table, local_scope, result, max_depth);
            }

            Expression::Conditional {
                condition,
                true_value,
                false_value,
                ..
            } => {
                self.validate_conditional_expression(condition, true_value, false_value, func, symbol_table, local_scope, result, max_depth);
            }

            Expression::PropertyAccess { object, .. } => {
                self.validate_expression(object, func, symbol_table, local_scope, result, max_depth - 1);
            }

            Expression::IndexAccess { object, index, .. } => {
                self.validate_expression(object, func, symbol_table, local_scope, result, max_depth - 1);
                self.validate_expression(index, func, symbol_table, local_scope, result, max_depth - 1);
            }

            Expression::Value { value, .. } => {
                self.validate_value(value, func, symbol_table, local_scope, result);
            }

            Expression::Parenthesized { expression, .. } => {
                self.validate_expression(expression, func, symbol_table, local_scope, result, max_depth - 1);
            }

            Expression::TypeCast { expression, .. } => {
                self.validate_expression(expression, func, symbol_table, local_scope, result, max_depth - 1);
            }

            _ => {}
        }
    }

    fn validate_identifier(
        &self,
        name: &str,
        func_name: &str,
        local_scope: &LocalScopeTracker,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
        position: Position,
    ) {
        if local_scope.has_variable(name)
            || local_scope.has_parameter(name)
            || symbol_table.has_enum(name)
            || symbol_table.has_function(name)
            || symbol_table.is_builtin_static_object(name)
            || symbol_table.is_imported_namespace(name)
        {
            return;
        }

        self.add_warning(
            result,
            "QFUNC_WARN001",
            &format!(
                "Identifier '{}' not found in local scope or symbol table in function '{}'",
                name, func_name
            ),
            "QUICKFUNCS",
            position,
        );
    }

    fn validate_qualified_identifier(
        &self,
        parts: &[String],
        arguments: Option<&Vec<Expression>>,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if parts.len() < 2 {
            return;
        }

        let first_part = &parts[0];
        let second_part = &parts[1];

        // Check if it's a local variable/parameter (object property access)
        if local_scope.has_variable(first_part) || local_scope.has_parameter(first_part) {
            if let Some(args) = arguments {
                for arg in args {
                    self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
                }
            }
            return;
        }

        // Check for local enum access
        if parts.len() == 2 && arguments.is_none() && symbol_table.has_enum(first_part) {
            if !symbol_table.has_enum_field(first_part, second_part) {
                if let Some(fields) = symbol_table.try_get_enum(first_part) {
                    let valid_values: Vec<&String> = fields.keys().collect();
                    self.add_error(
                        result,
                        "QFUNC052",
                        "ENUM_VALUE_NOT_FOUND",
                        &format!("Enum '{}' does not have value '{}'", first_part, second_part),
                        &format!("Valid values: {}", valid_values.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                        Position::UNKNOWN,
                    );
                }
            }
            return;
        }

        // Check for namespace access
        if symbol_table.is_imported_namespace(first_part) {
            self.validate_namespace_access(parts, arguments, func, symbol_table, local_scope, result, max_depth);
            return;
        }

        // Check for static object access
        if has_static_object(first_part) {
            self.validate_static_object_access(parts, arguments, func, symbol_table, local_scope, result, max_depth);
            return;
        }

        // Check for DATA section variable
        if symbol_table.has_data_variable(first_part) {
            if let Some(args) = arguments {
                for arg in args {
                    self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
                }
            }
            return;
        }

        // Unknown - will be resolved at runtime
        self.add_warning(
            result,
            "QFUNC_WARN001",
            &format!(
                "Identifier '{}' not found in scope - will be resolved at runtime",
                first_part
            ),
            "QUICKFUNCS",
            Position::UNKNOWN,
        );

        if let Some(args) = arguments {
            for arg in args {
                self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
            }
        }
    }

    fn validate_namespace_access(
        &self,
        parts: &[String],
        arguments: Option<&Vec<Expression>>,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        let namespace_name = &parts[0];
        let member_name = &parts[1];

        if parts.len() == 2 {
            if let Some(args) = arguments {
                // Namespaced function call
                let func_info = symbol_table.get_namespaced_function(namespace_name, member_name);
                if func_info.is_none() {
                    self.add_error(
                        result,
                        "QFUNC045",
                        "IMPORTED_FUNCTION_NOT_FOUND",
                        &format!(
                            "Function '{}' not found in namespace '{}'",
                            member_name, namespace_name
                        ),
                        "",
                        Position::UNKNOWN,
                    );
                    return;
                }

                let expected_params = func_info.unwrap().signature.parameters.len();
                let actual_params = args.len();

                if actual_params != expected_params {
                    self.add_error(
                        result,
                        "QFUNC046",
                        "PARAMETER_COUNT_MISMATCH",
                        &format!(
                            "Function '{}.{}' expects {} parameter(s) but got {}",
                            namespace_name, member_name, expected_params, actual_params
                        ),
                        "",
                        Position::UNKNOWN,
                    );
                }

                for arg in args {
                    self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
                }
            } else {
                // Namespaced enum reference
                if symbol_table.get_namespaced_enum(namespace_name, member_name).is_none() {
                    self.add_error(
                        result,
                        "QFUNC055",
                        "NAMESPACE_MEMBER_NOT_FOUND",
                        &format!(
                            "Namespace '{}' does not have member '{}'",
                            namespace_name, member_name
                        ),
                        "Check the imported file for available functions and enums",
                        Position::UNKNOWN,
                    );
                }
            }
        } else if parts.len() == 3 {
            // Imported enum access: namespace.enum.value
            let enum_name = &parts[1];
            let enum_value = &parts[2];

            let enum_fields = symbol_table.get_namespaced_enum(namespace_name, enum_name);
            if enum_fields.is_none() {
                self.add_error(
                    result,
                    "QFUNC054",
                    "IMPORTED_ENUM_NOT_FOUND",
                    &format!(
                        "Namespace '{}' does not have enum '{}'",
                        namespace_name, enum_name
                    ),
                    "Check the imported file for available enums",
                    Position::UNKNOWN,
                );
                return;
            }

            if !enum_fields.unwrap().contains_key(enum_value) {
                let valid_values: Vec<&String> = enum_fields.unwrap().keys().collect();
                self.add_error(
                    result,
                    "QFUNC056",
                    "ENUM_VALUE_NOT_FOUND",
                    &format!(
                        "Enum '{}.{}' does not have value '{}'",
                        namespace_name, enum_name, enum_value
                    ),
                    &format!("Valid values: {}", valid_values.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                    Position::UNKNOWN,
                );
            }
        }
    }

    fn validate_static_object_access(
        &self,
        parts: &[String],
        arguments: Option<&Vec<Expression>>,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        let object_name = &parts[0];
        let method_name = &parts[1];

        if let Some(args) = arguments {
            if !has_static_method(object_name, method_name) {
                self.add_error(
                    result,
                    "QFUNC050",
                    "STATIC_METHOD_NOT_FOUND",
                    &format!(
                        "Static object '{}' has no method '{}'",
                        object_name, method_name
                    ),
                    "",
                    Position::UNKNOWN,
                );
            }

            for arg in args {
                self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
            }
        }
    }

    fn validate_arithmetic_op_expression(
        &self,
        left: &Expression,
        right: &Expression,
        operator: &str,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if !is_valid_arithmetic_operator(operator) {
            self.add_error(
                result,
                "QFUNC025",
                "INVALID_ARITHMETIC_OPERATOR",
                &format!(
                    "Invalid arithmetic operator '{}' in function '{}'",
                    operator, func.name
                ),
                "Valid operators: +, -, *, /, %, **, %%, %&, &%",
                left.position(),
            );
            return;
        }

        self.validate_expression(left, func, symbol_table, local_scope, result, max_depth - 1);
        self.validate_expression(right, func, symbol_table, local_scope, result, max_depth - 1);

        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
        let left_type = type_inference_visitor.infer_type_from_expression(left);
        let right_type = type_inference_visitor.infer_type_from_expression(right);

        if let (Some(lt), Some(rt)) = (left_type, right_type) {
            if operator == "+" {
                if lt == DataType::String && rt == DataType::String {
                    return;
                }

                if lt == DataType::String || rt == DataType::String {
                    self.add_error(
                        result,
                        "QFUNC026",
                        "INVALID_STRING_OPERATION",
                        &format!(
                            "Cannot concatenate string with {:?} in function '{}'",
                            if lt == DataType::String { rt } else { lt },
                            func.name
                        ),
                        "Use .toString() to explicitly convert to string, or use only string + string",
                        left.position(),
                    );
                    return;
                }
            }

            if !Self::is_numeric_type(lt) {
                self.add_error(
                    result,
                    "QFUNC027",
                    "NON_NUMERIC_OPERAND",
                    &format!(
                        "Left operand of '{}' must be numeric, got {:?} in function '{}'",
                        operator, lt, func.name
                    ),
                    "Use int, float, or double types for arithmetic operations",
                    left.position(),
                );
            }

            if !Self::is_numeric_type(rt) {
                self.add_error(
                    result,
                    "QFUNC028",
                    "NON_NUMERIC_OPERAND",
                    &format!(
                        "Right operand of '{}' must be numeric, got {:?} in function '{}'",
                        operator, rt, func.name
                    ),
                    "Use int, float, or double types for arithmetic operations",
                    right.position(),
                );
            }
        }
    }

    fn validate_bitwise_op_expression(
        &self,
        left: &Expression,
        right: &Expression,
        operator: &str,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if !is_valid_bitwise_operator(operator) {
            self.add_error(
                result,
                "QFUNC029",
                "INVALID_BITWISE_OPERATOR",
                &format!(
                    "Invalid bitwise operator '{}' in function '{}'",
                    operator, func.name
                ),
                "Valid operators: &, |, ^, <<, >>",
                left.position(),
            );
            return;
        }

        self.validate_expression(left, func, symbol_table, local_scope, result, max_depth - 1);
        self.validate_expression(right, func, symbol_table, local_scope, result, max_depth - 1);

        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
        let left_type = type_inference_visitor.infer_type_from_expression(left);
        let right_type = type_inference_visitor.infer_type_from_expression(right);

        if let Some(lt) = left_type {
            if lt != DataType::Int {
                self.add_error(
                    result,
                    "QFUNC030",
                    "NON_INT_BITWISE_OPERAND",
                    &format!(
                        "Bitwise operator '{}' requires int type, got {:?} in function '{}'",
                        operator, lt, func.name
                    ),
                    "Convert to int or use arithmetic operators instead",
                    left.position(),
                );
            }
        }

        if let Some(rt) = right_type {
            if rt != DataType::Int {
                self.add_error(
                    result,
                    "QFUNC031",
                    "NON_INT_BITWISE_OPERAND",
                    &format!(
                        "Bitwise operator '{}' requires int type, got {:?} in function '{}'",
                        operator, rt, func.name
                    ),
                    "Convert to int or use arithmetic operators instead",
                    right.position(),
                );
            }
        }
    }

    fn validate_comparison_op_expression(
        &self,
        left: &Expression,
        right: &Expression,
        operator: &str,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if !is_valid_comparison_operator(operator) {
            self.add_error(
                result,
                "QFUNC032",
                "INVALID_COMPARISON_OPERATOR",
                &format!(
                    "Invalid comparison operator '{}' in function '{}'",
                    operator, func.name
                ),
                "Valid operators: ==, !=, >, <, >=, <=",
                left.position(),
            );
            return;
        }

        self.validate_expression(left, func, symbol_table, local_scope, result, max_depth - 1);
        self.validate_expression(right, func, symbol_table, local_scope, result, max_depth - 1);

        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
        let left_type = type_inference_visitor.infer_type_from_expression(left);
        let right_type = type_inference_visitor.infer_type_from_expression(right);

        if let (Some(lt), Some(rt)) = (left_type, right_type) {
            if operator == "==" || operator == "!=" {
                if !Self::are_types_comparable(lt, rt) {
                    self.add_warning(
                        result,
                        "QFUNC_WARN002",
                        &format!(
                            "Comparing incompatible types {:?} and {:?} in function '{}'",
                            lt, rt, func.name
                        ),
                        "QUICKFUNCS",
                        left.position(),
                    );
                }
                return;
            }

            if !Self::is_numeric_type(lt) || !Self::is_numeric_type(rt) {
                self.add_error(
                    result,
                    "QFUNC033",
                    "NON_NUMERIC_COMPARISON",
                    &format!(
                        "Comparison operator '{}' requires numeric types, got {:?} and {:?} in function '{}'",
                        operator, lt, rt, func.name
                    ),
                    "Use numeric types (int, float, double) for relational comparisons",
                    left.position(),
                );
            }
        }
    }

    fn validate_logical_op_expression(
        &self,
        left: &Expression,
        right: &Expression,
        operator: &str,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if !is_valid_logical_operator(operator) {
            self.add_error(
                result,
                "QFUNC034",
                "INVALID_LOGICAL_OPERATOR",
                &format!(
                    "Invalid logical operator '{}' in function '{}'",
                    operator, func.name
                ),
                "Valid operators: &&, ||, and, or",
                left.position(),
            );
            return;
        }

        self.validate_expression(left, func, symbol_table, local_scope, result, max_depth - 1);
        self.validate_expression(right, func, symbol_table, local_scope, result, max_depth - 1);

        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
        let left_type = type_inference_visitor.infer_type_from_expression(left);
        let right_type = type_inference_visitor.infer_type_from_expression(right);

        if let Some(lt) = left_type {
            if lt != DataType::Bool {
                self.add_error(
                    result,
                    "QFUNC035",
                    "NON_BOOL_LOGICAL_OPERAND",
                    &format!(
                        "Logical operator '{}' requires bool type, got {:?} in function '{}'",
                        operator, lt, func.name
                    ),
                    "Use comparison operators to create boolean values",
                    left.position(),
                );
            }
        }

        if let Some(rt) = right_type {
            if rt != DataType::Bool {
                self.add_error(
                    result,
                    "QFUNC036",
                    "NON_BOOL_LOGICAL_OPERAND",
                    &format!(
                        "Logical operator '{}' requires bool type, got {:?} in function '{}'",
                        operator, rt, func.name
                    ),
                    "Use comparison operators to create boolean values",
                    right.position(),
                );
            }
        }
    }

    fn validate_unary_op_expression(
        &self,
        operand: &Expression,
        operator: &str,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if !is_valid_unary_operator(operator) {
            self.add_error(
                result,
                "QFUNC037",
                "INVALID_UNARY_OPERATOR",
                &format!(
                    "Invalid unary operator '{}' in function '{}'",
                    operator, func.name
                ),
                "Valid operators: !, not, -, +, ~?",
                operand.position(),
            );
            return;
        }

        self.validate_expression(operand, func, symbol_table, local_scope, result, max_depth - 1);

        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
        let operand_type = type_inference_visitor.infer_type_from_expression(operand);

        if let Some(ot) = operand_type {
            if operator == "!" || operator == "not" {
                if ot != DataType::Bool {
                    self.add_error(
                        result,
                        "QFUNC038",
                        "NON_BOOL_NOT_OPERAND",
                        &format!(
                            "Logical NOT requires bool type, got {:?} in function '{}'",
                            ot, func.name
                        ),
                        "Use comparison to create boolean value",
                        operand.position(),
                    );
                }
            } else if operator == "~?" {
                if ot != DataType::Int {
                    self.add_error(
                        result,
                        "QFUNC039",
                        "NON_INT_BITWISE_NOT",
                        &format!(
                            "Bitwise NOT (~?) requires int type, got {:?} in function '{}'",
                            ot, func.name
                        ),
                        "Convert to int before using bitwise NOT",
                        operand.position(),
                    );
                }
            } else if operator == "-" || operator == "+" {
                if !Self::is_numeric_type(ot) {
                    self.add_error(
                        result,
                        "QFUNC040",
                        "NON_NUMERIC_UNARY",
                        &format!(
                            "Unary '{}' requires numeric type, got {:?} in function '{}'",
                            operator, ot, func.name
                        ),
                        "Use numeric types (int, float, double)",
                        operand.position(),
                    );
                }
            }
        }
    }

    fn validate_conditional_expression(
        &self,
        condition: &Expression,
        true_value: &Expression,
        false_value: &Expression,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        self.validate_expression(condition, func, symbol_table, local_scope, result, max_depth - 1);

        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
        let condition_type = type_inference_visitor.infer_type_from_expression(condition);

        if let Some(ct) = condition_type {
            if ct != DataType::Bool {
                self.add_error(
                    result,
                    "QFUNC041",
                    "NON_BOOL_TERNARY_CONDITION",
                    &format!(
                        "Ternary condition must be bool, got {:?} in function '{}'",
                        ct, func.name
                    ),
                    "Use comparison operators to create boolean condition",
                    condition.position(),
                );
            }
        }

        self.validate_expression(true_value, func, symbol_table, local_scope, result, max_depth - 1);
        self.validate_expression(false_value, func, symbol_table, local_scope, result, max_depth - 1);

        let true_type = type_inference_visitor.infer_type_from_expression(true_value);
        let false_type = type_inference_visitor.infer_type_from_expression(false_value);

        if let (Some(tt), Some(ft)) = (true_type, false_type) {
            if !Self::are_types_comparable(tt, ft) {
                self.add_warning(
                    result,
                    "QFUNC_WARN003",
                    &format!(
                        "Ternary branches have incompatible types: {:?} and {:?} in function '{}'",
                        tt, ft, func.name
                    ),
                    "QUICKFUNCS",
                    condition.position(),
                );
            }
        }
    }

    fn validate_quick_func_call(
        &self,
        name: &str,
        arguments: &[Expression],
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        // Check if it's a lambda invocation (local variable)
        if local_scope.has_variable(name) {
            if self.is_verbose {
                self.error_manager.log_debug(&format!(
                    "    Lambda invocation detected: {}()",
                    name
                ));
            }

            for arg in arguments {
                self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
            }
            return;
        }

        // Real function call
        if !symbol_table.has_function(name) {
            self.add_error(
                result,
                "QFUNC042",
                "FUNCTION_NOT_FOUND",
                &format!("Function '{}' is not defined in @QUICKFUNCS", name),
                "Define function in @QUICKFUNCS section or check spelling",
                Position::UNKNOWN,
            );
            return;
        }

        if let Some(func_sig) = symbol_table.try_get_function(name) {
            let expected_param_count = func_sig.parameters.len();
            let actual_arg_count = arguments.len();

            if actual_arg_count != expected_param_count {
                self.add_error(
                    result,
                    "QFUNC043",
                    "WRONG_ARGUMENT_COUNT",
                    &format!(
                        "Function '{}' expects {} arguments, got {}",
                        name, expected_param_count, actual_arg_count
                    ),
                    &format!("Check function signature: {}", func_sig),
                    Position::UNKNOWN,
                );
            }
        }

        for arg in arguments {
            self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
        }
    }

    fn validate_imported_function_call(
        &self,
        namespace_name: &str,
        function_name: &str,
        arguments: &[Expression],
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if self.is_verbose {
            self.error_manager.log_debug(&format!(
                "    Validating imported function: {}.{}()",
                namespace_name, function_name
            ));
        }

        // Check if it's actually a local variable (instance method call)
        if local_scope.has_variable(namespace_name) {
            if self.is_verbose {
                self.error_manager.log_debug(&format!(
                    "    '{}' is a local variable, treating as instance method call",
                    namespace_name
                ));
            }

            for arg in arguments {
                self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
            }
            return;
        }

        if !symbol_table.is_imported_namespace(namespace_name) {
            self.add_error(
                result,
                "QFUNC044",
                "NAMESPACE_NOT_FOUND",
                &format!(
                    "Namespace '{}' not found. Did you import it in @IMPORTS?",
                    namespace_name
                ),
                &format!(
                    "Add to @IMPORTS: {} from \"path/to/file.mdix\"",
                    namespace_name
                ),
                Position::UNKNOWN,
            );
            return;
        }

        let func_info = symbol_table.get_namespaced_function(namespace_name, function_name);
        if func_info.is_none() {
            if let Some(ns) = symbol_table.try_get_namespace(namespace_name) {
                let available_funcs: Vec<&String> = ns.functions.keys().collect();
                self.add_error(
                    result,
                    "QFUNC045",
                    "IMPORTED_FUNCTION_NOT_FOUND",
                    &format!(
                        "Function '{}' not found in namespace '{}'",
                        function_name, namespace_name
                    ),
                    &format!("Available functions: {}", available_funcs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                    Position::UNKNOWN,
                );
            } else {
                self.add_error(
                    result,
                    "QFUNC045",
                    "IMPORTED_FUNCTION_NOT_FOUND",
                    &format!(
                        "Function '{}' not found in namespace '{}'",
                        function_name, namespace_name
                    ),
                    "",
                    Position::UNKNOWN,
                );
            }
            return;
        }

        let function_sig = &func_info.unwrap().signature;
        let expected_param_count = function_sig.parameters.len();
        let actual_param_count = arguments.len();

        if actual_param_count != expected_param_count {
            self.add_error(
                result,
                "QFUNC046",
                "PARAMETER_COUNT_MISMATCH",
                &format!(
                    "Function '{}.{}' expects {} parameter(s) but got {}",
                    namespace_name, function_name, expected_param_count, actual_param_count
                ),
                &format!("Expected: {}", function_sig),
                Position::UNKNOWN,
            );
        }

        for arg in arguments {
            self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
        }

        if self.is_verbose {
            self.error_manager.log_debug(&format!(
                "    Imported function validated: {}.{}() returns {:?}",
                namespace_name, function_name, function_sig.return_type
            ));
        }
    }

    fn validate_instance_method_call(
        &self,
        instance: &Expression,
        method_name: &str,
        arguments: &[Expression],
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        let chain_depth = Self::count_method_chain_depth(instance);
        if chain_depth > MAX_METHOD_CHAIN_DEPTH {
            self.add_error(
                result,
                "QFUNC066",
                "METHOD_CHAIN_TOO_DEEP",
                &format!(
                    "Method chain depth ({}) exceeds maximum of {} in function '{}'",
                    chain_depth, MAX_METHOD_CHAIN_DEPTH, func.name
                ),
                "Break up the method chain into intermediate variables",
                instance.position(),
            );
            return;
        }

        self.validate_expression(instance, func, symbol_table, local_scope, result, max_depth - 1);

        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, None);
        let instance_type = type_inference_visitor.infer_type_from_expression(instance);

        if let Some(inst_type) = instance_type {
            let dix_type = Self::convert_data_type_to_dix_type(inst_type);

            if let Some(dt) = dix_type {
                if !has_instance_method(dt, method_name) {
                    self.add_error(
                        result,
                        "QFUNC047",
                        "INSTANCE_METHOD_NOT_FOUND",
                        &format!("Type '{:?}' has no instance method '{}'", inst_type, method_name),
                        &format!("Type '{:?}' has no such method", inst_type),
                        instance.position(),
                    );
                }
            }
        } else if self.is_verbose {
            self.error_manager.log_debug(&format!(
                "    Could not infer type for instance in method call: {}()",
                method_name
            ));
        }

        for arg in arguments {
            self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
        }
    }

    fn validate_static_method_call(
        &self,
        object_name: &str,
        method_name: &str,
        arguments: &[Expression],
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
    ) {
        if !has_static_object(object_name) {
            self.add_error(
                result,
                "QFUNC049",
                "STATIC_OBJECT_NOT_FOUND",
                &format!("Static object '{}' is not defined", object_name),
                "Available static objects: Math, DateTime, Array, Random, Enum, Guid, IpAddress, Dix",
                Position::UNKNOWN,
            );
        } else if !has_static_method(object_name, method_name) {
            self.add_error(
                result,
                "QFUNC050",
                "STATIC_METHOD_NOT_FOUND",
                &format!(
                    "Static object '{}' has no method '{}'",
                    object_name, method_name
                ),
                "",
                Position::UNKNOWN,
            );
        }

        for arg in arguments {
            self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
        }
    }

    fn validate_enum_access(
        &self,
        namespace_name: Option<&str>,
        enum_name: &str,
        value: &str,
        function_name: &str,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
        position: Position,
    ) {
        if self.is_verbose {
            let full_name = if let Some(ns) = namespace_name {
                format!("{}.{}.{}", ns, enum_name, value)
            } else {
                format!("{}.{}", enum_name, value)
            };
            self.error_manager.log_debug(&format!(
                "    Validating enum access: {}",
                full_name
            ));
        }

        if let Some(ns) = namespace_name {
            // Imported enum
            let enum_fields = symbol_table.get_namespaced_enum(ns, enum_name);
            if enum_fields.is_none() {
                if let Some(namespace) = symbol_table.try_get_namespace(ns) {
                    let available_enums: Vec<&String> = namespace.enums.keys().collect();
                    let suggestion = if available_enums.is_empty() {
                        String::new()
                    } else {
                        format!("Available enums: {}", available_enums.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "))
                    };

                    self.add_error(
                        result,
                        "QFUNC055",
                        "IMPORTED_ENUM_NOT_FOUND",
                        &format!("Enum '{}' not found in namespace '{}'", enum_name, ns),
                        &suggestion,
                        position,
                    );
                } else {
                    self.add_error(
                        result,
                        "QFUNC055",
                        "IMPORTED_ENUM_NOT_FOUND",
                        &format!("Enum '{}' not found in namespace '{}'", enum_name, ns),
                        "",
                        position,
                    );
                }
                return;
            }

            if !enum_fields.unwrap().contains_key(value) {
                let valid_values: Vec<&String> = enum_fields.unwrap().keys().collect();
                self.add_error(
                    result,
                    "QFUNC056",
                    "ENUM_VALUE_NOT_FOUND",
                    &format!(
                        "Enum '{}.{}' does not have value '{}'",
                        ns, enum_name, value
                    ),
                    &format!("Valid values: {}", valid_values.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                    position,
                );
            }
        } else {
            // Local enum
            if !symbol_table.has_enum(enum_name) {
                self.add_error(
                    result,
                    "QFUNC052",
                    "ENUM_NOT_FOUND",
                    &format!("Enum '{}' not defined in @ENUMS section", enum_name),
                    "Define enum in @ENUMS section or check spelling",
                    position,
                );
                return;
            }

            if !symbol_table.has_enum_field(enum_name, value) {
                if let Some(fields) = symbol_table.try_get_enum(enum_name) {
                    let valid_values: Vec<&String> = fields.keys().collect();
                    self.add_error(
                        result,
                        "QFUNC053",
                        "ENUM_VALUE_NOT_FOUND",
                        &format!("Enum '{}' does not have value '{}'", enum_name, value),
                        &format!("Valid values: {}", valid_values.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                        position,
                    );
                }
            }
        }
    }

    // ==================== VALUE VALIDATION ====================

    fn validate_value(
        &self,
        value: &Value,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
    ) {
        match value {
            Value::Array { values, .. } => {
                if values.len() > MAX_ARRAY_ELEMENTS {
                    self.add_error(
                        result,
                        "QFUNC057",
                        "ARRAY_TOO_LARGE",
                        &format!(
                            "Array has {} elements, exceeds limit of {}",
                            values.len(),
                            MAX_ARRAY_ELEMENTS
                        ),
                        &format!("Reduce array size to {} or fewer elements", MAX_ARRAY_ELEMENTS),
                        value.position(),
                    );
                }

                self.validate_array_homogeneity(values, &func.name, local_scope, symbol_table, result, value.position());

                for item in values {
                    self.validate_value(item, func, symbol_table, local_scope, result);
                }
            }

            Value::Object { properties, .. } => {
                self.validate_object_literal_keys(value, &func.name, result);

                if properties.len() > MAX_OBJECT_PROPERTIES {
                    self.add_error(
                        result,
                        "QFUNC058",
                        "OBJECT_TOO_LARGE",
                        &format!(
                            "Object has {} properties, exceeds limit of {}",
                            properties.len(),
                            MAX_OBJECT_PROPERTIES
                        ),
                        &format!(
                            "Reduce object size to {} or fewer properties",
                            MAX_OBJECT_PROPERTIES
                        ),
                        value.position(),
                    );
                }

                for prop in properties {
                    self.validate_value(&prop.value, func, symbol_table, local_scope, result);
                }
            }

            Value::PrefixedConstructor { prefix, arguments, .. } => {
                if prefix.eq_ignore_ascii_case("t") && arguments.len() > MAX_TUPLE_ARGUMENTS {
                    self.add_error(
                        result,
                        "QFUNC059",
                        "TUPLE_TOO_LARGE",
                        &format!(
                            "Tuple has {} arguments, exceeds limit of {}",
                            arguments.len(),
                            MAX_TUPLE_ARGUMENTS
                        ),
                        &format!(
                            "Reduce tuple size to {} or fewer arguments",
                            MAX_TUPLE_ARGUMENTS
                        ),
                        value.position(),
                    );
                }

                for arg in arguments {
                    self.validate_value(arg, func, symbol_table, local_scope, result);
                }
            }

            Value::InterpolatedString { expressions, .. } => {
                let max_depth = Self::calculate_max_depth(100);
                for expr in expressions {
                    self.validate_expression(expr, func, symbol_table, local_scope, result, max_depth);
                }
            }

            Value::Expression { expr, .. } => {
                let max_depth = Self::calculate_max_depth(100);
                self.validate_expression(expr, func, symbol_table, local_scope, result, max_depth);
            }

            Value::Lambda { .. } => {
                // Lambda validation happens elsewhere
            }

            _ => {}
        }
    }

    fn validate_array_homogeneity(
        &self,
        values: &[Value],
        _function_name: &str,
        local_scope: &LocalScopeTracker,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
        position: Position,
    ) {
        if values.is_empty() {
            return;
        }

        let local_variable_types = local_scope.get_all_variable_types();
        let type_inference_visitor = TypeInferenceVisitor::new(symbol_table, Some(local_variable_types));

        let first_type = type_inference_visitor.infer_type_from_value(&values[0]);

        if first_type.is_none() {
            if self.is_verbose {
                self.error_manager.log_debug(
                    "    Cannot infer type of first array element - skipping homogeneity check"
                );
            }
            return;
        }

        let first_type = first_type.unwrap();

        for (i, element) in values.iter().enumerate().skip(1) {
            let element_type = type_inference_visitor.infer_type_from_value(element);

            if let Some(elem_type) = element_type {
                if !Self::are_types_compatible_strict(elem_type, first_type) {
                    self.add_error(
                        result,
                        "QFUNC077",
                        "ARRAY_HETEROGENEOUS",
                        &format!(
                            "Array element {} has type {:?} but array expects {:?} (from first element)",
                            i + 1,
                            elem_type,
                            first_type
                        ),
                        &format!(
                            "All array elements must have the same type. Convert element to {:?} or use separate arrays",
                            first_type
                        ),
                        position,
                    );
                }
            } else {
                self.add_warning(
                    result,
                    "QFUNC_WARN008",
                    &format!(
                        "Cannot infer type of array element {} in function '{}'",
                        i + 1,
                        _function_name
                    ),
                    "QUICKFUNCS",
                    position,
                );
            }
        }

        if self.is_verbose {
            self.error_manager.log_debug(&format!(
                "    Array homogeneity validated: all {} elements are {:?}",
                values.len(),
                first_type
            ));
        }
    }

    fn validate_object_literal_keys(
        &self,
        object: &Value,
        function_name: &str,
        result: &mut SectionAnalysisResult,
    ) {
        let properties = match object {
            Value::Object { properties, .. } => properties,
            _ => return,
        };

        if properties.is_empty() {
            return;
        }

        let mut seen_keys = FxHashSet::default();
        let mut duplicate_keys = FxHashSet::default();

        for prop in properties {
            if !seen_keys.insert(&prop.key) {
                duplicate_keys.insert(&prop.key);

                self.add_error(
                    result,
                    "QFUNC060",
                    "DUPLICATE_OBJECT_KEY",
                    &format!("Duplicate object key '{}' in function '{}'", prop.key, function_name),
                    &format!(
                        "Each key in an object must be unique. Remove or rename duplicate key '{}'",
                        prop.key
                    ),
                    prop.position,
                );
            }
        }
    }

    // ==================== TYPE SYSTEM METHODS ====================

    #[inline]
    fn are_types_compatible_strict(source_type: DataType, target_type: DataType) -> bool {
        if source_type == target_type {
            return true;
        }

        if target_type == DataType::Any || source_type == DataType::Any {
            return true;
        }

        if Self::is_numeric_type(source_type) && Self::is_numeric_type(target_type) {
            return true;
        }

        if (source_type == DataType::Date && target_type == DataType::Timestamp)
            || (source_type == DataType::Timestamp && target_type == DataType::Date)
        {
            return true;
        }

        false
    }

    #[inline]
    fn are_types_comparable(type1: DataType, type2: DataType) -> bool {
        if type1 == type2 {
            return true;
        }

        if type1 == DataType::Any || type2 == DataType::Any {
            return true;
        }

        if Self::is_numeric_type(type1) && Self::is_numeric_type(type2) {
            return true;
        }

        if (type1 == DataType::Date || type1 == DataType::Timestamp)
            && (type2 == DataType::Date || type2 == DataType::Timestamp)
        {
            return true;
        }

        false
    }

    #[inline]
    fn is_numeric_type(data_type: DataType) -> bool {
        matches!(
            data_type,
            DataType::Int | DataType::Float | DataType::Double
        )
    }

    fn validate_arithmetic_operation(
        &self,
        op: &str,
        left_type: DataType,
        right_type: DataType,
        function_name: &str,
        result: &mut SectionAnalysisResult,
        position: Position,
    ) {
        if op == "+=" {
            if left_type == DataType::String && right_type == DataType::String {
                return;
            }

            if left_type == DataType::String || right_type == DataType::String {
                self.add_error(
                    result,
                    "QFUNC061",
                    "INVALID_STRING_CONCAT_ASSIGN",
                    &format!(
                        "Cannot use '+=' to concatenate string with non-string in function '{}'",
                        function_name
                    ),
                    "Use only string += string, or convert to string first",
                    position,
                );
                return;
            }
        }

        if !Self::is_numeric_type(left_type) {
            self.add_error(
                result,
                "QFUNC062",
                "NON_NUMERIC_ARITHMETIC_ASSIGN",
                &format!(
                    "Arithmetic assignment '{}' requires numeric type, left operand is {:?}",
                    op, left_type
                ),
                "Use int, float, or double types",
                position,
            );
        }

        if !Self::is_numeric_type(right_type) {
            self.add_error(
                result,
                "QFUNC063",
                "NON_NUMERIC_ARITHMETIC_ASSIGN",
                &format!(
                    "Arithmetic assignment '{}' requires numeric type, right operand is {:?}",
                    op, right_type
                ),
                "Use int, float, or double types",
                position,
            );
        }

        if matches!(op, "&=" | "|=" | "^=" | "<<=" | ">>=") {
            if left_type != DataType::Int {
                self.add_error(
                    result,
                    "QFUNC064",
                    "NON_INT_BITWISE_ASSIGN",
                    &format!("Bitwise assignment '{}' requires int type, got {:?}", op, left_type),
                    "Convert to int before using bitwise assignment",
                    position,
                );
            }

            if right_type != DataType::Int {
                self.add_error(
                    result,
                    "QFUNC065",
                    "NON_INT_BITWISE_ASSIGN",
                    &format!("Bitwise assignment '{}' requires int type, got {:?}", op, right_type),
                    "Convert to int before using bitwise assignment",
                    position,
                );
            }
        }
    }

    fn convert_data_type_to_dix_type(data_type: DataType) -> Option<DixType> {
        match data_type {
            DataType::Int => Some(DixType::Int),
            DataType::Float => Some(DixType::Float),
            DataType::Double => Some(DixType::Double),
            DataType::String => Some(DixType::String),
            DataType::Bool => Some(DixType::Bool),
            DataType::Array => Some(DixType::Array),
            DataType::Tuple => Some(DixType::Tuple),
            DataType::Object => Some(DixType::Object),
            DataType::Hex => Some(DixType::Hex),
            DataType::Blob => Some(DixType::Blob),
            DataType::Regex => Some(DixType::Regex),
            DataType::Date => Some(DixType::Date),
            DataType::Timestamp => Some(DixType::Timestamp),
            DataType::Enum => Some(DixType::Enum),
            DataType::Any | DataType::Function | DataType::Range => None,
        }
    }

    fn count_method_chain_depth(expr: &Expression) -> usize {
        let mut depth = 0;
        let mut current = expr;

        loop {
            match current {
                Expression::InstanceMethodCall { instance, .. } => {
                    depth += 1;
                    current = instance;
                }
                Expression::PropertyAccess { object, .. } => {
                    depth += 1;
                    current = object;
                }
                _ => break,
            }
        }

        depth
    }

    // ==================== SYMBOL TABLE POPULATION ====================

    fn populate_symbol_table(
        &self,
        section: &QuickFuncsSection,
        symbol_table: &mut SymbolTable,
        duplicate_functions: &FxHashSet<&String>,
        _result: &mut SectionAnalysisResult,
    ) {
        let mut success_count = 0;
        let skip_count = duplicate_functions.len();

        for func in &section.functions {
            if duplicate_functions.contains(&func.name) {
                continue;
            }

            let parameter_info: Vec<ParameterInfo> = func
                .parameters
                .iter()
                .filter(|p| Self::is_valid_identifier(&p.name))
                .map(|p| ParameterInfo {
                    name: p.name.clone(),
                    param_type: p.data_type,
                    has_default_value: p.default_value.is_some(),
                    default_value: p.default_value.clone(),
                })
                .collect();

            let scopes = if let Some(ref scope_list) = func.scope_list {
                scope_list.clone()
            } else {
                Vec::new()
            };

            let signature = FunctionSignature {
                name: func.name.clone(),
                return_type: func.return_type,
                parameters: parameter_info,
                scopes,
                line: func.position.line as i32,
                column: func.position.column as i32,
            };

            symbol_table.add_function(func.name.clone(), signature);
            success_count += 1;

            if self.is_verbose {
                self.error_manager.log_debug(&format!(
                    "    Pre-registered function '{}' in symbol table",
                    func.name
                ));
            }
        }

        if self.is_debug {
            self.error_manager.log_info(&format!(
                "Symbol table populated: {} functions added, {} skipped",
                success_count, skip_count
            ));
        }
    }

    // ==================== HELPER METHODS ====================

    #[inline]
    fn is_valid_identifier(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        let mut chars = name.chars();
        let first = chars.next().unwrap();

        if !first.is_ascii_alphabetic() && first != '_' {
            return false;
        }

        chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    #[inline]
    fn is_valid_data_path(path: &str) -> bool {
        if path.is_empty() {
            return false;
        }

        path.split('.').all(|segment| {
            !segment.is_empty() && Self::is_valid_identifier(segment)
        })
    }

    #[inline]
    fn is_valid_value_expression(expr: &Expression) -> bool {
        !matches!(
            expr,
            Expression::ArithmeticOp { .. }
                | Expression::ComparisonOp { .. }
                | Expression::LogicalOp { .. }
        )
    }

    #[inline]
    fn should_halt(&self, result: &SectionAnalysisResult) -> bool {
        !result.errors.is_empty()
            && matches!(
                self.operational_settings.error_handling_strategy,
                ErrorHandlingStrategy::Halt
            )
    }

    // ==================== ERROR/WARNING HELPERS ====================

    #[inline]
    fn add_error(
        &self,
        result: &mut SectionAnalysisResult,
        error_id: &str,
        error_type: &str,
        message: &str,
        suggestion: &str,
        position: Position,
    ) {
        let error = SemanticErrorInfo {
            error_id: error_id.to_string(),
            error_type: error_type.to_string(),
            message: message.to_string(),
            section_name: "QUICKFUNCS".to_string(),
            suggestion: suggestion.to_string(),
            position: Some(position),
        };

        result.errors.push(error.clone());

        if self.is_debug {
            self.error_manager.log_error(&format!(
                "[{}] {}: {}",
                error_id, error_type, message
            ));
        }
    }

    #[inline]
    fn add_warning(
        &self,
        result: &mut SectionAnalysisResult,
        warning_id: &str,
        message: &str,
        section_name: &str,
        position: Position,
    ) {
        let warning = SemanticWarningInfo {
            warning_id: warning_id.to_string(),
            message: message.to_string(),
            section_name: section_name.to_string(),
            position: Some(position),
        };

        result.warnings.push(warning);

        if self.is_debug {
            self.error_manager.log_debug(&format!("[{}] {}", warning_id, message));
        }
    }
}

// ==================== HELPER STRUCTS ====================

/// Tracks local variables and parameters in function scope
struct LocalScopeTracker {
    variables: FxHashMap<String, VariableInfo>,
    parameters: FxHashSet<String>,
}

impl LocalScopeTracker {
    fn new(func_parameters: &[QuickFuncParam]) -> Self {
        let mut variables = FxHashMap::default();
        let mut parameters = FxHashSet::default();

        for param in func_parameters {
            parameters.insert(param.name.clone());
            variables.insert(
                param.name.clone(),
                VariableInfo {
                    name: param.name.clone(),
                    var_type: param.data_type,
                    is_const: true,
                    is_parameter: true,
                },
            );
        }

        LocalScopeTracker {
            variables,
            parameters,
        }
    }

    fn add_variable(&mut self, name: String, var_type: Option<DataType>, is_const: bool) {
        self.variables.insert(
            name.clone(),
            VariableInfo {
                name,
                var_type,
                is_const,
                is_parameter: false,
            },
        );
    }

    fn has_variable(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    fn has_parameter(&self, name: &str) -> bool {
        self.parameters.contains(name)
    }

    fn is_const(&self, name: &str) -> bool {
        self.variables
            .get(name)
            .map(|v| v.is_const)
            .unwrap_or(false)
    }

    fn get_variable_type(&self, name: &str) -> Option<DataType> {
        self.variables.get(name).and_then(|v| v.var_type)
    }

    fn update_variable_type(&mut self, name: &str, var_type: DataType) {
        if let Some(var_info) = self.variables.get_mut(name) {
            if var_info.var_type.is_none() {
                var_info.var_type = Some(var_type);
            }
        }
    }

    fn get_declared_variable_names(&self) -> impl Iterator<Item = &String> {
        self.variables
            .values()
            .filter(|v| !v.is_parameter)
            .map(|v| &v.name)
    }

    fn get_all_variable_types(&self) -> HashMap<String, Option<DataType>> {
        self.variables
            .iter()
            .map(|(k, v)| (k.clone(), v.var_type))
            .collect()
    }
}

#[derive(Clone)]
struct VariableInfo {
    name: String,
    var_type: Option<DataType>,
    is_const: bool,
    is_parameter: bool,
}

/// Analyzes return paths in function
struct ReturnPathAnalyzer {
    _expected_return_type: DataType,
    has_unconditional_return: bool,
}

impl ReturnPathAnalyzer {
    fn new(expected_return_type: DataType) -> Self {
        ReturnPathAnalyzer {
            _expected_return_type: expected_return_type,
            has_unconditional_return: false,
        }
    }

    fn add_return(&mut self) {
        self.has_unconditional_return = true;
    }

    fn all_paths_return(&self) -> bool {
        self.has_unconditional_return
    }
}

/// Collects variable references in function
struct VariableReferenceCollector {
    referenced_variables: FxHashSet<String>,
    parameters: FxHashSet<String>,
}

impl VariableReferenceCollector {
    fn new(func_parameters: &[QuickFuncParam]) -> Self {
        let parameters: FxHashSet<String> = func_parameters
            .iter()
            .map(|p| p.name.clone())
            .collect();

        VariableReferenceCollector {
            referenced_variables: FxHashSet::default(),
            parameters,
        }
    }

    fn collect_from_function(&mut self, func: &QuickFunction) -> FxHashSet<String> {
        for statement in &func.body {
            self.collect_from_statement(statement);
        }
        self.referenced_variables.clone()
    }

    fn collect_from_statement(&mut self, statement: &QuickFuncStatement) {
        match statement {
            QuickFuncStatement::Return { value, .. } => {
                self.collect_from_expression(value);
            }
            QuickFuncStatement::Assignment { value, .. } => {
                self.collect_from_expression(value);
            }
            QuickFuncStatement::ArithmeticAssignment { variable, value, .. } => {
                self.add_reference(variable);
                self.collect_from_expression(value);
            }
            QuickFuncStatement::VariableDeclaration { value, .. } => {
                self.collect_from_expression(value);
            }
            QuickFuncStatement::ObjectCreation { object, .. } => {
                self.collect_from_value(object);
            }
            QuickFuncStatement::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_from_expression(condition);
                for stmt in then_branch {
                    self.collect_from_statement(stmt);
                }
                if let Some(else_stmts) = else_branch {
                    for stmt in else_stmts {
                        self.collect_from_statement(stmt);
                    }
                }
            }
            QuickFuncStatement::Switch {
                expression,
                cases,
                default_case,
                ..
            } => {
                self.collect_from_expression(expression);
                for case in cases {
                    for stmt in &case.statements {
                        self.collect_from_statement(stmt);
                    }
                }
                if let Some(default) = default_case {
                    for stmt in &default.statements {
                        self.collect_from_statement(stmt);
                    }
                }
            }
            QuickFuncStatement::Log { value, .. } => {
                self.collect_from_expression(value);
            }
            QuickFuncStatement::ExpressionStatement { expression, .. } => {
                self.collect_from_expression(expression);
            }
        }
    }

    fn collect_from_expression(&mut self, expr: &Expression) {
        match expr {
            Expression::Identifier { name, .. } => {
                self.add_reference(name);
            }
            Expression::QualifiedIdentifier { parts, arguments, .. } => {
                if let Some(first) = parts.first() {
                    self.add_reference(first);
                }
                if let Some(args) = arguments {
                    for arg in args {
                        self.collect_from_expression(arg);
                    }
                }
            }
            Expression::ArithmeticOp { left, right, .. }
            | Expression::ComparisonOp { left, right, .. }
            | Expression::LogicalOp { left, right, .. }
            | Expression::BitwiseOp { left, right, .. } => {
                self.collect_from_expression(left);
                self.collect_from_expression(right);
            }
            Expression::UnaryOp { operand, .. } => {
                self.collect_from_expression(operand);
            }
            Expression::Conditional {
                condition,
                true_value,
                false_value,
                ..
            } => {
                self.collect_from_expression(condition);
                self.collect_from_expression(true_value);
                self.collect_from_expression(false_value);
            }
            Expression::Parenthesized { expression, .. } => {
                self.collect_from_expression(expression);
            }
            Expression::PropertyAccess { object, .. } => {
                self.collect_from_expression(object);
            }
            Expression::IndexAccess { object, index, .. } => {
                self.collect_from_expression(object);
                self.collect_from_expression(index);
            }
            Expression::QuickFuncCall { arguments, .. }
            | Expression::ImportedFunctionCall { arguments, .. }
            | Expression::StaticMethodCall { arguments, .. } => {
                for arg in arguments {
                    self.collect_from_expression(arg);
                }
            }
            Expression::InstanceMethodCall {
                instance, arguments, ..
            } => {
                self.collect_from_expression(instance);
                for arg in arguments {
                    self.collect_from_expression(arg);
                }
            }
            Expression::Value { value, .. } => {
                self.collect_from_value(value);
            }
            Expression::TypeCast { expression, .. } => {
                self.collect_from_expression(expression);
            }
            _ => {}
        }
    }

    fn collect_from_value(&mut self, value: &Value) {
        match value {
            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                for item in values {
                    self.collect_from_value(item);
                }
            }
            Value::Object { properties, .. } => {
                for prop in properties {
                    self.collect_from_value(&prop.value);
                }
            }
            Value::PrefixedConstructor { arguments, .. } => {
                for arg in arguments {
                    self.collect_from_value(arg);
                }
            }
            Value::InterpolatedString { expressions, .. } => {
                for expr in expressions {
                    self.collect_from_expression(expr);
                }
            }
            Value::QuickFuncCall { arguments, .. } => {
                for arg in arguments {
                    self.collect_from_expression(arg);
                }
            }
            Value::Expression { expr, .. } => {
                self.collect_from_expression(expr);
            }
            Value::Lambda { body, .. } => {
                self.collect_from_expression(body);
            }
            Value::Range { start, end, .. } => {
                self.collect_from_value(start);
                self.collect_from_value(end);
            }
            Value::Identifier { value, .. } => {
                self.add_reference(value);
            }
            _ => {}
        }
    }

    fn add_reference(&mut self, name: &str) {
        if !self.parameters.contains(name) {
            self.referenced_variables.insert(name.to_string());
        }
    }
}