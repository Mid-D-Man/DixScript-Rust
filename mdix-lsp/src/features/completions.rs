// mdix-lsp/src/features/completions.rs
//! Completion provider.
//!
//! Triggered by: '@', '.', '<', '~'
//! Provides: section snippets, enum values, function names, type annotations,
//! built-in static object methods, keyword completions inside QuickFuncs.
//!
//! The trigger character is taken from the LSP request context when present.
//! Source-text inference (`trigger_char`) is only used as a fallback for
//! editors that do not populate `context.triggerCharacter`.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse,
    Documentation, InsertTextFormat, MarkupContent, MarkupKind, Position,
};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use dixscript::Compiler::AST::DixScript;
use crate::document::Document;

/// Entry point called from `server.rs`.
///
/// `trigger` — the `triggerCharacter` from the LSP `CompletionContext`, if
/// the editor provided one.  When `None`, the character immediately before
/// the cursor is used instead.
pub fn provide(
    doc: Option<&Document>,
    pos: Position,
    trigger: Option<&str>,
) -> Option<CompletionResponse> {
    let items = match doc {
        None => section_snippet_completions(),
        Some(d) => {
            // Prefer the LSP context trigger; fall back to source inference.
            let trigger_ch: char = trigger
                .and_then(|t| t.chars().next())
                .unwrap_or_else(|| trigger_char(&d.source, pos));

            match trigger_ch {
                '@' => section_snippet_completions(),
                '<' => type_annotation_completions(),
                '~' => vec![],
                '.' => dot_completions(d, pos),
                _   => general_completions(d, pos),
            }
        }
    };

    if items.is_empty() {
        None
    } else {
        Some(CompletionResponse::Array(items))
    }
}

// ── Section snippets ──────────────────────────────────────────────────────────

fn section_snippet_completions() -> Vec<CompletionItem> {
    let sections: &[(&str, &str, &str)] = &[
        (
            "@CONFIG",
            "@CONFIG(\n  version -> \"1.0.0\"\n  features -> \"advanced\"\n  error_handling -> \"halt\"\n)",
            "Compiler settings section",
        ),
        (
            "@IMPORTS",
            "@IMPORTS(\n  ${1:Alias} from \"${2:path/to/file.mdix}\"\n)",
            "Import other .mdix files",
        ),
        (
            "@DLM",
            "@DLM(\n  DCompressor.gzip\n  DEncryptor.aes256\n)",
            "Data Lifecycle Modules (compression, encryption, auditing)",
        ),
        (
            "@ENUMS",
            "@ENUMS(\n  ${1:EnumName} { ${2:VALUE_A}, ${3:VALUE_B} }\n)",
            "Named integer constants",
        ),
        (
            "@QUICKFUNCS",
            "@QUICKFUNCS(\n  ~${1:funcName}<${2:int}>(${3:param}) {\n    return ${4:param}\n  }\n)",
            "Compile-time functions",
        ),
        (
            "@DATA",
            "@DATA(\n  ${1:key} = ${2:value}\n)",
            "Data payload section",
        ),
        (
            "@SECURITY",
            "@SECURITY(\n  encryption -> {\n    mode = \"password\",\n    algorithm = \"aes256-gcm\"\n  }\n)",
            "Encryption configuration",
        ),
    ];

    sections
        .iter()
        .map(|(label, snippet, doc)| CompletionItem {
            label:               label.to_string(),
            kind:                Some(CompletionItemKind::MODULE),
            detail:              Some("DixScript section".to_string()),
            documentation:       Some(Documentation::MarkupContent(MarkupContent {
                kind:  MarkupKind::Markdown,
                value: doc.to_string(),
            })),
            insert_text:         Some(snippet.to_string()),
            insert_text_format:  Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        })
        .collect()
}

// ── Type annotation completions  (<int>, <string>, …) ─────────────────────────

