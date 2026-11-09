// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/value_resolver_context_document.rs"
// ============================================================================
//! Tests for the data context and the call finder (phases 2 and 3 of value resolution) over a
//! `Document`.
//!
//! Phase 2 builds the map from the path of every literal in `@DATA` to its runtime value, and
//! phase 3 finds every quickfunction call. Both now read the arena: `ValueResolver::with_document`
//! points them at a `Document`, and without one the resolver copies the owned data section into a
//! scratch arena once and the phases share it. The tests check five things:
//!
//! * the whole result of the resolver, calls and their history included, is the one the owned
//!   implementation produced for crafted sources, four corpus files and data sections built by
//!   hand (`RECORDING`, `HAND_RECORDING`, fingerprints of results produced by running the original
//!   code), through the document and through the owned entry point;
//! * the data context and the list of found calls are the original ones for each hand-built
//!   section (`CONTEXT_RECORDING`, `WALK_RECORDING`), and a few rules are asserted outright: what
//!   converts and what does not, how aliases resolve and where they stop, which expressions the
//!   call finder enters and which it does not;
//! * the phases read the `Document` when they have one, and the owned `find_all` is the same as
//!   the arena `find_all_in`;
//! * phases 2 and 3 allocate nothing in the document's arena.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use dixscript::Compiler::AST::arena::Document;
use dixscript::Compiler::AST::data_types::{CompatibilityMode, ErrorHandlingStrategy};
use dixscript::Compiler::AST::{
    DataEntry, DataSection, DataType, DebugMode, DixScript, Expression as E, ObjectProperty,
    Position, PropertyAssignment, TablePath, Value as V,
};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::ValueResolution::{ASTWalker, FunctionCallInfo, ValueResolver};
use dixscript::Compiler::Core::{GeneralAstEnhancer, GeneralParser, GeneralSemanticAnalyzer};
use dixscript::Compiler::Utilities::SymbolTable;
use dixscript::ErrorManager::ErrorManager;

// ==================== helpers ====================

const BASE_TYPES: &str = include_str!("../../mdix_files/tests/imports/base_types.mdix");
const ENUM_ONLY_TYPES: &str = include_str!("../../mdix_files/tests/imports/enum_only_types.mdix");

/// A directory holding the two import libraries the sources import, written once per run.
fn fixture_dir() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir()
            .join(format!("dix_value_resolver_enums_document_{}", std::process::id()));
        fs::create_dir_all(&dir).expect("the fixture directory is created");
        fs::write(dir.join("base_types.mdix"), BASE_TYPES).expect("a library is written");
        fs::write(dir.join("enum_only_types.mdix"), ENUM_ONLY_TYPES).expect("a library is written");
        dir
    })
}

/// 64 bit FNV-1a, the fingerprint of a result's debug text.
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

/// Replaces every quoted date and timestamp by `"<D>"`; the default of a date or timestamp
/// parameter is the time of the run.
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

/// Everything the loader has when it reaches value resolution.
struct Prepared {
    document: Document,
    enhanced: DixScript,
    symbols: SymbolTable,
    settings: OperationalSettings,
}

/// Runs the loader's stages up to value resolution on `source`, the contents of the file at
/// `path`. The stages go on after a semantic error, so a source with a bad enum reference still
/// reaches the resolver.
fn prepare(path: &str, source: &[u8]) -> Prepared {
    let em = ErrorManager::new_isolated_silent();
    let initial = OperationalSettings {
        source_file_path: Some(path.to_string()),
        compatibility_mode: CompatibilityMode::Strict,
        ..OperationalSettings::default()
    };
    let tokens = Tokenizer::new_with_error_manager_from_bytes(source, &initial, em.clone()).tokenize();
    let split = split_config_tokens(tokens.tokens);
    let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
    let config = handler.process_config_tokens(&split.config_tokens);
    let mut settings = config.operational_settings;
    settings.source_file_path = Some(path.to_string());
    settings.error_handling_strategy = ErrorHandlingStrategy::Continue;
    settings.debug_mode = DebugMode::Off;
    em.update_settings(settings.clone());
    let mut document =
        GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &settings, em.clone())
            .expect("the parser starts")
            .parse_document()
            .expect("the source parses");
    let ast = document.export();
    let semantic = GeneralSemanticAnalyzer::new_with_error_manager(&ast, &settings, em.clone())
        .with_document(&document)
        .analyze();
    let enhanced = GeneralAstEnhancer::new_with_error_manager(&settings, em)
        .with_document(&mut document)
        .enhance(&ast, Some(&semantic))
        .enhanced_ast;
    let symbols = semantic.symbol_table.expect("analysis leaves a symbol table");
    Prepared { document, enhanced, symbols, settings }
}

/// The one place the tests attach a document to the resolver.
fn with_doc<'a>(resolver: ValueResolver<'a>, document: &'a mut Document) -> ValueResolver<'a> {
    resolver.with_document(document)
}


thread_local! {
    static POS: Cell<usize> = const { Cell::new(100) };
}

/// A position no other node shares, so a rewrite that mixes up positions changes the recording.
fn pos() -> Position {
    POS.with(|p| {
        let line = p.get();
        p.set(line + 1);
        Position { line, column: 3 }
    })
}
fn int(v: i32) -> V {
    V::Integer { value: v, position: pos() }
}
fn ev(e: &str, f: &str) -> V {
    V::EnumValue { enum_name: e.into(), value: f.into(), position: pos() }
}
fn ea(ns: Option<&str>, e: &str, f: &str) -> E {
    E::EnumAccess { namespace_name: ns.map(String::from), enum_name: e.into(), value: f.into(), position: pos() }
}
fn bx(e: E) -> Box<E> {
    Box::new(e)
}
fn ie(v: i32) -> E {
    E::Value { value: int(v), position: pos() }
}
fn vexp(e: E) -> V {
    V::Expression { expr: bx(e), position: pos() }
}
fn simple(name: &str, v: V) -> DataEntry {
    DataEntry::SimpleProperty { name: name.into(), data_type: None, value: v, position: pos() }
}
fn arr(vs: Vec<V>) -> V {
    V::Array { values: vs, position: pos() }
}
fn nest(vs: Vec<V>, level: usize) -> V {
    V::NestedArray { values: vs, level, position: pos() }
}
fn obj(ps: Vec<(&str, V)>) -> V {
    V::Object {
        properties: ps
            .into_iter()
            .map(|(k, v)| ObjectProperty { key: k.into(), value: v, position: pos() })
            .collect(),
        position: pos(),
    }
}
fn bin(kind: u8, l: E, op: &str, r: E) -> E {
    let (left, right, operator, position) = (bx(l), bx(r), op.to_string(), pos());
    match kind {
        0 => E::ArithmeticOp { left, operator, right, position },
        1 => E::BitwiseOp { left, operator, right, position },
        2 => E::ComparisonOp { left, operator, right, position },
        _ => E::LogicalOp { left, operator, right, position },
    }
}
fn tbl(path: &[&str], props: Vec<(&str, V)>) -> DataEntry {
    DataEntry::TableProperty {
        path: TablePath::new(path.iter().map(|s| s.to_string()).collect()),
        properties: props.into_iter().map(|(n, v)| PropertyAssignment::new(n.into(), None, v, pos())).collect(),
        position: pos(),
    }
}
fn grp(path: &[&str], items: Vec<V>) -> DataEntry {
    DataEntry::GroupArray { path: TablePath::new(path.iter().map(|s| s.to_string()).collect()), items, position: pos() }
}
fn objprop(name: &str, o: V) -> DataEntry {
    DataEntry::ObjectProperty { name: name.into(), data_type: Some(DataType::Object), object: Box::new(o), position: pos() }
}
fn ok_entry() -> DataEntry {
    simple("ok", ev("Color", "RED"))
}


fn qfv(name: &str, args: Vec<E>) -> V {
    V::QuickFuncCall { function_name: name.into(), arguments: args, position: pos() }
}
fn eqf(name: &str, args: Vec<E>) -> E {
    E::QuickFuncCall { name: name.into(), arguments: args, position: pos() }
}
fn ident(name: &str) -> V {
    V::Identifier { value: name.into(), position: pos() }
}
fn sv(s: &str) -> V {
    V::String { value: s.into(), position: pos() }
}
fn ctor(prefix: &str, args: Vec<V>) -> V {
    V::PrefixedConstructor { prefix: prefix.into(), arguments: args, position: pos() }
}

