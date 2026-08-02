//! Hover provider.
//!
//! ## Changes from previous version
//! - Static signature tables moved to hover_data.rs
//! - Chained method calls now resolve correctly via infer_close_paren_dix_type
//! - Control-flow identifiers (log, if, elif, chk) emitted as Identifier tokens
//!   are now detected by looking at the following ControlFlowColon token
//! - DATA table/group-array properties show richer sub-item information
//! - hover_table_path_prefix distinguishes tables from group arrays

use std::collections::HashMap;
use std::panic;

use dixscript::Builtins::Core::DixType;
use dixscript::Builtins::Resolver::{instance_method_registry, static_object_registry};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::AST::{
    DataType, ElemType, Expression, QuickFuncStatement, TypeInferenceVisitor, Value,
};
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::document::Document;
use crate::features::hover_data::{INSTANCE_METHOD_SIGS, STATIC_SIGS};
use crate::features::local_scope;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>, pos: Position) -> Option<Hover> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc, pos)));
    match result {
        Ok(hover) => hover,
        Err(payload) => {
            let msg = payload.downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("Hover panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>, pos: Position) -> Option<Hover> {
    let doc = doc?;

    if doc.pos_in_config(pos) {
        return hover_config_line(doc, pos);
    }

    let (token, index) = token_and_index_at(&doc.tokens, pos)?;
    let content = hover_content_for(token, index, doc)?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: None,
    })
}

// ── Token dispatch ─────────────────────────────────────────────────────────────

fn hover_content_for(token: &Token, index: usize, doc: &Document) -> Option<String> {
    match &token.token_type {
        TokenType::SectionConfig => Some(section_hover("@CONFIG",
            "Compiler settings and file metadata.",
            "@CONFIG(\n  version -> \"1.0.0\"\n  author -> \"name\"\n  debug_mode -> \"off\"\n  error_handling -> \"halt\"\n  compatibility_mode -> \"strict\"\n  features -> \"advanced\"\n)",
            "All entries use `key -> value` syntax.\n\n**Keys:** `version`, `author`, `created`, `encoding`, `debug_mode` (`off`/`regular`/`verbose`), `error_handling` (`halt`/`continue`/`recover`), `compatibility_mode` (`strict`/`best_effort`/`permissive`), `features` (`basic`/`advanced`)."
        )),
        TokenType::SectionImports => Some(section_hover("@IMPORTS",
            "Import other `.mdix` files.",
            "@IMPORTS(\n  Utils from \"common/utils.mdix\"\n  Base  from_cloud \"https://example.com/base.mdix\"\n)",
            "The alias becomes a namespace. Call: `Utils.myFunc(x)`. Access enums: `Utils.Status.ACTIVE`.\n\nOptional: `verify \"hash\"` to check file integrity."
        )),
        TokenType::SectionDLM => Some(section_hover("@DLM",
            "Data Lifecycle Modules — applied at compile time.",
            "@DLM(\n  DCompressor.gzip\n  DEncryptor.aes256\n)",
            "**Compressors:** `DCompressor.gzip`, `.bzip2`, `.lzma`\n\n**Encryptors:** `DEncryptor.aes256`, `.aes128`, `.chacha20`, `.xor`\n\n**Auditor:** `DAuditor.diy`, `.enhanced`\n\nIf `DEncryptor` is present, `@SECURITY` is required."
        )),
        TokenType::SectionEnums => Some(section_hover("@ENUMS",
            "Named integer constants.",
            "@ENUMS(\n  Difficulty { EASY = 0, NORMAL = 1, HARD = 2 }\n  AIType     { PASSIVE, NEUTRAL, AGGRESSIVE, BOSS }\n)",
            "Values auto-increment from 0 if omitted. Access with `EnumName.FIELD`. Annotate variables `<enum>` to enable enum access."
        )),
        TokenType::SectionQuickFuncs => Some(section_hover("@QUICKFUNCS",
            "Compile-time functions — zero runtime overhead.",
            "@QUICKFUNCS(\n  ~weapon<object>(id, damage<int>) {\n    return {\n      id     = id\n      damage = damage\n      range  = damage * 2\n    }\n  }\n)",
            "All computation happens at compile time.\n\n**Syntax:** `~name<returnType>(params) { ... return expr }`\n\n**Statements:** `let`, `let mut`, `const`, `if:`, `elif:`, `else`, `chk:`, `return`, `log:`"
        )),
        TokenType::SectionData => Some(section_hover("@DATA",
            "Data payload — the main output of the file.",
            "@DATA(\n  // Flat properties (single =)\n  app_name = \"MyApp\"\n  port<int> = 8080\n\n  // Table property (single :)\n  server: host = \"localhost\", port = 8080\n\n  // Group array (double ::)\n  tags:: \"alpha\", \"beta\", \"v1\"\n)",
            "**Two-tier ordering rule:** flat properties must come before any table/group entries.\n\nCommas between entries are optional."
        )),
        TokenType::SectionSecurity => Some(section_hover("@SECURITY",
            "Encryption configuration.",
            "@SECURITY(\n  encryption -> {\n    mode = \"keyfile\",\n    algorithm = \"aes256-gcm\"\n  }\n)",
            "**Modes:** `\"keyfile\"` (auto-generated `.key` file), `\"password\"` (prompted at compile time), `\"manual\"` (you supply `key`/`iv` — ⚠️ plaintext in source)\n\n**Algorithms:** `\"aes256-gcm\"`, `\"aes128-gcm\"`, `\"chacha20-poly1305\"`, `\"aes256\"`, `\"aes128\"`, `\"chacha20\"`, `\"xor\"` (⚠️ obfuscation only)\n\n**Blocks:** `encryption`, `validation`, `keystore`, `override`, `metadata`\n\nCompile: `mdix compile secrets.mdix --password`"
        )),

        TokenType::Keyword(kw) => hover_keyword(kw),
        TokenType::Bool(b) => Some(format!(
            "**`{}`** — boolean literal\n\nType: `<bool>`.",
            if *b { "true" } else { "false" }
        )),
        TokenType::Identifier(name) => {
            if token.section == SectionId::Config {
                hover_config_key(name)
                    .or_else(|| hover_identifier(doc, name, token.section, index))
            } else if token.section == SectionId::Security {
                hover_security_identifier(&doc.tokens, index, name)
                    .or_else(|| hover_identifier(doc, name, token.section, index))
            } else {
                hover_identifier(doc, name, token.section, index)
            }
        }
        TokenType::Date(d)       => hover_date(d),
        TokenType::Timestamp(ts) => hover_timestamp(ts),

        TokenType::RegexConstructor(_)  => Some(hover_regex(&doc.tokens, index)),
        TokenType::BlobConstructor(_)   => Some(hover_blob(&doc.tokens, index)),
        TokenType::TupleConstructor(_)  => Some(concat!(
            "**`t:(...)`** — tuple constructor\n\n",
            "Mixed-type collection, maximum 6 elements.\n\n",
            "```mdix\ncoord = t:(128.5, 0.0, -64.3)\n```\n\n",
            "Methods: `.first()`, `.second()` … `.sixth()`, `.get(index)`, `.toArray()`, `.length()`, `.contains(val)`, `.containsAny(arr)`, `.reverse()`, `.swap(i1,i2)`."
        ).to_string()),
        TokenType::HexColor(hex) => hover_hex_color(hex),
        TokenType::Integer(i) => Some(format!(
            "**`{}`** — 32-bit signed integer literal (`<int>`)\n\nRange: −2,147,483,648 to 2,147,483,647\n\nIf your value exceeds this range, use the `L` suffix: `{}L`",
            i, i
        )),
        TokenType::Long(l) => Some(format!(
            "**`{}L`** — 64-bit signed integer literal (`<long>`)\n\nRange: −9,223,372,036,854,775,808 to 9,223,372,036,854,775,807\n\nUse `_` separators for readability: `9_000_000_000L`",
            l
        )),
        TokenType::Float(f) => Some(format!(
            "**`{}f`** — 32-bit float literal (`<float>`)\n\nRequires the `f` suffix. Precision: ~7 significant decimal digits.",
            f
        )),
        TokenType::Double(d) => Some(format!(
            "**`{}`** — 64-bit double literal (`<double>`)\n\nIEEE 754 `f64`. Precision: ~15–17 significant decimal digits.",
            d
        )),
        TokenType::ScientificNotation(d) => Some(format!(
            "**`{:e}`** — scientific notation (`<double>`)\n\nStored as IEEE 754 64-bit `f64`.",
            d
        )),

        TokenType::String(s) => {
            if token.section == SectionId::Security {
                if let Some(content) = hover_security_value_at(&doc.tokens, index, s) {
                    return Some(content);
                }
            }
            Some(format!(
                "**String literal** (`<string>`)\n\nLength: {} characters\n\n```mdix\n\"{}\"\n```",
                s.len(), s
            ))
        }
        TokenType::StringSingle(s) => {
            if token.section == SectionId::Security {
                if let Some(content) = hover_security_value_at(&doc.tokens, index, s) {
                    return Some(content);
                }
            }
            Some(format!(
                "**String literal** (`<string>`, single-quoted)\n\nLength: {} characters",
                s.len()
            ))
        }
        TokenType::InterpolatedString(s) => Some(format!(
            "**Interpolated string** (`<string>`)\n\nUse `{{expr}}` to embed expressions at compile time.\n\n```mdix\n$\"{}\"\n```",
            s
        )),
        TokenType::ArithmeticOp(op)       => hover_operator(op, "arithmetic"),
        TokenType::ArithmeticAssignOp(op) => hover_operator(op, "arithmetic assignment"),
        TokenType::ComparisonOp(op)       => hover_operator(op, "comparison"),
        TokenType::LogicalOp(op)          => hover_operator(op, "logical"),
        TokenType::BitwiseOp(op)          => hover_operator(op, "bitwise"),

        TokenType::DoubleColon            => Some("**`::`** — group array operator\n\n```mdix\ntags:: \"alpha\", \"beta\", \"v1\"\n```".to_string()),
        TokenType::Arrow                  => Some("**`=>`** — association / scope operator".to_string()),
        TokenType::SwitchCase             => Some("**`->`** — switch-case / association operator\n\n```mdix\nencryption -> { mode = \"password\" }\n```".to_string()),
        TokenType::Symbol('~')            => Some("**`~`** — QuickFunc declaration prefix\n\n```mdix\n~myFunc<int>(x<int>) { return x * 2 }\n```".to_string()),

        TokenType::Comment(_) => None,
        _ => None,
    }
}

// ── Typed signature lookup (from hover_data tables) ───────────────────────────

