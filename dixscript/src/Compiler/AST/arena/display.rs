// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena/display.rs"
// ============================================================================
//! `Display` for the arena handles, written to produce exactly the text the owned
//! `Value`, `Expression` and `QuickFuncStatement` produce. The wording is copied from
//! `values.rs`, `expressions.rs` and `statements.rs`; `tests/arena_roundtrip.rs` compares the
//! two outputs on every sample and on the whole corpus, so a drift in either shows up there.
//!
//! Every `match` lists every variant with no wildcard arm.

use super::nodes::{ExprNode, StmtNode, SwitchCaseNode, ValueNode};
use super::view::{ExprRef, StmtRef, ValueRef};
use super::{Arena, ExprId, StmtId, StrList, ValueId};
use std::fmt::{self, Display, Formatter, Write};

impl Display for ValueRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt_value(self.arena(), self.id(), f)
    }
}

impl Display for ExprRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt_expr(self.arena(), self.id(), f)
    }
}

impl Display for StmtRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt_stmt(self.arena(), self.id(), f)
    }
}

/// Writes `items` separated by `", "`, calling `each` for every one.
fn comma<T: Copy>(
    f: &mut Formatter<'_>,
    items: &[T],
    mut each: impl FnMut(&mut Formatter<'_>, T) -> fmt::Result,
) -> fmt::Result {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        each(f, *item)?;
    }
    Ok(())
}

fn exprs(a: &Arena, f: &mut Formatter<'_>, list: super::ExprList) -> fmt::Result {
    comma(f, a.expr_list(list), |f, id| fmt_expr(a, id, f))
}

fn values(a: &Arena, f: &mut Formatter<'_>, list: super::ValueList) -> fmt::Result {
    comma(f, a.value_list(list), |f, id| fmt_value(a, id, f))
}

/// Writes the strings of `list` joined by `sep`.
fn joined(a: &Arena, f: &mut Formatter<'_>, list: StrList, sep: &str) -> fmt::Result {
    for (i, s) in a.str_list(list).iter().enumerate() {
        if i > 0 {
            f.write_str(sep)?;
        }
        f.write_str(a.text(*s))?;
    }
    Ok(())
}

fn fmt_case(a: &Arena, case: &SwitchCaseNode, f: &mut Formatter<'_>) -> fmt::Result {
    f.write_str("-> ")?;
    fmt_value(a, case.case_value, f)?;
    f.write_str(" { ")?;
    for id in a.stmt_list(case.statements) {
        fmt_stmt(a, *id, f)?;
        f.write_str("; ")?;
    }
    f.write_char('}')
}

pub(super) fn fmt_value(a: &Arena, id: ValueId, f: &mut Formatter<'_>) -> fmt::Result {
    match *a.value(id) {
        ValueNode::Integer { value, .. } => write!(f, "{}", value),
        ValueNode::Long { value, .. } => write!(f, "{}L", value),
        ValueNode::Float { value, .. } => write!(f, "{}f", value),
        ValueNode::Double { value, .. } => {
            if value.is_finite() && value.fract() == 0.0 {
                write!(f, "{:.1}", value)
            } else {
                write!(f, "{}", value)
            }
        }
        ValueNode::ScientificNotation { value, .. } => write!(f, "{:e}", value),
        ValueNode::String { value, .. } => write!(f, "\"{}\"", a.text(value)),
        ValueNode::Boolean { value, .. } => write!(f, "{}", if value { "true" } else { "false" }),
        ValueNode::InterpolatedString { template, .. } => write!(f, "$\"{}\"", a.text(template)),
        ValueNode::HexColor { value, .. }
        | ValueNode::Date { value, .. }
        | ValueNode::Timestamp { value, .. } => f.write_str(a.text(value)),
        ValueNode::Null { .. } => f.write_str("null"),
        ValueNode::Array { values: list, .. } | ValueNode::NestedArray { values: list, .. } => {
            f.write_char('[')?;
            values(a, f, list)?;
            f.write_char(']')
        }
        ValueNode::Object { properties, .. } => {
            f.write_char('{')?;
            comma(f, a.prop_list(properties), |f, prop| {
                write!(f, "{} = ", a.text(prop.key))?;
                fmt_value(a, prop.value, f)
            })?;
            f.write_char('}')
        }
        ValueNode::PrefixedConstructor { prefix, arguments, .. } => {
            write!(f, "{}:(", a.text(prefix))?;
            values(a, f, arguments)?;
            f.write_char(')')
        }
        ValueNode::EnumValue { enum_name, value, .. } => {
            write!(f, "{}.{}", a.text(enum_name), a.text(value))
        }
        ValueNode::Identifier { value, .. } => f.write_str(a.text(value)),
        ValueNode::QuickFuncCall { function_name, arguments, .. } => {
            write!(f, "{}(", a.text(function_name))?;
            exprs(a, f, arguments)?;
            f.write_char(')')
        }
        ValueNode::Expression { expr, .. } => {
            f.write_str("<ToEvaluate: ")?;
            fmt_expr(a, expr, f)?;
            f.write_char('>')
        }
        ValueNode::Range { start, end, .. } => {
            fmt_value(a, start, f)?;
            f.write_str("..")?;
            fmt_value(a, end, f)
        }
        ValueNode::Lambda { parameters, body, .. } => {
            f.write_char('(')?;
            joined(a, f, parameters, ", ")?;
            f.write_str(") => ")?;
            fmt_expr(a, body, f)
        }
        ValueNode::ParseError { message, .. } => write!(f, "ParseError: {}", a.text(message)),
        ValueNode::Error { message, .. } => write!(f, "Error: {}", a.text(message)),
        ValueNode::Unknown { data, .. } => {
            let d = a.unknown(data);
            write!(
                f,
                "Unknown {}: {} (Compatibility: {:?})",
                d.element_type, d.element_name, d.compatibility_result
            )
        }
    }
}

