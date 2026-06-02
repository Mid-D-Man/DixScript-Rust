// mdix-lsp/src/features/inlay_hints.rs
use std::panic;
use std::collections::HashMap;

use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position};
use dixscript::Compiler::AST::{
    DataEntry, DataType, ElemType, Expression, QuickFuncStatement, Value, TypeInferenceVisitor,
};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Builtins::Core::DixType;
use dixscript::Builtins::Resolver::{instance_method_registry, static_object_registry};
use dixscript::Compiler::Utilities::SymbolTable;
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

// ── Inference context ─────────────────────────────────────────────────────────

/// All read-only state threaded through type-inference helpers.
struct InferCtx<'a> {
    qf_return_types: &'a HashMap<String, DataType>,
    param_types:     &'a HashMap<String, Option<DataType>>,
    symbol_table:    Option<&'a SymbolTable>,
}

impl<'a> InferCtx<'a> {
    fn new(
        qf_return_types: &'a HashMap<String, DataType>,
        param_types:     &'a HashMap<String, Option<DataType>>,
        symbol_table:    Option<&'a SymbolTable>,
    ) -> Self {
        Self { qf_return_types, param_types, symbol_table }
    }
}

// ── Precise type via compiler's TypeInferenceVisitor ─────────────────────────

fn precise_dt(expr: &Expression, ctx: &InferCtx<'_>) -> Option<DataType> {
    let st         = ctx.symbol_table?;
    let local_vars = Some(ctx.param_types.clone());
    let visitor    = TypeInferenceVisitor::new(st, local_vars);
    visitor.infer_type_from_expression(expr)
}

// ── Build full typed DataType from expression ─────────────────────────────────

fn build_typed_dt(expr: &Expression, ctx: &InferCtx<'_>) -> Option<DataType> {
    if let Some(dt) = precise_dt(expr, ctx) {
        return Some(dt);
    }
    match expr {
        Expression::Value { value, .. } => build_typed_dt_from_value(value, ctx),
        Expression::Identifier { name, .. } => {
            ctx.param_types.get(name.as_str()).copied().flatten()
        }
        Expression::QuickFuncCall { name, .. } | Expression::FunctionCall { name, .. } => {
            resolve_func_name_type(name, ctx)
        }
        Expression::ImportedFunctionCall { namespace_name, function_name, .. } => {
            ctx.symbol_table
                .and_then(|st| st.get_namespaced_function(namespace_name, function_name))
                .and_then(|i| i.signature.return_type)
        }
        Expression::QualifiedIdentifier { parts, arguments, .. } if arguments.is_some() => {
            if parts.len() == 2 {
                if let Some(st) = ctx.symbol_table {
                    if let Some(info) = st.get_namespaced_function(&parts[0], &parts[1]) {
                        return info.signature.return_type;
                    }
                }
                static_return_dt(&parts[0], &parts[1])
            } else {
                None
            }
        }
        _ => infer_expr(expr, ctx).and_then(|h| basic_hint_str_to_dt(&h)),
    }
}

fn build_typed_dt_from_value(value: &Value, ctx: &InferCtx<'_>) -> Option<DataType> {
    match value {
        // Tuple constructor t:(...)
        Value::PrefixedConstructor { prefix, arguments, .. }
            if prefix.eq_ignore_ascii_case("t") =>
        {
            let mut slots = [None; 6];
            for (i, arg) in arguments.iter().enumerate().take(6) {
                if let Some(type_name) = infer_base_type(arg, ctx) {
                    slots[i] = ElemType::from_keyword(&type_name);
                }
            }
            if slots.iter().any(|s| s.is_some()) {
                Some(DataType::TypedTuple(slots))
            } else {
                Some(DataType::Tuple)
            }
        }
        // Array literals
        Value::Array { values, .. } | Value::NestedArray { values, .. } => {
            if values.is_empty() {
                return Some(DataType::Array);
            }
            let first_name = infer_base_type(&values[0], ctx)?;
            let elem       = ElemType::from_keyword(&first_name)?;
            let all_same   = values.iter().skip(1).all(|v| {
                infer_base_type(v, ctx).as_deref() == Some(first_name.as_str())
            });
            if all_same { Some(DataType::TypedArray(elem)) } else { Some(DataType::Array) }
        }
        // Primitives
        Value::Integer { .. }                                    => Some(DataType::Int),
        Value::Long { .. }                                       => Some(DataType::Long),
        Value::Float { .. }                                      => Some(DataType::Float),
        Value::Double { .. } | Value::ScientificNotation { .. } => Some(DataType::Double),
        Value::String { .. } | Value::InterpolatedString { .. } => Some(DataType::String),
        Value::Boolean { .. }                                    => Some(DataType::Bool),
        Value::HexColor { .. }                                   => Some(DataType::Hex),
        Value::Date { .. }                                       => Some(DataType::Date),
        Value::Timestamp { .. }                                  => Some(DataType::Timestamp),
        Value::EnumValue { .. }                                  => Some(DataType::Enum),
        Value::Object { .. }                                     => Some(DataType::Object),
        Value::Null { .. }                                       => None,
        // Other prefixed constructors
        Value::PrefixedConstructor { prefix, .. } => match prefix.as_str() {
            "b" => Some(DataType::Blob),
            "r" => Some(DataType::Regex),
            _   => None,
        },
        // Function call
        Value::QuickFuncCall { function_name, .. } => {
            resolve_func_name_type(function_name, ctx)
        }
        // Identifier reference
        Value::Identifier { value: name, .. } => {
            ctx.param_types.get(name.as_str()).copied().flatten()
        }
        // Wrapped expression
        Value::Expression { expr, .. } => build_typed_dt(expr, ctx),
        _ => None,
    }
}

