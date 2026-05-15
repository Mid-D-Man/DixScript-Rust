
//! Executes QuickFunction bodies at compile time.
//!
//! Parameter threading eliminates mutable field swapping. DebugConfig is
//! cached on construction — format!() is never evaluated when debug is off.
//! All errors are typed InterpreterError variants. Recursion depth has both
//! absolute and dynamic limits.

use std::cell::RefCell;
use std::net::ToSocketAddrs;
use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::Builtins::Core::{DixType, DixValue};
use crate::Builtins::Resolver::builtin_call_resolver;
use crate::Compiler::AST::{
    Expression, ObjectProperty, Position, QuickFunction, QuickFuncParam,
    QuickFuncStatement, SwitchCase, Value,
};
use crate::Compiler::Core::DebugMode;
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Utilities::symbol_table::ImportedNamespace;
use crate::ErrorManager::{DebugConfig, ErrorManager};

use super::execution_context::ExecutionContext;
use super::supporting_classes::ExecutionError;

const ABSOLUTE_MAX_RECURSION: u32 = 10_000;
const BASE_RECURSION_DEPTH: u32 = 1_000;

fn calculate_recursion_limit(param_count: usize, body_size: usize) -> u32 {
    let complexity_factor = (param_count * 10 + body_size / 5) as u32;
    (BASE_RECURSION_DEPTH + complexity_factor).min(ABSOLUTE_MAX_RECURSION)
}

#[derive(Debug, Clone)]
pub enum InterpreterError {
    RecursionLimitExceeded {
        function_name: String,
        position: Position,
        depth: u32,
        limit: u32,
    },
    UndefinedVariable {
        name: String,
        function_name: String,
        position: Position,
        checked_scopes: String,
    },
    UndefinedFunction {
        name: String,
        position: Position,
    },
    NamespaceNotFound {
        name: String,
        position: Position,
    },
    FunctionNotInNamespace {
        namespace: String,
        function: String,
        position: Position,
    },
    ParameterCountMismatch {
        expected: usize,
        got: usize,
        required: usize,
        position: Position,
    },
    ParameterEvalFailed {
        index: usize,
        param_name: String,
        inner: Box<InterpreterError>,
        position: Position,
    },
    DivisionByZero {
        position: Position,
    },
    PropertyNotFound {
        property: String,
        position: Position,
    },
    IndexOutOfBounds {
        index: i64,
        length: usize,
        position: Position,
    },
    InvalidOperation {
        message: String,
        position: Position,
    },
    UnsupportedStatement {
        variant: String,
        position: Position,
    },
    UnsupportedExpression {
        variant: String,
        position: Position,
    },
    BuiltinCallFailed {
        object: String,
        method: String,
        message: String,
        position: Position,
    },
    InvalidEnumAccess {
        location: String,
        position: Position,
    },
    LambdaParamMismatch {
        expected: usize,
        got: usize,
        position: Position,
    },
    ConfigKeyNotFound {
        key: String,
        position: Position,
    },
    ScopeMappingMismatch {
        variable: String,
        mapped_path: String,
        position: Position,
    },
}

impl From<String> for InterpreterError {
    fn from(message: String) -> Self {
        InterpreterError::InvalidOperation {
            message,
            position: Position::UNKNOWN,
        }
    }
}

impl std::fmt::Display for InterpreterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterpreterError::RecursionLimitExceeded {
                function_name,
                position,
                depth,
                limit,
            } => write!(
                f,
                "Recursion limit exceeded in '{}' at {} (depth: {}, limit: {})",
                function_name, position, depth, limit
            ),
            InterpreterError::UndefinedVariable {
                name,
                function_name,
                position,
                checked_scopes,
            } => write!(
                f,
                "Undefined variable '{}' in function '{}' at {} (checked: {})",
                name, function_name, position, checked_scopes
            ),
            InterpreterError::UndefinedFunction { name, position } => {
                write!(f, "Undefined function '{}' at {}", name, position)
            }
            InterpreterError::ParameterCountMismatch {
                expected,
                got,
                required,
                position,
            } => write!(
                f,
                "Parameter count mismatch at {}: expected {}, got {} (required: {})",
                position, expected, got, required
            ),
            InterpreterError::DivisionByZero { position } => {
                write!(f, "Division by zero at {}", position)
            }
            InterpreterError::InvalidOperation { message, position } => {
                write!(f, "Invalid operation at {}: {}", position, message)
            }
            _ => write!(f, "{:?}", self),
        }
    }
}

impl std::error::Error for InterpreterError {}

#[derive(Debug, Clone)]
pub struct LambdaAst {
    pub params: Vec<String>,
    pub body: Expression,
}

pub struct FunctionInterpreter<'a> {
    symbol_table: &'a SymbolTable,
    quick_functions: Vec<QuickFunction>,
    data_context: Rc<RefCell<FxHashMap<String, DixValue>>>,
    debug_config: DebugConfig,
    recursion_depth: u32,
    current_recursion_limit: u32,
    lambda_registry: FxHashMap<String, LambdaAst>,
    log_statements: Vec<String>,
    error_manager: ErrorManager,
}

