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
/// Using a single struct avoids adding 3+ parameters to every call.
struct InferCtx<'a> {
    /// Return types of locally-declared QuickFuncs — fast path, no ST needed.
    qf_return_types: &'a HashMap<String, DataType>,
    /// Param / local-var types for the active QuickFunc scope; empty at @DATA.
    param_types:     &'a HashMap<String, Option<DataType>>,
    /// Full semantic symbol table — present once analysis has run.
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

/// Use the full compiler TypeInferenceVisitor to get a precise `DataType` for
/// an expression.  Handles TypedArray/TypedTuple element methods, imported
/// function return types, and symbol-table property paths.
///
/// Returns `None` when no symbol table is available or when the visitor
/// cannot determine a concrete type.
fn precise_dt(expr: &Expression, ctx: &InferCtx<'_>) -> Option<DataType> {
    let st = ctx.symbol_table?;
    // Clone the param map so the visitor owns it (cheap: small map).
    let local_vars = Some(ctx.param_types.clone());
    let visitor    = TypeInferenceVisitor::new(st, local_vars);
    visitor.infer_type_from_expression(expr)
}

// ── Main provider ─────────────────────────────────────────────────────────────

fn provide_inner(doc: Option<&Document>) -> Option<Vec<InlayHint>> {
    let doc = doc?;
    let ast = doc.ast.as_ref()?;

    instance_method_registry::initialize();
    static_object_registry::initialize_static_registry();

    let symbol_table: Option<&SymbolTable> = doc
        .semantic_result
        .as_ref()
        .and_then(|sr| sr.symbol_table.as_ref());

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

    // ── @DATA ─────────────────────────────────────────────────────────────────
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
                        Some(DataType::TypedArray(_)) | Some(DataType::TypedTuple(_)) => continue,
                        Some(dt @ DataType::Array) | Some(dt @ DataType::Tuple) => {
                            let count = collection_len(value);
                            match count {
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
                }

                DataEntry::TableProperty { properties, .. } => {
                    for prop in properties {
                        if !prop.position.is_valid() { continue; }

                        let label = match prop.data_type {
                            Some(DataType::TypedArray(_)) | Some(DataType::TypedTuple(_)) => continue,
                            Some(dt @ DataType::Array) | Some(dt @ DataType::Tuple) => {
                                let count = collection_len(&prop.value);
                                match count {
                                    Some(n) if n > 0 => format_data_type_as_hint(dt, Some(n)),
                                    _                => continue,
                                }
                            }
                            Some(_) => continue,
                            None => infer_value(&prop.value, &base_ctx)
                                .unwrap_or_else(|| "<any>".to_string()),
                        };

                        let line = prop.position.line.saturating_sub(1) as u32;
                        let col  = (prop.position.column.saturating_sub(1)
                            + prop.name.len()) as u32;
                        hints.push(make_hint(line, col, label));
                    }
                }

                DataEntry::GroupArray { path, items, position } => {
                    if items.is_empty() || !position.is_valid() { continue; }
                    let label    = array_label_from_values(items, &base_ctx);
                    let path_str = path.segments.join(".");
                    let line     = position.line.saturating_sub(1) as u32;
                    let col      = (position.column.saturating_sub(1)
                        + path_str.len()) as u32;
                    hints.push(make_hint(line, col, label));
                }

                // ── ObjectProperty (post-enhancement) ─────────────────────────
                // After AST enhancement the `object` box holds the resolved Value:
                //   Value::QuickFuncCall    → local QuickFunc result type
                //   Value::Expression { ImportedFunctionCall } → imported fn type (via ST)
                //   Value::Object           → plain <object> literal
                // Only emit a hint when no explicit annotation is in the source.
                DataEntry::ObjectProperty { name, data_type, object, position } => {
                    if !position.is_valid() { continue; }
                    if data_type.is_some() { continue; }

                    let label = infer_value(object, &base_ctx)
                        .unwrap_or_else(|| "<object>".to_string());

                    let line = position.line.saturating_sub(1) as u32;
                    let col  = (position.column.saturating_sub(1) + name.len()) as u32;
                    hints.push(make_hint(line, col, label));
                }
            }
        }
    }

    // ── @QUICKFUNCS ───────────────────────────────────────────────────────────
    if let Some(qf) = &ast.quick_functions {
        for func in &qf.functions {
            let param_types: HashMap<String, Option<DataType>> = func
                .parameters
                .iter()
                .map(|p| (p.name.clone(), p.data_type))
                .collect();

            let func_ctx = InferCtx::new(&qf_return_types, &param_types, symbol_table);

            for param in &func.parameters {
                if param.data_type.is_some() || !param.position.is_valid() { continue; }
                let line = param.position.line.saturating_sub(1) as u32;
                let col  = (param.position.column.saturating_sub(1)
                    + param.name.len()) as u32;
                hints.push(make_hint(line, col, "<any>".to_string()));
            }

            collect_qf_var_hints(&func.body, &doc.tokens, &func_ctx, &mut hints);
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
            resolve_func_name_type(function_name, ctx)
                .map(|dt| {
                    format!("{}", dt)
                        .trim_start_matches('<')
                        .trim_end_matches('>')
                        .to_string()
                })
        }

        // After AST enhancement, imported function calls live inside
        // Value::Expression { expr: ImportedFunctionCall }.
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
    if types.iter().all(|t| t == "?") { return format!("<tuple:{}>", types.len()); }
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
    let size = if bytes.len() >= 1024 * 1024 {
        format!("{}MB", bytes.len() / (1024 * 1024))
    } else if bytes.len() >= 1024 {
        format!("{}KB", bytes.len() / 1024)
    } else {
        format!("{}B", bytes.len())
    };
    format!("<blob:{}:{}>", category, size)
}

