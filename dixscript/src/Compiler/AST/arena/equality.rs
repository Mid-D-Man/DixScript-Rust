// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena"
// ============================================================================
//! Structural equality between two nodes, each in its own arena (or both in the same one).
//!
//! The rules are the owned types' derived `PartialEq`: the same variant, equal fields, equal
//! children, equal text, equal positions. Floats compare as IEEE numbers, so a tree holding a
//! `NaN` is not equal to itself, exactly as with the owned types.
//!
//! Each function matches every variant of the left node with no wildcard arm and then asks
//! whether the right node is the same variant with equal contents, so adding a variant fails to
//! compile until it is compared.

use super::nodes::{ExprNode, ObjectPropNode, StmtNode, SwitchCaseNode, ValueNode};
use super::{Arena, ExprId, ExprList, Str, StmtId, StmtList, StrList, ValueId, ValueList};

fn list_eq<T: Copy>(l: &[T], r: &[T], mut eq: impl FnMut(T, T) -> bool) -> bool {
    l.len() == r.len() && l.iter().zip(r).all(|(x, y)| eq(*x, *y))
}

fn text_eq(a: &Arena, x: Str, b: &Arena, y: Str) -> bool {
    a.text(x) == b.text(y)
}

fn opt_text_eq(a: &Arena, x: Option<Str>, b: &Arena, y: Option<Str>) -> bool {
    match (x, y) {
        (Some(x), Some(y)) => text_eq(a, x, b, y),
        (None, None) => true,
        _ => false,
    }
}

fn strs_eq(a: &Arena, x: StrList, b: &Arena, y: StrList) -> bool {
    list_eq(a.str_list(x), b.str_list(y), |s, t| text_eq(a, s, b, t))
}

fn values_eq(a: &Arena, x: ValueList, b: &Arena, y: ValueList) -> bool {
    list_eq(a.value_list(x), b.value_list(y), |s, t| value_eq(a, s, b, t))
}

fn exprs_eq(a: &Arena, x: ExprList, b: &Arena, y: ExprList) -> bool {
    list_eq(a.expr_list(x), b.expr_list(y), |s, t| expr_eq(a, s, b, t))
}

fn opt_exprs_eq(a: &Arena, x: Option<ExprList>, b: &Arena, y: Option<ExprList>) -> bool {
    match (x, y) {
        (Some(x), Some(y)) => exprs_eq(a, x, b, y),
        (None, None) => true,
        _ => false,
    }
}

fn stmts_eq(a: &Arena, x: StmtList, b: &Arena, y: StmtList) -> bool {
    list_eq(a.stmt_list(x), b.stmt_list(y), |s, t| stmt_eq(a, s, b, t))
}

fn opt_stmts_eq(a: &Arena, x: Option<StmtList>, b: &Arena, y: Option<StmtList>) -> bool {
    match (x, y) {
        (Some(x), Some(y)) => stmts_eq(a, x, b, y),
        (None, None) => true,
        _ => false,
    }
}

fn prop_eq(a: &Arena, x: &ObjectPropNode, b: &Arena, y: &ObjectPropNode) -> bool {
    text_eq(a, x.key, b, y.key) && x.position == y.position && value_eq(a, x.value, b, y.value)
}

fn case_eq(a: &Arena, x: &SwitchCaseNode, b: &Arena, y: &SwitchCaseNode) -> bool {
    x.position == y.position
        && value_eq(a, x.case_value, b, y.case_value)
        && stmts_eq(a, x.statements, b, y.statements)
}