fn basic_hint_str_to_dt(hint: &str) -> Option<DataType> {
    let s = hint.trim_start_matches('<').trim_end_matches('>');
    let s = s.split('[').next().unwrap_or(s);
    let s = s.split('<').next().unwrap_or(s);
    let s = s.split('(').next().unwrap_or(s);
    match s {
        "int"       => Some(DataType::Int),
        "long"      => Some(DataType::Long),
        "float"     => Some(DataType::Float),
        "double"    => Some(DataType::Double),
        "string"    => Some(DataType::String),
        "bool"      => Some(DataType::Bool),
        "array"     => Some(DataType::Array),
        "tuple"     => Some(DataType::Tuple),
        "object"    => Some(DataType::Object),
        "hex"       => Some(DataType::Hex),
        "blob"      => Some(DataType::Blob),
        "regex"     => Some(DataType::Regex),
        "date"      => Some(DataType::Date),
        "timestamp" => Some(DataType::Timestamp),
        "enum"      => Some(DataType::Enum),
        _           => None,
    }
}

fn collection_elem_count(expr: &Expression) -> Option<usize> {
    match expr {
        Expression::Value { value, .. } => collection_len(value),
        _ => None,
    }
}

// ── Main provider ─────────────────────────────────────────────────────────────

fn provide_inner(doc: Option<&Document>) -> Option<Vec<InlayHint>> {
    let doc = doc?;
    let ast = doc.ast.as_ref()?;

    // Guard: initialize registries (idempotent)
    instance_method_registry::initialize();
    static_object_registry::initialize_static_registry();

    let symbol_table: Option<&SymbolTable> = doc
        .semantic_result
        .as_ref()
        .and_then(|sr| sr.symbol_table.as_ref());

    // Build QuickFunc return-type map for fast lookup
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

    let no_params: HashMap<String, Option<DataType>> = HashMap::new();
    let base_ctx = InferCtx::new(&qf_return_types, &no_params, symbol_table);

    let mut hints: Vec<InlayHint> = Vec::new();

    // ── @DATA section ─────────────────────────────────────────────────────────
    if let Some(data) = &ast.data {
        let type_index = doc
            .semantic_result
            .as_ref()
            .and_then(|sr| sr.type_index.as_ref());

        for entry in &data.entries {
            match entry {
                DataEntry::SimpleProperty { name, data_type, value, position } => {
                    if !position.is_valid() { continue; }

                    let label = match data_type {
                        // Already has typed annotation — skip (no redundant hint)
                        Some(DataType::TypedArray(_)) | Some(DataType::TypedTuple(_)) => continue,
                        Some(dt @ DataType::Array) | Some(dt @ DataType::Tuple) => {
                            match collection_len(value) {
                                Some(n) if n > 0 => format_data_type_as_hint(*dt, Some(n)),
                                _                => continue,
                            }
                        }
                        Some(_) => continue,
                        None => {
                            type_index
                                .and_then(|idx| idx.get(name.as_str()))
                                .map(|dt| format_data_type_as_hint(*dt, collection_len(value)))
                                .or_else(|| infer_value(value, &base_ctx))
                                .unwrap_or_else(|| "<any>".to_string())
                        }
                    };

                    let line = position.line.saturating_sub(1) as u32;
                    let col  = (position.column.saturating_sub(1) + name.len()) as u32;
                    hints.push(make_hint(line, col, label));

                    // Bonus: parameter name hints for QuickFunc calls
                    emit_qf_param_name_hints(value, doc, &mut hints);
                }

                DataEntry::TableProperty { properties, .. } => {
                    for prop in properties {
                        if !prop.position.is_valid() { continue; }

                        let label = match prop.data_type {
                            Some(DataType::TypedArray(_)) | Some(DataType::TypedTuple(_)) => continue,
                            Some(dt @ DataType::Array) | Some(dt @ DataType::Tuple) => {
                                match collection_len(&prop.value) {
                                    Some(n) if n > 0 => format_data_type_as_hint(dt, Some(n)),
                                    _                => continue,
                                }
                            }
                            Some(_) => continue,
                            None    => infer_value(&prop.value, &base_ctx)
                                           .unwrap_or_else(|| "<any>".to_string()),
                        };

                        let line = prop.position.line.saturating_sub(1) as u32;
                        let col  = (prop.position.column.saturating_sub(1) + prop.name.len()) as u32;
                        hints.push(make_hint(line, col, label));

                        emit_qf_param_name_hints(&prop.value, doc, &mut hints);
                    }
                }

                DataEntry::GroupArray { path, items, position } => {
                    if items.is_empty() || !position.is_valid() { continue; }
                    let label    = array_label_from_values(items, &base_ctx);
                    let path_str = path.segments.join(".");
                    let line     = position.line.saturating_sub(1) as u32;
                    let col      = (position.column.saturating_sub(1) + path_str.len()) as u32;
                    hints.push(make_hint(line, col, label));
                }

                DataEntry::ObjectProperty { name, data_type, object, position } => {
                    if !position.is_valid() || data_type.is_some() { continue; }
                    let label = infer_value(object, &base_ctx)
                        .unwrap_or_else(|| "<object>".to_string());
                    let line = position.line.saturating_sub(1) as u32;
                    let col  = (position.column.saturating_sub(1) + name.len()) as u32;
                    hints.push(make_hint(line, col, label));
                }
            }
        }
    }

    // ── @QUICKFUNCS section ───────────────────────────────────────────────────
    if let Some(qf) = &ast.quick_functions {
        for func in &qf.functions {
            // Build initial context from explicit parameter type annotations
            let initial_types: HashMap<String, Option<DataType>> = func
                .parameters
                .iter()
                .map(|p| (p.name.clone(), p.data_type))
                .collect();

            // Emit <any> hints for unannotated parameters
            for param in &func.parameters {
                if param.data_type.is_some() || !param.position.is_valid() { continue; }
                let line = param.position.line.saturating_sub(1) as u32;
                let col  = (param.position.column.saturating_sub(1) + param.name.len()) as u32;
                hints.push(make_hint(line, col, "<any>".to_string()));
            }

            // Process body with accumulating context (returns updated context, discarded here)
            let _ = collect_qf_var_hints(
                &func.body,
                &doc.tokens,
                &qf_return_types,
                initial_types,
                symbol_table,
                &mut hints,
            );
        }
    }

    if hints.is_empty() { None } else { Some(hints) }
}

