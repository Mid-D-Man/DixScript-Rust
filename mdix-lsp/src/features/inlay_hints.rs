// mdix-lsp/src/features/inlay_hints.rs

use std::panic;
use std::collections::HashMap;

use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position};
use dixscript::Compiler::AST::{DataEntry, DataType, Value, Expression, QuickFuncStatement};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Builtins::Core::DixType;
use dixscript::Builtins::Resolver::{instance_method_registry, static_object_registry};
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

    // Initialise registries once — both are OnceLock-backed and idempotent.
    instance_method_registry::initialize();
    static_object_registry::initialize_static_registry();

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
                        .unwrap_or_else(|| "<any>".to_string());
                    let line = position.line.saturating_sub(1) as u32;
                    let col  = (position.column.saturating_sub(1) + name.len()) as u32;
                    hints.push(make_hint(line, col, type_label));
                }

                DataEntry::TableProperty { ref properties, .. } => {
                    for prop in properties {
                        if prop.data_type.is_some() { continue; }
                        let type_label =
                            infer_type_label(&prop.value, &qf_return_types, &no_params)
                            .unwrap_or_else(|| "<any>".to_string());
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
                    .unwrap_or_else(|| "<any>".to_string());

                let target_line = position.line;
                let hint_line   = position.line.saturating_sub(1) as u32;

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

// ── DixType ↔ hint-string conversion (used by registry lookups) ───────────────

fn hint_to_dix_type(hint: &str) -> Option<DixType> {
    match hint {
        "<int>"       => Some(DixType::Int),
        "<float>"     => Some(DixType::Float),
        "<double>"    => Some(DixType::Double),
        "<string>"    => Some(DixType::String),
        "<bool>"      => Some(DixType::Bool),
        "<array>"     => Some(DixType::Array),
        "<tuple>"     => Some(DixType::Tuple),
        "<object>"    => Some(DixType::Object),
        "<hex>"       => Some(DixType::Hex),
        "<blob>"      => Some(DixType::Blob),
        "<regex>"     => Some(DixType::Regex),
        "<date>"      => Some(DixType::Date),
        "<timestamp>" => Some(DixType::Timestamp),
        "<enum>"      => Some(DixType::Enum),
        "<any>"       => Some(DixType::Any),
        _             => None,
    }
}

fn dix_type_to_hint(dix_type: DixType) -> Option<String> {
    match dix_type {
        DixType::Int       => Some("<int>".to_string()),
        DixType::Float     => Some("<float>".to_string()),
        DixType::Double    => Some("<double>".to_string()),
        DixType::String    => Some("<string>".to_string()),
        DixType::Bool      => Some("<bool>".to_string()),
        DixType::Array     => Some("<array>".to_string()),
        DixType::Tuple     => Some("<tuple>".to_string()),
        DixType::Object    => Some("<object>".to_string()),
        DixType::Hex       => Some("<hex>".to_string()),
        DixType::Blob      => Some("<blob>".to_string()),
        DixType::Regex     => Some("<regex>".to_string()),
        DixType::Date      => Some("<date>".to_string()),
        DixType::Timestamp => Some("<timestamp>".to_string()),
        DixType::Enum      => Some("<enum>".to_string()),
        DixType::Any       => Some("<any>".to_string()),
        DixType::Void | DixType::Null => None,
    }
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
// Operates on the PRE-ENHANCEMENT AST.  Before enhancement, method calls
// like `arr.first()` and `DateTime.year(d)` are QualifiedIdentifier nodes.
// BuiltinFunction nodes (`host.length()`) are also handled here.

fn infer_type_label_expr(
    expr:            &Expression,
    qf_return_types: &HashMap<String, DataType>,
    param_types:     &HashMap<String, Option<DataType>>,
) -> Option<String> {
    match expr {
        // ── Plain identifier ──────────────────────────────────────────────
        Expression::Identifier { name, .. } => {
            param_types.get(name.as_str()).map(|opt_dt| match *opt_dt {
                Some(dt) => fmt_type(dt),
                None     => "<any>".to_string(),
            })
        }

        // ── Literal value ─────────────────────────────────────────────────
        Expression::Value { value, .. } => {
            infer_type_label(value, qf_return_types, param_types)
        }

        // ── QuickFunc call by name ─────────────────────────────────────────
        Expression::QuickFuncCall { name, .. } => {
            qf_return_types.get(name.as_str()).map(|rt| fmt_type(*rt))
        }

        // ── Generic function call ──────────────────────────────────────────
        Expression::FunctionCall { name, .. } => {
            qf_return_types.get(name.as_str()).map(|rt| fmt_type(*rt))
        }

        // ── QualifiedIdentifier — the main pre-enhancement dispatch ───────
        //
        // `arr.first()`, `numbers.sum()`, `host.length()`, `DateTime.year(d)`
        // all arrive here before enhancement resolves them to typed call nodes.
        Expression::QualifiedIdentifier { parts, arguments, .. } => {
            infer_qualified_id(parts, arguments.as_ref(), qf_return_types, param_types)
        }

        // ── BuiltinFunction — parser emits this for `.method()` chains ────
        // e.g. `host.length()` → BuiltinFunction { target: Identifier("host"),
        //                                          method: "length", ... }
        Expression::BuiltinFunction { target, method, .. } => {
            let receiver = infer_type_label_expr(target, qf_return_types, param_types);
            infer_instance_method_return(receiver.as_deref(), method)
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

        Expression::ComparisonOp { .. } | Expression::LogicalOp { .. } => {
            Some("<bool>".to_string())
        }

        Expression::BitwiseOp { .. } => Some("<int>".to_string()),

        Expression::UnaryOp { operator, operand, .. } => {
            if operator.as_str() == "!" || operator.as_str() == "not" {
                Some("<bool>".to_string())
            } else {
                infer_type_label_expr(operand, qf_return_types, param_types)
            }
        }

        Expression::Conditional { true_value, false_value, .. } => {
            infer_type_label_expr(true_value, qf_return_types, param_types)
                .or_else(|| infer_type_label_expr(false_value, qf_return_types, param_types))
        }

        Expression::Parenthesized { expression, .. } => {
            infer_type_label_expr(expression, qf_return_types, param_types)
        }

        // ── Already-resolved post-enhancement nodes ───────────────────────
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

    // 1. PascalCase first part → static object (Math, DateTime, Array, …)
    if receiver.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
        return infer_static_method_return(receiver, member);
    }

    // 2. First part in param_types → instance method / property on known type
    if let Some(opt_dt) = param_types.get(receiver.as_str()) {
        let receiver_hint: Option<String> = match *opt_dt {
            Some(dt) => Some(fmt_type(dt)),
            None     => None,
        };
        if let Some(result) = infer_instance_method_return(receiver_hint.as_deref(), member) {
            return Some(result);
        }
        // Property access on known type where method not found — return receiver type.
        if !is_call {
            return receiver_hint;
        }
    }

    // 3. First part is a QuickFunc name whose return type is the receiver.
    if is_call {
        if let Some(rt) = qf_return_types.get(receiver.as_str()) {
            let recv_hint = fmt_type(*rt);
            return infer_instance_method_return(Some(recv_hint.as_str()), member);
        }
    }

    // 4. Three-part qualified identifiers (namespace.Object.member).
    if parts.len() >= 3 {
        let obj = &parts[0];
        let mth = &parts[1];
        if obj.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            return infer_static_method_return(obj, mth);
        }
    }

    None
}

// ── Registry-backed type lookup ───────────────────────────────────────────────

/// Look up the return type of an instance method call using the actual
/// `instance_method_registry`.  Handles ALL registered types (String, Int,
/// Float, Double, Array, Tuple, Blob, Regex, Object, universal methods, …).
fn infer_instance_method_return(receiver_hint: Option<&str>, method_name: &str) -> Option<String> {
    let dix_type = hint_to_dix_type(receiver_hint?)?;
    let method   = instance_method_registry::get_instance_method(dix_type, method_name)?;
    dix_type_to_hint(method.return_type())
}

/// Look up the return type of a static method call using the actual
/// `static_object_registry`.  Covers Math, DateTime, Array, Random,
/// Enum, Guid, IpAddress, Dix — exactly what the runtime registers.
fn infer_static_method_return(object_name: &str, method_name: &str) -> Option<String> {
    let info = static_object_registry::get_method_info(object_name, method_name)?;
    dix_type_to_hint(info.return_type)
                    }