fn type_annotation_completions() -> Vec<CompletionItem> {
    let types = [
        ("int",       "32-bit signed integer"),
        ("float",     "32-bit float (use f suffix: 3.14f)"),
        ("double",    "64-bit float"),
        ("string",    "UTF-8 string"),
        ("bool",      "Boolean: true or false"),
        ("array",     "Homogeneous ordered collection"),
        ("tuple",     "Mixed-type collection (max 4 elements)"),
        ("object",    "Key-value map"),
        ("hex",       "Hex colour or integer: #FF5733 or 0xFF"),
        ("blob",      "Base64-encoded binary: b:(\"...\")"),
        ("regex",     "Regular expression: r:(\"...\")"),
        ("date",      "ISO date: 2025-01-15"),
        ("timestamp", "ISO timestamp: 2025-01-15T10:30:00Z"),
        ("enum",      "Enum value from @ENUMS"),
    ];

    types
        .iter()
        .map(|(name, detail)| CompletionItem {
            label:              format!("<{}>", name),
            kind:               Some(CompletionItemKind::TYPE_PARAMETER),
            detail:             Some(detail.to_string()),
            insert_text:        Some(name.to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..Default::default()
        })
        .collect()
}

// ── Dot completions  (EnumName. → values, StaticObj. → methods) ───────────────

fn dot_completions(doc: &Document, pos: Position) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    let word_before = word_before_dot(&doc.source, pos);
    if word_before.is_empty() {
        return items;
    }

    // Check if the word matches a known enum name.
    if let Some(ast) = &doc.ast {
        items.extend(enum_value_completions(ast, &word_before));
    }

    // Check if the word matches a known static object.
    items.extend(static_method_completions(&word_before));

    // Check if it matches an imported namespace.
    if let Some(sr) = doc.semantic_result.as_ref() {
        if let Some(st) = &sr.symbol_table {
            if let Some(ns) = st.try_get_namespace(&word_before) {
                for func_name in ns.functions.keys() {
                    items.push(CompletionItem {
                        label:  func_name.clone(),
                        kind:   Some(CompletionItemKind::FUNCTION),
                        detail: Some(format!("from {}", ns.alias)),
                        ..Default::default()
                    });
                }
                for enum_name in ns.enums.keys() {
                    items.push(CompletionItem {
                        label:  enum_name.clone(),
                        kind:   Some(CompletionItemKind::ENUM),
                        detail: Some(format!("enum from {}", ns.alias)),
                        ..Default::default()
                    });
                }
            }
        }
    }

    items
}

fn enum_value_completions(ast: &DixScript, enum_name: &str) -> Vec<CompletionItem> {
    let enums = match &ast.enums {
        Some(e) => e,
        None    => return vec![],
    };

    for decl in &enums.enums {
        if decl.name == enum_name {
            return decl
                .fields
                .iter()
                .map(|f| {
                    let detail = f.value
                        .map(|v| format!("= {}", v))
                        .unwrap_or_default();
                    CompletionItem {
                        label:  f.name.clone(),
                        kind:   Some(CompletionItemKind::ENUM_MEMBER),
                        detail: Some(detail),
                        ..Default::default()
                    }
                })
                .collect();
        }
    }

    vec![]
}

