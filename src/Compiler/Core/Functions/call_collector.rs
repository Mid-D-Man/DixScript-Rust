use super::call_graph::CallGraph;
use crate::Compiler::AST::{QuickFunction, QuickFuncStatement, Expression, Value};
use std::collections::HashSet;

/// Scans QuickFunction AST nodes to collect all function-to-function calls
/// Builds the call graph by visiting every expression and statement
///
/// CRITICAL: Properly tracks lambda parameters and local variables
/// to avoid treating lambda invocations as function calls
///
/// Handles:
/// - Direct function calls: funcA()
/// - Calls in expressions: x = funcA() + funcB()
/// - Calls in control flow: if: funcA() > 10 { ... }
/// - Nested calls: funcA(funcB(x))
/// - Lambda invocations: func = (x) => x * 2; func(5)  // NOT a function call
/// - Nested lambdas with parameter shadowing
pub struct FunctionCallCollector<'a> {
    call_graph: &'a mut CallGraph,
    current_function: String,
    
    /// Track local scope (lambda params, function params, local vars)
    /// Stack of scopes - inner scopes can shadow outer scopes
    local_scope_stack: Vec<HashSet<String>>,
}

impl<'a> FunctionCallCollector<'a> {
    pub fn new(call_graph: &'a mut CallGraph) -> Self {
        FunctionCallCollector {
            call_graph,
            current_function: String::new(),
            local_scope_stack: Vec::new(),
        }
    }
    
    /// Analyze a single function and add all its calls to the graph
    pub fn analyze_function(&mut self, func: &QuickFunction) {
        self.current_function = func.name.clone();
        
        // Register the function in the graph (even if it makes no calls)
        self.call_graph.add_function(func.name.clone());
        
        // Create local scope with function parameters
        let function_scope: HashSet<String> = func.parameters
            .iter()
            .map(|p| p.name.clone())
            .collect();
        self.local_scope_stack.push(function_scope);
        
        // Scan all statements in the function body
        for statement in &func.body {
            self.analyze_statement(statement);
        }
        
        // Clean up function scope
        self.local_scope_stack.pop();
    }
    
    /// Check if identifier is in local scope (lambda param, function param, or local var)
    fn is_in_local_scope(&self, name: &str) -> bool {
        // Check all active scopes (inner to outer)
        self.local_scope_stack.iter().any(|scope| scope.contains(name))
    }
    
    /// Add identifier to current scope (for local variables)
    fn add_to_current_scope(&mut self, name: String) {
        if let Some(scope) = self.local_scope_stack.last_mut() {
            scope.insert(name);
        }
    }
    
    // ==================== STATEMENT ANALYSIS ====================
    
    fn analyze_statement(&mut self, statement: &QuickFuncStatement) {
        match statement {
            QuickFuncStatement::Return { value, .. } => {
                self.analyze_expression(value);
            }
            
            QuickFuncStatement::Assignment { variable, value, .. } => {
                // Track local variable
                self.add_to_current_scope(variable.clone());
                self.analyze_expression(value);
            }
            
            QuickFuncStatement::ArithmeticAssignment { value, .. } => {
                self.analyze_expression(value);
            }
            
            QuickFuncStatement::If { condition, then_branch, else_branch, .. } => {
                // Condition
                self.analyze_expression(condition);
                
                // Then branch
                for stmt in then_branch {
                    self.analyze_statement(stmt);
                }
                
                // Else branch
                if let Some(else_stmts) = else_branch {
                    for stmt in else_stmts {
                        self.analyze_statement(stmt);
                    }
                }
            }
            
            QuickFuncStatement::Switch { expression, cases, default_case, .. } => {
                // Switch expression
                self.analyze_expression(expression);
                
                // All cases
                for case in cases {
                    for stmt in &case.statements {
                        self.analyze_statement(stmt);
                    }
                }
                
                // Default case
                if let Some(default) = default_case {
                    for stmt in &default.statements {
                        self.analyze_statement(stmt);
                    }
                }
            }
            
            QuickFuncStatement::Log { value, .. } => {
                self.analyze_expression(value);
            }
            
            QuickFuncStatement::ExpressionStatement { expression, .. } => {
                self.analyze_expression(expression);
            }
            
            QuickFuncStatement::ObjectCreation { variable, object, .. } => {
                // Track local variable
                self.add_to_current_scope(variable.clone());
                self.analyze_value(object);
            }
            
            QuickFuncStatement::VariableDeclaration { variable_name, value, .. } => {
                // Track local variable
                self.add_to_current_scope(variable_name.clone());
                self.analyze_expression(value);
            }
        }
    }
    
    // ==================== EXPRESSION ANALYSIS ====================
    