impl<'a> FunctionInterpreter<'a> {
    pub fn new(
        symbol_table: &'a SymbolTable,
        quick_functions: Vec<QuickFunction>,
        data_context: Rc<RefCell<FxHashMap<String, DixValue>>>,
        debug_mode: DebugMode,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
       Self::new_with_error_manager(symbol_table,quick_functions,data_context,debug_mode,error_manager)
    }
    pub fn new_with_error_manager(
        symbol_table: &'a SymbolTable,
        quick_functions: Vec<QuickFunction>,
        data_context: Rc<RefCell<FxHashMap<String, DixValue>>>,
        debug_mode: DebugMode,
        error_manager:ErrorManager
    ) -> Self {

        let func_count = quick_functions.len();

        FunctionInterpreter {
            symbol_table,
            quick_functions,
            data_context,
            debug_config: DebugConfig::from_debug_mode(debug_mode),
            recursion_depth: 0,
            current_recursion_limit: BASE_RECURSION_DEPTH,
            lambda_registry: FxHashMap::with_capacity_and_hasher(
                func_count.max(4),
                Default::default(),
            ),
            log_statements: Vec::with_capacity(4),
            error_manager,
        }
    }
    pub fn update_data_context(&mut self, key: String, value: DixValue) {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Updated data context: {} = {}",
                key,
                value.get_type().get_type_name()
            ));
        }
        self.data_context.borrow_mut().insert(key, value);
    }

    pub fn find_function(&self, name: &str) -> Option<&QuickFunction> {
        self.quick_functions.iter().find(|f| f.name == name)
    }

    pub fn take_logs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log_statements)
    }

    pub fn execute(
        &mut self,
        function: &QuickFunction,
        arguments: &[Expression],
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let param_count = function.parameters.len();
        let body_size = function.body.len();
        self.current_recursion_limit = calculate_recursion_limit(param_count, body_size);

        self.recursion_depth += 1;

        if self.recursion_depth > self.current_recursion_limit {
            let err = InterpreterError::RecursionLimitExceeded {
                function_name: function.name.clone(),
                position: function.position,
                depth: self.recursion_depth,
                limit: self.current_recursion_limit,
            };
            self.recursion_depth -= 1;
            self.error_manager.add_value_resolution_error(
                crate::ErrorManager::ValueResolutionErrorType::InvalidOperation,
                format!("Recursion limit exceeded in '{}'", function.name),
                function.position.line as i32,
                function.position.column as i32,
                Some("@QUICKFUNCS".to_string()),
                None,
                Some(function.name.clone()),
                Some("Check for infinite recursion".to_string()),
            );
            return Err(err);
        }

        self.bind_parameters(
            &function.parameters,
            arguments,
            context,
            scope_context,
            namespace,
        )?;

        let mut last_result = DixValue::null();

        for (i, statement) in function.body.iter().enumerate() {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[Execute] Statement {}/{}: {}",
                    i + 1,
                    function.body.len(),
                    statement_variant_name(statement)
                ));
            }

            last_result =
                self.execute_statement(statement, context, scope_context, namespace)?;

            if matches!(statement, QuickFuncStatement::Return { .. }) {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug(&format!(
                        "[Execute] Explicit return: {}",
                        last_result.get_type().get_type_name()
                    ));
                }
                self.recursion_depth -= 1;
                return Ok(last_result);
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[Execute] Implicit return: {}",
                last_result.get_type().get_type_name()
            ));
        }

        self.recursion_depth -= 1;
        Ok(last_result)
    }

    fn bind_parameters(
        &mut self,
        parameters: &[QuickFuncParam],
        arguments: &[Expression],
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<(), InterpreterError> {
        let required_count = parameters
            .iter()
            .filter(|p| p.default_value.is_none())
            .count();

        if arguments.len() < required_count {
            let position = parameters
                .first()
                .map(|p| p.position)
                .unwrap_or(Position::UNKNOWN);
            return Err(InterpreterError::ParameterCountMismatch {
                expected: parameters.len(),
                got: arguments.len(),
                required: required_count,
                position,
            });
        }

        for (i, param) in parameters.iter().enumerate() {
            let value = if i < arguments.len() {
                self.evaluate_expression(
                    &arguments[i],
                    context,
                    scope_context,
                    namespace,
                )
                .map_err(|e| InterpreterError::ParameterEvalFailed {
                    index: i,
                    param_name: param.name.clone(),
                    inner: Box::new(e),
                    position: arguments[i].position(),
                })?
            } else if let Some(ref default) = param.default_value {
                self.evaluate_expression(default, context, scope_context, namespace)
                    .map_err(|e| InterpreterError::ParameterEvalFailed {
                        index: i,
                        param_name: param.name.clone(),
                        inner: Box::new(e),
                        position: param.position,
                    })?
            } else {
                return Err(InterpreterError::ParameterCountMismatch {
                    expected: parameters.len(),
                    got: arguments.len(),
                    required: required_count,
                    position: param.position,
                });
            };

            context
                .define_variable(&param.name, value)
                .map_err(|e| InterpreterError::InvalidOperation {
                    message: e.to_string(),
                    position: param.position,
                })?;
        }

        Ok(())
    }

    fn resolve_identifier(
        &self,
        name: &str,
        position: Position,
        context: &ExecutionContext,
        scope_context: &FxHashMap<String, String>,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_enabled {
            self.error_manager
                .log_debug(&format!("[ResolveId] Resolving: {}", name));
        }

        if let Ok(value) = context.get_variable(name) {
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "  Found in execution context: {}",
                    value.get_type().get_type_name()
                ));
            }
            return Ok(value);
        }

        if let Some(full_path) = scope_context.get(name) {
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "  Checking scope context: {} -> {}",
                    name, full_path
                ));
            }
            if let Some(value) = self.data_context.borrow().get(full_path) {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "  Found in scope context: {}",
                        value.get_type().get_type_name()
                    ));
                }
                return Ok(value.clone());
            } else {
                return Err(InterpreterError::ScopeMappingMismatch {
                    variable: name.to_string(),
                    mapped_path: full_path.clone(),
                    position,
                });
            }
        }

        if let Some(value) = self.try_resolve_by_path_suffix(name) {
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "  Found by path search: {}",
                    value.get_type().get_type_name()
                ));
            }
            return Ok(value);
        }

        if let Some(value) = self.data_context.borrow().get(name) {
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "  Found in global context: {}",
                    value.get_type().get_type_name()
                ));
            }
            return Ok(value.clone());
        }

        let scope_keys = scope_context
            .keys()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        Err(InterpreterError::UndefinedVariable {
            name: name.to_string(),
            function_name: context.function_name().to_string(),
            position,
            checked_scopes: scope_keys,
        })
    }

    fn try_resolve_by_path_suffix(&self, name: &str) -> Option<DixValue> {
        let suffix = format!(".{}", name);
        let data_ctx = self.data_context.borrow();
        for (key, value) in data_ctx.iter() {
            if key.to_string() == name.to_string() || key.ends_with(&suffix) {
                return Some(value.clone());
            }
        }
        None
    }

    fn execute_statement(
        &mut self,
        statement: &QuickFuncStatement,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "[ExecuteStatement] {}",
                statement_variant_name(statement)
            ));
        }

        match statement {
            QuickFuncStatement::Return { value, .. } => {
                self.execute_return(value, context, scope_context, namespace)
            }
            QuickFuncStatement::Assignment { variable, value, position } => {
                self.execute_assignment(
                    variable, value, *position, context, scope_context, namespace,
                )
            }
            QuickFuncStatement::ArithmeticAssignment {
                variable,
                operator,
                value,
                position,
            } => self.execute_arithmetic_assignment(
                variable, operator, value, *position, context, scope_context, namespace,
            ),
            QuickFuncStatement::If {
                condition,
                then_branch,
                else_branch,
                position,
            } => self.execute_if(
                condition,
                then_branch,
                else_branch.as_ref(),
                *position,
                context,
                scope_context,
                namespace,
            ),
            QuickFuncStatement::Switch {
                expression,
                cases,
                default_case,
                position,
            } => self.execute_switch(
                expression,
                cases,
                default_case.as_ref(),
                *position,
                context,
                scope_context,
                namespace,
            ),
            QuickFuncStatement::Log { value, position } => {
                self.execute_log(value, *position, context, scope_context, namespace)
            }
            QuickFuncStatement::VariableDeclaration {
                variable_name,
                value,
                position,
                ..
            } => self.execute_variable_declaration(
                variable_name, value, *position, context, scope_context, namespace,
            ),
            QuickFuncStatement::ExpressionStatement { expression, .. } => {
                self.evaluate_expression(expression, context, scope_context, namespace)
            }
            QuickFuncStatement::ObjectCreation {
                variable,
                object,
                position,
            } => self.execute_object_creation(
                variable, object, *position, context, scope_context, namespace,
            ),
        }
    }

    fn execute_return(
        &mut self,
        value: &Expression,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_enabled {
            self.error_manager
                .log_debug("[ExecuteReturn] Processing return statement");
        }

        let return_value =
            self.evaluate_expression(value, context, scope_context, namespace)?;

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[ExecuteReturn] Evaluated to: {} = {}",
                return_value.get_type().get_type_name(),
                return_value
            ));
        }

        Ok(return_value)
    }

    fn execute_assignment(
        &mut self,
        variable: &str,
        value: &Expression,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let val =
            self.evaluate_expression(value, context, scope_context, namespace)?;

        if let Expression::Value {
            value: Value::Lambda { parameters, body, .. },
            ..
        } = value
        {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[Lambda] Registered lambda for variable: {}",
                    variable
                ));
            }
            self.lambda_registry.insert(
                variable.to_string(),
                LambdaAst {
                    params: parameters.clone(),
                    body: *body.clone(),
                },
            );
        }

        if context.has_variable(variable) {
            context
                .set_variable(variable, val.clone())
                .map_err(|e| InterpreterError::InvalidOperation {
                    message: e.to_string(),
                    position,
                })?;
        } else {
            context
                .define_variable(variable, val.clone())
                .map_err(|e| InterpreterError::InvalidOperation {
                    message: e.to_string(),
                    position,
                })?;
        }

        Ok(val)
    }

    fn execute_variable_declaration(
        &mut self,
        variable_name: &str,
        value: &Expression,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[ExecuteVariableDeclaration] {}",
                variable_name
            ));
        }

        let val =
            self.evaluate_expression(value, context, scope_context, namespace)?;

        if let Expression::Value {
            value: Value::Lambda { parameters, body, .. },
            ..
        } = value
        {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[Lambda] Registered lambda for variable: {}",
                    variable_name
                ));
            }
            self.lambda_registry.insert(
                variable_name.to_string(),
                LambdaAst {
                    params: parameters.clone(),
                    body: *body.clone(),
                },
            );
        }

        context
            .define_variable(variable_name, val.clone())
            .map_err(|e| InterpreterError::InvalidOperation {
                message: e.to_string(),
                position,
            })?;

        Ok(val)
    }

    fn execute_arithmetic_assignment(
        &mut self,
        variable: &str,
        operator: &str,
        value: &Expression,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let current_value =
            context.get_variable(variable).map_err(|_| {
                InterpreterError::UndefinedVariable {
                    name: variable.to_string(),
                    function_name: context.function_name().to_string(),
                    position,
                    checked_scopes: "execution context".to_string(),
                }
            })?;

        let operand_value =
            self.evaluate_expression(value, context, scope_context, namespace)?;

        let use_long = current_value.get_type() == DixType::Long
            || operand_value.get_type() == DixType::Long;

        let result = match operator {
            "+=" => current_value.add(&operand_value).map_err(|e| {
                InterpreterError::InvalidOperation { message: e, position }
            })?,
            "-=" => current_value
                .subtract(&operand_value)
                .map_err(|e| InterpreterError::InvalidOperation { message: e, position })?,
            "*=" => current_value
                .multiply(&operand_value)
                .map_err(|e| InterpreterError::InvalidOperation { message: e, position })?,
            "/=" => current_value.divide(&operand_value).map_err(|e| {
                if e.contains("zero") {
                    InterpreterError::DivisionByZero { position }
                } else {
                    InterpreterError::InvalidOperation { message: e, position }
                }
            })?,
            // Modulo: preserve Long when both operands are integer types
            "%=" => {
                if use_long && current_value.get_type() != DixType::Float
                    && current_value.get_type() != DixType::Double
                    && operand_value.get_type() != DixType::Float
                    && operand_value.get_type() != DixType::Double
                {
                    let rv = operand_value.as_long();
                    if rv == 0 {
                        return Err(InterpreterError::DivisionByZero { position });
                    }
                    DixValue::from_long(current_value.as_long() % rv)
                } else {
                    DixValue::from_double(
                        current_value.as_double() % operand_value.as_double(),
                    )
                }
            }
            "**=" => DixValue::from_double(
                current_value
                    .as_double()
                    .powf(operand_value.as_double()),
            ),
            // Bitwise assign ops — use Long path if either operand is Long
            "&=" => {
                if use_long {
                    DixValue::from_long(current_value.as_long() & operand_value.as_long())
                } else {
                    DixValue::from_int(current_value.as_int() & operand_value.as_int())
                }
            }
            "|=" => {
                if use_long {
                    DixValue::from_long(current_value.as_long() | operand_value.as_long())
                } else {
                    DixValue::from_int(current_value.as_int() | operand_value.as_int())
                }
            }
            "^=" => {
                if use_long {
                    DixValue::from_long(current_value.as_long() ^ operand_value.as_long())
                } else {
                    DixValue::from_int(current_value.as_int() ^ operand_value.as_int())
                }
            }
            "<<=" => {
                if use_long {
                    DixValue::from_long(current_value.as_long() << operand_value.as_long())
                } else {
                    DixValue::from_int(current_value.as_int() << operand_value.as_int())
                }
            }
            ">>=" => {
                if use_long {
                    DixValue::from_long(current_value.as_long() >> operand_value.as_long())
                } else {
                    DixValue::from_int(current_value.as_int() >> operand_value.as_int())
                }
            }
            _ => {
                return Err(InterpreterError::UnsupportedStatement {
                    variant: format!("Arithmetic assignment operator: {}", operator),
                    position,
                })
            }
        };

        context
            .set_variable(variable, result.clone())
            .map_err(|e| InterpreterError::InvalidOperation {
                message: e.to_string(),
                position,
            })?;

        Ok(result)
    }

    fn execute_object_creation(
        &mut self,
        variable: &str,
        object: &Value,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let dix_obj = self.convert_ast_value_to_dix_value(
            object,
            context,
            scope_context,
            namespace,
        )?;

        context
            .define_variable(variable, dix_obj.clone())
            .map_err(|e| InterpreterError::InvalidOperation {
                message: e.to_string(),
                position,
            })?;

        Ok(dix_obj)
    }

    fn execute_if(
        &mut self,
        condition: &Expression,
        then_branch: &[QuickFuncStatement],
        else_branch: Option<&Vec<QuickFuncStatement>>,
        _position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_enabled {
            self.error_manager
                .log_debug("[ExecuteIf] Evaluating condition");
        }

        let cond_value =
            self.evaluate_expression(condition, context, scope_context, namespace)?;

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "[ExecuteIf] Condition: {}",
                cond_value.as_bool()
            ));
        }

        if cond_value.as_bool() {
            if self.debug_config.is_enabled {
                self.error_manager
                    .log_debug("[ExecuteIf] Taking THEN branch");
            }
            let mut last_result = DixValue::null();
            for stmt in then_branch {
                last_result = self.execute_statement(
                    stmt, context, scope_context, namespace,
                )?;
                if matches!(stmt, QuickFuncStatement::Return { .. }) {
                    if self.debug_config.is_enabled {
                        self.error_manager.log_debug(&format!(
                            "[ExecuteIf] Explicit return from THEN: {}",
                            last_result
                        ));
                    }
                    return Ok(last_result);
                }
            }
            Ok(last_result)
        } else if let Some(else_stmts) = else_branch {
            if self.debug_config.is_enabled {
                self.error_manager
                    .log_debug("[ExecuteIf] Taking ELSE branch");
            }
            let mut last_result = DixValue::null();
            for stmt in else_stmts {
                last_result = self.execute_statement(
                    stmt, context, scope_context, namespace,
                )?;
                if matches!(stmt, QuickFuncStatement::Return { .. }) {
                    if self.debug_config.is_enabled {
                        self.error_manager.log_debug(&format!(
                            "[ExecuteIf] Explicit return from ELSE: {}",
                            last_result
                        ));
                    }
                    return Ok(last_result);
                }
            }
            Ok(last_result)
        } else {
            if self.debug_config.is_enabled {
                self.error_manager
                    .log_debug("[ExecuteIf] No branch taken");
            }
            Ok(DixValue::null())
        }
    }

    fn execute_switch(
        &mut self,
        expression: &Expression,
        cases: &[SwitchCase],
        default_case: Option<&SwitchCase>,
        _position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_enabled {
            self.error_manager
                .log_debug("[ExecuteSwitch] Evaluating switch expression");
        }

        let switch_value =
            self.evaluate_expression(expression, context, scope_context, namespace)?;

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "[ExecuteSwitch] Switch value: {} = {}",
                switch_value.get_type().get_type_name(),
                switch_value
            ));
        }

        for (i, case) in cases.iter().enumerate() {
            let case_value = self.convert_ast_value_to_dix_value(
                &case.case_value,
                context,
                scope_context,
                namespace,
            )?;

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "[ExecuteSwitch] Comparing with case [{}]: {}",
                    i, case_value
                ));
            }

            if switch_value.equal_to(&case_value) {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug(&format!(
                        "[ExecuteSwitch] Match found at case [{}]",
                        i
                    ));
                }
                let mut last_result = DixValue::null();
                for stmt in &case.statements {
                    last_result = self.execute_statement(
                        stmt, context, scope_context, namespace,
                    )?;
                    if matches!(stmt, QuickFuncStatement::Return { .. }) {
                        return Ok(last_result);
                    }
                }
                return Ok(last_result);
            }
        }

        if let Some(default) = default_case {
            if self.debug_config.is_enabled {
                self.error_manager
                    .log_debug("[ExecuteSwitch] No match, executing default");
            }
            let mut last_result = DixValue::null();
            for stmt in &default.statements {
                last_result = self.execute_statement(
                    stmt, context, scope_context, namespace,
                )?;
                if matches!(stmt, QuickFuncStatement::Return { .. }) {
                    return Ok(last_result);
                }
            }
            return Ok(last_result);
        }

        if self.debug_config.is_enabled {
            self.error_manager
                .log_debug("[ExecuteSwitch] No match and no default");
        }

        Ok(DixValue::null())
    }

    fn execute_log(
        &mut self,
        value: &Expression,
        _position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let val =
            self.evaluate_expression(value, context, scope_context, namespace)?;
        let message = val.as_string();

        self.log_statements.push(message.clone());

        if self.debug_config.is_enabled {
            self.error_manager
                .log_debug(&format!("[log:] {}", message));
        }

        Ok(DixValue::null())
    }