// ── Typed-collection formatting ───────────────────────────────────────────────

pub fn format_data_type_as_hint(dt: DataType, count: Option<usize>) -> String {
    match dt {
        DataType::TypedArray(elem) => match count {
            Some(n) => format!("<{}[{}]>", elem, n),
            None    => format!("<array<{}>>", elem),
        },
        DataType::TypedTuple(slots) => {
            let types: Vec<String> = slots
                .iter()
                .filter_map(|&s| s)
                .map(|e| format!("{}", e))
                .collect();
            if types.is_empty() {
                match count {
                    Some(n) => format!("<tuple[{}]>", n),
                    None    => "<tuple>".to_string(),
                }
            } else {
                format!("<tuple({})>", types.join(","))
            }
        }
        DataType::Array => match count {
            Some(n) => format!("<array[{}]>", n),
            None    => "<array>".to_string(),
        },
        DataType::Tuple => match count {
            Some(n) => format!("<tuple[{}]>", n),
            None    => "<tuple>".to_string(),
        },
        other => format!("<{}>", other),
    }
}

fn collection_len(value: &Value) -> Option<usize> {
    match value {
        Value::Array { values, .. } | Value::NestedArray { values, .. } => Some(values.len()),
        Value::PrefixedConstructor { prefix, arguments, .. }
            if prefix.eq_ignore_ascii_case("t") =>
        {
            Some(arguments.len())
        }
        _ => None,
    }
}

// ── Array / collection label helpers ─────────────────────────────────────────

fn array_label_from_values(items: &[Value], ctx: &InferCtx<'_>) -> String {
    let count = items.len();
    match uniform_base_type(items, ctx) {
        Some(base) => format!("<{}[{}]>", base, count),
        None       => format!("<any[{}]>", count),
    }
}

fn uniform_base_type(items: &[Value], ctx: &InferCtx<'_>) -> Option<String> {
    if items.is_empty() { return None; }
    let first = infer_base_type(&items[0], ctx)?;
    for v in items.iter().skip(1) {
        match infer_base_type(v, ctx) {
            Some(ref t) if *t == first => {}
            Some(_)                    => return None,
            None                       => {}
        }
    }
    Some(first)
}

fn infer_base_type(value: &Value, ctx: &InferCtx<'_>) -> Option<String> {
    match value {
        Value::Integer { .. }                                        => Some("int".to_string()),
        Value::Long { .. }                                           => Some("long".to_string()),
        Value::Float { .. }                                          => Some("float".to_string()),
        Value::Double { .. } | Value::ScientificNotation { .. }     => Some("double".to_string()),
        Value::String { .. } | Value::InterpolatedString { .. }     => Some("string".to_string()),
        Value::Boolean { .. }                                        => Some("bool".to_string()),
        Value::HexColor { .. }                                       => Some("hex".to_string()),
        Value::Date { .. }                                           => Some("date".to_string()),
        Value::Timestamp { .. }                                      => Some("timestamp".to_string()),
        Value::EnumValue { .. }                                      => Some("enum".to_string()),
        Value::Object { .. }                                         => Some("object".to_string()),
        Value::Null { .. }                                           => None,

        Value::Array { values, .. } | Value::NestedArray { values, .. } => {
            let inner = uniform_base_type(values, ctx);
            Some(match inner {
                Some(t) => format!("array<{}>", t),
                None    => "array".to_string(),
            })
        }

        Value::PrefixedConstructor { prefix, arguments, .. } => match prefix.as_str() {
            "b" => Some("blob".to_string()),
            "r" => Some("regex".to_string()),
            "t" => {
                let elem_types: Vec<String> = arguments
                    .iter()
                    .take(6)
                    .filter_map(|v| infer_base_type(v, ctx))
                    .collect();
                if elem_types.is_empty() {
                    Some("tuple".to_string())
                } else {
                    Some(format!("tuple({})", elem_types.join(",")))
                }
            }
            _ => None,
        },

        Value::QuickFuncCall { function_name, .. } => {
            resolve_func_name_type(function_name, ctx).map(|dt| {
                format!("{}", dt)
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_string()
            })
        }

        Value::Expression { expr, .. } => infer_expr(expr, ctx).map(|s| {
            s.trim_start_matches('<').trim_end_matches('>').to_string()
        }),

        Value::Identifier { value: name, .. } => ctx
            .param_types
            .get(name.as_str())
            .and_then(|opt_dt| {
                opt_dt.map(|dt| {
                    format!("{}", dt)
                        .trim_start_matches('<')
                        .trim_end_matches('>')
                        .to_string()
                })
            }),

        _ => None,
    }
}

