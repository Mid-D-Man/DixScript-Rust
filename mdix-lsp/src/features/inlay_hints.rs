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

    // Build QuickFunc name → return type lookup for function-call value inference.
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
        if let DataEntry::SimpleProperty {
            ref name,
            ref data_type,
            ref value,
            ref position,
        } = *entry
        {
            // Skip properties that already carry an explicit type annotation.
            if data_type.is_some() {
                continue;
            }

            let type_label = type_index.as_ref()
                .and_then(|idx| idx.get(name.as_str()))
                .map(|dt| format!(": {}", dt))
                .or_else(|| infer_type_label(value, &qf_return_types))
                .unwrap_or_else(|| ": auto".to_string());

            let line = position.line.saturating_sub(1) as u32;
            let col  = (position.column.saturating_sub(1) + name.len()) as u32;

            hints.push(InlayHint {
                position:      Position::new(line, col),
                label:         InlayHintLabel::String(type_label),
                kind:          Some(InlayHintKind::TYPE),
                text_edits:    None,
                tooltip:       None,
                padding_left:  Some(false),
                padding_right: Some(true),
                data:          None,
            });
        }
    }

    if hints.is_empty() { None } else { Some(hints) }
}

/// Infer a display type label from the AST value node.
/// `qf_return_types` is consulted when the value is a QuickFunc call expression.
fn infer_type_label(
    value: &Value,
    qf_return_types: &std::collections::HashMap<String, DataType>,
) -> Option<String> {
    // Handle QuickFunc call expressions before the main match so we can
    // delegate early without fighting the "let dt = match" pattern.
    if let Value::Expression { expr, .. } = value {
        use dixscript::Compiler::AST::Expression;
        match expr.as_ref() {
            Expression::QuickFuncCall { name, .. } => {
                return qf_return_types
                    .get(name.as_str())
                    .map(|rt| format!(": {}", rt));
            }
            Expression::Value { value: inner, .. } => {
                return infer_type_label(inner, qf_return_types);
            }
            _ => return None,
        }
    }

    let dt = match value {
        Value::Integer { .. }              => DataType::Int,
        Value::Float { .. }                => DataType::Float,
        Value::Double { .. }               => DataType::Double,
        Value::ScientificNotation { .. }   => DataType::Double,
        Value::String { .. }               => DataType::String,
        Value::InterpolatedString { .. }   => DataType::String,
        Value::Boolean { .. }              => DataType::Bool,
        Value::Array { .. } | Value::NestedArray { .. } => DataType::Array,
        Value::Object { .. }               => DataType::Object,
        Value::HexColor { .. }             => DataType::Hex,
        Value::Date { .. }                 => DataType::Date,
        Value::Timestamp { .. }            => DataType::Timestamp,
        Value::EnumValue { .. }            => DataType::Enum,
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