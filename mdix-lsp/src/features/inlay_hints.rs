// mdix-lsp/src/features/inlay_hints.rs

use std::panic;
use std::collections::HashMap;

use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position};
use dixscript::Compiler::AST::{DataEntry, DataType, Value, Expression, QuickFuncStatement};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>) -> Option<Vec<InlayHint>> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc)));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload.downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("inlay_hints panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>) -> Option<Vec<InlayHint>> {
    let doc = doc?;
    let ast = doc.ast.as_ref()?;

    // QuickFunc name → declared return type, for call-site inference.
    let qf_return_types: HashMap<String, DataType> = ast
        .quick_functions
        .as_ref()
        .map(|qf| {
            qf.functions.iter()
                .filter_map(|f| f.return_type.map(|rt| (f.name.clone(), rt)))
                .collect()
        })
        .unwrap_or_default();

    let mut hints = Vec::new();

    // ── @DATA section ────────────────────────────────────────────────────────
    if let Some(data) = &ast.data {
        let type_index = doc.semantic_result.as_ref()
            .and_then(|sr| sr.type_index.as_ref());
        let no_params: HashMap<String, Option<DataType>> = HashMap::new();

        for entry in &data.entries {
            match entry {
                DataEntry::SimpleProperty { ref name, ref data_type, ref value, ref position } => {
                    if data_type.is_some() { continue; }
                    let type_label = type_index
                        .as_ref()
                        .and_then(|idx| idx.get(name.as_str()))
                        .map(|dt| fmt_type(*dt))
                        .or_else(|| infer_type_label(value, &qf_return_types, &no_params))
                        .unwrap_or_else(|| "<auto>".to_string());
                    let line = position.line.saturating_sub(1) as u32;
                    let col  = (position.column.saturating_sub(1) + name.len()) as u32;
                    hints.push(make_hint(line, col, type_label));
                }

                DataEntry::TableProperty { ref properties, .. } => {
                    for prop in properties {
                        if prop.data_type.is_some() { continue; }
                        let type_label =
                            infer_type_label(&prop.value, &qf_return_types, &no_params)
                            .unwrap_or_else(|| "<auto>".to_string());
                        let line = prop.position.line.saturating_sub(1) as u32;
                        let col  = (prop.position.column.saturating_sub(1) + prop.name.len()) as u32;
                        hints.push(make_hint(line, col, type_label));
                    }
                }

                DataEntry::GroupArray { ref path, ref items, ref position } => {
                    if items.is_empty() { continue; }
                    let elem_type = items.first()
                        .and_then(|v| infer_type_label(v, &qf_return_types, &no_params))
                        .map(|t| t.trim_start_matches('<').trim_end_matches('>').to_string())
                        .unwrap_or_else(|| "any".to_string());
                    let path_str = path.segments.join(".");
                    let line = position.line.saturating_sub(1) as u32;
                    let col  = (position.column.saturating_sub(1) + path_str.len()) as u32;
                    hints.push(make_hint(line, col, format!("<{}>[{}]", elem_type, items.len())));
                }

                DataEntry::ObjectProperty { .. } => {}
            }
        }
    }

    // ── @QUICKFUNCS section ──────────────────────────────────────────────────
    if let Some(qf) = &ast.quick_functions {
        for func in &qf.functions {
            let param_types: HashMap<String, Option<DataType>> = func.parameters.iter()
                .map(|p| (p.name.clone(), p.data_type))
                .collect();

            // Untyped parameter hints.
            for param in &func.parameters {
                if param.data_type.is_some() { continue; }
                let line = param.position.line.saturating_sub(1) as u32;
                let col  = (param.position.column.saturating_sub(1) + param.name.len()) as u32;
                hints.push(make_hint(line, col, "<any>".to_string()));
            }

            collect_qf_var_hints(
                &func.body,
                &doc.tokens,
                &qf_return_types,
                &param_types,
                &mut hints,
            );
        }
    }

    if hints.is_empty() { None } else { Some(hints) }
}

// ── Variable declaration hint collector ───────────────────────────────────────