fn sniff_blob_category(b: &[u8]) -> &'static str {
    if b.len() < 4 { return "data"; }
    if b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF                                         { return "image"; }
    if b[0] == 0x89 && b[1] == 0x50 && b[2] == 0x4E && b[3] == 0x47                        { return "image"; }
    if b[0] == 0x47 && b[1] == 0x49 && b[2] == 0x46                                         { return "image"; }
    if b.len() >= 12
        && b[0] == 0x52 && b[1] == 0x49 && b[2] == 0x46 && b[3] == 0x46
        && b[8] == 0x57 && b[9] == 0x45 && b[10] == 0x42 && b[11] == 0x50                  { return "image"; }
    if b[0] == 0x42 && b[1] == 0x4D                                                          { return "image"; }
    if b[0] == 0x49 && b[1] == 0x44 && b[2] == 0x33                                         { return "audio"; }
    if b[0] == 0xFF && (b[1] & 0xE0) == 0xE0                                                { return "audio"; }
    if b[0] == 0x4F && b[1] == 0x67 && b[2] == 0x67 && b[3] == 0x53                        { return "audio"; }
    if b[0] == 0x66 && b[1] == 0x4C && b[2] == 0x61 && b[3] == 0x43                        { return "audio"; }
    if b.len() >= 12
        && b[0] == 0x52 && b[1] == 0x49 && b[2] == 0x46 && b[3] == 0x46
        && b[8] == 0x57 && b[9] == 0x41 && b[10] == 0x56 && b[11] == 0x45                 { return "audio"; }
    if b.len() >= 12
        && b[0] == 0x52 && b[1] == 0x49 && b[2] == 0x46 && b[3] == 0x46
        && b[8] == 0x41 && b[9] == 0x56 && b[10] == 0x49                                   { return "video"; }
    if b[0] == 0x1A && b[1] == 0x45 && b[2] == 0xDF && b[3] == 0xA3                        { return "video"; }
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        return match &b[8..12] {
            b"M4A " | b"M4B "                                          => "audio",
            b"M4V " | b"mp42" | b"avc1" | b"isom" | b"iso2"           => "video",
            _                                                           => "video",
        };
    }
    if b[0] == 0x25 && b[1] == 0x50 && b[2] == 0x44 && b[3] == 0x46                       { return "pdf";  }
    if b[0] == 0x50 && b[1] == 0x4B                                                          { return "zip";  }
    if b[0] == 0x1F && b[1] == 0x8B                                                          { return "gzip"; }
    if b.len() >= 6
        && b[0] == 0x37 && b[1] == 0x7A && b[2] == 0xBC
        && b[3] == 0xAF && b[4] == 0x27 && b[5] == 0x1C                                    { return "7z";   }
    if b.len() >= 5
        && b[0] == 0xFD && b[1] == 0x37 && b[2] == 0x7A
        && b[3] == 0x58 && b[4] == 0x5A                                                     { return "xz";   }
    if b[0] == 0x77 && b[1] == 0x4F && b[2] == 0x46 && b[3] == 0x46                       { return "font"; }
    if b[0] == 0x77 && b[1] == 0x4F && b[2] == 0x46 && b[3] == 0x32                       { return "font"; }
    if b[0] == 0x00 && b[1] == 0x01 && b[2] == 0x00 && b[3] == 0x00                       { return "font"; }
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

