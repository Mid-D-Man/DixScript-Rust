
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
use crate::Compiler::Core::SectionEnhancers::{
    QualifiedIdentifierKey, QualifiedIdentifierResolution, QualifiedIdentifierType,
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

// ── Operator validator (free function) ─────────────────────────────────────

#[inline]
fn is_valid_data_type(data_type: DataType) -> bool {
    matches!(
        data_type,
        DataType::Int
            | DataType::Long
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
            | DataType::TypedArray(_)   // typed collection — valid annotation
            | DataType::TypedTuple(_)
    )
}

/// `true` for numeric types eligible for arithmetic promotion.
#[inline]
fn is_numeric_type(dt: DataType) -> bool {
    matches!(dt, DataType::Int | DataType::Long | DataType::Float | DataType::Double | DataType::Enum)
}
/// `true` for types valid as bitwise/shift operands: int and long.
/// `long` was previously excluded, which caused spurious errors for any
/// bitwise or shift operation (&, |, ^, <<, >>, ~?, &=, |=, ^=, <<=, >>=)
/// performed on a `long`-typed variable or expression.
#[inline]
fn is_bitwise_operand_type(dt: DataType) -> bool {
    matches!(dt, DataType::Int | DataType::Long)
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
       Self::new_with_error_manager(operational_settings,ErrorManager::get_shared_instance())
    }
pub fn new_with_error_manager(
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
) -> Self {
    QuickFuncsSectionAnalyzer {
        error_manager,
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


        let cycle_validator = CycleDetectionValidator::new_with_error_manager(
            &self.operational_settings,   self.error_manager.clone(),

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

    // ── validate_default_value_type_strict ────────────────────────────────────────

fn validate_default_value_type_strict(
    &self,
    param:        &QuickFuncParam,
    func_name:    &str,
    symbol_table: &SymbolTable,
    result:       &mut SectionAnalysisResult,
) {
    let (default_value, expected_type) = match (&param.default_value, param.data_type) {
        (Some(v), Some(t)) => (v, t),
        _ => return,
    };

    let visitor  = TypeInferenceVisitor::new(symbol_table, None);
    let inferred = visitor.infer_type_from_expression(default_value);

    match inferred {
        Some(actual) if !Self::are_types_compatible_strict(actual, expected_type) => {
            self.add_error(
                result,
                "QFUNC009",
                "DEFAULT_VALUE_TYPE_MISMATCH",
                &format!(
                    "Default value type ({:?}) does not match parameter type ({:?}) \
                     for '{}' in function '{}'",
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
            // Cannot infer type — acceptable for complex default expressions.
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "Cannot infer default-value type for '{}' in '{}' — skipping",
                    param.name, func_name
                ));
            }
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

// ── validate_return_statement ─────────────────────────────────────────────────
//
// CHANGE: removed QFUNC_WARN004 ("cannot infer return type").
// The warning fired constantly for complex expressions — method chains,
// qualified identifiers before AST enhancement, arithmetic on dynamic types,
// conditional returns, etc.  When type inference succeeds AND the types are
// incompatible we still emit QFUNC015 (the real error).  When inference
// simply cannot determine the type we skip the check rather than spamming
// the user with noise.

fn validate_return_statement(
    &self,
    value:        &Expression,
    func:         &QuickFunction,
    symbol_table: &SymbolTable,
    local_scope:  &LocalScopeTracker,
    result:       &mut SectionAnalysisResult,
) {
    let max_depth = Self::calculate_max_depth(100);
    self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);

    let local_variable_types = local_scope.get_all_variable_types();
    let element_type_hints   = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_variable_types),
        Some(element_type_hints),
    );
    let return_value_type = visitor.infer_type_from_expression(value);
    let expected = func.return_type.unwrap();

    match return_value_type {
        // Any is the universal wildcard — always compatible.
        Some(actual) if actual == DataType::Any => {}

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
                    "Return type {:?} matches expected {:?} in '{}'",
                    actual, expected, func.name
                ));
            }
        }

        // Type cannot be inferred.  This is perfectly normal for complex
        // expressions: method chains, qualified identifiers (resolved only
        // after AST enhancement), arithmetic across dynamic types, etc.
        // Skip type checking rather than emitting a spurious warning.
        None => {
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "Cannot infer return expression type in '{}' — skipping type check \
                     (normal for complex or pre-enhancement expressions)",
                    func.name
                ));
            }
        }
    }
}


