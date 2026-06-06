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

// ── Precise type via TypeInferenceVisitor ─────────────────────────────────────

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
        Expression::QuickFuncCall { name, .. }
        | Expression::FunctionCall { name, .. } => {
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
        // Lambda / function value
        Value::Lambda { .. } => Some(DataType::Function),

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
            if values.is_empty() { return Some(DataType::Array); }
            let first_name = infer_base_type(&values[0], ctx)?;
            let elem       = ElemType::from_keyword(&first_name)?;
            let all_same   = values.iter().skip(1).all(|v| {
                infer_base_type(v, ctx).as_deref() == Some(first_name.as_str())
            });
            if all_same { Some(DataType::TypedArray(elem)) } else { Some(DataType::Array) }
        }
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
        Value::PrefixedConstructor { prefix, .. } => match prefix.as_str() {
            "b" => Some(DataType::Blob),
            "r" => Some(DataType::Regex),
            _   => None,
        },
        Value::QuickFuncCall { function_name, .. } => resolve_func_name_type(function_name, ctx),
        Value::Identifier { value: name, .. } => {
            ctx.param_types.get(name.as_str()).copied().flatten()
        }
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

    instance_method_registry::initialize();
    static_object_registry::initialize_static_registry();

    let symbol_table: Option<&SymbolTable> = doc
        .semantic_result.as_ref()
        .and_then(|sr| sr.symbol_table.as_ref());

    let qf_return_types: HashMap<String, DataType> = ast
        .quick_functions.as_ref()
        .map(|qf| {
            qf.functions.iter()
                .filter_map(|f| f.return_type.map(|rt| (f.name.clone(), rt)))
                .collect()
        })
        .unwrap_or_default();

    let no_params: HashMap<String, Option<DataType>> = HashMap::new();
    let base_ctx = InferCtx::new(&qf_return_types, &no_params, symbol_table);

    let mut hints: Vec<InlayHint> = Vec::new();

    // ── @DATA section ─────────────────────────────────────────────────────────
    if let Some(data) = &ast.data {
        let type_index = doc.semantic_result.as_ref().and_then(|sr| sr.type_index.as_ref());

        for entry in &data.entries {
            match entry {
                DataEntry::SimpleProperty { name, data_type, value, position } => {
                    if !position.is_valid() { continue; }

                    let label = match data_type {
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

                    // Param hints for any function calls in this value
                    // (includes function calls inside array literals via the Array arm)
                    emit_value_param_hints(value, doc, &mut hints);
                    // Nested property type hints for object literals
                    emit_data_nested_value_hints(value, &base_ctx, &mut hints);
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

                        emit_value_param_hints(&prop.value, doc, &mut hints);
                        emit_data_nested_value_hints(&prop.value, &base_ctx, &mut hints);
                    }
                }

                DataEntry::GroupArray { path, items, position } => {
                    if items.is_empty() || !position.is_valid() { continue; }
                    let label    = array_label_from_values(items, &base_ctx);
                    let path_str = path.segments.join(".");
                    let line     = position.line.saturating_sub(1) as u32;
                    let col      = (position.column.saturating_sub(1) + path_str.len()) as u32;
                    hints.push(make_hint(line, col, label));

                    for item in items {
                        emit_value_param_hints(item, doc, &mut hints);
                        emit_data_nested_value_hints(item, &base_ctx, &mut hints);
                    }
                }

                DataEntry::ObjectProperty { name, data_type, object, position } => {
                    if !position.is_valid() || data_type.is_some() { continue; }
                    let label = infer_value(object, &base_ctx)
                        .unwrap_or_else(|| "<object>".to_string());
                    let line = position.line.saturating_sub(1) as u32;
                    let col  = (position.column.saturating_sub(1) + name.len()) as u32;
                    hints.push(make_hint(line, col, label));

                    emit_value_param_hints(object, doc, &mut hints);
                    emit_data_nested_value_hints(object, &base_ctx, &mut hints);
                }
            }
        }
    }

    // ── @QUICKFUNCS section ───────────────────────────────────────────────────
    if let Some(qf) = &ast.quick_functions {
        for func in &qf.functions {
            let initial_types: HashMap<String, Option<DataType>> = func
                .parameters.iter()
                .map(|p| (p.name.clone(), p.data_type))
                .collect();

            // <any> hints for unannotated parameters
            for param in &func.parameters {
                if param.data_type.is_some() || !param.position.is_valid() { continue; }
                let line = param.position.line.saturating_sub(1) as u32;
                let col  = (param.position.column.saturating_sub(1) + param.name.len()) as u32;
                hints.push(make_hint(line, col, "<any>".to_string()));
            }

            let _ = collect_qf_var_hints(
                &func.body,
                &doc.tokens,
                &qf_return_types,
                initial_types,
                symbol_table,
                doc,
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
            let types: Vec<String> = slots.iter().filter_map(|&s| s).map(|e| format!("{}", e)).collect();
            if types.is_empty() {
                match count { Some(n) => format!("<tuple[{}]>", n), None => "<tuple>".to_string() }
            } else {
                format!("<tuple({})>", types.join(","))
            }
        }
        DataType::Array  => match count { Some(n) => format!("<array[{}]>", n),  None => "<array>".to_string() },
        DataType::Tuple  => match count { Some(n) => format!("<tuple[{}]>", n),  None => "<tuple>".to_string() },
        other => format!("<{}>", other),
    }
}

