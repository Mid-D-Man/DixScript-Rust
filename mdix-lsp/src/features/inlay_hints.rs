// mdix-lsp/src/features/inlay_hints.rs
//! Inlay-hint provider.
//!
//! Shows inferred type labels next to unannotated identifiers in @DATA and
//! @QUICKFUNCS. All inference is conservative: when uncertain, we emit
//! `<any>` rather than a misleading label.
//!
//! ## Blob labels
//!
//! `b:(...)` constructors are decoded from base64 and sniffed via magic
//! bytes so the hint can read `<blob:image:4KB>`, `<blob:audio:12KB>`, etc.
//!
//! ## Array / tuple labels
//!
//! Arrays show `<type[count]>` when all elements have the same inferred type,
//! or `<any[count]>` when the types are mixed or unknown.
//! Tuples show `<tuple(int,str,bool)>` with per-element types (up to 6).
//!
//! ## Static / instance methods
//!
//! Return types are resolved via the live `StaticObjectRegistry` and
//! `InstanceMethodRegistry`.  Unknown calls fall back to `<any>`.

use std::panic;
use std::collections::HashMap;

use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position};
use dixscript::Compiler::AST::{DataEntry, DataType, Expression, QuickFuncStatement, Value};
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
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
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

    // Initialise registries (idempotent OnceLock-backed).
    instance_method_registry::initialize();
    static_object_registry::initialize_static_registry();

    // QuickFunc name → declared return type (used at every call site).
    let qf_return_types: HashMap<String, DataType> = ast
        .quick_functions
        .as_ref()
        .map(|qf| {
            qf.functions
                .iter()
                .filter_map(|f| f.return_type.map(|rt| (f.name.clone(), rt)))
                .collect()
        })
        .unwrap_or_default();

    let mut hints: Vec<InlayHint> = Vec::new();
    let no_params: HashMap<String, Option<DataType>> = HashMap::new();

    // ── @DATA ─────────────────────────────────────────────────────────────────
    if let Some(data) = &ast.data {
        let type_index = doc
            .semantic_result
            .as_ref()
            .and_then(|sr| sr.type_index.as_ref());

        for entry in &data.entries {
            match entry {
                // ── Simple property (x = ...) ─────────────────────────────────
                DataEntry::SimpleProperty {
                    name,
                    data_type,
                    value,
                    position,
                } => {
                    if data_type.is_some() || !position.is_valid() {
                        continue;
                    }
                    let label = type_index
                        .and_then(|idx| idx.get(name.as_str()))
                        .map(|dt| fmt_type(*dt))
                        .or_else(|| infer_value(value, &qf_return_types, &no_params))
                        .unwrap_or_else(|| "<any>".to_string());

                    let line = position.line.saturating_sub(1) as u32;
                    let col = (position.column.saturating_sub(1) + name.len()) as u32;
                    hints.push(make_hint(line, col, label));
                }

                // ── Table property (path: k = v, ...) ─────────────────────────
                DataEntry::TableProperty { properties, .. } => {
                    for prop in properties {
                        if prop.data_type.is_some() || !prop.position.is_valid() {
                            continue;
                        }
                        let label =
                            infer_value(&prop.value, &qf_return_types, &no_params)
                                .unwrap_or_else(|| "<any>".to_string());
                        let line = prop.position.line.saturating_sub(1) as u32;
                        let col =
                            (prop.position.column.saturating_sub(1) + prop.name.len()) as u32;
                        hints.push(make_hint(line, col, label));
                    }
                }

                // ── Group array (path:: v, v, ...) ────────────────────────────
                DataEntry::GroupArray {
                    path,
                    items,
                    position,
                } => {
                    if items.is_empty() || !position.is_valid() {
                        continue;
                    }
                    let label = array_label(items, &qf_return_types, &no_params);
                    let path_str = path.segments.join(".");
                    let line = position.line.saturating_sub(1) as u32;
                    let col = (position.column.saturating_sub(1) + path_str.len()) as u32;
                    hints.push(make_hint(line, col, label));
                }

                // ObjectProperty is self-documenting ({ ... }), skip.
                DataEntry::ObjectProperty { .. } => {}
            }
        }
    }

    // ── @QUICKFUNCS ───────────────────────────────────────────────────────────
    if let Some(qf) = &ast.quick_functions {
        for func in &qf.functions {
            // Map parameter names → their declared types (may be None).
            let param_types: HashMap<String, Option<DataType>> = func
                .parameters
                .iter()
                .map(|p| (p.name.clone(), p.data_type))
                .collect();

            // Unannotated parameters → <any>.
            for param in &func.parameters {
                if param.data_type.is_some() || !param.position.is_valid() {
                    continue;
                }
                let line = param.position.line.saturating_sub(1) as u32;
                let col = (param.position.column.saturating_sub(1) + param.name.len()) as u32;
                hints.push(make_hint(line, col, "<any>".to_string()));
            }

            // Local variable declarations.
            collect_qf_var_hints(
                &func.body,
                &doc.tokens,
                &qf_return_types,
                &param_types,
                &mut hints,
            );
        }
    }

    if hints.is_empty() {
        None
    } else {
        Some(hints)
    }
}

