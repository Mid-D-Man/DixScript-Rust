// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena/export.rs"
// ============================================================================
//! Copies a tree out of the arena as the original owned types.
//!
//! The exact mirror of `import.rs`: every `match` lists every variant with no wildcard arm,
//! and `export(import(x)) == x` for every tree. Exporting is how a consumer that has not
//! moved to the arena yet still gets the owned tree it expects.

use super::{Arena, CaseList, ExprId, ExprList, PropList, StmtId, StmtList, StrList, ValueId, ValueList};
use super::nodes::{ExprNode, StmtNode, ValueNode};
use crate::Compiler::AST::{
    Expression, ObjectProperty, QuickFuncStatement, SwitchCase, Value,
};

impl Arena {
    /// Copies the tree rooted at `id` out of the arena as an owned [`Value`].
    ///
    /// The result equals the value that was imported: `export_value(import_value(v)) == v`.
    ///
    /// # Panics
    ///
    /// Panics if `id`, or anything it reaches, is out of range for this arena, for example an
    /// id from another arena.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::Arena;
    /// use dixscript::Compiler::AST::{Position, Value};
    ///
    /// let mut arena = Arena::new();
    /// let id = arena.import_value(&Value::Null { position: Position::new(1, 1) });
    /// assert_eq!(arena.export_value(id), Value::Null { position: Position::new(1, 1) });
    /// ```
    pub fn export_value(&self, id: ValueId) -> Value {
        match *self.value(id) {
            ValueNode::Integer { value, position } => Value::Integer { value, position },
            ValueNode::Long { value, position } => Value::Long { value, position },
            ValueNode::Float { value, position } => Value::Float { value, position },
            ValueNode::Double { value, position } => Value::Double { value, position },
            ValueNode::ScientificNotation { value, position } => Value::ScientificNotation { value, position },
            ValueNode::String { value, position } => Value::String { value: self.text(value).to_string(), position },
            ValueNode::Boolean { value, position } => Value::Boolean { value, position },
            ValueNode::InterpolatedString { template, expressions, position } => Value::InterpolatedString {
                template: self.text(template).to_string(),
                expressions: self.export_expr_list(expressions),
                position,
            },
            ValueNode::HexColor { value, position } => Value::HexColor { value: self.text(value).to_string(), position },
            ValueNode::Date { value, position } => Value::Date { value: self.text(value).to_string(), position },
            ValueNode::Timestamp { value, position } => Value::Timestamp { value: self.text(value).to_string(), position },
            ValueNode::Null { position } => Value::Null { position },
            ValueNode::Array { values, position } => Value::Array { values: self.export_value_list(values), position },
            ValueNode::NestedArray { values, level, position } => Value::NestedArray {
                values: self.export_value_list(values),
                level,
                position,
            },
            ValueNode::Object { properties, position } => Value::Object {
                properties: self.export_props(properties),
                position,
            },
            ValueNode::PrefixedConstructor { prefix, arguments, position } => Value::PrefixedConstructor {
                prefix: self.text(prefix).to_string(),
                arguments: self.export_value_list(arguments),
                position,
            },
            ValueNode::EnumValue { enum_name, value, position } => Value::EnumValue {
                enum_name: self.text(enum_name).to_string(),
                value: self.text(value).to_string(),
                position,
            },
            ValueNode::Identifier { value, position } => Value::Identifier { value: self.text(value).to_string(), position },
            ValueNode::QuickFuncCall { function_name, arguments, position } => Value::QuickFuncCall {
                function_name: self.text(function_name).to_string(),
                arguments: self.export_expr_list(arguments),
                position,
            },
            ValueNode::Expression { expr, position } => Value::Expression {
                expr: Box::new(self.export_expr(expr)),
                position,
            },
            ValueNode::Range { start, end, position } => Value::Range {
                start: Box::new(self.export_value(start)),
                end: Box::new(self.export_value(end)),
                position,
            },
            ValueNode::Lambda { parameters, body, statements, position } => Value::Lambda {
                parameters: self.export_strings(parameters),
                body: Box::new(self.export_expr(body)),
                statements: self.export_stmt_list(statements),
                position,
            },
            ValueNode::ParseError { message, position } => Value::ParseError { message: self.text(message).to_string(), position },
            ValueNode::Error { message, position } => Value::Error { message: self.text(message).to_string(), position },
            ValueNode::Unknown { data, position } => {
                let d = self.unknown(data);
                Value::Unknown {
                    element_type: d.element_type.clone(),
                    element_name: d.element_name.clone(),
                    element_data: d.element_data.clone(),
                    compatibility_result: d.compatibility_result.clone(),
                    position,
                }
            }
        }
    }