fn collection_len(value: &Value) -> Option<usize> {
    match value {
        Value::Array { values, .. } | Value::NestedArray { values, .. } => Some(values.len()),
        Value::PrefixedConstructor { prefix, arguments, .. }
            if prefix.eq_ignore_ascii_case("t") => Some(arguments.len()),
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
            Some(_) => return None,
            None    => {}
        }
    }
    Some(first)
}

fn infer_base_type(value: &Value, ctx: &InferCtx<'_>) -> Option<String> {
    match value {
        Value::Integer { .. }                                     => Some("int".to_string()),
        Value::Long { .. }                                        => Some("long".to_string()),
        Value::Float { .. }                                       => Some("float".to_string()),
        Value::Double { .. } | Value::ScientificNotation { .. }  => Some("double".to_string()),
        Value::String { .. } | Value::InterpolatedString { .. }  => Some("string".to_string()),
        Value::Boolean { .. }                                     => Some("bool".to_string()),
        Value::HexColor { .. }                                    => Some("hex".to_string()),
        Value::Date { .. }                                        => Some("date".to_string()),
        Value::Timestamp { .. }                                   => Some("timestamp".to_string()),
        Value::EnumValue { .. }                                   => Some("enum".to_string()),
        Value::Object { .. }                                      => Some("object".to_string()),
        Value::Lambda { .. }                                      => Some("function".to_string()),
        Value::Null { .. }                                        => None,
        Value::Array { values, .. } | Value::NestedArray { values, .. } => {
            let inner = uniform_base_type(values, ctx);
            Some(match inner { Some(t) => format!("array<{}>", t), None => "array".to_string() })
        }
        Value::PrefixedConstructor { prefix, arguments, .. } => match prefix.as_str() {
            "b" => Some("blob".to_string()),
            "r" => Some("regex".to_string()),
            "t" => {
                let elem_types: Vec<String> = arguments.iter().take(6)
                    .filter_map(|v| infer_base_type(v, ctx)).collect();
                if elem_types.is_empty() { Some("tuple".to_string()) }
                else { Some(format!("tuple({})", elem_types.join(","))) }
            }
            _ => None,
        },
        Value::QuickFuncCall { function_name, .. } => {
            resolve_func_name_type(function_name, ctx).map(|dt| {
                format!("{}", dt).trim_start_matches('<').trim_end_matches('>').to_string()
            })
        }
        Value::Expression { expr, .. } => infer_expr(expr, ctx).map(|s| {
            s.trim_start_matches('<').trim_end_matches('>').to_string()
        }),
        Value::Identifier { value: name, .. } => ctx.param_types.get(name.as_str())
            .and_then(|opt_dt| opt_dt.map(|dt| {
                format!("{}", dt).trim_start_matches('<').trim_end_matches('>').to_string()
            })),
        _ => None,
    }
}

