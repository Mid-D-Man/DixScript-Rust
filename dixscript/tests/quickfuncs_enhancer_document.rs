// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/quickfuncs_enhancer_document.rs"
// ============================================================================
//! Tests for the `@QUICKFUNCS` enhancer reading a `Document`.
//!
//! The enhancer gives parameters their synthetic defaults and replaces qualified identifiers by
//! the nodes semantic analysis found them to be. It now works on the arena:
//! `QuickFunctionsAstEnhancer::enhance_doc` and `QualifiedIdentifierResolver::resolve_statement_list`
//! read a `Document`'s nodes and allocate the ones that change, and `GeneralAstEnhancer::with_document`
//! points the whole enhancer at a `Document`. The owned entry points are wrappers over the same code.
//! The tests check five things:
//!
//! * the enhanced `@QUICKFUNCS` section of every crafted source is the one the owned-tree enhancer
//!   produced before the change (the `RECORDING` table, a fingerprint of the section's debug text
//!   produced by running the original code), through the owned entry point and through the document;
//! * the same for sections built by hand, which reach what the parser cannot write: every
//!   resolution type, the fallback for an identifier with no resolution, every expression, value
//!   and statement form, and every parameter type (`HAND_RECORDING`);
//! * the enhancer really reads the `Document` when it has one and the owned tree when it does not;
//! * a node that did not change is shared with the input, not copied, and the input section means
//!   what it meant before;
//! * the owned entry points of the resolver give what the enhancer puts in a function body.

use std::collections::HashMap;

use dixscript::Compiler::AST::arena::{Arena, DocQuickFuncs, Document};
use dixscript::Compiler::AST::data_types::{CompatibilityMode, ElemType, ErrorHandlingStrategy};
use dixscript::Compiler::AST::{
    DataType, DeclarationType, DixScript, Expression, ObjectProperty, Position, QuickFuncParam,
    QuickFuncStatement, QuickFuncsSection, QuickFunction, SwitchCase, Value,
};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::SectionAnalyzers::SectionAnalysisResult;
use dixscript::Compiler::Core::SectionEnhancers::{
    QualifiedIdentifierKey, QualifiedIdentifierResolution, QualifiedIdentifierResolver,
    QualifiedIdentifierType, QuickFunctionsAstEnhancer,
};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::{GeneralAstEnhancer, GeneralParser, GeneralSemanticAnalyzer};
use dixscript::ErrorManager::{DebugConfig, ErrorManager};

// ==================== helpers ====================

struct Parsed {
    document: Document,
    ast: DixScript,
    settings: OperationalSettings,
}

fn parse(source: &str) -> Parsed {
    let label = "quickfuncs-enhancer-document-test";
    let em = ErrorManager::new_isolated_silent();
    let initial = OperationalSettings {
        source_file_path: Some(label.to_string()),
        compatibility_mode: CompatibilityMode::Strict,
        ..OperationalSettings::default()
    };
    let tokens =
        Tokenizer::new_with_error_manager_from_bytes(source.as_bytes(), &initial, em.clone()).tokenize();
    let split = split_config_tokens(tokens.tokens);
    let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
    let config = handler.process_config_tokens(&split.config_tokens);
    let mut settings = config.operational_settings;
    settings.source_file_path = Some(label.to_string());
    settings.error_handling_strategy = ErrorHandlingStrategy::Continue;
    em.update_settings(settings.clone());
    let document = GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &settings, em)
        .expect("the parser starts")
        .parse_document()
        .expect("the source parses");
    let ast = document.export();
    Parsed { document, ast, settings }
}