    /// Copies the tree rooted at `id` out of the arena as an owned [`Expression`].
    ///
    /// See [`Arena::export_value`].
    ///
    /// # Panics
    ///
    /// Panics if `id`, or anything it reaches, is out of range for this arena, for example an
    /// id from another arena.
    pub fn export_expr(&self, id: ExprId) -> Expression {
        match *self.expr(id) {
            ExprNode::Identifier { name, position } => Expression::Identifier { name: self.text(name).to_string(), position },
            ExprNode::QualifiedIdentifier { parts, arguments, position } => Expression::QualifiedIdentifier {
                parts: self.export_strings(parts),
                arguments: arguments.map(|a| self.export_expr_list(a)),
                position,
            },
            ExprNode::FunctionCall { name, arguments, position } => Expression::FunctionCall {
                name: self.text(name).to_string(),
                arguments: self.export_expr_list(arguments),
                position,
            },
            ExprNode::QuickFuncCall { name, arguments, position } => Expression::QuickFuncCall {
                name: self.text(name).to_string(),
                arguments: self.export_expr_list(arguments),
                position,
            },
            ExprNode::DixFunctionCall { function_name, arguments, position } => Expression::DixFunctionCall {
                function_name: self.text(function_name).to_string(),
                arguments: self.export_expr_list(arguments),
                position,
            },
            ExprNode::StaticMethodCall { object_name, method_name, arguments, position } => Expression::StaticMethodCall {
                object_name: self.text(object_name).to_string(),
                method_name: self.text(method_name).to_string(),
                arguments: self.export_expr_list(arguments),
                position,
            },
            ExprNode::InstanceMethodCall { instance, method_name, arguments, position } => Expression::InstanceMethodCall {
                instance: Box::new(self.export_expr(instance)),
                method_name: self.text(method_name).to_string(),
                arguments: self.export_expr_list(arguments),
                position,
            },
            ExprNode::BuiltinFunction { target, method, arguments, position } => Expression::BuiltinFunction {
                target: Box::new(self.export_expr(target)),
                method: self.text(method).to_string(),
                arguments: arguments.map(|a| self.export_expr_list(a)),
                position,
            },
            ExprNode::StaticFunction { class_name, method, arguments, position } => Expression::StaticFunction {
                class_name: self.text(class_name).to_string(),
                method: self.text(method).to_string(),
                arguments: self.export_expr_list(arguments),
                position,
            },
            ExprNode::ImportedFunctionCall { namespace_name, function_name, arguments, position } => Expression::ImportedFunctionCall {
                namespace_name: self.text(namespace_name).to_string(),
                function_name: self.text(function_name).to_string(),
                arguments: self.export_expr_list(arguments),
                position,
            },
            ExprNode::ArithmeticOp { left, operator, right, position } => Expression::ArithmeticOp {
                left: Box::new(self.export_expr(left)),
                operator: self.text(operator).to_string(),
                right: Box::new(self.export_expr(right)),
                position,
            },
            ExprNode::BitwiseOp { left, operator, right, position } => Expression::BitwiseOp {
                left: Box::new(self.export_expr(left)),
                operator: self.text(operator).to_string(),
                right: Box::new(self.export_expr(right)),
                position,
            },
            ExprNode::ComparisonOp { left, operator, right, position } => Expression::ComparisonOp {
                left: Box::new(self.export_expr(left)),
                operator: self.text(operator).to_string(),
                right: Box::new(self.export_expr(right)),
                position,
            },
            ExprNode::LogicalOp { left, operator, right, position } => Expression::LogicalOp {
                left: Box::new(self.export_expr(left)),
                operator: self.text(operator).to_string(),
                right: Box::new(self.export_expr(right)),
                position,
            },
            ExprNode::UnaryOp { operator, operand, position } => Expression::UnaryOp {
                operator: self.text(operator).to_string(),
                operand: Box::new(self.export_expr(operand)),
                position,
            },
            ExprNode::ConfigAccess { key, position } => Expression::ConfigAccess { key: self.text(key).to_string(), position },
            ExprNode::EnumAccess { namespace_name, enum_name, value, position } => Expression::EnumAccess {
                namespace_name: namespace_name.map(|n| self.text(n).to_string()),
                enum_name: self.text(enum_name).to_string(),
                value: self.text(value).to_string(),
                position,
            },
            ExprNode::ObjectAccess { path, position } => Expression::ObjectAccess { path: self.export_strings(path), position },
            ExprNode::PropertyAccess { object, property, position } => Expression::PropertyAccess {
                object: Box::new(self.export_expr(object)),
                property: self.text(property).to_string(),
                position,
            },
            ExprNode::IndexAccess { object, index, position } => Expression::IndexAccess {
                object: Box::new(self.export_expr(object)),
                index: Box::new(self.export_expr(index)),
                position,
            },
            ExprNode::Value { value, position } => Expression::Value { value: self.export_value(value), position },
            ExprNode::Parenthesized { expression, position } => Expression::Parenthesized {
                expression: Box::new(self.export_expr(expression)),
                position,
            },
            ExprNode::Conditional { condition, true_value, false_value, position } => Expression::Conditional {
                condition: Box::new(self.export_expr(condition)),
                true_value: Box::new(self.export_expr(true_value)),
                false_value: Box::new(self.export_expr(false_value)),
                position,
            },
            ExprNode::TypeCast { expression, target_type, position } => Expression::TypeCast {
                expression: Box::new(self.export_expr(expression)),
                target_type,
                position,
            },
        }
    }