fn hand_cases() -> Vec<(&'static str, Vec<DataEntry>)> {
    POS.with(|p| p.set(100));
    let b = |n: &str| E::ImportedFunctionCall { namespace_name: "Base".into(), function_name: n.into(), arguments: vec![ie(1)], position: pos() };
    let mut v: Vec<(&'static str, Vec<DataEntry>)> = Vec::new();
    v.push(("empty", vec![]));
    v.push(("no_calls", vec![simple("a", int(1)), simple("b", sv("x"))]));
    v.push(("ctx_scalars", vec![
        simple("i", int(1)),
        simple("l", V::Long { value: 5_000_000_000, position: pos() }),
        simple("f", V::Float { value: 1.5, position: pos() }),
        simple("d", V::Double { value: 2.25, position: pos() }),
        simple("sci", V::ScientificNotation { value: 1.0e10, position: pos() }),
        simple("s", sv("text")),
        simple("t", V::Boolean { value: true, position: pos() }),
        simple("n", V::Null { position: pos() }),
        simple("h", V::HexColor { value: "#FF00AA".into(), position: pos() }),
        simple("date_ok", V::Date { value: "2024-03-05".into(), position: pos() }),
        simple("date_full", V::Date { value: "2024-03-05T10:20:30Z".into(), position: pos() }),
        simple("date_bad", V::Date { value: "not-a-date".into(), position: pos() }),
        simple("ts_ok", V::Timestamp { value: "2024-03-05T10:20:30Z".into(), position: pos() }),
        simple("ts_bad", V::Timestamp { value: "bad".into(), position: pos() }),
        simple("interp", V::InterpolatedString { template: "a{0}".into(), expressions: vec![ie(1)], position: pos() }),
        simple("en", ev("Color", "RED")),
        simple("unres", ident("nothing")),
        simple("err", V::Error { message: "e".into(), position: pos() }),
        simple("perr", V::ParseError { message: "p".into(), position: pos() }),
        simple("call", qfv("f", vec![ie(1)])),
        simple("expr", vexp(ie(1))),
    ]));
    v.push(("ctx_ctor", vec![
        simple("b_ok", ctor("b", vec![sv("aGVsbG8=")])),
        simple("b_none", ctor("b", vec![])),
        simple("b_nonstr", ctor("b", vec![int(1)])),
        simple("r_ok", ctor("r", vec![sv("[a-z]+")])),
        simple("r_none", ctor("r", vec![])),
        simple("r_bad", ctor("r", vec![sv("(")])),
        simple("t3", ctor("t", vec![int(1), sv("x"), V::Boolean { value: false, position: pos() }])),
        simple("t7", ctor("t", vec![int(1), int(2), int(3), int(4), int(5), int(6), int(7)])),
        simple("t_call", ctor("t", vec![int(1), qfv("f", vec![ie(1)])])),
        simple("t_empty", ctor("t", vec![])),
        simple("upper", ctor("T", vec![int(1), int(2)])),
        simple("other", ctor("x", vec![int(1)])),
    ]));
    v.push(("ctx_containers", vec![
        simple("a_empty", arr(vec![])),
        simple("a_ints", arr(vec![int(1), int(2), int(3)])),
        simple("a_mixed", arr(vec![int(1), sv("x"), V::Null { position: pos() }])),
        simple("a_nested", nest(vec![nest(vec![int(1), int(2)], 2), nest(vec![int(3)], 2)], 1)),
        simple("a_partial", arr(vec![int(1), qfv("f", vec![ie(1)]), int(3)])),
        simple("a_all_calls", arr(vec![qfv("f", vec![ie(1)]), qfv("g", vec![ie(2)])])),
        simple("a_in_a", arr(vec![arr(vec![int(1)]), arr(vec![])])),
        simple("o_empty", obj(vec![])),
        simple("o_flat", obj(vec![("x", int(1)), ("y", sv("s"))])),
        simple("o_nested", obj(vec![("x", int(1)), ("n", obj(vec![("p", int(2)), ("q", arr(vec![int(3), int(4)]))]))])),
        simple("o_call", obj(vec![("x", int(1)), ("c", qfv("f", vec![ie(1)])), ("z", int(3))])),
        objprop("op_flat", obj(vec![("k", int(1)), ("m", obj(vec![("deep", int(9))]))])),
        objprop("op_empty", obj(vec![])),
        objprop("op_nonobject", arr(vec![int(1), int(2)])),
    ]));
    v.push(("ctx_tables_groups", vec![
        simple("top", int(10)),
        simple("alias_top", ident("top")),
        tbl(&["srv"], vec![("host", sv("h")), ("port", int(80)), ("ref_top", ident("top")), ("ref_port", ident("port")), ("arr", arr(vec![int(1), int(2)])), ("miss", ident("nope"))]),
        tbl(&["a", "b", "c"], vec![("deep", int(7)), ("like_deep", ident("deep"))]),
        tbl(&["t2"], vec![]),
        grp(&["g1"], vec![int(1), sv("two"), ident("top"), arr(vec![int(3)]), obj(vec![("k", int(4))]), qfv("f", vec![ie(1)]), ident("nope")]),
        grp(&["g", "h"], vec![int(5), ident("deep")]),
        grp(&["g3"], vec![]),
    ]));
    v.push(("alias_limit", {
        let mut e = Vec::new();
        for i in 0..10 {
            let name = format!("e{}", i);
            if i == 9 { e.push(simple(&name, int(42))); } else { e.push(simple(&name, ident(&format!("e{}", i + 1)))); }
        }
        e.push(simple("fwd", ident("later")));
        e.push(simple("later", int(1)));
        e.push(simple("cyc_a", ident("cyc_b")));
        e.push(simple("cyc_b", ident("cyc_a")));
        e.push(simple("self_ref", ident("self_ref")));
        e.push(simple("dup", int(1)));
        e.push(simple("dup", int(2)));
        e.push(simple("dup_alias", ident("dup")));
        e
    }));
    v.push(("walk_values", vec![
        simple("direct", qfv("f", vec![ie(1)])),
        simple("nested_args", qfv("f", vec![eqf("g", vec![ie(1)]), ie(2)])),
        simple("arr", arr(vec![qfv("f", vec![ie(1)]), int(2), qfv("g", vec![ie(3)])])),
        simple("nest", nest(vec![nest(vec![qfv("f", vec![ie(1)])], 2)], 1)),
        simple("obj", obj(vec![("a", qfv("f", vec![ie(1)])), ("b", obj(vec![("c", qfv("g", vec![ie(2)]))]))])),
        objprop("objprop", obj(vec![("a", qfv("f", vec![ie(1)]))])),
        simple("ctor", ctor("t", vec![qfv("f", vec![ie(1)]), int(2)])),
        simple("interp", V::InterpolatedString { template: "{0}{1}".into(), expressions: vec![eqf("f", vec![ie(1)]), ie(2)], position: pos() }),
        simple("range", V::Range { start: Box::new(qfv("f", vec![ie(1)])), end: Box::new(qfv("g", vec![ie(2)])), position: pos() }),
        simple("lambda", V::Lambda { parameters: vec!["x".into()], body: bx(eqf("f", vec![ie(1)])), statements: vec![], position: pos() }),
        simple("wrapped", vexp(eqf("f", vec![ie(1)]))),
        simple("plain", int(1)),
    ]));
    v.push(("walk_exprs_entered", vec![
        simple("arith", vexp(bin(0, eqf("f", vec![ie(1)]), "+", eqf("g", vec![ie(2)])))),
        simple("cmp", vexp(bin(2, eqf("f", vec![ie(1)]), "<", ie(2)))),
        simple("logic", vexp(bin(3, ie(1), "&&", eqf("f", vec![ie(2)])))),
        simple("unary", vexp(E::UnaryOp { operator: "-".into(), operand: bx(eqf("f", vec![ie(1)])), position: pos() })),
        simple("cond", vexp(E::Conditional { condition: bx(eqf("f", vec![ie(1)])), true_value: bx(eqf("g", vec![ie(2)])), false_value: bx(eqf("h", vec![ie(3)])), position: pos() })),
        simple("prop", vexp(E::PropertyAccess { object: bx(eqf("f", vec![ie(1)])), property: "p".into(), position: pos() })),
        simple("idx", vexp(E::IndexAccess { object: bx(eqf("f", vec![ie(1)])), index: bx(eqf("g", vec![ie(2)])), position: pos() })),
        simple("stat", vexp(E::StaticMethodCall { object_name: "Math".into(), method_name: "max".into(), arguments: vec![eqf("f", vec![ie(1)]), ie(2)], position: pos() })),
        simple("inst", vexp(E::InstanceMethodCall { instance: bx(eqf("f", vec![ie(1)])), method_name: "m".into(), arguments: vec![eqf("g", vec![ie(2)])], position: pos() })),
        simple("imp_nested", vexp(E::ImportedFunctionCall { namespace_name: "Base".into(), function_name: "scaledValue".into(), arguments: vec![eqf("f", vec![ie(1)]), b("scaledValue")], position: pos() })),
        simple("eqf_nested", vexp(eqf("f", vec![eqf("g", vec![ie(1)])]))),
    ]));
    v.push(("walk_exprs_not_entered", vec![
        simple("bit", vexp(bin(1, eqf("f", vec![ie(1)]), "|", ie(2)))),
        simple("paren", vexp(E::Parenthesized { expression: bx(eqf("f", vec![ie(1)])), position: pos() })),
        simple("cast", vexp(E::TypeCast { expression: bx(eqf("f", vec![ie(1)])), target_type: DataType::Int, position: pos() })),
        simple("builtin", vexp(E::BuiltinFunction { target: bx(eqf("f", vec![ie(1)])), method: "m".into(), arguments: Some(vec![eqf("g", vec![ie(2)])]), position: pos() })),
        simple("fncall", vexp(E::FunctionCall { name: "g".into(), arguments: vec![eqf("f", vec![ie(1)])], position: pos() })),
        simple("dix", vexp(E::DixFunctionCall { function_name: "d".into(), arguments: vec![eqf("f", vec![ie(1)])], position: pos() })),
        simple("staticfn", vexp(E::StaticFunction { class_name: "C".into(), method: "m".into(), arguments: vec![eqf("f", vec![ie(1)])], position: pos() })),
        simple("qual", vexp(E::QualifiedIdentifier { parts: vec!["a".into(), "b".into()], arguments: Some(vec![eqf("f", vec![ie(1)])]), position: pos() })),
        simple("objacc", vexp(E::ObjectAccess { path: vec!["a".into()], position: pos() })),
        simple("identexpr", vexp(E::Identifier { name: "x".into(), position: pos() })),
        simple("valexpr", vexp(E::Value { value: qfv("f", vec![ie(1)]), position: pos() })),
        simple("enumacc", vexp(ea(None, "Color", "RED"))),
        simple("cfg", vexp(E::ConfigAccess { key: "k".into(), position: pos() })),
    ]));
    v.push(("walk_names", vec![
        simple("plain", qfv("f", vec![ie(1)])),
        simple("imported", qfv("Base.scaledValue", vec![ie(1), ie(2)])),
        simple("unknown_ns", qfv("Zed.f", vec![ie(1)])),
        simple("three", qfv("Base.a.b", vec![ie(1)])),
        simple("trailing_dot", qfv("Base.", vec![ie(1)])),
        simple("expr_imported", vexp(eqf("Base.scaledValue", vec![ie(1)]))),
        simple("expr_unknown", vexp(eqf("Zed.f", vec![ie(1)]))),
        simple("imp_node", vexp(E::ImportedFunctionCall { namespace_name: "Zed".into(), function_name: "q".into(), arguments: vec![ie(1)], position: pos() })),
    ]));
    v.push(("walk_scopes", vec![
        simple("s0", qfv("f", vec![ie(1)])),
        tbl(&["a", "b"], vec![("p", qfv("f", vec![ie(1)])), ("q", arr(vec![qfv("g", vec![ie(2)]), int(3), qfv("h", vec![ie(4)])])), ("r", obj(vec![("k", qfv("i", vec![ie(5)]))]))]),
        simple("s1", qfv("f", vec![ie(1)])),
        grp(&["rows"], vec![obj(vec![("c", qfv("f", vec![ie(1)])), ("d", int(2))]), qfv("g", vec![ie(2)]), arr(vec![qfv("h", vec![ie(3)])]), int(4), qfv("i", vec![ie(5)])]),
        grp(&["x", "y"], vec![qfv("f", vec![ie(1)])]),
        objprop("op", obj(vec![("a", qfv("f", vec![ie(1)])), ("b", arr(vec![nest(vec![qfv("g", vec![ie(2)])], 2)]))])),
        tbl(&["t"], vec![("only", int(1))]),
        simple("s2", qfv("f", vec![ie(1)])),
    ]));
    v.push(("alias_forms", vec![
        simple("first_alias", ident("later_lit")),
        simple("top", int(10)),
        simple("xtop", int(99)),
        simple("ytop", int(98)),
        simple("ztop", int(97)),
        simple("alias_top", ident("top")),
        simple("exact_alias", ident("DATA.top")),
        tbl(&["tc"], vec![("p0", ident("p1")), ("p1", ident("p2")), ("p2", int(5))]),
        grp(&["gc"], vec![ident("gc_alias"), int(1)]),
        simple("gc_alias", ident("gc_lit")),
        simple("gc_lit", int(7)),
        objprop("op_alias", obj(vec![("k", int(1))])),
        simple("later_lit", int(3)),
    ]));
    // One alias, in a table only and in a group only: each is the only thing the pass changes.
    v.push(("alias_table_only", vec![tbl(&["t1"], vec![("a", ident("b")), ("b", int(1))])]));
    v.push(("alias_group_only", vec![grp(&["g1"], vec![ident("lit"), int(1)]), simple("lit", int(5))]));
    v
}


// ==================== run helpers ====================

/// What a run of the resolver left behind.
struct Outcome {
    ok: bool,
    errors: Vec<String>,
    resolved: Option<DixScript>,
    /// One line per call the resolver made, without its time.
    history: Vec<String>,
    /// The log text; empty unless the run asked for logging.
    log: String,
    /// The document the resolver was given, after the run (`None` for a run without one).
    document: Option<Document>,
}

/// How a run is made.
#[derive(Clone, Copy)]
struct Run {
    through_document: bool,
    verbose_log: bool,
}

const PLAIN: Run = Run { through_document: false, verbose_log: false };

fn registry_lock() -> std::sync::MutexGuard<'static, ()> {
    dixscript::Builtins::Static::enum_object::ENUM_REGISTRY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Resolves `enhanced`, with `document` attached when there is one. The resolver reads the enum
/// registry, so the caller holds its lock.
fn resolve(
    enhanced: DixScript,
    symbols: &SymbolTable,
    settings: &OperationalSettings,
    document: Option<Document>,
    verbose_log: bool,
) -> Outcome {
    let em = if verbose_log { ErrorManager::new_isolated() } else { ErrorManager::new_isolated_silent() };
    let debug = if verbose_log { DebugMode::Verbose } else { settings.debug_mode };
    if verbose_log {
        let mut verbose = settings.clone();
        verbose.debug_mode = DebugMode::Verbose;
        em.update_settings(verbose);
    }
    let mut document = document;
    let mut resolver = ValueResolver::new_with_error_manager(enhanced, symbols, debug, em.clone());
    if let Some(doc) = document.as_mut() {
        resolver = with_doc(resolver, doc);
    }
    let result = resolver.resolve();
    let history = result
        .resolution_history
        .iter()
        .map(|r| {
            format!(
                "{} {:?} {} {} {:?} {:?} {} {}",
                r.function_name, r.namespace_name, r.location, r.scope, r.arguments, r.result,
                r.success, r.error_message
            )
        })
        .collect();
    let log = if verbose_log { em.get_log_contents() } else { String::new() };
    Outcome { ok: result.is_success, errors: result.errors, resolved: result.resolved_ast, history, log, document }
}

/// Resolves the source file at `path` whose text is `source`.
fn resolve_source(path: &str, source: &str, run: Run) -> Outcome {
    let _guard = registry_lock();
    let prepared = prepare(path, source.as_bytes());
    let document = run.through_document.then_some(prepared.document);
    resolve(prepared.enhanced, &prepared.symbols, &prepared.settings, document, run.verbose_log)
}

/// The fingerprint of an outcome: the verdict, the errors, the whole resolved tree and the history
/// of calls, with the order of the `@CONFIG` entries (a hash map) fixed and the times masked.
fn fingerprint(outcome: &Outcome) -> u64 {
    let mut resolved = outcome.resolved.clone();
    if let Some(config) = resolved.as_mut().and_then(|r| r.config.as_mut()) {
        config.entries.sort_by(|a, b| a.key.cmp(&b.key));
    }
    fnv(&mask_times(&format!("{}|{:?}|{:?}|{:?}", outcome.ok, outcome.errors, resolved, outcome.history)))
}

/// `log` without its colour codes.
fn strip_ansi(log: &str) -> String {
    let mut plain = String::with_capacity(log.len());
    let mut chars = log.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for d in chars.by_ref() {
                if d == 'm' {
                    break;
                }
            }
        } else {
            plain.push(c);
        }
    }
    plain
}

