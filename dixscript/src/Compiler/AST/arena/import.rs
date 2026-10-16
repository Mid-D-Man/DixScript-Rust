// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena/import.rs"
// ============================================================================
//! Copies an owned tree into the arena.
//!
//! Every `match` here lists every variant with no wildcard arm, so adding a variant to
//! `Value`, `Expression` or `QuickFuncStatement` fails to compile until it is imported.
//!
//! A list is imported by first reserving its whole run in the list buffer with placeholder
//! entries, then filling the entries in as each child is imported. Children that have lists
//! of their own append after the reserved run, so every run stays contiguous without a
//! scratch stack.

use super::{
    Arena, CaseId, CaseList, ExprId, ExprList, ObjectPropNode, PropList, Str, StmtId, StmtList,
    StrList, SwitchCaseNode, UnknownData, UnknownId, ValueId, ValueList,
};
use super::nodes::{ExprNode, StmtNode, ValueNode};
use crate::Compiler::AST::{
    Expression, ObjectProperty, Position, QuickFuncStatement, SwitchCase, Value,
};

const EMPTY_STR: Str = Str { start: 0, len: 0 };

fn placeholder_prop() -> ObjectPropNode {
    ObjectPropNode { key: EMPTY_STR, value: ValueId(0), position: Position::UNKNOWN }
}

fn placeholder_case() -> SwitchCaseNode {
    SwitchCaseNode {
        case_value: ValueId(0),
        statements: StmtList::new(0, 0),
        position: Position::UNKNOWN,
    }
}

impl Arena {
    /// Copies an owned [`Value`] tree into the arena and returns the id of its root.
    ///
    /// Every node, list and string of the tree is copied, so the arena does not borrow from
    /// `value`. [`Arena::export_value`] reverses it exactly.
    ///
    /// # Panics
    ///
    /// Panics if the arena would need more nodes, list entries or text bytes than a `u32` can
    /// address.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::Arena;
    /// use dixscript::Compiler::AST::{Position, Value};
    ///
    /// let value = Value::EnumValue {
    ///     enum_name: "Color".to_string(),
    ///     value: "Red".to_string(),
    ///     position: Position::new(3, 7),
    /// };
    /// let mut arena = Arena::new();
    /// let id = arena.import_value(&value);
    /// assert_eq!(arena.export_value(id), value);
    /// ```
    pub fn import_value(&mut self, value: &Value) -> ValueId {
        let node = match value {
            Value::Integer { value, position } => ValueNode::Integer { value: *value, position: *position },
            Value::Long { value, position } => ValueNode::Long { value: *value, position: *position },
            Value::Float { value, position } => ValueNode::Float { value: *value, position: *position },
            Value::Double { value, position } => ValueNode::Double { value: *value, position: *position },
            Value::ScientificNotation { value, position } => {
                ValueNode::ScientificNotation { value: *value, position: *position }
            }
            Value::String { value, position } => {
                ValueNode::String { value: self.alloc_str(value), position: *position }
            }
            Value::Boolean { value, position } => ValueNode::Boolean { value: *value, position: *position },
            Value::InterpolatedString { template, expressions, position } => {
                let template = self.alloc_str(template);
                let expressions = self.import_expr_list(expressions);
                ValueNode::InterpolatedString { template, expressions, position: *position }
            }
            Value::HexColor { value, position } => {
                ValueNode::HexColor { value: self.alloc_str(value), position: *position }
            }
            Value::Date { value, position } => {
                ValueNode::Date { value: self.alloc_str(value), position: *position }
            }
            Value::Timestamp { value, position } => {
                ValueNode::Timestamp { value: self.alloc_str(value), position: *position }
            }
            Value::Null { position } => ValueNode::Null { position: *position },
            Value::Array { values, position } => {
                ValueNode::Array { values: self.import_value_list(values), position: *position }
            }
            Value::NestedArray { values, level, position } => ValueNode::NestedArray {
                values: self.import_value_list(values),
                level: *level,
                position: *position,
            },
            Value::Object { properties, position } => {
                ValueNode::Object { properties: self.import_props(properties), position: *position }
            }
            Value::PrefixedConstructor { prefix, arguments, position } => {
                let prefix = self.alloc_str(prefix);
                let arguments = self.import_value_list(arguments);
                ValueNode::PrefixedConstructor { prefix, arguments, position: *position }
            }
            Value::EnumValue { enum_name, value, position } => {
                let enum_name = self.alloc_str(enum_name);
                let value = self.alloc_str(value);
                ValueNode::EnumValue { enum_name, value, position: *position }
            }
            Value::Identifier { value, position } => {
                ValueNode::Identifier { value: self.alloc_str(value), position: *position }
            }
            Value::QuickFuncCall { function_name, arguments, position } => {
                let function_name = self.alloc_str(function_name);
                let arguments = self.import_expr_list(arguments);
                ValueNode::QuickFuncCall { function_name, arguments, position: *position }
            }
            Value::Expression { expr, position } => {
                ValueNode::Expression { expr: self.import_expr(expr), position: *position }
            }
            Value::Range { start, end, position } => {
                let start = self.import_value(start);
                let end = self.import_value(end);
                ValueNode::Range { start, end, position: *position }
            }
            Value::Lambda { parameters, body, statements, position } => {
                let parameters = self.import_str_list(parameters);
                let body = self.import_expr(body);
                let statements = self.import_stmt_list(statements);
                ValueNode::Lambda { parameters, body, statements, position: *position }
            }
            Value::ParseError { message, position } => {
                ValueNode::ParseError { message: self.alloc_str(message), position: *position }
            }
            Value::Error { message, position } => {
                ValueNode::Error { message: self.alloc_str(message), position: *position }
            }
            Value::Unknown { element_type, element_name, element_data, compatibility_result, position } => {
                let data = UnknownId(super::to_u32(self.unknowns.len()));
                self.unknowns.push(UnknownData {
                    element_type: element_type.clone(),
                    element_name: element_name.clone(),
                    element_data: element_data.clone(),
                    compatibility_result: compatibility_result.clone(),
                });
                ValueNode::Unknown { data, position: *position }
            }
        };
        self.alloc_value(node)
    }