pub(super) fn fmt_expr(a: &Arena, id: ExprId, f: &mut Formatter<'_>) -> fmt::Result {
    // `name(args)` with a literal prefix already written by the caller.
    fn call(a: &Arena, f: &mut Formatter<'_>, args: super::ExprList) -> fmt::Result {
        f.write_char('(')?;
        exprs(a, f, args)?;
        f.write_char(')')
    }

    fn binary(
        a: &Arena,
        f: &mut Formatter<'_>,
        left: ExprId,
        operator: super::Str,
        right: ExprId,
    ) -> fmt::Result {
        f.write_char('(')?;
        fmt_expr(a, left, f)?;
        write!(f, " {} ", a.text(operator))?;
        fmt_expr(a, right, f)?;
        f.write_char(')')
    }

    match *a.expr(id) {
        ExprNode::Identifier { name, .. } => f.write_str(a.text(name)),
        ExprNode::QualifiedIdentifier { parts, arguments, .. } => {
            joined(a, f, parts, ".")?;
            if let Some(args) = arguments {
                call(a, f, args)?;
            }
            Ok(())
        }
        ExprNode::FunctionCall { name, arguments, .. }
        | ExprNode::QuickFuncCall { name, arguments, .. } => {
            f.write_str(a.text(name))?;
            call(a, f, arguments)
        }
        ExprNode::DixFunctionCall { function_name, arguments, .. } => {
            write!(f, "Dix.{}", a.text(function_name))?;
            call(a, f, arguments)
        }
        ExprNode::StaticMethodCall { object_name, method_name, arguments, .. } => {
            write!(f, "{}.{}", a.text(object_name), a.text(method_name))?;
            call(a, f, arguments)
        }
        ExprNode::InstanceMethodCall { instance, method_name, arguments, .. } => {
            fmt_expr(a, instance, f)?;
            write!(f, ".{}", a.text(method_name))?;
            call(a, f, arguments)
        }
        ExprNode::BuiltinFunction { target, method, arguments, .. } => {
            fmt_expr(a, target, f)?;
            write!(f, ".{}", a.text(method))?;
            if let Some(args) = arguments {
                call(a, f, args)?;
            }
            Ok(())
        }
        ExprNode::StaticFunction { class_name, method, arguments, .. } => {
            write!(f, "{}.{}", a.text(class_name), a.text(method))?;
            call(a, f, arguments)
        }
        ExprNode::ImportedFunctionCall { namespace_name, function_name, arguments, .. } => {
            write!(f, "{}.{}", a.text(namespace_name), a.text(function_name))?;
            call(a, f, arguments)
        }
        ExprNode::ArithmeticOp { left, operator, right, .. }
        | ExprNode::BitwiseOp { left, operator, right, .. }
        | ExprNode::ComparisonOp { left, operator, right, .. }
        | ExprNode::LogicalOp { left, operator, right, .. } => binary(a, f, left, operator, right),
        ExprNode::UnaryOp { operator, operand, .. } => {
            f.write_str(a.text(operator))?;
            fmt_expr(a, operand, f)
        }
        ExprNode::ConfigAccess { key, .. } => write!(f, "config.{}", a.text(key)),
        ExprNode::EnumAccess { namespace_name, enum_name, value, .. } => {
            if let Some(ns) = namespace_name {
                write!(f, "{}.{}.{}", a.text(ns), a.text(enum_name), a.text(value))
            } else {
                write!(f, "{}.{}", a.text(enum_name), a.text(value))
            }
        }
        ExprNode::ObjectAccess { path, .. } => joined(a, f, path, "."),
        ExprNode::PropertyAccess { object, property, .. } => {
            fmt_expr(a, object, f)?;
            write!(f, ".{}", a.text(property))
        }
        ExprNode::IndexAccess { object, index, .. } => {
            fmt_expr(a, object, f)?;
            f.write_char('[')?;
            fmt_expr(a, index, f)?;
            f.write_char(']')
        }
        ExprNode::Value { value, .. } => fmt_value(a, value, f),
        ExprNode::Parenthesized { expression, .. } => {
            f.write_char('(')?;
            fmt_expr(a, expression, f)?;
            f.write_char(')')
        }
        ExprNode::Conditional { condition, true_value, false_value, .. } => {
            fmt_expr(a, condition, f)?;
            f.write_str(" ? ")?;
            fmt_expr(a, true_value, f)?;
            f.write_str(" : ")?;
            fmt_expr(a, false_value, f)
        }
        ExprNode::TypeCast { expression, target_type, .. } => {
            fmt_expr(a, expression, f)?;
            write!(f, " as<{}>", target_type)
        }
    }
}