fn hover_instance_method_from_sigs(type_name: &str, method_name: &str) -> Option<String> {
    INSTANCE_METHOD_SIGS.iter()
        .find(|(t, m, _, _, _)| *t == type_name && *m == method_name)
        .map(|(_, method, params, ret, example)| {
            let sig = if params.is_empty() {
                format!("{}() → {}", method, ret)
            } else {
                format!("{}({}) → {}", method, params, ret)
            };
            format!(
                "**`{sig}`** — `<{type_}>` instance method\n\n**Returns:** `<{ret}>`\n\n```mdix\n// Example:\n{example}\n```",
                sig = sig, type_ = type_name, ret = ret, example = example,
            )
        })
}

// ── Chained-call type inference ───────────────────────────────────────────────
//
// When the receiver token is `)`, scan backward through the token stream to
// find the enclosing call expression and infer what type it returns.
// This enables hover on chains like:  a.b().c().d()
//   receiver of .d() is ')' → find .c() return type
//   receiver of .c() is ')' → find .b() return type
//   receiver of .b() is the 'a' identifier

fn infer_close_paren_dix_type(
    doc:             &Document,
    tokens:          &[Token],
    close_paren_idx: usize,
    section:         SectionId,
    depth:           usize,
) -> Option<DixType> {
    if depth > 6 { return None; }

    // ── Step 1: find the matching '(' ─────────────────────────────────────────
    let mut paren_depth = 0i32;
    let mut open_idx    = None;
    for i in (0..=close_paren_idx).rev() {
        match &tokens[i].token_type {
            TokenType::Symbol(')') => paren_depth += 1,
            TokenType::Symbol('(') => {
                paren_depth -= 1;
                if paren_depth == 0 { open_idx = Some(i); break; }
            }
            _ => {}
        }
    }
    let open_idx = open_idx.filter(|&i| i > 0)?;

    // ── Step 2: find the function/method name before '(' ─────────────────────
    // Skip over a type annotation `<Type>` that may sit between name and '('
    let mut name_idx = open_idx - 1;
    if matches!(&tokens[name_idx].token_type,
        TokenType::Symbol('>') | TokenType::Identifier(_)
        if matches!(&tokens[name_idx].token_type, TokenType::Symbol('>') )
    ) {
        let mut angle = 0i32;
        let start = name_idx;
        for i in (0..start).rev() {
            match &tokens[i].token_type {
                TokenType::Symbol('>') => angle += 1,
                TokenType::BitwiseOp(op) if *op == ">>" => angle += 2,
                TokenType::Symbol('<') => {
                    angle -= 1;
                    if angle <= 0 {
                        if i > 0 { name_idx = i - 1; }
                        break;
                    }
                }
                _ if angle > 0 => {}
                _ => break,
            }
        }
    }

    let func_name = match &tokens.get(name_idx)?.token_type {
        TokenType::Identifier(n) => n.clone(),
        _ => return None,
    };

    // ── Step 3: check if it's a dot-method call: receiver.method(...) ─────────
    if name_idx >= 2 {
        let dot_tok = tokens.get(name_idx - 1)?;
        if matches!(dot_tok.token_type, TokenType::Symbol('.')) {
            let recv_tok = tokens.get(name_idx - 2)?;

            // Static object method: Math.sqrt(...), DateTime.now(), etc.
            if let TokenType::Identifier(recv_name) = &recv_tok.token_type {
                static_object_registry::initialize_static_registry();
                if static_object_registry::has_static_object(recv_name) {
                    if let Some(info) = static_object_registry::get_method_info(recv_name, &func_name) {
                        return TypeInferenceVisitor::convert_dix_type_to_data_type(info.return_type)
                            .and_then(|dt| TypeInferenceVisitor::convert_data_type_to_dix_type(dt));
                    }
                    return None;
                }

                // Imported namespace function: Utils.calc(...)
                if let Some(st) = doc.semantic_result.as_ref()
                    .and_then(|sr| sr.symbol_table.as_ref())
                {
                    if let Some(info) = st.get_namespaced_function(recv_name, &func_name) {
                        return info.signature.return_type
                            .and_then(|dt| TypeInferenceVisitor::convert_data_type_to_dix_type(dt));
                    }
                }
            }

            // Instance method on a chained call chain or plain variable
            let recv_dix = if matches!(recv_tok.token_type, TokenType::Symbol(')')) {
                infer_close_paren_dix_type(doc, tokens, name_idx - 2, section, depth + 1)
            } else {
                infer_receiver_dix_type(doc, recv_tok, section)
            };

            if let Some(dix) = recv_dix {
                instance_method_registry::initialize();
                if let Some(m) = instance_method_registry::get_instance_method(dix, &func_name) {
                    let ret = m.return_type();
                    if !matches!(ret, DixType::Void | DixType::Null) {
                        return Some(ret);
                    }
                }
            }
            return None;
        }
    }

    // ── Step 4: direct call — QuickFunc or symbol-table function ──────────────
    lookup_func_return_dix_type(doc, &func_name)
}

/// Look up a function's return DixType from QF AST or symbol table.
fn lookup_func_return_dix_type(doc: &Document, func_name: &str) -> Option<DixType> {
    if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
        if let Some(f) = qf.functions.iter().find(|f| f.name == func_name) {
            if let Some(dt) = f.return_type {
                return TypeInferenceVisitor::convert_data_type_to_dix_type(dt);
            }
        }
    }
    if let Some(st) = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {
        if let Some(sig) = st.try_get_function(func_name) {
            if let Some(dt) = sig.return_type {
                return TypeInferenceVisitor::convert_data_type_to_dix_type(dt);
            }
        }
    }
    None
}

// ── Dot-method hover dispatcher ───────────────────────────────────────────────

fn hover_after_dot(
    doc:         &Document,
    method_name: &str,
    section:     SectionId,
    token_index: usize,
) -> Option<String> {
    if token_index < 2 { return None; }

    let prev = doc.tokens.get(token_index - 1)?;
    if !matches!(prev.token_type, TokenType::Symbol('.')) { return None; }

    let receiver = doc.tokens.get(token_index - 2)?;

    // ── Priority 1: imported namespace member ─────────────────────────────────
    if let TokenType::Identifier(recv_name) = &receiver.token_type {
        if let Some(content) = hover_imported_namespace_member(doc, recv_name, method_name) {
            return Some(content);
        }
    }

    // ── Priority 2: static object method ─────────────────────────────────────
    if let TokenType::Identifier(recv_name) = &receiver.token_type {
        static_object_registry::initialize_static_registry();
        if static_object_registry::has_static_object(recv_name) {
            if let Some(content) = hover_static_method(recv_name, method_name) {
                return Some(content);
            }
            if let Some(info) = static_object_registry::get_method_info(recv_name, method_name) {
                let pc = info.parameter_count.max(0) as usize;
                let params_str = if pc == 0 { String::new() } else {
                    (1..=pc).map(|i| format!("arg{}", i)).collect::<Vec<_>>().join(", ")
                };
                return Some(format!(
                    "**`{obj}.{method}({params})`** — static method\n\n{desc}\n\n**Returns:** `<{ret}>`",
                    obj = recv_name, method = method_name, params = params_str,
                    desc = info.description, ret = info.return_type.get_type_name(),
                ));
            }
            return None;
        }
    }

    // ── Priority 3: instance method — handles both variable receivers AND
    //    chained calls where receiver token is ')' ────────────────────────────
    let receiver_type = if matches!(receiver.token_type, TokenType::Symbol(')')) {
        // Chained call: resolve what the preceding call returns
        infer_close_paren_dix_type(doc, &doc.tokens, token_index - 2, section, 0)
    } else if matches!(receiver.token_type, TokenType::Symbol(']')) {
        // Index access result: treat as Array element (limited inference)
        Some(DixType::Array)
    } else {
        infer_receiver_dix_type(doc, receiver, section)
    };

    let receiver_type = match receiver_type {
        Some(t) => t,
        None    => return None,
    };
    let type_name = receiver_type.get_type_name();

    // Try the typed signature table first (has param names and examples)
    if let Some(content) = hover_instance_method_from_sigs(type_name, method_name) {
        return Some(content);
    }

    // Fall back to registry
    instance_method_registry::initialize();
    let method = instance_method_registry::get_instance_method(receiver_type, method_name)?;

    let pc = (method.parameter_count() as i32 - 1).max(0) as usize;
    let params_str = if pc == 0 { String::new() } else {
        (1..=pc).map(|i| format!("arg{}", i)).collect::<Vec<_>>().join(", ")
    };

    // Enhanced return type string — uses full DataType for typed collections
    let ret_type_str = {
        let recv_full_dt = if let TokenType::Identifier(recv_name) = &receiver.token_type {
            infer_identifier_full_data_type(doc, recv_name, section)
        } else {
            None
        };
        let method_ret = method.return_type();
        match recv_full_dt {
            Some(DataType::TypedArray(elem))
                if matches!(method_ret, DixType::Any)
                    && ARRAY_ELEMENT_METHODS.contains(&method_name) =>
            {
                format!("{} *(element type)*", elem)
            }
            Some(DataType::TypedTuple(slots))
                if matches!(method_ret, DixType::Any)
                    && TUPLE_ELEMENT_METHODS.contains(&method_name) =>
            {
                let types: Vec<String> =
                    slots.iter().filter_map(|&s| s).map(|e| format!("{}", e)).collect();
                if types.is_empty() { method_ret.get_type_name().to_string() }
                else { format!("tuple element ({})", types.join("|")) }
            }
            _ => method_ret.get_type_name().to_string(),
        }
    };

    Some(format!(
        "**`{method}({params})`** — `<{type_}>` instance method\n\n{desc}\n\n**Returns:** `<{ret}>`",
        method = method_name, params = params_str, type_ = type_name,
        desc = method.description(), ret = ret_type_str,
    ))
}

const ARRAY_ELEMENT_METHODS: &[&str] = &["first", "last", "get", "at", "pop", "random"];
const TUPLE_ELEMENT_METHODS: &[&str] =
    &["first", "second", "third", "fourth", "fifth", "sixth", "get", "at"];

// ── Imported namespace hover helpers ──────────────────────────────────────────