    /// Copies an owned [`Expression`] tree into the arena and returns the id of its root.
    ///
    /// See [`Arena::import_value`] for the details; [`Arena::export_expr`] reverses it exactly.
    ///
    /// # Panics
    ///
    /// Panics if the arena would need more nodes, list entries or text bytes than a `u32` can
    /// address.
    pub fn import_expr(&mut self, expr: &Expression) -> ExprId {
        let node = match expr {
            Expression::Identifier { name, position } => {
                ExprNode::Identifier { name: self.alloc_str(name), position: *position }
            }
            Expression::QualifiedIdentifier { parts, arguments, position } => {
                let parts = self.import_str_list(parts);
                let arguments = arguments.as_ref().map(|args| self.import_expr_list(args));
                ExprNode::QualifiedIdentifier { parts, arguments, position: *position }
            }
            Expression::FunctionCall { name, arguments, position } => {
                let name = self.alloc_str(name);
                let arguments = self.import_expr_list(arguments);
                ExprNode::FunctionCall { name, arguments, position: *position }
            }
            Expression::QuickFuncCall { name, arguments, position } => {
                let name = self.alloc_str(name);
                let arguments = self.import_expr_list(arguments);
                ExprNode::QuickFuncCall { name, arguments, position: *position }
            }
            Expression::DixFunctionCall { function_name, arguments, position } => {
                let function_name = self.alloc_str(function_name);
                let arguments = self.import_expr_list(arguments);
                ExprNode::DixFunctionCall { function_name, arguments, position: *position }
            }
            Expression::StaticMethodCall { object_name, method_name, arguments, position } => {
                let object_name = self.alloc_str(object_name);
                let method_name = self.alloc_str(method_name);
                let arguments = self.import_expr_list(arguments);
                ExprNode::StaticMethodCall { object_name, method_name, arguments, position: *position }
            }
            Expression::InstanceMethodCall { instance, method_name, arguments, position } => {
                let instance = self.import_expr(instance);
                let method_name = self.alloc_str(method_name);
                let arguments = self.import_expr_list(arguments);
                ExprNode::InstanceMethodCall { instance, method_name, arguments, position: *position }
            }
            Expression::BuiltinFunction { target, method, arguments, position } => {
                let target = self.import_expr(target);
                let method = self.alloc_str(method);
                let arguments = arguments.as_ref().map(|args| self.import_expr_list(args));
                ExprNode::BuiltinFunction { target, method, arguments, position: *position }
            }
            Expression::StaticFunction { class_name, method, arguments, position } => {
                let class_name = self.alloc_str(class_name);
                let method = self.alloc_str(method);
                let arguments = self.import_expr_list(arguments);
                ExprNode::StaticFunction { class_name, method, arguments, position: *position }
            }
            Expression::ImportedFunctionCall { namespace_name, function_name, arguments, position } => {
                let namespace_name = self.alloc_str(namespace_name);
                let function_name = self.alloc_str(function_name);
                let arguments = self.import_expr_list(arguments);
                ExprNode::ImportedFunctionCall { namespace_name, function_name, arguments, position: *position }
            }
            Expression::ArithmeticOp { left, operator, right, position } => {
                let left = self.import_expr(left);
                let operator = self.alloc_str(operator);
                let right = self.import_expr(right);
                ExprNode::ArithmeticOp { left, operator, right, position: *position }
            }
            Expression::BitwiseOp { left, operator, right, position } => {
                let left = self.import_expr(left);
                let operator = self.alloc_str(operator);
                let right = self.import_expr(right);
                ExprNode::BitwiseOp { left, operator, right, position: *position }
            }
            Expression::ComparisonOp { left, operator, right, position } => {
                let left = self.import_expr(left);
                let operator = self.alloc_str(operator);
                let right = self.import_expr(right);
                ExprNode::ComparisonOp { left, operator, right, position: *position }
            }
            Expression::LogicalOp { left, operator, right, position } => {
                let left = self.import_expr(left);
                let operator = self.alloc_str(operator);
                let right = self.import_expr(right);
                ExprNode::LogicalOp { left, operator, right, position: *position }
            }
            Expression::UnaryOp { operator, operand, position } => {
                let operator = self.alloc_str(operator);
                let operand = self.import_expr(operand);
                ExprNode::UnaryOp { operator, operand, position: *position }
            }
            Expression::ConfigAccess { key, position } => {
                ExprNode::ConfigAccess { key: self.alloc_str(key), position: *position }
            }
            Expression::EnumAccess { namespace_name, enum_name, value, position } => {
                let namespace_name = namespace_name.as_ref().map(|n| self.alloc_str(n));
                let enum_name = self.alloc_str(enum_name);
                let value = self.alloc_str(value);
                ExprNode::EnumAccess { namespace_name, enum_name, value, position: *position }
            }
            Expression::ObjectAccess { path, position } => {
                ExprNode::ObjectAccess { path: self.import_str_list(path), position: *position }
            }
            Expression::PropertyAccess { object, property, position } => {
                let object = self.import_expr(object);
                let property = self.alloc_str(property);
                ExprNode::PropertyAccess { object, property, position: *position }
            }
            Expression::IndexAccess { object, index, position } => {
                let object = self.import_expr(object);
                let index = self.import_expr(index);
                ExprNode::IndexAccess { object, index, position: *position }
            }
            Expression::Value { value, position } => {
                ExprNode::Value { value: self.import_value(value), position: *position }
            }
            Expression::Parenthesized { expression, position } => {
                ExprNode::Parenthesized { expression: self.import_expr(expression), position: *position }
            }
            Expression::Conditional { condition, true_value, false_value, position } => {
                let condition = self.import_expr(condition);
                let true_value = self.import_expr(true_value);
                let false_value = self.import_expr(false_value);
                ExprNode::Conditional { condition, true_value, false_value, position: *position }
            }
            Expression::TypeCast { expression, target_type, position } => {
                let expression = self.import_expr(expression);
                ExprNode::TypeCast { expression, target_type: *target_type, position: *position }
            }
        };
        self.alloc_expr(node)
    }

