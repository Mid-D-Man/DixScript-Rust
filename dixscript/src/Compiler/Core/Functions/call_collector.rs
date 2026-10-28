use super::call_graph::CallGraph;
use crate::Compiler::AST::arena::{Arena, DocQuickFuncs, DocQuickFunction, ExprId, StmtId, ValueId};
use crate::Compiler::AST::arena::{ExprNode, StmtNode, ValueNode};
use crate::Compiler::AST::{QuickFunction, QuickFuncsSection};
use std::collections::HashSet;

/// Scans QuickFunction AST nodes to collect all function-to-function calls
/// Builds the call graph by visiting every expression and statement
///
/// Properly tracks lambda parameters and local variables
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

    /// Analyze a single function held as the owned type and add all its calls to the graph.
    ///
    /// This copies the function into a scratch arena and calls
    /// [`analyze_function_doc`](Self::analyze_function_doc), so the graph is the one that method
    /// builds for the same function.
    ///
    /// # Panics
    ///
    /// Panics if the function needs more nodes or text than an arena can address (more than
    /// `u32::MAX` of either).
    pub fn analyze_function(&mut self, func: &QuickFunction) {
        let mut arena = Arena::new();
        let section = QuickFuncsSection::new(vec![func.clone()], func.position);
        let doc = DocQuickFuncs::from_owned(&mut arena, &section);
        self.analyze_function_doc(&arena, &doc.functions[0]);
    }

    /// Analyze a single function of a `Document` and add all its calls to the graph.
    ///
    /// `func` and `arena` must come from the same `Document`.
    pub fn analyze_function_doc(&mut self, arena: &Arena, func: &DocQuickFunction) {
        self.current_function = func.name.clone();

        // Register the function in the graph (even if it makes no calls)
        self.call_graph.add_function(func.name.clone());

        // Function parameters form the base scope
        let function_scope: HashSet<String> = func.parameters
            .iter()
            .map(|p| p.name.clone())
            .collect();
        self.local_scope_stack.push(function_scope);

        for &statement in arena.stmt_list(func.body) {
            self.analyze_statement(arena, statement);
        }

        self.local_scope_stack.pop();
    }

    /// Check if a name is in any active local scope
    fn is_in_local_scope(&self, name: &str) -> bool {
        self.local_scope_stack.iter().any(|scope| scope.contains(name))
    }

    /// Add a variable to the current (innermost) scope
    fn add_to_current_scope(&mut self, name: String) {
        if let Some(scope) = self.local_scope_stack.last_mut() {
            scope.insert(name);
        }
    }

    fn analyze_statement(&mut self, arena: &Arena, statement: StmtId) {
        match arena.stmt(statement) {
            StmtNode::Return { value, .. } => {
                self.analyze_expression(arena, *value);
            }
            StmtNode::Assignment { variable, value, .. } => {
                // Assignment creates/updates a variable in current scope
                self.add_to_current_scope(arena.text(*variable).to_string());
                self.analyze_expression(arena, *value);
            }
            StmtNode::ArithmeticAssignment { value, .. } => {
                self.analyze_expression(arena, *value);
            }
            StmtNode::If { condition, then_branch, else_branch, .. } => {
                self.analyze_expression(arena, *condition);
                for &stmt in arena.stmt_list(*then_branch) {
                    self.analyze_statement(arena, stmt);
                }
                if let Some(else_stmts) = else_branch {
                    for &stmt in arena.stmt_list(*else_stmts) {
                        self.analyze_statement(arena, stmt);
                    }
                }
            }
            StmtNode::Switch { expression, cases, default_case, .. } => {
                self.analyze_expression(arena, *expression);
                for case in arena.case_list(*cases) {
                    for &stmt in arena.stmt_list(case.statements) {
                        self.analyze_statement(arena, stmt);
                    }
                }
                if let Some(default) = default_case {
                    for &stmt in arena.stmt_list(arena.case(*default).statements) {
                        self.analyze_statement(arena, stmt);
                    }
                }
            }
            StmtNode::Log { value, .. } => {
                self.analyze_expression(arena, *value);
            }
            StmtNode::ExpressionStatement { expression, .. } => {
                self.analyze_expression(arena, *expression);
            }
            StmtNode::ObjectCreation { variable, object, .. } => {
                // Object creation creates a variable in current scope
                self.add_to_current_scope(arena.text(*variable).to_string());
                self.analyze_value(arena, *object);
            }
            StmtNode::VariableDeclaration { variable_name, value, .. } => {
                // Variable declarations add to the current scope
                self.add_to_current_scope(arena.text(*variable_name).to_string());
                self.analyze_expression(arena, *value);
            }
        }
    }

    fn analyze_expression(&mut self, arena: &Arena, expr: ExprId) {
        match arena.expr(expr) {
            ExprNode::QuickFuncCall { name, arguments, .. } => {
                let name = arena.text(*name);
                // If the name is in local scope it is a lambda call or local function, not a
                // global function call.
                if self.is_in_local_scope(name) {
                    for &arg in arena.expr_list(*arguments) {
                        self.analyze_expression(arena, arg);
                    }
                    return;
                }

                // Record this as a function call edge
                self.call_graph.add_edge(
                    self.current_function.clone(),
                    name.to_string(),
                    arena.expr(expr).position(),
                );

                // Recursively analyze arguments (nested calls)
                for &arg in arena.expr_list(*arguments) {
                    self.analyze_expression(arena, arg);
                }
            }

            // Binary operations
            ExprNode::ArithmeticOp { left, right, .. }
            | ExprNode::BitwiseOp { left, right, .. }
            | ExprNode::ComparisonOp { left, right, .. }
            | ExprNode::LogicalOp { left, right, .. } => {
                self.analyze_expression(arena, *left);
                self.analyze_expression(arena, *right);
            }

            ExprNode::UnaryOp { operand, .. } => {
                self.analyze_expression(arena, *operand);
            }

            ExprNode::Conditional { condition, true_value, false_value, .. } => {
                self.analyze_expression(arena, *condition);
                self.analyze_expression(arena, *true_value);
                self.analyze_expression(arena, *false_value);
            }

            ExprNode::Parenthesized { expression, .. } => {
                self.analyze_expression(arena, *expression);
            }

            ExprNode::PropertyAccess { object, .. } => {
                self.analyze_expression(arena, *object);
            }

            ExprNode::IndexAccess { object, index, .. } => {
                self.analyze_expression(arena, *object);
                self.analyze_expression(arena, *index);
            }

            ExprNode::InstanceMethodCall { instance, arguments, .. } => {
                self.analyze_expression(arena, *instance);
                for &arg in arena.expr_list(*arguments) {
                    self.analyze_expression(arena, arg);
                }
            }

            ExprNode::StaticMethodCall { arguments, .. }
            | ExprNode::DixFunctionCall { arguments, .. }
            | ExprNode::FunctionCall { arguments, .. }
            | ExprNode::ImportedFunctionCall { arguments, .. }
            | ExprNode::StaticFunction { arguments, .. } => {
                for &arg in arena.expr_list(*arguments) {
                    self.analyze_expression(arena, arg);
                }
            }

            ExprNode::Value { value, .. } => {
                self.analyze_value(arena, *value);
            }

            ExprNode::TypeCast { expression, .. } => {
                self.analyze_expression(arena, *expression);
            }

            ExprNode::QualifiedIdentifier { arguments, .. } => {
                if let Some(args) = arguments {
                    for &arg in arena.expr_list(*args) {
                        self.analyze_expression(arena, arg);
                    }
                }
            }

            ExprNode::BuiltinFunction { target, arguments, .. } => {
                self.analyze_expression(arena, *target);
                if let Some(args) = arguments {
                    for &arg in arena.expr_list(*args) {
                        self.analyze_expression(arena, arg);
                    }
                }
            }

            // Leaf expressions: no calls inside
            ExprNode::Identifier { .. }
            | ExprNode::EnumAccess { .. }
            | ExprNode::ConfigAccess { .. }
            | ExprNode::ObjectAccess { .. } => {}
        }
    }

    fn analyze_value(&mut self, arena: &Arena, value: ValueId) {
        match arena.value(value) {
            ValueNode::Lambda { parameters, body, .. } => {
                // Lambda parameters create a new scope. Nested lambdas can shadow outer
                // parameters.
                let lambda_scope: HashSet<String> = arena
                    .str_list(*parameters)
                    .iter()
                    .map(|s| arena.text(*s).to_string())
                    .collect();
                self.local_scope_stack.push(lambda_scope);
                self.analyze_expression(arena, *body);
                self.local_scope_stack.pop();
            }

            ValueNode::Array { values, .. } | ValueNode::NestedArray { values, .. } => {
                for &elem in arena.value_list(*values) {
                    self.analyze_value(arena, elem);
                }
            }

            ValueNode::Object { properties, .. } => {
                for prop in arena.prop_list(*properties) {
                    self.analyze_value(arena, prop.value);
                }
            }

            ValueNode::PrefixedConstructor { arguments, .. } => {
                for &arg in arena.value_list(*arguments) {
                    self.analyze_value(arena, arg);
                }
            }

            ValueNode::InterpolatedString { expressions, .. } => {
                for &expr in arena.expr_list(*expressions) {
                    self.analyze_expression(arena, expr);
                }
            }

            ValueNode::QuickFuncCall { function_name, arguments, .. } => {
                let function_name = arena.text(*function_name);
                // If the name is in local scope it is a lambda call, not a function call.
                if self.is_in_local_scope(function_name) {
                    for &arg in arena.expr_list(*arguments) {
                        self.analyze_expression(arena, arg);
                    }
                    return;
                }

                self.call_graph.add_edge(
                    self.current_function.clone(),
                    function_name.to_string(),
                    arena.value(value).position(),
                );

                for &arg in arena.expr_list(*arguments) {
                    self.analyze_expression(arena, arg);
                }
            }

            ValueNode::Expression { expr, .. } => {
                self.analyze_expression(arena, *expr);
            }

            ValueNode::Range { start, end, .. } => {
                self.analyze_value(arena, *start);
                self.analyze_value(arena, *end);
            }

            // Leaf values: no calls inside
            ValueNode::Integer { .. }
            | ValueNode::Long { .. }
            | ValueNode::Float { .. }
            | ValueNode::Double { .. }
            | ValueNode::ScientificNotation { .. }
            | ValueNode::HexColor { .. }
            | ValueNode::String { .. }
            | ValueNode::Boolean { .. }
            | ValueNode::Date { .. }
            | ValueNode::Timestamp { .. }
            | ValueNode::Null { .. }
            | ValueNode::EnumValue { .. }
            | ValueNode::Identifier { .. }
            | ValueNode::ParseError { .. }
            | ValueNode::Error { .. }
            | ValueNode::Unknown { .. } => {}
        }
    }
}