fn tuple_label(arguments: &[Value], ctx: &InferCtx<'_>) -> String {
    let types: Vec<String> = arguments.iter().take(6)
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
    let bytes = general_purpose::STANDARD.decode(b64.trim())
        .or_else(|_| general_purpose::STANDARD_NO_PAD.decode(b64.trim()))
        .or_else(|_| general_purpose::URL_SAFE.decode(b64.trim()))
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(b64.trim()));
    let bytes = match bytes { Ok(b) => b, Err(_) => return "<blob:invalid>".to_string() };
    let category = sniff_blob_category(&bytes);
    let size = if bytes.len() >= 1_048_576 { format!("{}MB", bytes.len() / 1_048_576) }
               else if bytes.len() >= 1024 { format!("{}KB", bytes.len() / 1024) }
               else { format!("{}B", bytes.len()) };
    format!("<blob:{}:{}>", category, size)
}

fn sniff_blob_category(b: &[u8]) -> &'static str {
    if b.len() < 4 { return "data"; }
    if b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF { return "image"; }
    if b[0] == 0x89 && b[1] == 0x50 && b[2] == 0x4E && b[3] == 0x47 { return "image"; }
    if b[0] == 0x47 && b[1] == 0x49 && b[2] == 0x46 { return "image"; }
    if b[0] == 0x42 && b[1] == 0x4D { return "image"; }
    if b[0] == 0x49 && b[1] == 0x44 && b[2] == 0x33 { return "audio"; }
    if b[0] == 0xFF && (b[1] & 0xE0) == 0xE0 { return "audio"; }
    if b[0] == 0x4F && b[1] == 0x67 && b[2] == 0x67 && b[3] == 0x53 { return "audio"; }
    if b[0] == 0x66 && b[1] == 0x4C && b[2] == 0x61 && b[3] == 0x43 { return "audio"; }
    if b.len() >= 12 && b[0]==0x52 && b[1]==0x49 && b[2]==0x46 && b[3]==0x46
        && b[8]==0x41 && b[9]==0x56 && b[10]==0x49 { return "video"; }
    if b[0] == 0x1A && b[1] == 0x45 && b[2] == 0xDF && b[3] == 0xA3 { return "video"; }
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        return match &b[8..12] {
            b"M4A " | b"M4B " => "audio",
            _ => "video",
        };
    }
    if b[0] == 0x25 && b[1] == 0x50 && b[2] == 0x44 && b[3] == 0x46 { return "pdf"; }
    if b[0] == 0x50 && b[1] == 0x4B { return "zip"; }
    if b[0] == 0x1F && b[1] == 0x8B { return "gzip"; }
    let is_printable = b.iter().take(64).all(|&byte| {
        byte == b'\t' || byte == b'\n' || byte == b'\r' || (0x20..=0x7E).contains(&byte)
    });
    if is_printable {
        let head = std::str::from_utf8(&b[..b.len().min(32)]).unwrap_or("");
        let t = head.trim_start();
        if t.starts_with('{') || t.starts_with('[') { return "json"; }
        if t.starts_with('<') { return "xml"; }
        return "text";
    }
    "data"
}

// ═══════════════════════════════════════════════════════════════════════════════
// PARAMETER HINT SYSTEM
//
// emit_value_param_hints   — entry for Value (DATA section, group array items)
// emit_expr_param_hints    — entry for Expression (QF body statements)
// emit_param_hints_for_name — unified local→namespace→symtable lookup
// emit_param_hints_for_imported — direct namespace lookup
// emit_arg_hints           — emits the actual InlayHint objects
// ═══════════════════════════════════════════════════════════════════════════════