fn collect_qf_var_hints(
    stmts:           &[QuickFuncStatement],
    tokens:          &[Token],
    qf_return_types: &HashMap<String, DataType>,
    param_types:     &HashMap<String, Option<DataType>>,
    hints:           &mut Vec<InlayHint>,
) {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration {
                variable_name, data_type, value, position, ..
            } => {
                if data_type.is_some() { continue; }

                let type_label = infer_type_label_expr(value, qf_return_types, param_types)
                    .unwrap_or_else(|| "<?>".to_string());

                let target_line = position.line;
                let hint_line   = position.line.saturating_sub(1) as u32;

                // Find the exact column of the variable name token on this line.
                let col = tokens.iter()
                    .filter(|t| t.line == target_line)
                    .find(|t| matches!(&t.token_type,
                        TokenType::Identifier(id) if id.as_str() == variable_name.as_str()))
                    .map(|tok| (tok.column.saturating_sub(1) + variable_name.len()) as u32)
                    .unwrap_or_else(|| {
                        (position.column.saturating_sub(1) + 4 + variable_name.len()) as u32
                    });

                hints.push(make_hint(hint_line, col, type_label));
            }

            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                collect_qf_var_hints(then_branch, tokens, qf_return_types, param_types, hints);
                if let Some(else_stmts) = else_branch {
                    collect_qf_var_hints(else_stmts, tokens, qf_return_types, param_types, hints);
                }
            }

            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    collect_qf_var_hints(&case.statements, tokens, qf_return_types, param_types, hints);
                }
                if let Some(dc) = default_case {
                    collect_qf_var_hints(&dc.statements, tokens, qf_return_types, param_types, hints);
                }
            }

            _ => {}
        }
    }
}

// ── Hint constructor ──────────────────────────────────────────────────────────

fn make_hint(line: u32, col: u32, label: String) -> InlayHint {
    InlayHint {
        position:      Position::new(line, col),
        label:         InlayHintLabel::String(label),
        kind:          Some(InlayHintKind::TYPE),
        text_edits:    None,
        tooltip:       None,
        padding_left:  Some(false),
        padding_right: Some(true),
        data:          None,
    }
}

fn fmt_type(dt: DataType) -> String {
    format!("<{}>", dt)
}

// ── Type inference from a Value node ─────────────────────────────────────────

fn infer_type_label(
    value:           &Value,
    qf_return_types: &HashMap<String, DataType>,
    param_types:     &HashMap<String, Option<DataType>>,
) -> Option<String> {
    if let Value::Expression { expr, .. } = value {
        return infer_type_label_expr(expr, qf_return_types, param_types);
    }
    if let Value::QuickFuncCall { function_name, .. } = value {
        return qf_return_types.get(function_name.as_str()).map(|rt| fmt_type(*rt));
    }
    if let Value::Identifier { value: name, .. } = value {
        if let Some(opt_dt) = param_types.get(name.as_str()) {
            return Some(match *opt_dt {
                Some(dt) => fmt_type(dt),
                None     => "<any>".to_string(),
            });
        }
    }
    if matches!(value, Value::Null { .. }) {
        return Some("<null>".to_string());
    }

    let dt = match value {
        Value::Integer { .. }            => DataType::Int,
        Value::Float { .. }              => DataType::Float,
        Value::Double { .. }             => DataType::Double,
        Value::ScientificNotation { .. } => DataType::Double,
        Value::String { .. }             => DataType::String,
        Value::InterpolatedString { .. } => DataType::String,
        Value::Boolean { .. }            => DataType::Bool,
        Value::Array { .. }
        | Value::NestedArray { .. }      => DataType::Array,
        Value::Object { .. }             => DataType::Object,
        Value::HexColor { .. }           => DataType::Hex,
        Value::Date { .. }               => DataType::Date,
        Value::Timestamp { .. }          => DataType::Timestamp,
        Value::EnumValue { .. }          => DataType::Enum,
        Value::PrefixedConstructor { prefix, .. } => match prefix.as_str() {
            "b" => DataType::Blob,
            "t" => DataType::Tuple,
            "r" => DataType::Regex,
            _   => return None,
        },
        _ => return None,
    };

    Some(fmt_type(dt))
}

// ── Type inference from an Expression node ────────────────────────────────────
//
// NOTE: This operates on the PRE-ENHANCEMENT AST (doc.ast).  Before enhancement,
// method calls like `arr.first()` and `DateTime.year(d)` are represented as
// QualifiedIdentifier nodes, not InstanceMethodCall / StaticMethodCall.
// The QualifiedIdentifier arm below is the primary fix for `<?>` hints.