pub(super) fn value_eq(a: &Arena, x: ValueId, b: &Arena, y: ValueId) -> bool {
    use ValueNode as N;
    let other = *b.value(y);
    match *a.value(x) {
        N::Integer { value, position } => {
            matches!(other, N::Integer { value: v, position: p } if v == value && p == position)
        }
        N::Long { value, position } => {
            matches!(other, N::Long { value: v, position: p } if v == value && p == position)
        }
        N::Float { value, position } => {
            matches!(other, N::Float { value: v, position: p } if v == value && p == position)
        }
        N::Double { value, position } => {
            matches!(other, N::Double { value: v, position: p } if v == value && p == position)
        }
        N::ScientificNotation { value, position } => matches!(
            other,
            N::ScientificNotation { value: v, position: p } if v == value && p == position
        ),
        N::String { value, position } => matches!(
            other,
            N::String { value: v, position: p } if p == position && text_eq(a, value, b, v)
        ),
        N::Boolean { value, position } => {
            matches!(other, N::Boolean { value: v, position: p } if v == value && p == position)
        }
        N::InterpolatedString { template, expressions, position } => matches!(
            other,
            N::InterpolatedString { template: t, expressions: e, position: p }
                if p == position && text_eq(a, template, b, t) && exprs_eq(a, expressions, b, e)
        ),
        N::HexColor { value, position } => matches!(
            other,
            N::HexColor { value: v, position: p } if p == position && text_eq(a, value, b, v)
        ),
        N::Date { value, position } => matches!(
            other,
            N::Date { value: v, position: p } if p == position && text_eq(a, value, b, v)
        ),
        N::Timestamp { value, position } => matches!(
            other,
            N::Timestamp { value: v, position: p } if p == position && text_eq(a, value, b, v)
        ),
        N::Null { position } => matches!(other, N::Null { position: p } if p == position),
        N::Array { values, position } => matches!(
            other,
            N::Array { values: v, position: p } if p == position && values_eq(a, values, b, v)
        ),
        N::NestedArray { values, level, position } => matches!(
            other,
            N::NestedArray { values: v, level: l, position: p }
                if p == position && l == level && values_eq(a, values, b, v)
        ),
        N::Object { properties, position } => matches!(
            other,
            N::Object { properties: q, position: p }
                if p == position
                    && list_eq(a.prop_list(properties), b.prop_list(q), |s, t| prop_eq(a, &s, b, &t))
        ),
        N::PrefixedConstructor { prefix, arguments, position } => matches!(
            other,
            N::PrefixedConstructor { prefix: q, arguments: r, position: p }
                if p == position && text_eq(a, prefix, b, q) && values_eq(a, arguments, b, r)
        ),
        N::EnumValue { enum_name, value, position } => matches!(
            other,
            N::EnumValue { enum_name: n, value: v, position: p }
                if p == position && text_eq(a, enum_name, b, n) && text_eq(a, value, b, v)
        ),
        N::Identifier { value, position } => matches!(
            other,
            N::Identifier { value: v, position: p } if p == position && text_eq(a, value, b, v)
        ),
        N::QuickFuncCall { function_name, arguments, position } => matches!(
            other,
            N::QuickFuncCall { function_name: n, arguments: r, position: p }
                if p == position && text_eq(a, function_name, b, n) && exprs_eq(a, arguments, b, r)
        ),
        N::Expression { expr, position } => matches!(
            other,
            N::Expression { expr: e, position: p } if p == position && expr_eq(a, expr, b, e)
        ),
        N::Range { start, end, position } => matches!(
            other,
            N::Range { start: s, end: e, position: p }
                if p == position && value_eq(a, start, b, s) && value_eq(a, end, b, e)
        ),
        N::Lambda { parameters, body, statements, position } => matches!(
            other,
            N::Lambda { parameters: q, body: d, statements: s, position: p }
                if p == position
                    && strs_eq(a, parameters, b, q)
                    && expr_eq(a, body, b, d)
                    && stmts_eq(a, statements, b, s)
        ),
        N::ParseError { message, position } => matches!(
            other,
            N::ParseError { message: m, position: p } if p == position && text_eq(a, message, b, m)
        ),
        N::Error { message, position } => matches!(
            other,
            N::Error { message: m, position: p } if p == position && text_eq(a, message, b, m)
        ),
        N::Unknown { data, position } => matches!(
            other,
            N::Unknown { data: d, position: p } if p == position && a.unknown(data) == b.unknown(d)
        ),
    }
}