/// Entry point for Value-based calls (DATA section property values, group array
/// items, and regular array literals).
///
/// Handles ALL value types that can represent or contain function calls,
/// including inline arrays whose items may be function calls.
pub fn emit_value_param_hints(value: &Value, doc: &Document, hints: &mut Vec<InlayHint>) {
    match value {
        // Direct function call value (common in DATA section and group arrays)
        Value::QuickFuncCall { function_name, arguments, .. } => {
            if arguments.len() >= 2 {
                emit_param_hints_for_name(function_name, arguments, doc, hints);
            }
        }
        // Expression-wrapped call (imported calls, complex expressions)
        Value::Expression { expr, .. } => {
            emit_expr_param_hints(expr, doc, hints);
        }
        // Array literals: recurse into each item so function calls inside
        // inline arrays (e.g. `enemies = [createEnemy("Goblin", 50, 10), ...]`)
        // also receive parameter inlay hints.
        Value::Array { values, .. } | Value::NestedArray { values, .. } => {
            for item in values {
                emit_value_param_hints(item, doc, hints);
            }
        }
        _ => {}
    }
}

/// Entry point for Expression-based calls (QF body statements of all types).
/// Handles ALL expression types that can represent function calls.
pub fn emit_expr_param_hints(expr: &Expression, doc: &Document, hints: &mut Vec<InlayHint>) {
    match expr {
        // Local or generic function calls — use unified name lookup
        Expression::QuickFuncCall { name, arguments, .. }
        | Expression::FunctionCall { name, arguments, .. }
            if arguments.len() >= 2 =>
        {
            emit_param_hints_for_name(name, arguments, doc, hints);
        }

        // Explicitly resolved imported call
        Expression::ImportedFunctionCall { namespace_name, function_name, arguments, .. }
            if arguments.len() >= 2 =>
        {
            emit_param_hints_for_imported(namespace_name, function_name, arguments, doc, hints);
        }

        // Qualified identifier: could be Namespace.func(args) or local.method(args)
        // Before/after semantic enhancement; handles Utils.calc(x, y, z)
        Expression::QualifiedIdentifier { parts, arguments: Some(args), .. }
            if parts.len() == 2 && args.len() >= 2 =>
        {
            // Try as namespace.function first, then as local dotted name
            if emit_param_hints_for_imported_checked(&parts[0], &parts[1], args, doc, hints) {
                // imported lookup succeeded
            } else {
                // Fall back to local lookup with full dotted name (e.g. "alias.func")
                emit_param_hints_for_name(&parts.join("."), args, doc, hints);
            }
        }

        // Parenthesized expression — unwrap and recurse
        Expression::Parenthesized { expression, .. } => {
            emit_expr_param_hints(expression, doc, hints);
        }

        _ => {}
    }
}

/// Unified function name lookup: tries local QF → symbol table → dotted namespace.
/// This ensures both regular calls AND imported calls are handled from any context.
fn emit_param_hints_for_name(
    func_name: &str,
    arguments: &[Expression],
    doc:       &Document,
    hints:     &mut Vec<InlayHint>,
) {
    // ── 1. Try local QuickFuncs (exact name match) ────────────────────────────
    if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
        if let Some(func) = qf.functions.iter().find(|f| f.name == func_name) {
            if func.parameters.len() >= 2 {
                let param_names: Vec<&str> =
                    func.parameters.iter().map(|p| p.name.as_str()).collect();
                emit_arg_hints(arguments, &param_names, hints);
            }
            return; // Found locally — don't fall through even if 0/1 params
        }
    }

    // ── 2. Try symbol table (covers functions registered by semantic analysis) ─
    if let Some(st) = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {
        if let Some(sig) = st.try_get_function(func_name) {
            if sig.parameters.len() >= 2 {
                let param_names: Vec<&str> =
                    sig.parameters.iter().map(|p| p.name.as_str()).collect();
                emit_arg_hints(arguments, &param_names, hints);
            }
            return;
        }
    }

    // ── 3. Try dotted name as namespace.function (e.g. "Utils.calc") ──────────
    if let Some(dot_pos) = func_name.find('.') {
        let ns      = &func_name[..dot_pos];
        let fn_name = &func_name[dot_pos + 1..];
        emit_param_hints_for_imported(ns, fn_name, arguments, doc, hints);
    }
}