fn hover_imported_namespace_member(
    doc:            &Document,
    namespace_name: &str,
    member_name:    &str,
) -> Option<String> {
    let st = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    let ns = st.try_get_namespace(namespace_name)?;

    if let Some(func_info) = ns.functions.get(member_name) {
        let params: Vec<String> = func_info.signature.parameters.iter()
            .map(|p| {
                let t = p.param_type.map(|dt| format!("<{}>", dt)).unwrap_or_default();
                let d = if p.has_default_value { " = …" } else { "" };
                format!("{}{}{}", p.name, t, d)
            })
            .collect();
        let ret = func_info.signature.return_type.map(|t| format!("{}", t)).unwrap_or_else(|| "?".to_string());
        let param_names: Vec<&str> = func_info.signature.parameters.iter().map(|p| p.name.as_str()).collect();
        let scope_note = {
            let scopes = &func_info.signature.scopes;
            if scopes.is_empty() || (scopes.len() == 1 && scopes[0].eq_ignore_ascii_case("global")) {
                String::new()
            } else {
                format!("\n\n**Scope:** `=> {}`", scopes.join(", "))
            }
        };
        return Some(format!(
            "**`{ns}.{name}<{ret}>({params})` — imported QuickFunc**\n\nNamespace: `{ns}`  \nFile: `{file}`{scope}\n\nCompile-time function — zero runtime overhead.\n\n```mdix\n// Usage:\n{ns}.{name}({args})\n```",
            ns = namespace_name, name = member_name, ret = ret,
            params = params.join(", "), file = ns.file_path, scope = scope_note,
            args = param_names.join(", "),
        ));
    }

    if let Some(fields) = ns.enums.get(member_name) {
        let mut field_list: Vec<String> =
            fields.iter().map(|(f, v)| format!("`{} = {}`", f, v)).collect();
        field_list.sort();
        let shown: Vec<&str> = field_list.iter().take(10).map(|s| s.as_str()).collect();
        let more = if fields.len() > 10 { format!(" … (+{} more)", fields.len() - 10) } else { String::new() };
        return Some(format!(
            "**`{ns}.{name}`** — imported enum type\n\nNamespace: `{ns}`  \nFile: `{file}`\n\n**Fields:** {fields}{more}\n\nAccess: `{ns}.{name}.FIELD_NAME`",
            ns = namespace_name, name = member_name, file = ns.file_path,
            fields = shown.join(", "), more = more,
        ));
    }

    None
}

fn hover_imported_enum_field_at(
    doc:         &Document,
    field_name:  &str,
    token_index: usize,
) -> Option<String> {
    if token_index < 4 { return None; }
    let dot1     = doc.tokens.get(token_index - 1)?;
    if !matches!(dot1.token_type, TokenType::Symbol('.')) { return None; }
    let enum_tok = doc.tokens.get(token_index - 2)?;
    let enum_name = match &enum_tok.token_type { TokenType::Identifier(n) => n.clone(), _ => return None };
    let dot2     = doc.tokens.get(token_index - 3)?;
    if !matches!(dot2.token_type, TokenType::Symbol('.')) { return None; }
    let ns_tok   = doc.tokens.get(token_index - 4)?;
    let namespace_name = match &ns_tok.token_type { TokenType::Identifier(n) => n.clone(), _ => return None };

    let st     = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    let ns     = st.try_get_namespace(&namespace_name)?;
    let fields = ns.enums.get(&enum_name)?;
    let value  = fields.get(field_name)?;

    Some(format!(
        "**`{ns}.{enum_}.{field}`** — imported enum field\n\nValue: **`{value}`**\n\nNamespace: `{ns}`  \nFile: `{file}`\n\n```mdix\nmy_var<enum> = {ns}.{enum_}.{field}\n```",
        ns = namespace_name, enum_ = enum_name, field = field_name,
        value = value, file = ns.file_path,
    ))
}

// ── Type resolution helpers ───────────────────────────────────────────────────

fn infer_receiver_dix_type(doc: &Document, tok: &Token, section: SectionId) -> Option<DixType> {
    match &tok.token_type {
        TokenType::String(_) | TokenType::StringSingle(_) | TokenType::InterpolatedString(_) => Some(DixType::String),

        TokenType::Long(_)   => Some(DixType::Long),
        TokenType::Float(_)  => Some(DixType::Float),
        TokenType::Double(_) | TokenType::ScientificNotation(_) => Some(DixType::Double),
        TokenType::Bool(_)   => Some(DixType::Bool),
        TokenType::HexColor(_) => Some(DixType::Hex),
        TokenType::Date(_)   => Some(DixType::Date),
        TokenType::Timestamp(_) => Some(DixType::Timestamp),
        TokenType::Symbol(']') => Some(DixType::Array),
        TokenType::Symbol('}') => Some(DixType::Object),
        TokenType::BlobConstructor(_)  => Some(DixType::Blob),
        TokenType::RegexConstructor(_) => Some(DixType::Regex),
        TokenType::TupleConstructor(_) => Some(DixType::Tuple),
        // ')' is handled by infer_close_paren_dix_type at call sites — return None here
        TokenType::Symbol(')') => None,
        TokenType::Identifier(name) => infer_identifier_dix_type(doc, name, section),
        _ => None,
    }
}

fn infer_identifier_dix_type(doc: &Document, name: &str, section: SectionId) -> Option<DixType> {
    if section == SectionId::QuickFuncs {
        if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
            let st = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref());
            for func in &qf.functions {
                let locals = local_scope::local_variable_types_for_function(func, st);
                if let Some(dt_opt) = locals.get(name) {
                    // Known parameter/local in this function — shadows any
                    // same-named DATA variable, so commit to this answer
                    // (even if the type itself couldn't be resolved) rather
                    // than falling through to the DATA-section lookups below.
                    return dt_opt.and_then(ast_data_type_to_dix_type);
                }
            }
        }
    }

    if let Some(type_idx) = doc.semantic_result.as_ref()?.type_index.as_ref() {
        if let Some(&dt) = type_idx.get(name) { return ast_data_type_to_dix_type(dt); }
    }

    let st  = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    let var = st.try_get_data_variable(name)
        .or_else(|| st.try_get_data_variable(&format!("DATA.{}", name)))?;
    ast_data_type_to_dix_type(var.effective_type()?)
}

fn infer_identifier_full_data_type(doc: &Document, name: &str, section: SectionId) -> Option<DataType> {
    if section == SectionId::QuickFuncs {
        if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
            let st = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref());
            for func in &qf.functions {
                if let Some((dt_opt, _)) = find_var_decl_in_stmts(&func.body, name) {
                    if let Some(dt) = dt_opt { return Some(dt); }
                    if let Some(val_expr) = find_var_value_in_stmts(&func.body, name) {
                        if let Some(st) = st {
                            // Full running map (params + every prior local in
                            // this body), not just parameters — so e.g.
                            // `mytup = sometuple.first()` resolves `sometuple`
                            // even when it's itself a local a few lines up.
                            // infer_full_dt_for_hover still does its own
                            // tuple/array element-detail enhancement on top
                            // of this, unchanged.
                            let locals = local_scope::local_variable_types_for_function(func, Some(st));
                            return infer_full_dt_for_hover(val_expr, &locals, st);
                        }
                    }
                    return None;
                }
            }
        }
    }

    if let Some(type_idx) = doc.semantic_result.as_ref()?.type_index.as_ref() {
        if let Some(&dt) = type_idx.get(name) { return Some(dt); }
    }

    let st  = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    let var = st.try_get_data_variable(name)
        .or_else(|| st.try_get_data_variable(&format!("DATA.{}", name)))?;
    var.effective_type()
}

fn ast_data_type_to_dix_type(dt: DataType) -> Option<DixType> {
    match dt {
        DataType::Int => Some(DixType::Int),
        DataType::Long => Some(DixType::Long),
        DataType::Float => Some(DixType::Float),
        DataType::Double => Some(DixType::Double),
        DataType::String => Some(DixType::String),
        DataType::Bool => Some(DixType::Bool),
        DataType::Array | DataType::TypedArray(_) => Some(DixType::Array),
        DataType::Tuple | DataType::TypedTuple(_) => Some(DixType::Tuple),
        DataType::Object => Some(DixType::Object),
        DataType::Hex => Some(DixType::Hex),
        DataType::Blob => Some(DixType::Blob),
        DataType::Regex => Some(DixType::Regex),
        DataType::Date => Some(DixType::Date),
        DataType::Timestamp => Some(DixType::Timestamp),
        DataType::Enum => Some(DixType::Enum),
        _ => None,
    }
}

fn infer_full_dt_for_hover(
    expr:   &Expression,
    params: &HashMap<String, Option<DataType>>,
    st:     &dixscript::Compiler::Utilities::SymbolTable,
) -> Option<DataType> {
    let base = TypeInferenceVisitor::new(st, Some(params.clone())).infer_type_from_expression(expr);
    match (&base, expr) {
        (Some(DataType::Tuple) | None, Expression::Value { value, .. })
        | (Some(DataType::Array), Expression::Value { value, .. }) => {
            enhance_collection_dt(value, params, st).or(base)
        }
        _ => base,
    }
}

fn enhance_collection_dt(
    value:  &Value,
    params: &HashMap<String, Option<DataType>>,
    st:     &dixscript::Compiler::Utilities::SymbolTable,
) -> Option<DataType> {
    match value {
        Value::PrefixedConstructor { prefix, arguments, .. }
            if prefix.eq_ignore_ascii_case("t") =>
        {
            let mut slots = [None; 6];
            for (i, arg) in arguments.iter().enumerate().take(6) {
                slots[i] = elem_type_from_value_hover(arg, params, st);
            }
            if slots.iter().any(|s| s.is_some()) { Some(DataType::TypedTuple(slots)) }
            else { Some(DataType::Tuple) }
        }
        Value::Array { values, .. } | Value::NestedArray { values, .. } => {
            if values.is_empty() { return Some(DataType::Array); }
            let first = values.first().and_then(|v| elem_type_from_value_hover(v, params, st));
            let uniform = first.is_some() && values.iter().skip(1).all(|v| {
                elem_type_from_value_hover(v, params, st) == first
            });
            if uniform { Some(DataType::TypedArray(first.unwrap())) } else { Some(DataType::Array) }
        }
        _ => None,
    }
}

fn elem_type_from_value_hover(
    value:  &Value,
    params: &HashMap<String, Option<DataType>>,
    st:     &dixscript::Compiler::Utilities::SymbolTable,
) -> Option<ElemType> {
    let dt: DataType = match value {
        Value::Integer { .. }    => DataType::Int,
        Value::Long { .. }       => DataType::Long,
        Value::Float { .. }      => DataType::Float,
        Value::Double { .. } | Value::ScientificNotation { .. } => DataType::Double,
        Value::String { .. } | Value::InterpolatedString { .. } => DataType::String,
        Value::Boolean { .. }    => DataType::Bool,
        Value::HexColor { .. }   => DataType::Hex,
        Value::Date { .. }       => DataType::Date,
        Value::Timestamp { .. }  => DataType::Timestamp,
        Value::EnumValue { .. }  => DataType::Enum,
        Value::Object { .. }     => DataType::Object,
        Value::Identifier { value: name, .. } => params.get(name.as_str()).and_then(|o| *o)?,
        Value::Expression { expr, .. } => {
            TypeInferenceVisitor::new(st, Some(params.clone())).infer_type_from_expression(expr)?
        }
        _ => return None,
    };
    ElemType::from_data_type(dt)
}