pub(super) fn fmt_stmt(a: &Arena, id: StmtId, f: &mut Formatter<'_>) -> fmt::Result {
    fn block(a: &Arena, f: &mut Formatter<'_>, list: super::StmtList) -> fmt::Result {
        for id in a.stmt_list(list) {
            fmt_stmt(a, *id, f)?;
            f.write_str("; ")?;
        }
        Ok(())
    }

    match *a.stmt(id) {
        StmtNode::Return { value, .. } => {
            f.write_str("return ")?;
            fmt_expr(a, value, f)
        }
        StmtNode::If { condition, then_branch, else_branch, .. } => {
            f.write_str("if: ")?;
            fmt_expr(a, condition, f)?;
            f.write_str(" { ")?;
            block(a, f, then_branch)?;
            f.write_char('}')?;
            if let Some(else_stmts) = else_branch {
                f.write_str(" else { ")?;
                block(a, f, else_stmts)?;
                f.write_char('}')?;
            }
            Ok(())
        }
        StmtNode::Switch { expression, cases, default_case, .. } => {
            f.write_str("chk: ")?;
            fmt_expr(a, expression, f)?;
            f.write_str(" { ")?;
            for case in a.case_list(cases) {
                fmt_case(a, case, f)?;
                f.write_char(' ')?;
            }
            if let Some(default) = default_case {
                fmt_case(a, a.case(default), f)?;
                f.write_char(' ')?;
            }
            f.write_char('}')
        }
        StmtNode::Assignment { variable, value, .. } => {
            write!(f, "{} = ", a.text(variable))?;
            fmt_expr(a, value, f)
        }
        StmtNode::ArithmeticAssignment { variable, operator, value, .. } => {
            write!(f, "{} {} ", a.text(variable), a.text(operator))?;
            fmt_expr(a, value, f)
        }
        StmtNode::ObjectCreation { variable, object, .. } => {
            write!(f, "{} = ", a.text(variable))?;
            fmt_value(a, object, f)
        }
        StmtNode::Log { value, .. } => {
            f.write_str("log: ")?;
            fmt_expr(a, value, f)
        }
        StmtNode::ExpressionStatement { expression, .. } => fmt_expr(a, expression, f),
        StmtNode::VariableDeclaration {
            declaration_type,
            is_mutable,
            variable_name,
            data_type,
            value,
            ..
        } => {
            write!(f, "{}", declaration_type)?;
            if is_mutable {
                f.write_str(" mut")?;
            }
            write!(f, " {}", a.text(variable_name))?;
            if let Some(dt) = data_type {
                write!(f, "<{}>", dt)?;
            }
            f.write_str(" = ")?;
            fmt_expr(a, value, f)
        }
    }
}