fn infer_type_label_expr(
    expr:            &Expression,
    qf_return_types: &HashMap<String, DataType>,
    param_types:     &HashMap<String, Option<DataType>>,
) -> Option<String> {
    match expr {
        // ── Plain identifier: check param_types ───────────────────────────
        Expression::Identifier { name, .. } => {
            param_types.get(name.as_str()).map(|opt_dt| match *opt_dt {
                Some(dt) => fmt_type(dt),
                None     => "<any>".to_string(),
            })
        }

        // ── Literal or collection value ───────────────────────────────────
        Expression::Value { value, .. } => {
            infer_type_label(value, qf_return_types, param_types)
        }

        // ── QuickFunc call by name ─────────────────────────────────────────
        Expression::QuickFuncCall { name, .. } => {
            qf_return_types.get(name.as_str()).map(|rt| fmt_type(*rt))
        }

        // ── Generic function call (parser emits this for some call forms) ──
        Expression::FunctionCall { name, .. } => {
            qf_return_types.get(name.as_str()).map(|rt| fmt_type(*rt))
        }

        // ── QualifiedIdentifier: the main fix
        //
        // Before enhancement, `arr.first()`, `numbers.sum()`, `host.length()`,
        // and `DateTime.year(d)` all land here as:
        //   QualifiedIdentifier { parts: ["receiver", "method"], arguments: Some([...]) }
        //
        // Resolution priority:
        //   1. PascalCase first part → static method (Math, DateTime, Array, …)
        //   2. First part in param_types → instance method on known receiver type
        //   3. First part in qf_return_types → QuickFunc call
        //
        // Property access (no arguments) gets the same treatment so that
        // `arr.length` (without parens) also resolves correctly.
        Expression::QualifiedIdentifier { parts, arguments, .. } => {
            infer_qualified_id(parts, arguments.as_ref(), qf_return_types, param_types)
        }

        // ── Arithmetic ────────────────────────────────────────────────────
        Expression::ArithmeticOp { left, operator, right, .. } => {
            let lt = infer_type_label_expr(left,  qf_return_types, param_types);
            let rt = infer_type_label_expr(right, qf_return_types, param_types);
            if operator.as_str() == "+" {
                if lt.as_deref() == Some("<string>") || rt.as_deref() == Some("<string>") {
                    return Some("<string>".to_string());
                }
            }
            match (lt.as_deref(), rt.as_deref()) {
                (Some("<double>"), _) | (_, Some("<double>")) => Some("<double>".to_string()),
                (Some("<float>"),  _) | (_, Some("<float>"))  => Some("<float>".to_string()),
                (Some("<int>"),    _) | (_, Some("<int>"))     => Some("<int>".to_string()),
                (Some(l), Some(r)) if l == r => Some(l.to_string()),
                (Some(l), _) => Some(l.to_string()),
                (_, Some(r)) => Some(r.to_string()),
                _ => None,
            }
        }

        // ── Logical / comparison always produce bool ──────────────────────
        Expression::ComparisonOp { .. } | Expression::LogicalOp { .. } => {
            Some("<bool>".to_string())
        }

        // ── Bitwise ops produce int ───────────────────────────────────────
        Expression::BitwiseOp { .. } => Some("<int>".to_string()),

        // ── Unary ─────────────────────────────────────────────────────────
        Expression::UnaryOp { operator, operand, .. } => {
            if operator.as_str() == "!" || operator.as_str() == "not" {
                Some("<bool>".to_string())
            } else {
                infer_type_label_expr(operand, qf_return_types, param_types)
            }
        }

        // ── Ternary ───────────────────────────────────────────────────────
        Expression::Conditional { true_value, false_value, .. } => {
            infer_type_label_expr(true_value, qf_return_types, param_types)
                .or_else(|| infer_type_label_expr(false_value, qf_return_types, param_types))
        }

        // ── Parenthesised ─────────────────────────────────────────────────
        Expression::Parenthesized { expression, .. } => {
            infer_type_label_expr(expression, qf_return_types, param_types)
        }

        // ── Already-resolved post-enhancement nodes ───────────────────────
        // These appear if inlay_hints is ever called on an enhanced AST.
        Expression::StaticMethodCall { object_name, method_name, .. } => {
            infer_static_method_return(object_name, method_name)
        }
        Expression::StaticFunction { class_name, method, .. } => {
            infer_static_method_return(class_name, method)
        }
        Expression::InstanceMethodCall { instance, method_name, .. } => {
            let receiver = infer_type_label_expr(instance, qf_return_types, param_types);
            infer_instance_method_return(receiver.as_deref(), method_name)
        }
        Expression::PropertyAccess { object, property, .. } => {
            let receiver = infer_type_label_expr(object, qf_return_types, param_types);
            infer_instance_method_return(receiver.as_deref(), property)
        }

        _ => None,
    }
}

// ── QualifiedIdentifier resolution ───────────────────────────────────────────