fn format_dt_type_str(dt: DataType) -> String {
    match dt {
        DataType::TypedArray(elem) => format!("array<{}>", elem),
        DataType::TypedTuple(slots) => {
            let types: Vec<String> = slots.iter().filter_map(|&s| s).map(|e| format!("{}", e)).collect();
            if types.is_empty() { "tuple".to_string() } else { format!("tuple({})", types.join(",")) }
        }
        other => format!("{}", other),
    }
}

// ── Statement helpers ─────────────────────────────────────────────────────────

fn find_var_value_in_stmts<'a>(stmts: &'a [QuickFuncStatement], name: &str) -> Option<&'a Expression> {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration { variable_name, value, .. }
                if *variable_name == name => return Some(value),
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                if let Some(r) = find_var_value_in_stmts(then_branch, name) { return Some(r); }
                if let Some(eb) = else_branch {
                    if let Some(r) = find_var_value_in_stmts(eb, name) { return Some(r); }
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    if let Some(r) = find_var_value_in_stmts(&case.statements, name) { return Some(r); }
                }
                if let Some(dc) = default_case {
                    if let Some(r) = find_var_value_in_stmts(&dc.statements, name) { return Some(r); }
                }
            }
            _ => {}
        }
    }
    None
}

fn find_var_decl_in_stmts(stmts: &[QuickFuncStatement], name: &str) -> Option<(Option<DataType>, bool)> {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration { variable_name, data_type, is_mutable, .. }
                if variable_name.as_str() == name =>
            {
                return Some((*data_type, *is_mutable));
            }
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                if let Some(r) = find_var_decl_in_stmts(then_branch, name) { return Some(r); }
                if let Some(eb) = else_branch {
                    if let Some(r) = find_var_decl_in_stmts(eb, name) { return Some(r); }
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    if let Some(r) = find_var_decl_in_stmts(&case.statements, name) { return Some(r); }
                }
                if let Some(dc) = default_case {
                    if let Some(r) = find_var_decl_in_stmts(&dc.statements, name) { return Some(r); }
                }
            }
            _ => {}
        }
    }
    None
}

// ── Section hover builder ─────────────────────────────────────────────────────

fn section_hover(name: &str, description: &str, example: &str, notes: &str) -> String {
    format!(
        "**`{}`** — DixScript section\n\n{}\n\n```mdix\n{}\n```\n\n{}",
        name, description, example, notes
    )
}

// ── DLM module hover ──────────────────────────────────────────────────────────

fn hover_dlm_module(name: &str) -> Option<String> {
    match name {
        "DCompressor" => Some(concat!(
            "**`DCompressor`** — DLM compression module\n\n",
            "| Subtype | Algorithm | Notes |\n",
            "|---------|-----------|-------|\n",
            "| `gzip`  | DEFLATE   | All targets including wasm32 |\n",
            "| `bzip2` | BWT+Huffman | ~15% better; not on wasm32 |\n",
            "| `lzma`  | LZMA      | Best ratio; not on wasm32 |\n\n",
            "```mdix\n@DLM(\n  DCompressor.gzip\n)\n```"
        ).to_string()),
        "DEncryptor" => Some(concat!(
            "**`DEncryptor`** — DLM encryption module\n\nRequires `@SECURITY`.\n\n",
            "| Subtype    | Algorithm         | Key |\n",
            "|------------|-------------------|-----|\n",
            "| `aes256`   | AES-256-GCM       | 256-bit |\n",
            "| `aes128`   | AES-128-GCM       | 128-bit |\n",
            "| `chacha20` | ChaCha20-Poly1305 | 256-bit |\n",
            "| `xor`      | XOR (⚠️ weak)     | varies |\n\n",
            "```mdix\n@DLM(DCompressor.gzip, DEncryptor.aes256)\n```"
        ).to_string()),
        "DAuditor" => Some(concat!(
            "**`DAuditor`** — DLM audit module\n\n",
            "| Subtype    | Behaviour |\n",
            "|------------|-----------|\n",
            "| `diy`      | User-registered audit hook |\n",
            "| `enhanced` | Built-in checksum + metadata |\n\n",
            "```mdix\n@DLM(\n  DAuditor.enhanced\n)\n```"
        ).to_string()),
        _ => None,
    }
}

fn hover_dlm_subtype(name: &str) -> Option<String> {
    match name {
        "gzip"     => Some("**`gzip`** — DEFLATE compression. Available on all targets including `wasm32`.\n\nUsage: `DCompressor.gzip`".to_string()),
        "bzip2"    => Some("**`bzip2`** — BWT + Huffman. Better than gzip, not on `wasm32`.\n\nUsage: `DCompressor.bzip2`".to_string()),
        "lzma"     => Some("**`lzma`** — Best ratio, slowest. Not on `wasm32`.\n\nUsage: `DCompressor.lzma`".to_string()),
        "aes256"   => Some("**`aes256`** — AES-256-GCM. Recommended default.\n\nUsage: `DEncryptor.aes256`".to_string()),
        "aes128"   => Some("**`aes128`** — AES-128-GCM. Faster on targets without hardware AES.\n\nUsage: `DEncryptor.aes128`".to_string()),
        "chacha20" => Some("**`chacha20`** — ChaCha20-Poly1305. Preferred on mobile/ARM.\n\nUsage: `DEncryptor.chacha20`".to_string()),
        "xor"      => Some("**`xor`** — XOR obfuscation.\n\n⚠️ **Not real encryption.** Obfuscation only.\n\nUsage: `DEncryptor.xor`".to_string()),
        "diy"      => Some("**`diy`** — DIY audit hook.\n\nUsage: `DAuditor.diy`".to_string()),
        "enhanced" => Some("**`enhanced`** — Built-in checksum audit.\n\nUsage: `DAuditor.enhanced`".to_string()),
        _ => None,
    }
}

// ── SECURITY hover ─────────────────────────────────────────────────────────────
//
// Ground truth cross-checked against two independent sources so completions
// and hover can't quietly drift apart again:
//   - security_section_analyzer.rs (what the compiler actually validates/warns on)
//   - security_utilities.rs        (what the compiler auto-generates as defaults)
// `metadata` is deliberately freeform — no fixed field set exists for it.

/// Disambiguate an Identifier token inside `@SECURITY`: a block key is
/// followed by `->`, a field key is followed by `=`.
fn hover_security_identifier(tokens: &[Token], index: usize, name: &str) -> Option<String> {
    let next = tokens.get(index + 1)?;
    if matches!(next.token_type, TokenType::SwitchCase) {
        return hover_security_block_key(name);
    }
    if matches!(next.token_type, TokenType::Symbol('=')) {
        return hover_security_field_key(name);
    }
    None
}

fn hover_security_block_key(name: &str) -> Option<String> {
    let content = match name.to_lowercase().as_str() {
        "encryption" => concat!(
            "**`encryption`** — @SECURITY block\n\n",
            "Configures how the encryption key is supplied. Required when `@DLM` includes a `DEncryptor` module.\n\n",
            "**Fields:** `mode`, `algorithm`, `key_length`, `kdf`*, `kdf_memory`*, `kdf_iterations`*, `kdf_parallelism`* — *password mode only. `key`†, `iv`† — †manual mode only.\n\n",
            "```mdix\nencryption -> {\n  mode      = \"keyfile\",\n  algorithm = \"aes256-gcm\"\n}\n```",
        ),
        "validation" => concat!(
            "**`validation`** — @SECURITY block\n\n",
            "Content-integrity and authentication-tag settings.\n\n",
            "**Fields:** `checksum_algorithm`, `auth_tag_length`, `hmac_algorithm`\n\n",
            "```mdix\nvalidation -> {\n  checksum_algorithm = \"sha256\"\n}\n```",
        ),
        "keystore" => concat!(
            "**`keystore`** — @SECURITY block\n\n",
            "Key-file management, relevant when `mode = \"keyfile\"`.\n\n",
            "**Fields:** `auto_generate`, `backup_count`, `backup_naming`\n\n",
            "```mdix\nkeystore -> {\n  auto_generate = true,\n  backup_count  = 3\n}\n```",
        ),
        "override" => concat!(
            "**`override`** — @SECURITY block\n\n",
            "Only meaningful with `encryption -> { mode = \"manual\" }`.\n\n",
            "**Fields:** `manual_key_warning_accepted`\n\n",
            "```mdix\noverride -> {\n  manual_key_warning_accepted = true\n}\n```\n\n",
            "⚠️ Manual mode stores the encryption key in **PLAINTEXT** in source.",
        ),
        "metadata" => concat!(
            "**`metadata`** — @SECURITY block\n\n",
            "Freeform key/value metadata. Not validated against a fixed field set — any keys are accepted.\n\n",
            "```mdix\nmetadata -> {\n  owner = \"team-name\"\n}\n```",
        ),
        _ => return None,
    };
    Some(content.to_string())
}

fn hover_security_field_key(name: &str) -> Option<String> {
    let (block, desc): (&str, &str) = match name {
        "mode" => (
            "encryption",
            "How the encryption key is supplied.\n\n**Values:** `\"keyfile\"` *(default)*, `\"password\"`, `\"manual\"`",
        ),
        "algorithm" => (
            "encryption",
            "Encryption algorithm.\n\n**Values:** `\"aes256-gcm\"` *(default)*, `\"aes128-gcm\"`, `\"chacha20-poly1305\"`, `\"aes256\"`, `\"aes128\"`, `\"chacha20\"`, `\"xor\"` (⚠️ obfuscation only)",
        ),
        "key_length" => (
            "encryption",
            "Key length in bytes. Auto-derived from `algorithm` if omitted (xor→32, aes128→16, aes256/chacha20→32).",
        ),
        "kdf" => (
            "encryption",
            "Key derivation function — only used when `mode = \"password\"`.\n\n**Values:** `\"argon2id\"` *(default)*, `\"pbkdf2\"`",
        ),
        "kdf_memory"      => ("encryption", "Argon2 memory cost in KiB. Recommended minimum: `65536`."),
        "kdf_iterations"  => ("encryption", "Argon2 iteration count. Recommended minimum: `3`."),
        "kdf_parallelism" => ("encryption", "Argon2 parallelism factor. Recommended minimum: `4`."),
        "key" => (
            "encryption",
            "**CRITICAL**: the raw encryption key, stored in PLAINTEXT in source. Required (with `iv`) when `mode = \"manual\"`.",
        ),
        "iv" => (
            "encryption",
            "Initialization vector. Required (with `key`) when `mode = \"manual\"`.",
        ),
        "checksum_algorithm" => (
            "validation",
            "Hash algorithm for content-integrity checks.\n\n**Values:** `\"sha256\"` *(default)*, `\"sha512\"`, `\"hmac-sha256\"`, `\"hmac-sha512\"`",
        ),
        "auth_tag_length" => ("validation", "Authentication tag length in bits. Default: `128`."),
        "hmac_algorithm" => (
            "validation",
            "HMAC algorithm for message authentication.\n\n**Values:** `\"hmac-sha256\"` *(default)*, `\"hmac-sha512\"`",
        ),
        "auto_generate" => ("keystore", "When `true` *(default)*, the compiler auto-generates a `.mdix.key` file."),
        "backup_count"  => ("keystore", "Number of previous key-file backups to retain. Range `0`-`10`, default `3`."),
        "backup_naming" => (
            "keystore",
            "Naming convention for backup key files.\n\n**Values:** `\"timestamp\"` *(default)*, `\"sequence\"`",
        ),
        "manual_key_warning_accepted" => (
            "override",
            "**Required** for `mode = \"manual\"`. Explicitly acknowledges the key is stored in PLAINTEXT in source.",
        ),
        _ => return None,
    };
    Some(format!("**`{}`** — `{}` field\n\n{}", name, block, desc))
}