// ── Tuple / blob label helpers ────────────────────────────────────────────────

fn tuple_label(arguments: &[Value], ctx: &InferCtx<'_>) -> String {
    let types: Vec<String> = arguments
        .iter()
        .take(6)
        .map(|v| infer_base_type(v, ctx).unwrap_or_else(|| "?".to_string()))
        .collect();
    if types.is_empty() { return "<tuple>".to_string(); }
    if types.iter().all(|t| *t == "?") { return format!("<tuple:{}>", types.len()); }
    format!("<tuple({})>", types.join(","))
}

fn blob_label(arguments: &[Value]) -> String {
    let b64 = match arguments.first() {
        Some(Value::String { value, .. })                => value.as_str(),
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
        Ok(b)  => b,
        Err(_) => return "<blob:invalid>".to_string(),
    };

    let category = sniff_blob_category(&bytes);
    let size = if bytes.len() >= 1_048_576 {
        format!("{}MB", bytes.len() / 1_048_576)
    } else if bytes.len() >= 1024 {
        format!("{}KB", bytes.len() / 1024)
    } else {
        format!("{}B", bytes.len())
    };
    format!("<blob:{}:{}>", category, size)
}

fn sniff_blob_category(b: &[u8]) -> &'static str {
    if b.len() < 4 { return "data"; }
    if b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF                                        { return "image"; }
    if b[0] == 0x89 && b[1] == 0x50 && b[2] == 0x4E && b[3] == 0x47                       { return "image"; }
    if b[0] == 0x47 && b[1] == 0x49 && b[2] == 0x46                                        { return "image"; }
    if b.len() >= 12
        && b[0] == 0x52 && b[1] == 0x49 && b[2] == 0x46 && b[3] == 0x46
        && b[8] == 0x57 && b[9] == 0x45 && b[10] == 0x42 && b[11] == 0x50                 { return "image"; }
    if b[0] == 0x42 && b[1] == 0x4D                                                         { return "image"; }
    if b[0] == 0x49 && b[1] == 0x44 && b[2] == 0x33                                        { return "audio"; }
    if b[0] == 0xFF && (b[1] & 0xE0) == 0xE0                                               { return "audio"; }
    if b[0] == 0x4F && b[1] == 0x67 && b[2] == 0x67 && b[3] == 0x53                       { return "audio"; }
    if b[0] == 0x66 && b[1] == 0x4C && b[2] == 0x61 && b[3] == 0x43                       { return "audio"; }
    if b.len() >= 12
        && b[0] == 0x52 && b[1] == 0x49 && b[2] == 0x46 && b[3] == 0x46
        && b[8] == 0x57 && b[9] == 0x41 && b[10] == 0x56 && b[11] == 0x45                { return "audio"; }
    if b.len() >= 12
        && b[0] == 0x52 && b[1] == 0x49 && b[2] == 0x46 && b[3] == 0x46
        && b[8] == 0x41 && b[9] == 0x56 && b[10] == 0x49                                  { return "video"; }
    if b[0] == 0x1A && b[1] == 0x45 && b[2] == 0xDF && b[3] == 0xA3                       { return "video"; }
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        return match &b[8..12] {
            b"M4A " | b"M4B "                                   => "audio",
            b"M4V " | b"mp42" | b"avc1" | b"isom" | b"iso2"    => "video",
            _                                                    => "video",
        };
    }
    if b[0] == 0x25 && b[1] == 0x50 && b[2] == 0x44 && b[3] == 0x46                      { return "pdf";  }
    if b[0] == 0x50 && b[1] == 0x4B                                                         { return "zip";  }
    if b[0] == 0x1F && b[1] == 0x8B                                                         { return "gzip"; }
    if b.len() >= 6
        && b[0] == 0x37 && b[1] == 0x7A && b[2] == 0xBC
        && b[3] == 0xAF && b[4] == 0x27 && b[5] == 0x1C                                   { return "7z";   }
    if b.len() >= 5
        && b[0] == 0xFD && b[1] == 0x37 && b[2] == 0x7A
        && b[3] == 0x58 && b[4] == 0x5A                                                    { return "xz";   }
    if b[0] == 0x77 && b[1] == 0x4F && b[2] == 0x46 && b[3] == 0x46                      { return "font"; }
    if b[0] == 0x77 && b[1] == 0x4F && b[2] == 0x46 && b[3] == 0x32                      { return "font"; }
    if b[0] == 0x00 && b[1] == 0x01 && b[2] == 0x00 && b[3] == 0x00                      { return "font"; }
    let is_printable = b.iter().take(64).all(|&byte| {
        byte == b'\t' || byte == b'\n' || byte == b'\r' || (0x20..=0x7E).contains(&byte)
    });
    if is_printable {
        let head    = std::str::from_utf8(&b[..b.len().min(32)]).unwrap_or("");
        let trimmed = head.trim_start();
        if trimmed.starts_with('{') || trimmed.starts_with('[') { return "json"; }
        if trimmed.starts_with('<')                              { return "xml";  }
        return "text";
    }
    "data"
}