/// Infer the result type of a `QualifiedIdentifier` node.
///
/// Handles both call forms (`arguments = Some(...)`) and property-access
/// forms (`arguments = None`).
fn infer_qualified_id(
    parts:           &[String],
    arguments:       Option<&Vec<Expression>>,
    qf_return_types: &HashMap<String, DataType>,
    param_types:     &HashMap<String, Option<DataType>>,
) -> Option<String> {
    if parts.len() < 2 { return None; }

    let receiver = &parts[0];
    let member   = &parts[1];
    let is_call  = arguments.is_some();

    // ── 1. Static call / access: PascalCase first part ────────────────────
    // e.g. DateTime.year(d), Math.sqrt(x), Array.range(1,5)
    if receiver.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
        if is_call {
            return infer_static_method_return(receiver, member);
        }
        // Property-style static access (e.g. Math.pi without parens):
        return infer_static_method_return(receiver, member);
    }

    // ── 2. Instance call / access: receiver is a known parameter ─────────
    // e.g. arr.first(), numbers.sum(), host.length(), arr.length
    if let Some(opt_dt) = param_types.get(receiver.as_str()) {
        let receiver_hint: Option<String> = match *opt_dt {
            Some(dt) => Some(fmt_type(dt)),
            None     => None,
        };
        let result = infer_instance_method_return(receiver_hint.as_deref(), member);
        if result.is_some() {
            return result;
        }
        // Fall through: unknown method on known type — return receiver type
        // for property-access forms (e.g. obj.someField).
        if !is_call {
            return receiver_hint;
        }
    }

    // ── 3. QuickFunc call whose result is chained ─────────────────────────
    // e.g. chainedCalculation(a, b).toString() — unlikely but defensively handled.
    if is_call {
        if let Some(rt) = qf_return_types.get(receiver.as_str()) {
            let recv_hint = fmt_type(*rt);
            return infer_instance_method_return(Some(recv_hint.as_str()), member);
        }
    }

    // ── 4. Three-part: namespace.Enum.VALUE or ns.func() ─────────────────
    if parts.len() == 3 {
        let obj = &parts[0];
        let mth = &parts[1];
        if obj.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            return infer_static_method_return(obj, mth);
        }
    }

    None
}

// ── Built-in instance method return types ─────────────────────────────────────

fn infer_instance_method_return(
    receiver_hint: Option<&str>,
    method_name:   &str,
) -> Option<String> {
    match (receiver_hint, method_name) {
        // ── Array ─────────────────────────────────────────────────────────
        (Some("<array>"), "sum")
        | (Some("<array>"), "average")
        | (Some("<array>"), "min")
        | (Some("<array>"), "max")         => Some("<double>".to_string()),

        (Some("<array>"), "length")
        | (Some("<array>"), "indexOf")
        | (Some("<array>"), "lastIndexOf") => Some("<int>".to_string()),

        (Some("<array>"), "isEmpty")
        | (Some("<array>"), "contains")    => Some("<bool>".to_string()),

        (Some("<array>"), "join")          => Some("<string>".to_string()),

        (Some("<array>"), "first")
        | (Some("<array>"), "last")
        | (Some("<array>"), "get")         => Some("<any>".to_string()),

        (Some("<array>"), "sort")
        | (Some("<array>"), "reverse")
        | (Some("<array>"), "push")
        | (Some("<array>"), "pop")
        | (Some("<array>"), "unique")
        | (Some("<array>"), "flatten")     => Some("<array>".to_string()),

        // ── String ────────────────────────────────────────────────────────
        (Some("<string>"), "length")       => Some("<int>".to_string()),

        (Some("<string>"), "isEmpty")
        | (Some("<string>"), "contains")
        | (Some("<string>"), "startsWith")
        | (Some("<string>"), "endsWith")   => Some("<bool>".to_string()),

        (Some("<string>"), "toUpper")
        | (Some("<string>"), "toLower")
        | (Some("<string>"), "trim")
        | (Some("<string>"), "replace")
        | (Some("<string>"), "substring")  => Some("<string>".to_string()),

        (Some("<string>"), "split")        => Some("<array>".to_string()),

        // ── Object ────────────────────────────────────────────────────────
        (Some("<object>"), "keys")
        | (Some("<object>"), "values")     => Some("<array>".to_string()),
        (Some("<object>"), "has")          => Some("<bool>".to_string()),
        (Some("<object>"), "get")          => Some("<any>".to_string()),

        // ── Numeric ───────────────────────────────────────────────────────
        (Some("<int>"), "toString")
        | (Some("<float>"), "toString")
        | (Some("<double>"), "toString")   => Some("<string>".to_string()),

        (Some("<int>"), "toFloat")         => Some("<float>".to_string()),
        (Some("<int>"), "toDouble")        => Some("<double>".to_string()),
        (Some("<float>"), "toDouble")      => Some("<double>".to_string()),
        (Some("<float>"), "toInt")
        | (Some("<double>"), "toInt")      => Some("<int>".to_string()),

        _ => None,
    }
}