/// Hover for the *value* token of a `field = "value"` security entry.
/// Walks back through `=` to the field-key identifier to know which value
/// table to look the literal up in.
fn hover_security_value_at(tokens: &[Token], index: usize, value: &str) -> Option<String> {
    if index < 2 { return None; }
    let eq_tok = tokens.get(index - 1)?;
    if !matches!(eq_tok.token_type, TokenType::Symbol('=')) { return None; }
    let field_tok = tokens.get(index - 2)?;
    let field_name = match &field_tok.token_type {
        TokenType::Identifier(n) => n.as_str(),
        TokenType::Keyword(k)    => *k,
        _ => return None,
    };
    hover_security_value(field_name, value)
}

fn hover_security_value(field_name: &str, value: &str) -> Option<String> {
    let desc = match (field_name, value) {
        ("mode", "keyfile")  => "Compiler auto-generates a `.mdix.key` file — recommended.",
        ("mode", "password") => "Compiler prompts for a passphrase at compile time.",
        ("mode", "manual")   => "You supply `key`/`iv` directly.\n\n⚠️ **CRITICAL:** stored in PLAINTEXT in source.",

        ("algorithm", "aes256-gcm")        => "AES-256-GCM — recommended.",
        ("algorithm", "aes128-gcm")        => "AES-128-GCM — faster, slightly smaller keys.",
        ("algorithm", "chacha20-poly1305") => "ChaCha20-Poly1305 — excellent on mobile/ARM.",
        ("algorithm", "aes256")            => "AES-256, non-AEAD variant.",
        ("algorithm", "aes128")            => "AES-128, non-AEAD variant.",
        ("algorithm", "chacha20")          => "ChaCha20, non-AEAD variant.",
        ("algorithm", "xor")               => "⚠️ Obfuscation only — NOT cryptographically secure.",

        ("kdf", "argon2id") => "Argon2id — recommended.",
        ("kdf", "pbkdf2")   => "PBKDF2.",

        ("checksum_algorithm", "sha256")      => "SHA-256 — recommended.",
        ("checksum_algorithm", "sha512")      => "SHA-512 — stronger, larger output.",
        ("checksum_algorithm", "hmac-sha256") => "HMAC-SHA256 — keyed integrity check.",
        ("checksum_algorithm", "hmac-sha512") => "HMAC-SHA512 — keyed integrity check, stronger.",

        ("hmac_algorithm", "hmac-sha256") => "HMAC-SHA256 — recommended.",
        ("hmac_algorithm", "hmac-sha512") => "HMAC-SHA512.",

        ("backup_naming", "timestamp") => "Embed a timestamp in the backup key filename.",
        ("backup_naming", "sequence")  => "Use an incrementing sequence number.",

        _ => return None,
    };
    Some(format!("**`\"{}\"`** — `{}` value\n\n{}", value, field_name, desc))
}

// ── CONFIG key hover ──────────────────────────────────────────────────────────

fn hover_config_key(name: &str) -> Option<String> {
    let content = match name.to_lowercase().as_str() {
        "version"            => "**`version`** — CONFIG key\n\nDixScript format version.\n\nExample: `version -> \"1.0.0\"`",
        "encoding"           => "**`encoding`** — CONFIG key\n\nSource file encoding.\n\nSupported: `\"utf-8\"` *(default)*, `\"utf-16\"`, `\"ascii\"`, `\"iso-8859-1\"`",
        "author"             => "**`author`** — CONFIG key\n\nFile author. Free-form string.",
        "created"            => "**`created`** — CONFIG key\n\nFile creation timestamp. Format: `YYYY-MM-DDThh:mm:ssZ`",
        "features"           => "**`features`** — CONFIG key\n\n| Value | Sections |\n|-------|----------|\n| `\"basic\"` | DATA, SECURITY only |\n| `\"advanced\"` | All sections *(default)* |",
        "debug_mode"         => "**`debug_mode`** — CONFIG key\n\n| Value | Effect |\n|-------|--------|\n| `\"off\"` | No output *(default)* |\n| `\"regular\"` | Key resolution |\n| `\"verbose\"` | Full trace |",
        "error_handling"     => "**`error_handling`** — CONFIG key\n\n| Value | Behaviour |\n|-------|----------|\n| `\"halt\"` | Stop on first error *(default)* |\n| `\"continue\"` | Collect all |\n| `\"recover\"` | Try to parse past |",
        "compatibility_mode" => "**`compatibility_mode`** — CONFIG key\n\n| Value | Behaviour |\n|-------|----------|\n| `\"strict\"` | Reject unknown *(default)* |\n| `\"best_effort\"` | Warn, continue |\n| `\"permissive\"` | Accept anything |",
        _ => return None,
    };
    Some(content.to_string())
}

// ── Keyword hover ─────────────────────────────────────────────────────────────

fn hover_keyword(kw: &str) -> Option<String> {
    // Type keywords redirect to data-type hover
    match kw {
        "int" | "long" | "float" | "double" | "string" | "bool" | "array" | "tuple"
        | "object" | "hex" | "blob" | "regex" | "date" | "timestamp" | "enum" | "any" => {
            return hover_data_type(kw);
        }
        _ => {}
    }
    let content: &str = match kw {
        "if" | "if:"    => "**`if:`** — conditional (colon required).\n\n```mdix\nif: x > 0 {\n  return x\n} else {\n  return -1\n}\n```",
        "elif" | "elif:"=> "**`elif:`** — else-if branch.",
        "else"          => "**`else`** — fallback branch (no colon).",
        "chk" | "chk:" => "**`chk:`** — switch/match\n\n```mdix\nchk: aiType {\n  -> AIType.PASSIVE { return 0 }\n  -> miss           { return 5 }\n}\n```",
        "miss"          => "**`miss`** — default case in `chk:`. Must be last.\n\n```mdix\nchk: val {\n  -> 0    { return \"zero\" }\n  -> miss { return \"other\" }\n}\n```",
        "return"        => "**`return`** — return a value from a QuickFunc.",
        "log" | "log:"  => "**`log:`** — compile-time log output (zero runtime cost).\n\n```mdix\nlog: \"Building enemy: \" + name\nlog: health\n```\n\nLogs appear in the compiler output, not at runtime.",
        "let"           => "**`let`** — immutable local variable.\n\n```mdix\nlet result = x + y\nlet name<string> = \"Alice\"\nlet big<long> = 9_000_000_000L\n```\n\nUse `let mut` for mutable.",
        "mut"           => "**`mut`** — mutable modifier. Use as `let mut name = ...`",
        "const"         => "**`const`** — compile-time constant.",
        "and"           => "**`and`** — logical AND (= `&&`).",
        "or"            => "**`or`** — logical OR (= `||`).",
        "not"           => "**`not`** — logical NOT (= `!`).",
        "true"          => "**`true`** — boolean literal (`<bool>`)",
        "false"         => "**`false`** — boolean literal (`<bool>`)",
        "null"          => "**`null`** — null literal (absent value).",
        "from"          => "**`from`** — import keyword.\n\n```mdix\nUtils from \"common/utils.mdix\"\n```",
        "from_cloud"    => "**`from_cloud`** — remote import.",
        "verify"        => "**`verify`** — hash verification for imports.",
        "global"        => "**`global`** — global scope modifier for QuickFuncs.",
        _               => return None,
    };
    Some(content.to_string())
}

// ── Data type annotation hover ────────────────────────────────────────────────

fn hover_data_type(dt: &str) -> Option<String> {
    let content = match dt {
        "int"       => "**`<int>`** — 32-bit signed integer\n\nRange: −2,147,483,648 to 2,147,483,647\n\nUse `<long>` for larger values.\n\n```mdix\nport<int> = 8080\n```",
        "long"      => "**`<long>`** — 64-bit signed integer\n\nRange: ±9.2×10¹⁸. Literals require `L` suffix.\n\n```mdix\npopulation<long> = 8_100_000_000L\n```",
        "float"     => "**`<float>`** — 32-bit single-precision float\n\nRequires `f` suffix on literals. ~7 significant digits.\n\n```mdix\nspeed<float> = 3.14f\n```",
        "double"    => "**`<double>`** — 64-bit double-precision float (IEEE 754 f64)\n\nDefault for decimal literals without `f`. ~15–17 digits.\n\n```mdix\nprecision<double> = 3.14159265358979\n```",
        "string"    => "**`<string>`** — UTF-8 text\n\n```mdix\napp_name<string> = \"DixScript\"\n```",
        "bool"      => "**`<bool>`** — boolean\n\n```mdix\nenabled<bool> = true\n```",
        "array"     => "**`<array>`** — ordered collection\n\n```mdix\ntags:: \"alpha\", \"beta\"\n```\n\nMethods: `.length()`, `.contains(v)`, `.get(i)`, `.push(v)`, `.pop()`, `.join(sep)`, `.sort()`, `.first()`, `.last()`, `.sum()`, `.average()` …\n\nUse `<array<int>>` for a typed array.",
        "tuple"     => "**`<tuple>`** — mixed-type collection (max 6 elements)\n\n```mdix\ncoord = t:(128.5, 0.0, -64.3)\n```\n\nMethods: `.first()`, `.second()` … `.sixth()`, `.get(i)`, `.length()`, `.toArray()`, `.containsAny(arr)`\n\nUse `<tuple<int,bool>>` for a typed tuple.",
        "object"    => "**`<object>`** — key-value map `{ key = value }`",
        "hex"       => "**`<hex>`** — hex color or integer\n\n```mdix\ncolor<hex> = #FF5733\nmask<hex>  = 0xFF00FF\n```",
        "blob"      => "**`<blob>`** — base64-encoded binary\n\n```mdix\navatar<blob> = b:(\"SGVsbG8gV29ybGQ=\")\n```",
        "regex"     => "**`<regex>`** — compiled regular expression\n\n```mdix\nemail<regex> = r:(\"^[\\\\w.]+@[\\\\w.]+$\")\n```",
        "date"      => "**`<date>`** — ISO 8601 date `YYYY-MM-DD`\n\n```mdix\nrelease<date> = 2025-12-31\n```",
        "timestamp" => "**`<timestamp>`** — ISO 8601 date-time\n\n```mdix\ncreated<timestamp> = 2025-01-15T10:30:00Z\n```",
        "enum"      => "**`<enum>`** — enum value from `@ENUMS`\n\n```mdix\nlevel<enum> = Difficulty.HARD\n```",
        "any"       => "**`<any>`** — accepts any type",
        _ => return None,
    };
    Some(content.to_string())
}