// ── Parameter name hints for QuickFunc calls in @DATA ─────────────────────────

/// Emits `paramName:` hints before each argument in a QuickFunc call.
/// Only activates for calls with 2+ parameters to avoid noise on trivial calls.
fn emit_qf_param_name_hints(value: &Value, doc: &Document, hints: &mut Vec<InlayHint>) {
    let (function_name, arguments) = match value {
        Value::QuickFuncCall { function_name, arguments, .. } => {
            (function_name.as_str(), arguments.as_slice())
        }
        Value::Expression { expr, .. } => match expr.as_ref() {
            Expression::QuickFuncCall { name, arguments, .. } => {
                (name.as_str(), arguments.as_slice())
            }
            _ => return,
        },
        _ => return,
    };

    if arguments.is_empty() { return; }

    let qf = match doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
        Some(q) => q,
        None    => return,
    };
    let func = match qf.functions.iter().find(|f| f.name == function_name) {
        Some(f) => f,
        None    => return,
    };

    // Skip single-param calls — not worth the visual noise
    if func.parameters.len() <= 1 { return; }

    for (i, arg) in arguments.iter().enumerate() {
        let param = match func.parameters.get(i) {
            Some(p) => p,
            None    => break,
        };

        let pos = arg.position();
        if !pos.is_valid() { continue; }

        let line = pos.line.saturating_sub(1) as u32;
        let col  = pos.column.saturating_sub(1) as u32;

        hints.push(InlayHint {
            position:      Position::new(line, col),
            label:         InlayHintLabel::String(format!("{}:", param.name)),
            kind:          Some(InlayHintKind::PARAMETER),
            text_edits:    None,
            tooltip:       None,
            padding_left:  Some(false),
            padding_right: Some(true),
            data:          None,
        });
    }
}

// ── QuickFunc variable-declaration hints — accumulating context ───────────────
//
// Takes `running` by value and returns the updated map.
// Branching statements (if/switch) CLONE running before recursing so variables
// declared inside branches don't leak into siblings or the outer sequential scope.
//
// KEY BORROW FIX: we never hold an InferCtx (which borrows &running) alive across
// a running.insert() call.  Every ctx is created inside a scoped block { } so the
// borrow ends before we mutate running.

fn collect_qf_var_hints(
    stmts:           &[QuickFuncStatement],
    tokens:          &[Token],
    qf_return_types: &HashMap<String, DataType>,
    mut running:     HashMap<String, Option<DataType>>,
    symbol_table:    Option<&SymbolTable>,
    hints:           &mut Vec<InlayHint>,
) -> HashMap<String, Option<DataType>> {
    for stmt in stmts {
        // DO NOT create InferCtx here — it would borrow `running` for the whole match.
        // Instead, create it in scoped blocks inside each arm.

        match stmt {
            QuickFuncStatement::VariableDeclaration {
                variable_name,
                data_type,
                value,
                position,
                ..
            } => {
                if data_type.is_none() {
                    // ── Step 1: infer typed DataType (scoped borrow) ───────────
                    let typed_dt = {
                        let ctx = InferCtx::new(qf_return_types, &running, symbol_table);
                        build_typed_dt(value, &ctx)
                        // ctx + its &running borrow dropped here
                    };

                    // ── Step 2: build label string (scoped borrow) ────────────
                    let label = {
                        let ctx = InferCtx::new(qf_return_types, &running, symbol_table);
                        match &typed_dt {
                            Some(dt) => format_data_type_as_hint(*dt, collection_elem_count(value)),
                            None     => infer_expr(value, &ctx)
                                            .unwrap_or_else(|| "<any>".to_string()),
                        }
                        // ctx dropped here
                    };

                    // ── Step 3: emit hint (no borrows) ────────────────────────
                    if position.is_valid() {
                        let target_line = position.line;
                        let hint_line   = target_line.saturating_sub(1) as u32;

                        // Find the identifier token on this line for precise column
                        let col = tokens
                            .iter()
                            .filter(|t| t.line == target_line)
                            .find(|t| {
                                matches!(&t.token_type,
                                    TokenType::Identifier(id)
                                        if id.as_str() == variable_name.as_str())
                            })
                            .map(|tok| (tok.column.saturating_sub(1) + variable_name.len()) as u32)
                            .unwrap_or_else(|| {
                                // Fallback: past `let ` prefix
                                (position.column.saturating_sub(1) + 4 + variable_name.len()) as u32
                            });

                        hints.push(make_hint(hint_line, col, label));
                    }

                    // ── Step 4: store inferred type — running is now free ──────
                    running.insert(variable_name.clone(), typed_dt);

                    // ── Step 5: propagate object property types ────────────────
                    // We need to read from running (for the ctx) AND write to running.
                    // Fix: clone running as a snapshot so ctx borrows the clone
                    // while we mutate the original.
                    let snapshot = running.clone();
                    populate_object_property_types(
                        variable_name,
                        value,
                        qf_return_types,
                        &snapshot,
                        symbol_table,
                        &mut running,
                    );

                } else {
                    // Annotated declaration — no hint, but update context
                    running.insert(variable_name.clone(), *data_type);

                    // Still propagate object props for annotated object vars
                    let snapshot = running.clone();
                    populate_object_property_types(
                        variable_name,
                        value,
                        qf_return_types,
                        &snapshot,
                        symbol_table,
                        &mut running,
                    );
                }
            }

            // Branching: clone running so branch-local variables don't escape
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                let _ = collect_qf_var_hints(
                    then_branch, tokens, qf_return_types,
                    running.clone(), symbol_table, hints,
                );
                if let Some(eb) = else_branch {
                    let _ = collect_qf_var_hints(
                        eb, tokens, qf_return_types,
                        running.clone(), symbol_table, hints,
                    );
                }
            }

            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    let _ = collect_qf_var_hints(
                        &case.statements, tokens, qf_return_types,
                        running.clone(), symbol_table, hints,
                    );
                }
                if let Some(dc) = default_case {
                    let _ = collect_qf_var_hints(
                        &dc.statements, tokens, qf_return_types,
                        running.clone(), symbol_table, hints,
                    );
                }
            }

            _ => {}
        }
    }

    running
}