// ── Group-array / explicit-array label ───────────────────────────────────────

/// `<type[count]>` when all elements share a type; `<any[count]>` when mixed.
fn array_label(
    items: &[Value],
    qf_return_types: &HashMap<String, DataType>,
    param_types: &HashMap<String, Option<DataType>>,
) -> String {
    let count = items.len();
    let elem = uniform_element_type(items, qf_return_types, param_types);
    match elem {
        Some(t) => {
            let inner = t.trim_start_matches('<').trim_end_matches('>');
            format!("<{}[{}]>", inner, count)
        }
        None => format!("<any[{}]>", count),
    }
}

/// Returns the shared element type when all items have the same inferred type,
/// or `None` when they differ or are all unknown.
fn uniform_element_type(
    items: &[Value],
    qf_return_types: &HashMap<String, DataType>,
    param_types: &HashMap<String, Option<DataType>>,
) -> Option<String> {
    let mut iter = items
        .iter()
        .map(|v| infer_value(v, qf_return_types, param_types));

    let first = iter.next()??; // None if first item is unknown

    for next in iter {
        match next {
            Some(ref t) if t == &first => {}
            _ => return None, // mixed or unknown → fall back to any
        }
    }
    Some(first)
}

// ── QuickFunc variable-declaration hints ─────────────────────────────────────

fn collect_qf_var_hints(
    stmts: &[QuickFuncStatement],
    tokens: &[Token],
    qf_return_types: &HashMap<String, DataType>,
    param_types: &HashMap<String, Option<DataType>>,
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
                if data_type.is_some() {
                    continue;
                }
                let label = infer_expr(value, qf_return_types, param_types)
                    .unwrap_or_else(|| "<any>".to_string());

                let target_line = position.line;
                let hint_line = target_line.saturating_sub(1) as u32;

                let col = tokens
                    .iter()
                    .filter(|t| t.line == target_line)
                    .find(|t| {
                        matches!(&t.token_type,
                            TokenType::Identifier(id) if id.as_str() == variable_name.as_str())
                    })
                    .map(|tok| (tok.column.saturating_sub(1) + variable_name.len()) as u32)
                    .unwrap_or_else(|| {
                        (position.column.saturating_sub(1) + 4 + variable_name.len()) as u32
                    });

                hints.push(make_hint(hint_line, col, label));
            }

            QuickFuncStatement::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_qf_var_hints(then_branch, tokens, qf_return_types, param_types, hints);
                if let Some(else_stmts) = else_branch {
                    collect_qf_var_hints(
                        else_stmts,
                        tokens,
                        qf_return_types,
                        param_types,
                        hints,
                    );
                }
            }

            QuickFuncStatement::Switch {
                cases,
                default_case,
                ..
            } => {
                for case in cases {
                    collect_qf_var_hints(
                        &case.statements,
                        tokens,
                        qf_return_types,
                        param_types,
                        hints,
                    );
                }
                if let Some(dc) = default_case {
                    collect_qf_var_hints(
                        &dc.statements,
                        tokens,
                        qf_return_types,
                        param_types,
                        hints,
                    );
                }
            }

            _ => {}
        }
    }
}

// ── Value-level type inference ────────────────────────────────────────────────