/// Look up an imported namespace function and emit param hints.
/// Returns true if the namespace function was found (even if 0/1 params).
fn emit_param_hints_for_imported_checked(
    namespace: &str,
    func_name: &str,
    arguments: &[Expression],
    doc:       &Document,
    hints:     &mut Vec<InlayHint>,
) -> bool {
    let st = match doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {
        Some(st) => st,
        None     => return false,
    };
    let func_info = match st.get_namespaced_function(namespace, func_name) {
        Some(fi) => fi,
        None     => return false,
    };
    if func_info.signature.parameters.len() >= 2 {
        let param_names: Vec<&str> =
            func_info.signature.parameters.iter().map(|p| p.name.as_str()).collect();
        emit_arg_hints(arguments, &param_names, hints);
    }
    true // namespace function was found
}

/// Look up an imported namespace function and emit param hints (no return value).
fn emit_param_hints_for_imported(
    namespace: &str,
    func_name: &str,
    arguments: &[Expression],
    doc:       &Document,
    hints:     &mut Vec<InlayHint>,
) {
    emit_param_hints_for_imported_checked(namespace, func_name, arguments, doc, hints);
}

/// Emit the actual `paramName:` inlay hints at each argument position.
fn emit_arg_hints(
    arguments:   &[Expression],
    param_names: &[&str],
    hints:       &mut Vec<InlayHint>,
) {
    for (i, arg) in arguments.iter().enumerate() {
        let param_name = match param_names.get(i) { Some(n) => n, None => break };
        let pos = arg.position();
        if !pos.is_valid() { continue; }
        let line = pos.line.saturating_sub(1) as u32;
        let col  = pos.column.saturating_sub(1) as u32;
        hints.push(InlayHint {
            position:      Position::new(line, col),
            label:         InlayHintLabel::String(format!("{}:", param_name)),
            kind:          Some(InlayHintKind::PARAMETER),
            text_edits:    None,
            tooltip:       None,
            padding_left:  Some(false),
            padding_right: Some(true),
            data:          None,
        });
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// QUICKFUNC BODY VARIABLE HINTS
// ═══════════════════════════════════════════════════════════════════════════════

fn collect_qf_var_hints(
    stmts:           &[QuickFuncStatement],
    tokens:          &[Token],
    qf_return_types: &HashMap<String, DataType>,
    mut running:     HashMap<String, Option<DataType>>,
    symbol_table:    Option<&SymbolTable>,
    doc:             &Document,
    hints:           &mut Vec<InlayHint>,
) -> HashMap<String, Option<DataType>> {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration {
                variable_name,
                data_type,
                value,
                position,
                ..
            } => {
                if data_type.is_none() {
                    // ── Step 1: infer typed DataType ───────────────────────────
                    let typed_dt = {
                        let ctx = InferCtx::new(qf_return_types, &running, symbol_table);
                        build_typed_dt(value, &ctx)
                    };

                    // ── Step 2: build label string ─────────────────────────────
                    let label = {
                        let ctx = InferCtx::new(qf_return_types, &running, symbol_table);

                        // Special case: lambda — show parameter signature
                        if let Expression::Value { value: Value::Lambda { parameters, .. }, .. } = value {
                            format!("<function({})>", parameters.join(", "))
                        } else {
                            let from_infer = infer_expr(value, &ctx);
                            let is_generic = |s: &str| {
                                matches!(s, "<any>" | "<array>" | "<tuple>" | "<object>" | "<function>")
                            };
                            match (&from_infer, &typed_dt) {
                                (Some(s), _) if !is_generic(s) => s.clone(),
                                (Some(s), Some(dt)) => {
                                    let formatted = format_data_type_as_hint(*dt, collection_elem_count(value));
                                    if !is_generic(&formatted) { formatted } else { s.clone() }
                                }
                                (None, Some(dt)) => format_data_type_as_hint(*dt, collection_elem_count(value)),
                                (None, None)     => "<any>".to_string(),
                                (Some(s), None)  => s.clone(),
                            }
                        }
                    };

                    // ── Step 3: emit type hint ─────────────────────────────────
                    if position.is_valid() {
                        let target_line = position.line;
                        let hint_line   = target_line.saturating_sub(1) as u32;
                        let col = tokens
                            .iter()
                            .filter(|t| t.line == target_line)
                            .find(|t| matches!(&t.token_type,
                                TokenType::Identifier(id) if id.as_str() == variable_name.as_str()))
                            .map(|tok| (tok.column.saturating_sub(1) + variable_name.len()) as u32)
                            .unwrap_or_else(|| {
                                (position.column.saturating_sub(1) + 4 + variable_name.len()) as u32
                            });
                        hints.push(make_hint(hint_line, col, label));
                    }

                    // ── Step 4: param hints for function calls in value ────────
                    emit_expr_param_hints(value, doc, hints);

                    // ── Step 5: nested object property hints ───────────────────
                    {
                        let ctx = InferCtx::new(qf_return_types, &running, symbol_table);
                        if let Expression::Value { value: inner_val, .. } = value {
                            emit_data_nested_value_hints(inner_val, &ctx, hints);
                        }
                    }

                    // ── Step 6: store inferred type ────────────────────────────
                    running.insert(variable_name.clone(), typed_dt);

                    // ── Step 7: propagate object property types ────────────────
                    let snapshot = running.clone();
                    populate_object_property_types(
                        variable_name, value, qf_return_types,
                        &snapshot, symbol_table, &mut running,
                    );

                } else {
                    // Annotated declaration
                    running.insert(variable_name.clone(), *data_type);
                    emit_expr_param_hints(value, doc, hints);
                    {
                        let ctx = InferCtx::new(qf_return_types, &running, symbol_table);
                        if let Expression::Value { value: inner_val, .. } = value {
                            emit_data_nested_value_hints(inner_val, &ctx, hints);
                        }
                    }
                    let snapshot = running.clone();
                    populate_object_property_types(
                        variable_name, value, qf_return_types,
                        &snapshot, symbol_table, &mut running,
                    );
                }
            }

            // Non-declaration statements: emit param hints only
            QuickFuncStatement::Assignment { value, .. }
            | QuickFuncStatement::ArithmeticAssignment { value, .. } => {
                emit_expr_param_hints(value, doc, hints);
            }

            QuickFuncStatement::ExpressionStatement { expression, .. } => {
                emit_expr_param_hints(expression, doc, hints);
            }

            QuickFuncStatement::Log { value, .. }
            | QuickFuncStatement::Return { value, .. } => {
                emit_expr_param_hints(value, doc, hints);
            }

            // Branching: clone running so branch-local variables don't escape into siblings
            QuickFuncStatement::If { condition, then_branch, else_branch, .. } => {
                emit_expr_param_hints(condition, doc, hints);
                let _ = collect_qf_var_hints(
                    then_branch, tokens, qf_return_types,
                    running.clone(), symbol_table, doc, hints,
                );
                if let Some(eb) = else_branch {
                    let _ = collect_qf_var_hints(
                        eb, tokens, qf_return_types,
                        running.clone(), symbol_table, doc, hints,
                    );
                }
            }

            QuickFuncStatement::Switch { expression, cases, default_case, .. } => {
                emit_expr_param_hints(expression, doc, hints);
                for case in cases {
                    let _ = collect_qf_var_hints(
                        &case.statements, tokens, qf_return_types,
                        running.clone(), symbol_table, doc, hints,
                    );
                }
                if let Some(dc) = default_case {
                    let _ = collect_qf_var_hints(
                        &dc.statements, tokens, qf_return_types,
                        running.clone(), symbol_table, doc, hints,
                    );
                }
            }

            QuickFuncStatement::ObjectCreation { object, .. } => {
                let ctx = InferCtx::new(qf_return_types, &running, symbol_table);
                emit_data_nested_value_hints(object, &ctx, hints);
            }
        }
    }

    running
}