// ── Object property type propagation ─────────────────────────────────────────

/// Walk an object literal expression and register each property's inferred type
/// into `running` as `"varname.propname"` entries.
///
/// This allows subsequent statements like `let x = player.hp` to resolve the
/// type of `player.hp` through our custom path lookup in `infer_expr`.
///
/// Takes `param_types_snapshot` — a clone of `running` made by the caller —
/// so this function can borrow it immutably while also mutating `running`.
/// This avoids the simultaneous immutable+mutable borrow that would occur if
/// we passed `&InferCtx` that internally holds `&running`.
fn populate_object_property_types(
    variable_name:        &str,
    value_expr:           &Expression,
    qf_return_types:      &HashMap<String, DataType>,
    param_types_snapshot: &HashMap<String, Option<DataType>>,
    symbol_table:         Option<&SymbolTable>,
    running:              &mut HashMap<String, Option<DataType>>,
) {
    // Only handle direct object literal: let player = { hp = 100, name = "x" }
    let obj_value = match value_expr {
        Expression::Value { value, .. } => value,
        _ => return,
    };

    let properties = match obj_value {
        Value::Object { properties, .. } => properties,
        _ => return,
    };

    // Use the snapshot (does not conflict with &mut running)
    let ctx = InferCtx::new(qf_return_types, param_types_snapshot, symbol_table);

    for prop in properties {
        // Wrap each property inference in catch_unwind so one bad prop
        // doesn't prevent the rest from being registered
        let prop_dt = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            build_typed_dt_from_value(&prop.value, &ctx)
        }))
        .unwrap_or(None);

        running.insert(format!("{}.{}", variable_name, prop.key), prop_dt);
    }
}

// ── Value-level type inference ────────────────────────────────────────────────

fn infer_value(value: &Value, ctx: &InferCtx<'_>) -> Option<String> {
    match value {
        Value::Expression { expr, .. }        => infer_expr(expr, ctx),

        Value::QuickFuncCall { function_name, .. } => {
            resolve_func_name_type(function_name, ctx)
                .map(|dt| format_data_type_as_hint(dt, None))
        }

        Value::Identifier { value: name, .. } => ctx
            .param_types
            .get(name.as_str())
            .map(|opt| match *opt {
                Some(dt) => format_data_type_as_hint(dt, None),
                None     => "<any>".to_string(),
            }),

        Value::Null { .. }                           => Some("<null>".to_string()),
        Value::Integer { .. }                        => Some("<int>".to_string()),
        Value::Long { .. }                           => Some("<long>".to_string()),
        Value::Float { .. }                          => Some("<float>".to_string()),
        Value::Double { .. } | Value::ScientificNotation { .. } => Some("<double>".to_string()),
        Value::String { .. } | Value::InterpolatedString { .. } => Some("<string>".to_string()),
        Value::Boolean { .. }                        => Some("<bool>".to_string()),
        Value::HexColor { .. }                       => Some("<hex>".to_string()),
        Value::Date { .. }                           => Some("<date>".to_string()),
        Value::Timestamp { .. }                      => Some("<timestamp>".to_string()),
        Value::EnumValue { .. }                      => Some("<enum>".to_string()),
        Value::Object { properties, .. }             => {
            // Show property count for objects
            Some(format!("<object:{}>", properties.len()))
        }

        Value::Array { values, .. } | Value::NestedArray { values, .. } => {
            Some(array_label_from_values(values, ctx))
        }

        Value::PrefixedConstructor { prefix, arguments, .. } => match prefix.as_str() {
            "b" => Some(blob_label(arguments)),
            "t" => Some(tuple_label(arguments, ctx)),
            "r" => Some("<regex>".to_string()),
            _   => None,
        },

        _ => None,
    }
}