    /// Copies an owned [`QuickFuncStatement`] tree into the arena and returns the id of its
    /// root.
    ///
    /// See [`Arena::import_value`] for the details; [`Arena::export_stmt`] reverses it exactly.
    ///
    /// # Panics
    ///
    /// Panics if the arena would need more nodes, list entries or text bytes than a `u32` can
    /// address.
    pub fn import_stmt(&mut self, stmt: &QuickFuncStatement) -> StmtId {
        let node = match stmt {
            QuickFuncStatement::Return { value, position } => {
                StmtNode::Return { value: self.import_expr(value), position: *position }
            }
            QuickFuncStatement::If { condition, then_branch, else_branch, position } => {
                let condition = self.import_expr(condition);
                let then_branch = self.import_stmt_list(then_branch);
                let else_branch = else_branch.as_ref().map(|b| self.import_stmt_list(b));
                StmtNode::If { condition, then_branch, else_branch, position: *position }
            }
            QuickFuncStatement::Switch { expression, cases, default_case, position } => {
                let expression = self.import_expr(expression);
                let cases = self.import_cases(cases);
                let default_case = default_case.as_ref().map(|case| {
                    let id = CaseId(super::to_u32(self.cases.len()));
                    self.cases.push(placeholder_case());
                    let node = self.import_case(case);
                    self.cases[id.index()] = node;
                    id
                });
                StmtNode::Switch { expression, cases, default_case, position: *position }
            }
            QuickFuncStatement::Assignment { variable, value, position } => {
                let variable = self.alloc_str(variable);
                let value = self.import_expr(value);
                StmtNode::Assignment { variable, value, position: *position }
            }
            QuickFuncStatement::ArithmeticAssignment { variable, operator, value, position } => {
                let variable = self.alloc_str(variable);
                let operator = self.alloc_str(operator);
                let value = self.import_expr(value);
                StmtNode::ArithmeticAssignment { variable, operator, value, position: *position }
            }
            QuickFuncStatement::ObjectCreation { variable, object, position } => {
                let variable = self.alloc_str(variable);
                let object = self.import_value(object);
                StmtNode::ObjectCreation { variable, object, position: *position }
            }
            QuickFuncStatement::Log { value, position } => {
                StmtNode::Log { value: self.import_expr(value), position: *position }
            }
            QuickFuncStatement::ExpressionStatement { expression, position } => {
                StmtNode::ExpressionStatement { expression: self.import_expr(expression), position: *position }
            }
            QuickFuncStatement::VariableDeclaration {
                declaration_type,
                is_mutable,
                variable_name,
                data_type,
                value,
                position,
            } => {
                let variable_name = self.alloc_str(variable_name);
                let value = self.import_expr(value);
                StmtNode::VariableDeclaration {
                    declaration_type: *declaration_type,
                    is_mutable: *is_mutable,
                    variable_name,
                    data_type: *data_type,
                    value,
                    position: *position,
                }
            }
        };
        self.alloc_stmt(node)
    }