// ── Object property type propagation ─────────────────────────────────────────

fn populate_object_property_types(
    variable_name:        &str,
    value_expr:           &Expression,
    qf_return_types:      &HashMap<String, DataType>,
    param_types_snapshot: &HashMap<String, Option<DataType>>,
    symbol_table:         Option<&SymbolTable>,
    running:              &mut HashMap<String, Option<DataType>>,
) {
    let obj_value = match value_expr {
        Expression::Value { value, .. } => value,
        _ => return,
    };
    populate_object_property_types_for_value(
        variable_name, obj_value, qf_return_types,
        param_types_snapshot, symbol_table, running, 0,
    );
}

fn populate_object_property_types_for_value(
    prefix:          &str,
    obj_value:       &Value,
    qf_return_types: &HashMap<String, DataType>,
    snapshot:        &HashMap<String, Option<DataType>>,
    symbol_table:    Option<&SymbolTable>,
    running:         &mut HashMap<String, Option<DataType>>,
    depth:           usize,
) {
    if depth > 4 { return; }
    let properties = match obj_value {
        Value::Object { properties, .. } => properties,
        _ => return,
    };
    let ctx = InferCtx::new(qf_return_types, snapshot, symbol_table);
    for prop in properties {
        let prop_dt = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            build_typed_dt_from_value(&prop.value, &ctx)
        })).unwrap_or(None);
        let key = format!("{}.{}", prefix, prop.key);
        running.insert(key.clone(), prop_dt);
        if matches!(prop.value, Value::Object { .. }) {
            let new_snapshot = running.clone();
            populate_object_property_types_for_value(
                &key, &prop.value, qf_return_types,
                &new_snapshot, symbol_table, running, depth + 1,
            );
        }
    }
}