// ── Resolve a function-name string to its return DataType ─────────────────────

fn resolve_func_name_type(function_name: &str, ctx: &InferCtx<'_>) -> Option<DataType> {
    // 1. Locally-declared QuickFunc
    if let Some(&rt) = ctx.qf_return_types.get(function_name) {
        return Some(rt);
    }

    if let Some(st) = ctx.symbol_table {
        // 2. Symbol table functions
        if let Some(sig) = st.try_get_function(function_name) {
            if let Some(rt) = sig.return_type {
                return Some(rt);
            }
        }

        // 3. "Namespace.FunctionName" dotted form
        if let Some(dot_pos) = function_name.find('.') {
            let ns_name = &function_name[..dot_pos];
            let fn_name = &function_name[dot_pos + 1..];
            if let Some(info) = st.get_namespaced_function(ns_name, fn_name) {
                if let Some(rt) = info.signature.return_type {
                    return Some(rt);
                }
            }
        }
    }

    None
}

// ── Expression-level type inference ──────────────────────────────────────────

fn infer_expr(expr: &Expression, ctx: &InferCtx<'_>) -> Option<String> {
    // ── Fast path: TypeInferenceVisitor ───────────────────────────────────────
    if let Some(dt) = precise_dt(expr, ctx) {
        match dt {
            // Plain Array/Tuple/Any: let manual inference add richer info
            DataType::Array | DataType::Tuple | DataType::Any => {}
            dt => return Some(format_data_type_as_hint(dt, None)),
        }
    }

    // ── Manual / fallback inference ───────────────────────────────────────────
    match expr {
        Expression::Value { value, .. } => infer_value(value, ctx),

        Expression::Identifier { name, .. } => ctx
            .param_types
            .get(name.as_str())
            .map(|opt| match *opt {
                Some(dt) => format_data_type_as_hint(dt, None),
                None     => "<any>".to_string(),
            }),

        Expression::QuickFuncCall { name, .. } | Expression::FunctionCall { name, .. } => {
            resolve_func_name_type(name, ctx)
                .map(|dt| format_data_type_as_hint(dt, None))
        }

        Expression::ImportedFunctionCall { namespace_name, function_name, .. } => {
            ctx.symbol_table
                .and_then(|st| st.get_namespaced_function(namespace_name, function_name))
                .and_then(|info| info.signature.return_type)
                .map(|dt| format_data_type_as_hint(dt, None))
        }

        Expression::DixFunctionCall { .. } => Some("<any>".to_string()),

        Expression::QualifiedIdentifier { parts, arguments, .. } => {
            infer_qualified(parts, arguments.as_ref(), ctx)
        }

        Expression::StaticMethodCall { object_name, method_name, .. } => {
            static_return(object_name, method_name)
        }

        Expression::StaticFunction { class_name, method, .. } => {
            static_return(class_name, method)
        }

        Expression::InstanceMethodCall { instance, method_name, .. } => {
            let recv = infer_expr(instance, ctx);
            instance_return(recv.as_deref(), method_name)
        }

        Expression::BuiltinFunction { target, method, .. } => {
            let recv = infer_expr(target, ctx);
            instance_return(recv.as_deref(), method)
        }

        Expression::PropertyAccess { object, property, .. } => {
            // ── Fast path: TypeInferenceVisitor (symbol table / typed collections)
            if let Some(dt) = precise_dt(expr, ctx) {
                return match dt {
                    // Object/Any too vague — fall through to path lookup
                    DataType::Object | DataType::Any => None,
                    dt => Some(format_data_type_as_hint(dt, None)),
                };
            }

            // ── Registered object property path lookup ─────────────────────────
            // After populate_object_property_types, running contains entries like
            // "player.hp" → Some(Int).  Both single-level and nested paths work.
            if let Some(full_path) = build_property_path(expr) {
                if let Some(&Some(dt)) = ctx.param_types.get(full_path.as_str()) {
                    return Some(format_data_type_as_hint(dt, None));
                }
            }

            // ── Instance method/property fallback ──────────────────────────────
            let recv = infer_expr(object, ctx);
            instance_return(recv.as_deref(), property)
        }

        Expression::IndexAccess { .. } => None,

        Expression::ArithmeticOp { left, operator, right, .. } => {
            let lt = infer_expr(left, ctx);
            let rt = infer_expr(right, ctx);
            if operator.as_str() == "+" {
                if lt.as_deref() == Some("<string>") || rt.as_deref() == Some("<string>") {
                    return Some("<string>".to_string());
                }
            }
            match (lt.as_deref(), rt.as_deref()) {
                (Some("<double>"), _) | (_, Some("<double>")) => Some("<double>".to_string()),
                (Some("<float>"),  _) | (_, Some("<float>"))  => Some("<float>".to_string()),
                (Some("<long>"),   _) | (_, Some("<long>"))   => Some("<long>".to_string()),
                (Some("<int>"),    _) | (_, Some("<int>"))    => Some("<int>".to_string()),
                (Some(l), Some(r)) if l == r                  => Some(l.to_string()),
                (Some(l), _)                                   => Some(l.to_string()),
                (_, Some(r))                                   => Some(r.to_string()),
                _                                              => None,
            }
        }

        Expression::ComparisonOp { .. } | Expression::LogicalOp { .. } => {
            Some("<bool>".to_string())
        }

        Expression::BitwiseOp { .. } => Some("<int>".to_string()),

        Expression::UnaryOp { operator, operand, .. } => {
            if matches!(operator.as_str(), "!" | "not") {
                Some("<bool>".to_string())
            } else {
                infer_expr(operand, ctx)
            }
        }

        Expression::Conditional { true_value, false_value, .. } => {
            let t = infer_expr(true_value, ctx);
            let f = infer_expr(false_value, ctx);
            match (t, f) {
                (Some(a), Some(b)) if a == b => Some(a),
                (Some(_), Some(_))           => Some("<any>".to_string()),
                (t, f)                       => t.or(f),
            }
        }

        Expression::Parenthesized { expression, .. } => infer_expr(expression, ctx),

        Expression::TypeCast { target_type, .. } => {
            Some(format_data_type_as_hint(*target_type, None))
        }

        Expression::EnumAccess { .. }   => Some("<enum>".to_string()),
        Expression::ConfigAccess { .. } => None,
        _                               => None,
    }
}