    fn import_case(&mut self, case: &SwitchCase) -> SwitchCaseNode {
        let case_value = self.import_value(&case.case_value);
        let statements = self.import_stmt_list(&case.statements);
        SwitchCaseNode { case_value, statements, position: case.position }
    }

    // -- lists --------------------------------------------------------------

    pub(super) fn import_value_list(&mut self, src: &[Value]) -> ValueList {
        let start = self.value_lists.len();
        self.value_lists.resize(start + src.len(), ValueId(0));
        for (k, value) in src.iter().enumerate() {
            let id = self.import_value(value);
            self.value_lists[start + k] = id;
        }
        ValueList::new(start, src.len())
    }

    pub(super) fn import_expr_list(&mut self, src: &[Expression]) -> ExprList {
        let start = self.expr_lists.len();
        self.expr_lists.resize(start + src.len(), ExprId(0));
        for (k, expr) in src.iter().enumerate() {
            let id = self.import_expr(expr);
            self.expr_lists[start + k] = id;
        }
        ExprList::new(start, src.len())
    }

    pub(super) fn import_stmt_list(&mut self, src: &[QuickFuncStatement]) -> StmtList {
        let start = self.stmt_lists.len();
        self.stmt_lists.resize(start + src.len(), StmtId(0));
        for (k, stmt) in src.iter().enumerate() {
            let id = self.import_stmt(stmt);
            self.stmt_lists[start + k] = id;
        }
        StmtList::new(start, src.len())
    }

    pub(super) fn import_str_list(&mut self, src: &[String]) -> StrList {
        let start = self.str_lists.len();
        for s in src {
            let span = self.alloc_str(s);
            self.str_lists.push(span);
        }
        StrList::new(start, src.len())
    }

    pub(super) fn import_props(&mut self, src: &[ObjectProperty]) -> PropList {
        let start = self.props.len();
        self.props.resize(start + src.len(), placeholder_prop());
        for (k, prop) in src.iter().enumerate() {
            let key = self.alloc_str(&prop.key);
            let value = self.import_value(&prop.value);
            self.props[start + k] = ObjectPropNode { key, value, position: prop.position };
        }
        PropList::new(start, src.len())
    }

    pub(super) fn import_cases(&mut self, src: &[SwitchCase]) -> CaseList {
        let start = self.cases.len();
        self.cases.resize(start + src.len(), placeholder_case());
        for (k, case) in src.iter().enumerate() {
            let node = self.import_case(case);
            self.cases[start + k] = node;
        }
        CaseList::new(start, src.len())
    }
}