/// Infer the DixScript type label (`<int>`, `<blob:audio:4KB>`, …) for a Value
/// node.  Returns `None` when no reliable inference is possible.
fn infer_value(
    value: &Value,
    qf_return_types: &HashMap<String, DataType>,
    param_types: &HashMap<String, Option<DataType>>,
) -> Option<String> {
    match value {
        // ── Delegate to expression inference ─────────────────────────────────
        Value::Expression { expr, .. } => infer_expr(expr, qf_return_types, param_types),
        Value::QuickFuncCall { function_name, .. } => {
            qf_return_types.get(function_name.as_str()).map(|rt| fmt_type(*rt))
        }
        Value::Identifier { value: name, .. } => {
            param_types
                .get(name.as_str())
                .map(|opt_dt| match *opt_dt {
                    Some(dt) => fmt_type(dt),
                    None => "<any>".to_string(),
                })
        }

        // ── Null ──────────────────────────────────────────────────────────────
        Value::Null { .. } => Some("<null>".to_string()),

        // ── Primitives ────────────────────────────────────────────────────────
        Value::Integer { .. } => Some("<int>".to_string()),
        Value::Float { .. } => Some("<float>".to_string()),
        Value::Double { .. } | Value::ScientificNotation { .. } => Some("<double>".to_string()),
        Value::String { .. } | Value::InterpolatedString { .. } => Some("<string>".to_string()),
        Value::Boolean { .. } => Some("<bool>".to_string()),
        Value::HexColor { .. } => Some("<hex>".to_string()),
        Value::Date { .. } => Some("<date>".to_string()),
        Value::Timestamp { .. } => Some("<timestamp>".to_string()),
        Value::EnumValue { .. } => Some("<enum>".to_string()),
        Value::Object { .. } => Some("<object>".to_string()),

        // ── Arrays ────────────────────────────────────────────────────────────
        Value::Array { values, .. } | Value::NestedArray { values, .. } => {
            if values.is_empty() {
                Some("<array[0]>".to_string())
            } else {
                Some(array_label(values, qf_return_types, param_types))
            }
        }

        // ── Prefixed constructors ─────────────────────────────────────────────
        Value::PrefixedConstructor { prefix, arguments, .. } => {
            match prefix.as_str() {
                "b" => Some(blob_label(arguments)),
                "t" => Some(tuple_label(arguments, qf_return_types, param_types)),
                "r" => Some("<regex>".to_string()),
                _ => None,
            }
        }

        _ => None,
    }
}

// ── Blob label ────────────────────────────────────────────────────────────────

/// Decode the base64 argument, sniff magic bytes, produce a descriptive label.
///
/// Examples:
///   `<blob:image:48KB>`   for a JPEG
///   `<blob:audio:220KB>`  for an MP3
///   `<blob:pdf:12KB>`     for a PDF
///   `<blob:data:3B>`      for unknown binary
///   `<blob:invalid>`      when base64 is malformed
fn blob_label(arguments: &[Value]) -> String {
    // Extract the base64 string from the first constructor argument.
    let b64 = match arguments.first() {
        Some(Value::String { value, .. }) => value.as_str(),
        Some(Value::InterpolatedString { template, .. }) => template.as_str(),
        _ => return "<blob>".to_string(),
    };

    use base64::{engine::general_purpose, Engine as _};

    let bytes = general_purpose::STANDARD
        .decode(b64.trim())
        .or_else(|_| general_purpose::STANDARD_NO_PAD.decode(b64.trim()))
        .or_else(|_| general_purpose::URL_SAFE.decode(b64.trim()))
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(b64.trim()));

    let bytes = match bytes {
        Ok(b) => b,
        Err(_) => return "<blob:invalid>".to_string(),
    };

    let category = sniff_blob_category(&bytes);
    let size = if bytes.len() >= 1024 * 1024 {
        format!("{}MB", bytes.len() / (1024 * 1024))
    } else if bytes.len() >= 1024 {
        format!("{}KB", bytes.len() / 1024)
    } else {
        format!("{}B", bytes.len())
    };

    format!("<blob:{}:{}>", category, size)
}