/// The messages of the log lines that contain `needle`, without their time and level.
fn messages(log: &str, needle: &str) -> Vec<String> {
    strip_ansi(log)
        .lines()
        .filter(|line| line.contains(needle))
        .map(|line| line.splitn(3, "] ").nth(2).unwrap_or(line).trim_end().to_string())
        .collect()
}

/// The data context as the verbose log prints it, `(path, value)` sorted by path.
fn context_of(log: &str) -> BTreeMap<String, String> {
    let plain = strip_ansi(log);
    let mut context = BTreeMap::new();
    for line in plain.lines() {
        if let Some(rest) = line.split("[Debug]   ").nth(1) {
            if let Some((key, value)) = rest.split_once(" = DixValue ") {
                if key.starts_with("DATA") {
                    context.insert(key.to_string(), format!("DixValue {value}"));
                }
            }
        }
    }
    context
}

/// Splits `text` at the top-level `", "` separators, outside strings and brackets.
fn split_top(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let (mut parts, mut depth, mut start, mut quoted, mut i) = (Vec::new(), 0i32, 0usize, false, 0usize);
    while i < bytes.len() {
        let c = bytes[i];
        if quoted {
            if c == b'\\' {
                i += 1;
            } else if c == b'"' {
                quoted = false;
            }
        } else {
            match c {
                b'"' => quoted = true,
                b'{' | b'(' | b'[' => depth += 1,
                b'}' | b')' | b']' => depth -= 1,
                b',' if depth == 0 && bytes.get(i + 1) == Some(&b' ') => {
                    parts.push(&text[start..i]);
                    start = i + 2;
                    i += 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    parts.push(&text[start..]);
    parts
}

/// Sorts the entries of every `Object({...})` in `text`; their order comes from a hash map.
fn canonical_objects(text: &str) -> String {
    const OPEN: &str = "Object({";
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(OPEN) {
        let body = at + OPEN.len();
        let bytes = rest.as_bytes();
        let (mut depth, mut quoted, mut end) = (1i32, false, body);
        while depth > 0 && end < bytes.len() {
            let c = bytes[end];
            if quoted {
                if c == b'\\' {
                    end += 1;
                } else if c == b'"' {
                    quoted = false;
                }
            } else if c == b'"' {
                quoted = true;
            } else if c == b'{' {
                depth += 1;
            } else if c == b'}' {
                depth -= 1;
            }
            end += 1;
        }
        let inner = &rest[body..end - 1];
        let mut entries: Vec<String> =
            if inner.is_empty() { Vec::new() } else { split_top(inner).into_iter().map(canonical_objects).collect() };
        entries.sort();
        out.push_str(&rest[..body]);
        out.push_str(&entries.join(", "));
        out.push_str("})");
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// `context_of` with the times masked, one `path = value` line per entry.
///
/// A date or timestamp that does not parse becomes the time of the run, so the value of those
/// entries (`date_bad`, `ts_bad`) is replaced by a marker; their presence is what is recorded.
fn context_text(context: &BTreeMap<String, String>) -> String {
    context
        .iter()
        .map(|(k, v)| {
            if k.ends_with("date_bad") || k.ends_with("ts_bad") {
                format!("{k} = <now>")
            } else {
                format!("{k} = {}", mask_times(&canonical_objects(v)))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn value_in<'a>(context: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    context.get(key).unwrap_or_else(|| panic!("no context entry {key}; has {:?}", context.keys().collect::<Vec<_>>()))
}

// ==================== sources ====================

const HEADER: &str = "@CONFIG(\n  version -> \"1.0.0\"\n  features -> \"advanced\"\n)\n\n";
const ENUMS: &str = "@ENUMS(\n  Color { RED = 1, GREEN = 2, BLUE = 3 }\n  Level { LOW = 1, MID = 5, HIGH = 10 }\n)\n\n";
const IMPORTS: &str = "@IMPORTS(\n  Base from \"base_types.mdix\"\n  Types from \"enum_only_types.mdix\"\n)\n\n";
/// Functions the hand-built sections call. Each takes one integer.
const FUNCTIONS: &str = "@QUICKFUNCS(\n  ~f<int> => global(x<int>) { return x + 1 }\n  ~g<int> => global(x<int>) { return x + 2 }\n  ~h<int> => global(x<int>) { return x + 3 }\n  ~i<int> => global(x<int>) { return x + 4 }\n)\n\n";

/// The crafted sources, `(name, text)`. None returns an object from a function or calls a random
/// or clock builtin, whose output could not be recorded.
fn sources() -> Vec<(&'static str, String)> {
    let (h, i) = (HEADER, IMPORTS);
    let qf = "@QUICKFUNCS(\n  ~inc<int> => global(x<int>) { return x + 1 }\n  ~add<int> => global(a<int>, b<int>) { return a + b }\n  ~label<string> => global(s<string>) { return s }\n)\n\n";
    let mut v: Vec<(&'static str, String)> = Vec::new();
    v.push(("d01_aliases", format!("{h}@DATA(\n  e0 = e1\n  e1 = e2\n  e2 = e3\n  e3 = 5\n  a = 1\n  b = a\n  c = b\n  tbl.srv:\n    host = \"h\",\n    port = 80,\n    ref_a = a,\n    ref_port = port\n  rows::\n    {{ x = 1 }},\n    {{ x = 2 }}\n)\n")));
    v.push(("d02_calls_positions", format!("{h}{qf}@DATA(\n  a = inc(1)\n  b = add(inc(1), inc(2))\n  arr = [inc(1), inc(2), 3]\n  grid = [[inc(1)], [inc(2), 3]]\n  obj = {{ x = inc(1), y = add(1, 2), nested = {{ z = inc(4) }} }}\n  tup = t:(inc(1), 2)\n  tbl.t:\n    k = inc(5),\n    l = add(inc(1), 2)\n  rows::\n    {{ a = inc(1) }},\n    {{ a = inc(2), b = label(\"x\") }}\n  calls::\n    inc(7),\n    add(1, 2)\n)\n")));
    v.push(("d03_imported_calls", format!("{h}{i}@DATA(\n  a = Base.scaledValue(10, Base.Rarity.RARE)\n  b = Base.scaledValue(Base.scaledValue(1, Base.Rarity.COMMON), Base.Rarity.EPIC)\n  arr = [Base.scaledValue(1, Base.Rarity.COMMON), 2]\n  tbl.t:\n    k = Base.scaledValue(3, Base.Rarity.RARE)\n)\n")));
    v.push(("d04_literals_only", format!("{h}@DATA(\n  i = 1\n  s = \"text\"\n  arr = [1, 2, 3]\n  obj = {{ a = 1, b = [1, 2], c = {{ d = 2 }} }}\n  tbl.t:\n    x = 1,\n    y = \"z\"\n  rows::\n    {{ a = 1 }},\n    {{ a = 2 }}\n)\n")));
    v.push(("d05_mixed_scopes", format!("{h}@QUICKFUNCS(\n  ~inc<int> => global(x<int>) {{ return x + 1 }}\n  ~sub<int> => srv(x<int>) {{ return x - 1 }}\n)\n\n@DATA(\n  top = inc(1)\n  srv.cfg:\n    a = 1,\n    b = inc(2),\n    c = srv.sub(5)\n  other.cfg:\n    z = inc(3)\n  after = inc(4)\n)\n")));
    v
}

/// The corpus files the recording includes, relative to the crate directory.
const CORPUS: &[&str] = &[
    "../mdix_files/intermediate/FunctionScoping.mdix",
    "../mdix_files/intermediate/CompleteFeature.mdix",
    "../mdix_files/advanced/data_variable_usage.mdix",
    "../mdix_files/advanced/all_datatypes_test.mdix",
];

fn all_inputs() -> Vec<(String, String, String)> {
    let dir = fixture_dir();
    let mut inputs = Vec::new();
    for (name, text) in sources() {
        let path = dir.join(format!("{name}.mdix"));
        fs::write(&path, &text).expect("a source is written");
        inputs.push((name.to_string(), path.to_string_lossy().to_string(), text));
    }
    for rel in CORPUS {
        let text = fs::read_to_string(rel).unwrap_or_else(|e| panic!("the corpus file {rel} is missing: {e}"));
        inputs.push((rel.to_string(), rel.to_string(), text));
    }
    inputs
}

/// `(name, ok, fingerprint)` of every source, produced by running the original code.
const RECORDING: &[(&str, bool, u64)] = &[
    ("d01_aliases", true, 0xddcfb76f70f0afb7),
    ("d02_calls_positions", true, 0xbc762735fd4823b1),
    ("d03_imported_calls", true, 0x51b6d9290a693985),
    ("d04_literals_only", true, 0xbafcdf58f9387bef),
    ("d05_mixed_scopes", false, 0xcac73e9b5e15236e),
    ("../mdix_files/intermediate/FunctionScoping.mdix", true, 0x914524c2f6ae31c4),
    ("../mdix_files/intermediate/CompleteFeature.mdix", true, 0x2561bf37d16957a2),
    ("../mdix_files/advanced/data_variable_usage.mdix", true, 0xe6315e06366fd7a8),
    ("../mdix_files/advanced/all_datatypes_test.mdix", true, 0xc63e697a49b62871),
];

/// `(case, ok, fingerprint)` of every hand-built case, produced by running the original code.
const HAND_RECORDING: &[(&str, bool, u64)] = &[
    ("empty", true, 0x545e85d2a6e55ff9),
    ("no_calls", true, 0xc64dcb4aa19c61d8),
    ("ctx_scalars", true, 0x3642bcd8894c2ba5),
    ("ctx_ctor", true, 0xd772bc73dbb8b4ed),
    ("ctx_containers", true, 0x903d9ac354b9b592),
    ("ctx_tables_groups", true, 0x1d38949d2c5a18fc),
    ("alias_limit", true, 0x72e1cb7332973cfa),
    ("walk_values", true, 0xcf771f96b3a4b6d9),
    ("walk_exprs_entered", true, 0xef84035f3a1b0419),
    ("walk_exprs_not_entered", true, 0xdbfc06ade2175705),
    ("walk_names", false, 0x156b92b2fa6f37bb),
    ("walk_scopes", true, 0xa37c5febbfd984a1),
    ("alias_forms", true, 0xc9e161e1db9d21a2),
    ("alias_table_only", true, 0x92475e1f107ca73b),
    ("alias_group_only", true, 0x85455ed7d310b2f3),
];

/// `(case, summary, fingerprint)` of the data context of every hand-built case, produced by
/// running the original code. The summary is the number of entries.
const CONTEXT_RECORDING: &[(&str, usize, u64)] = &[
    ("empty", 0, 0xcbf29ce484222325),
    ("no_calls", 2, 0xceb817741b97bcb1),
    ("ctx_scalars", 16, 0x78b82b2531603e4f),
    ("ctx_ctor", 9, 0x9f67384e91af6f8f),
    ("ctx_containers", 39, 0xd62aaff3f676ded2),
    ("ctx_tables_groups", 20, 0x6765a088b89f7959),
    ("alias_limit", 13, 0xdea2d8162101756a),
    ("walk_values", 3, 0x95cdb400682bf368),
    ("walk_exprs_entered", 0, 0xcbf29ce484222325),
    ("walk_exprs_not_entered", 0, 0xcbf29ce484222325),
    ("walk_names", 0, 0xcbf29ce484222325),
    ("walk_scopes", 4, 0x96628f6317f63919),
    ("alias_forms", 17, 0x71c757928b7097d5),
    ("alias_table_only", 2, 0xd80a461c929d0f5c),
    ("alias_group_only", 3, 0xd84e4c717427dd77),
];

/// `(case, summary, fingerprint)` of the calls found in every hand-built case, produced by
/// running the original code. The summary is `name@location` of each call, in order.
const WALK_RECORDING: &[(&str, &str, u64)] = &[
    ("empty", "", 0x09612b07b5ecb5a5),
    ("no_calls", "", 0x09612b07b5ecb5a5),
    ("ctx_scalars", "f@DATA.call", 0x1bd09087ca3f2ab6),
    ("ctx_ctor", "f@DATA.t_call", 0xc858f613853323e7),
    ("ctx_containers", "f@DATA.a_partial[1],f@DATA.a_all_calls[0],g@DATA.a_all_calls[1],f@DATA.o_call.c", 0xdc20df9192e5e9d0),
    ("ctx_tables_groups", "f@DATA.g1[5]", 0xb81adb92bbd5f2a9),
    ("alias_limit", "", 0x09612b07b5ecb5a5),
    ("walk_values", "f@DATA.direct,f@DATA.nested_args,g@DATA.nested_args,f@DATA.arr[0],g@DATA.arr[2],f@DATA.nest[0][0],f@DATA.obj.a,g@DATA.obj.b.c,f@DATA.objprop.a,f@DATA.ctor,f@DATA.interp,f@DATA.range,g@DATA.range,f@DATA.lambda,f@DATA.wrapped", 0x1243285d06cf7216),
    ("walk_exprs_entered", "f@DATA.arith,g@DATA.arith,f@DATA.cmp,f@DATA.logic,f@DATA.unary,f@DATA.cond,g@DATA.cond,h@DATA.cond,f@DATA.prop,f@DATA.idx,g@DATA.idx,f@DATA.stat,f@DATA.inst,g@DATA.inst,Base.scaledValue@DATA.imp_nested,f@DATA.imp_nested,Base.scaledValue@DATA.imp_nested,f@DATA.eqf_nested", 0x5bf66255fa60776b),
    ("walk_exprs_not_entered", "", 0x09612b07b5ecb5a5),
    ("walk_names", "f@DATA.plain,Base.scaledValue@DATA.imported,Zed.f@DATA.unknown_ns,Base.a.b@DATA.three,Base.@DATA.trailing_dot,Base.scaledValue@DATA.expr_imported,Zed.f@DATA.expr_unknown,Zed.q@DATA.imp_node", 0x01bdf789370432fa),
    ("walk_scopes", "f@DATA.s0,f@DATA.a.b.p,g@DATA.a.b.q[0],h@DATA.a.b.q[2],i@DATA.a.b.r.k,f@DATA.s1,f@DATA.rows[0].c,g@DATA.rows[1],h@DATA.rows[2][0],i@DATA.rows[4],f@DATA.x.y[0],f@DATA.op.a,g@DATA.op.b[0][0],f@DATA.s2", 0x5e94f973354c25cb),
    ("alias_forms", "", 0x09612b07b5ecb5a5),
    ("alias_table_only", "", 0x09612b07b5ecb5a5),
    ("alias_group_only", "", 0x09612b07b5ecb5a5),
];


/// The base every hand-built section is run against: local enums, both imports and the functions.
fn base_source() -> String {
    format!("{HEADER}{IMPORTS}{ENUMS}{FUNCTIONS}@DATA(\n  x = 1\n)\n")
}

fn base_path() -> PathBuf {
    let path = fixture_dir().join("hand_base.mdix");
    fs::write(&path, base_source()).expect("the base is written");
    path
}

/// Prepares the base and puts `entries` in as its data section, in the tree and in a document
/// built from it, as the loader's document is.
fn prepared_with(entries: Vec<DataEntry>) -> Prepared {
    let path = base_path();
    let mut prepared = prepare(&path.to_string_lossy(), base_source().as_bytes());
    prepared.enhanced.data = Some(DataSection::new(entries, Position { line: 1, column: 1 }));
    prepared.document = Document::from_owned(&prepared.enhanced);
    prepared
}

fn resolve_hand(entries: Vec<DataEntry>, run: Run) -> Outcome {
    let _guard = registry_lock();
    let prepared = prepared_with(entries);
    let document = run.through_document.then_some(prepared.document);
    resolve(prepared.enhanced, &prepared.symbols, &prepared.settings, document, run.verbose_log)
}

fn case_entries(case: &str) -> Vec<DataEntry> {
    hand_cases().into_iter().find(|(n, _)| *n == case).unwrap_or_else(|| panic!("no case {case}")).1
}

/// The calls the owned call finder finds in `entries`.
fn walk_owned(entries: Vec<DataEntry>) -> Vec<FunctionCallInfo> {
    let _guard = registry_lock();
    let prepared = prepared_with(entries);
    let data = prepared.enhanced.data.as_ref().expect("a data section");
    ASTWalker::new(ErrorManager::new_isolated_silent(), &prepared.symbols, DebugMode::Off).find_all(data)
}

/// The calls the arena call finder finds in `entries`.
fn walk_arena(entries: Vec<DataEntry>) -> Vec<FunctionCallInfo> {
    let _guard = registry_lock();
    let prepared = prepared_with(entries);
    let data = prepared.document.data.as_ref().expect("a data section");
    find_in(&prepared.symbols, &prepared.document, data)
}

/// The one place the tests call the arena entry of the call finder.
fn find_in(
    symbols: &SymbolTable,
    document: &Document,
    data: &dixscript::Compiler::AST::arena::DocData,
) -> Vec<FunctionCallInfo> {
    ASTWalker::new(ErrorManager::new_isolated_silent(), symbols, DebugMode::Off).find_all_in(&document.arena, data)
}

/// `name@location` of each call, in the order they were found.
fn walk_summary(calls: &[FunctionCallInfo]) -> String {
    calls
        .iter()
        .map(|c| match &c.namespace_name {
            Some(ns) => format!("{ns}.{}@{}", c.function_name, c.location),
            None => format!("{}@{}", c.function_name, c.location),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn hand_runs(through_document: bool) -> Vec<(&'static str, bool, u64)> {
    hand_cases()
        .into_iter()
        .map(|(case, entries)| {
            let outcome = resolve_hand(entries, Run { through_document, verbose_log: false });
            (case, outcome.ok, fingerprint(&outcome))
        })
        .collect()
}

fn context_runs(through_document: bool) -> Vec<(&'static str, usize, u64)> {
    hand_cases()
        .into_iter()
        .map(|(case, entries)| {
            let outcome = resolve_hand(entries, Run { through_document, verbose_log: true });
            let context = context_of(&outcome.log);
            (case, context.len(), fnv(&context_text(&context)))
        })
        .collect()
}

fn walk_runs(arena: bool) -> Vec<(&'static str, String, u64)> {
    hand_cases()
        .into_iter()
        .map(|(case, entries)| {
            let calls = if arena { walk_arena(entries) } else { walk_owned(entries) };
            (case, walk_summary(&calls), fnv(&mask_times(&format!("{calls:?}"))))
        })
        .collect()
}

#[test]
#[ignore]
fn print_recording() {
    for (name, path, text) in all_inputs() {
        let outcome = resolve_source(&path, &text, PLAIN);
        println!("RECORDING    (\"{}\", {}, 0x{:016x}),", name, outcome.ok, fingerprint(&outcome));
    }
    for (case, ok, hash) in hand_runs(false) {
        println!("HAND_RECORDING    (\"{}\", {}, 0x{:016x}),", case, ok, hash);
    }
    for (case, count, hash) in context_runs(false) {
        println!("CONTEXT_RECORDING    (\"{}\", {}, 0x{:016x}),", case, count, hash);
    }
    for (case, summary, hash) in walk_runs(false) {
        println!("WALK_RECORDING    (\"{}\", \"{}\", 0x{:016x}),", case, summary, hash);
    }
}

#[test]
#[ignore]
fn print_contexts() {
    for (case, entries) in hand_cases() {
        let outcome = resolve_hand(entries, Run { through_document: false, verbose_log: true });
        println!("CASE {case}");
        let context = context_of(&outcome.log);
        for line in context_text(&context).lines() {
            println!("   {line}");
        }
    }
}

#[test]
#[ignore]
fn print_logs() {
    for case in ["no_calls", "alias_forms", "alias_limit", "ctx_scalars", "walk_values", "walk_names", "ctx_tables_groups", "alias_table_only", "alias_group_only"] {
        let outcome = resolve_hand(case_entries(case), Run { through_document: false, verbose_log: true });
        println!("LOGCASE {case}");
        for needle in ["IdentifierAliasPass", "Data context populated", "function calls in DATA", "Processing entry 0"] {
            for m in messages(&outcome.log, needle) {
                println!("   {m}");
            }
        }
    }
}

fn check_recording(through_document: bool) {
    let inputs = all_inputs();
    assert_eq!(inputs.len(), RECORDING.len(), "RECORDING is out of date; run print_recording");
    for ((name, path, text), (rec_name, rec_ok, rec_hash)) in inputs.iter().zip(RECORDING) {
        assert_eq!(name, rec_name);
        let outcome = resolve_source(path, text, Run { through_document, verbose_log: false });
        assert_eq!(outcome.ok, *rec_ok, "{name}: verdict");
        assert_eq!(fingerprint(&outcome), *rec_hash, "{name}: result (document: {through_document})");
    }
}

#[test]
fn recorded_sources_match_through_the_document() {
    check_recording(true);
}

#[test]
fn recorded_sources_match_through_the_owned_entry_point() {
    check_recording(false);
}

fn check_hand_recording(through_document: bool) {
    let runs = hand_runs(through_document);
    assert_eq!(runs.len(), HAND_RECORDING.len(), "HAND_RECORDING is out of date; run print_recording");
    for (run, rec) in runs.iter().zip(HAND_RECORDING) {
        assert_eq!(run.0, rec.0);
        assert_eq!(run.1, rec.1, "{}: verdict", rec.0);
        assert_eq!(run.2, rec.2, "{}: result (document: {through_document})", rec.0);
    }
}

#[test]
fn recorded_hand_built_cases_match_through_the_document() {
    check_hand_recording(true);
}

#[test]
fn recorded_hand_built_cases_match_through_the_owned_entry_point() {
    check_hand_recording(false);
}

fn check_context_recording(through_document: bool) {
    let runs = context_runs(through_document);
    assert_eq!(runs.len(), CONTEXT_RECORDING.len(), "CONTEXT_RECORDING is out of date; run print_recording");
    for (run, rec) in runs.iter().zip(CONTEXT_RECORDING) {
        assert_eq!(run.0, rec.0);
        assert_eq!(run.1, rec.1, "{}: number of context entries", rec.0);
        assert_eq!(run.2, rec.2, "{}: context (document: {through_document})", rec.0);
    }
}

#[test]
fn recorded_data_contexts_match_through_the_document() {
    check_context_recording(true);
}

#[test]
fn recorded_data_contexts_match_through_the_owned_entry_point() {
    check_context_recording(false);
}

#[test]
fn recorded_found_calls_match_for_the_owned_and_the_arena_call_finder() {
    for arena in [false, true] {
        let runs = walk_runs(arena);
        assert_eq!(runs.len(), WALK_RECORDING.len(), "WALK_RECORDING is out of date; run print_recording");
        for (run, rec) in runs.iter().zip(WALK_RECORDING) {
            assert_eq!(run.0, rec.0);
            assert_eq!(run.1, rec.1, "{}: calls found (arena: {arena})", rec.0);
            assert_eq!(run.2, rec.2, "{}: the calls in full (arena: {arena})", rec.0);
        }
    }
}

// ==================== the data context ====================

/// The context of a hand-built case, through both paths, which must agree.
fn context_of_case(case: &str) -> BTreeMap<String, String> {
    let through_document = context_of(&resolve_hand(case_entries(case), Run { through_document: true, verbose_log: true }).log);
    let owned = context_of(&resolve_hand(case_entries(case), Run { through_document: false, verbose_log: true }).log);
    assert_eq!(context_text(&through_document), context_text(&owned), "{case}: the two paths agree");
    owned
}

fn assert_absent(context: &BTreeMap<String, String>, keys: &[&str]) {
    for key in keys {
        assert!(!context.contains_key(*key), "{key} should not be in the context");
    }
}

#[test]
fn the_context_converts_every_kind_of_literal() {
    let context = context_of_case("ctx_scalars");
    for (key, expected) in [
        ("DATA.i", "Int(1)"),
        ("DATA.l", "Long(5000000000)"),
        ("DATA.f", "Float(1.5)"),
        ("DATA.d", "Double(2.25)"),
        ("DATA.sci", "Double(10000000000.0)"),
        ("DATA.s", "String(\"text\")"),
        ("DATA.t", "Bool(true)"),
        ("DATA.n", "Null"),
        ("DATA.h", "Hex(\"#FF00AA\")"),
        ("DATA.date_ok", "Date(2024-03-05T00:00:00Z)"),
        ("DATA.date_full", "Date(2024-03-05T10:20:30Z)"),
        ("DATA.ts_ok", "Timestamp(2024-03-05T10:20:30Z)"),
        // The text of the template, and an enum value as the integer 0.
        ("DATA.interp", "String(\"a{0}\")"),
        ("DATA.en", "Int(0)"),
    ] {
        assert!(value_in(&context, key).contains(expected), "{key}: {}", value_in(&context, key));
    }
    // A date or timestamp that does not parse still gets an entry, holding the time of the run.
    value_in(&context, "DATA.date_bad");
    value_in(&context, "DATA.ts_bad");
    // What is not a literal is not in the context.
    assert_absent(&context, &["DATA.unres", "DATA.err", "DATA.perr", "DATA.call", "DATA.expr"]);
}

#[test]
fn prefixed_constructors_convert_for_b_r_and_t_only() {
    let context = context_of_case("ctx_ctor");
    assert!(value_in(&context, "DATA.b_ok").contains("Blob(\"aGVsbG8=\")"));
    // A blob without a string argument is empty, and a regex without one matches anything.
    assert!(value_in(&context, "DATA.b_none").contains("Blob(\"\")"));
    assert!(value_in(&context, "DATA.b_nonstr").contains("Blob(\"\")"));
    assert!(value_in(&context, "DATA.r_ok").contains("Regex(\"[a-z]+\")"));
    assert!(value_in(&context, "DATA.r_none").contains("Regex(\".*\")"));
    assert!(value_in(&context, "DATA.t3").contains("Tuple("));
    assert_eq!(value_in(&context, "DATA.t7").matches("Int(").count(), 6, "a tuple keeps six elements");
    assert!(value_in(&context, "DATA.t_empty").contains("Tuple([])"));
    // The prefix is not case sensitive.
    assert!(value_in(&context, "DATA.upper").contains("Tuple("));
    // An invalid regex, a tuple with a call in it and an unknown prefix do not convert.
    assert_absent(&context, &["DATA.r_bad", "DATA.t_call", "DATA.other"]);
}

#[test]
fn an_array_or_object_is_in_the_context_only_when_every_part_converted() {
    let context = context_of_case("ctx_containers");
    // The parts of an array with a call in it are there, the array is not.
    value_in(&context, "DATA.a_partial[0]");
    value_in(&context, "DATA.a_partial[2]");
    assert_absent(&context, &["DATA.a_partial", "DATA.a_partial[1]", "DATA.a_all_calls", "DATA.a_all_calls[0]"]);
    // The same for an object.
    value_in(&context, "DATA.o_call.x");
    value_in(&context, "DATA.o_call.z");
    assert_absent(&context, &["DATA.o_call", "DATA.o_call.c"]);
    // An empty array is not inserted and an empty object is.
    assert_absent(&context, &["DATA.a_empty"]);
    assert!(value_in(&context, "DATA.o_empty").contains("Object({})"));
    // Every level of a nested array and object is there, and a property whose value is an array is
    // inserted like any other.
    for key in ["DATA.a_nested", "DATA.a_nested[0]", "DATA.a_nested[0][1]", "DATA.a_nested[1][0]", "DATA.a_in_a[0][0]", "DATA.o_nested.n.q[1]", "DATA.op_flat.m.deep", "DATA.op_nonobject[1]"] {
        value_in(&context, key);
    }
    assert!(value_in(&context, "DATA.a_mixed").contains("Null"), "an array may hold null");
    assert_absent(&context, &["DATA.a_in_a[1]"]);
}

#[test]
fn tables_and_groups_have_their_own_paths() {
    let context = context_of_case("ctx_tables_groups");
    for (key, expected) in [
        ("DATA.srv.host", "String(\"h\")"),
        ("DATA.srv.port", "Int(80)"),
        ("DATA.a.b.c.deep", "Int(7)"),
        ("DATA.g1[0]", "Int(1)"),
        ("DATA.g1[1]", "String(\"two\")"),
        ("DATA.g1[3][0]", "Int(3)"),
        ("DATA.g1[4].k", "Int(4)"),
        ("DATA.g.h[0]", "Int(5)"),
        ("DATA.srv.arr[1]", "Int(2)"),
    ] {
        assert!(value_in(&context, key).contains(expected), "{key}");
    }
    // Aliases inside tables and groups, found by the end of a path.
    for (key, expected) in [
        ("DATA.srv.ref_top", "Int(10)"),
        ("DATA.srv.ref_port", "Int(80)"),
        ("DATA.a.b.c.like_deep", "Int(7)"),
        ("DATA.g1[2]", "Int(10)"),
        ("DATA.g.h[1]", "Int(7)"),
    ] {
        assert!(value_in(&context, key).contains(expected), "{key}");
    }
    assert_absent(&context, &["DATA.srv.miss", "DATA.g1[5]", "DATA.g1[6]", "DATA.t2.x", "DATA.g3[0]"]);
}

#[test]
fn aliases_resolve_in_passes_and_stop_after_eight() {
    let context = context_of_case("alias_limit");
    // A chain declared backwards needs one pass per link: nine links are in, the tenth is not.
    for n in 1..=9 {
        assert!(value_in(&context, &format!("DATA.e{n}")).contains("Int(42)"), "e{n}");
    }
    assert_absent(&context, &["DATA.e0"]);
    // A name declared later is found on a later pass.
    assert!(value_in(&context, "DATA.fwd").contains("Int(1)"));
    // A cycle and a self reference never resolve.
    assert_absent(&context, &["DATA.cyc_a", "DATA.cyc_b", "DATA.self_ref"]);
    // When a name is declared twice the last one wins, and an alias to it follows.
    assert!(value_in(&context, "DATA.dup").contains("Int(2)"));
    assert!(value_in(&context, "DATA.dup_alias").contains("Int(2)"));
}

#[test]
fn an_empty_data_section_has_an_empty_context() {
    assert!(context_of_case("empty").is_empty());
    assert_eq!(context_of_case("no_calls").len(), 2);
}

#[test]
fn aliases_resolve_inside_tables_and_groups_and_by_full_key() {
    let context = context_of_case("alias_forms");
    for (key, expected) in [
        // An alias in the first entry.
        ("DATA.first_alias", "Int(3)"),
        // Found by the end of a path, not by a name that only ends the same way.
        ("DATA.alias_top", "Int(10)"),
        // Named by its full key.
        ("DATA.exact_alias", "Int(10)"),
        // A chain inside a table, and an alias in a group of an alias.
        ("DATA.tc.p0", "Int(5)"),
        ("DATA.tc.p1", "Int(5)"),
        ("DATA.gc[0]", "Int(7)"),
        ("DATA.gc[1]", "Int(1)"),
        ("DATA.gc_alias", "Int(7)"),
        ("DATA.xtop", "Int(99)"),
        ("DATA.ytop", "Int(98)"),
        ("DATA.ztop", "Int(97)"),
    ] {
        assert!(value_in(&context, key).contains(expected), "{key}: {}", value_in(&context, key));
    }
    // An object property is not an alias, but its members are in the context.
    value_in(&context, "DATA.op_alias.k");
}

fn log_of(case: &str, through_document: bool) -> String {
    resolve_hand(case_entries(case), Run { through_document, verbose_log: true }).log
}

#[test]
fn the_phases_log_their_passes_and_counts() {
    let passes = |changed: &[bool]| -> Vec<String> {
        changed.iter().enumerate().map(|(i, c)| format!("[IdentifierAliasPass {}] changed={c}", i + 1)).collect()
    };
    let cases: &[(&str, Vec<String>, usize, usize)] = &[
        // (case, the alias passes, literal entries in the context, calls found)
        ("no_calls", passes(&[false]), 2, 0),
        ("alias_forms", passes(&[true, true, false]), 10, 0),
        ("alias_limit", passes(&[true; 8]), 4, 0),
        // One alias each, in a table and in a group: the only change of the pass.
        ("alias_table_only", passes(&[true, false]), 1, 0),
        ("alias_group_only", passes(&[true, false]), 2, 0),
        ("ctx_tables_groups", passes(&[true, false]), 14, 1),
        ("ctx_scalars", passes(&[false]), 16, 1),
        ("walk_values", passes(&[false]), 3, 15),
        ("walk_names", passes(&[false]), 0, 8),
    ];
    for through_document in [true, false] {
        for (case, alias_passes, literals, calls) in cases {
            let log = log_of(case, through_document);
            assert_eq!(&messages(&log, "IdentifierAliasPass"), alias_passes, "{case}");
            assert_eq!(
                messages(&log, "Data context populated"),
                [format!("Data context populated: {literals} literal entries")],
                "{case}"
            );
            assert_eq!(
                messages(&log, "function calls in DATA"),
                [format!("Found {calls} function calls in DATA section")],
                "{case}"
            );
            // The call finder says what it does at the verbose level, entry by entry.
            assert_eq!(
                messages(&log, "Processing entry 0"),
                ["[ASTWalker] Processing entry 0 (scope reset to ROOT)".to_string()],
                "{case}"
            );
        }
    }
}

// ==================== the call finder ====================

fn found(case: &str) -> Vec<String> {
    walk_summary(&walk_owned(case_entries(case))).split(',').filter(|s| !s.is_empty()).map(String::from).collect()
}

#[test]
fn the_call_finder_enters_every_value_position() {
    assert_eq!(
        found("walk_values"),
        [
            "f@DATA.direct", "f@DATA.nested_args", "g@DATA.nested_args", "f@DATA.arr[0]", "g@DATA.arr[2]",
            "f@DATA.nest[0][0]", "f@DATA.obj.a", "g@DATA.obj.b.c", "f@DATA.objprop.a", "f@DATA.ctor",
            "f@DATA.interp", "f@DATA.range", "g@DATA.range", "f@DATA.lambda", "f@DATA.wrapped",
        ]
    );
}

#[test]
fn the_call_finder_enters_some_expression_kinds_and_not_others() {
    // Entered: the binary operators except the bitwise ones, unary, conditional, property and index
    // access, static and instance method calls, and the arguments of an imported function call.
    assert_eq!(
        found("walk_exprs_entered"),
        [
            "f@DATA.arith", "g@DATA.arith", "f@DATA.cmp", "f@DATA.logic", "f@DATA.unary", "f@DATA.cond",
            "g@DATA.cond", "h@DATA.cond", "f@DATA.prop", "f@DATA.idx", "g@DATA.idx", "f@DATA.stat",
            "f@DATA.inst", "g@DATA.inst", "Base.scaledValue@DATA.imp_nested", "f@DATA.imp_nested",
            "Base.scaledValue@DATA.imp_nested",
            // The call inside a call in an expression is not found, only the outer one.
            "f@DATA.eqf_nested",
        ]
    );
    // Not entered: bitwise operators, parentheses, casts, built-in, static and plain function calls,
    // qualified identifiers, values wrapped in an expression and the rest.
    assert!(found("walk_exprs_not_entered").is_empty());
}

#[test]
fn a_name_is_split_only_for_an_imported_namespace() {
    assert_eq!(
        found("walk_names"),
        [
            "f@DATA.plain",
            "Base.scaledValue@DATA.imported",
            // An unknown namespace, and a name with more than one dot, stay whole.
            "Zed.f@DATA.unknown_ns",
            "Base.a.b@DATA.three",
            // A trailing dot gives an empty function name in the imported namespace.
            "Base.@DATA.trailing_dot",
            "Base.scaledValue@DATA.expr_imported",
            "Zed.f@DATA.expr_unknown",
            // The node that names its namespace is not checked against the imports.
            "Zed.q@DATA.imp_node",
        ]
    );
    let calls = walk_owned(case_entries("walk_names"));
    let namespaces: Vec<Option<&str>> = calls.iter().map(|c| c.namespace_name.as_deref()).collect();
    assert_eq!(
        namespaces,
        [None, Some("Base"), None, None, Some("Base"), Some("Base"), None, Some("Zed")],
        "the name, not just the text, is split"
    );
}

#[test]
fn locations_follow_tables_groups_arrays_and_objects() {
    assert_eq!(
        found("walk_scopes"),
        [
            "f@DATA.s0", "f@DATA.a.b.p", "g@DATA.a.b.q[0]", "h@DATA.a.b.q[2]", "i@DATA.a.b.r.k", "f@DATA.s1",
            "f@DATA.rows[0].c", "g@DATA.rows[1]", "h@DATA.rows[2][0]", "i@DATA.rows[4]", "f@DATA.x.y[0]",
            "f@DATA.op.a", "g@DATA.op.b[0][0]", "f@DATA.s2",
        ]
    );
    assert!(found("empty").is_empty() && found("no_calls").is_empty());
}

#[test]
fn a_found_call_carries_its_arguments() {
    let calls = walk_owned(vec![simple("a", qfv("f", vec![ie(7), eqf("g", vec![ie(8)])]))]);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].function_name, "f");
    assert_eq!(calls[0].arguments.len(), 2);
    assert!(matches!(calls[0].arguments[0], E::Value { value: V::Integer { value: 7, .. }, .. }));
    assert!(matches!(calls[0].arguments[1], E::QuickFuncCall { ref name, .. } if name == "g"));
    assert_eq!(calls[1].function_name, "g");
    assert_eq!(calls[1].location, "DATA.a");
}

// ==================== which tree is read ====================

fn history_names(outcome: &Outcome) -> Vec<String> {
    outcome.history.iter().map(|h| h.split(' ').next().unwrap_or_default().to_string()).collect()
}

fn with_data(prepared: &Prepared, entries: Vec<DataEntry>) -> DixScript {
    let mut ast = prepared.enhanced.clone();
    ast.data = Some(DataSection::new(entries, Position { line: 1, column: 1 }));
    ast
}

#[test]
fn the_phases_read_the_document_when_it_has_one() {
    let _guard = registry_lock();
    let prepared = prepared_with(vec![]);
    let symbols = &prepared.symbols;
    let owned = || with_data(&prepared, vec![simple("o", qfv("f", vec![ie(1)]))]);
    let other = with_data(&prepared, vec![simple("d", qfv("g", vec![ie(2)]))]);

    // The owned tree calls `f` and the document `g`: with the document attached `g` is the call
    // found and made, without it `f`.
    let outcome = resolve(owned(), symbols, &prepared.settings, Some(Document::from_owned(&other)), false);
    assert_eq!(history_names(&outcome), ["g"]);
    let outcome = resolve(owned(), symbols, &prepared.settings, None, false);
    assert_eq!(history_names(&outcome), ["f"]);

    // A document without a data section: nothing is found, so the call stays where it is.
    let mut without = prepared.enhanced.clone();
    without.data = None;
    let outcome = resolve(owned(), symbols, &prepared.settings, Some(Document::from_owned(&without)), false);
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert!(outcome.history.is_empty());
    assert!(matches!(value_of(&outcome, "o"), V::QuickFuncCall { .. }));

    // An owned tree without one and a document with: the document's call is found and made.
    let mut none = prepared.enhanced.clone();
    none.data = None;
    let outcome = resolve(none, symbols, &prepared.settings, Some(Document::from_owned(&other)), false);
    assert_eq!(history_names(&outcome), ["g"]);
}

fn value_of<'a>(outcome: &'a Outcome, name: &str) -> &'a V {
    outcome
        .resolved
        .as_ref()
        .and_then(|r| r.data.as_ref())
        .expect("a data section")
        .entries
        .iter()
        .find_map(|entry| match entry {
            DataEntry::SimpleProperty { name: n, value, .. } if n == name => Some(value),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no property {name}"))
}

#[test]
fn the_owned_call_finder_is_the_arena_call_finder() {
    for (case, _) in hand_cases() {
        let owned = walk_owned(case_entries(case));
        let arena = walk_arena(case_entries(case));
        assert_eq!(format!("{owned:?}"), format!("{arena:?}"), "{case}");
    }
}

#[test]
fn phases_two_and_three_allocate_nothing_in_the_document() {
    let _guard = registry_lock();
    let prepared = prepared_with(vec![]);
    let ast = with_data(
        &prepared,
        vec![
            simple("a", int(1)),
            simple("b", qfv("f", vec![ie(1)])),
            simple("c", arr(vec![qfv("g", vec![ie(2)]), int(3)])),
            simple("d", ident("a")),
            tbl(&["t"], vec![("x", int(4)), ("y", qfv("h", vec![ie(5)])), ("z", ident("a"))]),
        ],
    );
    let document = Document::from_owned(&ast);
    let before = (document.arena.value_count(), document.arena.expr_count(), document.arena.used_bytes());
    let outcome = resolve(ast, &prepared.symbols, &prepared.settings, Some(document), false);
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert_eq!(history_names(&outcome).len(), 3, "the three calls were found and made");
    let document = outcome.document.as_ref().unwrap();
    assert_eq!((document.arena.value_count(), document.arena.expr_count(), document.arena.used_bytes()), before);
}