fn static_method_completions(object_name: &str) -> Vec<CompletionItem> {
    let catalogue: &[(&str, &[(&str, &str)])] = &[
        ("Math", &[
            ("sqrt",  "Math.sqrt(x: double) -> double"),
            ("round", "Math.round(x: double) -> int"),
            ("abs",   "Math.abs(x) -> same"),
            ("floor", "Math.floor(x: double) -> int"),
            ("ceil",  "Math.ceil(x: double) -> int"),
            ("min",   "Math.min(a, b) -> same"),
            ("max",   "Math.max(a, b) -> same"),
            ("pow",   "Math.pow(base, exp) -> double"),
            ("clamp", "Math.clamp(v, min, max) -> same"),
        ]),
        ("DateTime", &[
            ("now",      "DateTime.now() -> timestamp"),
            ("today",    "DateTime.today() -> date"),
            ("format",   "DateTime.format(ts, pattern) -> string"),
            ("year",     "DateTime.year(d) -> int"),
            ("month",    "DateTime.month(d) -> int"),
            ("day",      "DateTime.day(d) -> int"),
            ("subtract", "DateTime.subtract(a, b) -> int"),
        ]),
        ("Array", &[
            ("sort",    "Array.sort(arr) -> array"),
            ("reverse", "Array.reverse(arr) -> array"),
            ("slice",   "Array.slice(arr, start, end) -> array"),
            ("sum",     "Array.sum(arr) -> double"),
            ("range",   "Array.range(start, end) -> array"),
            ("length",  "Array.length(arr) -> int"),
            ("first",   "Array.first(arr) -> any"),
            ("last",    "Array.last(arr) -> any"),
        ]),
        ("Random", &[
            ("range",  "Random.range(min, max) -> int"),
            ("choice", "Random.choice(arr) -> any"),
        ]),
        ("Guid", &[
            ("new", "Guid.new() -> string"),
        ]),
    ];

    for (obj, methods) in catalogue {
        if *obj == object_name {
            return methods
                .iter()
                .map(|(method, sig)| CompletionItem {
                    label:  method.to_string(),
                    kind:   Some(CompletionItemKind::METHOD),
                    detail: Some(sig.to_string()),
                    ..Default::default()
                })
                .collect();
        }
    }

    vec![]
}

// ── General completions (no special trigger) ──────────────────────────────────

fn general_completions(doc: &Document, _pos: Position) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // QuickFunc names from the current document.
    if let Some(ast) = &doc.ast {
        if let Some(qf) = &ast.quick_functions {
            for func in &qf.functions {
                let params: Vec<String> = func
                    .parameters
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                items.push(CompletionItem {
                    label:  func.name.clone(),
                    kind:   Some(CompletionItemKind::FUNCTION),
                    detail: Some(format!(
                        "~{}<{}>({}) — QuickFunc",
                        func.name,
                        func.return_type
                            .as_ref()
                            .map(|t| format!("{:?}", t))
                            .unwrap_or_else(|| "?".to_string()),
                        params.join(", ")
                    )),
                    ..Default::default()
                });
            }
        }
    }

    // Keywords valid inside QuickFunc bodies.
    let keywords = ["return", "let", "if:", "elif:", "else", "chk:", "log:", "null", "true", "false"];
    for kw in keywords {
        items.push(CompletionItem {
            label: kw.to_string(),
            kind:  Some(CompletionItemKind::KEYWORD),
            ..Default::default()
        });
    }

    // Built-in static object names.
    for obj in &["Math", "DateTime", "Array", "Random", "Guid", "Dix", "Enum"] {
        items.push(CompletionItem {
            label:  obj.to_string(),
            kind:   Some(CompletionItemKind::CLASS),
            detail: Some("built-in static object".to_string()),
            ..Default::default()
        });
    }

    items
}

// ── Source text helpers ───────────────────────────────────────────────────────

/// Returns the character that immediately precedes the cursor on the same line.
/// Used only when the LSP context does not supply `triggerCharacter`.
fn trigger_char(source: &str, pos: Position) -> char {
    let line = source.lines().nth(pos.line as usize).unwrap_or("");
    if pos.character == 0 {
        return '\0';
    }
    line.chars().nth((pos.character - 1) as usize).unwrap_or('\0')
}