/// Identify the content category from magic bytes.
fn sniff_blob_category(b: &[u8]) -> &'static str {
    if b.len() < 4 {
        return "data";
    }

    // ── Images ────────────────────────────────────────────────────────────────
    if b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF {
        return "image"; // JPEG
    }
    if b[0] == 0x89 && b[1] == 0x50 && b[2] == 0x4E && b[3] == 0x47 {
        return "image"; // PNG
    }
    if b[0] == 0x47 && b[1] == 0x49 && b[2] == 0x46 {
        return "image"; // GIF
    }
    if b.len() >= 12
        && b[0] == 0x52 && b[1] == 0x49 && b[2] == 0x46 && b[3] == 0x46
        && b[8] == 0x57 && b[9] == 0x45 && b[10] == 0x42 && b[11] == 0x50
    {
        return "image"; // WebP
    }
    if b[0] == 0x42 && b[1] == 0x4D {
        return "image"; // BMP
    }
    if b.len() >= 4 && b[0] == 0x00 && b[1] == 0x00 && b[2] == 0x01 && b[3] == 0x00 {
        return "image"; // ICO
    }

    // ── Audio ─────────────────────────────────────────────────────────────────
    if b[0] == 0x49 && b[1] == 0x44 && b[2] == 0x33 {
        return "audio"; // MP3 with ID3 tag
    }
    if b[0] == 0xFF && (b[1] & 0xE0) == 0xE0 {
        return "audio"; // MP3 sync word
    }
    if b[0] == 0x4F && b[1] == 0x67 && b[2] == 0x67 && b[3] == 0x53 {
        return "audio"; // OGG
    }
    if b[0] == 0x66 && b[1] == 0x4C && b[2] == 0x61 && b[3] == 0x43 {
        return "audio"; // FLAC
    }
    if b[0] == 0xFF && (b[1] == 0xF1 || b[1] == 0xF9) {
        return "audio"; // AAC ADTS
    }
    // WAV: RIFF????WAVE
    if b.len() >= 12
        && b[0] == 0x52 && b[1] == 0x49 && b[2] == 0x46 && b[3] == 0x46
        && b[8] == 0x57 && b[9] == 0x41 && b[10] == 0x56 && b[11] == 0x45
    {
        return "audio";
    }

    // ── Video ─────────────────────────────────────────────────────────────────
    // AVI: RIFF????AVI
    if b.len() >= 12
        && b[0] == 0x52 && b[1] == 0x49 && b[2] == 0x46 && b[3] == 0x46
        && b[8] == 0x41 && b[9] == 0x56 && b[10] == 0x49
    {
        return "video";
    }
    // MKV / WebM
    if b[0] == 0x1A && b[1] == 0x45 && b[2] == 0xDF && b[3] == 0xA3 {
        return "video";
    }
    // MP4 / M4A / M4V ftyp box at offset 4
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        let brand = &b[8..12];
        return match brand {
            b"M4A " | b"M4B " => "audio",
            b"M4V " | b"mp42" | b"avc1" => "video",
            b"isom" | b"iso2" => "video",
            _ => "video",
        };
    }

    // ── Documents / archives ──────────────────────────────────────────────────
    if b[0] == 0x25 && b[1] == 0x50 && b[2] == 0x44 && b[3] == 0x46 {
        return "pdf"; // %PDF
    }
    if b[0] == 0x50 && b[1] == 0x4B {
        return "zip"; // PK (ZIP, DOCX, XLSX, JAR, …)
    }
    if b[0] == 0x1F && b[1] == 0x8B {
        return "gzip";
    }
    if b.len() >= 6
        && b[0] == 0x37 && b[1] == 0x7A && b[2] == 0xBC
        && b[3] == 0xAF && b[4] == 0x27 && b[5] == 0x1C
    {
        return "7z";
    }
    if b.len() >= 5 && b[0] == 0xFD && b[1] == 0x37 && b[2] == 0x7A
        && b[3] == 0x58 && b[4] == 0x5A
    {
        return "xz";
    }

    // ── Fonts ─────────────────────────────────────────────────────────────────
    if b[0] == 0x77 && b[1] == 0x4F && b[2] == 0x46 && b[3] == 0x46 {
        return "font"; // WOFF
    }
    if b[0] == 0x77 && b[1] == 0x4F && b[2] == 0x46 && b[3] == 0x32 {
        return "font"; // WOFF2
    }
    if b[0] == 0x00 && b[1] == 0x01 && b[2] == 0x00 && b[3] == 0x00 {
        return "font"; // TTF/OTF
    }

    // ── Text heuristic ─────────────────────────────────────────────────────────
    // If the first 64 bytes are all printable ASCII / common whitespace,
    // it's likely a text payload.
    let is_printable = b
        .iter()
        .take(64)
        .all(|&byte| byte == b'\t' || byte == b'\n' || byte == b'\r' || (0x20..=0x7E).contains(&byte));

    if is_printable {
        let head = std::str::from_utf8(&b[..b.len().min(32)]).unwrap_or("");
        let trimmed = head.trim_start();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            return "json";
        }
        if trimmed.starts_with('<') {
            return "xml";
        }
        return "text";
    }

    "data"
}