    fn analyze_expression(&mut self, expr: &Expression) {
        match expr {
            Expression::QuickFuncCall { name, arguments, .. } => {
                // CRITICAL FIX: Check if it's a lambda/local variable invocation
                if self.is_in_local_scope(name) {
                    // This is a lambda invocation or local variable call
                    // DON'T add to call graph, just analyze arguments
                    for arg in arguments {
                        self.analyze_expression(arg);
                    }
                    return;
                }
                
                // REAL FUNCTION CALL - Add to call graph
                self.call_graph.add_edge(
                    self.current_function.clone(),
                    name.clone(),
                    expr.position(),
                );
                
                // Analyze arguments (they might also contain function calls)
                for arg in arguments {
                    self.analyze_expression(arg);
                }
            }
            
            Expression::ArithmeticOp { left, right, .. } => {
                self.analyze_expression(left);
                self.analyze_expression(right);
            }
            
            Expression::BitwiseOp { left, right, .. } => {
                self.analyze_expression(left);
                self.analyze_expression(right);
            }
            
            Expression::ComparisonOp { left, right, .. } => {
                self.analyze_expression(left);
                self.analyze_expression(right);
            }
            
            Expression::LogicalOp { left, right, .. } => {
                self.analyze_expression(left);
                self.analyze_expression(right);
            }
            
            Expression::UnaryOp { operand, .. } => {
                self.analyze_expression(operand);
            }
            
            Expression::Conditional { condition, true_value, false_value, .. } => {
                self.analyze_expression(condition);
                self.analyze_expression(true_value);
                self.analyze_expression(false_value);
            }
            
            Expression::Parenthesized { expression, .. } => {
                self.analyze_expression(expression);
            }
            
            Expression::PropertyAccess { object, .. } => {
                self.analyze_expression(object);
            }
            
            Expression::IndexAccess { object, index, .. } => {
                self.analyze_expression(object);
                self.analyze_expression(index);
            }
            
            Expression::InstanceMethodCall { instance, arguments, .. } => {
                self.analyze_expression(instance);
                for arg in arguments {
                    self.analyze_expression(arg);
                }
            }
            
            Expression::StaticMethodCall { arguments, .. } => {
                for arg in arguments {
                    self.analyze_expression(arg);
                }
            }
            
            Expression::DixFunctionCall { arguments, .. } => {
                for arg in arguments {
                    self.analyze_expression(arg);
                }
            }
            
            Expression::Value { value, .. } => {
                self.analyze_value(value);
            }
            
            Expression::TypeCast { expression, .. } => {
                self.analyze_expression(expression);
            }
            
            Expression::FunctionCall { arguments, .. } => {
                for arg in arguments {
                    self.analyze_expression(arg);
                }
            }
            
            Expression::ImportedFunctionCall { arguments, .. } => {
                for arg in arguments {
                    self.analyze_expression(arg);
                }
            }
            
            Expression::QualifiedIdentifier { arguments, .. } => {
                if let Some(args) = arguments {
                    for arg in args {
                        self.analyze_expression(arg);
                    }
                }
            }
            
            Expression::BuiltinFunction { target, arguments, .. } => {
                self.analyze_expression(target);
                if let Some(args) = arguments {
                    for arg in args {
                        self.analyze_expression(arg);
                    }
                }
            }
            
            Expression::StaticFunction { arguments, .. } => {
                for arg in arguments {
                    self.analyze_expression(arg);
                }
            }
            
            // Simple expressions with no sub-expressions
            Expression::Identifier { .. }
            | Expression::EnumAccess { .. }
            | Expression::ConfigAccess { .. }
            | Expression::ObjectAccess { .. } => {
                // No function calls possible in these
            }
        }
    }
    
    // ==================== VALUE ANALYSIS ====================
    
    fn analyze_value(&mut self, value: &Value) {
        match value {
            Value::Lambda { parameters, body, .. } => {
                // Create scope for lambda parameters
                let lambda_scope: HashSet<String> = parameters.iter().cloned().collect();
                self.local_scope_stack.push(lambda_scope);
                
                // Analyze lambda body (can now see lambda parameters)
                self.analyze_expression(body);
                
                // Pop lambda scope when done
                self.local_scope_stack.pop();
            }
            
            Value::Array { values, .. } => {
                for elem in values {
                    self.analyze_value(elem);
                }
            }
            
            Value::NestedArray { values, .. } => {
                for elem in values {
                    self.analyze_value(elem);
                }
            }
            
            Value::Object { properties, .. } => {
                for prop in properties {
                    self.analyze_value(&prop.value);
                }
            }
            
            Value::PrefixedConstructor { arguments, .. } => {
                for arg in arguments {
                    self.analyze_value(arg);
                }
            }
            
            Value::InterpolatedString { expressions, .. } => {
                for expr in expressions {
                    self.analyze_expression(expr);
                }
            }
            
            Value::QuickFuncCall { function_name, arguments, .. } => {
                // SAME FIX: Check if it's a lambda invocation
                if self.is_in_local_scope(function_name) {
                    // Lambda invocation - just analyze arguments
                    for arg in arguments {
                        self.analyze_expression(arg);
                    }
                    return;
                }
                
                // Real function call - add to call graph
                self.call_graph.add_edge(
                    self.current_function.clone(),
                    function_name.clone(),
                    value.position(),
                );
                
                for arg in arguments {
                    self.analyze_expression(arg);
                }
            }
            
            Value::Expression { expr, .. } => {
                self.analyze_expression(expr);
            }
            
            Value::Range { start, end, .. } => {
                self.analyze_value(start);
                self.analyze_value(end);
            }
            
            // Primitives and simple values - no function calls
            Value::Integer { .. }
            | Value::Float { .. }
            | Value::Double { .. }
            | Value::ScientificNotation { .. }
            | Value::HexColor { .. }
            | Value::String { .. }
            | Value::Boolean { .. }
            | Value::Date { .. }
            | Value::Timestamp { .. }
            | Value::Null { .. }
            | Value::EnumValue { .. }
            | Value::Identifier { .. }
            | Value::ParseError { .. }
            | Value::Error { .. }
            | Value::Unknown { .. } => {
                // No function calls possible
            }
        }
    }
}