fn evaluate_expression(
        &mut self,
        expr: &Expression,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        match expr {
            Expression::Identifier { name, position } => {
                self.resolve_identifier(name, *position, context, scope_context)
            }

            Expression::Value { value, .. } => {
                self.convert_ast_value_to_dix_value(value, context, scope_context, namespace)
            }

            Expression::ArithmeticOp { left, operator, right, position } => {
                self.evaluate_arithmetic_op(
                    left, operator, right, *position, context, scope_context, namespace,
                )
            }

            Expression::BitwiseOp { left, operator, right, position } => {
                self.evaluate_bitwise_op(
                    left, operator, right, *position, context, scope_context, namespace,
                )
            }

            Expression::ComparisonOp { left, operator, right, position } => {
                self.evaluate_comparison_op(
                    left, operator, right, *position, context, scope_context, namespace,
                )
            }

            Expression::LogicalOp { left, operator, right, position } => {
                self.evaluate_logical_op(
                    left, operator, right, *position, context, scope_context, namespace,
                )
            }

            Expression::UnaryOp { operator, operand, position } => {
                self.evaluate_unary_op(
                    operator, operand, *position, context, scope_context, namespace,
                )
            }

            Expression::Conditional { condition, true_value, false_value, position } => {
                self.evaluate_conditional(
                    condition, true_value, false_value, *position, context, scope_context, namespace,
                )
            }

            Expression::StaticMethodCall { object_name, method_name, arguments, position } => {
                self.evaluate_static_method_call(
                    object_name, method_name, arguments, *position, context, scope_context, namespace,
                )
            }

            Expression::InstanceMethodCall { instance, method_name, arguments, position } => {
                self.evaluate_instance_method_call(
                    instance, method_name, arguments, *position, context, scope_context, namespace,
                )
            }

            Expression::PropertyAccess { object, property, position } => {
                self.evaluate_property_access(
                    object, property, *position, context, scope_context, namespace,
                )
            }

            Expression::IndexAccess { object, index, position } => {
                self.evaluate_index_access(
                    object, index, *position, context, scope_context, namespace,
                )
            }

            Expression::EnumAccess { namespace_name, enum_name, value, position } => {
                self.evaluate_enum_access(
                    namespace_name.as_deref(), enum_name, value, *position, namespace,
                )
            }

            Expression::QuickFuncCall { name, arguments, position } => {
                self.evaluate_quick_func_call(
                    name, arguments, *position, context, scope_context, namespace,
                )
            }

            Expression::ImportedFunctionCall {
                namespace_name,
                function_name,
                arguments,
                position,
            } => self.evaluate_imported_function_call(
                namespace_name, function_name, arguments, *position, context, scope_context,
            ),

            Expression::ConfigAccess { key, position } => {
                self.evaluate_config_access(key, *position)
            }

            Expression::Parenthesized { expression, .. } => {
                self.evaluate_expression(expression, context, scope_context, namespace)
            }

            other => Err(InterpreterError::UnsupportedExpression {
                variant: expr_variant_name(other).to_string(),
                position: other.position(),
            }),
        }
    }

    fn evaluate_arithmetic_op(
        &mut self,
        left: &Expression,
        operator: &str,
        right: &Expression,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        if matches!(operator, "<<" | ">>" | "&" | "|" | "^") {
            return self.evaluate_bitwise_op(
                left, operator, right, position, context, scope_context, namespace,
            );
        }

        match operator {
            "%%" => {
                return self.evaluate_circular_modulo(
                    left, right, position, context, scope_context, namespace,
                )
            }
            "%&" => {
                return self.evaluate_percentage(
                    left, right, position, context, scope_context, namespace,
                )
            }
            "&%" => {
                return self.evaluate_bitwise_modulo(
                    left, right, position, context, scope_context, namespace,
                )
            }
            _ => {}
        }

        let left_val  = self.evaluate_expression(left,  context, scope_context, namespace)?;
        let right_val = self.evaluate_expression(right, context, scope_context, namespace)?;

        let left_type  = left_val.get_type();
        let right_type = right_val.get_type();

        // Long-integer arithmetic path: preserves i64 precision when neither
        // operand is Float or Double, and at least one is Long.
        let use_long = matches!((left_type, right_type),
        (DixType::Long, DixType::Long)
        | (DixType::Long, DixType::Int)
        | (DixType::Int,  DixType::Long)
    );

        if use_long {
            let lv = left_val.as_long();
            let rv = right_val.as_long();
            let result: Result<DixValue, String> = match operator {
                "+" => Ok(DixValue::from_long(lv.wrapping_add(rv))),
                "-" => Ok(DixValue::from_long(lv.wrapping_sub(rv))),
                "*" => Ok(DixValue::from_long(lv.wrapping_mul(rv))),
                "/" => {
                    if rv == 0 {
                        Err("division by zero".to_string())
                    } else {
                        Ok(DixValue::from_long(lv / rv))
                    }
                }
                "%" => {
                    if rv == 0 {
                        Err("division by zero".to_string())
                    } else {
                        Ok(DixValue::from_long(lv % rv))
                    }
                }
                // Power always returns Double for Long (same as standard numeric semantics)
                "**" => Ok(DixValue::from_double((lv as f64).powf(rv as f64))),
                _ => Err(format!("Unknown arithmetic operator: {}", operator)),
            };
            return result.map_err(|e| {
                if e.contains("zero") {
                    InterpreterError::DivisionByZero { position }
                } else {
                    InterpreterError::InvalidOperation { message: e, position }
                }
            });
        }

        // Standard float/double/int path.
        match operator {
            "+" => left_val.add(&right_val),
            "-" => left_val.subtract(&right_val),
            "*" => left_val.multiply(&right_val),
            "/" => left_val.divide(&right_val),
            "%" => Ok(DixValue::from_double(
                left_val.as_double() % right_val.as_double(),
            )),
            "**" => Ok(DixValue::from_double(
                left_val.as_double().powf(right_val.as_double()),
            )),
            _ => Err(format!("Unknown arithmetic operator: {}", operator)),
        }
            .map_err(|e| {
                if e.contains("zero") {
                    InterpreterError::DivisionByZero { position }
                } else {
                    InterpreterError::InvalidOperation { message: e, position }
                }
            })
    }

    fn evaluate_bitwise_op(
        &mut self,
        left: &Expression,
        operator: &str,
        right: &Expression,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let left_val  = self.evaluate_expression(left,  context, scope_context, namespace)?;
        let right_val = self.evaluate_expression(right, context, scope_context, namespace)?;

        if !left_val.is_numeric() || !right_val.is_numeric() {
            return Err(InterpreterError::InvalidOperation {
                message: format!(
                    "Bitwise operator '{}' requires numeric operands, got {} and {}",
                    operator,
                    left_val.get_type().get_type_name(),
                    right_val.get_type().get_type_name()
                ),
                position,
            });
        }

        // If either operand is Long use 64-bit arithmetic; otherwise stay at 32-bit.
        let use_long = left_val.get_type() == DixType::Long
            || right_val.get_type() == DixType::Long;

        if use_long {
            let lv = left_val.as_long();
            let rv = right_val.as_long();
            let result = match operator {
                "<<" => lv << rv,
                ">>" => lv >> rv,
                "&"  => lv & rv,
                "|"  => lv | rv,
                "^"  => lv ^ rv,
                _ => {
                    return Err(InterpreterError::InvalidOperation {
                        message: format!("Unknown bitwise operator: {}", operator),
                        position,
                    })
                }
            };
            Ok(DixValue::from_long(result))
        } else {
            let lv = left_val.as_int();
            let rv = right_val.as_int();
            let result = match operator {
                "<<" => lv << rv,
                ">>" => lv >> rv,
                "&"  => lv & rv,
                "|"  => lv | rv,
                "^"  => lv ^ rv,
                _ => {
                    return Err(InterpreterError::InvalidOperation {
                        message: format!("Unknown bitwise operator: {}", operator),
                        position,
                    })
                }
            };
            Ok(DixValue::from_int(result))
        }
    }

    fn evaluate_circular_modulo(
        &mut self,
        left: &Expression,
        right: &Expression,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let left_val  = self.evaluate_expression(left,  context, scope_context, namespace)?;
        let right_val = self.evaluate_expression(right, context, scope_context, namespace)?;

        if !left_val.is_numeric() || !right_val.is_numeric() {
            return Err(InterpreterError::InvalidOperation {
                message: "Circular modulo requires numeric operands".to_string(),
                position,
            });
        }

        let lt = left_val.get_type();
        let rt = right_val.get_type();

        // Integer-only path: preserves Long precision
        let is_integer_only = matches!(lt, DixType::Int | DixType::Long)
            && matches!(rt, DixType::Int | DixType::Long);

        if is_integer_only {
            let a  = left_val.as_long();
            let b  = right_val.as_long();
            if b == 0 {
                return Err(InterpreterError::DivisionByZero { position });
            }
            let result = ((a % b) + b) % b;
            return Ok(if lt == DixType::Long || rt == DixType::Long {
                DixValue::from_long(result)
            } else {
                DixValue::from_int(result as i32)
            });
        }

        let a = left_val.as_double();
        let b = right_val.as_double();
        let result = ((a % b) + b) % b;

        Ok(if lt == DixType::Float || rt == DixType::Float {
            DixValue::from_float(result as f32)
        } else {
            DixValue::from_double(result)
        })
    }

    fn evaluate_percentage(
        &mut self,
        amount: &Expression,
        percentage: &Expression,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let amount_val =
            self.evaluate_expression(amount, context, scope_context, namespace)?;
        let percentage_val =
            self.evaluate_expression(percentage, context, scope_context, namespace)?;

        if !amount_val.is_numeric() || !percentage_val.is_numeric() {
            return Err(InterpreterError::InvalidOperation {
                message: "Percentage operator requires numeric operands".to_string(),
                position,
            });
        }

        let result = (amount_val.as_double() * percentage_val.as_double()) / 100.0;

        Ok(
            if amount_val.get_type() == DixType::Int
                && percentage_val.get_type() == DixType::Int
            {
                DixValue::from_int(result as i32)
            } else if amount_val.get_type() == DixType::Float
                || percentage_val.get_type() == DixType::Float
            {
                DixValue::from_float(result as f32)
            } else {
                DixValue::from_double(result)
            },
        )
    }

    fn evaluate_bitwise_modulo(
        &mut self,
        left: &Expression,
        right: &Expression,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let left_val  = self.evaluate_expression(left,  context, scope_context, namespace)?;
        let right_val = self.evaluate_expression(right, context, scope_context, namespace)?;

        if !left_val.is_numeric() || !right_val.is_numeric() {
            return Err(InterpreterError::InvalidOperation {
                message: "Bitwise modulo requires numeric operands".to_string(),
                position,
            });
        }

        let use_long = left_val.get_type() == DixType::Long
            || right_val.get_type() == DixType::Long;

        if use_long {
            let rv = right_val.as_long();
            if rv <= 0 {
                return Err(InterpreterError::InvalidOperation {
                    message: "Bitwise modulo (&%) requires a positive right operand".to_string(),
                    position,
                });
            }
            Ok(DixValue::from_long(left_val.as_long() & (rv - 1)))
        } else {
            let rv = right_val.as_int();
            if rv <= 0 {
                return Err(InterpreterError::InvalidOperation {
                    message: "Bitwise modulo (&%) requires a positive right operand".to_string(),
                    position,
                });
            }
            Ok(DixValue::from_int(left_val.as_int() & (rv - 1)))
        }
    }

    fn evaluate_comparison_op(
        &mut self,
        left: &Expression,
        operator: &str,
        right: &Expression,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let left_val =
            self.evaluate_expression(left, context, scope_context, namespace)?;
        let right_val =
            self.evaluate_expression(right, context, scope_context, namespace)?;

        let result: Result<bool, InterpreterError> = match operator {
            "==" => Ok(left_val.equal_to(&right_val)),
            "!=" => Ok(!left_val.equal_to(&right_val)),
            "<" => left_val.less_than(&right_val).map_err(|e| {
                InterpreterError::InvalidOperation { message: e, position }
            }),
            ">" => left_val.greater_than(&right_val).map_err(|e| {
                InterpreterError::InvalidOperation { message: e, position }
            }),
            "<=" => {
                let less = left_val
                    .less_than(&right_val)
                    .map_err(|e| InterpreterError::InvalidOperation { message: e, position })?;
                Ok(less || left_val.equal_to(&right_val))
            }
            ">=" => {
                let greater = left_val
                    .greater_than(&right_val)
                    .map_err(|e| InterpreterError::InvalidOperation { message: e, position })?;
                Ok(greater || left_val.equal_to(&right_val))
            }
            _ => {
                return Err(InterpreterError::InvalidOperation {
                    message: format!("Unknown comparison operator: {}", operator),
                    position,
                })
            }
        };

        Ok(DixValue::from_bool(result?))
    }

    fn evaluate_logical_op(
        &mut self,
        left: &Expression,
        operator: &str,
        right: &Expression,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let left_val =
            self.evaluate_expression(left, context, scope_context, namespace)?;

        match operator {
            "&&" | "and" => {
                if !left_val.as_bool() {
                    return Ok(DixValue::from_bool(false));
                }
                let right_val =
                    self.evaluate_expression(right, context, scope_context, namespace)?;
                Ok(DixValue::from_bool(right_val.as_bool()))
            }
            "||" | "or" => {
                if left_val.as_bool() {
                    return Ok(DixValue::from_bool(true));
                }
                let right_val =
                    self.evaluate_expression(right, context, scope_context, namespace)?;
                Ok(DixValue::from_bool(right_val.as_bool()))
            }
            _ => Err(InterpreterError::InvalidOperation {
                message: format!("Unknown logical operator: {}", operator),
                position,
            }),
        }
    }

    fn evaluate_unary_op(
        &mut self,
        operator: &str,
        operand: &Expression,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let operand_val =
            self.evaluate_expression(operand, context, scope_context, namespace)?;

        match operator {
            "-" => {
                if !operand_val.is_numeric() {
                    return Err(InterpreterError::InvalidOperation {
                        message: "Cannot negate non-numeric value".to_string(),
                        position,
                    });
                }
                // Preserve integer types when negating; only fall to double for floats
                Ok(match operand_val.get_type() {
                    DixType::Long   => DixValue::from_long(-operand_val.as_long()),
                    DixType::Int    => DixValue::from_int(-operand_val.as_int()),
                    DixType::Float  => DixValue::from_float(-operand_val.as_float()),
                    _               => DixValue::from_double(-operand_val.as_double()),
                })
            }
            "!" | "not" => Ok(DixValue::from_bool(!operand_val.as_bool())),
            "++" => {
                if !operand_val.is_numeric() {
                    return Err(InterpreterError::InvalidOperation {
                        message: "Cannot increment non-numeric value".to_string(),
                        position,
                    });
                }
                Ok(match operand_val.get_type() {
                    DixType::Long  => DixValue::from_long(operand_val.as_long().wrapping_add(1)),
                    DixType::Int   => DixValue::from_int(operand_val.as_int().wrapping_add(1)),
                    DixType::Float => DixValue::from_float(operand_val.as_float() + 1.0),
                    _              => DixValue::from_double(operand_val.as_double() + 1.0),
                })
            }
            "--" => {
                if !operand_val.is_numeric() {
                    return Err(InterpreterError::InvalidOperation {
                        message: "Cannot decrement non-numeric value".to_string(),
                        position,
                    });
                }
                Ok(match operand_val.get_type() {
                    DixType::Long  => DixValue::from_long(operand_val.as_long().wrapping_sub(1)),
                    DixType::Int   => DixValue::from_int(operand_val.as_int().wrapping_sub(1)),
                    DixType::Float => DixValue::from_float(operand_val.as_float() - 1.0),
                    _              => DixValue::from_double(operand_val.as_double() - 1.0),
                })
            }
            "~?" => {
                // Bitwise NOT — use Long when operand is Long to avoid truncation
                if !operand_val.is_numeric() {
                    return Err(InterpreterError::InvalidOperation {
                        message: "Bitwise NOT requires numeric operand".to_string(),
                        position,
                    });
                }
                Ok(match operand_val.get_type() {
                    DixType::Long => DixValue::from_long(!operand_val.as_long()),
                    _             => DixValue::from_int(!operand_val.as_int()),
                })
            }
            _ => Err(InterpreterError::InvalidOperation {
                message: format!("Unknown unary operator: {}", operator),
                position,
            }),
        }
    }

    fn evaluate_conditional(
        &mut self,
        condition: &Expression,
        true_value: &Expression,
        false_value: &Expression,
        _position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let cond =
            self.evaluate_expression(condition, context, scope_context, namespace)?;

        if cond.as_bool() {
            self.evaluate_expression(true_value, context, scope_context, namespace)
        } else {
            self.evaluate_expression(false_value, context, scope_context, namespace)
        }
    }

    fn evaluate_static_method_call(
        &mut self,
        object_name: &str,
        method_name: &str,
        arguments: &[Expression],
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "[StaticMethodCall] {}.{} with {} args",
                object_name,
                method_name,
                arguments.len()
            ));
        }

        let mut args = Vec::with_capacity(arguments.len());
        for (i, arg) in arguments.iter().enumerate() {
            let val =
                self.evaluate_expression(arg, context, scope_context, namespace)?;
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "[StaticMethodCall] Arg[{}]: {} = {}",
                    i,
                    val.get_type().get_type_name(),
                    val
                ));
            }
            args.push(val);
        }

        builtin_call_resolver::resolve_static_call(object_name, method_name, &args)
            .map_err(|e| InterpreterError::BuiltinCallFailed {
                object: object_name.to_string(),
                method: method_name.to_string(),
                message: e,
                position,
            })
    }

    fn evaluate_instance_method_call(
        &mut self,
        instance: &Expression,
        method_name: &str,
        arguments: &[Expression],
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let instance_val =
            self.evaluate_expression(instance, context, scope_context, namespace)?;

        let mut args = Vec::with_capacity(arguments.len());
        for arg in arguments {
            args.push(
                self.evaluate_expression(arg, context, scope_context, namespace)?,
            );
        }

        builtin_call_resolver::resolve_instance_call(&instance_val, method_name, &args)
            .map_err(|e| InterpreterError::BuiltinCallFailed {
                object: format!("{:?}", instance_val.get_type()),
                method: method_name.to_string(),
                message: e,
                position,
            })
    }

    fn evaluate_property_access(
        &mut self,
        object: &Expression,
        property: &str,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let obj =
            self.evaluate_expression(object, context, scope_context, namespace)?;

        if obj.get_type() != DixType::Object {
            return Err(InterpreterError::InvalidOperation {
                message: format!(
                    "Cannot access property '{}' on non-object type {}",
                    property,
                    obj.get_type().get_type_name()
                ),
                position,
            });
        }

        obj.as_object()
            .get(property)
            .cloned()
            .ok_or_else(|| InterpreterError::PropertyNotFound {
                property: property.to_string(),
                position,
            })
    }

    fn evaluate_index_access(
        &mut self,
        object: &Expression,
        index: &Expression,
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let obj =
            self.evaluate_expression(object, context, scope_context, namespace)?;
        let idx =
            self.evaluate_expression(index, context, scope_context, namespace)?;

        match obj.get_type() {
            DixType::Array | DixType::Tuple => {
                let array = obj.as_array();
                let index_val = idx.as_int() as usize;
                if index_val >= array.len() {
                    return Err(InterpreterError::IndexOutOfBounds {
                        index: index_val as i64,
                        length: array.len(),
                        position,
                    });
                }
                Ok(array[index_val].clone())
            }
            DixType::Object => {
                let key = idx.as_string();
                obj.as_object()
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| InterpreterError::PropertyNotFound {
                        property: key,
                        position,
                    })
            }
            DixType::String => {
                let s = obj.as_string();
                let index_val = idx.as_int() as usize;
                if index_val >= s.len() {
                    return Err(InterpreterError::IndexOutOfBounds {
                        index: index_val as i64,
                        length: s.len(),
                        position,
                    });
                }
                Ok(DixValue::from_string(
                    s.chars().nth(index_val).unwrap().to_string(),
                ))
            }
            _ => Err(InterpreterError::InvalidOperation {
                message: format!(
                    "Cannot index type {}",
                    obj.get_type().get_type_name()
                ),
                position,
            }),
        }
    }