/// 64 bit FNV-1a, the fingerprint of a section's debug text.
fn fnv(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The length of a date (`2026-10-29`) or a timestamp (`2026-10-29T06:49:00Z`) at the start of
/// `bytes`, if there is one.
fn time_length(bytes: &[u8]) -> Option<usize> {
    let digit = |i: usize| bytes.get(i).map_or(false, u8::is_ascii_digit);
    let at = |i: usize, c: u8| bytes.get(i) == Some(&c);
    let date = (0..4).all(digit) && at(4, b'-') && digit(5) && digit(6) && at(7, b'-') && digit(8) && digit(9);
    if !date {
        return None;
    }
    let stamp = at(10, b'T') && digit(11) && digit(12) && at(13, b':') && digit(14) && digit(15)
        && at(16, b':') && digit(17) && digit(18) && at(19, b'Z');
    Some(if stamp { 20 } else { 10 })
}

/// Replaces every quoted date and timestamp by `"<D>"`. The default of a date or timestamp
/// parameter is today's date or the current time, which no recording can hold.
fn mask_times(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            if let Some(len) = time_length(&bytes[i + 1..]) {
                if bytes.get(i + 1 + len) == Some(&b'"') {
                    out.push_str("\"<D>\"");
                    i += len + 2;
                    continue;
                }
            }
        }
        let ch = text[i..].chars().next().expect("a character");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn fingerprint(section: &Option<QuickFuncsSection>) -> u64 {
    match section {
        Some(s) => fnv(&mask_times(&format!("{s:?}"))),
        None => fnv("None"),
    }
}

struct Enhanced {
    total: usize,
    section: Option<QuickFuncsSection>,
}

/// Parses, analyzes and enhances a source. With `through_document` the semantic analyzer and the
/// enhancer are given the document, as the loader does; without it they read the owned tree.
fn enhance_source(source: &str, through_document: bool) -> Enhanced {
    let mut parsed = parse(source);
    let em = ErrorManager::new_isolated_silent();
    let semantic = {
        let mut analyzer = GeneralSemanticAnalyzer::new_with_error_manager(&parsed.ast, &parsed.settings, em.clone());
        if through_document {
            analyzer = analyzer.with_document(&parsed.document);
        }
        analyzer.analyze()
    };
    let mut enhancer = GeneralAstEnhancer::new_with_error_manager(&parsed.settings, em);
    if through_document {
        enhancer = enhancer.with_document(&mut parsed.document);
    }
    let result = enhancer.enhance(&parsed.ast, Some(&semantic));
    Enhanced { total: result.total_enhancements, section: result.enhanced_ast.quick_functions }
}

// ==================== crafted sources ====================

const SOURCES: &[(&str, &str)] = &[
    ("array_methods_args", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = [3, 1, 2]
    let b = a.sort()
    let c = a.first()
    let d = a.contains(2)
    log: b
    log: c
    log: d
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("calls_between_functions", r##"@QUICKFUNCS(
  ~g<int> => global(n<int>) {
    return n * 2
  }
  ~f<int> => global() {
    let a = g(Math.abs(-2))
    let s = "x"
    let b = g(s.length)
    log: a
    log: b
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("chain_on_calls", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    let t = s.upper().lower().trim().length
    log: t
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("control_flow", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    if: s.length > 2 { log: s.upper() } else { log: s.lower() }
    chk: s.length { -> 3 { log: Math.PI } -> miss { log: s.trim() } }
    let mut n = 0
    n += Math.abs(-4)
    log: n
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("data_section_only", r##"@DATA(
  a = 1
  b = "two"
)
"##),
    ("empty_quickfuncs", r##"@QUICKFUNCS(
)
@DATA(
  a = 1
)
"##),
    ("enum_in_branches", r##"@ENUMS(
  Color { RED = 0, GREEN = 1, BLUE = 2 }
  Mode { FAST = 0, SLOW = 1 }
)
@QUICKFUNCS(
  ~f<int> => global() {
    let c = Color.GREEN
    if: c == Color.GREEN { log: Mode.FAST } else { log: Mode.SLOW }
    chk: c { -> Color.RED { log: 0 } -> miss { log: Color.BLUE } }
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("enums_with_data", r##"@ENUMS(
  Color { RED = 0, GREEN = 1, BLUE = 2 }
  Mode { FAST = 0, SLOW = 1 }
)
@QUICKFUNCS(
  ~f<int> => global() {
    let c = Color.RED
    log: c
    return 1
  }
)
@DATA(
  fav<enum> = Color.BLUE
  n = 3
)
"##),
    ("errors_in_functions", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = nofunc(1)
    let b = Missing.A
    log: b.nothing
    log: a
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("instance_calls", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    let t = s.upper()
    let u = s.upper().lower()
    let a = [1, 2, 3]
    let r = a.reverse()
    log: t
    log: u
    log: r
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("interpolation", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    let m = $"upper {s.upper()} pi {Math.PI}"
    log: m
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("lambda_statements", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let g = (s) => s.trim().upper()
    log: g(" x ")
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("lambdas_fallback", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let sq = (n) => n.abs()
    let rv = (xs) => xs.reverse()
    let rg = (k) => Array.range(1, k)
    let a = sq(2)
    log: a
    log: rv([1, 2])
    log: rg(3)
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("local_enums", r##"@ENUMS(
  Color { RED = 0, GREEN = 1, BLUE = 2 }
  Mode { FAST = 0, SLOW = 1 }
)
@QUICKFUNCS(
  ~f<int> => global() {
    let c = Color.RED
    let m = Mode.SLOW
    log: c
    log: m
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("many_functions", r##"@QUICKFUNCS(
  ~fn0<int> => global(p<int>) {
    let s = "v0"
    log: s.upper()
    log: Math.abs(p)
    return 1
  }
  ~fn1<int> => global(p<int>) {
    let s = "v1"
    log: s.upper()
    log: Math.abs(p)
    return 1
  }
  ~fn2<int> => global(p<int>) {
    let s = "v2"
    log: s.upper()
    log: Math.abs(p)
    return 1
  }
  ~fn3<int> => global(p<int>) {
    let s = "v3"
    log: s.upper()
    log: Math.abs(p)
    return 1
  }
  ~fn4<int> => global(p<int>) {
    let s = "v4"
    log: s.upper()
    log: Math.abs(p)
    return 1
  }
  ~fn5<int> => global(p<int>) {
    let s = "v5"
    log: s.upper()
    log: Math.abs(p)
    return 1
  }
  ~fn6<int> => global(p<int>) {
    let s = "v6"
    log: s.upper()
    log: Math.abs(p)
    return 1
  }
  ~fn7<int> => global(p<int>) {
    let s = "v7"
    log: s.upper()
    log: Math.abs(p)
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("nested_expressions", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    let a = (s.length + Math.abs(-1)) * (Math.max(2, 3) - s.length)
    log: a
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("params_with_explicit_defaults", r##"@QUICKFUNCS(
  ~f<int> => global(a<int> = 5, b<string> = "s", c<bool>) {
    log: a
    log: b
    log: c
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("property_chain", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let o = {a = {b = 1}}
    let x = o.a.b
    let s = "abc"
    let n = s.length
    log: x
    log: n
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("static_calls", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = Math.sqrt(16)
    let b = Math.PI
    let c = Math.abs(-3) + Math.max(1, 2)
    log: a
    log: b
    log: c
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("static_in_args", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = Math.max(Math.abs(-1), Math.sqrt(4))
    let b = Array.range(Math.abs(-1), 3)
    log: a
    log: b
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("ternary_ops", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    let a = s.length > 1 ? s.upper() : s.lower()
    let b = Math.abs(-1) + Math.abs(-2) * 2
    let c = !(s.length == 3)
    log: a
    log: b
    log: c
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("typed_params_defaults", r##"@QUICKFUNCS(
  ~f<int> => global(a<int>, b<string>, c<bool>, d<float>, e<long>, g<double>) {
    log: a
    log: b
    log: c
    log: d
    log: e
    log: g
    return 1
  }
  ~g2<int> => global(x<array<int>>, y<tuple<int,string>>, z<object>, h<hex>) {
    log: x
    log: y
    log: z
    log: h
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("unknown_identifiers", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = foo.bar
    let b = foo.baz(1)
    log: a
    log: b
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("untyped_params", r##"@QUICKFUNCS(
  ~f<int> => global(a, b) {
    log: a
    log: b
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("values_in_collections", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    let a = [s.upper(), s.lower()]
    let o = {k = s.upper(), m = Math.abs(-1)}
    let t = t:(s.length, Math.PI)
    log: a
    log: o
    log: t
    return 1
  }
)
@DATA(
  x = 1
)
"##),
];

/// (source, parameter defaults applied, fingerprint of the enhanced section), recorded from the
/// owned-tree enhancer before the change.
const RECORDING: &[(&str, usize, u64)] = &[
    ("array_methods_args", 0, 0x6407ab89cff54124),
    ("calls_between_functions", 1, 0x54b7b388b623674c),
    ("chain_on_calls", 0, 0xce6866af7862d66d),
    ("control_flow", 0, 0xd86e06f56c2c8dc2),
    ("data_section_only", 0, 0x669b18c6d2d9c95b),
    ("empty_quickfuncs", 0, 0x0d0f7f28a79a3daf),
    ("enum_in_branches", 0, 0xec8767319a30df3a),
    ("enums_with_data", 0, 0x5d380274d30268a1),
    ("errors_in_functions", 0, 0x04d6eee1fe7d27ae),
    ("instance_calls", 0, 0x1746cda029112e0b),
    ("interpolation", 0, 0x90e8f2b516b1061f),
    ("lambda_statements", 0, 0xc6a247046430a89b),
    ("lambdas_fallback", 0, 0x7dc190ff01ed9f0d),
    ("local_enums", 0, 0x951d7b8b177f59d8),
    ("many_functions", 8, 0x0e1c746ced261f3b),
    ("nested_expressions", 0, 0xa79399b0cc3a7b98),
    ("params_with_explicit_defaults", 1, 0x52119b7b25d945c0),
    ("property_chain", 0, 0xf349fd6764bd3dd3),
    ("static_calls", 0, 0x2d235f3dbcacb9ef),
    ("static_in_args", 0, 0xa6be700a610e16b5),
    ("ternary_ops", 0, 0x6b3fe714c77c8a8b),
    ("typed_params_defaults", 10, 0xb59a248834a45dec),
    ("unknown_identifiers", 0, 0xf1ec284edd6b03c1),
    ("untyped_params", 0, 0x0605f6bcedfc36b6),
    ("values_in_collections", 0, 0x9bd5cfb0ea214e0d),
];

fn recorded(name: &str) -> (usize, u64) {
    RECORDING
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, total, hash)| (*total, *hash))
        .unwrap_or_else(|| panic!("no recording for {name}"))
}

fn source(name: &str) -> &'static str {
    SOURCES.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).unwrap_or_else(|| panic!("no source {name}"))
}

#[test]
fn every_source_has_a_recording_and_the_other_way_round() {
    assert_eq!(SOURCES.len(), RECORDING.len());
    for (name, _) in SOURCES {
        recorded(name);
    }
}

#[test]
fn the_owned_enhancer_gives_the_recorded_sections() {
    for (name, text) in SOURCES {
        let enhanced = enhance_source(text, false);
        let (total, hash) = recorded(name);
        assert_eq!(enhanced.total, total, "{name}: parameter defaults applied");
        let shown = enhanced.section.as_ref().map(|s| s.to_string()).unwrap_or_default();
        assert_eq!(fingerprint(&enhanced.section), hash, "{name}: the enhanced section\n{shown}");
    }
}

#[test]
fn the_document_enhancer_gives_the_recorded_sections() {
    for (name, text) in SOURCES {
        let enhanced = enhance_source(text, true);
        let (total, hash) = recorded(name);
        assert_eq!(enhanced.total, total, "{name}: parameter defaults applied");
        let shown = enhanced.section.as_ref().map(|s| s.to_string()).unwrap_or_default();
        assert_eq!(fingerprint(&enhanced.section), hash, "{name}: the enhanced section\n{shown}");
    }
}

// ==================== hand-built sections ====================

// Hand-built sections: every branch of the resolver, the values and the defaults.
// ---------------------------------------------------------------------------
fn pos(l: usize, c: usize) -> Position {
    Position::new(l, c)
}

struct Build {
    next: usize,
    resolutions: Vec<(Position, Vec<String>, bool, QualifiedIdentifierType)>,
}

impl Build {
    fn new() -> Self {
        Build { next: 0, resolutions: Vec::new() }
    }
    /// A qualified identifier at a fresh position; `ty` records a resolution for it.
    fn q(&mut self, parts: &[&str], args: Option<Vec<Expression>>, ty: Option<QualifiedIdentifierType>) -> Expression {
        self.next += 1;
        let p = pos(100 + self.next, self.next);
        let parts: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
        if let Some(t) = ty {
            self.resolutions.push((p, parts.clone(), args.is_some(), t));
        }
        Expression::QualifiedIdentifier { parts, arguments: args, position: p }
    }
    fn map(&self) -> HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution> {
        self.resolutions
            .iter()
            .map(|(p, parts, is_call, t)| {
                (
                    QualifiedIdentifierKey { position: *p, parts: parts.clone(), is_call: *is_call },
                    QualifiedIdentifierResolution::new(*t, None, parts.clone(), *is_call, *p),
                )
            })
            .collect()
    }
}

fn eint(n: i32) -> Expression {
    Expression::Value { value: Value::Integer { value: n, position: pos(1, 1) }, position: pos(1, 1) }
}
fn eid(n: &str) -> Expression {
    Expression::Identifier { name: n.to_string(), position: pos(1, 2) }
}
fn bx(e: Expression) -> Box<Expression> {
    Box::new(e)
}
fn decl(name: &str, e: Expression) -> QuickFuncStatement {
    QuickFuncStatement::VariableDeclaration {
        declaration_type: DeclarationType::Let,
        is_mutable: true,
        variable_name: name.to_string(),
        data_type: None,
        value: e,
        position: pos(3, 3),
    }
}
fn log(e: Expression) -> QuickFuncStatement {
    QuickFuncStatement::Log { value: e, position: pos(4, 3) }
}
fn ret(e: Expression) -> QuickFuncStatement {
    QuickFuncStatement::Return { value: e, position: pos(2, 3) }
}
fn vexpr(e: Expression) -> Value {
    Value::Expression { expr: Box::new(e), position: pos(8, 1) }
}
fn function(name: &str, params: Vec<QuickFuncParam>, body: Vec<QuickFuncStatement>) -> QuickFunction {
    QuickFunction::new(name.to_string(), Some(DataType::Int), Some(vec!["global".to_string()]), params, body, pos(2, 1))
}
fn param(name: &str, t: Option<DataType>, d: Option<Expression>) -> QuickFuncParam {
    QuickFuncParam::new(name.to_string(), t, d, pos(2, 9))
}

use QualifiedIdentifierType as T;

fn hand_cases() -> Vec<(&'static str, Build, Vec<QuickFunction>)> {
    let mut v: Vec<(&'static str, Build, Vec<QuickFunction>)> = Vec::new();

    // every resolution type, with and without a call
    let mut b = Build::new();
    let body = vec![
        decl("a", b.q(&["Color", "RED"], None, Some(T::LocalEnumAccess))),
        decl("b", b.q(&["lib", "Color", "RED"], None, Some(T::ImportedEnumAccess))),
        decl("c", {
            let inner = b.q(&["s", "trim"], Some(vec![]), None);
            b.q(&["lib", "greet"], Some(vec![eint(1), inner]), Some(T::ImportedFunctionCall))
        }),
        decl("d", b.q(&["Math", "PI"], None, Some(T::StaticObjectAccess))),
        decl("e", {
            let inner = b.q(&["Math", "abs"], Some(vec![eint(-1)]), Some(T::StaticObjectAccess));
            b.q(&["Math", "sqrt"], Some(vec![inner]), Some(T::StaticObjectAccess))
        }),
        decl("f", b.q(&["o", "a", "b"], None, Some(T::ObjectPropertyAccess))),
        decl("g", b.q(&["s", "upper"], Some(vec![]), Some(T::ObjectPropertyAccess))),
        decl("h", b.q(&["o", "a", "trim"], Some(vec![eint(1)]), Some(T::ObjectPropertyAccess))),
        decl("i", b.q(&["o", "a", "b", "c", "m"], Some(vec![]), Some(T::ObjectPropertyAccess))),
        decl("j", b.q(&["lib", "Color"], None, Some(T::NamespaceEnumReference))),
        decl("k", b.q(&["x", "y"], None, Some(T::Unknown))),
        decl("l", b.q(&["x", "y", "z"], Some(vec![]), Some(T::Unknown))),
        decl("m", b.q(&["solo"], None, Some(T::ObjectPropertyAccess))),
        decl("n", b.q(&["solo"], Some(vec![]), Some(T::ObjectPropertyAccess))),
        ret(eint(1)),
    ];
    v.push(("res_all_types", b, vec![function("f", vec![], body)]));

    // no resolution recorded: the structural fallback
    let mut b = Build::new();
    let body = vec![
        decl("a", b.q(&["Math", "max"], Some(vec![eint(1), eint(2)]), None)),
        decl("b", b.q(&["s", "trim"], Some(vec![]), None)),
        decl("c", b.q(&["a", "b", "c"], Some(vec![eint(1)]), None)),
        decl("d", b.q(&["f"], Some(vec![eint(1)]), None)),
        decl("e", b.q(&["a", "b", "c"], None, None)),
        decl("f", b.q(&["a", "b"], None, None)),
        decl("g", b.q(&["a"], None, None)),
        decl("h", {
            let len = b.q(&["n", "len"], Some(vec![]), None);
            b.q(&["Array", "range"], Some(vec![eint(1), len]), None)
        }),
        decl("i", b.q(&["f"], Some(vec![]), Some(T::ObjectPropertyAccess))),
        ret(eint(1)),
    ];
    v.push(("fallback_only", b, vec![function("f", vec![], body)]));

    // a recorded resolution whose key does not match (wrong is_call) falls back too
    let mut b = Build::new();
    let q = b.q(&["s", "upper"], Some(vec![]), None);
    if let Expression::QualifiedIdentifier { parts, position, .. } = &q {
        b.resolutions.push((*position, parts.clone(), false, T::ObjectPropertyAccess));
    }
    v.push(("key_mismatch", b, vec![function("f", vec![], vec![decl("a", q), ret(eint(1))])]));

    // qualified identifiers inside every expression form the resolver walks
    let mut b = Build::new();
    let mut q = |parts: &[&str], call: bool| {
        let args = if call { Some(vec![]) } else { None };
        b.q(parts, args, Some(T::ObjectPropertyAccess))
    };
    let body = vec![
        decl("a", Expression::ArithmeticOp { left: bx(q(&["o", "x"], false)), operator: "+".into(), right: bx(q(&["o", "y"], false)), position: pos(5, 1) }),
        decl("b", Expression::BitwiseOp { left: bx(q(&["o", "x"], false)), operator: "&".into(), right: bx(q(&["o", "y"], false)), position: pos(5, 2) }),
        decl("c", Expression::ComparisonOp { left: bx(q(&["o", "x"], false)), operator: "==".into(), right: bx(q(&["o", "y"], false)), position: pos(5, 3) }),
        decl("d", Expression::LogicalOp { left: bx(q(&["o", "x"], false)), operator: "&&".into(), right: bx(q(&["o", "y"], false)), position: pos(5, 4) }),
        decl("e", Expression::UnaryOp { operator: "!".into(), operand: bx(q(&["o", "x"], false)), position: pos(5, 5) }),
        decl("f", Expression::Conditional { condition: bx(q(&["o", "c"], false)), true_value: bx(q(&["o", "t"], false)), false_value: bx(q(&["o", "f"], false)), position: pos(5, 6) }),
        decl("g", Expression::Parenthesized { expression: bx(q(&["o", "p"], false)), position: pos(5, 7) }),
        decl("h", Expression::PropertyAccess { object: bx(q(&["o", "p"], true)), property: "len".into(), position: pos(5, 8) }),
        decl("i", Expression::IndexAccess { object: bx(q(&["o", "arr"], false)), index: bx(q(&["o", "idx"], false)), position: pos(5, 9) }),
        decl("j", Expression::InstanceMethodCall { instance: bx(q(&["o", "inst"], true)), method_name: "m".into(), arguments: vec![q(&["o", "a1"], false), q(&["o", "a2"], true)], position: pos(5, 10) }),
        decl("k", Expression::StaticMethodCall { object_name: "Math".into(), method_name: "max".into(), arguments: vec![q(&["o", "a1"], false)], position: pos(5, 11) }),
        decl("l", Expression::QuickFuncCall { name: "g".into(), arguments: vec![q(&["o", "a1"], false)], position: pos(5, 12) }),
        decl("m", Expression::ImportedFunctionCall { namespace_name: "lib".into(), function_name: "f".into(), arguments: vec![q(&["o", "a1"], false)], position: pos(5, 13) }),
        decl("n", Expression::Value { value: Value::Expression { expr: bx(q(&["o", "v"], false)), position: pos(5, 14) }, position: pos(5, 15) }),
        ret(eint(1)),
    ];
    v.push(("expression_forms", b, vec![function("f", vec![], body)]));

    // forms the resolver does not walk: a qualified identifier inside stays as it is
    let mut b = Build::new();
    let mut q = |parts: &[&str]| b.q(parts, Some(vec![]), Some(T::ObjectPropertyAccess));
    let body = vec![
        decl("a", Expression::TypeCast { expression: bx(q(&["o", "x"])), target_type: DataType::Float, position: pos(6, 1) }),
        decl("b", Expression::BuiltinFunction { target: bx(q(&["o", "x"])), method: "len".into(), arguments: Some(vec![q(&["o", "y"])]), position: pos(6, 2) }),
        decl("c", Expression::StaticFunction { class_name: "Math".into(), method: "abs".into(), arguments: vec![q(&["o", "x"])], position: pos(6, 3) }),
        decl("d", Expression::FunctionCall { name: "f".into(), arguments: vec![q(&["o", "x"])], position: pos(6, 4) }),
        decl("e", Expression::DixFunctionCall { function_name: "dix".into(), arguments: vec![q(&["o", "x"])], position: pos(6, 5) }),
        decl("f", Expression::ConfigAccess { key: "k".into(), position: pos(6, 6) }),
        decl("g", Expression::EnumAccess { namespace_name: None, enum_name: "E".into(), value: "V".into(), position: pos(6, 7) }),
        decl("h", Expression::ObjectAccess { path: vec!["a".into(), "b".into()], position: pos(6, 8) }),
        decl("i", Expression::Value { value: Value::NestedArray { values: vec![vexpr(q(&["o", "x"]))], level: 1, position: pos(6, 9) }, position: pos(6, 10) }),
        decl("j", Expression::Value { value: Value::Range { start: Box::new(vexpr(q(&["o", "x"]))), end: Box::new(vexpr(q(&["o", "y"]))), position: pos(6, 11) }, position: pos(6, 12) }),
        decl("k", Expression::Value { value: Value::QuickFuncCall { function_name: "g".into(), arguments: vec![q(&["o", "x"])], position: pos(6, 13) }, position: pos(6, 14) }),
        ret(eint(1)),
    ];
    v.push(("expression_forms_not_walked", b, vec![function("f", vec![], body)]));

    // values
    let mut b = Build::new();
    let mut q = |parts: &[&str]| b.q(parts, Some(vec![]), Some(T::ObjectPropertyAccess));
    let ident = |s: &str| Value::Identifier { value: s.to_string(), position: pos(7, 1) };
    let body = vec![
        decl("a", Expression::Value { value: Value::Array { values: vec![vexpr(q(&["o", "x"])), ident("Status.DONE"), ident("A.B.C"), ident("plain"), ident("Trail."), ident(".Lead"), ident("")], position: pos(7, 2) }, position: pos(7, 3) }),
        decl("b", Expression::Value { value: Value::Object { properties: vec![ObjectProperty::new("k".into(), vexpr(q(&["o", "x"])), pos(7, 4)), ObjectProperty::new("e".into(), ident("E.V"), pos(7, 5))], position: pos(7, 6) }, position: pos(7, 7) }),
        decl("c", Expression::Value { value: Value::PrefixedConstructor { prefix: "t".into(), arguments: vec![vexpr(q(&["o", "x"])), Value::Integer { value: 1, position: pos(7, 8) }], position: pos(7, 9) }, position: pos(7, 10) }),
        decl("d", Expression::Value { value: Value::InterpolatedString { template: "{0} {1}".into(), expressions: vec![q(&["o", "x"]), eint(2)], position: pos(7, 11) }, position: pos(7, 12) }),
        decl("e", Expression::Value { value: Value::Lambda { parameters: vec!["n".into()], body: bx(q(&["n", "abs"])), statements: vec![decl("z", q(&["n", "sq"])), ret(q(&["n", "r"]))], position: pos(7, 13) }, position: pos(7, 14) }),
        decl("f", Expression::Value { value: Value::Lambda { parameters: vec![], body: bx(eint(1)), statements: vec![], position: pos(7, 15) }, position: pos(7, 16) }),
        ret(eint(1)),
    ];
    v.push(("values", b, vec![function("f", vec![], body)]));

    // every statement form, nested
    let mut b = Build::new();
    let mut q = |parts: &[&str]| b.q(parts, Some(vec![]), Some(T::ObjectPropertyAccess));
    let body = vec![
        QuickFuncStatement::Assignment { variable: "a".into(), value: q(&["o", "a"]), position: pos(9, 1) },
        QuickFuncStatement::ArithmeticAssignment { variable: "a".into(), operator: "+=".into(), value: q(&["o", "b"]), position: pos(9, 2) },
        QuickFuncStatement::ExpressionStatement { expression: q(&["o", "c"]), position: pos(9, 3) },
        log(q(&["o", "d"])),
        QuickFuncStatement::ObjectCreation { variable: "obj".into(), object: Value::Object { properties: vec![ObjectProperty::new("k".into(), vexpr(q(&["o", "e"])), pos(9, 4))], position: pos(9, 5) }, position: pos(9, 6) },
        QuickFuncStatement::If {
            condition: q(&["o", "f"]),
            then_branch: vec![log(q(&["o", "g"])), QuickFuncStatement::If { condition: q(&["o", "h"]), then_branch: vec![log(q(&["o", "i"]))], else_branch: None, position: pos(9, 7) }],
            else_branch: Some(vec![log(q(&["o", "j"]))]),
            position: pos(9, 8),
        },
        QuickFuncStatement::Switch {
            expression: q(&["o", "k"]),
            cases: vec![
                SwitchCase::new(Value::Integer { value: 1, position: pos(9, 9) }, vec![log(q(&["o", "l"]))], pos(9, 10)),
                SwitchCase::new(vexpr(q(&["o", "m"])), vec![ret(q(&["o", "n"]))], pos(9, 11)),
                SwitchCase::new(Value::Identifier { value: "E.V".into(), position: pos(9, 12) }, vec![], pos(9, 13)),
            ],
            default_case: Some(SwitchCase::new(Value::Null { position: pos(9, 14) }, vec![log(q(&["o", "p"]))], pos(9, 15))),
            position: pos(9, 16),
        },
        QuickFuncStatement::Switch { expression: eint(1), cases: vec![], default_case: None, position: pos(9, 17) },
        ret(q(&["o", "r"])),
    ];
    v.push(("statements", b, vec![function("f", vec![], body)]));

    // parameters: every type with no default, a default that is kept, no type
    let b = Build::new();
    let types = vec![
        DataType::Int, DataType::Long, DataType::Float, DataType::Double, DataType::String, DataType::Bool,
        DataType::Array, DataType::Tuple, DataType::Hex, DataType::Blob, DataType::Regex, DataType::Object,
        DataType::Timestamp, DataType::Date, DataType::Enum, DataType::Any, DataType::Function, DataType::Range,
        DataType::TypedArray(ElemType::Int), DataType::TypedArray(ElemType::String),
        DataType::TypedTuple([Some(ElemType::Int), Some(ElemType::String), None, None, None, None]),
    ];
    let mut params: Vec<QuickFuncParam> = types.iter().enumerate().map(|(i, t)| param(&format!("p{i}"), Some(*t), None)).collect();
    params.push(param("kept", Some(DataType::Int), Some(eint(42))));
    params.push(param("untyped", None, None));
    params.push(param("untyped_default", None, Some(eint(7))));
    v.push(("params", b, vec![function("f", params, vec![ret(eint(1))]), function("g", vec![param("only", Some(DataType::Int), None)], vec![])]));

    v.push(("no_functions", Build::new(), vec![]));
    v.push(("empty_function", Build::new(), vec![function("f", vec![], vec![])]));
    v
}



/// (case, mode, parameter defaults applied, fingerprint of the enhanced section), recorded from
/// the owned-tree enhancer before the change.
const HAND_RECORDING: &[(&str, &str, usize, u64)] = &[
    ("res_all_types", "resolutions", 0, 0xc537207ba9d10e18),
    ("res_all_types", "empty_map", 0, 0x0256259d4177fd23),
    ("res_all_types", "no_analysis", 0, 0x0256259d4177fd23),
    ("fallback_only", "resolutions", 0, 0xb1289cedb1b4696d),
    ("fallback_only", "empty_map", 0, 0x6d2858a89984b4e8),
    ("fallback_only", "no_analysis", 0, 0x6d2858a89984b4e8),
    ("key_mismatch", "resolutions", 0, 0xed7e05d1bd1bcdad),
    ("key_mismatch", "empty_map", 0, 0x72da01c097e57b30),
    ("key_mismatch", "no_analysis", 0, 0x72da01c097e57b30),
    ("expression_forms", "resolutions", 0, 0xd448de281b41091a),
    ("expression_forms", "empty_map", 0, 0xf0fad339a46b2b51),
    ("expression_forms", "no_analysis", 0, 0xf0fad339a46b2b51),
    ("expression_forms_not_walked", "resolutions", 0, 0xd1cba880777dc33e),
    ("expression_forms_not_walked", "empty_map", 0, 0xd1cba880777dc33e),
    ("expression_forms_not_walked", "no_analysis", 0, 0xd1cba880777dc33e),
    ("values", "resolutions", 0, 0xc689da4e2ef41355),
    ("values", "empty_map", 0, 0x986bdeea007afcee),
    ("values", "no_analysis", 0, 0x986bdeea007afcee),
    ("statements", "resolutions", 0, 0xa609801463a35bbb),
    ("statements", "empty_map", 0, 0x34ddf06968331973),
    ("statements", "no_analysis", 0, 0x34ddf06968331973),
    ("params", "resolutions", 22, 0x3f96a9a980fb6802),
    ("params", "empty_map", 22, 0x3f96a9a980fb6802),
    ("params", "no_analysis", 22, 0x3f96a9a980fb6802),
    ("no_functions", "resolutions", 0, 0x6a859b83de1920d7),
    ("no_functions", "empty_map", 0, 0x6a859b83de1920d7),
    ("no_functions", "no_analysis", 0, 0x6a859b83de1920d7),
    ("empty_function", "resolutions", 0, 0x09ed426eaefcb604),
    ("empty_function", "empty_map", 0, 0x09ed426eaefcb604),
    ("empty_function", "no_analysis", 0, 0x09ed426eaefcb604),
];

/// Runs the enhancer over a hand-built case in one of the three modes: with the recorded
/// resolutions, with an analysis result that has none, and with no analysis result at all.
fn enhance_hand(
    functions: &[QuickFunction],
    build: &Build,
    mode: &str,
    through_arena: bool,
) -> (usize, QuickFuncsSection) {
    let section = QuickFuncsSection::new(functions.to_vec(), pos(1, 1));
    let analysis = match mode {
        "resolutions" => {
            let mut a = SectionAnalysisResult::new("QUICKFUNCS");
            a.qualified_id_resolutions = build.map();
            Some(a)
        }
        "empty_map" => Some(SectionAnalysisResult::new("QUICKFUNCS")),
        "no_analysis" => None,
        other => panic!("unknown mode {other}"),
    };
    let settings = OperationalSettings::default();
    let em = ErrorManager::new_isolated_silent();
    let mut enhancer = QuickFunctionsAstEnhancer::new_with_error_manager(&settings, em);
    let enhanced = if through_arena {
        let mut arena = Arena::new();
        let doc = DocQuickFuncs::from_owned(&mut arena, &section);
        let enhanced = enhancer.enhance_doc(&doc, &mut arena, analysis.as_ref());
        enhanced.export(&arena)
    } else {
        enhancer.enhance(&section, analysis.as_ref())
    };
    (enhancer.get_enhancement_count(), enhanced)
}

#[test]
fn hand_built_sections_give_the_recorded_result_through_both_entry_points() {
    let cases = hand_cases();
    assert_eq!(cases.len() * 3, HAND_RECORDING.len());
    for (name, build, functions) in &cases {
        for mode in ["resolutions", "empty_map", "no_analysis"] {
            let (_, _, count, hash) = *HAND_RECORDING
                .iter()
                .find(|(n, m, _, _)| n == name && *m == mode)
                .unwrap_or_else(|| panic!("no recording for {name} [{mode}]"));
            for through_arena in [false, true] {
                let (applied, section) = enhance_hand(functions, build, mode, through_arena);
                assert_eq!(applied, count, "{name} [{mode}] arena={through_arena}: defaults applied");
                assert_eq!(
                    fingerprint(&Some(section.clone())),
                    hash,
                    "{name} [{mode}] arena={through_arena}\n{section}"
                );
            }
        }
    }
}

// ==================== which tree is read ====================

#[test]
fn the_enhancer_reads_the_document_when_it_has_one() {
    // The owned tree holds one file's section and the document another's; the semantic result is
    // the document's file's. With the document attached the result is the document's section
    // enhanced; without it the owned tree's section is enhanced instead, with resolutions that do
    // not belong to it.
    let mut document_file = parse(source("instance_calls"));
    let owned_file = parse(source("static_calls"));
    let em = ErrorManager::new_isolated_silent();
    let semantic = GeneralSemanticAnalyzer::new_with_error_manager(&document_file.ast, &document_file.settings, em.clone())
        .with_document(&document_file.document)
        .analyze();

    let with_document = GeneralAstEnhancer::new_with_error_manager(&document_file.settings, em.clone())
        .with_document(&mut document_file.document)
        .enhance(&owned_file.ast, Some(&semantic));
    let (_, wanted) = recorded("instance_calls");
    assert_eq!(fingerprint(&with_document.enhanced_ast.quick_functions), wanted);

    let without_document = GeneralAstEnhancer::new_with_error_manager(&document_file.settings, em)
        .enhance(&owned_file.ast, Some(&semantic));
    assert_ne!(fingerprint(&without_document.enhanced_ast.quick_functions), wanted);
}

#[test]
fn a_section_is_there_when_the_tree_that_is_read_says_so() {
    let em = ErrorManager::new_isolated_silent();

    // A document without a section, an owned tree with one: nothing is enhanced and the owned
    // section comes through as it was.
    let mut empty = parse(source("data_section_only"));
    let owned_file = parse(source("typed_params_defaults"));
    let result = GeneralAstEnhancer::new_with_error_manager(&empty.settings, em.clone())
        .with_document(&mut empty.document)
        .enhance(&owned_file.ast, None);
    assert_eq!(result.total_enhancements, 0);
    assert_eq!(
        format!("{:?}", result.enhanced_ast.quick_functions),
        format!("{:?}", owned_file.ast.quick_functions)
    );

    // A document with a section, an owned tree without one: the document's section is enhanced
    // and is the one in the result.
    let mut with_section = parse(source("typed_params_defaults"));
    let without = parse(source("data_section_only"));
    let result = GeneralAstEnhancer::new_with_error_manager(&with_section.settings, em)
        .with_document(&mut with_section.document)
        .enhance(&without.ast, None);
    let (total, hash) = {
        // With no analysis result only the defaults are applied, as in the `no_analysis` runs.
        let again = enhance_source(source("typed_params_defaults"), false);
        (again.total, fingerprint(&again.section))
    };
    assert_eq!(result.total_enhancements, total);
    assert_eq!(fingerprint(&result.enhanced_ast.quick_functions), hash);
}

// ==================== sharing, and the input left alone ====================

fn section_of(source_name: &str) -> (Arena, DocQuickFuncs) {
    let parsed = parse(source(source_name));
    let mut arena = Arena::new();
    let section = DocQuickFuncs::from_owned(&mut arena, parsed.ast.quick_functions.as_ref().expect("a section"));
    (arena, section)
}

#[test]
fn a_section_with_nothing_to_enhance_is_returned_without_allocating() {
    // No qualified identifiers and no parameter that needs a default.
    let text = "@QUICKFUNCS(\n  ~f<int> => global() {\n    let a = 1 + 2\n    if: a > 1 { log: a } else { log: 0 }\n    return a\n  }\n)\n@DATA(\n  x = 1\n)\n";
    let parsed = parse(text);
    let mut arena = Arena::new();
    let section = DocQuickFuncs::from_owned(&mut arena, parsed.ast.quick_functions.as_ref().expect("a section"));
    let (exprs, stmts, values) = (arena.expr_count(), arena.stmt_count(), arena.value_count());

    let settings = OperationalSettings::default();
    let mut analysis = SectionAnalysisResult::new("QUICKFUNCS");
    analysis.qualified_id_resolutions = HashMap::from([(
        QualifiedIdentifierKey { position: pos(99, 99), parts: vec!["a".into(), "b".into()], is_call: false },
        QualifiedIdentifierResolution::new(QualifiedIdentifierType::Unknown, None, vec!["a".into(), "b".into()], false, pos(99, 99)),
    )]);
    let mut enhancer = QuickFunctionsAstEnhancer::new_with_error_manager(&settings, ErrorManager::new_isolated_silent());
    let enhanced = enhancer.enhance_doc(&section, &mut arena, Some(&analysis));

    assert_eq!((arena.expr_count(), arena.stmt_count(), arena.value_count()), (exprs, stmts, values));
    assert_eq!(enhanced.functions[0].body, section.functions[0].body);
    assert_eq!(enhancer.get_enhancement_count(), 0);
}

#[test]
fn only_the_path_to_a_resolved_identifier_is_allocated() {
    let text = "@QUICKFUNCS(\n  ~f<int> => global() {\n    let a = 1\n    let s = \"abc\"\n    let t = s.upper()\n    log: a\n    return a\n  }\n)\n@DATA(\n  x = 1\n)\n";
    let mut parsed = parse(text);
    let em = ErrorManager::new_isolated_silent();
    let semantic = GeneralSemanticAnalyzer::new_with_error_manager(&parsed.ast, &parsed.settings, em.clone())
        .with_document(&parsed.document)
        .analyze();
    let analysis = semantic.section_results.get("QUICKFUNCS").expect("an analysis");
    assert_eq!(analysis.qualified_id_resolutions.len(), 1);

    let Document { arena, quick_functions, .. } = &mut parsed.document;
    let section = quick_functions.as_ref().expect("a section");
    let old_body: Vec<_> = arena.stmt_list(section.functions[0].body).to_vec();
    let mut enhancer = QuickFunctionsAstEnhancer::new_with_error_manager(&parsed.settings, em);
    let enhanced = enhancer.enhance_doc(section, arena, Some(analysis));
    let new_body: Vec<_> = arena.stmt_list(enhanced.functions[0].body).to_vec();

    assert_ne!(enhanced.functions[0].body, section.functions[0].body);
    assert_eq!(new_body.len(), old_body.len());
    for (i, (new, old)) in new_body.iter().zip(&old_body).enumerate() {
        // Only the declaration of `t` holds the qualified identifier.
        assert_eq!(new == old, i != 2, "statement {i}");
    }
}

#[test]
fn the_input_section_means_what_it_meant_before_it_was_enhanced() {
    for name in ["typed_params_defaults", "instance_calls", "values_in_collections", "control_flow"] {
        let mut parsed = parse(source(name));
        let em = ErrorManager::new_isolated_silent();
        let semantic = GeneralSemanticAnalyzer::new_with_error_manager(&parsed.ast, &parsed.settings, em.clone())
            .with_document(&parsed.document)
            .analyze();
        let Document { arena, quick_functions, .. } = &mut parsed.document;
        let section = quick_functions.as_ref().expect("a section");
        let before = format!("{:?}", section.export(arena));
        let mut enhancer = QuickFunctionsAstEnhancer::new_with_error_manager(&parsed.settings, em);
        let _ = enhancer.enhance_doc(section, arena, semantic.section_results.get("QUICKFUNCS"));
        assert_eq!(before, format!("{:?}", section.export(arena)), "{name}");
    }
}

#[test]
fn a_parameter_default_is_a_value_in_the_arena_of_the_right_kind() {
    let (mut arena, section) = section_of("typed_params_defaults");
    let settings = OperationalSettings::default();
    let mut enhancer = QuickFunctionsAstEnhancer::new_with_error_manager(&settings, ErrorManager::new_isolated_silent());
    let enhanced = enhancer.enhance_doc(&section, &mut arena, None);
    for function in &enhanced.functions {
        for parameter in &function.parameters {
            assert!(parameter.default_value.is_some(), "{} has a default", parameter.name);
        }
    }
    assert_eq!(enhancer.get_enhancement_count(), 10);
    // The input still has none.
    assert!(section.functions.iter().all(|f| f.parameters.iter().all(|p| p.default_value.is_none())));
}

#[test]
fn date_and_timestamp_defaults_have_the_shape_of_a_date_and_a_timestamp() {
    let section = QuickFuncsSection::new(
        vec![function("f", vec![param("d", Some(DataType::Date), None), param("t", Some(DataType::Timestamp), None)], vec![])],
        pos(1, 1),
    );
    let settings = OperationalSettings::default();
    let mut enhancer = QuickFunctionsAstEnhancer::new_with_error_manager(&settings, ErrorManager::new_isolated_silent());
    let enhanced = enhancer.enhance(&section, None);
    let defaults: Vec<&Expression> =
        enhanced.functions[0].parameters.iter().map(|p| p.default_value.as_ref().expect("a default")).collect();
    match defaults[0] {
        Expression::Value { value: Value::Date { value, .. }, .. } => {
            assert_eq!(time_length(value.as_bytes()), Some(10), "{value}");
            assert_eq!(value.len(), 10);
        }
        other => panic!("not a date: {other:?}"),
    }
    match defaults[1] {
        Expression::Value { value: Value::Timestamp { value, .. }, .. } => {
            assert_eq!(time_length(value.as_bytes()), Some(20), "{value}");
            assert_eq!(value.len(), 20);
        }
        other => panic!("not a timestamp: {other:?}"),
    }
}

#[test]
fn a_richer_section_with_nothing_to_enhance_allocates_nothing() {
    // A switch, an object, an array, a constructor, an interpolated string and a lambda, none of
    // which holds a qualified identifier: not one byte of the arena is used.
    let text = "@QUICKFUNCS(\n  ~f<int> => global() {\n    let a = 1\n    chk: a { -> 1 { log: [1, 2] } -> miss { log: {k = 1, m = [a]} } }\n    let t = t:(1, 2)\n    let s = $\"v {a}\"\n    let g = (n) => n + 1\n    log: t\n    log: s\n    log: g(a)\n    return a\n  }\n)\n@DATA(\n  x = 1\n)\n";
    let parsed = parse(text);
    let mut arena = Arena::new();
    let section = DocQuickFuncs::from_owned(&mut arena, parsed.ast.quick_functions.as_ref().expect("a section"));
    let before = arena.used_bytes();

    let settings = OperationalSettings::default();
    let mut enhancer = QuickFunctionsAstEnhancer::new_with_error_manager(&settings, ErrorManager::new_isolated_silent());
    let mut analysis = SectionAnalysisResult::new("QUICKFUNCS");
    analysis.qualified_id_resolutions = HashMap::from([(
        QualifiedIdentifierKey { position: pos(99, 99), parts: vec!["a".into(), "b".into()], is_call: false },
        QualifiedIdentifierResolution::new(QualifiedIdentifierType::Unknown, None, vec!["a".into(), "b".into()], false, pos(99, 99)),
    )]);
    let enhanced = enhancer.enhance_doc(&section, &mut arena, Some(&analysis));
    assert_eq!(arena.used_bytes(), before);
    assert_eq!(enhanced.functions[0].body, section.functions[0].body);
}

#[test]
fn arguments_are_resolved_whichever_way_their_call_was_built() {
    // Each outer call holds an inner qualified identifier `s.trim()` among its arguments.
    let mut build = Build::new();
    let mut inner = |b: &mut Build| b.q(&["s", "trim"], Some(vec![]), None);
    let body = vec![
        // the fallback for a call with three parts: a call of a function with a dotted name
        decl("a", { let i = inner(&mut build); build.q(&["a", "b", "c"], Some(vec![i]), None) }),
        // a recorded object method call
        decl("b", { let i = inner(&mut build); build.q(&["o", "m"], Some(vec![i]), Some(T::ObjectPropertyAccess)) }),
        // a recorded imported function call
        decl("c", { let i = inner(&mut build); build.q(&["lib", "f"], Some(vec![i]), Some(T::ImportedFunctionCall)) }),
        // a recorded static call and the fallback for one
        decl("d", { let i = inner(&mut build); build.q(&["Math", "max"], Some(vec![i]), Some(T::StaticObjectAccess)) }),
        decl("e", { let i = inner(&mut build); build.q(&["Math", "max"], Some(vec![i]), None) }),
        ret(eint(1)),
    ];
    let functions = vec![function("f", vec![], body)];
    let (_, section) = enhance_hand(&functions, &build, "resolutions", false);

    let value_of = |i: usize| match &section.functions[0].body[i] {
        QuickFuncStatement::VariableDeclaration { value, .. } => value.clone(),
        other => panic!("not a declaration: {other:?}"),
    };
    let first_argument = |call: &Expression| -> Expression {
        match call {
            Expression::QuickFuncCall { arguments, .. }
            | Expression::InstanceMethodCall { arguments, .. }
            | Expression::ImportedFunctionCall { arguments, .. }
            | Expression::StaticMethodCall { arguments, .. } => arguments[0].clone(),
            other => panic!("not a call: {other:?}"),
        }
    };
    for (i, outer) in ["QuickFuncCall", "InstanceMethodCall", "ImportedFunctionCall", "StaticMethodCall", "StaticMethodCall"].iter().enumerate() {
        let call = value_of(i);
        assert!(format!("{call:?}").starts_with(outer), "statement {i}: {call:?}");
        match first_argument(&call) {
            Expression::InstanceMethodCall { method_name, instance, .. } => {
                assert_eq!(method_name, "trim", "statement {i}");
                assert!(matches!(*instance, Expression::Identifier { ref name, .. } if name == "s"), "statement {i}");
            }
            other => panic!("statement {i}: the argument was left as {other:?}"),
        }
    }
}

#[test]
fn the_enhancement_info_names_the_section_and_counts_the_defaults() {
    let em = ErrorManager::new_isolated_silent();
    let parsed = parse(source("typed_params_defaults"));
    let result = GeneralAstEnhancer::new_with_error_manager(&parsed.settings, em.clone()).enhance(&parsed.ast, None);
    let info = result.section_enhancements.get("QUICKFUNCS").expect("an entry for the section");
    assert_eq!(info.section_name, "QUICKFUNCS");
    assert_eq!(info.enhancements_applied, 10);
    assert_eq!(info.enhancement_types, vec!["parameter_defaults", "qualified_identifier_resolution"]);
    assert_eq!(result.total_enhancements, 10);

    // A file without the section has no entry.
    let parsed = parse(source("data_section_only"));
    let result = GeneralAstEnhancer::new_with_error_manager(&parsed.settings, em).enhance(&parsed.ast, None);
    assert!(result.section_enhancements.get("QUICKFUNCS").is_none());
    assert_eq!(result.total_enhancements, 0);
}

// ==================== the owned entry points of the resolver ====================

#[test]
fn the_owned_resolver_entry_points_give_what_the_enhancer_puts_in_a_body() {
    let cases = hand_cases();
    let (_, build, functions) = cases.iter().find(|(n, _, _)| *n == "statements").expect("the statements case");
    let body = &functions[0].body;
    let resolver = QualifiedIdentifierResolver::new(build.map(), DebugConfig::from_debug_mode(OperationalSettings::default().debug_mode));

    let (_, enhanced) = enhance_hand(functions, build, "resolutions", false);
    let wanted = &enhanced.functions[0].body;

    let all = resolver.resolve_statements(body);
    assert_eq!(format!("{all:?}"), format!("{wanted:?}"));
    for (statement, wanted) in body.iter().zip(wanted) {
        assert_eq!(format!("{:?}", resolver.resolve_statement(statement)), format!("{wanted:?}"));
    }
    // An expression of a statement resolves to the expression of the resolved statement.
    if let (QuickFuncStatement::Log { value, .. }, QuickFuncStatement::Log { value: resolved, .. }) = (&body[3], &wanted[3]) {
        assert_eq!(format!("{:?}", resolver.resolve_expression(value)), format!("{resolved:?}"));
    } else {
        panic!("statement 3 is a log");
    }
}