pub(super) fn expr_eq(a: &Arena, x: ExprId, b: &Arena, y: ExprId) -> bool {
    use ExprNode as N;
    let other = *b.expr(y);
    match *a.expr(x) {
        N::Identifier { name, position } => matches!(
            other,
            N::Identifier { name: n, position: p } if p == position && text_eq(a, name, b, n)
        ),
        N::QualifiedIdentifier { parts, arguments, position } => matches!(
            other,
            N::QualifiedIdentifier { parts: q, arguments: r, position: p }
                if p == position && strs_eq(a, parts, b, q) && opt_exprs_eq(a, arguments, b, r)
        ),
        N::FunctionCall { name, arguments, position } => matches!(
            other,
            N::FunctionCall { name: n, arguments: r, position: p }
                if p == position && text_eq(a, name, b, n) && exprs_eq(a, arguments, b, r)
        ),
        N::QuickFuncCall { name, arguments, position } => matches!(
            other,
            N::QuickFuncCall { name: n, arguments: r, position: p }
                if p == position && text_eq(a, name, b, n) && exprs_eq(a, arguments, b, r)
        ),
        N::DixFunctionCall { function_name, arguments, position } => matches!(
            other,
            N::DixFunctionCall { function_name: n, arguments: r, position: p }
                if p == position && text_eq(a, function_name, b, n) && exprs_eq(a, arguments, b, r)
        ),
        N::StaticMethodCall { object_name, method_name, arguments, position } => matches!(
            other,
            N::StaticMethodCall { object_name: o, method_name: m, arguments: r, position: p }
                if p == position
                    && text_eq(a, object_name, b, o)
                    && text_eq(a, method_name, b, m)
                    && exprs_eq(a, arguments, b, r)
        ),
        N::InstanceMethodCall { instance, method_name, arguments, position } => matches!(
            other,
            N::InstanceMethodCall { instance: i, method_name: m, arguments: r, position: p }
                if p == position
                    && expr_eq(a, instance, b, i)
                    && text_eq(a, method_name, b, m)
                    && exprs_eq(a, arguments, b, r)
        ),
        N::BuiltinFunction { target, method, arguments, position } => matches!(
            other,
            N::BuiltinFunction { target: t, method: m, arguments: r, position: p }
                if p == position
                    && expr_eq(a, target, b, t)
                    && text_eq(a, method, b, m)
                    && opt_exprs_eq(a, arguments, b, r)
        ),
        N::StaticFunction { class_name, method, arguments, position } => matches!(
            other,
            N::StaticFunction { class_name: c, method: m, arguments: r, position: p }
                if p == position
                    && text_eq(a, class_name, b, c)
                    && text_eq(a, method, b, m)
                    && exprs_eq(a, arguments, b, r)
        ),
        N::ImportedFunctionCall { namespace_name, function_name, arguments, position } => matches!(
            other,
            N::ImportedFunctionCall { namespace_name: s, function_name: n, arguments: r, position: p }
                if p == position
                    && text_eq(a, namespace_name, b, s)
                    && text_eq(a, function_name, b, n)
                    && exprs_eq(a, arguments, b, r)
        ),
        N::ArithmeticOp { left, operator, right, position } => matches!(
            other,
            N::ArithmeticOp { left: l, operator: o, right: r, position: p }
                if p == position
                    && expr_eq(a, left, b, l)
                    && text_eq(a, operator, b, o)
                    && expr_eq(a, right, b, r)
        ),
        N::BitwiseOp { left, operator, right, position } => matches!(
            other,
            N::BitwiseOp { left: l, operator: o, right: r, position: p }
                if p == position
                    && expr_eq(a, left, b, l)
                    && text_eq(a, operator, b, o)
                    && expr_eq(a, right, b, r)
        ),
        N::ComparisonOp { left, operator, right, position } => matches!(
            other,
            N::ComparisonOp { left: l, operator: o, right: r, position: p }
                if p == position
                    && expr_eq(a, left, b, l)
                    && text_eq(a, operator, b, o)
                    && expr_eq(a, right, b, r)
        ),
        N::LogicalOp { left, operator, right, position } => matches!(
            other,
            N::LogicalOp { left: l, operator: o, right: r, position: p }
                if p == position
                    && expr_eq(a, left, b, l)
                    && text_eq(a, operator, b, o)
                    && expr_eq(a, right, b, r)
        ),
        N::UnaryOp { operator, operand, position } => matches!(
            other,
            N::UnaryOp { operator: o, operand: d, position: p }
                if p == position && text_eq(a, operator, b, o) && expr_eq(a, operand, b, d)
        ),
        N::ConfigAccess { key, position } => matches!(
            other,
            N::ConfigAccess { key: k, position: p } if p == position && text_eq(a, key, b, k)
        ),
        N::EnumAccess { namespace_name, enum_name, value, position } => matches!(
            other,
            N::EnumAccess { namespace_name: s, enum_name: n, value: v, position: p }
                if p == position
                    && opt_text_eq(a, namespace_name, b, s)
                    && text_eq(a, enum_name, b, n)
                    && text_eq(a, value, b, v)
        ),
        N::ObjectAccess { path, position } => matches!(
            other,
            N::ObjectAccess { path: q, position: p } if p == position && strs_eq(a, path, b, q)
        ),
        N::PropertyAccess { object, property, position } => matches!(
            other,
            N::PropertyAccess { object: o, property: q, position: p }
                if p == position && expr_eq(a, object, b, o) && text_eq(a, property, b, q)
        ),
        N::IndexAccess { object, index, position } => matches!(
            other,
            N::IndexAccess { object: o, index: i, position: p }
                if p == position && expr_eq(a, object, b, o) && expr_eq(a, index, b, i)
        ),
        N::Value { value, position } => matches!(
            other,
            N::Value { value: v, position: p } if p == position && value_eq(a, value, b, v)
        ),
        N::Parenthesized { expression, position } => matches!(
            other,
            N::Parenthesized { expression: e, position: p }
                if p == position && expr_eq(a, expression, b, e)
        ),
        N::Conditional { condition, true_value, false_value, position } => matches!(
            other,
            N::Conditional { condition: c, true_value: t, false_value: f, position: p }
                if p == position
                    && expr_eq(a, condition, b, c)
                    && expr_eq(a, true_value, b, t)
                    && expr_eq(a, false_value, b, f)
        ),
        N::TypeCast { expression, target_type, position } => matches!(
            other,
            N::TypeCast { expression: e, target_type: t, position: p }
                if p == position && t == target_type && expr_eq(a, expression, b, e)
        ),
    }
}