/// Build a dotted property path string from a PropertyAccess or Identifier chain.
/// E.g. `player.stats.hp` → `Some("player.stats.hp")`.
fn build_property_path(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier { name, .. } => Some(name.clone()),
        Expression::PropertyAccess { object, property, .. } => {
            build_property_path(object).map(|base| format!("{}.{}", base, property))
        }
        _ => None,
    }
}

// ── Qualified identifier dispatch ─────────────────────────────────────────────

fn infer_qualified(
    parts:     &[String],
    arguments: Option<&Vec<Expression>>,
    ctx:       &InferCtx<'_>,
) -> Option<String> {
    if parts.is_empty() { return None; }

    if parts.len() == 1 {
        if arguments.is_some() {
            return resolve_func_name_type(&parts[0], ctx)
                .map(|dt| format_data_type_as_hint(dt, None));
        }
        return ctx.param_types
            .get(parts[0].as_str())
            .and_then(|opt| opt.map(|dt| format_data_type_as_hint(dt, None)));
    }

    let head   = &parts[0];
    let member = &parts[1];

    // Static object method: Math.sqrt, Array.range, etc.
    if head.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
        let static_result = if parts.len() >= 3 {
            static_return(&parts[1], &parts[2])
        } else {
            static_return(head, member)
        };
        if static_result.is_some() { return static_result; }
    }

    // Imported namespace function: Utils.calc(...)
    if arguments.is_some() {
        if let Some(st) = ctx.symbol_table {
            if let Some(func_info) = st.get_namespaced_function(head, member) {
                if let Some(rt) = func_info.signature.return_type {
                    return Some(format_data_type_as_hint(rt, None));
                }
            }
        }
    }

    if arguments.is_none() { return None; }

    // Variable dot method
    if let Some(opt_dt) = ctx.param_types.get(head.as_str()) {
        let recv = opt_dt.map(|dt| format_data_type_as_hint(dt, None));
        if let Some(result) = instance_return(recv.as_deref(), member) {
            return Some(result);
        }
        return recv;
    }

    // QuickFunc return type dot method
    if let Some(rt) = ctx.qf_return_types.get(head.as_str()) {
        let recv = format_data_type_as_hint(*rt, None);
        return instance_return(Some(recv.as_str()), member);
    }

    None
}

// ── Registry-backed return type lookups ──────────────────────────────────────

fn static_return(object: &str, method: &str) -> Option<String> {
    static_object_registry::get_method_info(object, method)
        .and_then(|info| dix_to_hint(info.return_type))
}

fn static_return_dt(object: &str, method: &str) -> Option<DataType> {
    static_object_registry::get_method_info(object, method)
        .and_then(|info| TypeInferenceVisitor::convert_dix_type_to_data_type(info.return_type))
}

fn instance_return(receiver_hint: Option<&str>, method: &str) -> Option<String> {
    let dix_type = hint_to_dix(receiver_hint?)?;
    instance_method_registry::initialize();
    let m = instance_method_registry::get_instance_method(dix_type, method)?;
    dix_to_hint(m.return_type())
}

// ── Type conversion helpers ───────────────────────────────────────────────────

fn hint_to_dix(hint: &str) -> Option<DixType> {
    let s = hint.trim_start_matches('<').trim_end_matches('>');
    let s = match s.find('[') { Some(pos) => &s[..pos], None => s };
    let s = match s.find('<') { Some(pos) => &s[..pos], None => s };
    let s = match s.find('(') { Some(pos) => &s[..pos], None => s };
    match s {
        "int"       => Some(DixType::Int),
        "long"      => Some(DixType::Long),
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
        DixType::Long      => Some("<long>".to_string()),
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
