// src/Compiler/Core/SectionAnalyzers/quickfuncs_section_analyzer.rs
//! QuickFunctions Section Analyzer — semantic validation for @QUICKFUNCS.
//!
//! Validates function signatures, parameters, return types, local variable
//! declarations, all expression forms, control flow, and circular call cycles.

use crate::Compiler::AST::*;
use crate::Compiler::AST::Visitors::TypeInferenceVisitor;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::Compiler::Core::Functions::CycleDetectionValidator;
use crate::Compiler::Core::SectionAnalyzers::{
    SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo,
};
use crate::Compiler::Utilities::{SymbolTable, ParameterInfo, FunctionSignature};
use crate::Builtins::Core::DixType;
use crate::Builtins::Resolver::{has_instance_method, has_static_method, has_static_object};
use crate::Utilities::Keywords;
use crate::ErrorManager::{ErrorManager, DebugConfig};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashMap;

// ==================== CONSTANTS ====================

const MAX_ABSOLUTE_VALIDATION_DEPTH: usize = 500;
const BASE_VALIDATION_DEPTH: usize = 100;
const MAX_TUPLE_ARGUMENTS: usize = 6;
const MAX_ARRAY_ELEMENTS: usize = 10_000;
const MAX_OBJECT_PROPERTIES: usize = 1_000;
const MAX_FUNCTION_PARAMETERS: usize = 100;
const MAX_FUNCTION_BODY_STATEMENTS: usize = 1_000;
const MAX_NESTING_DEPTH: usize = 50;
const MAX_METHOD_CHAIN_DEPTH: usize = 10;

// ==================== OPERATOR VALIDATORS ====================

#[inline]
fn is_valid_arithmetic_operator(op: &str) -> bool {
    matches!(op, "+" | "-" | "*" | "/" | "%" | "**" | "%%" | "%&" | "&%")
}

#[inline]
fn is_valid_bitwise_operator(op: &str) -> bool {
    matches!(op, "&" | "|" | "^" | "<<" | ">>")
}

#[inline]
fn is_valid_comparison_operator(op: &str) -> bool {
    matches!(op, "==" | "!=" | ">" | "<" | ">=" | "<=")
}

#[inline]
fn is_valid_logical_operator(op: &str) -> bool {
    matches!(op, "&&" | "||" | "and" | "or")
}

#[inline]
fn is_valid_unary_operator(op: &str) -> bool {
    matches!(op, "!" | "not" | "-" | "+" | "~?")
}

#[inline]
fn is_valid_arithmetic_assign_op(op: &str) -> bool {
    matches!(
        op,
        "+=" | "-=" | "*=" | "/=" | "%=" | "**=" | "&=" | "|=" | "^=" | "<<=" | ">>="
    )
}

#[inline]
fn is_valid_data_type(data_type: DataType) -> bool {
    matches!(
        data_type,
        DataType::Int
            | DataType::Float
            | DataType::Double
            | DataType::String
            | DataType::Bool
            | DataType::Array
            | DataType::Tuple
            | DataType::Hex
            | DataType::Blob
            | DataType::Regex
            | DataType::Object
            | DataType::Timestamp
            | DataType::Date
            | DataType::Enum
            | DataType::Any
            | DataType::Function
            | DataType::Range
    )
}

/// `true` for numeric types eligible for arithmetic promotion.
#[inline]
fn is_numeric_type(dt: DataType) -> bool {
    matches!(dt, DataType::Int | DataType::Float | DataType::Double)
}

// ==================== ANALYZER ====================

/// Semantic analyzer for the @QUICKFUNCS section.
pub struct QuickFuncsSectionAnalyzer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
}