fn evaluate_enum_access(
    &self,
    namespace_name: Option<&str>,
    enum_name: &str,
    value: &str,
    position: Position,
    namespace: Option<&ImportedNamespace>,
) -> Result<DixValue, InterpreterError> {
    if let Some(ns_name) = namespace_name {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[EnumAccess] Imported: {}.{}.{}",
                ns_name, enum_name, value
            ));
        }

        let ns = self
            .resolve_namespace(ns_name, namespace)
            .ok_or_else(|| InterpreterError::NamespaceNotFound {
                name: ns_name.to_string(),
                position,
            })?;

        let enum_fields = ns.enums.get(enum_name).ok_or_else(|| {
            InterpreterError::InvalidEnumAccess {
                location: format!("{}.{}.{}", ns_name, enum_name, value),
                position,
            }
        })?;

        let field_value = enum_fields.get(value).ok_or_else(|| {
            InterpreterError::InvalidEnumAccess {
                location: format!("{}.{}.{}", ns_name, enum_name, value),
                position,
            }
        })?;

        return Ok(DixValue::from_int(*field_value));
    }

    // FIX: when executing inside an imported namespace function, that namespace's
    // own @ENUMS are not in self.symbol_table (the caller's table). Check the
    // current execution namespace's enums first before falling back globally.
    if let Some(current_ns) = namespace {
        if let Some(fields) = current_ns.enums.get(enum_name) {
            if let Some(&int_val) = fields.get(value) {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "[EnumAccess] Resolved '{}' from current namespace enums",
                        format!("{}.{}", enum_name, value)
                    ));
                }
                return Ok(DixValue::from_int(int_val));
            }
        }
    }

    // Fall back to the global (caller's) symbol table for locally-defined enums.
    self.symbol_table
        .try_get_enum_field_value(enum_name, value)
        .map(DixValue::from_int)
        .ok_or_else(|| InterpreterError::InvalidEnumAccess {
            location: format!("{}.{}", enum_name, value),
            position,
        })
}

    fn evaluate_config_access(
        &self,
        key: &str,
        position: Position,
    ) -> Result<DixValue, InterpreterError> {
        self.symbol_table
            .get_config(key)
            .map(|s| DixValue::from_string(s.clone()))
            .ok_or_else(|| InterpreterError::ConfigKeyNotFound {
                key: key.to_string(),
                position,
            })
    }

    // =========================================================================
    // THE FIX: evaluate_quick_func_call and evaluate_imported_function_call
    //
    // Root cause: arguments were raw Expression nodes evaluated inside the
    // callee's empty context. They must be evaluated in the CALLER's context
    // first, then passed as already-resolved Value nodes to the callee.
    // =========================================================================

    fn evaluate_quick_func_call(
        &mut self,
        name: &str,
        arguments: &[Expression],
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        // Lambda registry takes priority.
        if let Some(lambda) = self.lambda_registry.get(name).cloned() {
            if self.debug_config.is_enabled {
                self.error_manager
                    .log_debug(&format!("[Lambda] Invoking: {}", name));
            }
            return self.invoke_lambda(
                &lambda, arguments, position, context, scope_context, namespace,
            );
        }

        // FIX: evaluate all arguments in the CALLER's context before constructing
        // the callee's context. This prevents parameter names from the callee's
        // own signature (e.g. "rarity", "baseValue") from being looked up in an
        // empty execution context during bind_parameters.
        let evaluated_args = self.evaluate_arguments_in_caller_context(
            arguments, position, context, scope_context, namespace,
        )?;

        // Wrap each resolved DixValue as a literal Expression::Value so that
        // bind_parameters can call evaluate_expression on them safely — they
        // will simply return the wrapped value immediately.
        let literal_args: Vec<Expression> = evaluated_args
            .iter()
            .map(|dv| Expression::Value {
                value: self.dix_value_to_ast_value(dv, position),
                position,
            })
            .collect();

        // Check current namespace functions first.
        if let Some(ns) = namespace {
            if let Some(func_info) = ns.functions.get(name) {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug(&format!(
                        "[QuickFuncCall] Found '{}' in current namespace",
                        name
                    ));
                }
                let func_ast = func_info.ast.clone();
                let mut nested_context = ExecutionContext::new(name, None);
                return self.execute(
                    &func_ast,
                    &literal_args,
                    &mut nested_context,
                    scope_context,
                    namespace,
                );
            }
        }

        // Clone to release immutable borrow before execute() takes mutable borrow.
        let function = self
            .quick_functions
            .iter()
            .find(|f| f.name == name)
            .cloned()
            .ok_or_else(|| InterpreterError::UndefinedFunction {
                name: name.to_string(),
                position,
            })?;

        let mut nested_context = ExecutionContext::new(name, None);
        self.execute(&function, &literal_args, &mut nested_context, scope_context, None)
    }

    fn evaluate_imported_function_call(
        &mut self,
        namespace_name: &str,
        function_name: &str,
        arguments: &[Expression],
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[ImportedCall] {}.{}",
                namespace_name, function_name
            ));
        }

        // FIX: evaluate all arguments in the CALLER's context first.
        // The callee is in a different namespace and its execution context
        // starts empty — it cannot see the caller's local variables.
        let evaluated_args = self.evaluate_arguments_in_caller_context(
            arguments, position, context, scope_context, None,
        )?;

        let literal_args: Vec<Expression> = evaluated_args
            .iter()
            .map(|dv| Expression::Value {
                value: self.dix_value_to_ast_value(dv, position),
                position,
            })
            .collect();

        // Resolve and clone the function AST before taking a mutable borrow.
        let (func_ast, target_namespace) = {
            let ns = self
                .resolve_namespace(namespace_name, None)
                .ok_or_else(|| InterpreterError::NamespaceNotFound {
                    name: namespace_name.to_string(),
                    position,
                })?;

            let func_ast = ns
                .functions
                .get(function_name)
                .ok_or_else(|| InterpreterError::FunctionNotInNamespace {
                    namespace: namespace_name.to_string(),
                    function: function_name.to_string(),
                    position,
                })?
                .ast
                .clone();

            (func_ast, ns as *const ImportedNamespace)
        };

        // SAFETY: symbol_table lives for 'a which outlives this call.
        let ns_ref: &ImportedNamespace = unsafe { &*target_namespace };

        let fqn = format!("{}.{}", namespace_name, function_name);
        let mut imported_context = ExecutionContext::new(&fqn, None);

        self.execute(
            &func_ast,
            &literal_args,
            &mut imported_context,
            scope_context,
            Some(ns_ref),
        )
    }

    // =========================================================================
    // Helpers for the fixed call sites
    // =========================================================================

    /// Evaluate every argument expression in the *caller's* context, returning
    /// a vec of resolved DixValues. Called before constructing any callee context.
    pub fn evaluate_arguments_in_caller_context(
    &mut self,
    arguments: &[Expression],
    position: Position,
    context: &mut ExecutionContext,
    scope_context: &FxHashMap<String, String>,
    namespace: Option<&ImportedNamespace>,
) -> Result<Vec<DixValue>, InterpreterError> {
    let mut evaluated = Vec::with_capacity(arguments.len());
    for (i, arg) in arguments.iter().enumerate() {
        let val = self
            .evaluate_expression(arg, context, scope_context, namespace)
            .map_err(|e| InterpreterError::ParameterEvalFailed {
                index: i,
                param_name: format!("arg{}", i),
                inner: Box::new(e),
                position: arg.position(),
            })?;
        evaluated.push(val);
    }
    Ok(evaluated)
        }

    /// Convert a DixValue back to a lightweight AST Value literal so it can be
    /// passed to execute() → bind_parameters() → evaluate_expression(), which
    /// will return it immediately without any variable lookup.
    fn dix_value_to_ast_value(&self, dix: &DixValue, position: Position) -> Value {
        match dix.get_type() {
            DixType::Int       => Value::Integer { value: dix.as_int(), position },
            DixType::Long      => Value::Long    { value: dix.as_long(), position },
            DixType::Float     => Value::Float   { value: dix.as_float(), position },
            DixType::Double    => Value::Double  { value: dix.as_double(), position },
            DixType::String    => Value::String  { value: dix.as_string(), position },
            DixType::Bool      => Value::Boolean { value: dix.as_bool(), position },
            DixType::Null      => Value::Null { position },
            DixType::Hex       => Value::HexColor { value: dix.as_string(), position },
            DixType::Blob      => Value::PrefixedConstructor {
                prefix: "b".to_string(),
                arguments: vec![Value::String {
                    value: dix.as_blob_base64().unwrap_or_default(),
                    position,
                }],
                position,
            },
            DixType::Regex     => Value::PrefixedConstructor {
                prefix: "r".to_string(),
                arguments: vec![Value::String {
                    value: dix.as_string(),
                    position,
                }],
                position,
            },
            DixType::Array | DixType::Tuple => {
                let values: Vec<Value> = dix
                    .as_array()
                    .iter()
                    .map(|item| self.dix_value_to_ast_value(item, position))
                    .collect();
                Value::Array { values, position }
            }
            DixType::Object    => {
                let properties: Vec<ObjectProperty> = dix
                    .as_object()
                    .iter()
                    .map(|(key, val)| ObjectProperty {
                        key: key.clone(),
                        value: self.dix_value_to_ast_value(val, position),
                        position,
                    })
                    .collect();
                Value::Object { properties, position }
            }
            DixType::Date      => Value::Date      { value: dix.as_string(), position },
            DixType::Timestamp => Value::Timestamp { value: dix.as_string(), position },
            _                  => Value::String    { value: dix.as_string(), position },
        }
    }

    fn invoke_lambda(
        &mut self,
        lambda: &LambdaAst,
        arguments: &[Expression],
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        if lambda.params.len() != arguments.len() {
            return Err(InterpreterError::LambdaParamMismatch {
                expected: lambda.params.len(),
                got: arguments.len(),
                position,
            });
        }

        let mut lambda_context = ExecutionContext::new("<lambda>", None);

        for (i, param_name) in lambda.params.iter().enumerate() {
            let arg_value =
                self.evaluate_expression(&arguments[i], context, scope_context, namespace)?;
            lambda_context
                .define_variable(param_name, arg_value)
                .map_err(|e| InterpreterError::InvalidOperation {
                    message: e.to_string(),
                    position,
                })?;
        }

        self.evaluate_expression(
            &lambda.body,
            &mut lambda_context,
            scope_context,
            namespace,
        )
    }

    fn resolve_namespace(
        &self,
        namespace_name: &str,
        current_namespace: Option<&'a ImportedNamespace>,
    ) -> Option<&'a ImportedNamespace> {
        if let Some(ns) = self.symbol_table.try_get_namespace(namespace_name) {
            return Some(ns);
        }
        if let Some(current_ns) = current_namespace {
            if let Some(local_ns) = current_ns.local_imports.get(namespace_name) {
                return Some(local_ns);
            }
        }
        None
    }

    fn convert_ast_value_to_dix_value(
        &mut self,
        value: &Value,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        match value {
            Value::Expression { expr, .. } => {
                self.evaluate_expression(expr, context, scope_context, namespace)
            }
            Value::Integer { value, .. }            => Ok(DixValue::from_int(*value)),
            Value::Long { value, .. }               => Ok(DixValue::from_long(*value)),
            Value::Float { value, .. }              => Ok(DixValue::from_float(*value)),
            Value::Double { value, .. }             => Ok(DixValue::from_double(*value)),
            Value::ScientificNotation { value, .. } => Ok(DixValue::from_double(*value)),
            Value::String { value, .. }             => Ok(DixValue::from_string(value.clone())),
            Value::Boolean { value, .. }            => Ok(DixValue::from_bool(*value)),
            Value::Null { .. }                      => Ok(DixValue::null()),
            Value::HexColor { value, .. }           => Ok(DixValue::from_hex(value.clone())),

            Value::Array { values, position }
            | Value::NestedArray { values, position, .. } => {
                self.convert_array(values, *position, context, scope_context, namespace)
            }

            Value::Object { properties, position } => {
                self.convert_object_literal(
                    properties, *position, context, scope_context, namespace,
                )
            }

            Value::PrefixedConstructor { prefix, arguments, position } => {
                self.convert_prefixed_constructor(
                    prefix, arguments, *position, context, scope_context, namespace,
                )
            }

            Value::Lambda { parameters, .. } => Ok(DixValue::from_string(format!(
                "<lambda:{}_params>",
                parameters.len()
            ))),

            Value::QuickFuncCall { function_name, arguments, position } => {
                self.evaluate_quick_func_call(
                    function_name, arguments, *position, context, scope_context, namespace,
                )
            }

            Value::InterpolatedString { template, expressions, position } => {
                self.evaluate_interpolated_string(
                    template, expressions, *position, context, scope_context, namespace,
                )
            }

            Value::EnumValue { enum_name, value: enum_value, position } => {
                self.evaluate_enum_access(None, enum_name, enum_value, *position, namespace)
            }

            Value::Identifier { value: id_value, position } => {
                self.resolve_identifier(id_value, *position, context, scope_context)
            }

            Value::Date { value, .. } => {
                use chrono::NaiveDate;
                let date =
                    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|e| {
                        InterpreterError::InvalidOperation {
                            message: format!("Invalid date format: {}", e),
                            position: Position::UNKNOWN,
                        }
                    })?;
                Ok(DixValue::from_date(
                    chrono::DateTime::from_naive_utc_and_offset(
                        date.and_hms_opt(0, 0, 0).unwrap(),
                        chrono::Utc,
                    ),
                ))
            }

            Value::Timestamp { value, .. } => {
                let timestamp = value
                    .parse::<chrono::DateTime<chrono::Utc>>()
                    .map_err(|e| InterpreterError::InvalidOperation {
                        message: format!("Invalid timestamp format: {}", e),
                        position: Position::UNKNOWN,
                    })?;
                Ok(DixValue::from_timestamp(timestamp))
            }

            _ => Err(InterpreterError::InvalidOperation {
                message: format!("Unsupported value type: {}", value_variant_name(value)),
                position: Position::UNKNOWN,
            }),
        }
    }

    fn convert_array(
        &mut self,
        values: &[Value],
        _position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let mut dix_values = Vec::with_capacity(values.len());
        for value in values {
            dix_values.push(
                self.convert_ast_value_to_dix_value(value, context, scope_context, namespace)?,
            );
        }
        Ok(DixValue::from_array(dix_values))
    }

    fn convert_object_literal(
        &mut self,
        properties: &[ObjectProperty],
        _position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        let mut dix_obj =
            FxHashMap::with_capacity_and_hasher(properties.len(), Default::default());
        for prop in properties {
            let value = self.convert_ast_value_to_dix_value(
                &prop.value, context, scope_context, namespace,
            )?;
            dix_obj.insert(prop.key.clone(), value);
        }
        Ok(DixValue::from_object(dix_obj.into_iter().collect()))
    }

    fn convert_prefixed_constructor(
        &mut self,
        prefix: &str,
        arguments: &[Value],
        position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        match prefix.to_lowercase().as_str() {
            "t" => {
                let cap = arguments.len().min(6);
                let mut tuple_values = Vec::with_capacity(cap);
                for arg in arguments.iter().take(6) {
                    tuple_values.push(self.convert_ast_value_to_dix_value(
                        arg, context, scope_context, namespace,
                    )?);
                }
                Ok(DixValue::from_tuple(tuple_values))
            }
            "b" => {
                let base64_data = if arguments.is_empty() {
                    String::new()
                } else {
                    match &arguments[0] {
                        Value::String { value, .. } => value.clone(),
                        other => self
                            .convert_ast_value_to_dix_value(
                                other, context, scope_context, namespace,
                            )?
                            .as_string(),
                    }
                };
                DixValue::from_blob(base64_data).map_err(|e| {
                    InterpreterError::InvalidOperation { message: e, position }
                })
            }
            "r" => {
                let pattern = if arguments.is_empty() {
                    ".*".to_string()
                } else {
                    match &arguments[0] {
                        Value::String { value, .. } => value.clone(),
                        other => self
                            .convert_ast_value_to_dix_value(
                                other, context, scope_context, namespace,
                            )?
                            .as_string(),
                    }
                };
                DixValue::from_regex(pattern).map_err(|e| {
                    InterpreterError::InvalidOperation { message: e, position }
                })
            }
            _ => Err(InterpreterError::InvalidOperation {
                message: format!("Unknown prefix constructor: {}", prefix),
                position,
            }),
        }
    }

    fn evaluate_interpolated_string(
        &mut self,
        template: &str,
        expressions: &[Expression],
        _position: Position,
        context: &mut ExecutionContext,
        scope_context: &FxHashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "[InterpolatedString] Template: '{}', expressions: {}",
                template,
                expressions.len()
            ));
        }

        let mut result = String::with_capacity(template.len() + expressions.len() * 8);
        result.push_str(template);

        for (i, expr) in expressions.iter().enumerate() {
            let value =
                self.evaluate_expression(expr, context, scope_context, namespace)?;
            let placeholder = format!("{{{}}}", i);
            let value_string = value.as_string();

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "[InterpolatedString] Replacing '{}' with '{}'",
                    placeholder, value_string
                ));
            }

            result = result.replace(&placeholder, &value_string);
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "[InterpolatedString] Result: '{}'",
                result
            ));
        }

        Ok(DixValue::from_string(result))
    }
}