    /// Copies the tree rooted at `id` out of the arena as an owned [`QuickFuncStatement`].
    ///
    /// See [`Arena::export_value`].
    ///
    /// # Panics
    ///
    /// Panics if `id`, or anything it reaches, is out of range for this arena, for example an
    /// id from another arena.
    pub fn export_stmt(&self, id: StmtId) -> QuickFuncStatement {
        match *self.stmt(id) {
            StmtNode::Return { value, position } => QuickFuncStatement::Return { value: self.export_expr(value), position },
            StmtNode::If { condition, then_branch, else_branch, position } => QuickFuncStatement::If {
                condition: self.export_expr(condition),
                then_branch: self.export_stmt_list(then_branch),
                else_branch: else_branch.map(|b| self.export_stmt_list(b)),
                position,
            },
            StmtNode::Switch { expression, cases, default_case, position } => QuickFuncStatement::Switch {
                expression: self.export_expr(expression),
                cases: self.export_cases(cases),
                default_case: default_case.map(|c| self.export_case(self.case(c))),
                position,
            },
            StmtNode::Assignment { variable, value, position } => QuickFuncStatement::Assignment {
                variable: self.text(variable).to_string(),
                value: self.export_expr(value),
                position,
            },
            StmtNode::ArithmeticAssignment { variable, operator, value, position } => QuickFuncStatement::ArithmeticAssignment {
                variable: self.text(variable).to_string(),
                operator: self.text(operator).to_string(),
                value: self.export_expr(value),
                position,
            },
            StmtNode::ObjectCreation { variable, object, position } => QuickFuncStatement::ObjectCreation {
                variable: self.text(variable).to_string(),
                object: self.export_value(object),
                position,
            },
            StmtNode::Log { value, position } => QuickFuncStatement::Log { value: self.export_expr(value), position },
            StmtNode::ExpressionStatement { expression, position } => QuickFuncStatement::ExpressionStatement {
                expression: self.export_expr(expression),
                position,
            },
            StmtNode::VariableDeclaration { declaration_type, is_mutable, variable_name, data_type, value, position } => {
                QuickFuncStatement::VariableDeclaration {
                    declaration_type,
                    is_mutable,
                    variable_name: self.text(variable_name).to_string(),
                    data_type,
                    value: self.export_expr(value),
                    position,
                }
            }
        }
    }

    fn export_case(&self, case: &super::SwitchCaseNode) -> SwitchCase {
        SwitchCase {
            case_value: self.export_value(case.case_value),
            statements: self.export_stmt_list(case.statements),
            position: case.position,
        }
    }

    // -- lists --------------------------------------------------------------

    fn export_value_list(&self, list: ValueList) -> Vec<Value> {
        self.value_list(list).iter().map(|id| self.export_value(*id)).collect()
    }

    fn export_expr_list(&self, list: ExprList) -> Vec<Expression> {
        self.expr_list(list).iter().map(|id| self.export_expr(*id)).collect()
    }

    fn export_stmt_list(&self, list: StmtList) -> Vec<QuickFuncStatement> {
        self.stmt_list(list).iter().map(|id| self.export_stmt(*id)).collect()
    }

    fn export_strings(&self, list: StrList) -> Vec<String> {
        self.str_list(list).iter().map(|s| self.text(*s).to_string()).collect()
    }

    fn export_props(&self, list: PropList) -> Vec<ObjectProperty> {
        self.prop_list(list)
            .iter()
            .map(|p| ObjectProperty {
                key: self.text(p.key).to_string(),
                value: self.export_value(p.value),
                position: p.position,
            })
            .collect()
    }

    fn export_cases(&self, list: CaseList) -> Vec<SwitchCase> {
        self.case_list(list).iter().map(|c| self.export_case(c)).collect()
    }
}
