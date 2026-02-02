// src/Compiler/Core/ValueResolution/function_interpreter.rs
//!
//! FunctionInterpreter — Executes QuickFunction bodies at compile time.
//!
//! ## Key improvements over C#:
//! - Parameter threading eliminates mutable field swapping (no save/restore dance)
//! - DebugConfig cached — format!() never called when debug is off
//! - All errors are typed InterpreterError variants (no string throws)
//! - Recursion depth has both absolute and dynamic limits
//! - Dead code removed (EvaluateIdentifier, duplicate Blob/Regex converters)

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::Builtins::Core::{DixType, DixValue};
use crate::Builtins::Resolver::BuiltinCallResolver;
use crate::Compiler::AST::{
    Expression, Position, QuickFunction, QuickFuncParam, QuickFuncStatement,
    SwitchCase, Value, ObjectProperty,
};
use crate::Compiler::Core::DebugMode;
use crate::Compiler::Utilities::SymbolTable;
use crate::ErrorManager::ErrorManager;

use super::execution_context::ExecutionContext;
use super::supporting_classes::{DebugConfig, ImportedNamespace};

// ==================== RECURSION LIMITS ====================

/// Absolute maximum recursion depth - never exceeded
const ABSOLUTE_MAX_RECURSION: u32 = 10000;

/// Base recursion depth for simple functions
const BASE_RECURSION_DEPTH: u32 = 1000;

/// Calculate dynamic recursion limit based on function complexity
fn calculate_recursion_limit(param_count: usize, body_size: usize) -> u32 {
    // Factor in parameter count and body size
    let complexity_factor = (param_count * 10 + body_size / 5) as u32;
    let dynamic_limit = BASE_RECURSION_DEPTH + complexity_factor;
    
    // Never exceed absolute maximum
    dynamic_limit.min(ABSOLUTE_MAX_RECURSION)
}

// ==================== INTERPRETER ERROR ====================

/// Typed errors from function interpretation
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

impl std::fmt::Display for InterpreterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterpreterError::RecursionLimitExceeded { function_name, position, depth, limit } => {
                write!(f, "Recursion limit exceeded in function '{}' at {} (depth: {}, limit: {})",
                       function_name, position, depth, limit)
            }
            InterpreterError::UndefinedVariable { name, function_name, position, checked_scopes } => {
                write!(f, "Undefined variable '{}' in function '{}' at {} (checked: {})",
                       name, function_name, position, checked_scopes)
            }
            InterpreterError::UndefinedFunction { name, position } => {
                write!(f, "Undefined function '{}' at {}", name, position)
            }
            InterpreterError::ParameterCountMismatch { expected, got, required, position } => {
                write!(f, "Parameter count mismatch at {}: expected {}, got {} (required: {})",
                       position, expected, got, required)
            }
            InterpreterError::DivisionByZero { position } => {
                write!(f, "Division by zero at {}", position)
            }
            InterpreterError::InvalidOperation { message, position } => {
                write!(f, "Invalid operation at {}: {}", position, message)
            }
            _ => write!(f, "{:?}", self), // Fallback for other variants
        }
    }
}

impl std::error::Error for InterpreterError {}

// ==================== LAMBDA AST ====================

/// Lambda representation for registry
#[derive(Debug, Clone)]
pub struct LambdaAst {
    pub params: Vec<String>,
    pub body: Expression,
}

// ==================== FUNCTION INTERPRETER ====================

/// Executes QuickFunction bodies at compile time with parameter threading
pub struct FunctionInterpreter<'a> {
    symbol_table: &'a SymbolTable,
    quick_functions: Vec<QuickFunction>,
    data_context: Rc<RefCell<HashMap<String, DixValue>>>,
    debug_config: DebugConfig,
    recursion_depth: u32,
    current_recursion_limit: u32,
    lambda_registry: HashMap<String, LambdaAst>,
    log_statements: Vec<String>,
    error_manager: ErrorManager,
}

impl<'a> FunctionInterpreter<'a> {
    // ==================== CONSTRUCTOR ====================
    
    pub fn new(
        symbol_table: &'a SymbolTable,
        quick_functions: Vec<QuickFunction>,
        data_context: Rc<RefCell<HashMap<String, DixValue>>>,
        debug_mode: DebugMode,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        
        FunctionInterpreter {
            symbol_table,
            quick_functions,
            data_context,
            debug_config: DebugConfig::from_mode(debug_mode),
            recursion_depth: 0,
            current_recursion_limit: BASE_RECURSION_DEPTH,
            lambda_registry: HashMap::new(),
            log_statements: Vec::new(),
            error_manager,
        }
    }
    
    // ==================== DATA CONTEXT UPDATE ====================
    
