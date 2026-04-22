// mdix-lsp/src/features/inlay_hints.rs
use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position};
use dixscript::Compiler::AST::{DataEntry, DataType, Value, Expression, QuickFuncStatement};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>) -> Option<Vec<InlayHint>> {
    let doc = doc?;
    let ast = doc.ast.as_ref()?;

    // Build QuickFunc name → declared return-type lookup (used by both sections).
    let qf_return_types: std::collections::HashMap<String, DataType> = ast
        .quick_functions
        .as_ref()
        .map(|qf| {
            qf.functions
                .iter()
                .filter_map(|f| f.return_type.map(|rt| (f.name.clone(), rt)))
                .collect()
        })
        .unwrap_or_default();

    let mut hints = Vec::new();

    // ── @DATA section ────────────────────────────────────────────────────────
    if let Some(data) = &ast.data {
        let type_index = doc
            .semantic_result
            .as_ref()
            .and_then(|sr| sr.type_index.as_ref());

        for entry in &data.entries {
            match entry {
                // ── Flat property ────────────────────────────────────────────
                DataEntry::SimpleProperty {
                    ref name,
                    ref data_type,
                    ref value,
                    ref position,
                } => {
                    if data_type.is_some() { continue; } // already annotated

                    let type_label = type_index
                        .as_ref()
                        .and_then(|idx| idx.get(name.as_str()))
                        .map(|dt| format!(": {}", dt))
                        .or_else(|| infer_type_label(value, &qf_return_types))
                        .unwrap_or_else(|| ": auto".to_string());

                    let line = position.line.saturating_sub(1) as u32;
                    let col  = (position.column.saturating_sub(1) + name.len()) as u32;
                    hints.push(make_hint(line, col, type_label));
                }

                // ── Table property (server: host = "x", port = 8080) ─────────
                DataEntry::TableProperty { ref properties, .. } => {
                    for prop in properties {
                        if prop.data_type.is_some() { continue; }
                        let type_label = infer_type_label(&prop.value, &qf_return_types)
                            .unwrap_or_else(|| ": auto".to_string());
                        let line = prop.position.line.saturating_sub(1) as u32;
                        let col  = (prop.position.column.saturating_sub(1) + prop.name.len()) as u32;
                        hints.push(make_hint(line, col, type_label));
                    }
                }

                // ── Group array (tags:: "a", "b") ─────────────────────────────
                DataEntry::GroupArray { ref path, ref items, ref position } => {
                    if items.is_empty() { continue; }
                    let elem_type = items
                        .first()
                        .and_then(|v| infer_type_label(v, &qf_return_types))
                        .map(|t| t.trim_start_matches(": ").to_string())
                        .unwrap_or_else(|| "any".to_string());

                    let path_str = path.segments.join(".");
                    let line = position.line.saturating_sub(1) as u32;
                    let col  = (position.column.saturating_sub(1) + path_str.len()) as u32;
                    hints.push(make_hint(line, col, format!(": {}[{}]", elem_type, items.len())));
                }

                DataEntry::ObjectProperty { .. } => {}
            }
        }
    }

    // ── @QUICKFUNCS section ──────────────────────────────────────────────────
    if let Some(qf) = &ast.quick_functions {
        for func in &qf.functions {
            collect_qf_var_hints(&func.body, &doc.tokens, &qf_return_types, &mut hints);
        }
    }

    if hints.is_empty() { None } else { Some(hints) }
}

// ── QuickFuncs: collect type hints for let/const declarations ─────────────────