/// Returns the identifier word immediately before the '.' that triggered completion.
fn word_before_dot(source: &str, pos: Position) -> String {
    let line = source.lines().nth(pos.line as usize).unwrap_or("");
    let up_to = line
        .char_indices()
        .take_while(|(i, _)| *i < pos.character.saturating_sub(1) as usize)
        .map(|(_, c)| c)
        .collect::<String>();

    up_to
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .last()
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::run_pipeline;
    use crate::document::Document;
    use tower_lsp::lsp_types::{Position, Url};

    fn test_doc(source: &str) -> Document {
        let mut doc = Document::new(
            Url::parse("file:///test.mdix").unwrap(),
            source.to_string(),
            0,
        );
        run_pipeline(&mut doc);
        doc
    }

    #[test]
    fn section_snippet_completions_are_non_empty() {
        let items = section_snippet_completions();
        assert!(!items.is_empty(), "should return section snippets");
    }

    #[test]
    fn section_snippet_completions_include_all_sections() {
        let items  = section_snippet_completions();
        let labels: Vec<&str> = items.iter()
            .filter_map(|i| i.label.as_str().into())
            .collect();
        for expected in &["@CONFIG", "@IMPORTS", "@DLM", "@ENUMS",
                          "@QUICKFUNCS", "@DATA", "@SECURITY"] {
            assert!(labels.iter().any(|l| l == expected),
                "missing section completion: {}", expected);
        }
    }

    #[test]
    fn type_annotation_completions_include_primitives() {
        let items = type_annotation_completions();
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        for t in &["<int>", "<float>", "<string>", "<bool>", "<array>"] {
            assert!(labels.iter().any(|l| l == t),
                "missing type completion: {}", t);
        }
    }

    #[test]
    fn static_method_completions_math_non_empty() {
        let methods = static_method_completions("Math");
        assert!(!methods.is_empty(), "Math should have static methods");
        assert!(methods.iter().any(|m| m.label == "sqrt"), "Math.sqrt missing");
        assert!(methods.iter().any(|m| m.label == "round"), "Math.round missing");
    }

    #[test]
    fn static_method_completions_unknown_object_empty() {
        let methods = static_method_completions("NonExistentObject");
        assert!(methods.is_empty(), "unknown object should return no completions");
    }

    #[test]
    fn provide_on_none_doc_returns_section_snippets() {
        let result = provide(None, Position::new(0, 0), None);
        let _ = result;
    }

    #[test]
    fn provide_with_explicit_trigger_overrides_source_inference() {
        // Passing "<" as trigger must return type annotations even when the
        // character at that position in the source is not '<'.
        let doc    = test_doc("@DATA(\n  x = 1\n)");
        let result = provide(Some(&doc), Position::new(0, 0), Some("<"));
        let items  = match result {
            Some(CompletionResponse::Array(v)) => v,
            _ => vec![],
        };
        assert!(
            items.iter().any(|i| i.label.contains("int")),
            "explicit '<' trigger should produce type annotation completions; got: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn general_completions_include_keywords() {
        let doc    = test_doc("@DATA(\n  x = 1\n)");
        let result = general_completions(&doc, Position::new(0, 0));
        let labels: Vec<&str> = result.iter()
            .filter_map(|i| i.label.as_str().into())
            .collect();
        assert!(labels.contains(&"return"), "should include 'return' keyword");
        assert!(labels.contains(&"true"),   "should include 'true' keyword");
    }

    #[test]
    fn general_completions_include_static_objects() {
        let doc    = test_doc("@DATA(\n  x = 1\n)");
        let result = general_completions(&doc, Position::new(0, 0));
        let labels: Vec<&str> = result.iter()
            .filter_map(|i| i.label.as_str().into())
            .collect();
        assert!(labels.contains(&"Math"),     "should include Math");
        assert!(labels.contains(&"DateTime"), "should include DateTime");
        assert!(labels.contains(&"Array"),    "should include Array");
    }

    #[test]
    fn completions_quickfuncs_appear_after_pipeline() {
        let source = "@QUICKFUNCS(\n  ~calc<int>(x) { return x }\n)\n@DATA(\n  y = 1\n)";
        let doc    = test_doc(source);
        let result = general_completions(&doc, Position::new(3, 0));
        let labels: Vec<&str> = result.iter()
            .filter_map(|i| i.label.as_str().into())
            .collect();
        assert!(labels.contains(&"calc"),
            "QuickFunc 'calc' should appear in completions; got: {:?}", labels);
    }
    }