// =============================================================================
// Diagnostic helpers — zero allocation, used only on debug/error paths
// =============================================================================

fn statement_variant_name(stmt: &QuickFuncStatement) -> &'static str {
    match stmt {
        QuickFuncStatement::Return { .. }              => "Return",
        QuickFuncStatement::Assignment { .. }          => "Assignment",
        QuickFuncStatement::ArithmeticAssignment { .. }=> "ArithmeticAssignment",
        QuickFuncStatement::If { .. }                  => "If",
        QuickFuncStatement::Switch { .. }              => "Switch",
        QuickFuncStatement::Log { .. }                 => "Log",
        QuickFuncStatement::VariableDeclaration { .. } => "VariableDeclaration",
        QuickFuncStatement::ExpressionStatement { .. } => "ExpressionStatement",
        QuickFuncStatement::ObjectCreation { .. }      => "ObjectCreation",
    }
}

fn value_variant_name(value: &Value) -> &'static str {
    match value {
        Value::Integer { .. }            => "Integer",
        Value::Long { .. }            => "Long",
        Value::Float { .. }              => "Float",
        Value::Double { .. }             => "Double",
        Value::ScientificNotation { .. } => "ScientificNotation",
        Value::String { .. }             => "String",
        Value::Boolean { .. }            => "Boolean",
        Value::InterpolatedString { .. } => "InterpolatedString",
        Value::HexColor { .. }           => "HexColor",
        Value::Date { .. }               => "Date",
        Value::Timestamp { .. }          => "Timestamp",
        Value::Null { .. }               => "Null",
        Value::Array { .. }              => "Array",
        Value::NestedArray { .. }        => "NestedArray",
        Value::Object { .. }             => "Object",
        Value::PrefixedConstructor { .. }=> "PrefixedConstructor",
        Value::EnumValue { .. }          => "EnumValue",
        Value::Identifier { .. }         => "Identifier",
        Value::QuickFuncCall { .. }      => "QuickFuncCall",
        Value::Expression { .. }         => "Expression",
        Value::Range { .. }              => "Range",
        Value::Lambda { .. }             => "Lambda",
        Value::ParseError { .. }         => "ParseError",
        Value::Error { .. }              => "Error",
        Value::Unknown { .. }            => "Unknown",
    }
}