fn collect_qf_var_hints(
    stmts: &[QuickFuncStatement],
    tokens: &[Token],
    qf_return_types: &std::collections::HashMap<String, DataType>,
    hints: &mut Vec<InlayHint>,
) {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration {
                variable_name,
                data_type,
                value,
                position,
                ..
            } => {
                if data_type.is_some() { continue; } // already annotated

                let type_label = infer_type_label_expr(value, qf_return_types)
                    .unwrap_or_else(|| ": ?".to_string());

                // Find the exact Identifier token for `variable_name` on the
                // declaration line so the hint sits right after the name.
                let target_line = position.line; // 1-based
                let hint_line   = position.line.saturating_sub(1) as u32;

                let col = tokens
                    .iter()
                    .filter(|t| t.line == target_line)
                    .find(|t| {
                        matches!(&t.token_type, TokenType::Identifier(id)
                            if id.as_str() == variable_name.as_str())
                    })
                    .map(|tok| (tok.column.saturating_sub(1) + variable_name.len()) as u32)
                    .unwrap_or_else(|| {
                        // Fallback estimate: `let ` prefix is 4 chars
                        (position.column.saturating_sub(1) + 4 + variable_name.len()) as u32
                    });

                hints.push(make_hint(hint_line, col, type_label));
            }

            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                collect_qf_var_hints(then_branch, tokens, qf_return_types, hints);
                if let Some(else_stmts) = else_branch {
                    collect_qf_var_hints(else_stmts, tokens, qf_return_types, hints);
                }
            }

            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    collect_qf_var_hints(&case.statements, tokens, qf_return_types, hints);
                }
                if let Some(dc) = default_case {
                    collect_qf_var_hints(&dc.statements, tokens, qf_return_types, hints);
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

// ── Type inference from a Value node ─────────────────────────────────────────

fn infer_type_label(
    value: &Value,
    qf_return_types: &std::collections::HashMap<String, DataType>,
) -> Option<String> {
    // Unwrap Expression wrappers first
    if let Value::Expression { expr, .. } = value {
        return match expr.as_ref() {
            Expression::QuickFuncCall { name, .. } => qf_return_types
                .get(name.as_str())
                .map(|rt| format!(": {}", rt)),
            Expression::Value { value: inner, .. } => infer_type_label(inner, qf_return_types),
            Expression::Conditional { true_value, .. } => {
                infer_type_label_expr(true_value, qf_return_types)
            }
            other => infer_type_label_expr(other, qf_return_types),
        };
    }

    // Direct QuickFunc call value
    if let Value::QuickFuncCall { function_name, .. } = value {
        return qf_return_types
            .get(function_name.as_str())
            .map(|rt| format!(": {}", rt));
    }

    if matches!(value, Value::Null { .. }) {
        return Some(": null".to_string());
    }

    let dt = match value {
        Value::Integer { .. }               => DataType::Int,
        Value::Float { .. }                 => DataType::Float,
        Value::Double { .. }                => DataType::Double,
        Value::ScientificNotation { .. }    => DataType::Double,
        Value::String { .. }                => DataType::String,
        Value::InterpolatedString { .. }    => DataType::String,
        Value::Boolean { .. }               => DataType::Bool,
        Value::Array { .. }
        | Value::NestedArray { .. }         => DataType::Array,
        Value::Object { .. }                => DataType::Object,
        Value::HexColor { .. }              => DataType::Hex,
        Value::Date { .. }                  => DataType::Date,
        Value::Timestamp { .. }             => DataType::Timestamp,
        Value::EnumValue { .. }             => DataType::Enum,
        Value::PrefixedConstructor { prefix, .. } => match prefix.as_str() {
            "b" => DataType::Blob,
            "t" => DataType::Tuple,
            "r" => DataType::Regex,
            _   => return None,
        },
        _ => return None,
    };

    Some(format!(": {}", dt))
}

// ── Type inference from an Expression node ────────────────────────────────────

fn infer_type_label_expr(
    expr: &Expression,
    qf_return_types: &std::collections::HashMap<String, DataType>,
) -> Option<String> {
    match expr {
        Expression::Value { value, .. } => infer_type_label(value, qf_return_types),

        Expression::QuickFuncCall { name, .. } => qf_return_types
            .get(name.as_str())
            .map(|rt| format!(": {}", rt)),

        Expression::ArithmeticOp { left, right, .. } => {
            // Numeric type promotion: Double > Float > String > Int
            let lt = infer_type_label_expr(left, qf_return_types);
            let rt = infer_type_label_expr(right, qf_return_types);
            match (lt.as_deref(), rt.as_deref()) {
                (Some(": double"), _) | (_, Some(": double")) => Some(": double".to_string()),
                (Some(": float"),  _) | (_, Some(": float"))  => Some(": float".to_string()),
                (Some(": string"), _) | (_, Some(": string"))  => Some(": string".to_string()),
                (Some(": int"),    _) | (_, Some(": int"))     => Some(": int".to_string()),
                _ => lt,
            }
        }

        Expression::ComparisonOp { .. } | Expression::LogicalOp { .. } => {
            Some(": bool".to_string())
        }

        Expression::UnaryOp { operator, operand, .. } => {
            if operator.as_str() == "!" || operator.as_str() == "not" {
                Some(": bool".to_string())
            } else {
                infer_type_label_expr(operand, qf_return_types)
            }
        }

        Expression::Conditional { true_value, .. } => {
            infer_type_label_expr(true_value, qf_return_types)
        }

        Expression::Parenthesized { expression, .. } => {
            infer_type_label_expr(expression, qf_return_types)
        }

        _ => None,
    }
}