    pub fn update_data_context(&mut self, key: String, value: DixValue) {
        self.data_context.borrow_mut().insert(key.clone(), value.clone());
        
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Updated data context: {} = {}",
                key,
                value.get_type().get_type_name()
            ));
        }
    }
    
    // ==================== MAIN EXECUTION ENTRY ====================
    
    /// Execute function with parameter threading (no mutable field swapping)
    pub fn execute(
        &mut self,
        function: &QuickFunction,
        arguments: &[Expression],
        context: &mut ExecutionContext,
        scope_context: &HashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        // Calculate dynamic recursion limit based on function complexity
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
                Some("Check for infinite recursion".to_string()),
                Some(function.name.clone()),
            );
            
            return Err(err);
        }
        
        // Bind parameters
        self.bind_parameters(&function.parameters, arguments, context, scope_context, namespace)?;
        
        let mut last_result = DixValue::null();
        
        // Execute body statements
        for (i, statement) in function.body.iter().enumerate() {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[Execute] Statement {}/{}: {:?}",
                    i + 1,
                    function.body.len(),
                    statement_variant_name(statement)
                ));
            }
            
            last_result = self.execute_statement(statement, context, scope_context, namespace)?;
            
            // Early return on explicit return
            if matches!(statement, QuickFuncStatement::Return { .. }) {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug(&format!(
                        "[Execute] Explicit return: {} = {}",
                        last_result.get_type().get_type_name(),
                        last_result
                    ));
                }
                self.recursion_depth -= 1;
                return Ok(last_result);
            }
        }
        
        // Implicit return (last statement result)
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[Execute] Implicit return: {} = {}",
                last_result.get_type().get_type_name(),
                last_result
            ));
        }
        
        self.recursion_depth -= 1;
        Ok(last_result)
    }
    
    // ==================== PARAMETER BINDING ====================
    
    fn bind_parameters(
        &mut self,
        parameters: &[QuickFuncParam],
        arguments: &[Expression],
        context: &mut ExecutionContext,
        scope_context: &HashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<(), InterpreterError> {
        let required_count = parameters.iter().filter(|p| p.default_value.is_none()).count();
        
        if arguments.len() < required_count {
            let position = parameters.first()
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
                // Evaluate argument
                self.evaluate_expression(&arguments[i], context, scope_context, namespace)
                    .map_err(|e| InterpreterError::ParameterEvalFailed {
                        index: i,
                        param_name: param.name.clone(),
                        inner: Box::new(e),
                        position: arguments[i].position(),
                    })?
            } else if let Some(ref default) = param.default_value {
                // Use default value
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
            
            context.define_variable(&param.name, value)
                .map_err(|e| InterpreterError::InvalidOperation {
                    message: e.to_string(),
                    position: param.position,
                })?;
        }
        
        Ok(())
    }
    
    // ==================== IDENTIFIER RESOLUTION (4-TIER PRIORITY) ====================
    
    /// CRITICAL: 4-tier priority identifier resolution
    /// 1. Execution context (parameters/locals)
    /// 2. Scope context → data context (array item isolation)
    /// 3. Path-suffix search in data context
    /// 4. Direct key lookup in data context
    fn resolve_identifier(
        &self,
        name: &str,
        position: Position,
        context: &ExecutionContext,
        scope_context: &HashMap<String, String>,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!("[ResolveId] Resolving: {}", name));
        }
        
        // Priority 1: Execution context
        if let Ok(value) = context.get_variable(name) {
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "  ✅ Found in execution context: {}",
                    value.get_type().get_type_name()
                ));
            }
            return Ok(value);
        }
        
        // Priority 2: Scope context → data context (CRITICAL for array item isolation)
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
                        "  ✅ Found in scope context: {}",
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
        
        // Priority 3: Path-suffix search
        if let Some(value) = self.try_resolve_by_path_suffix(name) {
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "  ✅ Found by path search: {}",
                    value.get_type().get_type_name()
                ));
            }
            return Ok(value);
        }
        
        // Priority 4: Direct global lookup
        if let Some(value) = self.data_context.borrow().get(name) {
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "  ✅ Found in global context: {}",
                    value.get_type().get_type_name()
                ));
            }
            return Ok(value.clone());
        }
        
        // Not found anywhere
        let scope_keys = scope_context.keys()
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
        let data_ctx = self.data_context.borrow();
        
        for (key, value) in data_ctx.iter() {
            if key == name || key.ends_with(&format!(".{}", name)) {
                return Some(value.clone());
            }
        }
        
        None
    }
    
    // ==================== STATEMENT EXECUTION DISPATCHER ====================
    
    fn execute_statement(
        &mut self,
        statement: &QuickFuncStatement,
        context: &mut ExecutionContext,
        scope_context: &HashMap<String, String>,
        namespace: Option<&ImportedNamespace>,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "[ExecuteStatement] {:?}",
                statement_variant_name(statement)
            ));
        }
        
        match statement {
            QuickFuncStatement::Return { value, .. } => {
                self.execute_return(value, context, scope_context, namespace)
            }
            QuickFuncStatement::Assignment { variable, value, position } => {
                self.execute_assignment(variable, value, *position, context, scope_context, namespace)
            }
            QuickFuncStatement::ArithmeticAssignment { variable, operator, value, position } => {
                self.execute_arithmetic_assignment(variable, operator, value, *position, context, scope_context, namespace)
            }
            QuickFuncStatement::If { condition, then_branch, else_branch, position } => {
                self.execute_if(condition, then_branch, else_branch.as_ref(), *position, context, scope_context, namespace)
            }
            QuickFuncStatement::Switch { expression, cases, default_case, position } => {
                self.execute_switch(expression, cases, default_case.as_ref(), *position, context, scope_context, namespace)
            }
            QuickFuncStatement::Log { value, position } => {
                self.execute_log(value, *position, context, scope_context, namespace)
            }
            QuickFuncStatement::VariableDeclaration { variable_name, value, position, .. } => {
                self.execute_variable_declaration(variable_name, value, *position, context, scope_context, namespace)
            }
            QuickFuncStatement::ExpressionStatement { expression, .. } => {
                self.evaluate_expression(expression, context, scope_context, namespace)
            }
            QuickFuncStatement::ObjectCreation { variable, object, position } => {
                self.execute_object_creation(variable, object, *position, context, scope_context, namespace)
            }
        }
}