fn expr_variant_name(expr: &Expression) -> &'static str {
    match expr {
        Expression::Identifier { .. }          => "Identifier",
        Expression::QualifiedIdentifier { .. } => "QualifiedIdentifier",
        Expression::FunctionCall { .. }        => "FunctionCall",
        Expression::QuickFuncCall { .. }       => "QuickFuncCall",
        Expression::DixFunctionCall { .. }     => "DixFunctionCall",
        Expression::StaticMethodCall { .. }    => "StaticMethodCall",
        Expression::InstanceMethodCall { .. }  => "InstanceMethodCall",
        Expression::BuiltinFunction { .. }     => "BuiltinFunction",
        Expression::StaticFunction { .. }      => "StaticFunction",
        Expression::ImportedFunctionCall { .. }=> "ImportedFunctionCall",
        Expression::ArithmeticOp { .. }        => "ArithmeticOp",
        Expression::BitwiseOp { .. }           => "BitwiseOp",
        Expression::ComparisonOp { .. }        => "ComparisonOp",
        Expression::LogicalOp { .. }           => "LogicalOp",
        Expression::UnaryOp { .. }             => "UnaryOp",
        Expression::ConfigAccess { .. }        => "ConfigAccess",
        Expression::EnumAccess { .. }          => "EnumAccess",
        Expression::ObjectAccess { .. }        => "ObjectAccess",
        Expression::PropertyAccess { .. }      => "PropertyAccess",
        Expression::IndexAccess { .. }         => "IndexAccess",
        Expression::Value { .. }               => "Value",
        Expression::Parenthesized { .. }       => "Parenthesized",
        Expression::Conditional { .. }         => "Conditional",
        Expression::TypeCast { .. }            => "TypeCast",
    }
}
