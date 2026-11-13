// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/value_resolver_writeback_document.rs"
// ============================================================================
//! Phase 4 of value resolution (writing each call's result back into the data section), over the
//! arena.
//!
//! The write-back replaces a resolved call by its result at the call's position in the data
//! section. It moved from the owned tree to the arena without changing what it does, so most of
//! this file pins behaviour the owned write-back had:
//!
//! * a recording, produced by running the original code, of the whole outcome (verdict, errors,
//!   resolved tree and history of calls) of every input, with the document attached and without
//!   it;
//! * the shape of the resolved data section of every hand-built case, which reaches the
//!   quirks the grammar cannot: positions that more than one node shares, calls inside
//!   expressions, nested arrays, constructors and objects at every depth.
//!
//! What is new is how the result is written: along the path to the call, by allocating copies
//! of the arrays and objects on it and sharing everything else. The last tests check that
//! through the document's arena (they cannot pass on the owned write-back), and that the data
//! section of the document holds the result afterwards.
//!
//! `print_recording` and `print_shapes` (ignored) print the two tables; run them on the original
//! code to regenerate them.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use dixscript::Compiler::AST::arena::{DocDataEntry, Document, ValueId, ValueNode};
use dixscript::Compiler::AST::data_types::{CompatibilityMode, ErrorHandlingStrategy};
use dixscript::Compiler::AST::{
    DataEntry, DataSection, DataType, DebugMode, DixScript, Expression as E, ObjectProperty,
    Position, PropertyAssignment, TablePath, Value as V,
};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::ValueResolution::ValueResolver;
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
            .join(format!("dix_value_resolver_writeback_document_{}", std::process::id()));
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

// ==================== run helpers ====================

/// What a run of the resolver left behind.
struct Outcome {
    ok: bool,
    errors: Vec<String>,
    resolved: Option<DixScript>,
    /// One line per call the resolver made, without its time.
    history: Vec<String>,
    /// The document the resolver was given, after the run (`None` for a run without one).
    document: Option<Document>,
}

/// How a run is made.
#[derive(Clone, Copy)]
struct Run {
    through_document: bool,
}


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
) -> Outcome {
    let em = ErrorManager::new_isolated_silent();
    let mut document = document;
    let mut resolver = ValueResolver::new_with_error_manager(enhanced, symbols, settings.debug_mode, em);
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
    Outcome { ok: result.is_success, errors: result.errors, resolved: result.resolved_ast, history, document }
}

