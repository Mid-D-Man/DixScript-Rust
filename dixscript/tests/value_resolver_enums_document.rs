// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/value_resolver_enums_document.rs"
// ============================================================================
//! Tests for enum pre-resolution (the first phase of value resolution) over a `Document`.
//!
//! The phase checks every enum reference in `@DATA` against the symbol table and turns the ones
//! inside expressions into integers. It now runs on the arena: `ValueResolver::with_document`
//! points it at a `Document`, and without one it copies the owned data section into a scratch
//! arena and walks that, so there is one implementation of the walk. The tests check six things:
//!
//! * the resolved tree of every crafted source and of two corpus files is the one the owned
//!   walker produced before it was removed (the `RECORDING` table, a fingerprint of the result
//!   produced by running the original code), through the document and through the owned entry
//!   point;
//! * the same for data sections built by hand, which reach what the grammar cannot write:
//!   every expression form, the forms that are not entered, nested arrays of every depth, and
//!   a bad reference in every position (`HAND_RECORDING`);
//! * the messages of the errors are exactly the original ones, for every way a reference can be
//!   wrong;
//! * the phase reads the `Document` when it has one and the owned tree when it does not;
//! * data without a change allocates nothing in the document's arena, and data with one leaves
//!   the document holding the resolved section;
//! * the rules that are easy to lose in a rewrite: a nested array that held an enum comes back as
//!   a plain array, and an imported enum is merged into `@ENUMS` only through the dotted value
//!   form.

use std::cell::Cell;
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

/// What a run of the resolver left behind.
struct Outcome {
    ok: bool,
    errors: Vec<String>,
    resolved: Option<DixScript>,
    /// The document the resolver was given, after the run (`None` for a run without one).
    document: Option<Document>,
}

/// Resolves `enhanced`, with `document` attached when there is one. The resolver reads the
/// enum registry, so the caller holds its lock.
fn resolve(
    enhanced: DixScript,
    symbols: &SymbolTable,
    settings: &OperationalSettings,
    document: Option<Document>,
) -> Outcome {
    let em = ErrorManager::new_isolated_silent();
    let mut document = document;
    let mut resolver =
        ValueResolver::new_with_error_manager(enhanced, symbols, settings.debug_mode, em);
    if let Some(doc) = document.as_mut() {
        resolver = with_doc(resolver, doc);
    }
    let result = resolver.resolve();
    Outcome { ok: result.is_success, errors: result.errors, resolved: result.resolved_ast, document }
}

fn registry_lock() -> std::sync::MutexGuard<'static, ()> {
    dixscript::Builtins::Static::enum_object::ENUM_REGISTRY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Resolves the source file at `path` whose text is `source`.
fn resolve_source(path: &str, source: &str, through_document: bool) -> Outcome {
    let _guard = registry_lock();
    let prepared = prepare(path, source.as_bytes());
    let document = through_document.then_some(prepared.document);
    resolve(prepared.enhanced, &prepared.symbols, &prepared.settings, document)
}

/// The fingerprint of an outcome: the verdict, the errors and the whole resolved tree, with the
/// order of the `@CONFIG` entries (a hash map) fixed and the times masked.
fn fingerprint(outcome: &Outcome) -> u64 {
    let mut resolved = outcome.resolved.clone();
    if let Some(config) = resolved.as_mut().and_then(|r| r.config.as_mut()) {
        config.entries.sort_by(|a, b| a.key.cmp(&b.key));
    }
    fnv(&mask_times(&format!("{}|{:?}|{:?}", outcome.ok, outcome.errors, resolved)))
}

fn data_of(outcome: &Outcome) -> &[DataEntry] {
    &outcome
        .resolved
        .as_ref()
        .expect("a resolved tree")
        .data
        .as_ref()
        .expect("a data section")
        .entries
}