fn validate_variable_declaration_statement(
    &self,
    statement:    &QuickFuncStatement,
    func:         &QuickFunction,
    symbol_table: &SymbolTable,
    local_scope:  &mut LocalScopeTracker,
    result:       &mut SectionAnalysisResult,
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
            result, "QFUNC067", "INVALID_VARIABLE_NAME",
            &format!("Invalid variable name '{}' in function '{}'", variable_name, func.name),
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
            result, "QFUNC067B", "DATA_TYPE_KEYWORD_AS_VARIABLE",
            &format!(
                "Variable '{}' in function '{}' cannot use a data type keyword as name",
                variable_name, func.name
            ),
            &suggestion, *position,
        );
        return;
    }

    if Keywords::is_reserved_in_context(variable_name, "QUICKFUNCS") {
        self.add_error(
            result, "QFUNC068", "RESERVED_KEYWORD_AS_VARIABLE",
            &Keywords::get_keyword_usage_error(variable_name, "QUICKFUNCS"),
            &format!("Choose a different name for variable '{}'", variable_name),
            *position,
        );
        return;
    }

    if local_scope.has_variable(variable_name) {
        self.add_error(
            result, "QFUNC069", "VARIABLE_REDECLARATION",
            &format!("Variable '{}' already declared in function '{}'", variable_name, func.name),
            "Each variable must be declared only once. Use assignment to change its value.",
            *position,
        );
        return;
    }

    let max_depth = Self::calculate_max_depth(100);
    self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);

    let local_variable_types = local_scope.get_all_variable_types();
    let element_type_hints   = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_variable_types),
        Some(element_type_hints),
    );
    let inferred_type = visitor.infer_type_from_expression(value);

    // Type-compatibility check (only when both declared and inferred are known and not Any)
    if let (Some(&declared), Some(inferred)) = (data_type.as_ref(), inferred_type) {
        if inferred != DataType::Any && !Self::are_types_compatible_strict(inferred, declared) {
            self.add_error(
                result, "QFUNC071", "VARIABLE_TYPE_MISMATCH",
                &format!(
                    "Variable '{}' declared as {:?} but assigned value of type {:?}",
                    variable_name, declared, inferred
                ),
                &format!("Change the value to match {:?} or remove the type annotation", declared),
                *position,
            );
        }
    }

    let is_const     = matches!(declaration_type, DeclarationType::Const) || !is_mutable;
    let effective_type = data_type.or(inferred_type);

    // Infer element type for scope tracking.
    // TypedArray/TypedTuple declarations are authoritative — use the annotation directly
    // rather than trying to infer from the value (which would only give untyped Array/Tuple).
    let element_type: Option<DataType> = match effective_type {
        Some(DataType::TypedArray(elem)) => {
            // Declared annotation is ground truth
            Some(elem.to_data_type())
        }
        Some(DataType::TypedTuple(arr)) => {
            // First defined slot as representative element type
            arr[0].map(|e| e.to_data_type())
        }
        Some(DataType::Array) | Some(DataType::Tuple) => {
            // Untyped collection: infer from the assigned value
            visitor.infer_element_type_from_expression(value)
        }
        _ => None,
    };

    local_scope.add_variable_with_element_type(
        variable_name.clone(),
        effective_type,
        is_const,
        element_type,
    );

    if self.debug_config.is_verbose {
        self.error_manager.log_debug(&format!(
            "Declared {} variable '{}' type={:?} element_type={:?}",
            if is_const { "immutable" } else { "mutable" },
            variable_name,
            effective_type,
            element_type,
        ));
    }
            }

    // ==================== CONTROL FLOW VALIDATION ====================

    fn validate_if_statement(
    &self,
    condition:    &Expression,
    then_branch:  &[QuickFuncStatement],
    else_branch:  Option<&Vec<QuickFuncStatement>>,
    func:         &QuickFunction,
    symbol_table: &SymbolTable,
    local_scope:  &mut LocalScopeTracker,
    result:       &mut SectionAnalysisResult,
    nesting_depth: usize,
    max_depth:    usize,
    return_path:  &mut ReturnPathAnalyzer,
) {
    self.validate_expression(condition, func, symbol_table, local_scope, result, max_depth);

    let local_variable_types = local_scope.get_all_variable_types();
    let element_type_hints   = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_variable_types),
        Some(element_type_hints),
    );

    if let Some(cond_type) = visitor.infer_type_from_expression(condition) {
        // Any is permissive — skip the bool check entirely
        if cond_type != DataType::Bool && cond_type != DataType::Any {
            self.add_error(
                result,
                "QFUNC016",
                "NON_BOOLEAN_CONDITION",
                &format!(
                    "If statement condition must be boolean, got {:?}",
                    cond_type
                ),
                "Use comparison operators (==, !=, >, <, etc.) to create boolean conditions.",
                condition.position(),
            );
        }
    }

    let mut then_returns = ReturnPathAnalyzer::new(func.return_type.unwrap());
    let mut else_returns = ReturnPathAnalyzer::new(func.return_type.unwrap());

    for stmt in then_branch {
        self.validate_statement(
            stmt, func, symbol_table, local_scope, result,
            nesting_depth + 1, max_depth, &mut then_returns,
        );
    }

    if let Some(else_stmts) = else_branch {
        for stmt in else_stmts {
            self.validate_statement(
                stmt, func, symbol_table, local_scope, result,
                nesting_depth + 1, max_depth, &mut else_returns,
            );
        }
        if then_returns.all_paths_return() && else_returns.all_paths_return() {
            return_path.add_return();
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
        return_path: &mut ReturnPathAnalyzer,
    ) {
        self.validate_expression(expression, func, symbol_table, local_scope, result, max_depth);

        let has_default = default_case.is_some();
        let all_cases_return = cases.iter().all(|case| {
            let mut case_analyzer = ReturnPathAnalyzer::new(func.return_type.unwrap());
            for stmt in &case.statements {
                self.validate_statement(
                    stmt, func, symbol_table, local_scope, result,
                    nesting_depth + 1, max_depth, &mut case_analyzer,
                );
            }
            case_analyzer.all_paths_return()
        });

        let default_returns = if let Some(default) = default_case {
            let mut analyzer = ReturnPathAnalyzer::new(func.return_type.unwrap());
            for stmt in &default.statements {
                self.validate_statement(
                    stmt, func, symbol_table, local_scope, result,
                    nesting_depth + 1, max_depth, &mut analyzer,
                );
            }
            analyzer.all_paths_return()
        } else {
            false
        };

        if all_cases_return && has_default && default_returns {
            return_path.add_return();
        }
    }

    // ==================== ASSIGNMENT VALIDATION ====================

    fn validate_assignment_statement(
    &self,
    variable:     &str,
    value:        &Expression,
    func:         &QuickFunction,
    symbol_table: &SymbolTable,
    local_scope:  &mut LocalScopeTracker,
    result:       &mut SectionAnalysisResult,
) {
    if !Self::is_valid_identifier(variable) {
        self.add_error(
            result, "QFUNC017", "INVALID_VARIABLE_NAME",
            &format!("Invalid variable name '{}' in function '{}'", variable, func.name),
            "Variable names must start with a letter and contain only alphanumeric characters and underscores.",
            value.position(),
        );
        return;
    }

    if !local_scope.has_variable(variable) {
        self.add_error(
            result, "QFUNC072", "UNDECLARED_VARIABLE",
            &format!(
                "Variable '{}' used before declaration in function '{}'",
                variable, func.name
            ),
            &format!(
                "Declare it first: let {} = ...; or const {} = ...;",
                variable, variable
            ),
            value.position(),
        );
        return;
    }

    if local_scope.is_const(variable) {
        self.add_error(
            result, "QFUNC018", "CONST_REASSIGNMENT",
            &format!("Cannot reassign const variable '{}' in function '{}'", variable, func.name),
            "Use 'let mut' instead of 'const' or 'let' to allow mutation.",
            value.position(),
        );
        return;
    }

    let max_depth = Self::calculate_max_depth(100);
    self.validate_expression(value, func, symbol_table, local_scope, result, max_depth);

    let local_variable_types = local_scope.get_all_variable_types();
    let element_type_hints   = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_variable_types),
        Some(element_type_hints),
    );

    let existing_type = local_scope.get_variable_type(variable);
    let new_type      = visitor.infer_type_from_expression(value);

    match (existing_type, new_type) {
        (Some(existing), Some(new_t))
            if new_t != DataType::Any
                && !Self::are_types_compatible_strict(new_t, existing) =>
        {
            self.add_error(
                result, "QFUNC019", "TYPE_MISMATCH_REASSIGNMENT",
                &format!(
                    "Cannot assign {:?} to variable '{}' of type {:?}",
                    new_t, variable, existing
                ),
                "Variable types cannot change once assigned (unless type is 'any').",
                value.position(),
            );
        }
        (None, Some(new_t)) if new_t != DataType::Any => {
            local_scope.update_variable_type(variable, new_t);

            // Update element type for any collection being re-assigned.
            // TypedArray/TypedTuple annotations on the new value are
            // authoritative; fall back to value inference for untyped collections.
            if matches!(
                new_t,
                DataType::Array | DataType::Tuple
                    | DataType::TypedArray(_) | DataType::TypedTuple(_)
            ) {
                let new_elem = match new_t {
                    DataType::TypedArray(elem) => Some(elem.to_data_type()),
                    DataType::TypedTuple(arr)  => arr[0].map(|e| e.to_data_type()),
                    _ => visitor.infer_element_type_from_expression(value),
                };
                local_scope.update_variable_element_type(variable, new_elem);
            }

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "Inferred type {:?} for variable '{}'", new_t, variable
                ));
            }
        }
        _ => {}
    }
}

    fn validate_arithmetic_assignment_statement(
    &self,
    variable:     &str,
    operator:     &str,
    value:        &Expression,
    func:         &QuickFunction,
    symbol_table: &SymbolTable,
    local_scope:  &LocalScopeTracker,
    result:       &mut SectionAnalysisResult,
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
            "Declare the variable before using arithmetic assignment.",
            value.position(),
        );
        return;
    }

    if local_scope.is_const(variable) {
        self.add_error(
            result,
            "QFUNC021",
            "CONST_REASSIGNMENT",
            &format!("Cannot modify const variable '{}' with '{}'", variable, operator),
            "Remove 'const' to make the variable mutable.",
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

    // Use element hints so element-returning method results resolve correctly
    let local_variable_types = local_scope.get_all_variable_types();
    let element_type_hints   = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_variable_types),
        Some(element_type_hints),
    );

    if let (Some(var_t), Some(val_t)) = (
        local_scope.get_variable_type(variable),
        visitor.infer_type_from_expression(value),
    ) {
        // Skip operand checks when either side is Any (type deferred / unknown)
        if var_t != DataType::Any && val_t != DataType::Any {
            self.validate_arithmetic_operation(
                operator, var_t, val_t, &func.name, result, value.position(),
            );
        }
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
                &format!("Invalid variable name '{}' in function '{}'", variable, func.name),
                "Variable names must start with a letter and contain only alphanumeric characters and underscores.",
                object.position(),
            );
            return;
        }

        if local_scope.is_const(variable) {
            self.add_error(
                result,
                "QFUNC024",
                "CONST_REASSIGNMENT",
                &format!("Cannot reassign const variable '{}' in function '{}'", variable, func.name),
                "Remove 'const' to allow reassignment.",
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
        let referenced = collector.collect_from_function(func);

        for var_name in local_scope.get_declared_variable_names() {
            if !referenced.contains(var_name) {
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
            }
        }

        if self.debug_config.is_verbose {
            let declared = local_scope.get_declared_variable_names().count();
            self.error_manager.log_debug(&format!(
                "Variable usage: {}/{} variables used",
                referenced.len(),
                declared
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
                &format!("Maximum expression depth exceeded in function '{}'", func.name),
                "Simplify deeply nested expressions.",
                expr.position(),
            );
            return;
        }

        match expr {
            Expression::Identifier { name, .. } => {
                self.validate_identifier(name, &func.name, local_scope, symbol_table, result, expr.position());
            }
            // position is now captured explicitly so it can be passed to validate_qualified_identifier
            Expression::QualifiedIdentifier { parts, arguments, position } => {
                self.validate_qualified_identifier(
                    parts, arguments.as_ref(), func, symbol_table, local_scope, result, max_depth, *position,
                );
            }
            Expression::QuickFuncCall { name, arguments, .. } => {
                self.validate_quick_func_call(
                    name, arguments, func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::ImportedFunctionCall {
                namespace_name, function_name, arguments, ..
            } => {
                self.validate_imported_function_call(
                    namespace_name, function_name, arguments,
                    func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::InstanceMethodCall { instance, method_name, arguments, .. } => {
                self.validate_instance_method_call(
                    instance, method_name, arguments,
                    func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::StaticMethodCall { object_name, method_name, arguments, .. } => {
                self.validate_static_method_call(
                    object_name, method_name, arguments,
                    func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::EnumAccess { namespace_name, enum_name, value, position } => {
                self.validate_enum_access(
                    namespace_name.as_deref(), enum_name, value, &func.name, symbol_table, result, *position,
                );
            }
            Expression::ArithmeticOp { left, right, operator, .. } => {
                self.validate_arithmetic_op_expression(
                    left, right, operator, func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::BitwiseOp { left, right, operator, .. } => {
                self.validate_bitwise_op_expression(
                    left, right, operator, func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::ComparisonOp { left, right, operator, .. } => {
                self.validate_comparison_op_expression(
                    left, right, operator, func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::LogicalOp { left, right, operator, .. } => {
                self.validate_logical_op_expression(
                    left, right, operator, func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::UnaryOp { operand, operator, .. } => {
                self.validate_unary_op_expression(
                    operand, operator, func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::Conditional { condition, true_value, false_value, .. } => {
                self.validate_conditional_expression(
                    condition, true_value, false_value,
                    func, symbol_table, local_scope, result, max_depth,
                );
            }
            Expression::PropertyAccess { object, .. } => {
                self.validate_expression(
                    object, func, symbol_table, local_scope, result, max_depth - 1,
                );
            }
            Expression::IndexAccess { object, index, .. } => {
                self.validate_expression(object, func, symbol_table, local_scope, result, max_depth - 1);
                self.validate_expression(index, func, symbol_table, local_scope, result, max_depth - 1);
            }
            Expression::Value { value, .. } => {
                self.validate_value(value, func, symbol_table, local_scope, result);
            }
            Expression::Parenthesized { expression, .. } => {
                self.validate_expression(
                    expression, func, symbol_table, local_scope, result, max_depth - 1,
                );
            }
            Expression::TypeCast { expression, .. } => {
                self.validate_expression(
                    expression, func, symbol_table, local_scope, result, max_depth - 1,
                );
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

    // position parameter added — required so the QualifiedIdentifierKey built here
    // matches the key the resolver builds from the same expression node.
    fn validate_qualified_identifier(
        &self,
        parts: &[String],
        arguments: Option<&Vec<Expression>>,
        func: &QuickFunction,
        symbol_table: &SymbolTable,
        local_scope: &LocalScopeTracker,
        result: &mut SectionAnalysisResult,
        max_depth: usize,
        position: Position,
    ) {
        if parts.len() < 2 {
            return;
        }

        let first = &parts[0];
        let second = &parts[1];
        let is_call = arguments.is_some();

        // Helper closure: builds the key and inserts a resolution into the map.
        // Defined as a local macro-like pattern to keep call sites readable.
        let insert = |result: &mut SectionAnalysisResult,
                      resolved_type: QualifiedIdentifierType,
                      context: Option<String>| {
            let key = QualifiedIdentifierKey {
                position,
                parts: parts.to_vec(),
                is_call,
            };
            let resolution = QualifiedIdentifierResolution::new(
                resolved_type,
                context,
                parts.to_vec(),
                is_call,
                position,
            );
            result.qualified_id_resolutions.insert(key, resolution);
        };

        // Local variable or parameter — property access or instance method call.
        if local_scope.has_variable(first) || local_scope.has_parameter(first) {
            if let Some(args) = arguments {
                for arg in args {
                    self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
                }
            }
            insert(result, QualifiedIdentifierType::ObjectPropertyAccess, Some("local".to_string()));
            return;
        }

        // Local enum access: EnumName.FIELD (2 parts, no call).
        if parts.len() == 2 && arguments.is_none() && symbol_table.has_enum(first) {
            if !symbol_table.has_enum_field(first, second) {
                if let Some(fields) = symbol_table.try_get_enum(first) {
                    let valid: Vec<&String> = fields.keys().collect();
                    self.add_error(
                        result,
                        "QFUNC052",
                        "ENUM_VALUE_NOT_FOUND",
                        &format!("Enum '{}' does not have value '{}'", first, second),
                        &format!("Valid values: {}", valid.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                        position,
                    );
                }
            }
            insert(result, QualifiedIdentifierType::LocalEnumAccess, Some(first.clone()));
            return;
        }

        // Namespace access (imported file).
        if symbol_table.is_imported_namespace(first) {
            self.validate_namespace_access(
                parts, arguments, func, symbol_table, local_scope, result, max_depth,
            );

            // Determine the precise resolution type for the enhancer.
            let resolved_type = if parts.len() == 3 && arguments.is_none() {
                QualifiedIdentifierType::ImportedEnumAccess
            } else if parts.len() == 2 && arguments.is_none() {
                QualifiedIdentifierType::NamespaceEnumReference
            } else {
                QualifiedIdentifierType::ImportedFunctionCall
            };

            insert(result, resolved_type, Some(first.clone()));
            return;
        }

        // Builtin static object access: Math.sqrt(), DateTime.now(), etc.
        if has_static_object(first) {
            self.validate_static_object_access(
                parts, arguments, func, symbol_table, local_scope, result, max_depth,
            );
            insert(result, QualifiedIdentifierType::StaticObjectAccess, Some(first.clone()));
            return;
        }

        // DATA section variable — property access is valid at runtime.
        if symbol_table.has_data_variable(first) {
            if let Some(args) = arguments {
                for arg in args {
                    self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
                }
            }
            insert(result, QualifiedIdentifierType::ObjectPropertyAccess, Some("data".to_string()));
            return;
        }

        // Unknown — will be resolved at runtime; emit a warning and record as unknown.
        self.add_warning(
            result,
            "QFUNC_WARN001",
            &format!(
                "Identifier '{}' not found in scope — will be resolved at runtime",
                first
            ),
            "QUICKFUNCS",
            position,
        );

        if let Some(args) = arguments {
            for arg in args {
                self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
            }
        }

        insert(result, QualifiedIdentifierType::ObjectPropertyAccess, Some("unknown".to_string()));
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
        let ns = &parts[0];
        let member = &parts[1];

        if parts.len() == 2 {
            if let Some(args) = arguments {
                // Namespaced function call.
                match symbol_table.get_namespaced_function(ns, member) {
                    None => {
                        self.add_error(
                            result,
                            "QFUNC045",
                            "IMPORTED_FUNCTION_NOT_FOUND",
                            &format!("Function '{}' not found in namespace '{}'", member, ns),
                            "",
                            Position::UNKNOWN,
                        );
                        return;
                    }
                    Some(info) => {
                        let expected = info.signature.parameters.len();
                        if args.len() != expected {
                            self.add_error(
                                result,
                                "QFUNC046",
                                "PARAMETER_COUNT_MISMATCH",
                                &format!(
                                    "Function '{}.{}' expects {} parameter(s) but got {}",
                                    ns, member, expected, args.len()
                                ),
                                "",
                                Position::UNKNOWN,
                            );
                        }
                    }
                }
                for arg in args {
                    self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
                }
            } else {
                // Namespaced enum reference.
                if symbol_table.get_namespaced_enum(ns, member).is_none() {
                    self.add_error(
                        result,
                        "QFUNC055",
                        "NAMESPACE_MEMBER_NOT_FOUND",
                        &format!("Namespace '{}' does not have member '{}'", ns, member),
                        "Check the imported file for available functions and enums.",
                        Position::UNKNOWN,
                    );
                }
            }
        } else if parts.len() == 3 {
            // Imported enum access: ns.Enum.Value
            let enum_name = &parts[1];
            let enum_value = &parts[2];

            match symbol_table.get_namespaced_enum(ns, enum_name) {
                None => {
                    self.add_error(
                        result,
                        "QFUNC054",
                        "IMPORTED_ENUM_NOT_FOUND",
                        &format!("Namespace '{}' does not have enum '{}'", ns, enum_name),
                        "Check the imported file for available enums.",
                        Position::UNKNOWN,
                    );
                }
                Some(fields) if !fields.contains_key(enum_value) => {
                    let valid: Vec<&String> = fields.keys().collect();
                    self.add_error(
                        result,
                        "QFUNC056",
                        "ENUM_VALUE_NOT_FOUND",
                        &format!("Enum '{}.{}' does not have value '{}'", ns, enum_name, enum_value),
                        &format!("Valid values: {}", valid.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                        Position::UNKNOWN,
                    );
                }
                _ => {}
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
                    &format!("Static object '{}' has no method '{}'", object_name, method_name),
                    "",
                    Position::UNKNOWN,
                );
            }
            for arg in args {
                self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
            }
        }
    }

    // ==================== OPERATOR EXPRESSION VALIDATORS ====================

fn validate_arithmetic_op_expression(
    &self,
    left:         &Expression,
    right:        &Expression,
    operator:     &str,
    func:         &QuickFunction,
    symbol_table: &SymbolTable,
    local_scope:  &LocalScopeTracker,
    result:       &mut SectionAnalysisResult,
    max_depth:    usize,
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

    self.validate_expression(left,  func, symbol_table, local_scope, result, max_depth - 1);
    self.validate_expression(right, func, symbol_table, local_scope, result, max_depth - 1);

    let local_variable_types = local_scope.get_all_variable_types();
    let element_type_hints   = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_variable_types),
        Some(element_type_hints),
    );

    let lt = visitor.infer_type_from_expression(left);
    let rt = visitor.infer_type_from_expression(right);

    if let (Some(l), Some(r)) = (lt, rt) {
        // String concatenation path
        if operator == "+" {
            match (l, r) {
                (DataType::String, DataType::String) => return,
                (DataType::String, _) | (_, DataType::String) => {
                    // Only error if the non-string side is a known concrete type
                    let other = if l == DataType::String { r } else { l };
                    if other != DataType::Any {
                        self.add_error(
                            result,
                            "QFUNC026",
                            "INVALID_STRING_OPERATION",
                            &format!(
                                "Cannot concatenate string with {:?} in function '{}'",
                                other, func.name
                            ),
                            "Use only string + string, or convert to string first.",
                            left.position(),
                        );
                    }
                    return;
                }
                _ => {}
            }
        }

        // Numeric operand checks — skip for Any (type is unknown/deferred)
        if l != DataType::Any && !is_numeric_type(l) {
            self.add_error(
                result,
                "QFUNC027",
                "NON_NUMERIC_OPERAND",
                &format!(
                    "Left operand of '{}' must be numeric, got {:?} in '{}'",
                    operator, l, func.name
                ),
                "Use int, float, or double.",
                left.position(),
            );
        }
        if r != DataType::Any && !is_numeric_type(r) {
            self.add_error(
                result,
                "QFUNC028",
                "NON_NUMERIC_OPERAND",
                &format!(
                    "Right operand of '{}' must be numeric, got {:?} in '{}'",
                    operator, r, func.name
                ),
                "Use int, float, or double.",
                right.position(),
            );
        }
    }
}

    fn validate_bitwise_op_expression(
    &self,
    left:         &Expression,
    right:        &Expression,
    operator:     &str,
    func:         &QuickFunction,
    symbol_table: &SymbolTable,
    local_scope:  &LocalScopeTracker,
    result:       &mut SectionAnalysisResult,
    max_depth:    usize,
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

    self.validate_expression(left,  func, symbol_table, local_scope, result, max_depth - 1);
    self.validate_expression(right, func, symbol_table, local_scope, result, max_depth - 1);

    let local_variable_types = local_scope.get_all_variable_types();
    let element_type_hints   = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_variable_types),
        Some(element_type_hints),
    );

    for (side, expr, code) in [
        ("Left",  left,  "QFUNC030"),
        ("Right", right, "QFUNC031"),
    ] {
        if let Some(t) = visitor.infer_type_from_expression(expr) {
            // FIX: was `t != DataType::Int`, rejecting valid `long` operands.
            // Skip the check for Any — type is unknown/deferred.
            if !is_bitwise_operand_type(t) && t != DataType::Any {
                self.add_error(
                    result,
                    code,
                    "NON_INT_BITWISE_OPERAND",
                    &format!(
                        "{} operand of '{}' must be int or long, got {:?} in '{}'",
                        side, operator, t, func.name
                    ),
                    "Convert to int/long or use arithmetic operators instead.",
                    expr.position(),
                );
            }
        }
    }
}


    // ── Operator expression validators ─────────────────────────────────────────

fn validate_comparison_op_expression(
    &self,
    left:         &Expression,
    right:        &Expression,
    operator:     &str,
    func:         &QuickFunction,
    symbol_table: &SymbolTable,
    local_scope:  &LocalScopeTracker,
    result:       &mut SectionAnalysisResult,
    max_depth:    usize,
) {
    if !is_valid_comparison_operator(operator) {
        self.add_error(
            result,
            "QFUNC032",
            "INVALID_COMPARISON_OPERATOR",
            &format!("Invalid comparison operator '{}' in function '{}'", operator, func.name),
            "Valid operators: ==, !=, >, <, >=, <=",
            left.position(),
        );
        return;
    }

    self.validate_expression(left,  func, symbol_table, local_scope, result, max_depth - 1);
    self.validate_expression(right, func, symbol_table, local_scope, result, max_depth - 1);

    // Use full element-hint context so element-returning methods resolve correctly
    let local_variable_types = local_scope.get_all_variable_types();
    let element_type_hints   = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_variable_types),
        Some(element_type_hints),
    );

    let lt = visitor.infer_type_from_expression(left);
    let rt = visitor.infer_type_from_expression(right);

    if let (Some(l), Some(r)) = (lt, rt) {
        // Any on either side — permissive, skip further checks
        if l == DataType::Any || r == DataType::Any { return; }

        if operator == "==" || operator == "!=" {
            if !Self::are_types_comparable(l, r) {
                self.add_warning(
                    result,
                    "QFUNC_WARN002",
                    &format!(
                        "Comparing incompatible types {:?} and {:?} in function '{}'",
                        l, r, func.name
                    ),
                    "QUICKFUNCS",
                    left.position(),
                );
            }
            return;
        }

        // Relational operators require numeric operands
        if !is_numeric_type(l) || !is_numeric_type(r) {
            self.add_error(
                result,
                "QFUNC033",
                "NON_NUMERIC_COMPARISON",
                &format!(
                    "Operator '{}' requires numeric types, got {:?} and {:?} in '{}'",
                    operator, l, r, func.name
                ),
                "Use numeric types (int, float, double) for relational comparisons.",
                left.position(),
            );
        }
    }
}

    fn validate_logical_op_expression(
    &self,
    left:         &Expression,
    right:        &Expression,
    operator:     &str,
    func:         &QuickFunction,
    symbol_table: &SymbolTable,
    local_scope:  &LocalScopeTracker,
    result:       &mut SectionAnalysisResult,
    max_depth:    usize,
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

    self.validate_expression(left,  func, symbol_table, local_scope, result, max_depth - 1);
    self.validate_expression(right, func, symbol_table, local_scope, result, max_depth - 1);

    let local_variable_types = local_scope.get_all_variable_types();
    let element_type_hints   = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_variable_types),
        Some(element_type_hints),
    );

    for (code, expr, side) in [
        ("QFUNC035", left,  "Left"),
        ("QFUNC036", right, "Right"),
    ] {
        if let Some(t) = visitor.infer_type_from_expression(expr) {
            // Any and Bool are both acceptable — skip the error
            if t != DataType::Bool && t != DataType::Any {
                self.add_error(
                    result,
                    code,
                    "NON_BOOL_LOGICAL_OPERAND",
                    &format!(
                        "{} operand of '{}' must be bool, got {:?} in '{}'",
                        side, operator, t, func.name
                    ),
                    "Use comparison operators to create boolean values.",
                    expr.position(),
                );
            }
        }
    }
}

fn validate_unary_op_expression(
    &self,
    operand:      &Expression,
    operator:     &str,
    func:         &QuickFunction,
    symbol_table: &SymbolTable,
    local_scope:  &LocalScopeTracker,
    result:       &mut SectionAnalysisResult,
    max_depth:    usize,
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

    let local_variable_types = local_scope.get_all_variable_types();
    let element_type_hints   = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_variable_types),
        Some(element_type_hints),
    );

    if let Some(ot) = visitor.infer_type_from_expression(operand) {
        if ot == DataType::Any { return; } // skip type checks for Any

        match operator {
            "!" | "not" if ot != DataType::Bool => {
                self.add_error(
                    result,
                    "QFUNC038",
                    "NON_BOOL_NOT_OPERAND",
                    &format!(
                        "Logical NOT requires bool, got {:?} in '{}'",
                        ot, func.name
                    ),
                    "Use a comparison to create a boolean value.",
                    operand.position(),
                );
            }
            // FIX: was `ot != DataType::Int`, rejecting valid `long` operands.
            "~?" if !is_bitwise_operand_type(ot) => {
                self.add_error(
                    result,
                    "QFUNC039",
                    "NON_INT_BITWISE_NOT",
                    &format!(
                        "Bitwise NOT (~?) requires int or long, got {:?} in '{}'",
                        ot, func.name
                    ),
                    "Convert to int or long before using bitwise NOT.",
                    operand.position(),
                );
            }
            "-" | "+" if !is_numeric_type(ot) => {
                self.add_error(
                    result,
                    "QFUNC040",
                    "NON_NUMERIC_UNARY",
                    &format!(
                        "Unary '{}' requires numeric type, got {:?} in '{}'",
                        operator, ot, func.name
                    ),
                    "Use int, float, or double.",
                    operand.position(),
                );
            }
            _ => {}
        }
    }
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
        match (left_type, right_type) {
            (DataType::String, DataType::String) => return,
            (DataType::String, _) | (_, DataType::String) => {
                self.add_error(
                    result, "QFUNC061", "INVALID_STRING_CONCAT_ASSIGN",
                    &format!("Cannot use '+=' to concatenate string with non-string in function '{}'", function_name),
                    "Use only string += string, or convert to string first.", position,
                );
                return;
            }
            _ => {}
        }
    }

    if !is_numeric_type(left_type) {
        self.add_error(
            result, "QFUNC062", "NON_NUMERIC_ARITHMETIC_ASSIGN",
            &format!("Arithmetic assignment '{}' requires numeric type, left operand is {:?}", op, left_type),
            "Use int, float, or double.", position,
        );
    }
    if !is_numeric_type(right_type) {
        self.add_error(
            result, "QFUNC063", "NON_NUMERIC_ARITHMETIC_ASSIGN",
            &format!("Arithmetic assignment '{}' requires numeric type, right operand is {:?}", op, right_type),
            "Use int, float, or double.", position,
        );
    }

    // FIX: bitwise compound-assignment operators previously required
    // DataType::Int exactly, rejecting `long` operands — this is the
    // >>= bug: `x >>= 1` on a `long` variable would incorrectly error.
    if matches!(op, "&=" | "|=" | "^=" | "<<=" | ">>=") {
        if !is_bitwise_operand_type(left_type) {
            self.add_error(
                result, "QFUNC064", "NON_INT_BITWISE_ASSIGN",
                &format!("Bitwise assignment '{}' requires int or long, got {:?}", op, left_type),
                "Convert to int or long before using bitwise assignment.", position,
            );
        }
        if !is_bitwise_operand_type(right_type) {
            self.add_error(
                result, "QFUNC065", "NON_INT_BITWISE_ASSIGN",
                &format!("Bitwise assignment '{}' requires int or long, got {:?}", op, right_type),
                "Convert to int or long before using bitwise assignment.", position,
            );
        }
    }
}
// ── validate_conditional_expression ──────────────────────────────────────────
//
// CHANGE: tightened QFUNC_WARN003.
// Previously warned whenever inferred branch types were "incompatible".
// Now only warns when BOTH sides are simple primitive types (int/float/
// string/bool/enum/date/timestamp) so that collections, objects, and any
// expression where inference may return a simplified or base type do not
// produce false positives.

fn validate_conditional_expression(
    &self,
    condition:    &Expression,
    true_value:   &Expression,
    false_value:  &Expression,
    func:         &QuickFunction,
    symbol_table: &SymbolTable,
    local_scope:  &LocalScopeTracker,
    result:       &mut SectionAnalysisResult,
    max_depth:    usize,
) {
    self.validate_expression(
        condition, func, symbol_table, local_scope, result, max_depth - 1,
    );

    let local_variable_types = local_scope.get_all_variable_types();
    let element_type_hints   = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_variable_types),
        Some(element_type_hints),
    );

    if let Some(ct) = visitor.infer_type_from_expression(condition) {
        if ct != DataType::Bool && ct != DataType::Any {
            self.add_error(
                result,
                "QFUNC041",
                "NON_BOOL_TERNARY_CONDITION",
                &format!("Ternary condition must be bool, got {:?} in '{}'", ct, func.name),
                "Use comparison operators to create a boolean condition.",
                condition.position(),
            );
        }
    }

    self.validate_expression(
        true_value, func, symbol_table, local_scope, result, max_depth - 1,
    );
    self.validate_expression(
        false_value, func, symbol_table, local_scope, result, max_depth - 1,
    );

    let tt = visitor.infer_type_from_expression(true_value);
    let ft = visitor.infer_type_from_expression(false_value);

    if let (Some(t), Some(f)) = (tt, ft) {
        // Only warn when BOTH sides are known simple primitives that are clearly
        // incompatible.  For collections, objects, Any, or inferred base-types
        // (e.g. Array when the real type is TypedArray) skip the warning to
        // avoid false positives caused by the inference gap.
        #[inline]
        fn is_simple_primitive(dt: DataType) -> bool {
            matches!(
                dt,
                DataType::Int
                    | DataType::Long
                    | DataType::Float
                    | DataType::Double
                    | DataType::String
                    | DataType::Bool
                    | DataType::Enum
                    | DataType::Date
                    | DataType::Timestamp
            )
        }

        if t != DataType::Any
            && f != DataType::Any
            && is_simple_primitive(t)
            && is_simple_primitive(f)
            && !Self::are_types_comparable(t, f)
        {
            self.add_warning(
                result,
                "QFUNC_WARN003",
                &format!(
                    "Ternary branches have incompatible types {:?} and {:?} in '{}'",
                    t, f, func.name
                ),
                "QUICKFUNCS",
                condition.position(),
            );
        }
    }
            }
    // ==================== CALL VALIDATORS ====================

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
        if local_scope.has_variable(name) {
            for arg in arguments {
                self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
            }
            return;
        }

        if !symbol_table.has_function(name) {
            self.add_error(
                result, "QFUNC042", "FUNCTION_NOT_FOUND",
                &format!("Function '{}' is not defined in @QUICKFUNCS", name),
                "Define the function in @QUICKFUNCS or check the spelling.",
                Position::UNKNOWN,
            );
            return;
        }

        if let Some(sig) = symbol_table.try_get_function(name) {
            let expected = sig.parameters.len();
            if arguments.len() != expected {
                self.add_error(
                    result, "QFUNC043", "WRONG_ARGUMENT_COUNT",
                    &format!(
                        "Function '{}' expects {} arguments, got {}",
                        name, expected, arguments.len()
                    ),
                    &format!("Check the function signature: {}", sig),
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
        if local_scope.has_variable(namespace_name) {
            for arg in arguments {
                self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
            }
            return;
        }

        if !symbol_table.is_imported_namespace(namespace_name) {
            self.add_error(
                result, "QFUNC044", "NAMESPACE_NOT_FOUND",
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

        match symbol_table.get_namespaced_function(namespace_name, function_name) {
            None => {
                let suggestion = symbol_table
                    .try_get_namespace(namespace_name)
                    .map(|ns| {
                        let names: Vec<&String> = ns.functions.keys().collect();
                        format!(
                            "Available functions: {}",
                            names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        )
                    })
                    .unwrap_or_default();

                self.add_error(
                    result, "QFUNC045", "IMPORTED_FUNCTION_NOT_FOUND",
                    &format!(
                        "Function '{}' not found in namespace '{}'",
                        function_name, namespace_name
                    ),
                    &suggestion,
                    Position::UNKNOWN,
                );
            }
            Some(info) => {
                let expected = info.signature.parameters.len();
                if arguments.len() != expected {
                    self.add_error(
                        result, "QFUNC046", "PARAMETER_COUNT_MISMATCH",
                        &format!(
                            "Function '{}.{}' expects {} parameter(s) but got {}",
                            namespace_name, function_name, expected, arguments.len()
                        ),
                        &format!("Expected: {}", info.signature),
                        Position::UNKNOWN,
                    );
                }
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "Validated {}.{}() — return type {:?}",
                        namespace_name, function_name, info.signature.return_type
                    ));
                }
            }
        }

        for arg in arguments {
            self.validate_expression(arg, func, symbol_table, local_scope, result, max_depth - 1);
        }
    }

    fn validate_instance_method_call(
    &self,
    instance:     &Expression,
    method_name:  &str,
    arguments:    &[Expression],
    func:         &QuickFunction,
    symbol_table: &SymbolTable,
    local_scope:  &LocalScopeTracker,
    result:       &mut SectionAnalysisResult,
    max_depth:    usize,
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
            "Break up the method chain into intermediate variables.",
            instance.position(),
        );
        return;
    }

    self.validate_expression(instance, func, symbol_table, local_scope, result, max_depth - 1);

    // Element hints are essential here: .first()/.last() on a tracked array
    // would otherwise infer as Any and skip method validation entirely.
    let local_variable_types = local_scope.get_all_variable_types();
    let element_type_hints   = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_variable_types),
        Some(element_type_hints),
    );

    if let Some(inst_type) = visitor.infer_type_from_expression(instance) {
        // When instance type is Any (runtime-shaped / unknown), skip method
        // existence check to avoid false INSTANCE_METHOD_NOT_FOUND errors.
        if inst_type != DataType::Any {
            // Strip typed-collection wrappers for DixType lookup
            let lookup_type = inst_type.base_collection_type();
            if let Some(dix_type) = Self::convert_data_type_to_dix_type(lookup_type) {
                if !has_instance_method(dix_type, method_name) {
                    self.add_error(
                        result,
                        "QFUNC047",
                        "INSTANCE_METHOD_NOT_FOUND",
                        &format!(
                            "Type '{:?}' has no instance method '{}'",
                            inst_type, method_name
                        ),
                        &format!("Type '{:?}' has no such method.", inst_type),
                        instance.position(),
                    );
                }
            }
        }
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
                result, "QFUNC049", "STATIC_OBJECT_NOT_FOUND",
                &format!("Static object '{}' is not defined", object_name),
                "Available static objects: Math, DateTime, Array, Random, Enum, Guid, IpAddress, Dix",
                Position::UNKNOWN,
            );
        } else if !has_static_method(object_name, method_name) {
            self.add_error(
                result, "QFUNC050", "STATIC_METHOD_NOT_FOUND",
                &format!("Static object '{}' has no method '{}'", object_name, method_name),
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
        if self.debug_config.is_verbose {
            let fqn = match namespace_name {
                Some(ns) => format!("{}.{}.{}", ns, enum_name, value),
                None => format!("{}.{}", enum_name, value),
            };
            self.error_manager.log_debug(&format!("Validating enum access: {}", fqn));
        }

        match namespace_name {
            Some(ns) => {
                match symbol_table.get_namespaced_enum(ns, enum_name) {
                    None => {
                        let suggestion = symbol_table
                            .try_get_namespace(ns)
                            .map(|n| {
                                let names: Vec<&String> = n.enums.keys().collect();
                                if names.is_empty() {
                                    String::new()
                                } else {
                                    format!("Available enums: {}", names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "))
                                }
                            })
                            .unwrap_or_default();

                        self.add_error(
                            result, "QFUNC055", "IMPORTED_ENUM_NOT_FOUND",
                            &format!("Enum '{}' not found in namespace '{}'", enum_name, ns),
                            &suggestion, position,
                        );
                    }
                    Some(fields) if !fields.contains_key(value) => {
                        let valid: Vec<&String> = fields.keys().collect();
                        self.add_error(
                            result, "QFUNC056", "ENUM_VALUE_NOT_FOUND",
                            &format!("Enum '{}.{}' does not have value '{}'", ns, enum_name, value),
                            &format!("Valid values: {}", valid.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                            position,
                        );
                    }
                    _ => {}
                }
            }
            None => {
                if !symbol_table.has_enum(enum_name) {
                    self.add_error(
                        result, "QFUNC052", "ENUM_NOT_FOUND",
                        &format!("Enum '{}' not defined in @ENUMS section", enum_name),
                        "Define the enum in @ENUMS or check the spelling.", position,
                    );
                    return;
                }
                if !symbol_table.has_enum_field(enum_name, value) {
                    if let Some(fields) = symbol_table.try_get_enum(enum_name) {
                        let valid: Vec<&String> = fields.keys().collect();
                        self.add_error(
                            result, "QFUNC053", "ENUM_VALUE_NOT_FOUND",
                            &format!("Enum '{}' does not have value '{}' called in '{}'", enum_name, value,function_name),
                            &format!("Valid values: {}", valid.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                            position,
                        );
                    }
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
                        result, "QFUNC057", "ARRAY_TOO_LARGE",
                        &format!("Array has {} elements, exceeds limit of {}", values.len(), MAX_ARRAY_ELEMENTS),
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
                        result, "QFUNC058", "OBJECT_TOO_LARGE",
                        &format!("Object has {} properties, exceeds limit of {}", properties.len(), MAX_OBJECT_PROPERTIES),
                        &format!("Reduce to {} or fewer properties", MAX_OBJECT_PROPERTIES),
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
                        result, "QFUNC059", "TUPLE_TOO_LARGE",
                        &format!("Tuple has {} arguments, exceeds limit of {}", arguments.len(), MAX_TUPLE_ARGUMENTS),
                        &format!("Reduce tuple size to {} or fewer arguments", MAX_TUPLE_ARGUMENTS),
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
            _ => {}
        }
    }

    fn validate_array_homogeneity(
    &self,
    values:          &[Value],
    function_name:   &str,
    local_scope:     &LocalScopeTracker,
    symbol_table:    &SymbolTable,
    result:          &mut SectionAnalysisResult,
    position:        Position,
) {
    if values.len() < 2 {
        return;
    }

    let local_types       = local_scope.get_all_variable_types();
    let element_type_hints = local_scope.get_all_element_type_hints();
    let visitor = TypeInferenceVisitor::new_with_element_hints(
        symbol_table,
        Some(local_types),
        Some(element_type_hints),
    );

    let first_type = match visitor.infer_type_from_value(&values[0]) {
        Some(t) => t,
        None    => return, // Can't determine first type — skip homogeneity check
    };

    // Any-typed first element — can't make a useful assertion
    if first_type == DataType::Any {
        return;
    }

    for (i, element) in values.iter().enumerate().skip(1) {
        match visitor.infer_type_from_value(element) {
            Some(et) if et == DataType::Any => {
                // Element type is Any (deferred) — treat as compatible, skip
            }
            Some(et) if !Self::are_types_compatible_strict(et, first_type) => {
                self.add_error(
                    result,
                    "QFUNC077",
                    "ARRAY_HETEROGENEOUS",
                    &format!(
                        "Array element {} has type {:?} but array expects {:?} \
                         (from first element) in function '{}'",
                        i + 1, et, first_type, function_name
                    ),
                    &format!(
                        "All array elements must be the same type. \
                         Convert element to {:?} or use separate arrays.",
                        first_type
                    ),
                    position,
                );
            }
            None => {
                self.add_warning(
                    result,
                    "QFUNC_WARN008",
                    &format!(
                        "Cannot infer type of array element {} in function '{}'",
                        i + 1, function_name
                    ),
                    "QUICKFUNCS",
                    position,
                );
            }
            _ => {} // same type — OK
        }
    }

    if self.debug_config.is_verbose {
        self.error_manager.log_debug(&format!(
            "Array homogeneity validated: all {} elements are {:?}",
            values.len(), first_type
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

        let mut seen: FxHashSet<&str> =
            FxHashSet::with_capacity_and_hasher(properties.len(), Default::default());

        for prop in properties {
            if !seen.insert(prop.key.as_str()) {
                self.add_error(
                    result, "QFUNC060", "DUPLICATE_OBJECT_KEY",
                    &format!("Duplicate object key '{}' in function '{}'", prop.key, function_name),
                    &format!("Each key must be unique. Remove or rename duplicate key '{}'.", prop.key),
                    prop.position,
                );
            }
        }
    }

    // ==================== TYPE SYSTEM ====================

    // ── Type system (static methods on QuickFuncsSectionAnalyzer) ───────────────

#[inline]
fn are_types_compatible_strict(source: DataType, target: DataType) -> bool {
    // Identical types are always compatible
    if source == target { return true; }
    // Any is the universal wildcard
    if source == DataType::Any || target == DataType::Any { return true; }
    // Numeric promotion (all four numeric types are inter-compatible)
    if is_numeric_type(source) && is_numeric_type(target) { return true; }
    // Long ↔ Int widening
    if matches!((source, target),
        (DataType::Int, DataType::Long) | (DataType::Long, DataType::Int)
    ) { return true; }
    // Date ↔ Timestamp
    if matches!((source, target),
        (DataType::Date, DataType::Timestamp) | (DataType::Timestamp, DataType::Date)
    ) { return true; }

    // Typed ↔ untyped collection compatibility
    match (source, target) {
        // Array: untyped ↔ typed is always OK; typed ↔ typed needs matching elem type
        (DataType::Array, DataType::TypedArray(_))
        | (DataType::TypedArray(_), DataType::Array) => true,

        (DataType::TypedArray(s_elem), DataType::TypedArray(t_elem)) => {
            s_elem == t_elem
                || s_elem == ElemType::Any
                || t_elem == ElemType::Any
        }

        // Tuple: untyped ↔ typed is always OK; typed ↔ typed is loosely OK
        // (per-element structural checking deferred to runtime for now)
        (DataType::Tuple, DataType::TypedTuple(_))
        | (DataType::TypedTuple(_), DataType::Tuple) => true,

        (DataType::TypedTuple(_), DataType::TypedTuple(_)) => true,

        _ => false,
    }
}

#[inline]
fn are_types_comparable(a: DataType, b: DataType) -> bool {
    a == b
        || a == DataType::Any
        || b == DataType::Any
        || (is_numeric_type(a) && is_numeric_type(b))
        // Collections of the same class are comparable
        || (a.is_array() && b.is_array())
        || (a.is_tuple() && b.is_tuple())
        || matches!(
            (a, b),
            (DataType::Date, DataType::Timestamp)
                | (DataType::Timestamp, DataType::Date)
                | (DataType::Timestamp, DataType::Timestamp)
                | (DataType::Date, DataType::Date)
        )
}



    

    /// Maps DataType → DixType for instance-method registry lookups.
/// `Long` is included (was missing in previous version — bug fix).
/// TypedArray/TypedTuple map to their untyped DixType equivalents.
#[inline]
fn convert_data_type_to_dix_type(data_type: DataType) -> Option<DixType> {
    match data_type {
        DataType::Int               => Some(DixType::Int),
        DataType::Long              => Some(DixType::Long),   // ← was missing before
        DataType::Float             => Some(DixType::Float),
        DataType::Double            => Some(DixType::Double),
        DataType::String            => Some(DixType::String),
        DataType::Bool              => Some(DixType::Bool),
        DataType::Array             => Some(DixType::Array),
        DataType::Tuple             => Some(DixType::Tuple),
        DataType::Object            => Some(DixType::Object),
        DataType::Hex               => Some(DixType::Hex),
        DataType::Blob              => Some(DixType::Blob),
        DataType::Regex             => Some(DixType::Regex),
        DataType::Date              => Some(DixType::Date),
        DataType::Timestamp         => Some(DixType::Timestamp),
        DataType::Enum              => Some(DixType::Enum),
        DataType::TypedArray(_)     => Some(DixType::Array),
        DataType::TypedTuple(_)     => Some(DixType::Tuple),
        // Function / Range / Any have no direct DixType mapping
        _ => None,
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
        duplicate_functions: &FxHashSet<&str>,
        _result: &mut SectionAnalysisResult,
    ) {
        let mut registered = 0usize;

        for func in &section.functions {
            if duplicate_functions.contains(func.name.as_str()) {
                continue;
            }

            let parameters: Vec<ParameterInfo> = func
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

            let scopes = func.scope_list.clone().unwrap_or_default();

            let signature = FunctionSignature {
                name: func.name.clone(),
                return_type: func.return_type,
                parameters,
                scopes,
                line: func.position.line as i32,
                column: func.position.column as i32,
            };

            symbol_table.add_function(func.name.clone(), signature);
            registered += 1;

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "Pre-registered function '{}' in symbol table",
                    func.name
                ));
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Symbol table populated: {} functions registered, {} duplicates skipped",
                registered,
                duplicate_functions.len()
            ));
        }
    }

    // ==================== HELPERS ====================

    #[inline]
    fn is_valid_identifier(name: &str) -> bool {
        let mut chars = name.chars();
        match chars.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            }
            _ => false,
        }
    }

    #[inline]
    fn is_valid_data_path(path: &str) -> bool {
        !path.is_empty()
            && path.split('.').all(|seg| !seg.is_empty() && Self::is_valid_identifier(seg))
    }

    #[inline]
    fn should_halt(&self, result: &SectionAnalysisResult) -> bool {
        !result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

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
        result.errors.push(SemanticErrorInfo {
            error_id:    error_id.to_string(),
            error_type:  error_type.to_string(),
            message:     message.to_string(),
            section_name: "QUICKFUNCS".to_string(),
            suggestion:  suggestion.to_string(),
            position:    Some(position),
        });
        if self.debug_config.is_enabled {
            self.error_manager.log_error(&format!("[{}] {}: {}", error_id, error_type, message));
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
        result.warnings.push(SemanticWarningInfo {
            warning_id:   warning_id.to_string(),
            message:      message.to_string(),
            section_name: section_name.to_string(),
            position:     Some(position),
        });
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!("[{}] {}", warning_id, message));
        }
    }
}

// ==================== LOCAL SCOPE TRACKER ====================

struct LocalScopeTracker {
    variables:  FxHashMap<String, VariableScopeInfo>,
    parameters: FxHashSet<String>,
}

impl LocalScopeTracker {
    fn with_capacity(capacity: usize) -> Self {
        LocalScopeTracker {
            variables:  FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
            parameters: FxHashSet::with_capacity_and_hasher(capacity, Default::default()),
        }
    }

    /// Reset scope and pre-populate from function parameters.
/// TypedArray/TypedTuple annotations are used to populate element_type
/// so downstream method calls like .first() / .last() resolve correctly
/// even for parameters declared as <array<int>>.
fn reset_with_params(&mut self, func_parameters: &[QuickFuncParam]) {
    self.variables.clear();
    self.parameters.clear();

    for param in func_parameters {
        self.parameters.insert(param.name.clone());

        // Extract element type from typed-collection annotations so that
        // element-returning methods (.first(), .last(), .get(), etc.) on
        // typed parameters can resolve to the actual element type rather
        // than returning Any.
        let element_type: Option<DataType> = match param.data_type {
            Some(DataType::TypedArray(elem)) => Some(elem.to_data_type()),
            Some(DataType::TypedTuple(arr))  => arr[0].map(|e| e.to_data_type()),
            _ => None,
        };

        self.variables.insert(
            param.name.clone(),
            VariableScopeInfo {
                var_type:     param.data_type,
                is_const:     true,
                is_parameter: true,
                element_type,
            },
        );
    }
}

    // ── Variable registration ─────────────────────────────────────────────────

    /// Add a variable without an element type (scalars, objects, etc.).
    #[inline]
    fn add_variable(&mut self, name: String, var_type: Option<DataType>, is_const: bool) {
        self.variables.insert(
            name,
            VariableScopeInfo { var_type, is_const, is_parameter: false, element_type: None },
        );
    }

    /// Add an array or tuple variable and record its element type.
    #[inline]
    fn add_variable_with_element_type(
        &mut self,
        name:         String,
        var_type:     Option<DataType>,
        is_const:     bool,
        element_type: Option<DataType>,
    ) {
        self.variables.insert(
            name,
            VariableScopeInfo { var_type, is_const, is_parameter: false, element_type },
        );
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    #[inline]
    fn has_variable(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    #[inline]
    fn has_parameter(&self, name: &str) -> bool {
        self.parameters.contains(name)
    }

    #[inline]
    fn is_const(&self, name: &str) -> bool {
        self.variables.get(name).map_or(false, |v| v.is_const)
    }

    #[inline]
    fn get_variable_type(&self, name: &str) -> Option<DataType> {
        self.variables.get(name).and_then(|v| v.var_type)
    }

    /// Returns the element type of an array/tuple variable, or None.
    #[inline]
    fn get_element_type(&self, name: &str) -> Option<DataType> {
        self.variables.get(name).and_then(|v| v.element_type)
    }

    // ── Updates ───────────────────────────────────────────────────────────────

    /// Update the inferred type of a variable (used during assignment).
    fn update_variable_type(&mut self, name: &str, var_type: DataType) {
        if let Some(info) = self.variables.get_mut(name) {
            if info.var_type.is_none() {
                info.var_type = Some(var_type);
            }
        }
    }

    /// Update the element type of an array/tuple variable (used during assignment).
    fn update_variable_element_type(&mut self, name: &str, element_type: Option<DataType>) {
        if let Some(info) = self.variables.get_mut(name) {
            info.element_type = element_type;
        }
    }

    // ── Bulk accessors for TypeInferenceVisitor construction ─────────────────

    /// All variable types: name → Option<DataType>.  Includes parameters.
    fn get_all_variable_types(&self) -> HashMap<String, Option<DataType>> {
        self.variables
            .iter()
            .map(|(k, v)| (k.clone(), v.var_type))
            .collect()
    }

    /// Element type hints for array/tuple variables: name → DataType.
    /// Passed to `TypeInferenceVisitor::new_with_element_hints` so element-
    /// returning methods (`.first()`, `.last()`, etc.) resolve to the actual type.
    fn get_all_element_type_hints(&self) -> HashMap<String, DataType> {
        self.variables
            .iter()
            .filter_map(|(k, v)| v.element_type.map(|et| (k.clone(), et)))
            .collect()
    }

    /// Iterator over non-parameter variable names (for unused-variable detection).
    fn get_declared_variable_names(&self) -> impl Iterator<Item = &String> {
        self.variables
            .iter()
            .filter(|(_, v)| !v.is_parameter)
            .map(|(k, _)| k)
    }
}

struct VariableScopeInfo {
    var_type:     Option<DataType>,
    is_const:     bool,
    is_parameter: bool,
    /// For arrays and tuples: the element type if the collection is uniform.
    /// None for non-collection types or heterogeneous collections.
    element_type: Option<DataType>,
        }
// ==================== RETURN PATH ANALYZER ====================

struct ReturnPathAnalyzer {
    _expected_return_type: DataType,
    has_unconditional_return: bool,
}

impl ReturnPathAnalyzer {
    #[inline]
    fn new(expected_return_type: DataType) -> Self {
        ReturnPathAnalyzer {
            _expected_return_type: expected_return_type,
            has_unconditional_return: false,
        }
    }

    #[inline]
    fn add_return(&mut self) {
        self.has_unconditional_return = true;
    }

    #[inline]
    fn all_paths_return(&self) -> bool {
        self.has_unconditional_return
    }
}

// ==================== VARIABLE REFERENCE COLLECTOR ====================

struct VariableReferenceCollector {
    referenced: FxHashSet<String>,
    parameters: FxHashSet<String>,
}

impl VariableReferenceCollector {
    fn new(func_parameters: &[QuickFuncParam]) -> Self {
        let parameters: FxHashSet<String> =
            func_parameters.iter().map(|p| p.name.clone()).collect();
        VariableReferenceCollector {
            referenced: FxHashSet::default(),
            parameters,
        }
    }

    fn collect_from_function(&mut self, func: &QuickFunction) -> &FxHashSet<String> {
        for stmt in &func.body {
            self.collect_from_statement(stmt);
        }
        &self.referenced
    }

    fn collect_from_statement(&mut self, stmt: &QuickFuncStatement) {
        match stmt {
            QuickFuncStatement::Return { value, .. } => self.collect_expr(value),
            QuickFuncStatement::Assignment { value, .. } => self.collect_expr(value),
            QuickFuncStatement::ArithmeticAssignment { variable, value, .. } => {
                self.add_ref(variable);
                self.collect_expr(value);
            }
            QuickFuncStatement::VariableDeclaration { value, .. } => self.collect_expr(value),
            QuickFuncStatement::ObjectCreation { object, .. } => self.collect_value(object),
            QuickFuncStatement::If { condition, then_branch, else_branch, .. } => {
                self.collect_expr(condition);
                for s in then_branch { self.collect_from_statement(s); }
                if let Some(els) = else_branch {
                    for s in els { self.collect_from_statement(s); }
                }
            }
            QuickFuncStatement::Switch { expression, cases, default_case, .. } => {
                self.collect_expr(expression);
                for case in cases {
                    for s in &case.statements { self.collect_from_statement(s); }
                }
                if let Some(def) = default_case {
                    for s in &def.statements { self.collect_from_statement(s); }
                }
            }
            QuickFuncStatement::Log { value, .. } => self.collect_expr(value),
            QuickFuncStatement::ExpressionStatement { expression, .. } => self.collect_expr(expression),
        }
    }

    fn collect_expr(&mut self, expr: &Expression) {
        match expr {
            Expression::Identifier { name, .. } => self.add_ref(name),
            Expression::QualifiedIdentifier { parts, arguments, .. } => {
                if let Some(first) = parts.first() { self.add_ref(first); }
                if let Some(args) = arguments {
                    for a in args { self.collect_expr(a); }
                }
            }
            Expression::ArithmeticOp { left, right, .. }
            | Expression::ComparisonOp { left, right, .. }
            | Expression::LogicalOp { left, right, .. }
            | Expression::BitwiseOp { left, right, .. } => {
                self.collect_expr(left);
                self.collect_expr(right);
            }
            Expression::UnaryOp { operand, .. } => self.collect_expr(operand),
            Expression::Conditional { condition, true_value, false_value, .. } => {
                self.collect_expr(condition);
                self.collect_expr(true_value);
                self.collect_expr(false_value);
            }
            Expression::Parenthesized { expression, .. } => self.collect_expr(expression),
            Expression::PropertyAccess { object, .. } => self.collect_expr(object),
            Expression::IndexAccess { object, index, .. } => {
                self.collect_expr(object);
                self.collect_expr(index);
            }
            Expression::QuickFuncCall { arguments, .. }
            | Expression::ImportedFunctionCall { arguments, .. }
            | Expression::StaticMethodCall { arguments, .. } => {
                for a in arguments { self.collect_expr(a); }
            }
            Expression::InstanceMethodCall { instance, arguments, .. } => {
                self.collect_expr(instance);
                for a in arguments { self.collect_expr(a); }
            }
            Expression::Value { value, .. } => self.collect_value(value),
            Expression::TypeCast { expression, .. } => self.collect_expr(expression),
            _ => {}
        }
    }

    fn collect_value(&mut self, value: &Value) {
        match value {
            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                for v in values { self.collect_value(v); }
            }
            Value::Object { properties, .. } => {
                for p in properties { self.collect_value(&p.value); }
            }
            Value::PrefixedConstructor { arguments, .. } => {
                for a in arguments { self.collect_value(a); }
            }
            Value::InterpolatedString { expressions, .. } => {
                for e in expressions { self.collect_expr(e); }
            }
            Value::QuickFuncCall { arguments, .. } => {
                for a in arguments { self.collect_expr(a); }
            }
            Value::Expression { expr, .. } => self.collect_expr(expr),
            Value::Lambda { body, .. } => self.collect_expr(body),
            Value::Range { start, end, .. } => {
                self.collect_value(start);
                self.collect_value(end);
            }
            Value::Identifier { value, .. } => self.add_ref(value),
            _ => {}
        }
    }

    #[inline]
    fn add_ref(&mut self, name: &str) {
        if !self.parameters.contains(name) {
            self.referenced.insert(name.to_string());
        }
    }
}