// ── QuickFunc variable-declaration hints ─────────────────────────────────────

fn collect_qf_var_hints(
    stmts:  &[QuickFuncStatement],
    tokens: &[Token],
    ctx:    &InferCtx<'_>,
    hints:  &mut Vec<InlayHint>,
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
                if data_type.is_some() { continue; }

                let label = infer_expr(value, ctx)
                    .unwrap_or_else(|| "<any>".to_string());

                let target_line = position.line;
                let hint_line   = target_line.saturating_sub(1) as u32;

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
                        (position.column.saturating_sub(1) + 4 + variable_name.len()) as u32
                    });

                hints.push(make_hint(hint_line, col, label));
            }

            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                collect_qf_var_hints(then_branch, tokens, ctx, hints);
                if let Some(else_stmts) = else_branch {
                    collect_qf_var_hints(else_stmts, tokens, ctx, hints);
                }
            }

            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    collect_qf_var_hints(&case.statements, tokens, ctx, hints);
                }
                if let Some(dc) = default_case {
                    collect_qf_var_hints(&dc.statements, tokens, ctx, hints);
                }
            }

            _ => {}
        }
    }
}

// ── Value-level type inference ────────────────────────────────────────────────

fn infer_value(value: &Value, ctx: &InferCtx<'_>) -> Option<String> {
    match value {
        // Post-enhancement: qualified/imported calls live in Expression wrapper.
        Value::Expression { expr, .. } => infer_expr(expr, ctx),

        // Direct QuickFunc call. Covers three sub-cases:
        //   1. Plain local func: "createWeapon"  → qf_return_types
        //   2. In symbol-table:  "calc"           → ST try_get_function
        //   3. Dotted pre-enh:   "Utils.calc"     → ST namespace lookup
        Value::QuickFuncCall { function_name, .. } => {
            resolve_func_name_type(function_name, ctx)
                .map(|dt| format_data_type_as_hint(dt, None))
        }

        Value::Identifier { value: name, .. } => ctx
            .param_types
            .get(name.as_str())
            .map(|opt_dt| match *opt_dt {
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
        Value::Object { .. }                         => Some("<object>".to_string()),

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

/// Handles three patterns for `Value::QuickFuncCall { function_name }`:
///   1. Plain local name   → qf_return_types
///   2. In symbol table    → ST functions map
///   3. "ns.func" pattern  → ST namespace map (pre-enhancement dotted name)
fn resolve_func_name_type(function_name: &str, ctx: &InferCtx<'_>) -> Option<DataType> {
    // 1. Fast path: locally-declared QuickFunc
    if let Some(&rt) = ctx.qf_return_types.get(function_name) {
        return Some(rt);
    }

    if let Some(st) = ctx.symbol_table {
        // 2. Symbol table general functions
        if let Some(sig) = st.try_get_function(function_name) {
            if let Some(rt) = sig.return_type {
                return Some(rt);
            }
        }

        // 3. "Namespace.FunctionName" dotted form (pre-enhancement or parser artefact)
        if let Some(dot_pos) = function_name.find('.') {
            let ns_name  = &function_name[..dot_pos];
            let fn_name  = &function_name[dot_pos + 1..];
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
    // This handles typed-collection element methods (tuple.first → <int>),
    // imported-function return types (Utils.calc → <string>), symbol-table
    // property paths (weapon.damage), and more — with full compiler semantics.
    //
    // We skip the result only for plain Array/Tuple/Any where our manual
    // inference below produces richer output (element-type labels, counts).
    if let Some(dt) = precise_dt(expr, ctx) {
        match dt {
            // Let manual inference add element-type and count info.
            DataType::Array | DataType::Tuple | DataType::Any => {}
            // For every other type — including TypedArray(T), TypedTuple([…]),
            // ImportedFunctionCall return types, etc. — use the visitor result.
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

        // Local or symbol-table QuickFunc call
        Expression::QuickFuncCall { name, .. } | Expression::FunctionCall { name, .. } => {
            resolve_func_name_type(name, ctx)
                .map(|dt| format_data_type_as_hint(dt, None))
        }

        // ── Post-enhancement: imported namespace function call ─────────────────
        // e.g. Utils.calc(x) → ImportedFunctionCall after the enhancer resolves
        // the QualifiedIdentifier.  The visitor fast-path above should catch this
        // when the ST is available; this is the explicit fallback.
        Expression::ImportedFunctionCall { namespace_name, function_name, .. } => {
            ctx.symbol_table
                .and_then(|st| st.get_namespaced_function(namespace_name, function_name))
                .and_then(|info| info.signature.return_type)
                .map(|dt| format_data_type_as_hint(dt, None))
        }

        Expression::DixFunctionCall { .. } => Some("<any>".to_string()),

        // QualifiedIdentifier may appear in pre-enhancement AST or when the
        // enhancer couldn't fully resolve (analysis still in progress).
        Expression::QualifiedIdentifier { parts, arguments, .. } => {
            infer_qualified(parts, arguments.as_ref(), ctx)
        }

        Expression::StaticMethodCall { object_name, method_name, .. } => {
            static_return(object_name, method_name)
        }

        Expression::StaticFunction { class_name, method, .. } => {
            static_return(class_name, method)
        }

        // Instance method calls fall back to registry after the visitor
        // fast-path above (which handles typed-collection element methods).
        Expression::InstanceMethodCall { instance, method_name, .. } => {
            let recv = infer_expr(instance, ctx);
            instance_return(recv.as_deref(), method_name)
        }

        Expression::BuiltinFunction { target, method, .. } => {
            let recv = infer_expr(target, ctx);
            instance_return(recv.as_deref(), method)
        }

        Expression::PropertyAccess { object, property, .. } => {
            // visitor fast-path handles symbol-table paths; fallback to registry.
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

// ── Qualified identifier dispatch ─────────────────────────────────────────────

/// Type inference for `QualifiedIdentifier` expressions — used for pre-enhancement
/// AST nodes or when the enhancer hasn't yet resolved a call.
fn infer_qualified(
    parts:     &[String],
    arguments: Option<&Vec<Expression>>,
    ctx:       &InferCtx<'_>,
) -> Option<String> {
    if parts.is_empty() { return None; }

    // ── Single-part: plain function call (e.g. createWeapon(...)) ─────────────
    // The parser may emit single-element QualifiedIdentifier for unresolved calls
    // inside QuickFunc bodies.
    if parts.len() == 1 {
        if arguments.is_some() {
            return resolve_func_name_type(&parts[0], ctx)
                .map(|dt| format_data_type_as_hint(dt, None));
        }
        // Single-part property/variable reference — look up in param types.
        return ctx.param_types
            .get(parts[0].as_str())
            .and_then(|opt_dt| opt_dt.map(|dt| format_data_type_as_hint(dt, None)));
    }

    let head   = &parts[0];
    let member = &parts[1];

    // ── Static object (uppercase head): Math.sqrt, DateTime.now … ─────────────
    if head.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
        // Check static registry first (Math, DateTime, etc.)
        if let Some(result) = if parts.len() >= 3 {
            static_return(&parts[1], &parts[2])
        } else {
            static_return(head, member)
        } {
            return Some(result);
        }
        // Not a static object — fall through to namespace check.
        // (Imported namespace aliases often start with uppercase.)
    }

    // ── Imported namespace function: Utils.calc(...) ───────────────────────────
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

    // ── Parameter or local variable with method call ────────────────────────────
    if let Some(opt_dt) = ctx.param_types.get(head.as_str()) {
        let recv = opt_dt.map(|dt| format_data_type_as_hint(dt, None));
        if let Some(result) = instance_return(recv.as_deref(), member) {
            return Some(result);
        }
        return recv;
    }

    // ── QuickFunc return value with method chain ────────────────────────────────
    if let Some(rt) = ctx.qf_return_types.get(head.as_str()) {
        let recv = format_data_type_as_hint(*rt, None);
        return instance_return(Some(recv.as_str()), member);
    }

    None
}

// ── Registry-backed return type lookups ──────────────────────────────────────

fn static_return(object: &str, method: &str) -> Option<String> {
    let info = static_object_registry::get_method_info(object, method)?;
    dix_to_hint(info.return_type)
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
    // Strip count suffix: "int[3]" → "int"
    let s = match s.find('[') { Some(pos) => &s[..pos], None => s };
    // Strip typed-collection inner: "array<int>" → "array"
    let s = match s.find('<') { Some(pos) => &s[..pos], None => s };
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
