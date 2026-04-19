// mdix-lsp/src/features/inlay_hints.rs

use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position};
use dixscript::Compiler::AST::{DataEntry, DataType, Value};
use crate::document::Document;

pub fn provide(doc: Option<&Document>) -> Option<Vec<InlayHint>> {
    let doc  = doc?;
    let ast  = doc.ast.as_ref()?;
    let data = ast.data.as_ref()?;

    let type_index = doc
        .semantic_result
        .as_ref()
        .and_then(|sr| sr.type_index.as_ref());

    // Build QuickFunc name → return type lookup.
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

    for entry in &data.entries {
        match entry {
            // ── Flat property ────────────────────────────────────────────
            DataEntry::SimpleProperty {
                ref name,
                ref data_type,
                ref value,
                ref position,
            } => {
                if data_type.is_some() {
                    continue; // already annotated — skip
                }

                let type_label = type_index.as_ref()
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
                    if prop.data_type.is_some() {
                        continue; // already annotated
                    }

                    let type_label = infer_type_label(&prop.value, &qf_return_types)
                        .unwrap_or_else(|| ": auto".to_string());

                    let line = prop.position.line.saturating_sub(1) as u32;
                    let col  = (prop.position.column.saturating_sub(1) + prop.name.len()) as u32;
                    hints.push(make_hint(line, col, type_label));
                }
            }

            // ── Group array (items:: v1, v2, v3) ─────────────────────────
            DataEntry::GroupArray { ref path, ref items, ref position } => {
                if items.is_empty() {
                    continue;
                }

                // Infer element type from the first item; fall back to "any".
                let elem_type = items
                    .first()
                    .and_then(|v| infer_type_label(v, &qf_return_types))
                    .map(|t| t.trim_start_matches(": ").to_string())
                    .unwrap_or_else(|| "any".to_string());

                let path_str = path.segments.join(".");
                let line = position.line.saturating_sub(1) as u32;
                let col  = (position.column.saturating_sub(1) + path_str.len()) as u32;

                hints.push(make_hint(
                    line,
                    col,
                    format!(": {}[{}]", elem_type, items.len()),
                ));
            }

            DataEntry::ObjectProperty { .. } => {}
        }
    }

    if hints.is_empty() { None } else { Some(hints) }
}

// ── Hint constructor helper ───────────────────────────────────────────────────

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

// ── Type label inference ──────────────────────────────────────────────────────

/// Infer a display type label (e.g. `": int"`) from an AST value node.
///
/// Returns `None` when the type genuinely cannot be determined so that callers
/// can substitute `": auto"` — keeping `auto` only as the true last resort.
fn infer_type_label(
    value: &Value,
    qf_return_types: &std::collections::HashMap<String, DataType>,
) -> Option<String> {
    // ── Expression wrappers ──────────────────────────────────────────────
    if let Value::Expression { expr, .. } = value {
        use dixscript::Compiler::AST::Expression;
        return match expr.as_ref() {
            Expression::QuickFuncCall { name, .. } => qf_return_types
                .get(name.as_str())
                .map(|rt| format!(": {}", rt)),
            Expression::Value { value: inner, .. } => {
                infer_type_label(inner, qf_return_types)
            }
            Expression::Conditional { true_value, .. } => {
                // Infer from the true branch of a ternary.
                infer_type_label_expr(true_value, qf_return_types)
            }
            _ => None,
        };
    }

    // ── Direct QuickFunc call value ───────────────────────────────────────
    if let Value::QuickFuncCall { function_name, .. } = value {
        return qf_return_types
            .get(function_name.as_str())
            .map(|rt| format!(": {}", rt));
    }

    // ── Null literal ─────────────────────────────────────────────────────
    if matches!(value, Value::Null { .. }) {
        return Some(": null".to_string());
    }

    // ── Concrete value types ──────────────────────────────────────────────
    let dt = match value {
        Value::Integer { .. }                       => DataType::Int,
        Value::Float { .. }                         => DataType::Float,
        Value::Double { .. }                        => DataType::Double,
        Value::ScientificNotation { .. }            => DataType::Double,
        Value::String { .. }                        => DataType::String,
        Value::InterpolatedString { .. }            => DataType::String,
        Value::Boolean { .. }                       => DataType::Bool,
        Value::Array { .. } | Value::NestedArray { .. } => DataType::Array,
        Value::Object { .. }                        => DataType::Object,
        Value::HexColor { .. }                      => DataType::Hex,
        Value::Date { .. }                          => DataType::Date,
        Value::Timestamp { .. }                     => DataType::Timestamp,
        Value::EnumValue { .. }                     => DataType::Enum,
        Value::PrefixedConstructor { prefix, .. }   => match prefix.as_str() {
            "b" => DataType::Blob,
            "t" => DataType::Tuple,
            "r" => DataType::Regex,
            _   => return None,
        },
        // Everything else (Identifier, Lambda, Range, …) is unknown.
        _ => return None,
    };

    Some(format!(": {}", dt))
}

/// Infer a type label directly from an Expression node (used for ternary branches).
fn infer_type_label_expr(
    expr: &dixscript::Compiler::AST::Expression,
    qf_return_types: &std::collections::HashMap<String, DataType>,
) -> Option<String> {
    use dixscript::Compiler::AST::Expression;
    match expr {
        Expression::Value { value, .. } => infer_type_label(value, qf_return_types),
        Expression::QuickFuncCall { name, .. } => qf_return_types
            .get(name.as_str())
            .map(|rt| format!(": {}", rt)),
        _ => None,
    }
}