pub(super) fn stmt_eq(a: &Arena, x: StmtId, b: &Arena, y: StmtId) -> bool {
    use StmtNode as N;
    let other = *b.stmt(y);
    match *a.stmt(x) {
        N::Return { value, position } => matches!(
            other,
            N::Return { value: v, position: p } if p == position && expr_eq(a, value, b, v)
        ),
        N::If { condition, then_branch, else_branch, position } => matches!(
            other,
            N::If { condition: c, then_branch: t, else_branch: e, position: p }
                if p == position
                    && expr_eq(a, condition, b, c)
                    && stmts_eq(a, then_branch, b, t)
                    && opt_stmts_eq(a, else_branch, b, e)
        ),
        N::Switch { expression, cases, default_case, position } => matches!(
            other,
            N::Switch { expression: e, cases: c, default_case: d, position: p }
                if p == position
                    && expr_eq(a, expression, b, e)
                    && list_eq(a.case_list(cases), b.case_list(c), |s, t| case_eq(a, &s, b, &t))
                    && match (default_case, d) {
                        (Some(s), Some(t)) => case_eq(a, a.case(s), b, b.case(t)),
                        (None, None) => true,
                        _ => false,
                    }
        ),
        N::Assignment { variable, value, position } => matches!(
            other,
            N::Assignment { variable: n, value: v, position: p }
                if p == position && text_eq(a, variable, b, n) && expr_eq(a, value, b, v)
        ),
        N::ArithmeticAssignment { variable, operator, value, position } => matches!(
            other,
            N::ArithmeticAssignment { variable: n, operator: o, value: v, position: p }
                if p == position
                    && text_eq(a, variable, b, n)
                    && text_eq(a, operator, b, o)
                    && expr_eq(a, value, b, v)
        ),
        N::ObjectCreation { variable, object, position } => matches!(
            other,
            N::ObjectCreation { variable: n, object: o, position: p }
                if p == position && text_eq(a, variable, b, n) && value_eq(a, object, b, o)
        ),
        N::Log { value, position } => matches!(
            other,
            N::Log { value: v, position: p } if p == position && expr_eq(a, value, b, v)
        ),
        N::ExpressionStatement { expression, position } => matches!(
            other,
            N::ExpressionStatement { expression: e, position: p }
                if p == position && expr_eq(a, expression, b, e)
        ),
        N::VariableDeclaration {
            declaration_type,
            is_mutable,
            variable_name,
            data_type,
            value,
            position,
        } => matches!(
            other,
            N::VariableDeclaration {
                declaration_type: d,
                is_mutable: m,
                variable_name: n,
                data_type: t,
                value: v,
                position: p,
            } if p == position
                && d == declaration_type
                && m == is_mutable
                && t == data_type
                && text_eq(a, variable_name, b, n)
                && expr_eq(a, value, b, v)
        ),
    }
}