// ── Operator hover ────────────────────────────────────────────────────────────

fn hover_operator(op: &str, category: &str) -> Option<String> {
    let desc = match op {
        "+"   => "Addition or string concatenation",
        "-"   => "Subtraction",
        "*"   => "Multiplication",
        "/"   => "Division",
        "%"   => "Modulo (remainder)",
        "**"  => "Exponentiation: `2 ** 3 = 8`",
        "+="  => "Add and assign",
        "-="  => "Subtract and assign",
        "*="  => "Multiply and assign",
        "/="  => "Divide and assign",
        "%="  => "Modulo and assign",
        "=="  => "Equality",
        "!="  => "Inequality",
        "<"   => "Less than",
        ">"   => "Greater than",
        "<="  => "Less than or equal",
        ">="  => "Greater than or equal",
        "&&"  => "Logical AND (also: `and`)",
        "||"  => "Logical OR (also: `or`)",
        "&"   => "Bitwise AND",
        "|"   => "Bitwise OR",
        "^"   => "Bitwise XOR",
        "<<"  => "Left bit shift",
        ">>"  => "Right bit shift",
        _     => return None,
    };
    Some(format!("**`{}`** — {} operator\n\n{}", op, category, desc))
}

// ── Enum access hover ─────────────────────────────────────────────────────────

fn hover_enum_access(doc: &Document, enum_name: &str, field: &str) -> Option<String> {
    let st    = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    let value = st.try_get_enum_field_value(enum_name, field)?;
    Some(format!(
        "**`{}.{}`** — enum field\n\n```\n(enum) {} = {}\n```\n\nType: `<enum>`\n\nGet name at runtime: `Enum.getName(\"{}\", {})`",
        enum_name, field, field, value, enum_name, value
    ))
}

// ── QuickFunc local variable hover ────────────────────────────────────────────

fn hover_qf_local_var(doc: &Document, name: &str) -> Option<String> {
    let qf = doc.ast.as_ref()?.quick_functions.as_ref()?;
    let st = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref());

    for func in &qf.functions {
        if let Some((dt_opt, is_mutable)) = find_var_decl_in_stmts(&func.body, name) {
            let (type_str, note) = match dt_opt {
                Some(dt) => (format_dt_type_str(dt), "*(declared)*"),
                None => {
                    let inferred = if let Some(st) = st {
                        find_var_value_in_stmts(&func.body, name).and_then(|val_expr| {
                            let locals = local_scope::local_variable_types_for_function(func, Some(st));
                            infer_full_dt_for_hover(val_expr, &locals, st).map(format_dt_type_str)
                        })
                    } else { None };
                    match inferred {
                        Some(s) => (s, "*(inferred)*"),
                        None    => ("any".to_string(), "*(unknown)*"),
                    }
                }
            };
            let mut_str = if is_mutable { "mut " } else { "" };

            let method_hint = {
                let dix = ast_data_type_to_dix_type(dt_opt.unwrap_or(DataType::Any));
                if let Some(dt) = dix {
                    instance_method_registry::initialize();
                    let methods = instance_method_registry::get_instance_methods(dt);
                    if !methods.is_empty() {
                        let shown: Vec<&str> = methods.iter().take(5).map(|s| s.as_str()).collect();
                        format!(
                            "\n\nType methods: {} … *(type `.` to see all)*",
                            shown.iter().map(|m| format!("`{}`", m)).collect::<Vec<_>>().join(", ")
                        )
                    } else { String::new() }
                } else { String::new() }
            };

            return Some(format!(
                "**`{name}`** — local variable in `~{fn_name}`\n\nDeclared as: `let {mut_}{name}<{ty}>`\n\nType: `<{ty}>` {note}{methods}",
                name = name, fn_name = func.name, mut_ = mut_str,
                ty = type_str, note = note, methods = method_hint,
            ));
        }
    }
    None
}

// ── DATA table/group-array hover ──────────────────────────────────────────────

/// Rich hover for a TablePath token value (the whole "player.config" string).
fn hover_table_path_rich(doc: &Document, path: &str) -> Option<String> {
    let st            = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    let prefix_dot    = format!("DATA.{}.", path);
    let prefix_bracket= format!("DATA.{}[", path);

    let is_array = st.data_variables.keys().any(|p| p.starts_with(&prefix_bracket));

    if is_array {
        return hover_group_array_from_st(st, path, &prefix_bracket);
    }

    let mut children: Vec<(String, Option<DataType>)> = Vec::new();
    for (var_path, var) in &st.data_variables {
        if let Some(rest) = var_path.strip_prefix(&prefix_dot) {
            let seg = rest.split('.').next().and_then(|s| s.split('[').next()).unwrap_or(rest);
            if !children.iter().any(|(n, _)| *n == seg) {
                children.push((seg.to_string(), var.effective_type()));
            }
        }
    }

    if children.is_empty() { return None; }

    children.sort_by(|a, b| a.0.cmp(&b.0));
    let shown: Vec<String> = children.iter().take(12).map(|(n, dt)| {
        match dt {
            Some(t) => format!("`{}` `<{}>`", n, t),
            None    => format!("`{}`", n),
        }
    }).collect();
    let more = if children.len() > 12 {
        format!(" … (+{} more)", children.len() - 12)
    } else { String::new() };

    Some(format!(
        "**`{path}`** — DATA table\n\n**Properties:** {props}{more}\n\nRuntime: `data.get(\"{path}.property\")`",
        path = path, props = shown.join(", "), more = more,
    ))
}

fn hover_group_array_from_st(
    st:             &dixscript::Compiler::Utilities::SymbolTable,
    path:           &str,
    prefix_bracket: &str,
) -> Option<String> {
    let count = st.data_variables.keys()
        .filter(|p| p.starts_with(prefix_bracket))
        .filter_map(|p| {
            let rest = &p[prefix_bracket.len()..];
            rest.split(']').next().and_then(|n| n.parse::<usize>().ok())
        })
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);

    let item0_prefix = format!("DATA.{}[0].", path);
    let mut item_props: Vec<(String, Option<DataType>)> = st.data_variables.iter()
        .filter_map(|(p, var)| {
            p.strip_prefix(&item0_prefix).map(|rest| {
                let seg = rest.split('.').next().unwrap_or(rest).to_string();
                (seg, var.effective_type())
            })
        })
        .take(10)
        .collect();
    item_props.sort_by(|a, b| a.0.cmp(&b.0));
    item_props.dedup_by(|a, b| a.0 == b.0);

    let props_str = if item_props.is_empty() { String::new() } else {
        let listed: Vec<String> = item_props.iter().map(|(n, dt)| {
            match dt {
                Some(t) => format!("`{}` `<{}>`", n, t),
                None    => format!("`{}`", n),
            }
        }).collect();
        format!("\n\n**Item properties:** {}", listed.join(", "))
    };

    Some(format!(
        "**`{path}`** — DATA group array\n\n{count} item(s){props}\n\nRuntime: `data.get(\"{path}[i].property\")`\nAll items: `data.select_many(\"{path}.*.property\")`",
        path = path, count = count, props = props_str,
    ))
}

/// Hover for an identifier that is the root segment of a DATA table path.
fn hover_table_path_prefix(doc: &Document, name: &str) -> Option<String> {
    let st            = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    let prefix_dot    = format!("DATA.{}.", name);
    let prefix_bracket= format!("DATA.{}[", name);

    let is_array = st.data_variables.keys().any(|p| p.starts_with(&prefix_bracket));

    if is_array {
        return hover_group_array_from_st(st, name, &prefix_bracket);
    }

    let mut child_names: Vec<String> = Vec::new();
    let mut total_children: usize    = 0;

    for path in st.data_variables.keys() {
        if let Some(rest) = path.strip_prefix(&prefix_dot) {
            total_children += 1;
            let seg = rest.split('.').next()
                .and_then(|s| s.split('[').next())
                .unwrap_or(rest)
                .to_string();
            if !child_names.contains(&seg) { child_names.push(seg); }
        }
    }

    if total_children == 0 { return None; }

    child_names.sort();
    let shown: Vec<String> = child_names.iter().take(8).map(|s| format!("`{}`", s)).collect();
    let more = if child_names.len() > 8 {
        format!(" … and {} more", child_names.len() - 8)
    } else { String::new() };

    Some(format!(
        "**`{}`** — DATA table / group\n\n**Children:** {}{}\n\nRuntime access:\n```rust\nlet val = data.get(\"{}.property\")?;\n```",
        name, shown.join(", "), more, name
    ))
}

// ── Main identifier dispatcher ─────────────────────────────────────────────────