impl<'a> QuickFuncsSectionAnalyzer<'a> {
    /// Create a new analyzer, caching debug flags and shared services.
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        QuickFuncsSectionAnalyzer {
            error_manager: ErrorManager::get_shared_instance(),
            debug_config: DebugConfig::from_debug_mode(operational_settings.debug_mode),
            operational_settings,
        }
    }

    /// Depth limit scales with AST size; capped to avoid runaway recursion.
    #[inline]
    fn calculate_max_depth(ast_size: usize) -> usize {
        (BASE_VALIDATION_DEPTH + ast_size / 10).min(MAX_ABSOLUTE_VALIDATION_DEPTH)
    }

    // ==================== ENTRY POINT ====================

    pub fn analyze(
        &mut self,
        section: &QuickFuncsSection,
        symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        let mut result = SectionAnalysisResult::new("QUICKFUNCS");
        let function_count = section.functions.len();

        if !symbol_table.are_builtin_objects_populated() {
            symbol_table.populate_builtin_objects();
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Analyzing QUICKFUNCS section with {} function definitions",
                function_count
            ));
        }

        // Phase 1: duplicate function name detection.
        if self.debug_config.is_enabled {
            self.error_manager.log_debug("Phase 1: checking for duplicate function names");
        }

        let mut function_names: FxHashSet<&str> =
            FxHashSet::with_capacity_and_hasher(function_count, Default::default());
        let mut duplicate_functions: FxHashSet<&str> = FxHashSet::default();

        for func in &section.functions {
            if !function_names.insert(func.name.as_str()) {
                duplicate_functions.insert(func.name.as_str());
                self.add_error(
                    &mut result,
                    "QFUNC001",
                    "DUPLICATE_FUNCTION_NAME",
                    &format!("Function '{}' is defined multiple times", func.name),
                    "Each function must have a unique name. Remove or rename the duplicate.",
                    func.position,
                );
                if self.should_halt(&result) {
                    return result;
                }
            }
        }

        // Phase 2: pre-register all valid functions so forward calls resolve.
        if self.debug_config.is_enabled {
            self.error_manager.log_debug("Phase 2: pre-registering functions in symbol table");
        }
        self.populate_symbol_table(section, symbol_table, &duplicate_functions, &mut result);

        if !result.errors.is_empty() && self.should_halt(&result) {
            return result;
        }

        // Phase 3: validate each function. Reuse one LocalScopeTracker across functions
        // to avoid per-function heap allocation (reset_with_params clears and repopulates).
        if self.debug_config.is_enabled {
            self.error_manager.log_debug("Phase 3: validating individual function declarations");
        }

        let mut reusable_scope = LocalScopeTracker::with_capacity(16);

        for func in &section.functions {
            if duplicate_functions.contains(func.name.as_str()) {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "Skipping duplicate function '{}'",
                        func.name
                    ));
                }
                continue;
            }
            self.validate_quick_function(func, symbol_table, &mut result, &mut reusable_scope);
            if self.should_halt(&result) {
                return result;
            }
        }

        // Phase 4: circular call detection.
        if self.debug_config.is_enabled {
            self.error_manager.log_debug("Phase 4: detecting circular function calls");
        }

        // NOTE: CycleDetectionValidator currently requires owned OperationalSettings.
        // When its API is updated to accept &OperationalSettings, remove the clone.
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

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "QUICKFUNCS analysis complete: {} — errors: {}, warnings: {}",
                if result.is_success { "SUCCESS" } else { "FAILURE" },
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
        reusable_scope: &mut LocalScopeTracker,
    ) {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!("Validating function '{}'", func.name));
        }

        if !Self::is_valid_identifier(&func.name) {
            self.add_error(
                result,
                "QFUNC002",
                "INVALID_FUNCTION_NAME",
                &format!("Function name '{}' is not a valid identifier", func.name),
                "Function names must start with a letter and contain only alphanumeric characters and underscores.",
                func.position,
            );
            return;
        }

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

        if func.return_type.is_none() {
            self.add_error(
                result,
                "QFUNC003",
                "MISSING_RETURN_TYPE",
                &format!("Function '{}' must specify a return type", func.name),
                &format!("Add a return type, e.g. ~{}<int> or ~{}<bool>", func.name, func.name),
                func.position,
            );
            return;
        }

        self.validate_return_type(func, result);
        if self.should_halt(result) { return; }

        self.validate_parameters(func, symbol_table, result);
        if self.should_halt(result) { return; }

        self.validate_scopes(func, result);
        if self.should_halt(result) { return; }

        self.validate_function_body(func, symbol_table, result, reusable_scope);
    }

    fn validate_return_type(
        &self,
        func: &QuickFunction,
        result: &mut SectionAnalysisResult,
    ) {
        if let Some(rt) = func.return_type {
            if !is_valid_data_type(rt) {
                self.add_error(
                    result,
                    "QFUNC003B",
                    "INVALID_RETURN_TYPE",
                    &format!("Function '{}' has invalid return type: {:?}", func.name, rt),
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
                &format!("Reduce to {} or fewer parameters", MAX_FUNCTION_PARAMETERS),
                func.position,
            );
            return;
        }

        let mut param_names: FxHashSet<&str> =
            FxHashSet::with_capacity_and_hasher(param_count, Default::default());
        let mut duplicate_params: FxHashSet<&str> = FxHashSet::default();

        for param in &func.parameters {
            if !param_names.insert(param.name.as_str()) {
                duplicate_params.insert(param.name.as_str());
                self.add_error(
                    result,
                    "QFUNC005",
                    "DUPLICATE_PARAMETER_NAME",
                    &format!(
                        "Parameter '{}' is defined multiple times in function '{}'",
                        param.name, func.name
                    ),
                    "Each parameter must have a unique name.",
                    param.position,
                );
                if self.should_halt(result) { return; }
            }
        }

        let mut seen_default = false;

        for param in &func.parameters {
            if duplicate_params.contains(param.name.as_str()) {
                continue;
            }

            if !Self::is_valid_identifier(&param.name) {
                self.add_error(
                    result,
                    "QFUNC006",
                    "INVALID_PARAMETER_NAME",
                    &format!(
                        "Parameter '{}' in function '{}' is not a valid identifier",
                        param.name, func.name
                    ),
                    "Parameter names must start with a letter and contain only alphanumeric characters and underscores.",
                    param.position,
                );
                if self.should_halt(result) { return; }
                continue;
            }

            if Keywords::is_data_type_keyword(&param.name) {
                let suggestion = format!(
                    "Use a different name like 'my{}{}' or '{}Value'",
                    param.name.chars().next().unwrap_or('X').to_uppercase(),
                    &param.name[1..],
                    param.name
                );
                self.add_error(
                    result,
                    "QFUNC006C",
                    "DATA_TYPE_KEYWORD_AS_PARAMETER",
                    &format!(
                        "Parameter '{}' in function '{}' cannot use a data type keyword as name",
                        param.name, func.name
                    ),
                    &suggestion,
                    param.position,
                );
                if self.should_halt(result) { return; }
                continue;
            }

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
                if self.should_halt(result) { return; }
                continue;
            }

            if let Some(pt) = param.data_type {
                if !is_valid_data_type(pt) {
                    self.add_error(
                        result,
                        "QFUNC007",
                        "INVALID_PARAMETER_TYPE",
                        &format!(
                            "Parameter '{}' in function '{}' has invalid type: {:?}",
                            param.name, func.name, pt
                        ),
                        "Use a valid data type: int, float, double, string, bool, array, etc.",
                        param.position,
                    );
                    if self.should_halt(result) { return; }
                }
            }

            if param.data_type.is_some() && param.default_value.is_some() {
                self.validate_default_value_type_strict(param, &func.name, symbol_table, result);
            }

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
                    "Place all parameters with default values at the end of the parameter list.",
                    param.position,
                );
                if self.should_halt(result) { return; }
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
        let (default_value, expected_type) = match (&param.default_value, param.data_type) {
            (Some(v), Some(t)) => (v, t),
            _ => return,
        };

        let visitor = TypeInferenceVisitor::new(symbol_table, None);
        let inferred = visitor.infer_type_from_expression(default_value);

        match inferred {
            Some(actual) if !Self::are_types_compatible_strict(actual, expected_type) => {
                self.add_error(
                    result,
                    "QFUNC009",
                    "DEFAULT_VALUE_TYPE_MISMATCH",
                    &format!(
                        "Default value type ({:?}) does not match parameter type ({:?}) for '{}' in function '{}'",
                        actual, expected_type, param.name, func_name
                    ),
                    &format!(
                        "Change default value to match {:?} or remove the type annotation",
                        expected_type
                    ),
                    param.position,
                );
            }
            None => {
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
            _ => {}
        }
    }

    fn validate_scopes(
        &self,
        func: &QuickFunction,
        result: &mut SectionAnalysisResult,
    ) {
        let scope_list = match &func.scope_list {
            Some(s) => s,
            None => {
                self.add_warning(
                    result,
                    "QFUNC_WARN006",
                    &format!(
                        "Function '{}' has no scope declaration — callable only within its definition context",
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
                continue;
            }
            if !Self::is_valid_data_path(scope) {
                self.add_error(
                    result,
                    "QFUNC010",
                    "INVALID_SCOPE_SYNTAX",
                    &format!("Function '{}' has invalid scope syntax: '{}'", func.name, scope),
                    "Scope must be 'global' or a valid dotted path (e.g. 'user.profile').",
                    func.position,
                );
                if self.should_halt(result) { return; }
            }
        }
    }

    fn validate_function_body(
        &self,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        result: &mut SectionAnalysisResult,
        scope_tracker: &mut LocalScopeTracker,
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
                "Add a function body with a return statement, or remove the function.",
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
                &format!("Reduce to {} or fewer statements", MAX_FUNCTION_BODY_STATEMENTS),
                func.position,
            );
            return;
        }

        // Reuse the pre-allocated scope tracker.
        scope_tracker.reset_with_params(&func.parameters);

        let mut return_path = ReturnPathAnalyzer::new(func.return_type.unwrap());
        let max_depth = Self::calculate_max_depth(body_length);

        for statement in &func.body {
            self.validate_statement(
                statement,
                func,
                symbol_table,
                scope_tracker,
                result,
                0,
                max_depth,
                &mut return_path,
            );
            if self.should_halt(result) { return; }
        }

        if !return_path.all_paths_return() {
            self.add_error(
                result,
                "QFUNC013",
                "NOT_ALL_PATHS_RETURN",
                &format!(
                    "Function '{}' (return type {:?}) does not return on all code paths",
                    func.name,
                    func.return_type.unwrap()
                ),
                "Ensure every branch (if/else, switch/miss) ends with a return statement.",
                func.position,
            );
        }

        self.check_for_unused_variables(func, scope_tracker, result);
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
        return_path: &mut ReturnPathAnalyzer,
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
                "Simplify deeply nested code structures.",
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
                    "Function '{}' exceeds maximum nesting depth of {}",
                    func.name, MAX_NESTING_DEPTH
                ),
                "Extract nested code into separate functions.",
                statement.position(),
            );
            return;
        }

        match statement {
            QuickFuncStatement::Return { value, .. } => {
                self.validate_return_statement(value, func, symbol_table, local_scope, result);
                return_path.add_return();
            }
            QuickFuncStatement::If { condition, then_branch, else_branch, .. } => {
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
                    return_path,
                );
            }
            QuickFuncStatement::Switch { expression, cases, default_case, .. } => {
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
                    return_path,
                );
            }
            QuickFuncStatement::Assignment { variable, value, .. } => {
                self.validate_assignment_statement(
                    variable, value, func, symbol_table, local_scope, result,
                );
            }
            QuickFuncStatement::ArithmeticAssignment { variable, operator, value, .. } => {
                self.validate_arithmetic_assignment_statement(
                    variable, operator, value, func, symbol_table, local_scope, result,
                );
            }
            QuickFuncStatement::ObjectCreation { variable, object, .. } => {
                self.validate_object_creation_statement(
                    variable, object, func, symbol_table, local_scope, result,
                );
            }
            QuickFuncStatement::Log { value, .. } => {
                self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);
            }
            QuickFuncStatement::ExpressionStatement { expression, .. } => {
                self.validate_expression(
                    expression, func, symbol_table, local_scope, result, max_depth,
                );
            }
            QuickFuncStatement::VariableDeclaration { .. } => {
                self.validate_variable_declaration_statement(
                    statement, func, symbol_table, local_scope, result,
                );
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
        let visitor = TypeInferenceVisitor::new(symbol_table, Some(local_variable_types));
        let return_value_type = visitor.infer_type_from_expression(value);
        let expected = func.return_type.unwrap();

        match return_value_type {
            Some(actual) if !Self::are_types_compatible_strict(actual, expected) => {
                self.add_error(
                    result,
                    "QFUNC015",
                    "RETURN_TYPE_MISMATCH",
                    &format!(
                        "Function '{}' returns {:?} but declared return type is {:?}",
                        func.name, actual, expected
                    ),
                    &format!(
                        "Change the return value to match {:?} or update the function return type",
                        expected
                    ),
                    value.position(),
                );
            }
            Some(actual) => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "Return type {:?} matches expected {:?}",
                        actual, expected
                    ));
                }
            }
            None => {
                self.add_warning(
                    result,
                    "QFUNC_WARN004",
                    &format!(
                        "Cannot infer return type in function '{}'. Expected: {:?}",
                        func.name, expected
                    ),
                    "QUICKFUNCS",
                    value.position(),
                );
            }
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
        let (declaration_type, is_mutable, variable_name, data_type, value, position) =
            match statement {
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

        if !Self::is_valid_identifier(variable_name) {
            self.add_error(
                result,
                "QFUNC067",
                "INVALID_VARIABLE_NAME",
                &format!(
                    "Invalid variable name '{}' in function '{}'",
                    variable_name, func.name
                ),
                "Variable names must start with a letter and contain only alphanumeric characters and underscores.",
                *position,
            );
            return;
        }

        if Keywords::is_data_type_keyword(variable_name) {
            let suggestion = format!(
                "Use a different name like 'my{}{}' or '{}Value'",
                variable_name.chars().next().unwrap_or('X').to_uppercase(),
                &variable_name[1..],
                variable_name
            );
            self.add_error(
                result,
                "QFUNC067B",
                "DATA_TYPE_KEYWORD_AS_VARIABLE",
                &format!(
                    "Variable '{}' in function '{}' cannot use a data type keyword as name",
                    variable_name, func.name
                ),
                &suggestion,
                *position,
            );
            return;
        }

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

        let max_depth = Self::calculate_max_depth(100);
        self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);

        // Infer type once; use for both compatibility check and scope registration.
        let local_variable_types = local_scope.get_all_variable_types();
        let visitor = TypeInferenceVisitor::new(symbol_table, Some(local_variable_types));
        let inferred_type = visitor.infer_type_from_expression(value);

        if let (Some(&declared), Some(inferred)) = (data_type.as_ref(), inferred_type) {
            if !Self::are_types_compatible_strict(inferred, declared) {
                self.add_error(
                    result,
                    "QFUNC071",
                    "VARIABLE_TYPE_MISMATCH",
                    &format!(
                        "Variable '{}' declared as {:?} but assigned value of type {:?}",
                        variable_name, declared, inferred
                    ),
                    &format!(
                        "Change the value to match {:?} or remove the type annotation",
                        declared
                    ),
                    *position,
                );
            }
        }

        let is_const = matches!(declaration_type, DeclarationType::Const) || !is_mutable;
        let effective_type = data_type.or(inferred_type);

        local_scope.add_variable(variable_name.clone(), effective_type, is_const);

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "Declared {} variable '{}' with type {:?}",
                if is_const { "immutable" } else { "mutable" },
                variable_name,
                effective_type
            ));
        }
        }