// ── DATA section nested value hints ──────────────────────────────────────────

/// Emit type hints for all properties inside an object literal (recursively).
/// Used for both DATA section and QF body object literals.
fn emit_data_nested_value_hints(
    value: &Value,
    ctx:   &InferCtx<'_>,
    hints: &mut Vec<InlayHint>,
) {
    emit_data_nested_value_hints_depth(value, ctx, hints, 0);
}

fn emit_data_nested_value_hints_depth(
    value: &Value,
    ctx:   &InferCtx<'_>,
    hints: &mut Vec<InlayHint>,
    depth: usize,
) {
    if depth > 4 { return; }
    if let Value::Object { properties, .. } = value {
        for prop in properties {
            if !prop.position.is_valid() { continue; }
            let label = infer_value(&prop.value, ctx).unwrap_or_else(|| "<any>".to_string());
            let line = prop.position.line.saturating_sub(1) as u32;
            let col  = (prop.position.column.saturating_sub(1) + prop.key.len()) as u32;
            hints.push(make_hint(line, col, label));
            emit_data_nested_value_hints_depth(&prop.value, ctx, hints, depth + 1);
        }
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
        Value::Identifier { value: name, .. } => ctx.param_types.get(name.as_str())
            .map(|opt| match *opt {
                Some(dt) => format_data_type_as_hint(dt, None),
                None     => "<any>".to_string(),
            }),

        // Lambda — show parameter signature
        Value::Lambda { parameters, .. } => {
            Some(format!("<function({})>", parameters.join(", ")))
        }

        Value::Null { .. }                                       => Some("<null>".to_string()),
        Value::Integer { .. }                                    => Some("<int>".to_string()),
        Value::Long { .. }                                       => Some("<long>".to_string()),
        Value::Float { .. }                                      => Some("<float>".to_string()),
        Value::Double { .. } | Value::ScientificNotation { .. } => Some("<double>".to_string()),
        Value::String { .. } | Value::InterpolatedString { .. } => Some("<string>".to_string()),
        Value::Boolean { .. }                                    => Some("<bool>".to_string()),
        Value::HexColor { .. }                                   => Some("<hex>".to_string()),
        Value::Date { .. }                                       => Some("<date>".to_string()),
        Value::Timestamp { .. }                                  => Some("<timestamp>".to_string()),
        Value::EnumValue { .. }                                  => Some("<enum>".to_string()),
        Value::Object { properties, .. }                         => {
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
    if let Some(&rt) = ctx.qf_return_types.get(function_name) {
        return Some(rt);
    }
    if let Some(st) = ctx.symbol_table {
        if let Some(sig) = st.try_get_function(function_name) {
            if let Some(rt) = sig.return_type { return Some(rt); }
        }
        if let Some(dot_pos) = function_name.find('.') {
            let ns_name = &function_name[..dot_pos];
            let fn_name = &function_name[dot_pos + 1..];
            if let Some(info) = st.get_namespaced_function(ns_name, fn_name) {
                if let Some(rt) = info.signature.return_type { return Some(rt); }
            }
        }
    }
    None
}

// ── Expression-level type inference ──────────────────────────────────────────

fn infer_expr(expr: &Expression, ctx: &InferCtx<'_>) -> Option<String> {
    // Fast path: TypeInferenceVisitor
    if let Some(dt) = precise_dt(expr, ctx) {
        match dt {
            DataType::Array | DataType::Tuple | DataType::Any => {}
            DataType::Function => {
                if let Expression::Value { value: Value::Lambda { parameters, .. }, .. } = expr {
                    return Some(format!("<function({})>", parameters.join(", ")));
                }
                return Some("<function>".to_string());
            }
            dt => return Some(format_data_type_as_hint(dt, None)),
        }
    }

    match expr {
        Expression::Value { value, .. } => infer_value(value, ctx),

        Expression::Identifier { name, .. } => ctx.param_types.get(name.as_str())
            .map(|opt| match *opt {
                Some(dt) => format_data_type_as_hint(dt, None),
                None     => "<any>".to_string(),
            }),

        Expression::QuickFuncCall { name, .. }
        | Expression::FunctionCall { name, .. } => {
            resolve_func_name_type(name, ctx).map(|dt| format_data_type_as_hint(dt, None))
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
            if let Some(dt) = precise_dt(expr, ctx) {
                return match dt {
                    DataType::Object | DataType::Any => None,
                    dt => Some(format_data_type_as_hint(dt, None)),
                };
            }
            if let Some(full_path) = build_property_path(expr) {
                if let Some(&Some(dt)) = ctx.param_types.get(full_path.as_str()) {
                    return Some(format_data_type_as_hint(dt, None));
                }
            }
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
        return ctx.param_types.get(parts[0].as_str())
            .and_then(|opt| opt.map(|dt| format_data_type_as_hint(dt, None)));
    }

    let head   = &parts[0];
    let member = &parts[1];

    // Static object
    if head.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
        let static_result = if parts.len() >= 3 {
            static_return(&parts[1], &parts[2])
        } else {
            static_return(head, member)
        };
        if static_result.is_some() { return static_result; }
    }

    // Imported namespace function
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

    if let Some(opt_dt) = ctx.param_types.get(head.as_str()) {
        let recv = opt_dt.map(|dt| format_data_type_as_hint(dt, None));
        if let Some(result) = instance_return(recv.as_deref(), member) {
            return Some(result);
        }
        return recv;
    }

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