fn hover_identifier(
    doc:         &Document,
    name:        &str,
    section:     SectionId,
    token_index: usize,
) -> Option<String> {
    // ── Priority 0: control-flow identifiers (log, if, elif, chk) ─────────────
    // These are emitted as Identifier tokens (not Keyword) followed by ControlFlowColon.
    let next_is_cf_colon = doc.tokens.get(token_index + 1)
        .map(|t| matches!(t.token_type, TokenType::Keyword("if") | TokenType::Keyword("elif") | TokenType::Keyword("else") | TokenType::Keyword("chk") | TokenType::Keyword("log")))
        .unwrap_or(false);
    if next_is_cf_colon {
        if let Some(kw) = hover_keyword(name) { return Some(kw); }
    }

    // `miss` is the default case label in chk: — no colon after it
    if name == "miss" && section == SectionId::QuickFuncs {
        return hover_keyword(name);
    }

    // ── Priority 0.5: instance/static/namespace method (after dot) ────────────
    if let Some(content) = hover_after_dot(doc, name, section, token_index) {
        return Some(content);
    }

    // ── Priority 0.6: 3-part imported enum field: ns.EnumName.FIELD ──────────
    if let Some(content) = hover_imported_enum_field_at(doc, name, token_index) {
        return Some(content);
    }

    // ── Priority 0.7: DLM module / subtype names ─────────────────────────────
    if let Some(dlm) = hover_dlm_module(name) { return Some(dlm); }
    if let Some(dlm) = hover_dlm_subtype(name) { return Some(dlm); }

    // ── Priority 1: QuickFuncs section — params and local vars ────────────────
    if section == SectionId::QuickFuncs {
        if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
            for func in &qf.functions {
                for param in &func.parameters {
                    if param.name != name { continue; }
                    let type_str = param.data_type
                        .map(format_dt_type_str)
                        .unwrap_or_else(|| "any".to_string());
                    let default_note = if param.default_value.is_some() {
                        "\n\n*(has a default value)*"
                    } else { "" };
                    let method_hint = param.data_type
                        .and_then(ast_data_type_to_dix_type)
                        .map(|dix| {
                            instance_method_registry::initialize();
                            let methods = instance_method_registry::get_instance_methods(dix);
                            if methods.is_empty() { return String::new(); }
                            let shown: Vec<&str> = methods.iter().take(5).map(|s| s.as_str()).collect();
                            format!(
                                "\n\nType methods: {} … *(type `.` to see all)*",
                                shown.iter().map(|m| format!("`{}`", m)).collect::<Vec<_>>().join(", ")
                            )
                        })
                        .unwrap_or_default();
                    return Some(format!(
                        "**`{}`** — parameter of `~{}`\n\nType: `<{}>`{}{}",
                        name, func.name, type_str, default_note, method_hint
                    ));
                }
            }
        }
        if let Some(content) = hover_qf_local_var(doc, name) { return Some(content); }
    }

    // ── Priority 2: QuickFunc declaration / call site ─────────────────────────
    if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
        for func in &qf.functions {
            if func.name != name { continue; }
            let params: Vec<String> = func.parameters.iter()
                .map(|p| {
                    let t = p.data_type.map(|dt| format!("<{}>", dt)).unwrap_or_default();
                    let d = if p.default_value.is_some() { " = …" } else { "" };
                    format!("{}{}{}", p.name, t, d)
                })
                .collect();
            let ret = func.return_type.map(|t| format!("{}", t)).unwrap_or_else(|| "?".to_string());
            let scopes = func.scope_list.as_ref()
                .map(|s| format!("\n\n**Scope:** `=> {}`", s.join(", ")))
                .unwrap_or_default();
            let doc_comment = extract_doc_comment_for_func(&doc.tokens, func.position.line)
                .map(|c| format!("{}\n\n---\n\n", c))
                .unwrap_or_default();
            let param_names: Vec<&str> = func.parameters.iter().map(|p| p.name.as_str()).collect();
            return Some(format!(
                "{}**`~{}<{}>({})` — QuickFunc**\n\nCompile-time function.{}\n\n```mdix\n{}({})\n```",
                doc_comment, name, ret, params.join(", "), scopes, name, param_names.join(", ")
            ));
        }
    }

    // ── Priority 3: Enum type name ────────────────────────────────────────────
    if let Some(enums) = doc.ast.as_ref().and_then(|a| a.enums.as_ref()) {
        for decl in &enums.enums {
            if decl.name != name { continue; }
            let fields: Vec<String> = decl.fields.iter()
                .map(|f| {
                    let v = f.value.map(|n| format!(" = {}", n)).unwrap_or_default();
                    format!("`{}{}`", f.name, v)
                })
                .collect();
            return Some(format!(
                "**`{}`** — enum type\n\n**Fields:** {}\n\nAccess: `{}.FIELD_NAME`",
                name, fields.join(", "), name
            ));
        }
    }

    // ── Priority 4: Builtin static objects ───────────────────────────────────
    static_object_registry::initialize_static_registry();
    if static_object_registry::has_static_object(name) {
        return hover_static_object(name);
    }

    // ── Priority 5: Semantic symbol table ─────────────────────────────────────
    if let Some(st) = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {
        // Direct data variable
        if let Some(var) = st.try_get_data_variable(name)
            .or_else(|| st.try_get_data_variable(&format!("DATA.{}", name)))
        {
            return Some(format_data_var_hover(name, var));
        }

        // Suffix match for dotted paths (e.g. hover on "speed" finds DATA.player.config.speed)
        let suffix = format!(".{}", name);
        let mut best: Option<(usize, String, bool, Option<DataType>)> = None;
        for (path, var) in &st.data_variables {
            if !path.ends_with(&suffix) { continue; }
            let spec = path.len();
            match &best {
                None => best = Some((spec, path.clone(), var.is_inferred, var.effective_type())),
                Some((bs, _, _, _)) if spec > *bs =>
                    best = Some((spec, path.clone(), var.is_inferred, var.effective_type())),
                _ => {}
            }
        }
        if let Some((_, path, is_inferred, eff_type)) = best {
            let type_str = eff_type.map(format_dt_type_str).unwrap_or_else(|| "unknown".to_string());
            let inferred = if is_inferred { " *(inferred)*" } else { "" };
            let access   = path.strip_prefix("DATA.").unwrap_or(path.as_str());
            return Some(format!(
                "**`{}`** — DATA property\n\nFull path: `{}`\nType: `<{}>`{}\n\nRuntime access:\n```rust\nlet val: {} = data.get(\"{}\")?;\n```",
                name, access, type_str, inferred, type_str, access
            ));
        }

        // Imported namespace alias
        if let Some(ns) = st.try_get_namespace(name) {
            let funcs: Vec<String> = ns.functions.keys().take(6).map(|f| format!("`{}`", f)).collect();
            let enums_list: Vec<String> = ns.enums.keys().take(4).map(|e| format!("`{}`", e)).collect();
            return Some(format!(
                "**`{}`** — imported namespace\n\nFile: `{}`\n\n**Functions ({}):** {}\n\n**Enums ({}):** {}\n\nCall: `{}.funcName(…)`  Access enum: `{}.EnumName.FIELD`",
                name, ns.file_path,
                ns.functions.len(), funcs.join(", "),
                ns.enums.len(), enums_list.join(", "),
                name, name
            ));
        }
    }

    // ── Priority 6: Table path prefix in @DATA ─────────────────────────────
    if section == SectionId::Data {
        if let Some(content) = hover_table_path_prefix(doc, name) {
            return Some(content);
        }
    }

    None
}

// ── Format DATA variable hover ─────────────────────────────────────────────────

fn format_data_var_hover(name: &str, var: &dixscript::Compiler::Utilities::VariableInfo) -> String {
    let type_str = var.effective_type().map(format_dt_type_str).unwrap_or_else(|| "unknown".to_string());
    let inferred = if var.is_inferred { " (inferred)" } else { "" };
    let method_hint = var.effective_type()
        .and_then(ast_data_type_to_dix_type)
        .map(|dix| {
            instance_method_registry::initialize();
            let methods = instance_method_registry::get_instance_methods(dix);
            if methods.is_empty() { return String::new(); }
            let shown: Vec<&str> = methods.iter().take(5).map(|s| s.as_str()).collect();
            format!(
                "\n\nType methods: {} … *(type `.` to see all)*",
                shown.iter().map(|m| format!("`{}`", m)).collect::<Vec<_>>().join(", ")
            )
        })
        .unwrap_or_default();

    format!(
        "**`{}`** — DATA variable\n\nType: `<{}>`{}\n\nRuntime access:\n```rust\nlet val: {} = data.get(\"{}\")?;\n```{}",
        name, type_str, inferred, type_str, name, method_hint
    )
}

// ── Static object hover ───────────────────────────────────────────────────────

fn hover_static_object(name: &str) -> Option<String> {
    let (desc, methods) = match name {
        "Math"      => ("Mathematical functions.",
            vec!["sqrt(x)","pow(base,exp)","abs(x)","floor(x)","ceil(x)","round(x)",
                 "min(a,b)","max(a,b)","clamp(v,min,max)","sin(x)","cos(x)","tan(x)",
                 "log(x)","pi()","e()"]),
        "DateTime"  => ("Date and time utilities.",
            vec!["now()","today()","format(ts,pat)","year(d)","month(d)","day(d)",
                 "addDays(d,n)","subtract(a,b)","isLeapYear(y)"]),
        "Array"     => ("Array factory functions.",
            vec!["empty()","range(start,end)","fill(val,count)","of(…vals)","sort(arr)",
                 "unique(arr)","flatten(arr)","sum(arr)","average(arr)","min(arr)","max(arr)"]),
        "Random"    => ("Pseudo-random generation.",
            vec!["range(min,max)","nextFloat()","nextDouble()","nextBool()","choice(arr)",
                 "shuffle(arr)","alphanumeric(len)"]),
        "Guid"      => ("GUID / UUID v4 generation.",
            vec!["new()","parse(str)","validate(str)","empty()","format(guid,fmt)"]),
        "IpAddress" => ("IPv4 and IPv6 utilities.",
            vec!["parse(str)","validate(str)","isV4(str)","isV6(str)","isPrivate(str)",
                 "isLoopback(str)","localhost()","anyAddress()"]),
        "Enum"      => ("Runtime enum introspection.",
            vec!["getValues(name)","getName(name,val)","getValue(name,field)","count(name)",
                 "exists(name)","list()"]),
        "Dix"       => ("Logging and string utilities.",
            vec!["Log(msg)","LogInfo(msg)","LogWarning(msg)","LogError(msg)","Assert(cond,msg)",
                 "Format(tmpl,...args)","Join(sep,...vals)"]),
        _ => return None,
    };
    Some(format!(
        "**{}** — built-in static object\n\n{}\n\nMethods: {}\n\nType `.` after `{}` for completions.",
        name, desc,
        methods.iter().map(|m| format!("`{}`", m)).collect::<Vec<_>>().join(", "),
        name
    ))
}

// ── Static method hover ───────────────────────────────────────────────────────

fn hover_static_method(class: &str, method: &str) -> Option<String> {
    let entry = STATIC_SIGS.iter().find(|(c, m, _, _, _)| *c == class && *m == method)?;
    Some(format!(
        "**`{}`** — built-in static method\n\n{}\n\n**Returns:** see signature\n\n```mdix\n// Example:\n{}\n```",
        entry.2, entry.3, entry.4
    ))
}

// ── HexColor hover ────────────────────────────────────────────────────────────

fn hover_hex_color(hex: &str) -> Option<String> {
    let digits = hex.trim_start_matches('#');
    let (r, g, b, a, has_alpha): (u8, u8, u8, u8, bool) = match digits.len() {
        3 => {
            let e = |s: &str| -> Option<u8> { u8::from_str_radix(s, 16).ok().map(|n| (n << 4) | n) };
            (e(&digits[0..1])?, e(&digits[1..2])?, e(&digits[2..3])?, 255, false)
        }
        4 => {
            let e = |s: &str| -> Option<u8> { u8::from_str_radix(s, 16).ok().map(|n| (n << 4) | n) };
            (e(&digits[0..1])?, e(&digits[1..2])?, e(&digits[2..3])?, e(&digits[3..4])?, true)
        }
        6 => (
            u8::from_str_radix(&digits[0..2], 16).ok()?,
            u8::from_str_radix(&digits[2..4], 16).ok()?,
            u8::from_str_radix(&digits[4..6], 16).ok()?,
            255, false,
        ),
        8 => (
            u8::from_str_radix(&digits[0..2], 16).ok()?,
            u8::from_str_radix(&digits[2..4], 16).ok()?,
            u8::from_str_radix(&digits[4..6], 16).ok()?,
            u8::from_str_radix(&digits[6..8], 16).ok()?,
            true,
        ),
        _ => return None,
    };
    let alpha_line = if has_alpha {
        let pct = (a as f32 / 255.0 * 100.0).round() as u32;
        format!("Alpha | {} | {:02X} | {}% opacity", a, a, pct)
    } else {
        "Alpha | — | — | No alpha channel".to_string()
    };
    Some(format!(
        "**HexColor** `#{}`\n\n| Channel | Dec | Hex |\n|---------|-----|-----|\n| Red   | {} | {:02X} |\n| Green | {} | {:02X} |\n| Blue  | {} | {:02X} |\n| {} |\n\nType: `<hex>`",
        digits.to_uppercase(), r, r, g, g, b, b, alpha_line
    ))
}