// ── Tuple label ───────────────────────────────────────────────────────────────

/// `<tuple(int,str,bool)>` for determinable types; `<tuple:N>` otherwise.
fn tuple_label(
    arguments: &[Value],
    qf_return_types: &HashMap<String, DataType>,
    param_types: &HashMap<String, Option<DataType>>,
) -> String {
    let types: Vec<String> = arguments
        .iter()
        .take(6) // DixScript max tuple size
        .map(|v| {
            infer_value(v, qf_return_types, param_types)
                .map(|t| {
                    t.trim_start_matches('<')
                        .trim_end_matches('>')
                        .to_string()
                })
                .unwrap_or_else(|| "?".to_string())
        })
        .collect();

    if types.is_empty() {
        return "<tuple>".to_string();
    }
    // All unknown → show count only
    if types.iter().all(|t| t == "?") {
        return format!("<tuple:{}>", types.len());
    }
    format!("<tuple({})>", types.join(","))
}

// ── Expression-level type inference ──────────────────────────────────────────

fn infer_expr(
    expr: &Expression,
    qf_return_types: &HashMap<String, DataType>,
    param_types: &HashMap<String, Option<DataType>>,
) -> Option<String> {
    match expr {
        // ── Value literal ─────────────────────────────────────────────────────
        Expression::Value { value, .. } => infer_value(value, qf_return_types, param_types),

        // ── Identifier ───────────────────────────────────────────────────────
        Expression::Identifier { name, .. } => param_types
            .get(name.as_str())
            .map(|opt| match *opt {
                Some(dt) => fmt_type(dt),
                None => "<any>".to_string(),
            }),

        // ── QuickFunc / generic calls ─────────────────────────────────────────
        Expression::QuickFuncCall { name, .. } | Expression::FunctionCall { name, .. } => {
            qf_return_types.get(name.as_str()).map(|rt| fmt_type(*rt))
        }

        Expression::DixFunctionCall { .. } => Some("<any>".to_string()),

        // ── Qualified identifier (pre-enhancement: arr.first(), Math.sqrt()) ─
        Expression::QualifiedIdentifier { parts, arguments, .. } => {
            infer_qualified(parts, arguments.as_ref(), qf_return_types, param_types)
        }

        // ── Post-enhancement: static calls ────────────────────────────────────
        Expression::StaticMethodCall {
            object_name,
            method_name,
            ..
        } => static_return(object_name, method_name),

        Expression::StaticFunction {
            class_name,
            method,
            ..
        } => static_return(class_name, method),

        // ── Post-enhancement: instance method / property ──────────────────────
        Expression::InstanceMethodCall {
            instance,
            method_name,
            ..
        } => {
            let recv = infer_expr(instance, qf_return_types, param_types);
            instance_return(recv.as_deref(), method_name)
        }

        Expression::BuiltinFunction { target, method, .. } => {
            let recv = infer_expr(target, qf_return_types, param_types);
            instance_return(recv.as_deref(), method)
        }

        Expression::PropertyAccess { object, property, .. } => {
            let recv = infer_expr(object, qf_return_types, param_types);
            instance_return(recv.as_deref(), property)
        }

        Expression::IndexAccess { .. } => None, // element type not tracked yet

        // ── Arithmetic ────────────────────────────────────────────────────────
        Expression::ArithmeticOp {
            left,
            operator,
            right,
            ..
        } => {
            let lt = infer_expr(left, qf_return_types, param_types);
            let rt = infer_expr(right, qf_return_types, param_types);

            if operator.as_str() == "+" {
                if lt.as_deref() == Some("<string>") || rt.as_deref() == Some("<string>") {
                    return Some("<string>".to_string());
                }
            }

            match (lt.as_deref(), rt.as_deref()) {
                (Some("<double>"), _) | (_, Some("<double>")) => Some("<double>".to_string()),
                (Some("<float>"), _) | (_, Some("<float>")) => Some("<float>".to_string()),
                (Some("<int>"), _) | (_, Some("<int>")) => Some("<int>".to_string()),
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

        Expression::UnaryOp {
            operator, operand, ..
        } => {
            if matches!(operator.as_str(), "!" | "not") {
                Some("<bool>".to_string())
            } else {
                infer_expr(operand, qf_return_types, param_types)
            }
        }

        Expression::Conditional {
            true_value,
            false_value,
            ..
        } => {
            let t = infer_expr(true_value, qf_return_types, param_types);
            let f = infer_expr(false_value, qf_return_types, param_types);
            match (t, f) {
                (Some(a), Some(b)) if a == b => Some(a),
                (Some(_), Some(_)) => Some("<any>".to_string()),
                (t, f) => t.or(f),
            }
        }

        Expression::Parenthesized { expression, .. } => {
            infer_expr(expression, qf_return_types, param_types)
        }

        Expression::TypeCast { target_type, .. } => Some(fmt_type(*target_type)),

        Expression::EnumAccess { .. } => Some("<enum>".to_string()),

        Expression::ConfigAccess { .. } => None,

        _ => None,
    }
}

// ── Qualified identifier dispatch ─────────────────────────────────────────────

fn infer_qualified(
    parts: &[String],
    arguments: Option<&Vec<Expression>>,
    qf_return_types: &HashMap<String, DataType>,
    param_types: &HashMap<String, Option<DataType>>,
) -> Option<String> {
    if parts.len() < 2 {
        return None;
    }

    let head = &parts[0];
    let member = &parts[1];

    // 1. PascalCase → static object
    if head.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
        if parts.len() >= 3 {
            // e.g. Namespace.ClassName.method — try two-level static
            return static_return(&parts[1], &parts[2]);
        }
        return static_return(head, member);
    }

    // 2. No arguments → property or enum access (can't determine type here)
    if arguments.is_none() {
        return None;
    }

    // 3. Known parameter as receiver → instance method
    if let Some(opt_dt) = param_types.get(head.as_str()) {
        let recv = opt_dt.map(|dt| fmt_type(dt));
        if let Some(result) = instance_return(recv.as_deref(), member) {
            return Some(result);
        }
        return recv;
    }

    // 4. QuickFunc result as receiver
    if let Some(rt) = qf_return_types.get(head.as_str()) {
        let recv = fmt_type(*rt);
        return instance_return(Some(recv.as_str()), member);
    }

    None
}

// ── Registry-backed return type lookups ──────────────────────────────────────

/// Return type of a static method via the live registry.  `None` if unknown.
fn static_return(object: &str, method: &str) -> Option<String> {
    let info = static_object_registry::get_method_info(object, method)?;
    dix_to_hint(info.return_type)
}

/// Return type of an instance method given the receiver's hint label.
/// Returns `None` (not `<any>`) when the registry doesn't know the answer —
/// callers decide the fallback.
fn instance_return(receiver_hint: Option<&str>, method: &str) -> Option<String> {
    let dix_type = hint_to_dix(receiver_hint?)?;
    let m = instance_method_registry::get_instance_method(dix_type, method)?;
    dix_to_hint(m.return_type())
}

// ── Type conversion helpers ───────────────────────────────────────────────────

fn fmt_type(dt: DataType) -> String {
    format!("<{}>", dt)
}

/// Convert a hint string like `"<int>"`, `"int"`, or `"<blob:image:4KB>"` to
/// the corresponding `DixType`.
fn hint_to_dix(hint: &str) -> Option<DixType> {
    let s = hint.trim_start_matches('<').trim_end_matches('>');
    // Handle compound forms first (blob:image:4KB, array[3], tuple(int,…))
    if s.starts_with("blob") { return Some(DixType::Blob); }
    if s.starts_with("array") { return Some(DixType::Array); }
    if s.starts_with("tuple") { return Some(DixType::Tuple); }

    match s {
        "int"       => Some(DixType::Int),
        "float"     => Some(DixType::Float),
        "double"    => Some(DixType::Double),
        "string"    => Some(DixType::String),
        "bool"      => Some(DixType::Bool),
        "array"     => Some(DixType::Array),
        "tuple"     => Some(DixType::Tuple),
        "object"    => Some(DixType::Object),
        "hex"       => Some(DixType::Hex),
        "blob"      => Some(DixType::Blob),
        "regex"     => Some(DixType::Regex),
        "date"      => Some(DixType::Date),
        "timestamp" => Some(DixType::Timestamp),
        "enum"      => Some(DixType::Enum),
        "any"       => Some(DixType::Any),
        _           => None,
    }
}

fn dix_to_hint(dt: DixType) -> Option<String> {
    match dt {
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