// ── Static method return types ────────────────────────────────────────────────

fn infer_static_method_return(class: &str, method: &str) -> Option<String> {
    let dt = match (class, method) {
        // Math
        ("Math", "floor") | ("Math", "ceil") | ("Math", "round")
        | ("Math", "sign") | ("Math", "truncate")                => DataType::Int,
        ("Math", "abs") | ("Math", "sqrt") | ("Math", "pow")
        | ("Math", "min") | ("Math", "max") | ("Math", "clamp")
        | ("Math", "sin") | ("Math", "cos") | ("Math", "tan")
        | ("Math", "log") | ("Math", "log10") | ("Math", "exp")
        | ("Math", "pi") | ("Math", "e")
        | ("Math", "radians") | ("Math", "degrees")
        | ("Math", "remainder")                                   => DataType::Double,

        // DateTime
        ("DateTime", "year") | ("DateTime", "month")
        | ("DateTime", "day") | ("DateTime", "hour")
        | ("DateTime", "minute") | ("DateTime", "second")
        | ("DateTime", "millisecond") | ("DateTime", "dayOfWeek")
        | ("DateTime", "dayOfYear") | ("DateTime", "daysInMonth")
        | ("DateTime", "compare")                                 => DataType::Int,
        ("DateTime", "isLeapYear")                               => DataType::Bool,
        ("DateTime", "format")                                   => DataType::String,
        ("DateTime", "now") | ("DateTime", "utcNow")
        | ("DateTime", "parse") | ("DateTime", "parseExact")
        | ("DateTime", "createTime") | ("DateTime", "fromUnixTime")
        | ("DateTime", "addHours") | ("DateTime", "addMinutes")
        | ("DateTime", "addSeconds")                             => DataType::Timestamp,
        ("DateTime", "today") | ("DateTime", "create")
        | ("DateTime", "addDays") | ("DateTime", "addMonths")
        | ("DateTime", "addYears")                               => DataType::Date,
        ("DateTime", "subtract") | ("DateTime", "toUnixTime")   => DataType::Double,

        // Array
        ("Array", "sum") | ("Array", "average")
        | ("Array", "min") | ("Array", "max")                   => DataType::Double,
        ("Array", "contains")                                    => DataType::Bool,
        ("Array", "indexOf") | ("Array", "lastIndexOf")          => DataType::Int,
        ("Array", _)                                             => DataType::Array,

        // Random
        ("Random", "range")                                      => DataType::Int,
        ("Random", "float") | ("Random", "floatRange")           => DataType::Float,
        ("Random", "double") | ("Random", "doubleRange")         => DataType::Double,
        ("Random", "boolean")                                    => DataType::Bool,
        ("Random", "alphanumeric") | ("Random", "string")        => DataType::String,
        ("Random", _)                                            => DataType::Array,

        // Guid
        ("Guid", "validate")                                     => DataType::Bool,
        ("Guid", "new") | ("Guid", "parse") | ("Guid", "tryParse")
        | ("Guid", "format") | ("Guid", "empty")                => DataType::String,
        ("Guid", "toBytes") | ("Guid", "fromBytes")              => DataType::Array,

        // IpAddress
        ("IpAddress", "validate") | ("IpAddress", "isV4")
        | ("IpAddress", "isV6") | ("IpAddress", "isPrivate")
        | ("IpAddress", "isLoopback") | ("IpAddress", "isPublic")
        | ("IpAddress", "inRange")                               => DataType::Bool,
        ("IpAddress", "toBytes")                                 => DataType::Array,
        ("IpAddress", _)                                         => DataType::String,

        // Enum
        ("Enum", "getValues") | ("Enum", "list") | ("Enum", "toArray") => DataType::Array,
        ("Enum", "getName") | ("Enum", "random")                 => DataType::String,
        ("Enum", "getValue") | ("Enum", "count")
        | ("Enum", "min") | ("Enum", "max")                     => DataType::Int,
        ("Enum", "exists") | ("Enum", "hasValue")
        | ("Enum", "contains")                                   => DataType::Bool,

        // Dix
        ("Dix", "Format") | ("Dix", "Join")                     => DataType::String,

        _ => return None,
    };
    Some(fmt_type(dt))
}