/// Resolves the source file at `path` whose text is `source`.
fn resolve_source(path: &str, source: &str, run: Run) -> Outcome {
    let _guard = registry_lock();
    let prepared = prepare(path, source.as_bytes());
    let document = run.through_document.then_some(prepared.document);
    resolve(prepared.enhanced, &prepared.symbols, &prepared.settings, document)
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

const HEADER: &str = "@CONFIG(\n  version -> \"1.0.0\"\n  features -> \"advanced\"\n)\n\n";
const ENUMS: &str = "@ENUMS(\n  Color { RED = 1, GREEN = 2, BLUE = 3 }\n  Level { LOW = 1, MID = 5, HIGH = 10 }\n)\n\n";
const IMPORTS: &str = "@IMPORTS(\n  Base from \"base_types.mdix\"\n  Types from \"enum_only_types.mdix\"\n)\n\n";

// ==================== V3 inputs (write-back and phase 5) ====================

const QF3: &str = "@QUICKFUNCS(\n  ~f<int> => global(x<int>) { return x + 1 }\n  ~g<int> => global(x<int>) { return x + 2 }\n  ~h<int> => global(x<int>) { return x + 3 }\n  ~i<int> => global(x<int>) { return x + 4 }\n  ~boom<int> => global(x<int>) { return x / 0 }\n  ~mklong<long> => global(x<int>) { return 9_000_000_000L + x }\n  ~mkfloat<float> => global(x<int>) { return 1.5f }\n  ~mkdouble<double> => global(x<int>) { return 2.5 }\n  ~mkstr<string> => global(s<string>) { return s + \"!\" }\n  ~mkbool<bool> => global(x<int>) { return x > 0 }\n  ~mkhex<hex> => global(x<int>) { return #FF5733 }\n  ~mkarr<array> => global(x<int>) { return [x, x + 1, x + 2] }\n  ~mktup<tuple> => global(x<int>) { return t:(x, x + 1) }\n  ~mkobj<object> => global(x<int>) { return { only = x } }\n  ~mkdate<date> => global(x<int>) { return 2025-12-31 }\n  ~mkts<timestamp> => global(x<int>) { return 2025-12-31T23:59:59Z }\n  ~mkblob<blob> => global(x<int>) { return b:(\"QUJD\") }\n  ~mkre<regex> => global(x<int>) { return r:(\"^a+$\") }\n  ~mknested<array> => global(x<int>) { return [[x], [x + 1]] }\n  ~mknull<object> => global(x<int>) { return null }\n)\n\n";

fn sources3() -> Vec<(&'static str, String)> {
    let (h, i, e, q) = (HEADER, IMPORTS, ENUMS, QF3);
    let mut v: Vec<(&'static str, String)> = Vec::new();
    v.push(("w01_types", format!("{h}{q}@DATA(\n  i = f(1)\n  l = mklong(1)\n  fl = mkfloat(1)\n  d = mkdouble(1)\n  s = mkstr(\"hi\")\n  b = mkbool(1)\n  hx = mkhex(1)\n  a = mkarr(1)\n  t = mktup(1)\n  o = mkobj(1)\n  dt = mkdate(1)\n  ts = mkts(1)\n  bl = mkblob(1)\n  re = mkre(1)\n  n = mknested(1)\n  nl = mknull(1)\n)\n")));
    v.push(("w02_positions", format!("{h}{q}@DATA(\n  a = f(1)\n  arr = [f(1), 2, g(3)]\n  grid = [[f(1)], [g(2), 3]]\n  obj = {{ x = f(1), y = {{ z = g(2) }} }}\n  tup = t:(f(1), 2)\n  tbl.t:\n    k = f(5),\n    m = 7,\n    n = g(6)\n  rows::\n    f(7),\n    8,\n    {{ a = h(1), b = i(2) }}\n)\n")));
    v.push(("w03_containers", format!("{h}{q}@DATA(\n  arr = [mkarr(1), 2, mkobj(3)]\n  tbl.t:\n    k = mkarr(1),\n    m = mktup(2),\n    n = mknested(3)\n  rows::\n    mkarr(1),\n    mkobj(2),\n    mkstr(\"z\")\n)\n")));
    v.push(("w04_chains", format!("{h}{q}@DATA(\n  a = f(1)\n  b = g(a)\n  c = h(b)\n  tbl.t:\n    k = f(2),\n    m = g(k)\n  arr = [f(1), f(2)]\n)\n")));
    v.push(("w05_errors", format!("{h}{q}@DATA(\n  a = f(1)\n  b = boom(1)\n  c = g(2)\n  tbl.t:\n    k = h(3)\n)\n")));
    v.push(("w06_cycle", format!("{h}{q}@DATA(\n  a = f(b)\n  b = g(a)\n  c = h(1)\n)\n")));
    v.push(("w07_enums", format!("{h}{e}{q}@DATA(\n  one = Color.RED\n  arr = [Color.RED, f(1)]\n  nest = [[Color.BLUE, f(2)], [Level.HIGH]]\n  rows::\n    f(1),\n    Color.GREEN\n)\n")));
    v.push(("w08_dups", format!("{h}{q}@DATA(\n  a = f(1)\n  b = f(1)\n  arr = [f(1), f(1)]\n  tbl.t:\n    k = f(1),\n    m = f(1)\n)\n")));
    v.push(("w09_imported", format!("{h}{i}{q}@DATA(\n  a = Base.scaledValue(10, Base.Rarity.RARE)\n  arr = [Base.scaledValue(1, Base.Rarity.COMMON), f(2)]\n  tbl.t:\n    k = Base.scaledValue(3, Base.Rarity.RARE)\n    m = mkarr(1)\n)\n")));
    v.push(("w10_only_literals", format!("{h}{q}@DATA(\n  a = 1\n  b = [1, 2]\n  o = {{ x = 1 }}\n)\n")));
    v
}

fn base_source3() -> String {
    format!("{HEADER}{IMPORTS}{ENUMS}{QF3}@DATA(\n  x = 1\n)\n")
}

fn prepared_with3(entries: Vec<DataEntry>) -> Prepared {
    let path = fixture_dir().join("hand_base3.mdix");
    fs::write(&path, base_source3()).expect("the base is written");
    let mut prepared = prepare(&path.to_string_lossy(), base_source3().as_bytes());
    prepared.enhanced.data = Some(DataSection::new(entries, Position { line: 1, column: 1 }));
    prepared.document = Document::from_owned(&prepared.enhanced);
    prepared
}

fn resolve_hand3(entries: Vec<DataEntry>, run: Run) -> Outcome {
    let _guard = registry_lock();
    let prepared = prepared_with3(entries);
    let document = run.through_document.then_some(prepared.document);
    resolve(prepared.enhanced, &prepared.symbols, &prepared.settings, document)
}

fn se(s: &str) -> E {
    E::Value { value: sv(s), position: pos() }
}
fn ide(name: &str) -> E {
    E::Identifier { name: name.into(), position: pos() }
}

fn hand3_cases() -> Vec<(&'static str, Vec<DataEntry>)> {
    POS.with(|p| p.set(1000));
    let call = |n: &str, a: i32| qfv(n, vec![ie(a)]);
    let mut v: Vec<(&'static str, Vec<DataEntry>)> = Vec::new();
    v.push(("w_simple", vec![simple("a", call("f", 1)), simple("b", int(2)), simple("c", call("g", 2))]));
    v.push(("w_table", vec![tbl(&["t"], vec![("k", call("f", 1)), ("m", int(5)), ("n", call("g", 2))])]));
    v.push(("w_group", vec![grp(&["rows"], vec![call("f", 1), int(2), call("g", 3), obj(vec![("a", call("h", 4))])])]));
    v.push(("w_objprop", vec![objprop("o", obj(vec![("x", call("f", 1)), ("y", obj(vec![("z", call("g", 2))]))]))]));
    v.push(("w_array", vec![simple("arr", arr(vec![call("f", 1), int(2), call("g", 3)]))]));
    v.push(("w_nested", vec![
        simple("n1", nest(vec![call("f", 1), int(3)], 1)),
        simple("n2", nest(vec![nest(vec![call("g", 1)], 2), arr(vec![call("h", 2)])], 1)),
        simple("n3", nest(vec![int(1), int(2)], 1)),
    ]));
    v.push(("w_ctor", vec![simple("c", ctor("t", vec![call("f", 1), int(2)])), simple("d", ctor("t", vec![int(1)]))]));
    v.push(("w_deep", vec![objprop("o", obj(vec![("a", arr(vec![obj(vec![("b", ctor("t", vec![call("f", 1)]))]), call("g", 2)]))]))]));
    v.push(("w_types", vec![
        simple("i", call("f", 1)),
        simple("l", call("mklong", 1)),
        simple("fl", call("mkfloat", 1)),
        simple("d", call("mkdouble", 1)),
        simple("s", qfv("mkstr", vec![se("hi")])),
        simple("b", call("mkbool", 1)),
        simple("hx", call("mkhex", 1)),
        simple("a", call("mkarr", 1)),
        simple("t", call("mktup", 1)),
        simple("o", call("mkobj", 1)),
        simple("dt", call("mkdate", 1)),
        simple("ts", call("mkts", 1)),
        simple("bl", call("mkblob", 1)),
        simple("re", call("mkre", 1)),
        simple("n", call("mknested", 1)),
        simple("nl", call("mknull", 1)),
        tbl(&["t"], vec![("k", call("mkarr", 2)), ("m", call("mkobj", 3))]),
        grp(&["rows"], vec![call("mkarr", 4), call("f", 5)]),
    ]));
    // A call that shares its position with the array holding it: the index finds the array first.
    let p = pos();
    let q = pos();
    v.push(("w_dup_array_call", vec![
        simple("a", V::Array { values: vec![V::QuickFuncCall { function_name: "f".into(), arguments: vec![ie(1)], position: p }, V::QuickFuncCall { function_name: "g".into(), arguments: vec![ie(2)], position: q }], position: p }),
    ]));
    // The same, with a second node at the later call's position elsewhere: the fallback search finds it.
    let p = pos();
    let q = pos();
    v.push(("w_dup_fallback", vec![
        simple("a", V::Array { values: vec![V::QuickFuncCall { function_name: "f".into(), arguments: vec![ie(1)], position: p }, V::QuickFuncCall { function_name: "g".into(), arguments: vec![ie(2)], position: q }], position: p }),
        simple("b", V::Integer { value: 9, position: q }),
    ]));
    // The first call's result is an array. The path of the second call now leads into it, to an
    // element that has the first call's position, so the path is not to be trusted.
    let p = pos();
    let q = pos();
    v.push(("w_dup_stale_slot", vec![
        simple("a", V::Array { values: vec![V::QuickFuncCall { function_name: "mkarr".into(), arguments: vec![ie(1)], position: p }, V::QuickFuncCall { function_name: "g".into(), arguments: vec![ie(2)], position: q }], position: p }),
    ]));
    // The fallback search in every kind of place it enters. Each of `h1` to `h7` replaces itself
    // by its first call's result, which strands the second call's path, and the search then
    // finds the other node with the second call's position: in a table, a group, an object
    // property entry, a constructor, a nested array, an object, and (the array that holds its
    // own element's position, which the array takes) an array.
    let holder = |i: usize| -> (DataEntry, Position) {
        let (p, q) = (pos(), pos());
        let call = |n: &str, a: i32, at: Position| V::QuickFuncCall { function_name: n.into(), arguments: vec![ie(a)], position: at };
        (simple(&format!("h{i}"), V::Array { values: vec![call("f", 1, p), call("g", 2, q)], position: p }), q)
    };
    let (h1, q1) = holder(1);
    let (h2, q2) = holder(2);
    let (h3, q3) = holder(3);
    let (h4, q4) = holder(4);
    let (h5, q5) = holder(5);
    let (h6, q6) = holder(6);
    let (h7, q7) = holder(7);
    let plain = |at: Position| V::Integer { value: 9, position: at };
    v.push(("w_dup_fallback_everywhere", vec![
        h1, h2, h3, h4, h5, h6, h7,
        tbl(&["t1"], vec![("k", plain(q1))]),
        grp(&["g2"], vec![plain(q2)]),
        objprop("o3", plain(q3)),
        simple("c4", ctor("t", vec![plain(q4)])),
        simple("n5", nest(vec![plain(q5)], 1)),
        objprop("o6", obj(vec![("x", plain(q6))])),
        simple("p7", V::Array { values: vec![plain(q7)], position: q7 }),
    ]));
    // Two calls with one position in different entries.
    let p = pos();
    v.push(("w_dup_calls", vec![
        simple("a", V::QuickFuncCall { function_name: "f".into(), arguments: vec![ie(1)], position: p }),
        simple("b", V::QuickFuncCall { function_name: "g".into(), arguments: vec![ie(2)], position: p }),
    ]));
    // An earlier plain node at the call's position takes the result.
    let p = pos();
    v.push(("w_dup_earlier", vec![
        simple("a", V::Integer { value: 7, position: p }),
        simple("b", V::QuickFuncCall { function_name: "f".into(), arguments: vec![ie(1)], position: p }),
    ]));
    let p = pos();
    v.push(("w_expr", vec![
        simple("e1", vexp(bin(0, ie(1), "+", eqf("f", vec![ie(2)])))),
        simple("e2", V::Expression { expr: bx(E::QuickFuncCall { name: "g".into(), arguments: vec![ie(3)], position: p }), position: p }),
        simple("e3", vexp(eqf("h", vec![ie(4)]))),
    ]));
    v.push(("w_fail", vec![simple("a", call("f", 1)), simple("b", call("boom", 1)), simple("c", call("g", 2)), tbl(&["t"], vec![("k", call("h", 3))])]));
    v.push(("w_chain", vec![
        simple("a", call("f", 1)),
        simple("b", qfv("g", vec![ide("a")])),
        simple("c", qfv("h", vec![ide("b")])),
        tbl(&["t"], vec![("k", call("f", 2)), ("m", qfv("g", vec![ide("k")]))]),
    ]));
    v.push(("w_cycle", vec![simple("a", qfv("f", vec![ide("b")])), simple("b", qfv("g", vec![ide("a")])), simple("c", call("h", 1))]));
    // Phase 5: identifiers named by a full path.
    v.push(("p_ids", vec![
        simple("x", call("f", 1)),
        simple("base", int(5)),
        simple("y", ident("DATA.x")),
        simple("z", ident("DATA.base")),
        simple("gone", ident("DATA.nothere")),
        simple("arr", arr(vec![ident("DATA.x"), ident("DATA.base"), int(3), ident("nope")])),
        simple("nest", nest(vec![ident("DATA.x"), int(1)], 1)),
        simple("nest_none", nest(vec![ident("nope")], 1)),
        objprop("o", obj(vec![("p", ident("DATA.x")), ("q", obj(vec![("r", ident("DATA.base"))])), ("s", ident("nope"))])),
        simple("c", ctor("t", vec![ident("DATA.x"), int(2)])),
        tbl(&["t"], vec![("k", ident("DATA.x")), ("m", ident("DATA.t.k")), ("n", ident("DATA.y"))]),
        grp(&["rows"], vec![ident("DATA.x"), int(2), obj(vec![("a", ident("DATA.base"))])]),
    ]));
    v.push(("p_chain", vec![
        simple("x", call("f", 1)),
        simple("w", ident("DATA.z")),
        simple("z", ident("DATA.y")),
        simple("y", ident("DATA.x")),
    ]));
    // Each identifier below resolves to `DATA.x`, and the one after it names the path the first
    // sits at, which only exists in the context once the pass has put it there. So each reader
    // resolves in the next pass if, and only if, the path was built exactly.
    v.push(("p_paths", vec![
        simple("x", call("f", 1)),
        simple("arr", arr(vec![ident("DATA.x")])),
        simple("r_arr", ident("DATA.arr[0]")),
        objprop("o", obj(vec![("k", ident("DATA.x")), ("deep", obj(vec![("a", arr(vec![ident("DATA.x")]))]))])),
        simple("r_o", ident("DATA.o.k")),
        simple("r_deep", ident("DATA.o.deep.a[0]")),
        simple("c", ctor("t", vec![ident("DATA.x")])),
        simple("r_c", ident("DATA.c.__arg0")),
        simple("ca", arr(vec![ctor("t", vec![ident("DATA.x")])])),
        simple("r_ca", ident("DATA.ca[0].__arg0")),
        simple("n", nest(vec![ident("DATA.x")], 1)),
        simple("r_n", ident("DATA.n[0]")),
        tbl(&["t"], vec![("k", ident("DATA.x"))]),
        simple("r_t", ident("DATA.t.k")),
        grp(&["rows"], vec![ident("DATA.x")]),
        simple("r_rows", ident("DATA.rows[0]")),
        tbl(&["a", "b"], vec![("k", ident("DATA.x"))]),
        simple("r_ab", ident("DATA.a.b.k")),
        grp(&["g", "h"], vec![ident("DATA.x")]),
        simple("r_gh", ident("DATA.g.h[0]")),
        tbl(&["t2"], vec![("a", ident("DATA.x")), ("b", ident("DATA.x"))]),
        simple("r_t2a", ident("DATA.t2.a")),
        simple("r_t2b", ident("DATA.t2.b")),
    ]));
    v.push(("p_types", vec![
        simple("i", call("f", 1)),
        simple("l", call("mklong", 1)),
        simple("fl", call("mkfloat", 1)),
        simple("d", call("mkdouble", 1)),
        simple("s", qfv("mkstr", vec![se("hi")])),
        simple("b", call("mkbool", 1)),
        simple("hx", call("mkhex", 1)),
        simple("a", call("mkarr", 1)),
        simple("t", call("mktup", 1)),
        simple("o", call("mkobj", 1)),
        simple("dt", call("mkdate", 1)),
        simple("ts", call("mkts", 1)),
        simple("bl", call("mkblob", 1)),
        simple("re", call("mkre", 1)),
        simple("n", call("mknested", 1)),
        simple("nl", call("mknull", 1)),
        simple("ri", ident("DATA.i")),
        simple("rl", ident("DATA.l")),
        simple("rfl", ident("DATA.fl")),
        simple("rd", ident("DATA.d")),
        simple("rs", ident("DATA.s")),
        simple("rb", ident("DATA.b")),
        simple("rhx", ident("DATA.hx")),
        simple("ra", ident("DATA.a")),
        simple("rt", ident("DATA.t")),
        simple("ro", ident("DATA.o")),
        simple("rdt", ident("DATA.dt")),
        simple("rts", ident("DATA.ts")),
        simple("rbl", ident("DATA.bl")),
        simple("rre", ident("DATA.re")),
        simple("rn", ident("DATA.n")),
        simple("rnl", ident("DATA.nl")),
        simple("ra0", ident("DATA.a[0]")),
        simple("ro_only", ident("DATA.o.only")),
    ]));
    v.push(("p_literals", vec![
        simple("base", int(5)),
        simple("s", sv("text")),
        simple("arr", arr(vec![int(1), int(2)])),
        simple("y", ident("DATA.base")),
        simple("ys", ident("DATA.s")),
        simple("ya", ident("DATA.arr")),
        simple("y1", ident("DATA.arr[1]")),
        simple("nest", nest(vec![ident("DATA.base")], 1)),
        simple("none", ident("DATA.none")),
    ]));
    v.push(("p_after_fail", vec![simple("x", call("f", 1)), simple("b", call("boom", 1)), simple("y", ident("DATA.x"))]));
    v.push(("p_expr_ident", vec![simple("x", call("f", 1)), simple("e", vexp(E::Identifier { name: "DATA.x".into(), position: pos() }))]));
    v
}

// ==================== views of a result ====================

/// A value without positions: `2`, `[2, 5]`, `N1[2, 3]`, `t:(2, 3)`, `{only: 1}`; a call or an
/// identifier that is still there shows as `call f` or `id x`.
fn show(v: &V) -> String {
    let list = |values: &[V]| values.iter().map(show).collect::<Vec<_>>().join(", ");
    match v {
        V::Integer { value, .. } => value.to_string(),
        V::Long { value, .. } => format!("{value}L"),
        V::Float { value, .. } => format!("{value}f"),
        V::Double { value, .. } => format!("{value}d"),
        V::String { value, .. } => format!("{value:?}"),
        V::Boolean { value, .. } => value.to_string(),
        V::Null { .. } => "null".to_string(),
        V::HexColor { value, .. } => value.clone(),
        V::Date { value, .. } => format!("D({value})"),
        V::Timestamp { value, .. } => format!("T({value})"),
        V::Array { values, .. } => format!("[{}]", list(values)),
        V::NestedArray { values, level, .. } => format!("N{level}[{}]", list(values)),
        V::Object { properties, .. } => {
            let props: Vec<String> = properties.iter().map(|p| format!("{}: {}", p.key, show(&p.value))).collect();
            format!("{{{}}}", props.join(", "))
        }
        V::PrefixedConstructor { prefix, arguments, .. } => format!("{prefix}:({})", list(arguments)),
        V::QuickFuncCall { function_name, .. } => format!("call {function_name}"),
        V::Identifier { value, .. } => format!("id {value}"),
        V::Expression { .. } => "expr".to_string(),
        other => format!("{other:?}"),
    }
}

/// Every value of the resolved data section in order, as `name = shape` lines.
fn shapes(outcome: &Outcome) -> Vec<String> {
    let data = outcome.resolved.as_ref().and_then(|r| r.data.as_ref()).expect("a data section");
    let mut lines = Vec::new();
    for entry in &data.entries {
        match entry {
            DataEntry::SimpleProperty { name, value, .. } => lines.push(format!("{name} = {}", show(value))),
            DataEntry::ObjectProperty { name, object, .. } => lines.push(format!("{name} = {}", show(object))),
            DataEntry::TableProperty { path, properties, .. } => {
                for p in properties {
                    lines.push(format!("{}.{} = {}", path.segments.join("."), p.name, show(&p.value)));
                }
            }
            DataEntry::GroupArray { path, items, .. } => {
                for (i, item) in items.iter().enumerate() {
                    lines.push(format!("{}[{i}] = {}", path.segments.join("."), show(item)));
                }
            }
        }
    }
    lines
}

// ==================== inputs ====================

enum Input {
    /// A source file: its path and text.
    Source(String, String),
    /// A hand-built data section, by case name.
    Hand(&'static str),
}

fn case_entries(case: &str) -> Vec<DataEntry> {
    hand3_cases().into_iter().find(|(n, _)| *n == case).unwrap_or_else(|| panic!("no case {case}")).1
}

fn inputs() -> Vec<(String, Input)> {
    let mut all = Vec::new();
    for (name, text) in sources3() {
        let path = fixture_dir().join(format!("{name}.mdix"));
        fs::write(&path, &text).expect("a source is written");
        all.push((name.to_string(), Input::Source(path.to_string_lossy().to_string(), text)));
    }
    for (name, _) in hand3_cases() {
        all.push((format!("hand3_{name}"), Input::Hand(name)));
    }
    all
}

fn run_input(input: &Input, run: Run) -> Outcome {
    match input {
        Input::Source(path, text) => resolve_source(path, text, run),
        Input::Hand(case) => resolve_hand3(case_entries(case), run),
    }
}

const THROUGH_DOCUMENT: Run = Run { through_document: true };
const PLAIN: Run = Run { through_document: false };

// ==================== the recording ====================

/// `(name, ok, fingerprint)` of every input, produced by running the original code.
const RECORDING: &[(&str, bool, u64)] = &[
    ("w01_types", true, 0x185571c1ec0a73db),
    ("w02_positions", true, 0xdd835edbe6335ca5),
    ("w03_containers", true, 0x6c3f77323edb147a),
    ("w04_chains", true, 0xbe6844e8a29865e0),
    ("w05_errors", false, 0x783cf5400ab2668d),
    ("w06_cycle", false, 0x31d16a665511eab2),
    ("w07_enums", true, 0x0d9dd0afe3d484f0),
    ("w08_dups", true, 0x0729f77ee5fab7da),
    ("w09_imported", true, 0x32e68135916df54c),
    ("w10_only_literals", true, 0x4fc40277c07cf6d6),
    ("hand3_w_simple", true, 0x0aa20113115c9c43),
    ("hand3_w_table", true, 0xa3af408dbb0e5354),
    ("hand3_w_group", true, 0x9b146a183d75212e),
    ("hand3_w_objprop", true, 0x970cb91177920d86),
    ("hand3_w_array", true, 0x960eeb088eb605ec),
    ("hand3_w_nested", true, 0x23367a64df0bce12),
    ("hand3_w_ctor", true, 0x0db38b95d29c149e),
    ("hand3_w_deep", true, 0xf1519dcbfd7b0118),
    ("hand3_w_types", true, 0x37e31501521dc60c),
    ("hand3_w_dup_array_call", true, 0xecd87bc8d2e52ed4),
    ("hand3_w_dup_fallback", true, 0xbfa44325e0721cc5),
    ("hand3_w_dup_stale_slot", true, 0x64c93c7943e29584),
    ("hand3_w_dup_fallback_everywhere", true, 0x22c5c861591955ad),
    ("hand3_w_dup_calls", true, 0x946c3e0df0f775f5),
    ("hand3_w_dup_earlier", true, 0x247423dca0027d65),
    ("hand3_w_expr", true, 0x298bf9203dc58fc1),
    ("hand3_w_fail", false, 0x0138863b54137091),
    ("hand3_w_chain", true, 0x79c73b90ef78083d),
    ("hand3_w_cycle", false, 0xf6fbc0e8619e10ff),
    ("hand3_p_ids", true, 0x5f8333ba201134fc),
    ("hand3_p_chain", true, 0xef9669c6070df919),
    ("hand3_p_paths", true, 0xec648236cf628544),
    ("hand3_p_types", true, 0xff966ec9dba4b890),
    ("hand3_p_literals", true, 0xcdaf4521dcdd4995),
    ("hand3_p_after_fail", false, 0x13ba7a0bb8b7ada7),
    ("hand3_p_expr_ident", true, 0xb0b36d569095e674),
];

/// The resolved data section of every hand-built case, one line per value, produced by running
/// the original code. The cases pin these behaviours of the write-back:
///
/// * a result lands in a simple property, a table property, a group item, an object property,
///   an array element, a nested array (which keeps its level), a constructor argument and any
///   depth of those;
/// * a node that shares its position with a call takes the call's result: the array holding it
///   (`w_dup_array_call`), an earlier plain node (`w_dup_earlier`), or the first of two calls
///   with one position (`w_dup_calls`);
/// * when an earlier replacement removed the node a path led to, the first node with the
///   position takes the result (`w_dup_fallback`);
/// * a call whose position no value has is not written back (`w_expr`);
/// * a failed call leaves the rest of the calls replaced and phase 5 unrun (`w_fail`,
///   `p_after_fail`);
/// * phase 5 turns an identifier named by a full path into the value, in every position, and
///   turns a nested array that held one into a plain array.
const SHAPES: &[(&str, &[&str])] = &[
    ("w_simple", &["a = 2", "b = 2", "c = 4"]),
    ("w_table", &["t.k = 2", "t.m = 5", "t.n = 4"]),
    ("w_group", &["rows[0] = 2", "rows[1] = 2", "rows[2] = 5", "rows[3] = {a: 7}"]),
    ("w_objprop", &["o = {x: 2, y: {z: 4}}"]),
    ("w_array", &["arr = [2, 2, 5]"]),
    ("w_nested", &["n1 = N1[2, 3]", "n2 = N1[N2[3], [5]]", "n3 = N1[1, 2]"]),
    ("w_ctor", &["c = t:(2, 2)", "d = t:(1)"]),
    ("w_deep", &["o = {a: [{b: t:(2)}, 4]}"]),
    ("w_types", &["i = 2", "l = 9000000001L", "fl = 1.5f", "d = 2.5d", "s = \"hi!\"", "b = true", "hx = #FF5733", "a = [1, 2, 3]", "t = t:(1, 2)", "o = {only: 1}", "dt = D(2025-12-31)", "ts = T(2025-12-31T23:59:59.000Z)", "bl = b:(\"QUJD\")", "re = r:(\"regex_pattern\")", "n = [[1], [2]]", "nl = null", "t.k = [2, 3, 4]", "t.m = {only: 3}", "rows[0] = [4, 5, 6]", "rows[1] = 6"]),
    ("w_dup_array_call", &["a = 2"]),
    ("w_dup_fallback", &["a = 2", "b = 4"]),
    ("w_dup_stale_slot", &["a = [1, 2, 3]"]),
    ("w_dup_fallback_everywhere", &["h1 = 2", "h2 = 2", "h3 = 2", "h4 = 2", "h5 = 2", "h6 = 2", "h7 = 2", "t1.k = 4", "g2[0] = 4", "o3 = 4", "c4 = t:(4)", "n5 = N1[4]", "o6 = {x: 4}", "p7 = 4"]),
    ("w_dup_calls", &["a = 4", "b = call g"]),
    ("w_dup_earlier", &["a = 2", "b = call f"]),
    ("w_expr", &["e1 = expr", "e2 = 5", "e3 = expr"]),
    ("w_fail", &["a = 2", "b = call boom", "c = 4", "t.k = 6"]),
    ("w_chain", &["a = 2", "b = 4", "c = 7", "t.k = 3", "t.m = 5"]),
    ("w_cycle", &["a = call f", "b = call g", "c = 4"]),
    ("p_ids", &["x = 2", "base = 5", "y = 2", "z = 5", "gone = id DATA.nothere", "arr = [2, 5, 3, id nope]", "nest = [2, 1]", "nest_none = N1[id nope]", "o = {p: 2, q: {r: 5}, s: id nope}", "c = t:(2, 2)", "t.k = 2", "t.m = 2", "t.n = 2", "rows[0] = 2", "rows[1] = 2", "rows[2] = {a: 5}"]),
    ("p_chain", &["x = 2", "w = 2", "z = 2", "y = 2"]),
    ("p_paths", &["x = 2", "arr = [2]", "r_arr = 2", "o = {k: 2, deep: {a: [2]}}", "r_o = 2", "r_deep = 2", "c = t:(2)", "r_c = 2", "ca = [t:(2)]", "r_ca = 2", "n = [2]", "r_n = 2", "t.k = 2", "r_t = 2", "rows[0] = 2", "r_rows = 2", "a.b.k = 2", "r_ab = 2", "g.h[0] = 2", "r_gh = 2", "t2.a = 2", "t2.b = 2", "r_t2a = 2", "r_t2b = 2"]),
    ("p_types", &["i = 2", "l = 9000000001L", "fl = 1.5f", "d = 2.5d", "s = \"hi!\"", "b = true", "hx = #FF5733", "a = [1, 2, 3]", "t = t:(1, 2)", "o = {only: 1}", "dt = D(2025-12-31)", "ts = T(2025-12-31T23:59:59.000Z)", "bl = b:(\"QUJD\")", "re = r:(\"regex_pattern\")", "n = [[1], [2]]", "nl = null", "ri = 2", "rl = 9000000001L", "rfl = 1.5f", "rd = 2.5d", "rs = \"hi!\"", "rb = true", "rhx = #FF5733", "ra = [1, 2, 3]", "rt = t:(1, 2)", "ro = {only: 1}", "rdt = D(2025-12-31)", "rts = T(2025-12-31T23:59:59.000Z)", "rbl = b:(\"QUJD\")", "rre = r:(\"regex_pattern\")", "rn = [[1], [2]]", "rnl = null", "ra0 = id DATA.a[0]", "ro_only = id DATA.o.only"]),
    ("p_literals", &["base = 5", "s = \"text\"", "arr = [1, 2]", "y = 5", "ys = \"text\"", "ya = [1, 2]", "y1 = 2", "nest = [5]", "none = id DATA.none"]),
    ("p_after_fail", &["x = 2", "b = call boom", "y = id DATA.x"]),
    ("p_expr_ident", &["x = 2", "e = expr"]),
];

#[test]
fn the_recording_holds_through_the_document_and_without_it() {
    let table: BTreeMap<&str, (bool, u64)> = RECORDING.iter().map(|(n, ok, fp)| (*n, (*ok, *fp))).collect();
    let all = inputs();
    assert_eq!(table.len(), all.len(), "the recording and the inputs are out of step");
    for (name, input) in &all {
        let (ok, fp) = *table.get(name.as_str()).unwrap_or_else(|| panic!("{name} is not recorded"));
        for run in [THROUGH_DOCUMENT, PLAIN] {
            let outcome = run_input(input, run);
            assert_eq!(outcome.ok, ok, "{name}, document attached: {}", run.through_document);
            assert_eq!(
                fingerprint(&outcome),
                fp,
                "{name}, document attached: {} (print_recording regenerates the table from the original code)",
                run.through_document
            );
        }
    }
}

#[test]
fn the_resolved_data_has_the_recorded_shape() {
    let table: BTreeMap<&str, &[&str]> = SHAPES.iter().map(|(n, s)| (*n, *s)).collect();
    let cases = hand3_cases();
    assert_eq!(table.len(), cases.len(), "the shapes and the cases are out of step");
    for (case, _) in cases {
        let want = table.get(case).unwrap_or_else(|| panic!("{case} has no shape"));
        for run in [THROUGH_DOCUMENT, PLAIN] {
            let outcome = resolve_hand3(case_entries(case), run);
            assert_eq!(&shapes(&outcome), want, "{case}, document attached: {}", run.through_document);
        }
    }
}

#[test]
#[ignore = "prints the recording; run on the original code with --ignored --nocapture"]
fn print_recording() {
    println!();
    for (name, input) in inputs() {
        let outcome = run_input(&input, PLAIN);
        println!("    ({name:?}, {}, 0x{:016x}),", outcome.ok, fingerprint(&outcome));
    }
}

#[test]
#[ignore = "prints the shapes; run on the original code with --ignored --nocapture"]
fn print_shapes() {
    println!();
    for (case, _) in hand3_cases() {
        let outcome = resolve_hand3(case_entries(case), PLAIN);
        let lines: Vec<String> = shapes(&outcome).iter().map(|s| format!("{s:?}")).collect();
        println!("    ({case:?}, &[{}]),", lines.join(", "));
    }
}

// ==================== phase 5: passes and log ====================

/// `x` is a call and each of the 120 entries after it is an identifier naming the one before it,
/// more links than the passes phase 5 may make.
fn long_chain() -> Vec<DataEntry> {
    POS.with(|p| p.set(8000));
    let mut entries = vec![simple("x", qfv("f", vec![ie(1)]))];
    for i in 0..120 {
        let target = if i == 0 { "DATA.x".to_string() } else { format!("DATA.c{}", i - 1) };
        entries.push(simple(&format!("c{i}"), ident(&target)));
    }
    entries
}

/// The lines phase 5 logs with verbose logging on, without colour codes or times.
fn phase_five_log(entries: Vec<DataEntry>) -> Vec<String> {
    let _guard = registry_lock();
    let mut prepared = prepared_with3(entries);
    let em = ErrorManager::new_isolated();
    let mut verbose = prepared.settings.clone();
    verbose.debug_mode = DebugMode::Verbose;
    em.update_settings(verbose);
    let resolver = ValueResolver::new_with_error_manager(prepared.enhanced, &prepared.symbols, DebugMode::Verbose, em.clone());
    let _ = with_doc(resolver, &mut prepared.document).resolve();
    em.get_log_contents()
        .lines()
        .filter_map(|line| {
            let at = line.find("[Phase 5").or_else(|| line.find("Identifier resolution"))?;
            Some(line[at..].split('\x1b').next().unwrap_or("").to_string())
        })
        .collect()
}

/// The phase 5 log of some of the cases, produced by running the original code.
const PHASE_FIVE_LOG: &[(&str, &[&str])] = &[
    ("p_ids", &["[Phase 5, pass 1/18] resolved 11, 6 pending", "[Phase 5, pass 2/18] resolved 2, 4 pending", "[Phase 5, pass 3/18] resolved 0, 4 pending", "Identifier resolution: 13 resolved, 4 external/runtime"]),
    ("p_chain", &["[Phase 5, pass 1/8] resolved 1, 2 pending", "[Phase 5, pass 2/8] resolved 1, 1 pending", "[Phase 5, pass 3/8] resolved 1, 0 pending", "[Phase 5, pass 4/8] resolved 0, 0 pending", "Identifier resolution: 3 resolved, 0 external/runtime"]),
    ("p_paths", &["[Phase 5, pass 1/34] resolved 12, 12 pending", "[Phase 5, pass 2/34] resolved 12, 0 pending", "[Phase 5, pass 3/34] resolved 0, 0 pending", "Identifier resolution: 24 resolved, 0 external/runtime"]),
    ("p_literals", &["[Phase 5, pass 1/13] resolved 5, 1 pending", "[Phase 5, pass 2/13] resolved 0, 1 pending", "Identifier resolution: 5 resolved, 1 external/runtime"]),
    ("p_after_fail", &[]),
    ("p_types", &["[Phase 5, pass 1/51] resolved 16, 2 pending", "[Phase 5, pass 2/51] resolved 0, 2 pending", "Identifier resolution: 16 resolved, 2 external/runtime"]),
];

#[test]
fn phase_five_logs_each_pass_as_it_always_did() {
    for (case, want) in PHASE_FIVE_LOG {
        assert_eq!(&phase_five_log(case_entries(case)), want, "{case}");
    }
}

#[test]
fn a_chain_longer_than_the_passes_is_cut_off() {
    let outcome = resolve_hand3(long_chain(), THROUGH_DOCUMENT);
    assert!(outcome.ok, "{:?}", outcome.errors);
    let left = shapes(&outcome).iter().filter(|line| line.contains("= id ")).count();
    assert_eq!(left, CHAIN_LEFT, "identifiers left in a chain of 120 (print_phase_five regenerates it)");
    let log = phase_five_log(long_chain());
    assert_eq!(log.last().map(String::as_str), Some(CHAIN_LAST_LOG), "{log:?}");
    assert_eq!(log.len(), CHAIN_LOG_LINES, "{log:?}");
}

/// Identifiers left, last log line and number of log lines for `long_chain`, from the original.
const CHAIN_LEFT: usize = 56;
const CHAIN_LAST_LOG: &str = "Identifier resolution: 64 resolved, 56 external/runtime";
const CHAIN_LOG_LINES: usize = 65;

#[test]
#[ignore = "prints the phase 5 tables; run on the original code with --ignored --nocapture"]
fn print_phase_five() {
    println!();
    for case in ["p_ids", "p_chain", "p_paths", "p_literals", "p_after_fail", "p_types"] {
        let lines: Vec<String> = phase_five_log(case_entries(case)).iter().map(|l| format!("{l:?}")).collect();
        println!("    ({case:?}, &[{}]),", lines.join(", "));
    }
    let outcome = resolve_hand3(long_chain(), PLAIN);
    let left = shapes(&outcome).iter().filter(|line| line.contains("= id ")).count();
    let log = phase_five_log(long_chain());
    println!("CHAIN_LEFT {left}");
    println!("CHAIN_LAST {}", log.last().cloned().unwrap_or_default());
    println!("CHAIN_LINES {}", log.len());
}

// ==================== which tree the result is built from ====================

/// A resolver given an owned tree and a document that disagree (the loader never does this).
fn disagreeing(document_entries: Vec<DataEntry>, owned_entries: Vec<DataEntry>) -> Outcome {
    let _guard = registry_lock();
    let mut prepared = prepared_with3(document_entries);
    prepared.enhanced.data = Some(DataSection::new(owned_entries, Position { line: 1, column: 1 }));
    resolve(prepared.enhanced, &prepared.symbols, &prepared.settings, Some(prepared.document))
}

#[test]
fn a_run_that_writes_nothing_back_leaves_the_owned_data_alone() {
    POS.with(|p| p.set(7000));
    let outcome = disagreeing(vec![simple("d", int(2))], vec![simple("o", int(1))]);
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert_eq!(shapes(&outcome), ["o = 1"]);
}

#[test]
fn a_call_with_no_position_in_the_tree_leaves_the_owned_data_alone() {
    POS.with(|p| p.set(7100));
    let outcome = disagreeing(vec![simple("e", vexp(eqf("f", vec![ie(2)])))], vec![simple("o", int(1))]);
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert_eq!(outcome.history.len(), 1, "the call is made");
    assert_eq!(shapes(&outcome), ["o = 1"]);
}

#[test]
fn phase_five_reads_and_rewrites_the_document() {
    POS.with(|p| p.set(7200));
    let document = vec![simple("x", qfv("f", vec![ie(1)])), simple("y", ident("DATA.x"))];
    let outcome = disagreeing(document, vec![simple("o", int(1))]);
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert_eq!(shapes(&outcome), ["x = 2", "y = 2"]);
}

#[test]
fn a_resolved_identifier_copies_its_path_and_shares_the_rest() {
    let _guard = registry_lock();
    POS.with(|p| p.set(9000));
    let entries = vec![
        simple("x", qfv("f", vec![ie(1)])),
        simple("keep", int(5)),
        simple("arr", arr(vec![int(1), arr(vec![ident("DATA.x"), int(3)]), int(4)])),
        tbl(&["t"], vec![("a", int(1)), ("b", ident("DATA.x")), ("c", int(2))]),
        grp(&["rows"], vec![int(7), ident("DATA.x"), int(8)]),
    ];
    let prepared = prepared_with3(entries);
    let before = &prepared.document;
    let keep = simple_value(before, "keep");
    let arr_outer = simple_value(before, "arr");
    let arr_items = children(before, arr_outer);
    let arr_inner_items = children(before, arr_items[1]);
    let rows = group_items(before, "rows");
    let table = table_values(before, "t");
    let sizes = arena_sizes(before);

    let outcome = resolve(prepared.enhanced, &prepared.symbols, &prepared.settings, Some(prepared.document));
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert_eq!(
        shapes(&outcome),
        ["x = 2", "keep = 5", "arr = [1, [2, 3], 4]", "t.a = 1", "t.b = 2", "t.c = 2", "rows[0] = 7", "rows[1] = 2", "rows[2] = 8"]
    );
    let after = outcome.document.as_ref().expect("the document comes back");

    assert_eq!(simple_value(after, "keep"), keep);
    let new_outer = simple_value(after, "arr");
    assert_ne!(new_outer, arr_outer);
    let new_items = children(after, new_outer);
    assert_eq!((new_items[0], new_items[2]), (arr_items[0], arr_items[2]));
    assert_ne!(new_items[1], arr_items[1]);
    let new_inner = children(after, new_items[1]);
    assert_ne!(new_inner[0], arr_inner_items[0]);
    assert_eq!(new_inner[1], arr_inner_items[1]);
    let new_rows = group_items(after, "rows");
    assert_eq!((new_rows[0], new_rows[2]), (rows[0], rows[2]));
    assert_ne!(new_rows[1], rows[1]);
    let new_table = table_values(after, "t");
    assert_eq!((new_table[0], new_table[2]), (table[0], table[2]));
    assert_ne!(new_table[1], table[1]);

    // The call's result, three identifiers resolved to it, and the two arrays on the way to one.
    let grown = arena_sizes(after);
    assert_eq!((grown.0 - sizes.0, grown.1), (6, sizes.1));
}

#[test]
fn a_run_that_resolves_nothing_allocates_nothing() {
    let _guard = registry_lock();
    POS.with(|p| p.set(9500));
    let entries = vec![
        simple("arr", arr(vec![ident("nope"), int(1), arr(vec![ident("gone")])])),
        objprop("o", obj(vec![("k", ident("none"))])),
        simple("c", ctor("t", vec![ident("no")])),
        tbl(&["t"], vec![("k", ident("nothing"))]),
        grp(&["rows"], vec![ident("zip")]),
    ];
    let prepared = prepared_with3(entries);
    let sizes = arena_sizes(&prepared.document);
    let outcome = resolve(prepared.enhanced, &prepared.symbols, &prepared.settings, Some(prepared.document));
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert_eq!(shapes(&outcome).iter().filter(|l| l.contains("id ")).count(), 5, "all five entries still hold an identifier");
    assert_eq!(arena_sizes(outcome.document.as_ref().expect("the document comes back")), sizes);
}

// ==================== writing along the path ====================

fn data_ids(document: &Document) -> &[DocDataEntry] {
    &document.data.as_ref().expect("the document has a data section").entries
}

fn simple_value(document: &Document, wanted: &str) -> ValueId {
    data_ids(document)
        .iter()
        .find_map(|e| match e {
            DocDataEntry::SimpleProperty { name, value, .. } if name == wanted => Some(*value),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no property {wanted}"))
}

fn group_items(document: &Document, wanted: &str) -> Vec<ValueId> {
    data_ids(document)
        .iter()
        .find_map(|e| match e {
            DocDataEntry::GroupArray { path, items, .. } if path.segments.last().map(String::as_str) == Some(wanted) => {
                Some(document.arena.value_list(*items).to_vec())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no group {wanted}"))
}

fn table_values(document: &Document, wanted: &str) -> Vec<ValueId> {
    data_ids(document)
        .iter()
        .find_map(|e| match e {
            DocDataEntry::TableProperty { path, properties, .. } if path.segments.last().map(String::as_str) == Some(wanted) => {
                Some(properties.iter().map(|p| p.value).collect())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no table {wanted}"))
}

fn children(document: &Document, id: ValueId) -> Vec<ValueId> {
    match *document.arena.value(id) {
        ValueNode::Array { values, .. } | ValueNode::NestedArray { values, .. } => document.arena.value_list(values).to_vec(),
        ref other => panic!("not an array: {other:?}"),
    }
}

fn arena_sizes(document: &Document) -> (usize, usize, usize) {
    (document.arena.value_count(), document.arena.expr_count(), document.arena.used_bytes())
}

fn path_entries() -> Vec<DataEntry> {
    POS.with(|p| p.set(5000));
    let call = |n: &str, a: i32| qfv(n, vec![ie(a)]);
    vec![
        simple("keep", int(5)),
        simple("arr", arr(vec![int(1), arr(vec![call("f", 1), int(3)]), int(4)])),
        grp(&["rows"], vec![int(7), call("g", 1), int(8)]),
        tbl(&["t"], vec![("a", int(1)), ("b", call("h", 1)), ("c", int(2))]),
    ]
}

#[test]
fn a_replacement_copies_its_path_and_shares_the_rest() {
    let _guard = registry_lock();
    let prepared = prepared_with3(path_entries());
    let before = &prepared.document;
    let keep = simple_value(before, "keep");
    let arr_outer = simple_value(before, "arr");
    let arr_items = children(before, arr_outer);
    let arr_inner_items = children(before, arr_items[1]);
    let rows = group_items(before, "rows");
    let table = table_values(before, "t");
    let sizes = arena_sizes(before);

    let outcome = resolve(prepared.enhanced, &prepared.symbols, &prepared.settings, Some(prepared.document));
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert_eq!(shapes(&outcome), ["keep = 5", "arr = [1, [2, 3], 4]", "rows[0] = 7", "rows[1] = 3", "rows[2] = 8", "t.a = 1", "t.b = 4", "t.c = 2"]);
    let after = outcome.document.as_ref().expect("the document comes back");

    // Nothing on a path is shared, nothing off it is copied.
    assert_eq!(simple_value(after, "keep"), keep);
    let new_outer = simple_value(after, "arr");
    assert_ne!(new_outer, arr_outer);
    let new_items = children(after, new_outer);
    assert_eq!(new_items[0], arr_items[0]);
    assert_ne!(new_items[1], arr_items[1]);
    assert_eq!(new_items[2], arr_items[2]);
    let new_inner = children(after, new_items[1]);
    assert_ne!(new_inner[0], arr_inner_items[0]);
    assert_eq!(new_inner[1], arr_inner_items[1]);
    let new_rows = group_items(after, "rows");
    assert_eq!((new_rows[0], new_rows[2]), (rows[0], rows[2]));
    assert_ne!(new_rows[1], rows[1]);
    let new_table = table_values(after, "t");
    assert_eq!((new_table[0], new_table[2]), (table[0], table[2]));
    assert_ne!(new_table[1], table[1]);

    // Three results and the two arrays on the way to the first: five nodes, and no expression.
    let grown = arena_sizes(after);
    assert_eq!((grown.0 - sizes.0, grown.1), (5, sizes.1));
}

#[test]
fn a_call_with_no_position_in_the_tree_allocates_nothing() {
    let _guard = registry_lock();
    POS.with(|p| p.set(6000));
    let entries = vec![simple("keep", int(5)), simple("e", vexp(eqf("f", vec![ie(2)])))];
    let prepared = prepared_with3(entries);
    let sizes = arena_sizes(&prepared.document);
    let outcome = resolve(prepared.enhanced, &prepared.symbols, &prepared.settings, Some(prepared.document));
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert_eq!(outcome.history.len(), 1, "the call is made");
    assert_eq!(shapes(&outcome), ["keep = 5", "e = expr"]);
    assert_eq!(arena_sizes(outcome.document.as_ref().expect("the document comes back")), sizes);
}

#[test]
fn the_document_holds_the_result_afterwards() {
    let _guard = registry_lock();
    let prepared = prepared_with3(path_entries());
    let outcome = resolve(prepared.enhanced, &prepared.symbols, &prepared.settings, Some(prepared.document));
    let document = outcome.document.as_ref().expect("the document comes back");
    let held = document.data.as_ref().expect("a data section").export(&document.arena);
    let resolved = outcome.resolved.as_ref().and_then(|r| r.data.as_ref()).expect("a data section");
    assert_eq!(format!("{held:?}"), format!("{resolved:?}"));
}