// ── Date / Timestamp hover ─────────────────────────────────────────────────────

fn hover_date(date_str: &str) -> Option<String> {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 { return None; }
    let year: u32  = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32   = parts[2].parse().ok()?;
    let mname = month_name(month)?;
    let suf   = ordinal_suffix(day);
    Some(format!(
        "**Date:** `{}`\n\n{} {}{}, {}\n\nType: `<date>`",
        date_str, mname, day, suf, year
    ))
}

fn hover_timestamp(ts: &str) -> Option<String> {
    let tz = if ts.ends_with('Z') { "UTC" }
             else if ts.contains('+') { "with UTC offset" }
             else { "local time" };
    Some(format!("**Timestamp:** `{}`\n\n*{}*\n\nType: `<timestamp>`", ts, tz))
}

// ── Regex / Blob hover ─────────────────────────────────────────────────────────

fn hover_regex(tokens: &[Token], constructor_index: usize) -> String {
    let pattern = find_adjacent_string(tokens, constructor_index);
    match pattern {
        None => concat!(
            "**`r:(...)`** — regex constructor\n\n",
            "```mdix\nemail = r:(\"^[\\\\w.]+@[\\\\w.]+$\")\n```\n\n",
            "Type: `<regex>`\n\n",
            "Methods: `.test(str)`, `.match(str)`, `.replace(str,repl)`, `.split(str)`, `.isValid()`"
        ).to_string(),
        Some(pat) => match regex::Regex::new(&pat) {
            Ok(re) => {
                let groups = re.captures_len().saturating_sub(1);
                format!(
                    "**`r:(...)`** — regex\n\n```\n{}\n```\n\n✅ Valid — {} capture group{}\n\nType: `<regex>`",
                    pat, groups, if groups == 1 { "" } else { "s" }
                )
            }
            Err(e) => format!(
                "**`r:(...)`** — regex\n\n```\n{}\n```\n\n❌ Invalid: {}\n\nType: `<regex>`",
                pat, e.to_string().lines().next().unwrap_or("parse error")
            ),
        },
    }
}

fn hover_blob(tokens: &[Token], constructor_index: usize) -> String {
    let data = find_adjacent_string(tokens, constructor_index);
    match data {
        None => concat!(
            "**`b:(...)`** — blob constructor\n\nBase64-encoded binary data.\n\n",
            "Type: `<blob>`\n\n",
            "Methods: `.size()`, `.mimeType()`, `.toHex()`, `.toBytes()`, `.isValid()`, `.slice(start,end)`"
        ).to_string(),
        Some(b64) => {
            use base64::{engine::general_purpose, Engine as _};
            match general_purpose::STANDARD.decode(&b64) {
                Ok(bytes) => {
                    let category = detect_mime(&bytes);
                    let size = if bytes.len() >= 1_048_576 { format!("{}MB", bytes.len() / 1_048_576) }
                               else if bytes.len() >= 1024  { format!("{}KB", bytes.len() / 1024) }
                               else { format!("{}B", bytes.len()) };
                    format!("**`b:(...)`** — blob\n\n📦 {} · MIME category: `{}`\n\nType: `<blob>`", size, category)
                }
                Err(_) => format!(
                    "**`b:(...)`** — blob\n\n⚠️ {} chars — invalid base64\n\nType: `<blob>`",
                    b64.len()
                ),
            }
        }
    }
}

fn detect_mime(bytes: &[u8]) -> &'static str {
    if bytes.len() < 4 { return "application/octet-stream"; }
    match bytes {
        b if b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF => "image/jpeg",
        b if b[0] == 0x89 && b[1] == 0x50 && b[2] == 0x4E && b[3] == 0x47 => "image/png",
        b if b[0] == 0x47 && b[1] == 0x49 && b[2] == 0x46 => "image/gif",
        b if b[0] == 0x25 && b[1] == 0x50 && b[2] == 0x44 && b[3] == 0x46 => "application/pdf",
        b if b[0] == 0x50 && b[1] == 0x4B => "application/zip",
        b if b[0] == 0x1F && b[1] == 0x8B => "application/gzip",
        _ => "application/octet-stream",
    }
}

// ── Token-at-position lookup ───────────────────────────────────────────────────

pub fn token_and_index_at(tokens: &[Token], pos: Position) -> Option<(&Token, usize)> {
    let target_line = pos.line as usize + 1;
    let target_col  = pos.character as usize + 1;
    let mut best: Option<(&Token, usize)> = None;
    for (i, token) in tokens.iter().enumerate() {
        if token.line < target_line { continue; }
        if token.line > target_line { break; }
        if token.column > target_col { break; }
        let len = token_value_len(token);
        if target_col <= token.column + len { best = Some((token, i)); }
    }
    best
}

fn token_value_len(token: &Token) -> usize {
    match &token.token_type {
        TokenType::String(s)             => s.len() + 2,
        TokenType::StringSingle(s)       => s.len() + 2,
        TokenType::InterpolatedString(s) => s.len() + 3,
        TokenType::HexColor(h)           => h.len() + 1,
        TokenType::Comment(c)            => c.len() + 2,
        TokenType::Long(l)               => format!("{}L", l).len(),
        TokenType::Bool(b)               => if *b { 4 } else { 5 },

        TokenType::SectionConfig     => 7,
        TokenType::SectionImports    => 8,
        TokenType::SectionDLM        => 4,
        TokenType::SectionEnums      => 6,
        TokenType::SectionQuickFuncs => 11,
        TokenType::SectionData       => 5,
        TokenType::SectionSecurity   => 9,
        TokenType::DoubleColon       => 2,
        TokenType::Arrow             => 2,
        TokenType::SwitchCase        => 2,
        _ => {
            let v = token.get_token_value();
            if v.is_empty() { 1 } else { v.len() }
        }
    }
}

fn find_adjacent_string(tokens: &[Token], start_index: usize) -> Option<String> {
    for token in tokens.iter().skip(start_index + 1).take(5) {
        match &token.token_type {
            TokenType::String(s) | TokenType::StringSingle(s) => return Some(s.clone()),
            TokenType::SectionData | TokenType::EndOfFile      => break,
            _ => {}
        }
    }
    None
}

fn extract_doc_comment_for_func(tokens: &[Token], func_def_line: usize) -> Option<String> {
    if func_def_line == 0 { return None; }
    let search_start = func_def_line.saturating_sub(60);
    let mut spans: Vec<(usize, usize, String)> = tokens
        .iter()
        .filter(|t| t.line >= search_start && t.line < func_def_line)
        .filter_map(|t| {
            if let TokenType::Comment(c) = &t.token_type {
                let newlines = c.chars().filter(|&ch| ch == '\n').count();
                let end_line = t.line + newlines;
                if end_line < func_def_line { return Some((t.line, end_line, c.clone())); }
            }
            None
        })
        .collect();
    if spans.is_empty() { return None; }
    spans.sort_by_key(|(s, _, _)| *s);

    let mut collected: Vec<String> = Vec::new();
    let mut expected_end = func_def_line.saturating_sub(1);
    for (start, end, content) in spans.iter().rev() {
        if *end == expected_end {
            collected.insert(0, content.clone());
            expected_end = start.saturating_sub(1);
        } else { break; }
    }
    if collected.is_empty() { return None; }

    let raw = collected.join("\n").trim().to_string();
    let cleaned: String = raw.lines()
        .map(clean_doc_comment_line)
        .collect::<Vec<_>>()
        .join("\n");
    Some(cleaned)
}

/// Strip a single line's leading comment-marker remnant after the lexer has
/// already consumed the comment's own opening `//`:
///   `///  text` → token content `/  text` → strip leading `/` → `text`
///   `//!  text` → token content `!  text` → strip leading `!` → `text`
///
/// Only ever strips from the very start of the line, so fenced ```code```
/// examples inside the doc comment (division, paths, nested `//` comments,
/// etc.) are untouched unless they themselves happen to open with a stray
/// `/` or `!` immediately after the doc marker.
fn clean_doc_comment_line(l: &str) -> &str {
    let l = l.trim_start_matches('/').trim_start();
    let l = l.strip_prefix('!').unwrap_or(l);
    l.trim_start()
}

fn hover_config_line(doc: &Document, pos: Position) -> Option<Hover> {
    let line_text = doc.source.lines().nth(pos.line as usize)?;
    let trimmed   = line_text.trim();

    if trimmed.to_uppercase().starts_with("@CONFIG") {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind:  MarkupKind::Markdown,
                value: section_hover("@CONFIG", "Compiler settings and file metadata.",
                    "@CONFIG(\n  version -> \"1.0.0\"\n  debug_mode -> \"off\"\n)",
                    "Keys: `version`, `author`, `created`, `encoding`, `debug_mode`, `error_handling`, `compatibility_mode`, `features`."
                ),
            }),
            range: None,
        });
    }

    if let Some(arrow_byte) = line_text.find("->") {
        let key_raw = line_text[..arrow_byte].trim();
        let key_valid = !key_raw.is_empty()
            && !key_raw.starts_with('@')
            && !key_raw.starts_with("//")
            && key_raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ' ');
        if key_valid {
            let key = key_raw.trim();
            let content = hover_config_key(key)?;
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind:  MarkupKind::Markdown,
                    value: content,
                }),
                range: None,
            });
        }
    }
    None
}

// ── Calendar helpers ───────────────────────────────────────────────────────────

fn month_name(m: u32) -> Option<&'static str> {
    match m {
        1 => Some("January"),   2 => Some("February"),  3 => Some("March"),
        4 => Some("April"),     5 => Some("May"),        6 => Some("June"),
        7 => Some("July"),      8 => Some("August"),     9 => Some("September"),
        10=> Some("October"),  11 => Some("November"), 12 => Some("December"),
        _ => None,
    }
}

fn ordinal_suffix(d: u32) -> &'static str {
    match d {
        11 | 12 | 13         => "th",
        n if n % 10 == 1     => "st",
        n if n % 10 == 2     => "nd",
        n if n % 10 == 3     => "rd",
        _                    => "th",
    }
                                                      }