fn value_of<'a>(outcome: &'a Outcome, name: &str) -> &'a V {
    data_of(outcome)
        .iter()
        .find_map(|entry| match entry {
            DataEntry::SimpleProperty { name: n, value, .. } if n == name => Some(value),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no property {name}"))
}

// ==================== crafted sources ====================

const HEADER: &str = "@CONFIG(\n  version -> \"1.0.0\"\n  features -> \"advanced\"\n)\n\n";
const ENUMS: &str = "@ENUMS(\n  Color { RED = 1, GREEN = 2, BLUE = 3 }\n  Level { LOW = 1, MID = 5, HIGH = 10 }\n)\n\n";
const IMPORTS: &str = "@IMPORTS(\n  Base from \"base_types.mdix\"\n  Types from \"enum_only_types.mdix\"\n)\n\n";

/// The crafted sources, `(name, text)`. Each is written to the fixture directory so its imports
/// resolve. None returns an object from a function or calls a random or clock builtin, whose
/// output could not be recorded.
fn sources() -> Vec<(&'static str, String)> {
    let h = HEADER;
    let e = ENUMS;
    let i = IMPORTS;
    let mut v: Vec<(&'static str, String)> = Vec::new();
    v.push(("c01_positions", format!("{h}{e}@DATA(\n  plain<enum> = Color.RED\n  untyped = Level.HIGH\n  list = [Color.RED, Color.GREEN]\n  only_enum_list = [Color.BLUE]\n  grid = [[Color.RED, Color.GREEN], [Color.BLUE]]\n  plain_grid = [[1, 2], [3, 4]]\n  mixed_grid = [[Level.LOW], [Level.MID, Level.HIGH]]\n  deep = [[[Color.RED]], [[1]]]\n  obj = {{ c = Color.GREEN, nested = {{ l = Level.LOW }}, arr = [Level.HIGH], plain = 1 }}\n  tup = t:(Color.RED, 1)\n  rows::\n    {{ a = Color.RED, b = [Level.LOW] }},\n    {{ a = Color.BLUE, b = [Level.HIGH] }}\n  grp.key:\n    x = Level.MID,\n    y = [Color.GREEN]\n)\n")));
    v.push(("c03_calls", format!("{h}{e}@QUICKFUNCS(\n  ~lv<int> => global(l<enum>) {{ return l + 1 }}\n  ~two<int> => global(a<enum>, b<int>) {{ return a + b }}\n  ~arr<int> => global(xs<array>) {{ return xs.sum() }}\n)\n\n@DATA(\n  a = lv(Level.HIGH)\n  b = two(Level.LOW, 3)\n  c = two(Level.MID, Level.HIGH)\n  d = arr([Level.LOW, Level.MID])\n  e = lv(lv(Level.LOW))\n  h = arr([[Level.LOW], [Level.HIGH]])\n  rows::\n    lv(Level.LOW),\n    two(Color.GREEN, 4)\n)\n")));
    v.push(("c04_imported", format!("{h}{i}@ENUMS(\n  Slot {{ HEAD = 1, CHEST = 2 }}\n)\n\n@DATA(\n  r<enum> = Base.Rarity.EPIC\n  t = Types.Priority.HIGH\n  both = [Base.ItemType.WEAPON, Slot.HEAD, Types.Region.EU]\n  o = {{ r = Base.Rarity.RARE, s = Slot.CHEST }}\n  grid = [[Base.Rarity.COMMON], [Types.Region.NA]]\n  rows::\n    {{ r = Base.Rarity.COMMON, p = Types.Priority.LOW }},\n    {{ r = Base.Rarity.LEGENDARY, p = Types.Priority.CRITICAL }}\n)\n")));
    v.push(("c05_imported_no_local_enums", format!("{h}{i}@DATA(\n  r = Base.Rarity.EPIC\n  p = Types.Priority.CRITICAL\n  list = [Types.Region.APAC, Types.Region.LATAM]\n)\n")));
    v.push(("c06_imported_one_only", format!("{h}@IMPORTS(\n  Base from \"base_types.mdix\"\n)\n\n{e}@DATA(\n  local = Color.RED\n  imp = Base.ItemType.QUEST\n  scaled = Base.scaledValue(10, Base.Rarity.RARE)\n)\n")));
    v.push(("c07_no_enums", format!("{h}@DATA(\n  n = 1\n  s = \"x\"\n  arr = [1, 2, 3]\n  grid = [[1, 2], [3, 4]]\n  obj = {{ a = 1, b = [1, 2], c = {{ d = 2 }} }}\n  rows::\n    {{ a = 1, b = \"x\" }},\n    {{ a = 2, b = \"y\" }}\n)\n")));
    v.push(("c08_enum_only_in_function", format!("{h}{e}@QUICKFUNCS(\n  ~pick<int> => global(l<enum>) {{ return l == Level.HIGH ? Color.RED : Color.BLUE }}\n)\n\n@DATA(\n  v = pick(Level.HIGH)\n  plain = 3\n  list = [[1], [2]]\n)\n")));
    v.push(("c09_nested_enum_only", format!("{h}{e}@DATA(\n  a = [[Color.RED]]\n  b = [[1], [Color.RED]]\n  c = [[[Color.RED]]]\n  d = [[Color.RED], [Color.GREEN]]\n  e = [[1, 2], [3, 4], [Level.LOW]]\n)\n")));
    v.push(("c10_group_arrays", format!("{h}{e}@DATA(\n  top = Color.BLUE\n  tbl::\n    {{ c = Color.RED, d = 1 }},\n    {{ c = Color.GREEN, d = 2 }},\n    {{ c = Color.BLUE, d = 3 }}\n  more.group:\n    a = [Level.LOW, Level.MID],\n    b = {{ z = Level.HIGH }},\n    c = [[Level.LOW]]\n)\n")));
    v.push(("c11_error_unknown_enum", format!("{h}{e}@DATA(\n  ok = Color.RED\n  bad = Nope.X\n)\n")));
    v.push(("c12_error_unknown_field", format!("{h}{e}@DATA(\n  ok = Color.RED\n  bad = [Level.NOPE]\n)\n")));
    v.push(("c13_error_imported", format!("{h}{i}@DATA(\n  ok = Base.Rarity.EPIC\n  bad = Base.Rarity.NOPE\n)\n")));
    v.push(("c14_error_late", format!("{h}{e}@DATA(\n  first = [Color.RED]\n  second = {{ a = Color.GREEN, b = [[Level.LOW]] }}\n  rows::\n    {{ c = Color.RED }},\n    {{ c = Color.PURPLE }}\n)\n")));
    v
}

/// The corpus files the recording includes, relative to the crate directory.
const CORPUS: &[&str] = &[
    "../mdix_files/intermediate/EnumUsage.mdix",
    "../mdix_files/tests/imports/11_enum_only_import_no_funcs.mdix",
];

/// `(name, ok, fingerprint)` of every crafted source and corpus file, produced by running the
/// original owned-tree resolver. Regenerate with `print_recording` after a deliberate change to
/// a source, a corpus file or the resolver's output.
const RECORDING: &[(&str, bool, u64)] = &[
("c01_positions", true, 0x21317d1629be1861),
    ("c03_calls", true, 0x4bd1fbac9f5cce58),
    ("c04_imported", true, 0xe0fe655748f215ca),
    ("c05_imported_no_local_enums", true, 0x0dca00d3193938f0),
    ("c06_imported_one_only", true, 0xa7829ea3340c326f),
    ("c07_no_enums", true, 0x4964a9aa31199153),
    ("c08_enum_only_in_function", true, 0x64c3b66abfd459a1),
    ("c09_nested_enum_only", true, 0x2f96efa7a38493ef),
    ("c10_group_arrays", true, 0x3cad8dbc49038923),
    ("c11_error_unknown_enum", false, 0x1b27c2b50c28297b),
    ("c12_error_unknown_field", false, 0xcc1fdb68b04e025d),
    ("c13_error_imported", false, 0xf1715c3ffc57e002),
    ("c14_error_late", false, 0x7071addff4376008),
    ("../mdix_files/intermediate/EnumUsage.mdix", true, 0x90bc335304677fb1),
    ("../mdix_files/tests/imports/11_enum_only_import_no_funcs.mdix", true, 0x953d849cabea9afb),
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

#[test]
#[ignore]
fn print_recording() {
    for (name, path, text) in all_inputs() {
        let outcome = resolve_source(&path, &text, false);
        println!("    (\"{}\", {}, 0x{:016x}),", name, outcome.ok, fingerprint(&outcome));
    }
    for (base, case, ok, hash) in hand_runs(false) {
        println!("    (\"{}\", \"{}\", {}, 0x{:016x}),", base, case, ok, hash);
    }
}

fn check_recording(through_document: bool) {
    let inputs = all_inputs();
    assert_eq!(inputs.len(), RECORDING.len(), "RECORDING is out of date; run print_recording");
    for ((name, path, text), (rec_name, rec_ok, rec_hash)) in inputs.iter().zip(RECORDING) {
        assert_eq!(name, rec_name);
        let outcome = resolve_source(path, text, through_document);
        assert_eq!(outcome.ok, *rec_ok, "{name}: verdict");
        assert_eq!(fingerprint(&outcome), *rec_hash, "{name}: resolved tree (document: {through_document})");
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

// ==================== hand-built data sections ====================

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

/// The hand-built data sections, `(name, entries)`. Each is run in place of the data section of
/// the two bases below.
fn hand_cases() -> Vec<(&'static str, Vec<DataEntry>)> {
    POS.with(|p| p.set(100));
    let b = Some("Base");
    let mut v: Vec<(&'static str, Vec<DataEntry>)> = Vec::new();
    v.push(("empty", vec![]));
    v.push(("ops", vec![
        simple("arith", vexp(bin(0, ea(None, "Level", "LOW"), "+", ea(b, "Rarity", "EPIC")))),
        simple("bit", vexp(bin(1, ea(None, "Level", "LOW"), "|", ie(4)))),
        simple("cmp", vexp(bin(2, ea(None, "Color", "RED"), "==", ea(None, "Color", "RED")))),
        simple("logic", vexp(bin(3, ea(None, "Level", "MID"), "&&", ea(None, "Level", "HIGH")))),
        simple("unary", vexp(E::UnaryOp { operator: "-".into(), operand: bx(ea(None, "Level", "MID")), position: pos() })),
        simple("noenum", vexp(bin(0, ie(1), "+", ie(2)))),
    ]));
    v.push(("cond_paren_access", vec![
        simple("cond", vexp(E::Conditional {
            condition: bx(bin(2, ea(None, "Level", "HIGH"), ">", ie(5))),
            true_value: bx(ea(None, "Color", "RED")),
            false_value: bx(ea(None, "Color", "BLUE")),
            position: pos(),
        })),
        simple("paren", vexp(E::Parenthesized { expression: bx(ea(None, "Level", "LOW")), position: pos() })),
        simple("prop", vexp(E::PropertyAccess { object: bx(ea(None, "Level", "LOW")), property: "x".into(), position: pos() })),
        simple("idx", vexp(E::IndexAccess { object: bx(ea(None, "Level", "LOW")), index: bx(ea(b, "Rarity", "RARE")), position: pos() })),
        simple("valwrap", vexp(E::Value { value: ev("Level", "HIGH"), position: pos() })),
        simple("valwrap_arr", vexp(E::Value { value: arr(vec![ev("Level", "HIGH")]), position: pos() })),
    ]));
    v.push(("calls_and_terminals", vec![
        simple("qf", vexp(E::QuickFuncCall { name: "f".into(), arguments: vec![ea(None, "Color", "GREEN"), ie(2)], position: pos() })),
        simple("imp", vexp(E::ImportedFunctionCall { namespace_name: "Base".into(), function_name: "scaledValue".into(), arguments: vec![ie(1), ea(b, "Rarity", "COMMON")], position: pos() })),
        simple("stat", vexp(E::StaticMethodCall { object_name: "Math".into(), method_name: "max".into(), arguments: vec![ea(None, "Level", "LOW"), ea(None, "Level", "HIGH")], position: pos() })),
        simple("qfv", V::QuickFuncCall { function_name: "f".into(), arguments: vec![ea(None, "Color", "BLUE"), ea(b, "Rarity", "EPIC")], position: pos() }),
        simple("qfv_plain", V::QuickFuncCall { function_name: "f".into(), arguments: vec![ie(1)], position: pos() }),
    ]));
    v.push(("terminal_with_bad_enum", vec![
        simple("inst", vexp(E::InstanceMethodCall { instance: bx(ea(None, "Nope", "X")), method_name: "m".into(), arguments: vec![ea(None, "Nope", "X")], position: pos() })),
        simple("cast", vexp(E::TypeCast { expression: bx(ea(None, "Nope", "X")), target_type: DataType::Int, position: pos() })),
        simple("builtin", vexp(E::BuiltinFunction { target: bx(ea(None, "Nope", "X")), method: "m".into(), arguments: Some(vec![ea(None, "Nope", "X")]), position: pos() })),
        simple("fncall", vexp(E::FunctionCall { name: "g".into(), arguments: vec![ea(None, "Nope", "X")], position: pos() })),
        simple("dix", vexp(E::DixFunctionCall { function_name: "d".into(), arguments: vec![ea(None, "Nope", "X")], position: pos() })),
        simple("staticfn", vexp(E::StaticFunction { class_name: "C".into(), method: "m".into(), arguments: vec![ea(None, "Nope", "X")], position: pos() })),
        simple("qual", vexp(E::QualifiedIdentifier { parts: vec!["Nope".into(), "X".into()], arguments: None, position: pos() })),
        simple("ident", vexp(E::Identifier { name: "Nope".into(), position: pos() })),
        simple("cfg", vexp(E::ConfigAccess { key: "k".into(), position: pos() })),
        simple("objacc", vexp(E::ObjectAccess { path: vec!["a".into()], position: pos() })),
        simple("lambda", V::Lambda { parameters: vec!["x".into()], body: bx(ea(None, "Nope", "X")), statements: vec![], position: pos() }),
        simple("range", V::Range { start: Box::new(ev("Nope", "X")), end: Box::new(ev("Nope", "Y")), position: pos() }),
        simple("ident_value", V::Identifier { value: "Nope".into(), position: pos() }),
        simple("null", V::Null { position: pos() }),
    ]));
    v.push(("interpolated", vec![
        simple("s", V::InterpolatedString { template: "{0} {1}".into(), expressions: vec![ea(None, "Level", "LOW"), ea(None, "Color", "RED")], position: pos() }),
        simple("plain", V::InterpolatedString { template: "x{0}".into(), expressions: vec![ie(1)], position: pos() }),
        simple("none", V::InterpolatedString { template: "x".into(), expressions: vec![], position: pos() }),
    ]));
    v.push(("containers", vec![
        objprop("o", obj(vec![
            ("a", ev("Level", "LOW")),
            ("b", obj(vec![("c", ev("Color", "RED"))])),
            ("d", arr(vec![ev("Level", "HIGH")])),
            ("e", int(3)),
        ])),
        simple("ctor", V::PrefixedConstructor { prefix: "t".into(), arguments: vec![ev("Color", "RED"), int(1)], position: pos() }),
        simple("ctor_plain", V::PrefixedConstructor { prefix: "t".into(), arguments: vec![int(1), int(2)], position: pos() }),
        tbl(&["a", "b"], vec![("x", ev("Level", "MID")), ("y", arr(vec![ev("Color", "GREEN")])), ("z", int(1))]),
        tbl(&["t2"], vec![("p", int(1)), ("q", int(2))]),
        grp(&["g"], vec![obj(vec![("c", ev("Color", "BLUE"))]), obj(vec![("n", int(1))]), ev("Level", "LOW"), int(9)]),
        simple("plainobj", obj(vec![("n", int(1)), ("arr", arr(vec![int(1)]))])),
        objprop("plain_op", obj(vec![("n", int(1))])),
        objprop("emptyobj", obj(vec![])),
        simple("emptyarr", arr(vec![])),
        objprop("non_object", arr(vec![ev("Nope", "X")])),
    ]));
    v.push(("nested_arrays", vec![
        simple("n_only_enum", nest(vec![ev("Color", "RED")], 1)),
        simple("n_mixed", nest(vec![int(1), ev("Level", "LOW")], 1)),
        simple("n_inner", nest(vec![nest(vec![ev("Color", "RED")], 2), nest(vec![int(1)], 2)], 1)),
        simple("n_untouched", nest(vec![nest(vec![int(1)], 2)], 1)),
        simple("a_plain_enum", arr(vec![ev("Level", "HIGH")])),
        simple("n_obj", nest(vec![obj(vec![("k", ev("Color", "BLUE"))])], 1)),
        simple("n_deep3", nest(vec![nest(vec![nest(vec![ev("Color", "RED")], 3)], 2)], 1)),
        simple("n_in_arr", arr(vec![nest(vec![ev("Color", "RED")], 2), nest(vec![int(2)], 2)])),
        simple("n_empty", nest(vec![], 1)),
        simple("n_expr", nest(vec![vexp(ea(None, "Level", "LOW"))], 1)),
    ]));
    v.push(("imports_merge", vec![
        simple("a", vexp(ea(b, "Rarity", "EPIC"))),
        simple("a2", vexp(ea(b, "Rarity", "COMMON"))),
        simple("b", vexp(ea(b, "ItemType", "ARMOR"))),
        simple("c", vexp(ea(Some("Types"), "Priority", "HIGH"))),
        simple("d", arr(vec![ev("Level", "LOW")])),
    ]));
    v.push(("imports_only", vec![
        simple("a", vexp(ea(b, "Rarity", "EPIC"))),
        simple("c", vexp(ea(Some("Types"), "Priority", "HIGH"))),
        simple("l", arr(vec![vexp(ea(Some("Types"), "Region", "EU"))])),
        simple("n", nest(vec![vexp(ea(b, "ItemType", "QUEST"))], 1)),
    ]));
    v.push(("imports_value_form", vec![
        simple("vf", ev("Base.Rarity", "EPIC")),
        simple("vf_again", ev("Base.Rarity", "COMMON")),
        simple("vf2", ev("Types.Region", "EU")),
    ]));
    v.push(("changed_lists", vec![
        simple("arr_mid", arr(vec![int(1), vexp(ea(None, "Level", "HIGH")), int(2), vexp(ea(None, "Color", "BLUE"))])),
        simple("arr_last", arr(vec![int(1), int(2), vexp(ea(None, "Level", "LOW"))])),
        simple("nest_mid", nest(vec![int(1), vexp(ea(None, "Level", "HIGH")), int(2)], 1)),
        simple("qfv_args", V::QuickFuncCall { function_name: "f".into(), arguments: vec![ie(1), ea(None, "Level", "HIGH"), ie(2), ea(None, "Color", "BLUE")], position: pos() }),
        simple("ctor_args", V::PrefixedConstructor { prefix: "t".into(), arguments: vec![int(1), vexp(ea(None, "Level", "MID")), int(2)], position: pos() }),
        simple("interp", V::InterpolatedString { template: "{0}{1}{2}".into(), expressions: vec![ie(1), ea(None, "Level", "LOW"), ie(3)], position: pos() }),
        simple("expr_qf_args", vexp(E::QuickFuncCall { name: "f".into(), arguments: vec![ie(1), ea(None, "Level", "HIGH"), ie(2)], position: pos() })),
        simple("expr_imp_args", vexp(E::ImportedFunctionCall { namespace_name: "Base".into(), function_name: "scaledValue".into(), arguments: vec![ie(1), ea(b, "Rarity", "EPIC"), ie(2)], position: pos() })),
        simple("expr_static_args", vexp(E::StaticMethodCall { object_name: "Math".into(), method_name: "max".into(), arguments: vec![ie(1), ea(None, "Level", "LOW"), ie(2)], position: pos() })),
    ]));
    v.push(("changed_binary", vec![
        simple("arith_l", vexp(bin(0, ea(None, "Level", "LOW"), "+", ie(1)))),
        simple("arith_r", vexp(bin(0, ie(1), "+", ea(None, "Level", "HIGH")))),
        simple("bit_l", vexp(bin(1, ea(None, "Level", "LOW"), "|", ie(1)))),
        simple("bit_r", vexp(bin(1, ie(1), "|", ea(None, "Level", "HIGH")))),
        simple("cmp_l", vexp(bin(2, ea(None, "Level", "LOW"), "<", ie(1)))),
        simple("cmp_r", vexp(bin(2, ie(1), "<", ea(None, "Level", "HIGH")))),
        simple("logic_l", vexp(bin(3, ea(None, "Level", "LOW"), "||", ie(1)))),
        simple("logic_r", vexp(bin(3, ie(1), "||", ea(None, "Level", "HIGH")))),
        simple("idx_l", vexp(E::IndexAccess { object: bx(ea(None, "Level", "LOW")), index: bx(ie(1)), position: pos() })),
        simple("idx_r", vexp(E::IndexAccess { object: bx(ie(1)), index: bx(ea(None, "Level", "HIGH")), position: pos() })),
        simple("cond_c", vexp(E::Conditional { condition: bx(ea(None, "Level", "LOW")), true_value: bx(ie(1)), false_value: bx(ie(2)), position: pos() })),
        simple("cond_t", vexp(E::Conditional { condition: bx(ie(1)), true_value: bx(ea(None, "Level", "LOW")), false_value: bx(ie(2)), position: pos() })),
        simple("cond_f", vexp(E::Conditional { condition: bx(ie(1)), true_value: bx(ie(2)), false_value: bx(ea(None, "Level", "LOW")), position: pos() })),
        simple("unary_deep", vexp(E::UnaryOp { operator: "!".into(), operand: bx(E::Parenthesized { expression: bx(E::PropertyAccess { object: bx(ea(None, "Color", "RED")), property: "p".into(), position: pos() }), position: pos() }), position: pos() })),
    ]));
    v.push(("changed_containers", vec![
        objprop("op_mid", obj(vec![("a", int(1)), ("b", vexp(ea(None, "Level", "HIGH"))), ("c", int(2)), ("d", vexp(ea(None, "Color", "BLUE")))])),
        simple("o_mid", obj(vec![("a", int(1)), ("b", vexp(ea(None, "Level", "HIGH"))), ("n", obj(vec![("x", int(2)), ("y", arr(vec![int(3), vexp(ea(None, "Color", "GREEN"))]))]))])),
        tbl(&["tm"], vec![("p", int(1)), ("q", vexp(ea(None, "Level", "HIGH"))), ("r", int(2)), ("s", vexp(ea(None, "Color", "BLUE")))]),
        tbl(&["tn"], vec![("p", int(1)), ("q", int(2)), ("r", vexp(ea(None, "Level", "MID")))]),
        grp(&["gm"], vec![int(1), vexp(ea(None, "Level", "HIGH")), ev("Color", "RED"), vexp(ea(None, "Color", "BLUE"))]),
        grp(&["gn"], vec![int(1), int(2), vexp(ea(None, "Level", "LOW"))]),
    ]));
    v.push(("wrapped_value_changed", vec![
        simple("w", vexp(E::Value { value: arr(vec![int(1), vexp(ea(None, "Level", "HIGH"))]), position: pos() })),
        simple("w2", vexp(E::Value { value: obj(vec![("k", vexp(ea(None, "Color", "BLUE")))]), position: pos() })),
    ]));
    v.push(("entries_mixed", vec![
        simple("p0", int(1)),
        simple("c1", vexp(ea(None, "Level", "HIGH"))),
        simple("p2", ev("Color", "RED")),
        simple("c3", vexp(ea(None, "Color", "BLUE"))),
        simple("p4", int(2)),
    ]));
    v.push(("local_same_name", vec![
        simple("l", vexp(ea(None, "Rarity", "A"))),
        simple("i", vexp(ea(b, "Rarity", "LEGENDARY"))),
    ]));
    v.push(("e01_unknown_enum_value", vec![ok_entry(), simple("bad", ev("Nope", "X"))]));
    v.push(("e02_unknown_field_value", vec![ok_entry(), simple("bad", ev("Color", "PURPLE"))]));
    v.push(("e03_in_array", vec![simple("a", arr(vec![ev("Color", "RED"), ev("Level", "NOPE")]))]));
    v.push(("e04_in_object_prop", vec![objprop("o", obj(vec![("a", ev("Color", "RED")), ("n", obj(vec![("b", arr(vec![ev("Color", "PURPLE")]))]))]))]));
    v.push(("e05_expr", vec![simple("bad", vexp(ea(None, "Color", "PURPLE")))]));
    v.push(("e06_import_ns", vec![simple("bad", vexp(ea(Some("Zed"), "Rarity", "EPIC")))]));
    v.push(("e07_import_enum", vec![simple("bad", vexp(ea(b, "Nope", "X")))]));
    v.push(("e08_import_field", vec![simple("bad", vexp(ea(b, "Rarity", "NOPE")))]));
    v.push(("e09_table", vec![ok_entry(), tbl(&["t"], vec![("x", int(1)), ("y", ev("Color", "PURPLE"))])]));
    v.push(("e10_group", vec![ok_entry(), grp(&["g"], vec![obj(vec![("c", ev("Color", "RED"))]), obj(vec![("c", ev("Color", "PURPLE"))])])]));
    v.push(("e11_call_arg", vec![simple("bad", vexp(E::QuickFuncCall { name: "f".into(), arguments: vec![ie(1), ea(None, "Color", "PURPLE")], position: pos() }))]));
    v.push(("e12_interpolated", vec![simple("bad", V::InterpolatedString { template: "{0}".into(), expressions: vec![ea(None, "Color", "PURPLE")], position: pos() })]));
    v.push(("e13_ctor", vec![simple("bad", V::PrefixedConstructor { prefix: "t".into(), arguments: vec![int(1), ev("Color", "PURPLE")], position: pos() })]));
    v.push(("e14_conditional", vec![simple("bad", vexp(E::Conditional { condition: bx(ie(1)), true_value: bx(ea(None, "Color", "RED")), false_value: bx(ea(None, "Color", "PURPLE")), position: pos() }))]));
    v.push(("e15_after_import_use", vec![simple("i", ev("Base.Rarity", "EPIC")), simple("bad", ev("Nope", "X"))]));
    v.push(("e16_call_value_arg", vec![simple("bad", V::QuickFuncCall { function_name: "f".into(), arguments: vec![ea(None, "Level", "NOPE")], position: pos() })]));
    v.push(("e17_nested_array", vec![simple("bad", nest(vec![nest(vec![ev("Color", "PURPLE")], 2)], 1))]));
    v.push(("e18_first_wins", vec![simple("a", ev("Color", "PURPLE")), simple("b", ev("Nope", "X"))]));
    v.push(("e19_value_namespace", vec![simple("bad", ev("Zed.Rarity", "EPIC"))]));
    v.push(("e20_value_enum", vec![simple("bad", ev("Base.Nope", "X"))]));
    v.push(("e21_value_field", vec![simple("bad", ev("Base.Rarity", "NOPE"))]));
    v
}

/// The two bases the hand-built sections are run against: one with local enums and both imports
/// (one local enum shares its name with an imported one), one with the imports only.
fn bases() -> Vec<(&'static str, String)> {
    vec![
        ("local", format!("{HEADER}{IMPORTS}@ENUMS(\n  Color {{ RED = 1, GREEN = 2, BLUE = 3 }}\n  Level {{ LOW = 1, MID = 5, HIGH = 10 }}\n  Rarity {{ A = 1, B = 2 }}\n)\n\n@DATA(\n  x = 1\n)\n")),
        ("noenums", format!("{HEADER}{IMPORTS}@DATA(\n  x = 1\n)\n")),
    ]
}

/// Runs one hand-built data section against a base.
fn resolve_hand(base: &str, entries: Vec<DataEntry>, through_document: bool) -> Outcome {
    let dir = fixture_dir();
    let path = dir.join(format!("hand_base_{}.mdix", if base == "local" { "local" } else { "noenums" }));
    let text = &bases().into_iter().find(|(n, _)| *n == base).expect("a base").1;
    fs::write(&path, text).expect("a base is written");
    let _guard = registry_lock();
    let prepared = prepare(&path.to_string_lossy(), text.as_bytes());
    let mut enhanced = prepared.enhanced;
    enhanced.data = Some(DataSection::new(entries, Position { line: 1, column: 1 }));
    // The document is built from the tree being resolved, as the loader's is.
    let document = through_document.then(|| Document::from_owned(&enhanced));
    resolve(enhanced, &prepared.symbols, &prepared.settings, document)
}

/// `(base, case, ok, fingerprint)` of every hand-built case against both bases.
fn hand_runs(through_document: bool) -> Vec<(&'static str, &'static str, bool, u64)> {
    let mut runs = Vec::new();
    for (base, _) in bases() {
        for (case, entries) in hand_cases() {
            let outcome = resolve_hand(base, entries, through_document);
            runs.push((base, case, outcome.ok, fingerprint(&outcome)));
        }
    }
    runs
}

/// `(base, case, ok, fingerprint)` produced by running the original owned-tree resolver.
const HAND_RECORDING: &[(&str, &str, bool, u64)] = &[
    ("local", "empty", true, 0x2159dd4155f0a125),
    ("local", "ops", true, 0x3c504fa0661b50f9),
    ("local", "cond_paren_access", true, 0x8537b7465aabe075),
    ("local", "calls_and_terminals", false, 0x8d2dcefcb04c122d),
    ("local", "terminal_with_bad_enum", true, 0xdfb86ed03c40cf8b),
    ("local", "interpolated", true, 0x98d553da347ca465),
    ("local", "containers", true, 0x8f7ed25049e739ed),
    ("local", "nested_arrays", true, 0xca1380e3aae763f4),
    ("local", "imports_merge", true, 0xb129d43a1aa33ad9),
    ("local", "imports_only", true, 0xdeff75bb82141295),
    ("local", "imports_value_form", true, 0xc576fb1a979bf252),
    ("local", "changed_lists", false, 0xaa421cb13f0396df),
    ("local", "changed_binary", true, 0xedbda7988a6612f0),
    ("local", "changed_containers", true, 0x7a8e6e992ea4bb5c),
    ("local", "wrapped_value_changed", true, 0xdea4df5b7ea45887),
    ("local", "entries_mixed", true, 0xe5bf711d51dca81b),
    ("local", "local_same_name", true, 0x2ad33079c7046d86),
    ("local", "e01_unknown_enum_value", false, 0x2ecd6281cb4f4df0),
    ("local", "e02_unknown_field_value", false, 0x9b14f6e09eb4adc6),
    ("local", "e03_in_array", false, 0x41dc29cf098e6e2c),
    ("local", "e04_in_object_prop", false, 0x3f7eb1324e525ef3),
    ("local", "e05_expr", false, 0x348c868b143c4763),
    ("local", "e06_import_ns", false, 0x9ff7771b4ba335c3),
    ("local", "e07_import_enum", false, 0x7367742663f51139),
    ("local", "e08_import_field", false, 0x4656f1a9304a80f4),
    ("local", "e09_table", false, 0xaf81384cb1431123),
    ("local", "e10_group", false, 0x4304db76222cdb73),
    ("local", "e11_call_arg", false, 0x3c6c2d8e2754ab87),
    ("local", "e12_interpolated", false, 0x765cdca8e89ca972),
    ("local", "e13_ctor", false, 0xe3db007261f06d3e),
    ("local", "e14_conditional", false, 0x69a286ec04e5ca1c),
    ("local", "e15_after_import_use", false, 0x8586d1b4b4f5f3ae),
    ("local", "e16_call_value_arg", false, 0x51fd2f22dfc5afee),
    ("local", "e17_nested_array", false, 0x4ed38ae5c42c9bda),
    ("local", "e18_first_wins", false, 0xbc3f0fa3c0b510f6),
    ("local", "e19_value_namespace", false, 0x309d08a302fc7302),
    ("local", "e20_value_enum", false, 0x5cc693a3bb1038eb),
    ("local", "e21_value_field", false, 0xba2da27dca42f43d),
    ("noenums", "empty", true, 0x72d912be4ae64e20),
    ("noenums", "ops", false, 0x4f16c7db4dd0a345),
    ("noenums", "cond_paren_access", false, 0xa9b580bf56915b9f),
    ("noenums", "calls_and_terminals", false, 0x962dbc9d1cc3268a),
    ("noenums", "terminal_with_bad_enum", true, 0xfec0dae3d34ea05e),
    ("noenums", "interpolated", false, 0xf3decf5bc5cfadf7),
    ("noenums", "containers", false, 0x823f3cde72919dfc),
    ("noenums", "nested_arrays", false, 0x97e5db9a0405becb),
    ("noenums", "imports_merge", false, 0xca821b8bce245065),
    ("noenums", "imports_only", true, 0x54efb5e38918d640),
    ("noenums", "imports_value_form", true, 0x3884b8cee7ca5527),
    ("noenums", "changed_lists", false, 0x3e5f8b79976577f3),
    ("noenums", "changed_binary", false, 0x1ead8bdf2e4ae9bf),
    ("noenums", "changed_containers", false, 0xe0bcbacd6e301852),
    ("noenums", "wrapped_value_changed", false, 0x9ac527bb9b334ac5),
    ("noenums", "entries_mixed", false, 0x3b9b77932d036767),
    ("noenums", "local_same_name", false, 0xbbfad147322ef861),
    ("noenums", "e01_unknown_enum_value", false, 0x0226fa08b7198cce),
    ("noenums", "e02_unknown_field_value", false, 0x0a99253f928b6317),
    ("noenums", "e03_in_array", false, 0x5893043ba7d39190),
    ("noenums", "e04_in_object_prop", false, 0x2e6917c2aa9c67c0),
    ("noenums", "e05_expr", false, 0x913f82388acae71e),
    ("noenums", "e06_import_ns", false, 0x294e23326986ca1e),
    ("noenums", "e07_import_enum", false, 0xefb72cd0f8791440),
    ("noenums", "e08_import_field", false, 0xe8a91bd6de8deebd),
    ("noenums", "e09_table", false, 0xbe4a82137c7a9a36),
    ("noenums", "e10_group", false, 0xd81621c7835ec382),
    ("noenums", "e11_call_arg", false, 0x889a76c6a938e5aa),
    ("noenums", "e12_interpolated", false, 0x49317f086c320593),
    ("noenums", "e13_ctor", false, 0xe3a08aeac314f59f),
    ("noenums", "e14_conditional", false, 0xbc5dcd0fcf8acbbd),
    ("noenums", "e15_after_import_use", false, 0x50d570b21f2bf763),
    ("noenums", "e16_call_value_arg", false, 0x56ef3807cc115b13),
    ("noenums", "e17_nested_array", false, 0xbfcca4380b21fd6f),
    ("noenums", "e18_first_wins", false, 0xa41c3f36c92afa91),
    ("noenums", "e19_value_namespace", false, 0x9827d217d18ffc8f),
    ("noenums", "e20_value_enum", false, 0x7f12aafdf1d64aa0),
    ("noenums", "e21_value_field", false, 0xb11987d62d131668),
];

fn check_hand_recording(through_document: bool) {
    let runs = hand_runs(through_document);
    assert_eq!(runs.len(), HAND_RECORDING.len(), "HAND_RECORDING is out of date; run print_recording");
    for (run, rec) in runs.iter().zip(HAND_RECORDING) {
        assert_eq!((run.0, run.1), (rec.0, rec.1));
        assert_eq!(run.2, rec.2, "{}/{}: verdict", rec.0, rec.1);
        assert_eq!(run.3, rec.3, "{}/{}: resolved tree (document: {through_document})", rec.0, rec.1);
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

// ==================== messages of the errors ====================

fn first_error(base: &str, case: &str, through_document: bool) -> String {
    let entries = hand_cases().into_iter().find(|(n, _)| *n == case).expect("a case").1;
    let outcome = resolve_hand(base, entries, through_document);
    assert!(!outcome.ok, "{case} should fail");
    outcome.errors.into_iter().next().expect("an error")
}

#[test]
fn every_wrong_reference_is_reported_with_its_original_message() {
    let expected: &[(&str, &str)] = &[
        ("e01_unknown_enum_value", "Invalid enum access at Nope.X: Enum field 'X' not found"),
        ("e02_unknown_field_value", "Invalid enum access at Color.PURPLE: Enum field 'PURPLE' not found"),
        ("e05_expr", "Invalid enum access at Color.PURPLE: Enum 'Color' field 'PURPLE' not found"),
        ("e06_import_ns", "Invalid enum access at Zed.Rarity.EPIC: Namespace 'Zed' not found"),
        ("e07_import_enum", "Invalid enum access at Base.Nope.X: Enum 'Nope' not found in namespace 'Base'"),
        ("e08_import_field", "Invalid enum access at Base.Rarity.NOPE: Field 'NOPE' not found"),
        ("e19_value_namespace", "Invalid enum access at Zed.Rarity.EPIC: Namespace 'Zed' not found"),
        ("e20_value_enum", "Invalid enum access at Base.Nope.X: Enum 'Nope' not found"),
        ("e21_value_field", "Invalid enum access at Base.Rarity.NOPE: Field 'NOPE' not found"),
    ];
    for through_document in [true, false] {
        for (case, message) in expected {
            assert_eq!(&first_error("local", case, through_document), message, "{case}");
        }
    }
}

#[test]
fn the_first_wrong_reference_in_data_order_is_the_one_reported() {
    for through_document in [true, false] {
        assert_eq!(
            first_error("local", "e18_first_wins", through_document),
            "Invalid enum access at Color.PURPLE: Enum field 'PURPLE' not found"
        );
        // The nested position of the bad reference does not change which error comes first.
        assert_eq!(
            first_error("local", "e04_in_object_prop", through_document),
            "Invalid enum access at Color.PURPLE: Enum field 'PURPLE' not found"
        );
    }
}

#[test]
fn a_failed_run_leaves_the_data_section_as_it_was() {
    let entries = hand_cases().into_iter().find(|(n, _)| *n == "e15_after_import_use").expect("a case").1;
    for through_document in [true, false] {
        let outcome = resolve_hand("noenums", entries.clone(), through_document);
        assert!(!outcome.ok);
        assert_eq!(data_of(&outcome), entries.as_slice(), "the data is the input");
        // The imported enum referenced before the error was not merged into @ENUMS.
        assert!(outcome.resolved.as_ref().expect("a tree").enums.is_none());
    }
}

// ==================== rules that are easy to lose ====================

#[test]
fn a_nested_array_that_held_an_enum_comes_back_as_a_plain_array() {
    let entries = hand_cases().into_iter().find(|(n, _)| *n == "nested_arrays").expect("a case").1;
    for through_document in [true, false] {
        let outcome = resolve_hand("local", entries.clone(), through_document);
        assert!(outcome.ok, "{:?}", outcome.errors);
        // An enum value stays an enum value, but its nested array stops being nested.
        assert!(matches!(value_of(&outcome, "n_only_enum"), V::Array { .. }));
        assert!(matches!(value_of(&outcome, "n_mixed"), V::Array { .. }));
        assert!(matches!(value_of(&outcome, "n_obj"), V::Array { .. }));
        assert!(matches!(value_of(&outcome, "n_expr"), V::Array { .. }));
        match value_of(&outcome, "n_inner") {
            V::Array { values, .. } => {
                assert!(matches!(values[0], V::Array { .. }), "the inner array that held an enum");
                assert!(matches!(values[1], V::NestedArray { level: 2, .. }), "the one that did not");
            }
            other => panic!("n_inner is {other:?}"),
        }
        match value_of(&outcome, "n_deep3") {
            V::Array { values, .. } => match &values[0] {
                V::Array { values, .. } => assert!(matches!(values[0], V::Array { .. })),
                other => panic!("the middle level is {other:?}"),
            },
            other => panic!("n_deep3 is {other:?}"),
        }
        // Without an enum anywhere under it a nested array keeps its depth.
        assert!(matches!(value_of(&outcome, "n_untouched"), V::NestedArray { level: 1, .. }));
        assert!(matches!(value_of(&outcome, "n_empty"), V::NestedArray { level: 1, .. }));
        // A plain array that held an enum is a plain array, as it was.
        assert!(matches!(value_of(&outcome, "a_plain_enum"), V::Array { .. }));
        assert!(matches!(value_of(&outcome, "n_in_arr"), V::Array { .. }));
    }
}

#[test]
fn an_enum_value_stays_in_place_and_an_enum_access_becomes_an_integer() {
    let entries = vec![
        simple("value", ev("Level", "HIGH")),
        simple("expr", vexp(ea(None, "Level", "HIGH"))),
        simple("imported", vexp(ea(Some("Base"), "Rarity", "EPIC"))),
    ];
    for through_document in [true, false] {
        let outcome = resolve_hand("local", entries.clone(), through_document);
        assert!(outcome.ok, "{:?}", outcome.errors);
        assert!(matches!(value_of(&outcome, "value"), V::EnumValue { .. }));
        for (name, number) in [("expr", 10), ("imported", 3)] {
            match value_of(&outcome, name) {
                V::Expression { expr, .. } => match expr.as_ref() {
                    E::Value { value: V::Integer { value, .. }, .. } => assert_eq!(*value, number, "{name}"),
                    other => panic!("{name} resolved to {other:?}"),
                },
                other => panic!("{name} is {other:?}"),
            }
        }
    }
}

fn enum_names(outcome: &Outcome) -> Vec<String> {
    outcome
        .resolved
        .as_ref()
        .expect("a tree")
        .enums
        .as_ref()
        .map(|section| section.enums.iter().map(|d| d.name.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn an_imported_enum_is_merged_only_through_the_dotted_value_form() {
    let entries = |case: &str| hand_cases().into_iter().find(|(n, _)| *n == case).expect("a case").1;
    for through_document in [true, false] {
        // The dotted value form merges each enum once, whatever the number of uses, and creates
        // the section when the file has none.
        let outcome = resolve_hand("noenums", entries("imports_value_form"), through_document);
        assert!(outcome.ok, "{:?}", outcome.errors);
        let mut names = enum_names(&outcome);
        names.sort();
        assert_eq!(names, ["Base.Rarity", "Types.Region"]);
        let section = outcome.resolved.as_ref().unwrap().enums.as_ref().unwrap();
        let rarity = section.enums.iter().find(|d| d.name == "Base.Rarity").unwrap();
        let fields: Vec<(&str, Option<i32>)> = rarity.fields.iter().map(|f| (f.name.as_str(), f.value)).collect();
        assert_eq!(
            fields,
            [("COMMON", Some(0)), ("UNCOMMON", Some(1)), ("RARE", Some(2)), ("EPIC", Some(3)), ("LEGENDARY", Some(4))],
            "the fields are in value order"
        );

        // Next to local enums the merged ones are added after them.
        let outcome = resolve_hand("local", entries("imports_value_form"), through_document);
        let names = enum_names(&outcome);
        assert_eq!(&names[..3], ["Color", "Level", "Rarity"]);
        assert_eq!(names.len(), 5);

        // The expression form checks and resolves the reference but records nothing.
        let outcome = resolve_hand("noenums", entries("imports_only"), through_document);
        assert!(outcome.ok, "{:?}", outcome.errors);
        assert!(outcome.resolved.as_ref().unwrap().enums.is_none());
        let outcome = resolve_hand("local", entries("imports_merge"), through_document);
        assert_eq!(enum_names(&outcome), ["Color", "Level", "Rarity"]);
    }
}

// ==================== which tree is read ====================

fn prepared_local() -> Prepared {
    let dir = fixture_dir();
    let path = dir.join("hand_base_local.mdix");
    let text = &bases().into_iter().find(|(n, _)| *n == "local").unwrap().1;
    fs::write(&path, text).expect("a base is written");
    prepare(&path.to_string_lossy(), text.as_bytes())
}

fn expr_entry() -> DataEntry {
    simple("e", vexp(ea(None, "Level", "HIGH")))
}

fn first_value(entries: &[DataEntry], name: &str) -> V {
    entries
        .iter()
        .find_map(|entry| match entry {
            DataEntry::SimpleProperty { name: n, value, .. } if n == name => Some(value.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no property {name}"))
}

#[test]
fn the_phase_reads_the_document_when_it_has_one() {
    let _guard = registry_lock();
    let prepared = prepared_local();
    let symbols = &prepared.symbols;

    // The owned tree holds a bad reference, the document a good expression: with the document
    // attached the run succeeds and resolves the document's data.
    let mut owned = prepared.enhanced.clone();
    owned.data = Some(DataSection::new(vec![simple("bad", ev("Nope", "X"))], Position::UNKNOWN));
    let mut good = prepared.enhanced.clone();
    good.data = Some(DataSection::new(vec![expr_entry()], Position::UNKNOWN));
    let outcome = resolve(owned.clone(), symbols, &prepared.settings, Some(Document::from_owned(&good)));
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert!(matches!(
        first_value(data_of(&outcome), "e"),
        V::Expression { ref expr, .. } if matches!(expr.as_ref(), E::Value { value: V::Integer { value: 10, .. }, .. })
    ));
    // Without it the owned tree is read, and the bad reference is found.
    let outcome = resolve(owned, symbols, &prepared.settings, None);
    assert!(!outcome.ok);

    // The reverse: a good owned tree and a document with a bad reference.
    let mut owned = prepared.enhanced.clone();
    owned.data = Some(DataSection::new(vec![expr_entry()], Position::UNKNOWN));
    let mut bad = prepared.enhanced.clone();
    bad.data = Some(DataSection::new(vec![simple("bad", ev("Nope", "X"))], Position::UNKNOWN));
    let outcome = resolve(owned, symbols, &prepared.settings, Some(Document::from_owned(&bad)));
    assert!(!outcome.ok);
}

#[test]
fn the_document_decides_whether_there_is_a_data_section() {
    let _guard = registry_lock();
    let prepared = prepared_local();
    let symbols = &prepared.symbols;
    let with_data = |entries: Vec<DataEntry>| {
        let mut ast = prepared.enhanced.clone();
        ast.data = Some(DataSection::new(entries, Position::UNKNOWN));
        ast
    };

    // The document has no data section: the phase is skipped, so the expression in the owned
    // data keeps its enum access.
    let mut without = prepared.enhanced.clone();
    without.data = None;
    let outcome = resolve(with_data(vec![expr_entry()]), symbols, &prepared.settings, Some(Document::from_owned(&without)));
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert!(matches!(
        first_value(data_of(&outcome), "e"),
        V::Expression { ref expr, .. } if matches!(expr.as_ref(), E::EnumAccess { .. })
    ));

    // The owned tree has none and the document has: the document's data is resolved and becomes
    // the owned data the later phases read.
    let mut owned = prepared.enhanced.clone();
    owned.data = None;
    let doc = Document::from_owned(&with_data(vec![expr_entry()]));
    let outcome = resolve(owned, symbols, &prepared.settings, Some(doc));
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert!(matches!(
        first_value(data_of(&outcome), "e"),
        V::Expression { ref expr, .. } if matches!(expr.as_ref(), E::Value { value: V::Integer { value: 10, .. }, .. })
    ));
    // The same when the document's data has nothing to resolve: the owned tree still ends up
    // with the document's data, so a document without a change is not a reason to drop it.
    let mut owned = prepared.enhanced.clone();
    owned.data = None;
    let doc = Document::from_owned(&with_data(vec![simple("v", int(1))]));
    let outcome = resolve(owned, symbols, &prepared.settings, Some(doc));
    assert!(outcome.ok, "{:?}", outcome.errors);
    assert!(matches!(first_value(data_of(&outcome), "v"), V::Integer { value: 1, .. }));
}

// ==================== sharing ====================

fn arena_sizes(document: &Document) -> (usize, usize, usize) {
    (document.arena.value_count(), document.arena.expr_count(), document.arena.used_bytes())
}

#[test]
fn data_without_a_change_allocates_nothing_in_the_document() {
    let _guard = registry_lock();
    let prepared = prepared_local();
    let mut ast = prepared.enhanced.clone();
    ast.data = Some(DataSection::new(
        vec![
            simple("v", ev("Color", "RED")),
            simple("l", arr(vec![ev("Level", "LOW"), int(2)])),
            simple("o", obj(vec![("k", ev("Color", "BLUE")), ("n", int(1))])),
            simple("n", arr(vec![nest(vec![int(1)], 2)])),
            simple("x", vexp(ie(7))),
            simple("w", vexp(E::Value { value: ev("Color", "RED"), position: pos() })),
            simple("c", V::QuickFuncCall { function_name: "f".into(), arguments: vec![ie(1)], position: pos() }),
            simple("s", V::InterpolatedString { template: "{0}".into(), expressions: vec![ie(1)], position: pos() }),
            tbl(&["t"], vec![("x", ev("Level", "MID"))]),
            grp(&["g"], vec![ev("Color", "GREEN"), int(4)]),
        ],
        Position::UNKNOWN,
    ));
    let document = Document::from_owned(&ast);
    let before = arena_sizes(&document);
    let before_data = document.data.as_ref().unwrap().export(&document.arena);
    // The call to an undefined function fails a later phase; phase 1 is what is measured.
    let outcome = resolve(ast, &prepared.symbols, &prepared.settings, Some(document));
    let document = outcome.document.as_ref().unwrap();
    assert_eq!(arena_sizes(document), before, "nothing was allocated, not even a list");
    assert_eq!(document.data.as_ref().unwrap().export(&document.arena), before_data, "the section is as it was");
}

#[test]
fn data_with_a_change_leaves_the_resolved_section_in_the_document_and_shares_the_rest() {
    let _guard = registry_lock();
    let prepared = prepared_local();
    let mut ast = prepared.enhanced.clone();
    ast.data = Some(DataSection::new(
        vec![
            simple("before", arr(vec![ev("Color", "RED"), int(2), int(3)])),
            expr_entry(),
            simple("after", arr(vec![ev("Level", "LOW")])),
        ],
        Position::UNKNOWN,
    ));
    let document = Document::from_owned(&ast);
    let before = arena_sizes(&document);
    let ids_before: Vec<_> = match document.data.as_ref().unwrap().entries.as_slice() {
        [first, _, third] => [first, third]
            .iter()
            .map(|e| match e {
                dixscript::Compiler::AST::arena::DocDataEntry::SimpleProperty { value, .. } => *value,
                _ => panic!("a simple property"),
            })
            .collect(),
        other => panic!("{} entries", other.len()),
    };
    let outcome = resolve(ast, &prepared.symbols, &prepared.settings, Some(document));
    assert!(outcome.ok, "{:?}", outcome.errors);
    let document = outcome.document.as_ref().unwrap();
    let (values, exprs, _) = arena_sizes(document);
    // The access became an integer and a value expression, and the value that wrapped it was
    // rebuilt around the new expression; the two entries without a change were not copied.
    assert_eq!(values, before.0 + 2, "the integer and the rebuilt wrapper");
    assert_eq!(exprs, before.1 + 1, "the value expression");
    let entries = &document.data.as_ref().unwrap().entries;
    match (&entries[0], &entries[2]) {
        (
            dixscript::Compiler::AST::arena::DocDataEntry::SimpleProperty { value: a, .. },
            dixscript::Compiler::AST::arena::DocDataEntry::SimpleProperty { value: b, .. },
        ) => assert_eq!([*a, *b].as_slice(), ids_before.as_slice(), "untouched entries share their nodes"),
        _ => panic!("simple properties"),
    }
    // The document holds the resolved section: the owned export of its entry is the integer.
    let exported = document.data.as_ref().unwrap().export(&document.arena);
    assert!(matches!(
        first_value(&exported.entries, "e"),
        V::Expression { ref expr, .. } if matches!(expr.as_ref(), E::Value { value: V::Integer { value: 10, .. }, .. })
    ));
}

// ==================== the debug log ====================

/// Resolves `entries` with debug logging on and returns the text the resolver logged.
fn logged_run(base: &str, entries: Option<Vec<DataEntry>>, debug: DebugMode, through_document: bool) -> String {
    let dir = fixture_dir();
    let path = dir.join("hand_base_local.mdix");
    let text = &bases().into_iter().find(|(n, _)| *n == base).expect("a base").1;
    fs::write(&path, text).expect("a base is written");
    let _guard = registry_lock();
    let prepared = prepare(&path.to_string_lossy(), text.as_bytes());
    let mut enhanced = prepared.enhanced;
    enhanced.data = entries.map(|e| DataSection::new(e, Position { line: 1, column: 1 }));
    let mut document = through_document.then(|| Document::from_owned(&enhanced));
    let em = ErrorManager::new_isolated();
    let mut resolver = ValueResolver::new_with_error_manager(enhanced, &prepared.symbols, debug, em.clone());
    if let Some(doc) = document.as_mut() {
        resolver = with_doc(resolver, doc);
    }
    let _ = resolver.resolve();
    em.get_log_contents()
}

#[test]
fn the_debug_log_reports_the_counts_and_the_merge() {
    let entries = || {
        vec![
            simple("a", ev("Color", "RED")),
            simple("b", vexp(ea(None, "Level", "HIGH"))),
            simple("c", ev("Base.Rarity", "EPIC")),
            simple("d", vexp(ea(Some("Types"), "Region", "EU"))),
            simple("e", arr(vec![ev("Level", "LOW")])),
        ]
    };
    for through_document in [true, false] {
        let log = logged_run("local", Some(entries()), DebugMode::Regular, through_document);
        assert!(log.contains("Pre-processing: resolving all enum values to integers"), "{log}");
        assert!(log.contains("Resolved 3 local + 2 imported enum values"), "{log}");
        assert!(log.contains("Merged 1 imported enum declaration(s) into local @ENUMS"), "{log}");
        let log = logged_run("local", Some(vec![simple("a", int(1))]), DebugMode::Regular, through_document);
        assert!(log.contains("Resolved 0 local + 0 imported enum values"), "{log}");
        assert!(!log.contains("Merged"), "{log}");
        // Without a data section the phase does nothing and says nothing at this level.
        let log = logged_run("local", None, DebugMode::Regular, through_document);
        assert!(!log.contains("Pre-processing"), "{log}");
        assert!(!log.contains("Resolved"), "{log}");
    }
}
