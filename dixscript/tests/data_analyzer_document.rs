// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/data_analyzer_document.rs"
// ============================================================================
//! Tests for the `@DATA` analyzer reading a `Document`.
//!
//! `GeneralSemanticAnalyzer` can be given the arena form of a file with `with_document`, and the
//! `@DATA` analyzer then reads the `Document` where it used to read the owned tree. It must
//! report exactly what it reported before, and build the same indexes and symbol table entries.
//! The tests check four things:
//!
//! * the two paths give identical diagnostics, positions, section results and indexes for every
//!   source under all three error strategies;
//! * the diagnostics, the two indexes and the registered variables are the ones the owned-tree
//!   analyzer recorded before the change (the `RECORDING` table, produced by running the original
//!   code);
//! * the branches the parser cannot produce (an expression value, flat data after grouped data,
//!   a constructor with the wrong arguments, a nested array, odd enum and function names) give the
//!   recorded diagnostics through both entry points, `analyze` and `analyze_doc`, on sections
//!   built by hand (`HAND_RECORDING`);
//! * the analyzer really reads the `Document` when it has one and the owned tree when it does
//!   not, and takes its indexes from the one it read.

use std::collections::{BTreeMap, HashMap};

use dixscript::Compiler::AST::arena::{Arena, DocData};
use dixscript::Compiler::AST::{
    DataEntry, DataSection, DataType, Expression, ObjectProperty, PropertyAssignment, TablePath, Value,
};
use dixscript::Compiler::Core::SectionAnalyzers::DataSectionAnalyzer;

use dixscript::Compiler::AST::arena::Document;
use dixscript::Compiler::AST::data_types::{CompatibilityMode, ErrorHandlingStrategy};
use dixscript::Compiler::AST::{DixScript, Position};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::{
    GeneralParser, GeneralSemanticAnalyzer, SectionAnalysisResult, SemanticAnalysisResult,
};
use dixscript::Compiler::Utilities::SymbolTable;
use dixscript::ErrorManager::ErrorManager;

/// One parsed file: the document, its export, and the settings the parse produced.
struct Parsed {
    document: Document,
    ast: DixScript,
    settings: OperationalSettings,
}

/// Runs the loader's first four stages and returns the document, or `None` when the parse
/// fails.
fn parse(source: &str, strategy: ErrorHandlingStrategy) -> Option<Parsed> {
    let label = "data-analyzer-document-test";
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
    settings.error_handling_strategy = strategy;
    em.update_settings(settings.clone());
    let document = GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &settings, em)
        .expect("the parser starts")
        .parse_document()
        .ok()?;
    let ast = document.export();
    Some(Parsed { document, ast, settings })
}

/// Analyzes with a fresh error manager and returns the result and what reached the manager.
fn analyze(
    ast: &DixScript,
    settings: &OperationalSettings,
    document: Option<&Document>,
) -> (SemanticAnalysisResult, Vec<String>) {
    let em = ErrorManager::new_isolated_silent();
    let analyzer = GeneralSemanticAnalyzer::new_with_error_manager(ast, settings, em.clone());
    let result = match document {
        Some(doc) => analyzer.with_document(doc).analyze(),
        None => analyzer.analyze(),
    };
    let mut logged: Vec<String> = em
        .get_semantic_errors()
        .iter()
        .map(|e| format!("{}:{}:{}:{:?}:{:?}", e.line, e.column, e.message, e.section_name, e.suggestion))
        .collect();
    logged.sort();
    (result, logged)
}

/// Everything about a result that the tests compare, in a stable order.
fn describe(result: &SemanticAnalysisResult, logged: &[String]) -> String {
    let mut errors: Vec<String> = result
        .errors
        .iter()
        .map(|e| format!("[{}] {} | {} | {} | {} | {:?}", e.error_id, e.error_type, e.section_name, e.message, e.suggestion, e.position))
        .collect();
    errors.sort();
    let mut warnings: Vec<String> = result
        .warnings
        .iter()
        .map(|w| format!("[{}] {} | {} | {:?}", w.warning_id, w.message, w.section_name, w.position))
        .collect();
    warnings.sort();
    let mut sections: Vec<String> = result
        .section_results
        .iter()
        .map(|(name, s)| format!("{name} {}", describe_section(s)))
        .collect();
    sections.sort();
    format!(
        "success={}\nerrors={errors:#?}\nwarnings={warnings:#?}\nsections={sections:#?}\nlogged={logged:#?}",
        result.is_success
    )
}

/// One section result in a stable order.
fn describe_section(s: &SectionAnalysisResult) -> String {
    let mut e: Vec<String> = s.errors.iter().map(|e| format!("{}:{}:{:?}:{}", e.error_id, e.message, e.position, e.suggestion)).collect();
    e.sort();
    let mut w: Vec<String> = s.warnings.iter().map(|w| format!("{}:{}:{:?}", w.warning_id, w.message, w.position)).collect();
    w.sort();
    format!("ok={} errors={e:?} warnings={w:?}", s.is_success)
}

fn position(p: Option<Position>) -> String {
    match p {
        None => "-".to_string(),
        Some(p) => format!("{}:{}", p.line, p.column),
    }
}

/// The diagnostics of one section as `E|id|message|line:col` and `W|id|message|line:col`
/// lines, sorted.
fn section_lines(result: &SemanticAnalysisResult, section: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for e in result.errors.iter().filter(|e| e.section_name == section) {
        lines.push(format!("E|{}|{}|{}", e.error_id, e.message, position(e.position)));
    }
    for w in result.warnings.iter().filter(|w| w.section_name == section) {
        lines.push(format!("W|{}|{}|{}", w.warning_id, w.message, position(w.position)));
    }
    lines.sort();
    lines
}

fn source(name: &str) -> &'static str {
    SOURCES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("unknown source {name}")).1
}

fn strategy(name: &str) -> ErrorHandlingStrategy {
    match name {
        "halt" => ErrorHandlingStrategy::Halt,
        "continue" => ErrorHandlingStrategy::Continue,
        "recover" => ErrorHandlingStrategy::Recover,
        other => panic!("unknown strategy {other}"),
    }
}

const STRATEGIES: [&str; 3] = ["halt", "continue", "recover"];

/// The recorded cases: name, strategy, overall success, and the recorded section lines.
fn recorded_cases() -> Vec<(String, String, bool, Vec<String>)> {
    let mut cases: Vec<(String, String, bool, Vec<String>)> = Vec::new();
    for line in RECORDING.lines().filter(|l| !l.is_empty()) {
        if let Some(header) = line.strip_prefix('@') {
            let mut parts = header.split('|');
            let name = parts.next().expect("name").to_string();
            let strategy = parts.next().expect("strategy").to_string();
            let success = parts.next().expect("success") == "success=true";
            cases.push((name, strategy, success, Vec::new()));
        } else {
            cases.last_mut().expect("a header comes first").3.push(line.to_string());
        }
    }
    cases
}

/// The sources the tests run, as `(name, text)`. Each exercises a different check.
const SOURCES: &[(&str, &str)] = &[
    ("data_scalars", r##"@DATA(
  a = 1
  b = 2L
  c = 1.5f
  d = 2.5
  e = 1e3
  f = "s"
  h = true
  j = #FF00AA
  k = null
  l = -5
  m = -2.5
  d1 = 2026-09-06T00:00:00Z
  d2 = 2026-09-06
)
"##),
    ("data_declared_ok", r##"@DATA(
  a<int> = 1
  b<long> = 2
  c<float> = 1
  d<double> = 1.5f
  e<string> = "s"
  f<bool> = true
  g<array<int>> = [1, 2, 3]
  h<tuple<float,float>> = t:(1.5, 2.5)
  i<hex> = 5
)
"##),
    ("data_declared_mismatch", r##"@DATA(
  a<int> = "s"
  b<string> = 5
  c<bool> = 1
  d<int> = 1.5
  e<array> = [1]
  f<object> = [1]
  g<int> = null
)
"##),
    ("data_widening", r##"@DATA(
  a<float> = 1
  b<float> = 5L
  c<double> = 1.5f
  d<long> = 5
  e<float> = 2.5
  f<double> = 7
)
"##),
    ("data_ordering", r##"@DATA(
  t: x = 1
  a = 2
  o = {k = 1}
)
"##),
    ("data_dup_table_path", r##"@DATA(
  t: x = 1
  t: y = 2
  u.v: z = 3
)
"##),
    ("data_dup_group_path", r##"@DATA(
  g:: 1, 2
  g:: 3
)
"##),
    ("data_dup_flat", r##"@DATA(
  a = 1
  a = 2
  o = {k = 1}
  o = {k = 2}
  b = {x = 1}
  b = 3
)
"##),
    ("data_dup_table_prop", r##"@DATA(
  t: x = 1, x = 2, y = 3
)
"##),
    ("data_reserved_keyword", r##"@DATA(
  if = 1
  return = 3
  let = 4
  log = 5
)
"##),
    ("data_reserved_in_table", r##"@DATA(
  t: if = 1, y = 2
)
"##),
    ("data_nesting_11", r##"@DATA(
  a = [[[[[[[[[[[1]]]]]]]]]]]
)
"##),
    ("data_nesting_12", r##"@DATA(
  a = [[[[[[[[[[[[1]]]]]]]]]]]]
)
"##),
    ("data_nesting_13", r##"@DATA(
  a = [[[[[[[[[[[[[1]]]]]]]]]]]]]
)
"##),
    ("data_nesting_obj_13", r##"@DATA(
  o = {a = {a = {a = {a = {a = {a = {a = {a = {a = {a = {a = {a = {a = 1}}}}}}}}}}}}}
)
"##),
    ("data_nesting_obj_6", r##"@DATA(
  o = {a = {a = {a = {a = {a = {a = 1}}}}}}
)
"##),
    ("data_array_mixed", r##"@DATA(
  a = [1, "x"]
  b = [1, 2.5]
  c = [[1], ["a"]]
  d = [{a = 1}, {b = "x"}]
  e = [1L, 2]
  f = []
  g = [null, 1, null]
  h = [1, 2, 3]
)
"##),
    ("data_group_mixed", r##"@DATA(
  g:: 1, "s"
  h:: {a = 1}, {b = 2}
  k:: [1], [2], 3
)
"##),
    ("data_group_empty", r##"@DATA(
  e::
  f:: 1
)
"##),
    ("data_object_dup_prop", r##"@DATA(
  o = {a = 1, a = 2, b = {c = 1, c = 2}}
  p = {x = {y = 1}}
)
"##),
    ("data_tuple_sizes", r##"@DATA(
  t6 = t:(1, 2, 3, 4, 5, 6)
  t7 = t:(1, 2, 3, 4, 5, 6, 7)
  tm = t:(1, "a", true)
  tn = t:(a, b)
)
"##),
    ("data_enums", r##"@ENUMS(
  LogLevel { DEBUG = 0, INFO = 1 }
)
@DATA(
  x = LogLevel.INFO
  y = LogLevel.NOPE
  z = Missing.A
  w<enum> = LogLevel.DEBUG
  n = Base.Rarity.COMMON
  m = Base.Rarity
)
"##),
    ("data_functions", r##"@QUICKFUNCS(
  ~dbl<int>(n) {
    return n * 2
  }
  ~name() {
    return "x"
  }
)
@DATA(
  a = dbl(2)
  b = nofunc(1)
  c = Ns.f(1)
  d<string> = dbl(2)
  e = dbl(dbl(1))
  f<string> = name()
  g = name()
)
"##),
    ("data_invalid_expression", r##"@DATA(
  a = 1 + 2
  b = (3)
  c = !true
  o = {x = 1 + 1}
  r = [1 + 1, 2]
)
"##),
    ("data_blob_regex", r##"@DATA(
  b1 = b:("SGVsbG8=")
  b2 = b:("%%")
  b3 = b:()
  b4 = b:(1)
  b5 = b:("a", "b")
  r1 = r:("^[a-z]+$")
  r2 = r:("(")
  r3 = r:()
  r4 = r:(5)
  r5 = r:("a", "b")
)
"##),
    ("data_unknown_prefix", r##"@DATA(
  z = z:(1)
  a = 1
)
"##),
    ("data_identifiers", r##"@DATA(
  a = SomeName
  b = Colors.Red
  c = lib.fn(1)
  d = lib.Colors.Red
)
"##),
    ("data_range", r##"@DATA(
  r = 1..5
  s = 0..10
)
"##),
    ("data_tables_groups", r##"@DATA(
  t.u: x<int> = 1, y = "s", z = [1, 2]
  g:: {a = 1}, {a = 2}
  k.h:: 1, 2, 3
  m:: [1, 2], [3]
  w.q: o = {a = 1, b = {c = 2}}, n = 7
)
"##),
    ("data_object_property_typed", r##"@DATA(
  o<object> = {a = 1, b = [1, 2], c = {d = "s"}}
  p<int> = {x = 1}
  q = {z = 1}
)
"##),
    ("data_null_mixed", r##"@DATA(
  n1 = null
  n2<int> = null
  a = [null, 1, null]
  b = [null, null]
)
"##),
    ("data_short_names", r##"@DATA(
  a = {x = 1}
  b = {x = "s"}
  x = 5
  c = {y = {x = 2}}
)
"##),
    ("data_mismatch_array_types", r##"@DATA(
  a<array<int>> = ["a", "b"]
  b<array<string>> = ["a", "b"]
  c<tuple<int,string>> = t:(1, "x")
  d<tuple<int,string>> = t:("x", 1)
)
"##),
    ("data_with_security_and_schema", r##"@DATA(
  a = 1
  b = "x"
)
@SCHEMA(
  a = Schema.Int(true)
)
@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256" }
)
"##),
    ("data_blob_zero_args", r##"@DATA(
  b3 = b:()
)
"##),
    ("data_blob_not_string", r##"@DATA(
  b4 = b:(1)
)
"##),
    ("data_blob_two_args", r##"@DATA(
  b5 = b:("a", "b")
)
"##),
    ("data_blob_ok", r##"@DATA(
  b1 = b:("SGVsbG8=")
  b0 = b:("")
)
"##),
    ("data_regex_bad", r##"@DATA(
  r2 = r:("(")
)
"##),
    ("data_regex_zero_args", r##"@DATA(
  r3 = r:()
)
"##),
    ("data_regex_not_string", r##"@DATA(
  r4 = r:(5)
)
"##),
    ("data_regex_two_args", r##"@DATA(
  r5 = r:("a", "b")
)
"##),
    ("data_regex_ok", r##"@DATA(
  r1 = r:("^[a-z]+$")
  r0 = r:("")
)
"##),
    ("data_ordering_after_group", r##"@DATA(
  g:: 1, 2
  a = 3
)
"##),
    ("data_ordering_blank_lines", r##"@DATA(
  t.u: x = 1


  a = 2
)
"##),
    ("data_ordering_object_after_table", r##"@DATA(
  t: x = 1
  o = {k = 1}
)
"##),
    ("data_expression_alone", r##"@DATA(
  a = 1 + 2
)
"##),
    ("data_expression_paren", r##"@DATA(
  b = (3)
)
"##),
    ("data_only", r##"@DATA(
  a = 1
  b = [1, 2, 3]
)
"##),
];

/// The number of cases in `RECORDING`.
const EXPECTED_CASES: usize = 63;

/// What the owned-tree analyzer reported for each source before it moved to the arena,
/// captured by running the original code. A line `@name|strategy|success=..` starts a case. The
/// `E|id|message|line:col` and `W|id|message|line:col` lines after it are the section's
/// diagnostics (`-` for no position), followed by the short-name index, the type index and the
/// variables registered in the symbol table, where there are any.
const RECORDING: &str = r##"
@data_scalars|continue|success=true
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "d1": ["DATA.d1"], "d2": ["DATA.d2"], "e": ["DATA.e"], "f": ["DATA.f"], "h": ["DATA.h"], "j": ["DATA.j"], "k": ["DATA.k"], "l": ["DATA.l"], "m": ["DATA.m"]}
type_index={"DATA.a": "Int", "DATA.b": "Long", "DATA.c": "Float", "DATA.d": "Double", "DATA.d1": "Timestamp", "DATA.d2": "Date", "DATA.e": "Double", "DATA.f": "String", "DATA.h": "Bool", "DATA.j": "Hex", "DATA.l": "Int", "DATA.m": "Double"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Long), is_inferred: true, scope: \"global\", line: 3, column: 3 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Float), is_inferred: true, scope: \"global\", line: 4, column: 3 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: Some(Double), is_inferred: true, scope: \"global\", line: 5, column: 3 }", "d1": "VariableInfo { name: \"d1\", declared_type: None, inferred_type: Some(Timestamp), is_inferred: true, scope: \"global\", line: 13, column: 3 }", "d2": "VariableInfo { name: \"d2\", declared_type: None, inferred_type: Some(Date), is_inferred: true, scope: \"global\", line: 14, column: 3 }", "e": "VariableInfo { name: \"e\", declared_type: None, inferred_type: Some(Double), is_inferred: true, scope: \"global\", line: 6, column: 3 }", "f": "VariableInfo { name: \"f\", declared_type: None, inferred_type: Some(String), is_inferred: true, scope: \"global\", line: 7, column: 3 }", "h": "VariableInfo { name: \"h\", declared_type: None, inferred_type: Some(Bool), is_inferred: true, scope: \"global\", line: 8, column: 3 }", "j": "VariableInfo { name: \"j\", declared_type: None, inferred_type: Some(Hex), is_inferred: true, scope: \"global\", line: 9, column: 3 }", "k": "VariableInfo { name: \"k\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 10, column: 3 }", "l": "VariableInfo { name: \"l\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 11, column: 3 }", "m": "VariableInfo { name: \"m\", declared_type: None, inferred_type: Some(Double), is_inferred: true, scope: \"global\", line: 12, column: 3 }"}
@data_declared_ok|continue|success=true
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"], "g": ["DATA.g"], "h": ["DATA.h"], "i": ["DATA.i"]}
type_index={"DATA.a": "Int", "DATA.b": "Long", "DATA.c": "Float", "DATA.d": "Double", "DATA.e": "String", "DATA.f": "Bool", "DATA.g": "Array", "DATA.h": "Tuple", "DATA.i": "Int"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: Some(Int), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: Some(Long), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 3, column: 3 }", "c": "VariableInfo { name: \"c\", declared_type: Some(Float), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 4, column: 3 }", "d": "VariableInfo { name: \"d\", declared_type: Some(Double), inferred_type: Some(Float), is_inferred: false, scope: \"global\", line: 5, column: 3 }", "e": "VariableInfo { name: \"e\", declared_type: Some(String), inferred_type: Some(String), is_inferred: false, scope: \"global\", line: 6, column: 3 }", "f": "VariableInfo { name: \"f\", declared_type: Some(Bool), inferred_type: Some(Bool), is_inferred: false, scope: \"global\", line: 7, column: 3 }", "g": "VariableInfo { name: \"g\", declared_type: Some(TypedArray(Int)), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 8, column: 3 }", "h": "VariableInfo { name: \"h\", declared_type: Some(TypedTuple([Some(Float), Some(Float), None, None, None, None])), inferred_type: Some(Tuple), is_inferred: false, scope: \"global\", line: 9, column: 3 }", "i": "VariableInfo { name: \"i\", declared_type: Some(Hex), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 10, column: 3 }"}
@data_declared_mismatch|continue|success=false
E|DATA3F7|Property 'a' is declared as <int> but the value is <string>. These types are not compatible.|2:3
E|DATA3F7|Property 'b' is declared as <string> but the value is <int>. These types are not compatible.|3:3
E|DATA3F7|Property 'c' is declared as <bool> but the value is <int>. These types are not compatible.|4:3
E|DATA3F7|Property 'd' is declared as <int> but the value is <double>. These types are not compatible.|5:3
E|DATA3F7|Property 'f' is declared as <object> but the value is <array>. These types are not compatible.|7:3
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"], "g": ["DATA.g"]}
type_index={"DATA.a": "String", "DATA.b": "Int", "DATA.c": "Int", "DATA.d": "Double", "DATA.e": "Array", "DATA.f": "Array", "DATA.g": "Int"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: Some(Int), inferred_type: Some(String), is_inferred: false, scope: \"global\", line: 2, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: Some(String), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 3, column: 3 }", "c": "VariableInfo { name: \"c\", declared_type: Some(Bool), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 4, column: 3 }", "d": "VariableInfo { name: \"d\", declared_type: Some(Int), inferred_type: Some(Double), is_inferred: false, scope: \"global\", line: 5, column: 3 }", "e": "VariableInfo { name: \"e\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 6, column: 3 }", "f": "VariableInfo { name: \"f\", declared_type: Some(Object), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 7, column: 3 }", "g": "VariableInfo { name: \"g\", declared_type: Some(Int), inferred_type: None, is_inferred: false, scope: \"global\", line: 8, column: 3 }"}
@data_widening|continue|success=true
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"]}
type_index={"DATA.a": "Float", "DATA.b": "Float", "DATA.c": "Double", "DATA.d": "Long", "DATA.e": "Float", "DATA.f": "Double"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: Some(Float), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: Some(Float), inferred_type: Some(Long), is_inferred: false, scope: \"global\", line: 3, column: 3 }", "c": "VariableInfo { name: \"c\", declared_type: Some(Double), inferred_type: Some(Float), is_inferred: false, scope: \"global\", line: 4, column: 3 }", "d": "VariableInfo { name: \"d\", declared_type: Some(Long), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 5, column: 3 }", "e": "VariableInfo { name: \"e\", declared_type: Some(Float), inferred_type: Some(Double), is_inferred: false, scope: \"global\", line: 6, column: 3 }", "f": "VariableInfo { name: \"f\", declared_type: Some(Double), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 7, column: 3 }"}
@data_ordering|continue|success=true
short_name_index={"a": ["t.a"], "k": ["DATA.t.k"], "o": ["t.o"], "x": ["t.x"]}
type_index={"DATA.t.k": "Int", "t.a": "Int", "t.o": "Object", "t.x": "Int"}
data_variables={"t.a": "VariableInfo { name: \"t.a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 3, column: 3 }", "t.o": "VariableInfo { name: \"t.o\", declared_type: None, inferred_type: Some(Object), is_inferred: true, scope: \"t\", line: 4, column: 3 }", "t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 2, column: 6 }"}
@data_dup_table_path|continue|success=false
E|DATA5EE|Table property path 't' is defined multiple times|3:3
short_name_index={"x": ["t.x"], "y": ["t.y"], "z": ["u.v.z"]}
type_index={"t.x": "Int", "t.y": "Int", "u.v.z": "Int"}
data_variables={"t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 2, column: 6 }", "t.y": "VariableInfo { name: \"t.y\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 3, column: 6 }", "u.v.z": "VariableInfo { name: \"u.v.z\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"u.v\", line: 4, column: 8 }"}
@data_dup_group_path|continue|success=false
E|DATA7F1|Group array path 'g' is defined multiple times|3:3
data_variables={"g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 3, column: 3 }"}
@data_dup_flat|continue|success=false
E|DATA705|Flat property 'a' is defined more than once in @DATA. Each flat property name must be unique.|3:3
E|DATA705|Flat property 'b' is defined more than once in @DATA. Each flat property name must be unique.|7:3
E|DATA705|Flat property 'o' is defined more than once in @DATA. Each flat property name must be unique.|5:3
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"], "k": ["DATA.o.k"], "o": ["DATA.o"], "x": ["DATA.b.x"]}
type_index={"DATA.a": "Int", "DATA.b": "Object", "DATA.b.x": "Int", "DATA.o": "Object", "DATA.o.k": "Int"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 6, column: 3 }", "o": "VariableInfo { name: \"o\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 4, column: 3 }"}
@data_dup_table_prop|continue|success=false
E|DATA8C6|Property 'x' is defined more than once in table path 't'|2:13
short_name_index={"x": ["t.x"], "y": ["t.y"]}
type_index={"t.x": "Int", "t.y": "Int"}
data_variables={"t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 2, column: 6 }", "t.y": "VariableInfo { name: \"t.y\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 2, column: 20 }"}
@data_reserved_keyword|continue|success=false
E|DATA4E4|'if' is a reserved keyword and cannot be used as an identifier|2:3
E|DATA4E4|'let' is a reserved keyword and cannot be used as an identifier|4:3
E|DATA4E4|'return' is a reserved keyword and cannot be used as an identifier|3:3
short_name_index={"log": ["DATA.log"]}
type_index={"DATA.log": "Int"}
data_variables={"log": "VariableInfo { name: \"log\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 5, column: 3 }"}
@data_reserved_in_table|continue|success=false
E|DATA4E4|'if' is a reserved keyword and cannot be used as an identifier|2:6
short_name_index={"y": ["t.y"]}
type_index={"t.y": "Int"}
data_variables={"t.y": "VariableInfo { name: \"t.y\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 2, column: 14 }"}
@data_nesting_11|continue|success=true
short_name_index={"a": ["DATA.a"]}
type_index={"DATA.a": "Array"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_nesting_12|continue|success=true
short_name_index={"a": ["DATA.a"]}
type_index={"DATA.a": "Array"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_nesting_13|continue|success=false
E|DATA4E6|Nesting depth exceeds maximum of 12 levels in property 'a'[0][0][0][0][0][0][0][0][0][0][0][0][0]|2:20
short_name_index={"a": ["DATA.a"]}
type_index={"DATA.a": "Array"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_nesting_obj_13|continue|success=false
E|DATA4E6|Nesting depth exceeds maximum of 12 levels in object property 'o'.a.a.a.a.a.a.a.a.a.a.a.a.a|2:72
short_name_index={"a": ["DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a"], "o": ["DATA.o"]}
type_index={"DATA.o": "Object", "DATA.o.a": "Object"}
data_variables={"o": "VariableInfo { name: \"o\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_nesting_obj_6|continue|success=true
short_name_index={"a": ["DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a", "DATA.o.a"], "o": ["DATA.o"]}
type_index={"DATA.o": "Object", "DATA.o.a": "Object"}
data_variables={"o": "VariableInfo { name: \"o\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_array_mixed|continue|success=false
E|DATA677|Array in property 'a' contains mixed types: Int and String|2:11
E|DATA677|Array in property 'b' contains mixed types: Int and Double|3:11
E|DATA677|Array in property 'e' contains mixed types: Long and Int|6:12
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"], "g": ["DATA.g"], "h": ["DATA.h"]}
type_index={"DATA.a": "Array", "DATA.b": "Array", "DATA.c": "Array", "DATA.d": "Array", "DATA.e": "Array", "DATA.f": "Array", "DATA.g": "Array", "DATA.h": "Array"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 3, column: 3 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 4, column: 3 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 5, column: 3 }", "e": "VariableInfo { name: \"e\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 6, column: 3 }", "f": "VariableInfo { name: \"f\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 7, column: 3 }", "g": "VariableInfo { name: \"g\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 8, column: 3 }", "h": "VariableInfo { name: \"h\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 9, column: 3 }"}
@data_group_mixed|continue|success=false
E|DATA677|Group array 'g' contains mixed types: Int and String|2:10
E|DATA677|Group array 'k' contains mixed types: Array and Int|4:17
short_name_index={"a": ["h[0].a"], "b": ["h[1].b"]}
data_variables={"g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 2, column: 3 }", "h": "VariableInfo { name: \"h\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 3, column: 3 }", "k": "VariableInfo { name: \"k\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 3 }"}
@data_group_empty|continue|success=true
W|DATAW86A|Group array 'e' is empty|2:3
data_variables={"f": "VariableInfo { name: \"f\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 3, column: 3 }"}
@data_object_dup_prop|continue|success=false
E|DATA57F|Duplicate property 'a' in object at object property 'o'|2:15
E|DATA57F|Duplicate property 'c' in object at object property 'o'.b|2:34
short_name_index={"a": ["DATA.o.a", "DATA.o.a"], "b": ["DATA.o.b"], "c": ["DATA.o.c", "DATA.o.c"], "o": ["DATA.o"], "p": ["DATA.p"], "x": ["DATA.p.x"], "y": ["DATA.p.y"]}
type_index={"DATA.o": "Object", "DATA.o.a": "Int", "DATA.o.b": "Object", "DATA.o.c": "Int", "DATA.p": "Object", "DATA.p.x": "Object", "DATA.p.y": "Int"}
data_variables={"o": "VariableInfo { name: \"o\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 2, column: 3 }", "p": "VariableInfo { name: \"p\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 3, column: 3 }"}
@data_tuple_sizes|continue|success=false
E|DATA4A5|Tuple in property 't7' has 7 elements (maximum is 6)|3:8
short_name_index={"t6": ["DATA.t6"], "t7": ["DATA.t7"], "tm": ["DATA.tm"], "tn": ["DATA.tn"]}
type_index={"DATA.t6": "Tuple", "DATA.t7": "Tuple", "DATA.tm": "Tuple", "DATA.tn": "Tuple"}
data_variables={"t6": "VariableInfo { name: \"t6\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 3 }", "t7": "VariableInfo { name: \"t7\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 3, column: 3 }", "tm": "VariableInfo { name: \"tm\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 4, column: 3 }", "tn": "VariableInfo { name: \"tn\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 5, column: 3 }"}
@data_enums|continue|success=false
E|DATA460|Enum 'Base' not found in property 'm'|10:7
E|DATA460|Enum 'Missing' not found in property 'z'|7:7
E|DATA460|Namespace 'Base' not found — import it with @IMPORTS in property 'n'|9:7
E|DATA63C|Enum value 'LogLevel.NOPE' not found in property 'y'|6:7
short_name_index={"m": ["DATA.m"], "n": ["DATA.n"], "w": ["DATA.w"], "x": ["DATA.x"], "y": ["DATA.y"], "z": ["DATA.z"]}
type_index={"DATA.m": "Enum", "DATA.n": "Enum", "DATA.w": "Enum", "DATA.x": "Enum", "DATA.y": "Enum", "DATA.z": "Enum"}
data_variables={"m": "VariableInfo { name: \"m\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 10, column: 3 }", "n": "VariableInfo { name: \"n\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 9, column: 3 }", "w": "VariableInfo { name: \"w\", declared_type: Some(Enum), inferred_type: Some(Enum), is_inferred: false, scope: \"global\", line: 8, column: 3 }", "x": "VariableInfo { name: \"x\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 5, column: 3 }", "y": "VariableInfo { name: \"y\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 6, column: 3 }", "z": "VariableInfo { name: \"z\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 7, column: 3 }"}
@data_functions|continue|success=false
E|DATA3F7|Property 'd' is declared as <string> but the value is <int>. These types are not compatible.|13:3
E|DATA591|Function 'nofunc' not found in property 'b'|11:7
E|DATA591|Namespace 'Ns' not found — import it with @IMPORTS in property 'c'|12:7
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"], "g": ["DATA.g"]}
type_index={"DATA.a": "Int", "DATA.d": "Int", "DATA.e": "Int", "DATA.f": "String"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 10, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 11, column: 3 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 12, column: 3 }", "d": "VariableInfo { name: \"d\", declared_type: Some(String), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 13, column: 3 }", "e": "VariableInfo { name: \"e\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 14, column: 3 }", "f": "VariableInfo { name: \"f\", declared_type: Some(String), inferred_type: None, is_inferred: false, scope: \"global\", line: 15, column: 3 }", "g": "VariableInfo { name: \"g\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 16, column: 3 }"}
@data_functions|halt|success=false
data_variables={}
@data_invalid_expression|continue|success=true
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"]}
type_index={"DATA.a": "Int"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 3, column: 3 }"}
@data_invalid_expression|halt|success=true
data_variables={}
@data_blob_regex|continue|success=false
E|DATA5FF|Invalid base64 content in blob at property 'b2'|3:11
short_name_index={"b1": ["DATA.b1"], "b2": ["DATA.b2"], "b3": ["DATA.b3"], "b4": ["DATA.b4"]}
type_index={"DATA.b1": "Blob", "DATA.b2": "Blob", "DATA.b3": "Blob", "DATA.b4": "Blob"}
data_variables={"b1": "VariableInfo { name: \"b1\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 3 }", "b2": "VariableInfo { name: \"b2\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 3, column: 3 }", "b3": "VariableInfo { name: \"b3\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 4, column: 3 }", "b4": "VariableInfo { name: \"b4\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 5, column: 3 }"}
@data_blob_regex|halt|success=true
data_variables={}
@data_unknown_prefix|continue|success=true
short_name_index={"z": ["DATA.z"]}
data_variables={"z": "VariableInfo { name: \"z\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_unknown_prefix|halt|success=true
data_variables={}
@data_identifiers|continue|success=false
E|DATA460|Enum 'Colors' not found in property 'b'|3:7
E|DATA460|Namespace 'lib' not found — import it with @IMPORTS in property 'd'|5:7
E|DATA591|Namespace 'lib' not found — import it with @IMPORTS in property 'c'|4:7
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"]}
type_index={"DATA.b": "Enum", "DATA.d": "Enum"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 3, column: 3 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 4, column: 3 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 5, column: 3 }"}
@data_range|continue|success=true
short_name_index={"r": ["DATA.r"], "s": ["DATA.s"]}
type_index={"DATA.r": "Int", "DATA.s": "Int"}
data_variables={"r": "VariableInfo { name: \"r\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 3 }", "s": "VariableInfo { name: \"s\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 3, column: 3 }"}
@data_range|halt|success=true
data_variables={}
@data_tables_groups|continue|success=true
short_name_index={"a": ["DATA.w.q.a", "g[0].a", "g[1].a"], "b": ["DATA.w.q.b"], "c": ["DATA.w.q.o.c"], "n": ["w.q.n"], "o": ["w.q.o"], "x": ["t.u.x"], "y": ["t.u.y"], "z": ["t.u.z"]}
type_index={"DATA.w.q.a": "Int", "DATA.w.q.b": "Object", "DATA.w.q.o.c": "Int", "t.u.x": "Int", "t.u.y": "String", "t.u.z": "Array", "w.q.n": "Int", "w.q.o": "Object"}
data_variables={"g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 3, column: 3 }", "k.h": "VariableInfo { name: \"k.h\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 3 }", "m": "VariableInfo { name: \"m\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 5, column: 3 }", "t.u.x": "VariableInfo { name: \"t.u.x\", declared_type: Some(Int), inferred_type: Some(Int), is_inferred: false, scope: \"t.u\", line: 2, column: 8 }", "t.u.y": "VariableInfo { name: \"t.u.y\", declared_type: None, inferred_type: Some(String), is_inferred: true, scope: \"t.u\", line: 2, column: 20 }", "t.u.z": "VariableInfo { name: \"t.u.z\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"t.u\", line: 2, column: 29 }", "w.q.n": "VariableInfo { name: \"w.q.n\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"w.q\", line: 6, column: 34 }", "w.q.o": "VariableInfo { name: \"w.q.o\", declared_type: None, inferred_type: Some(Object), is_inferred: true, scope: \"w.q\", line: 6, column: 8 }"}
@data_object_property_typed|continue|success=true
short_name_index={"a": ["DATA.o.a"], "b": ["DATA.o.b"], "c": ["DATA.o.c"], "d": ["DATA.o.d"], "o": ["DATA.o"], "p": ["DATA.p"], "q": ["DATA.q"], "x": ["DATA.p.x"], "z": ["DATA.q.z"]}
type_index={"DATA.o": "Object", "DATA.o.a": "Int", "DATA.o.b": "Array", "DATA.o.c": "Object", "DATA.o.d": "String", "DATA.p": "Int", "DATA.p.x": "Int", "DATA.q": "Object", "DATA.q.z": "Int"}
data_variables={"o": "VariableInfo { name: \"o\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: false, scope: \"global\", line: 2, column: 3 }", "p": "VariableInfo { name: \"p\", declared_type: Some(Int), inferred_type: Some(Object), is_inferred: false, scope: \"global\", line: 3, column: 3 }", "q": "VariableInfo { name: \"q\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 4, column: 3 }"}
@data_null_mixed|continue|success=true
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"], "n1": ["DATA.n1"], "n2": ["DATA.n2"]}
type_index={"DATA.a": "Array", "DATA.b": "Array", "DATA.n2": "Int"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 4, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 5, column: 3 }", "n1": "VariableInfo { name: \"n1\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 3 }", "n2": "VariableInfo { name: \"n2\", declared_type: Some(Int), inferred_type: None, is_inferred: false, scope: \"global\", line: 3, column: 3 }"}
@data_short_names|continue|success=true
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "x": ["DATA.a.x", "DATA.b.x", "DATA.c.x", "DATA.x"], "y": ["DATA.c.y"]}
type_index={"DATA.a": "Object", "DATA.a.x": "Int", "DATA.b": "Object", "DATA.b.x": "String", "DATA.c": "Object", "DATA.c.x": "Int", "DATA.c.y": "Object", "DATA.x": "Int"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 2, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 3, column: 3 }", "c": "VariableInfo { name: \"c\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 5, column: 3 }", "x": "VariableInfo { name: \"x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 4, column: 3 }"}
@data_mismatch_array_types|continue|success=true
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"]}
type_index={"DATA.a": "Array", "DATA.b": "Array", "DATA.c": "Tuple", "DATA.d": "Tuple"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: Some(TypedArray(Int)), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 2, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: Some(TypedArray(String)), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 3, column: 3 }", "c": "VariableInfo { name: \"c\", declared_type: Some(TypedTuple([Some(Int), Some(String), None, None, None, None])), inferred_type: Some(Tuple), is_inferred: false, scope: \"global\", line: 4, column: 3 }", "d": "VariableInfo { name: \"d\", declared_type: Some(TypedTuple([Some(Int), Some(String), None, None, None, None])), inferred_type: Some(Tuple), is_inferred: false, scope: \"global\", line: 5, column: 3 }"}
@data_with_security_and_schema|continue|success=true
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"]}
type_index={"DATA.a": "Int", "DATA.b": "String"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(String), is_inferred: true, scope: \"global\", line: 3, column: 3 }"}
@data_blob_zero_args|continue|success=true
short_name_index={"b3": ["DATA.b3"]}
type_index={"DATA.b3": "Blob"}
data_variables={"b3": "VariableInfo { name: \"b3\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_blob_zero_args|halt|success=true
data_variables={}
@data_blob_not_string|continue|success=true
short_name_index={"b4": ["DATA.b4"]}
type_index={"DATA.b4": "Blob"}
data_variables={"b4": "VariableInfo { name: \"b4\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_blob_not_string|halt|success=true
data_variables={}
@data_blob_two_args|continue|success=false
E|DATA5FF|Invalid base64 content in blob at property 'b5'|2:11
short_name_index={"b5": ["DATA.b5"]}
type_index={"DATA.b5": "Blob"}
data_variables={"b5": "VariableInfo { name: \"b5\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_blob_two_args|halt|success=true
data_variables={}
@data_blob_ok|continue|success=true
short_name_index={"b0": ["DATA.b0"], "b1": ["DATA.b1"]}
type_index={"DATA.b0": "Blob", "DATA.b1": "Blob"}
data_variables={"b0": "VariableInfo { name: \"b0\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 3, column: 3 }", "b1": "VariableInfo { name: \"b1\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_regex_bad|continue|success=false
E|DATA65E|Invalid regex pattern in property 'r2'|2:11
short_name_index={"r2": ["DATA.r2"]}
type_index={"DATA.r2": "Regex"}
data_variables={"r2": "VariableInfo { name: \"r2\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_regex_zero_args|continue|success=true
short_name_index={"r3": ["DATA.r3"]}
type_index={"DATA.r3": "Regex"}
data_variables={"r3": "VariableInfo { name: \"r3\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_regex_zero_args|halt|success=true
data_variables={}
@data_regex_not_string|continue|success=true
short_name_index={"r4": ["DATA.r4"]}
type_index={"DATA.r4": "Regex"}
data_variables={"r4": "VariableInfo { name: \"r4\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_regex_not_string|halt|success=true
data_variables={}
@data_regex_two_args|continue|success=true
short_name_index={"r5": ["DATA.r5"]}
type_index={"DATA.r5": "Regex"}
data_variables={"r5": "VariableInfo { name: \"r5\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_regex_two_args|halt|success=true
data_variables={}
@data_regex_ok|continue|success=true
short_name_index={"r0": ["DATA.r0"], "r1": ["DATA.r1"]}
type_index={"DATA.r0": "Regex", "DATA.r1": "Regex"}
data_variables={"r0": "VariableInfo { name: \"r0\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 3, column: 3 }", "r1": "VariableInfo { name: \"r1\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_ordering_after_group|continue|success=true
data_variables={"g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 2, column: 3 }"}
@data_ordering_after_group|halt|success=true
data_variables={}
@data_ordering_blank_lines|continue|success=true
short_name_index={"a": ["t.u.a"], "x": ["t.u.x"]}
type_index={"t.u.a": "Int", "t.u.x": "Int"}
data_variables={"t.u.a": "VariableInfo { name: \"t.u.a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t.u\", line: 5, column: 3 }", "t.u.x": "VariableInfo { name: \"t.u.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t.u\", line: 2, column: 8 }"}
@data_ordering_object_after_table|continue|success=true
short_name_index={"k": ["DATA.t.k"], "o": ["t.o"], "x": ["t.x"]}
type_index={"DATA.t.k": "Int", "t.o": "Object", "t.x": "Int"}
data_variables={"t.o": "VariableInfo { name: \"t.o\", declared_type: None, inferred_type: Some(Object), is_inferred: true, scope: \"t\", line: 3, column: 3 }", "t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 2, column: 6 }"}
@data_expression_alone|continue|success=true
short_name_index={"a": ["DATA.a"]}
type_index={"DATA.a": "Int"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_expression_alone|halt|success=true
data_variables={}
@data_expression_paren|continue|success=true
short_name_index={"b": ["DATA.b"]}
data_variables={"b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 3 }"}
@data_expression_paren|halt|success=true
data_variables={}
@data_only|continue|success=true
short_name_index={"a": ["DATA.a"], "b": ["DATA.b"]}
type_index={"DATA.a": "Int", "DATA.b": "Array"}
data_variables={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 3 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 3, column: 3 }"}
"##;

/// The number of blocks in `HAND_RECORDING`.
const EXPECTED_HAND_BLOCKS: usize = 63;

/// What the owned-tree analyzer reported for each hand-built section before it moved to the
/// arena. A line `@name|strategy` starts a block, followed by the success flag, the errors and
/// warnings, the two indexes, the registered variables and the records left in the error manager.
const HAND_RECORDING: &str = r##"
@hand_ordering_table_then_simple|halt
success=false
E [DATA56E] ORDERING_VIOLATION | Flat property 'a' appears after grouped data. All flat properties must come before table properties and group arrays. | Move this property before any table properties (path:) or group arrays (path::) | Some(Position { line: 2, column: 1 })
E [DATA56E] ORDERING_VIOLATION | Flat property 'o' appears after grouped data. All flat properties must come before table properties and group arrays. | Move this property before any table properties (path:) or group arrays (path::) | Some(Position { line: 11, column: 1 })
short={"a": ["DATA.a"], "k": ["DATA.o.k"], "o": ["DATA.o"], "x": ["t.x"]}
types={"DATA.a": "Int", "DATA.o": "Object", "DATA.o.k": "Int", "t.x": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "o": "VariableInfo { name: \"o\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 11, column: 1 }", "t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 3, column: 1 }"}
logged=[]
@hand_ordering_table_then_simple|continue
success=false
E [DATA56E] ORDERING_VIOLATION | Flat property 'a' appears after grouped data. All flat properties must come before table properties and group arrays. | Move this property before any table properties (path:) or group arrays (path::) | Some(Position { line: 2, column: 1 })
E [DATA56E] ORDERING_VIOLATION | Flat property 'o' appears after grouped data. All flat properties must come before table properties and group arrays. | Move this property before any table properties (path:) or group arrays (path::) | Some(Position { line: 11, column: 1 })
short={"a": ["DATA.a"], "k": ["DATA.o.k"], "o": ["DATA.o"], "x": ["t.x"]}
types={"DATA.a": "Int", "DATA.o": "Object", "DATA.o.k": "Int", "t.x": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "o": "VariableInfo { name: \"o\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 11, column: 1 }", "t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 3, column: 1 }"}
logged=[]
@hand_ordering_table_then_simple|recover
success=false
E [DATA56E] ORDERING_VIOLATION | Flat property 'a' appears after grouped data. All flat properties must come before table properties and group arrays. | Move this property before any table properties (path:) or group arrays (path::) | Some(Position { line: 2, column: 1 })
E [DATA56E] ORDERING_VIOLATION | Flat property 'o' appears after grouped data. All flat properties must come before table properties and group arrays. | Move this property before any table properties (path:) or group arrays (path::) | Some(Position { line: 11, column: 1 })
short={"a": ["DATA.a"], "k": ["DATA.o.k"], "o": ["DATA.o"], "x": ["t.x"]}
types={"DATA.a": "Int", "DATA.o": "Object", "DATA.o.k": "Int", "t.x": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "o": "VariableInfo { name: \"o\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 11, column: 1 }", "t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 3, column: 1 }"}
logged=[]
@hand_ordering_group_then_simple|halt
success=false
E [DATA56E] ORDERING_VIOLATION | Flat property 'a' appears after grouped data. All flat properties must come before table properties and group arrays. | Move this property before any table properties (path:) or group arrays (path::) | Some(Position { line: 2, column: 1 })
short={"a": ["DATA.a"]}
types={"DATA.a": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 1 }"}
logged=[]
@hand_ordering_group_then_simple|continue
success=false
E [DATA56E] ORDERING_VIOLATION | Flat property 'a' appears after grouped data. All flat properties must come before table properties and group arrays. | Move this property before any table properties (path:) or group arrays (path::) | Some(Position { line: 2, column: 1 })
short={"a": ["DATA.a"]}
types={"DATA.a": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 1 }"}
logged=[]
@hand_ordering_group_then_simple|recover
success=false
E [DATA56E] ORDERING_VIOLATION | Flat property 'a' appears after grouped data. All flat properties must come before table properties and group arrays. | Move this property before any table properties (path:) or group arrays (path::) | Some(Position { line: 2, column: 1 })
short={"a": ["DATA.a"]}
types={"DATA.a": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 1 }"}
logged=[]
@hand_ordering_ok|halt
success=true
short={"a": ["DATA.a"], "x": ["t.x"]}
types={"DATA.a": "Int", "t.x": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 1 }", "t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 3, column: 1 }"}
logged=[]
@hand_ordering_ok|continue
success=true
short={"a": ["DATA.a"], "x": ["t.x"]}
types={"DATA.a": "Int", "t.x": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 1 }", "t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 3, column: 1 }"}
logged=[]
@hand_ordering_ok|recover
success=true
short={"a": ["DATA.a"], "x": ["t.x"]}
types={"DATA.a": "Int", "t.x": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 1 }", "t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 3, column: 1 }"}
logged=[]
@hand_expression_values|halt
success=false
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at property 'a' | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at property 'b'[1] | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at property 'c'.k | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at table property 't.x' | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "x": ["t.x"]}
types={"DATA.b": "Array", "DATA.c": "Object"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"t\", line: 3, column: 1 }"}
logged=[]
@hand_expression_values|continue
success=false
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at property 'a' | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at property 'b'[1] | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at property 'c'.k | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at table property 't.x' | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "x": ["t.x"]}
types={"DATA.b": "Array", "DATA.c": "Object"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"t\", line: 3, column: 1 }"}
logged=[]
@hand_expression_values|recover
success=false
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at property 'a' | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at property 'b'[1] | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at property 'c'.k | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at table property 't.x' | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "x": ["t.x"]}
types={"DATA.b": "Array", "DATA.c": "Object"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "t.x": "VariableInfo { name: \"t.x\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"t\", line: 3, column: 1 }"}
logged=[]
@hand_blob_forms|halt
success=false
E [DATA5FF] INVALID_BLOB_CONTENT | Blob constructor in property 'b0' must have exactly 1 argument (base64 string) | Use format: b:("base64EncodedData") | Some(Position { line: 7, column: 1 })
E [DATA5FF] INVALID_BLOB_CONTENT | Blob constructor in property 'b1' requires a string literal | Use format: b:("base64EncodedData") | Some(Position { line: 1, column: 2 })
E [DATA5FF] INVALID_BLOB_CONTENT | Blob constructor in property 'b2' must have exactly 1 argument (base64 string) | Use format: b:("base64EncodedData") | Some(Position { line: 7, column: 1 })
E [DATA5FF] INVALID_BLOB_CONTENT | Invalid base64 content in blob at property 'b4' | Ensure the string is valid base64 encoded data | Some(Position { line: 1, column: 3 })
short={"b0": ["DATA.b0"], "b1": ["DATA.b1"], "b2": ["DATA.b2"], "b3": ["DATA.b3"], "b4": ["DATA.b4"]}
types={"DATA.b0": "Blob", "DATA.b1": "Blob", "DATA.b2": "Blob", "DATA.b3": "Blob", "DATA.b4": "Blob"}
vars={"b0": "VariableInfo { name: \"b0\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b1": "VariableInfo { name: \"b1\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b2": "VariableInfo { name: \"b2\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b3": "VariableInfo { name: \"b3\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b4": "VariableInfo { name: \"b4\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_blob_forms|continue
success=false
E [DATA5FF] INVALID_BLOB_CONTENT | Blob constructor in property 'b0' must have exactly 1 argument (base64 string) | Use format: b:("base64EncodedData") | Some(Position { line: 7, column: 1 })
E [DATA5FF] INVALID_BLOB_CONTENT | Blob constructor in property 'b1' requires a string literal | Use format: b:("base64EncodedData") | Some(Position { line: 1, column: 2 })
E [DATA5FF] INVALID_BLOB_CONTENT | Blob constructor in property 'b2' must have exactly 1 argument (base64 string) | Use format: b:("base64EncodedData") | Some(Position { line: 7, column: 1 })
E [DATA5FF] INVALID_BLOB_CONTENT | Invalid base64 content in blob at property 'b4' | Ensure the string is valid base64 encoded data | Some(Position { line: 1, column: 3 })
short={"b0": ["DATA.b0"], "b1": ["DATA.b1"], "b2": ["DATA.b2"], "b3": ["DATA.b3"], "b4": ["DATA.b4"]}
types={"DATA.b0": "Blob", "DATA.b1": "Blob", "DATA.b2": "Blob", "DATA.b3": "Blob", "DATA.b4": "Blob"}
vars={"b0": "VariableInfo { name: \"b0\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b1": "VariableInfo { name: \"b1\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b2": "VariableInfo { name: \"b2\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b3": "VariableInfo { name: \"b3\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b4": "VariableInfo { name: \"b4\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_blob_forms|recover
success=false
E [DATA5FF] INVALID_BLOB_CONTENT | Blob constructor in property 'b0' must have exactly 1 argument (base64 string) | Use format: b:("base64EncodedData") | Some(Position { line: 7, column: 1 })
E [DATA5FF] INVALID_BLOB_CONTENT | Blob constructor in property 'b1' requires a string literal | Use format: b:("base64EncodedData") | Some(Position { line: 1, column: 2 })
E [DATA5FF] INVALID_BLOB_CONTENT | Blob constructor in property 'b2' must have exactly 1 argument (base64 string) | Use format: b:("base64EncodedData") | Some(Position { line: 7, column: 1 })
E [DATA5FF] INVALID_BLOB_CONTENT | Invalid base64 content in blob at property 'b4' | Ensure the string is valid base64 encoded data | Some(Position { line: 1, column: 3 })
short={"b0": ["DATA.b0"], "b1": ["DATA.b1"], "b2": ["DATA.b2"], "b3": ["DATA.b3"], "b4": ["DATA.b4"]}
types={"DATA.b0": "Blob", "DATA.b1": "Blob", "DATA.b2": "Blob", "DATA.b3": "Blob", "DATA.b4": "Blob"}
vars={"b0": "VariableInfo { name: \"b0\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b1": "VariableInfo { name: \"b1\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b2": "VariableInfo { name: \"b2\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b3": "VariableInfo { name: \"b3\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b4": "VariableInfo { name: \"b4\", declared_type: None, inferred_type: Some(Blob), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_regex_forms|halt
success=false
E [DATA65E] INVALID_REGEX_PATTERN | Invalid regex pattern in property 'r4' | Fix the regular expression syntax | Some(Position { line: 1, column: 3 })
E [DATA65E] INVALID_REGEX_PATTERN | Regex constructor in property 'r0' must have exactly 1 argument (pattern string) | Use format: r:("pattern") | Some(Position { line: 7, column: 1 })
E [DATA65E] INVALID_REGEX_PATTERN | Regex constructor in property 'r1' requires a string literal | Use format: r:("pattern") | Some(Position { line: 1, column: 2 })
E [DATA65E] INVALID_REGEX_PATTERN | Regex constructor in property 'r2' must have exactly 1 argument (pattern string) | Use format: r:("pattern") | Some(Position { line: 7, column: 1 })
short={"r0": ["DATA.r0"], "r1": ["DATA.r1"], "r2": ["DATA.r2"], "r3": ["DATA.r3"], "r4": ["DATA.r4"]}
types={"DATA.r0": "Regex", "DATA.r1": "Regex", "DATA.r2": "Regex", "DATA.r3": "Regex", "DATA.r4": "Regex"}
vars={"r0": "VariableInfo { name: \"r0\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "r1": "VariableInfo { name: \"r1\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "r2": "VariableInfo { name: \"r2\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "r3": "VariableInfo { name: \"r3\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "r4": "VariableInfo { name: \"r4\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_regex_forms|continue
success=false
E [DATA65E] INVALID_REGEX_PATTERN | Invalid regex pattern in property 'r4' | Fix the regular expression syntax | Some(Position { line: 1, column: 3 })
E [DATA65E] INVALID_REGEX_PATTERN | Regex constructor in property 'r0' must have exactly 1 argument (pattern string) | Use format: r:("pattern") | Some(Position { line: 7, column: 1 })
E [DATA65E] INVALID_REGEX_PATTERN | Regex constructor in property 'r1' requires a string literal | Use format: r:("pattern") | Some(Position { line: 1, column: 2 })
E [DATA65E] INVALID_REGEX_PATTERN | Regex constructor in property 'r2' must have exactly 1 argument (pattern string) | Use format: r:("pattern") | Some(Position { line: 7, column: 1 })
short={"r0": ["DATA.r0"], "r1": ["DATA.r1"], "r2": ["DATA.r2"], "r3": ["DATA.r3"], "r4": ["DATA.r4"]}
types={"DATA.r0": "Regex", "DATA.r1": "Regex", "DATA.r2": "Regex", "DATA.r3": "Regex", "DATA.r4": "Regex"}
vars={"r0": "VariableInfo { name: \"r0\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "r1": "VariableInfo { name: \"r1\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "r2": "VariableInfo { name: \"r2\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "r3": "VariableInfo { name: \"r3\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "r4": "VariableInfo { name: \"r4\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_regex_forms|recover
success=false
E [DATA65E] INVALID_REGEX_PATTERN | Invalid regex pattern in property 'r4' | Fix the regular expression syntax | Some(Position { line: 1, column: 3 })
E [DATA65E] INVALID_REGEX_PATTERN | Regex constructor in property 'r0' must have exactly 1 argument (pattern string) | Use format: r:("pattern") | Some(Position { line: 7, column: 1 })
E [DATA65E] INVALID_REGEX_PATTERN | Regex constructor in property 'r1' requires a string literal | Use format: r:("pattern") | Some(Position { line: 1, column: 2 })
E [DATA65E] INVALID_REGEX_PATTERN | Regex constructor in property 'r2' must have exactly 1 argument (pattern string) | Use format: r:("pattern") | Some(Position { line: 7, column: 1 })
short={"r0": ["DATA.r0"], "r1": ["DATA.r1"], "r2": ["DATA.r2"], "r3": ["DATA.r3"], "r4": ["DATA.r4"]}
types={"DATA.r0": "Regex", "DATA.r1": "Regex", "DATA.r2": "Regex", "DATA.r3": "Regex", "DATA.r4": "Regex"}
vars={"r0": "VariableInfo { name: \"r0\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "r1": "VariableInfo { name: \"r1\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "r2": "VariableInfo { name: \"r2\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "r3": "VariableInfo { name: \"r3\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "r4": "VariableInfo { name: \"r4\", declared_type: None, inferred_type: Some(Regex), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_tuple_forms|halt
success=false
E [DATA4A5] TUPLE_TOO_LARGE | Tuple in property 't7' has 7 elements (maximum is 6) | Use an array or object for more than 6 elements | Some(Position { line: 7, column: 1 })
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at property 'tn'[1] | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
short={"t0": ["DATA.t0"], "t6": ["DATA.t6"], "t7": ["DATA.t7"], "tn": ["DATA.tn"]}
types={"DATA.t0": "Tuple", "DATA.t6": "Tuple", "DATA.t7": "Tuple", "DATA.tn": "Tuple"}
vars={"t0": "VariableInfo { name: \"t0\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "t6": "VariableInfo { name: \"t6\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "t7": "VariableInfo { name: \"t7\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "tn": "VariableInfo { name: \"tn\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_tuple_forms|continue
success=false
E [DATA4A5] TUPLE_TOO_LARGE | Tuple in property 't7' has 7 elements (maximum is 6) | Use an array or object for more than 6 elements | Some(Position { line: 7, column: 1 })
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at property 'tn'[1] | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
short={"t0": ["DATA.t0"], "t6": ["DATA.t6"], "t7": ["DATA.t7"], "tn": ["DATA.tn"]}
types={"DATA.t0": "Tuple", "DATA.t6": "Tuple", "DATA.t7": "Tuple", "DATA.tn": "Tuple"}
vars={"t0": "VariableInfo { name: \"t0\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "t6": "VariableInfo { name: \"t6\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "t7": "VariableInfo { name: \"t7\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "tn": "VariableInfo { name: \"tn\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_tuple_forms|recover
success=false
E [DATA4A5] TUPLE_TOO_LARGE | Tuple in property 't7' has 7 elements (maximum is 6) | Use an array or object for more than 6 elements | Some(Position { line: 7, column: 1 })
E [DATA576] INVALID_EXPRESSION | Expression values not allowed in DATA section at property 'tn'[1] | Use only literals, function calls, or enum values | Some(Position { line: 8, column: 1 })
short={"t0": ["DATA.t0"], "t6": ["DATA.t6"], "t7": ["DATA.t7"], "tn": ["DATA.tn"]}
types={"DATA.t0": "Tuple", "DATA.t6": "Tuple", "DATA.t7": "Tuple", "DATA.tn": "Tuple"}
vars={"t0": "VariableInfo { name: \"t0\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "t6": "VariableInfo { name: \"t6\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "t7": "VariableInfo { name: \"t7\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "tn": "VariableInfo { name: \"tn\", declared_type: None, inferred_type: Some(Tuple), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_unknown_prefix|halt
success=true
short={"z": ["DATA.z"], "zz": ["DATA.zz"]}
types={"DATA.z": "Any", "DATA.zz": "Any"}
vars={"z": "VariableInfo { name: \"z\", declared_type: None, inferred_type: Some(Any), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "zz": "VariableInfo { name: \"zz\", declared_type: Some(Int), inferred_type: Some(Any), is_inferred: false, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_unknown_prefix|continue
success=true
short={"z": ["DATA.z"], "zz": ["DATA.zz"]}
types={"DATA.z": "Any", "DATA.zz": "Any"}
vars={"z": "VariableInfo { name: \"z\", declared_type: None, inferred_type: Some(Any), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "zz": "VariableInfo { name: \"zz\", declared_type: Some(Int), inferred_type: Some(Any), is_inferred: false, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_unknown_prefix|recover
success=true
short={"z": ["DATA.z"], "zz": ["DATA.zz"]}
types={"DATA.z": "Any", "DATA.zz": "Any"}
vars={"z": "VariableInfo { name: \"z\", declared_type: None, inferred_type: Some(Any), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "zz": "VariableInfo { name: \"zz\", declared_type: Some(Int), inferred_type: Some(Any), is_inferred: false, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_nested_array|halt
success=false
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Array in property 'b' contains mixed types: Int and String | All array elements must be the same type. | Some(Position { line: 1, column: 3 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"]}
types={"DATA.a": "Array", "DATA.b": "Array", "DATA.c": "Array", "DATA.d": "Array"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_nested_array|continue
success=false
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Array in property 'b' contains mixed types: Int and String | All array elements must be the same type. | Some(Position { line: 1, column: 3 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"]}
types={"DATA.a": "Array", "DATA.b": "Array", "DATA.c": "Array", "DATA.d": "Array"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_nested_array|recover
success=false
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Array in property 'b' contains mixed types: Int and String | All array elements must be the same type. | Some(Position { line: 1, column: 3 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"]}
types={"DATA.a": "Array", "DATA.b": "Array", "DATA.c": "Array", "DATA.d": "Array"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_enum_forms|halt
success=false
E [DATA460] ENUM_NOT_FOUND | Enum '' not found in property 'g' | Check @ENUMS section for available enums | Some(Position { line: 9, column: 1 })
E [DATA460] ENUM_NOT_FOUND | Enum 'Nope' not found in property 'c' | Check @ENUMS section for available enums | Some(Position { line: 9, column: 1 })
E [DATA460] ENUM_NOT_FOUND | Namespace 'A' not found — import it with @IMPORTS in property 'f' | Add: A from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 9, column: 1 })
E [DATA460] ENUM_NOT_FOUND | Namespace 'Base' not found — import it with @IMPORTS in property 'd' | Add: Base from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 9, column: 1 })
E [DATA460] ENUM_NOT_FOUND | Namespace 'Base' not found — import it with @IMPORTS in property 'e' | Add: Base from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 9, column: 1 })
E [DATA63C] ENUM_VALUE_NOT_FOUND | Enum value 'LogLevel.NOPE' not found in property 'b' | Check available values in enum 'LogLevel' | Some(Position { line: 9, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"], "g": ["DATA.g"]}
types={"DATA.a": "Enum", "DATA.b": "Enum", "DATA.c": "Enum", "DATA.d": "Enum", "DATA.e": "Enum", "DATA.f": "Enum", "DATA.g": "Enum"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "e": "VariableInfo { name: \"e\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "f": "VariableInfo { name: \"f\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "g": "VariableInfo { name: \"g\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_enum_forms|continue
success=false
E [DATA460] ENUM_NOT_FOUND | Enum '' not found in property 'g' | Check @ENUMS section for available enums | Some(Position { line: 9, column: 1 })
E [DATA460] ENUM_NOT_FOUND | Enum 'Nope' not found in property 'c' | Check @ENUMS section for available enums | Some(Position { line: 9, column: 1 })
E [DATA460] ENUM_NOT_FOUND | Namespace 'A' not found — import it with @IMPORTS in property 'f' | Add: A from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 9, column: 1 })
E [DATA460] ENUM_NOT_FOUND | Namespace 'Base' not found — import it with @IMPORTS in property 'd' | Add: Base from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 9, column: 1 })
E [DATA460] ENUM_NOT_FOUND | Namespace 'Base' not found — import it with @IMPORTS in property 'e' | Add: Base from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 9, column: 1 })
E [DATA63C] ENUM_VALUE_NOT_FOUND | Enum value 'LogLevel.NOPE' not found in property 'b' | Check available values in enum 'LogLevel' | Some(Position { line: 9, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"], "g": ["DATA.g"]}
types={"DATA.a": "Enum", "DATA.b": "Enum", "DATA.c": "Enum", "DATA.d": "Enum", "DATA.e": "Enum", "DATA.f": "Enum", "DATA.g": "Enum"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "e": "VariableInfo { name: \"e\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "f": "VariableInfo { name: \"f\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "g": "VariableInfo { name: \"g\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_enum_forms|recover
success=false
E [DATA460] ENUM_NOT_FOUND | Enum '' not found in property 'g' | Check @ENUMS section for available enums | Some(Position { line: 9, column: 1 })
E [DATA460] ENUM_NOT_FOUND | Enum 'Nope' not found in property 'c' | Check @ENUMS section for available enums | Some(Position { line: 9, column: 1 })
E [DATA460] ENUM_NOT_FOUND | Namespace 'A' not found — import it with @IMPORTS in property 'f' | Add: A from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 9, column: 1 })
E [DATA460] ENUM_NOT_FOUND | Namespace 'Base' not found — import it with @IMPORTS in property 'd' | Add: Base from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 9, column: 1 })
E [DATA460] ENUM_NOT_FOUND | Namespace 'Base' not found — import it with @IMPORTS in property 'e' | Add: Base from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 9, column: 1 })
E [DATA63C] ENUM_VALUE_NOT_FOUND | Enum value 'LogLevel.NOPE' not found in property 'b' | Check available values in enum 'LogLevel' | Some(Position { line: 9, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"], "g": ["DATA.g"]}
types={"DATA.a": "Enum", "DATA.b": "Enum", "DATA.c": "Enum", "DATA.d": "Enum", "DATA.e": "Enum", "DATA.f": "Enum", "DATA.g": "Enum"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "e": "VariableInfo { name: \"e\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "f": "VariableInfo { name: \"f\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "g": "VariableInfo { name: \"g\", declared_type: None, inferred_type: Some(Enum), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_function_forms|halt
success=false
E [DATA591] FUNCTION_NOT_FOUND | Function '' not found in property 'd' | Check @QUICKFUNCS section or imports for available functions | Some(Position { line: 10, column: 1 })
E [DATA591] FUNCTION_NOT_FOUND | Function 'f' not found in property 'a' | Check @QUICKFUNCS section or imports for available functions | Some(Position { line: 10, column: 1 })
E [DATA591] FUNCTION_NOT_FOUND | Function 'f' not found in property 'e' | Check @QUICKFUNCS section or imports for available functions | Some(Position { line: 10, column: 1 })
E [DATA591] FUNCTION_NOT_FOUND | Namespace 'A' not found — import it with @IMPORTS in property 'c' | Add: A from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 10, column: 1 })
E [DATA591] FUNCTION_NOT_FOUND | Namespace 'Ns' not found — import it with @IMPORTS in property 'b' | Add: Ns from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 10, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"]}
types={"DATA.e": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "e": "VariableInfo { name: \"e\", declared_type: Some(Int), inferred_type: None, is_inferred: false, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_function_forms|continue
success=false
E [DATA591] FUNCTION_NOT_FOUND | Function '' not found in property 'd' | Check @QUICKFUNCS section or imports for available functions | Some(Position { line: 10, column: 1 })
E [DATA591] FUNCTION_NOT_FOUND | Function 'f' not found in property 'a' | Check @QUICKFUNCS section or imports for available functions | Some(Position { line: 10, column: 1 })
E [DATA591] FUNCTION_NOT_FOUND | Function 'f' not found in property 'e' | Check @QUICKFUNCS section or imports for available functions | Some(Position { line: 10, column: 1 })
E [DATA591] FUNCTION_NOT_FOUND | Namespace 'A' not found — import it with @IMPORTS in property 'c' | Add: A from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 10, column: 1 })
E [DATA591] FUNCTION_NOT_FOUND | Namespace 'Ns' not found — import it with @IMPORTS in property 'b' | Add: Ns from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 10, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"]}
types={"DATA.e": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "e": "VariableInfo { name: \"e\", declared_type: Some(Int), inferred_type: None, is_inferred: false, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_function_forms|recover
success=false
E [DATA591] FUNCTION_NOT_FOUND | Function '' not found in property 'd' | Check @QUICKFUNCS section or imports for available functions | Some(Position { line: 10, column: 1 })
E [DATA591] FUNCTION_NOT_FOUND | Function 'f' not found in property 'a' | Check @QUICKFUNCS section or imports for available functions | Some(Position { line: 10, column: 1 })
E [DATA591] FUNCTION_NOT_FOUND | Function 'f' not found in property 'e' | Check @QUICKFUNCS section or imports for available functions | Some(Position { line: 10, column: 1 })
E [DATA591] FUNCTION_NOT_FOUND | Namespace 'A' not found — import it with @IMPORTS in property 'c' | Add: A from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 10, column: 1 })
E [DATA591] FUNCTION_NOT_FOUND | Namespace 'Ns' not found — import it with @IMPORTS in property 'b' | Add: Ns from "path/to/file.mdix" in @IMPORTS | Some(Position { line: 10, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"]}
types={"DATA.e": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "e": "VariableInfo { name: \"e\", declared_type: Some(Int), inferred_type: None, is_inferred: false, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_misc_values|halt
success=true
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"]}
types={"DATA.b": "Range", "DATA.e": "Function", "DATA.f": "String"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Range), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "e": "VariableInfo { name: \"e\", declared_type: None, inferred_type: Some(Function), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "f": "VariableInfo { name: \"f\", declared_type: None, inferred_type: Some(String), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_misc_values|continue
success=true
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"]}
types={"DATA.b": "Range", "DATA.e": "Function", "DATA.f": "String"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Range), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "e": "VariableInfo { name: \"e\", declared_type: None, inferred_type: Some(Function), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "f": "VariableInfo { name: \"f\", declared_type: None, inferred_type: Some(String), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_misc_values|recover
success=true
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"]}
types={"DATA.b": "Range", "DATA.e": "Function", "DATA.f": "String"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Range), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: None, is_inferred: true, scope: \"global\", line: 2, column: 1 }", "e": "VariableInfo { name: \"e\", declared_type: None, inferred_type: Some(Function), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "f": "VariableInfo { name: \"f\", declared_type: None, inferred_type: Some(String), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_deep_object|halt
success=true
short={"b": ["DATA.o.b"], "c": ["DATA.o.c"], "d": ["DATA.o.d"], "o": ["DATA.o"]}
types={"DATA.o": "Object", "DATA.o.b": "Object", "DATA.o.c": "Object", "DATA.o.d": "Int"}
vars={"o": "VariableInfo { name: \"o\", declared_type: None, inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_deep_object|continue
success=true
short={"b": ["DATA.o.b"], "c": ["DATA.o.c"], "d": ["DATA.o.d"], "o": ["DATA.o"]}
types={"DATA.o": "Object", "DATA.o.b": "Object", "DATA.o.c": "Object", "DATA.o.d": "Int"}
vars={"o": "VariableInfo { name: \"o\", declared_type: None, inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_deep_object|recover
success=true
short={"b": ["DATA.o.b"], "c": ["DATA.o.c"], "d": ["DATA.o.d"], "o": ["DATA.o"]}
types={"DATA.o": "Object", "DATA.o.b": "Object", "DATA.o.c": "Object", "DATA.o.d": "Int"}
vars={"o": "VariableInfo { name: \"o\", declared_type: None, inferred_type: Some(Object), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_object_in_group|halt
success=false
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Group array 'g' contains mixed types: Object and Int | All items in a group array must be the same type. | Some(Position { line: 1, column: 2 })
short={"a": ["g[0].a", "g[1].a"], "b": ["g[0].b"]}
types={}
vars={"g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 1 }"}
logged=[]
@hand_object_in_group|continue
success=false
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Group array 'g' contains mixed types: Object and Int | All items in a group array must be the same type. | Some(Position { line: 1, column: 2 })
short={"a": ["g[0].a", "g[1].a"], "b": ["g[0].b"]}
types={}
vars={"g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 1 }"}
logged=[]
@hand_object_in_group|recover
success=false
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Group array 'g' contains mixed types: Object and Int | All items in a group array must be the same type. | Some(Position { line: 1, column: 2 })
short={"a": ["g[0].a", "g[1].a"], "b": ["g[0].b"]}
types={}
vars={"g": "VariableInfo { name: \"g\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 1 }"}
logged=[]
@hand_table_with_object_values|halt
success=false
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Array in table property 'w.q.arr' contains mixed types: Int and String | All array elements must be the same type. | Some(Position { line: 1, column: 3 })
short={"a": ["DATA.w.q.a"], "arr": ["w.q.arr"], "b": ["DATA.w.q.b"], "c": ["DATA.w.q.o.c"], "n": ["w.q.n"], "o": ["w.q.o"]}
types={"DATA.w.q.a": "Int", "DATA.w.q.b": "Object", "DATA.w.q.o.c": "Int", "w.q.arr": "Array", "w.q.n": "Int", "w.q.o": "Object"}
vars={"w.q.arr": "VariableInfo { name: \"w.q.arr\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"w.q\", line: 3, column: 1 }", "w.q.n": "VariableInfo { name: \"w.q.n\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"w.q\", line: 3, column: 1 }", "w.q.o": "VariableInfo { name: \"w.q.o\", declared_type: None, inferred_type: Some(Object), is_inferred: true, scope: \"w.q\", line: 3, column: 1 }"}
logged=[]
@hand_table_with_object_values|continue
success=false
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Array in table property 'w.q.arr' contains mixed types: Int and String | All array elements must be the same type. | Some(Position { line: 1, column: 3 })
short={"a": ["DATA.w.q.a"], "arr": ["w.q.arr"], "b": ["DATA.w.q.b"], "c": ["DATA.w.q.o.c"], "n": ["w.q.n"], "o": ["w.q.o"]}
types={"DATA.w.q.a": "Int", "DATA.w.q.b": "Object", "DATA.w.q.o.c": "Int", "w.q.arr": "Array", "w.q.n": "Int", "w.q.o": "Object"}
vars={"w.q.arr": "VariableInfo { name: \"w.q.arr\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"w.q\", line: 3, column: 1 }", "w.q.n": "VariableInfo { name: \"w.q.n\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"w.q\", line: 3, column: 1 }", "w.q.o": "VariableInfo { name: \"w.q.o\", declared_type: None, inferred_type: Some(Object), is_inferred: true, scope: \"w.q\", line: 3, column: 1 }"}
logged=[]
@hand_table_with_object_values|recover
success=false
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Array in table property 'w.q.arr' contains mixed types: Int and String | All array elements must be the same type. | Some(Position { line: 1, column: 3 })
short={"a": ["DATA.w.q.a"], "arr": ["w.q.arr"], "b": ["DATA.w.q.b"], "c": ["DATA.w.q.o.c"], "n": ["w.q.n"], "o": ["w.q.o"]}
types={"DATA.w.q.a": "Int", "DATA.w.q.b": "Object", "DATA.w.q.o.c": "Int", "w.q.arr": "Array", "w.q.n": "Int", "w.q.o": "Object"}
vars={"w.q.arr": "VariableInfo { name: \"w.q.arr\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"w.q\", line: 3, column: 1 }", "w.q.n": "VariableInfo { name: \"w.q.n\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"w.q\", line: 3, column: 1 }", "w.q.o": "VariableInfo { name: \"w.q.o\", declared_type: None, inferred_type: Some(Object), is_inferred: true, scope: \"w.q\", line: 3, column: 1 }"}
logged=[]
@hand_dup_names_across_forms|halt
success=false
E [DATA5EE] DUPLICATE_TABLE_PATH | Table property path 'a' is defined multiple times | Combine assignments into a single table property or use different paths | Some(Position { line: 3, column: 1 })
E [DATA705] DUPLICATE_FLAT_PROPERTY | Flat property 'a' is defined more than once in @DATA. Each flat property name must be unique. | Remove or rename the duplicate 'a' property. Note: table properties (path:) and group arrays (path::) may share this name. | Some(Position { line: 2, column: 1 })
E [DATA705] DUPLICATE_FLAT_PROPERTY | Flat property 'a' is defined more than once in @DATA. Each flat property name must be unique. | Remove or rename the duplicate 'a' property. | Some(Position { line: 11, column: 1 })
short={"a": ["DATA.a"], "x": ["a.x", "a.x"]}
types={"DATA.a": "Int", "a.x": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 1 }", "a.x": "VariableInfo { name: \"a.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"a\", line: 3, column: 1 }"}
logged=[]
@hand_dup_names_across_forms|continue
success=false
E [DATA5EE] DUPLICATE_TABLE_PATH | Table property path 'a' is defined multiple times | Combine assignments into a single table property or use different paths | Some(Position { line: 3, column: 1 })
E [DATA705] DUPLICATE_FLAT_PROPERTY | Flat property 'a' is defined more than once in @DATA. Each flat property name must be unique. | Remove or rename the duplicate 'a' property. Note: table properties (path:) and group arrays (path::) may share this name. | Some(Position { line: 2, column: 1 })
E [DATA705] DUPLICATE_FLAT_PROPERTY | Flat property 'a' is defined more than once in @DATA. Each flat property name must be unique. | Remove or rename the duplicate 'a' property. | Some(Position { line: 11, column: 1 })
short={"a": ["DATA.a"], "x": ["a.x", "a.x"]}
types={"DATA.a": "Int", "a.x": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 1 }", "a.x": "VariableInfo { name: \"a.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"a\", line: 3, column: 1 }"}
logged=[]
@hand_dup_names_across_forms|recover
success=false
E [DATA5EE] DUPLICATE_TABLE_PATH | Table property path 'a' is defined multiple times | Combine assignments into a single table property or use different paths | Some(Position { line: 3, column: 1 })
E [DATA705] DUPLICATE_FLAT_PROPERTY | Flat property 'a' is defined more than once in @DATA. Each flat property name must be unique. | Remove or rename the duplicate 'a' property. Note: table properties (path:) and group arrays (path::) may share this name. | Some(Position { line: 2, column: 1 })
E [DATA705] DUPLICATE_FLAT_PROPERTY | Flat property 'a' is defined more than once in @DATA. Each flat property name must be unique. | Remove or rename the duplicate 'a' property. | Some(Position { line: 11, column: 1 })
short={"a": ["DATA.a"], "x": ["a.x", "a.x"]}
types={"DATA.a": "Int", "a.x": "Int"}
vars={"a": "VariableInfo { name: \"a\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 4, column: 1 }", "a.x": "VariableInfo { name: \"a.x\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"a\", line: 3, column: 1 }"}
logged=[]
@hand_reserved_names|halt
success=false
E [DATA4E4] RESERVED_KEYWORD | 'if' is a reserved keyword and cannot be used as an identifier | Choose a different name for property 'if' | Some(Position { line: 2, column: 1 })
E [DATA4E4] RESERVED_KEYWORD | 'let' is a reserved keyword and cannot be used as an identifier | Choose a different name for property 't.let' | Some(Position { line: 3, column: 1 })
E [DATA4E4] RESERVED_KEYWORD | 'return' is a reserved keyword and cannot be used as an identifier | Choose a different name for property 'return' | Some(Position { line: 2, column: 1 })
short={"ok": ["t.ok"]}
types={"t.ok": "Int"}
vars={"t.ok": "VariableInfo { name: \"t.ok\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 3, column: 1 }"}
logged=[]
@hand_reserved_names|continue
success=false
E [DATA4E4] RESERVED_KEYWORD | 'if' is a reserved keyword and cannot be used as an identifier | Choose a different name for property 'if' | Some(Position { line: 2, column: 1 })
E [DATA4E4] RESERVED_KEYWORD | 'let' is a reserved keyword and cannot be used as an identifier | Choose a different name for property 't.let' | Some(Position { line: 3, column: 1 })
E [DATA4E4] RESERVED_KEYWORD | 'return' is a reserved keyword and cannot be used as an identifier | Choose a different name for property 'return' | Some(Position { line: 2, column: 1 })
short={"ok": ["t.ok"]}
types={"t.ok": "Int"}
vars={"t.ok": "VariableInfo { name: \"t.ok\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 3, column: 1 }"}
logged=[]
@hand_reserved_names|recover
success=false
E [DATA4E4] RESERVED_KEYWORD | 'if' is a reserved keyword and cannot be used as an identifier | Choose a different name for property 'if' | Some(Position { line: 2, column: 1 })
E [DATA4E4] RESERVED_KEYWORD | 'let' is a reserved keyword and cannot be used as an identifier | Choose a different name for property 't.let' | Some(Position { line: 3, column: 1 })
E [DATA4E4] RESERVED_KEYWORD | 'return' is a reserved keyword and cannot be used as an identifier | Choose a different name for property 'return' | Some(Position { line: 2, column: 1 })
short={"ok": ["t.ok"]}
types={"t.ok": "Int"}
vars={"t.ok": "VariableInfo { name: \"t.ok\", declared_type: None, inferred_type: Some(Int), is_inferred: true, scope: \"t\", line: 3, column: 1 }"}
logged=[]
@hand_typed_everything|halt
success=false
E [DATA3F7] TYPE_MISMATCH | Property 'g' is declared as <string> but the value is <int>. These types are not compatible. | Either change the type annotation to <int> or provide a value of type <string>. | Some(Position { line: 2, column: 1 })
E [DATA3F7] TYPE_MISMATCH | Property 'j' is declared as <date> but the value is <timestamp>. These types are not compatible. | Either change the type annotation to <timestamp> or provide a value of type <date>. | Some(Position { line: 2, column: 1 })
E [DATA3F7] TYPE_MISMATCH | Property 'k' is declared as <timestamp> but the value is <date>. These types are not compatible. | Either change the type annotation to <date> or provide a value of type <timestamp>. | Some(Position { line: 2, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"], "g": ["DATA.g"], "h": ["DATA.h"], "i": ["DATA.i"], "j": ["DATA.j"], "k": ["DATA.k"], "l": ["DATA.l"]}
types={"DATA.a": "Int", "DATA.b": "Long", "DATA.c": "Float", "DATA.d": "Double", "DATA.e": "Int", "DATA.f": "String", "DATA.g": "Int", "DATA.h": "Array", "DATA.i": "Object", "DATA.j": "Timestamp", "DATA.k": "Date", "DATA.l": "Bool"}
vars={"a": "VariableInfo { name: \"a\", declared_type: Some(Int), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: Some(Long), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: Some(Float), inferred_type: Some(Double), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: Some(Double), inferred_type: Some(Float), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "e": "VariableInfo { name: \"e\", declared_type: Some(Hex), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "f": "VariableInfo { name: \"f\", declared_type: Some(Any), inferred_type: Some(String), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "g": "VariableInfo { name: \"g\", declared_type: Some(String), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "h": "VariableInfo { name: \"h\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "i": "VariableInfo { name: \"i\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "j": "VariableInfo { name: \"j\", declared_type: Some(Date), inferred_type: Some(Timestamp), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "k": "VariableInfo { name: \"k\", declared_type: Some(Timestamp), inferred_type: Some(Date), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "l": "VariableInfo { name: \"l\", declared_type: Some(Bool), inferred_type: Some(Bool), is_inferred: false, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_typed_everything|continue
success=false
E [DATA3F7] TYPE_MISMATCH | Property 'g' is declared as <string> but the value is <int>. These types are not compatible. | Either change the type annotation to <int> or provide a value of type <string>. | Some(Position { line: 2, column: 1 })
E [DATA3F7] TYPE_MISMATCH | Property 'j' is declared as <date> but the value is <timestamp>. These types are not compatible. | Either change the type annotation to <timestamp> or provide a value of type <date>. | Some(Position { line: 2, column: 1 })
E [DATA3F7] TYPE_MISMATCH | Property 'k' is declared as <timestamp> but the value is <date>. These types are not compatible. | Either change the type annotation to <date> or provide a value of type <timestamp>. | Some(Position { line: 2, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"], "g": ["DATA.g"], "h": ["DATA.h"], "i": ["DATA.i"], "j": ["DATA.j"], "k": ["DATA.k"], "l": ["DATA.l"]}
types={"DATA.a": "Int", "DATA.b": "Long", "DATA.c": "Float", "DATA.d": "Double", "DATA.e": "Int", "DATA.f": "String", "DATA.g": "Int", "DATA.h": "Array", "DATA.i": "Object", "DATA.j": "Timestamp", "DATA.k": "Date", "DATA.l": "Bool"}
vars={"a": "VariableInfo { name: \"a\", declared_type: Some(Int), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: Some(Long), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: Some(Float), inferred_type: Some(Double), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: Some(Double), inferred_type: Some(Float), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "e": "VariableInfo { name: \"e\", declared_type: Some(Hex), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "f": "VariableInfo { name: \"f\", declared_type: Some(Any), inferred_type: Some(String), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "g": "VariableInfo { name: \"g\", declared_type: Some(String), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "h": "VariableInfo { name: \"h\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "i": "VariableInfo { name: \"i\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "j": "VariableInfo { name: \"j\", declared_type: Some(Date), inferred_type: Some(Timestamp), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "k": "VariableInfo { name: \"k\", declared_type: Some(Timestamp), inferred_type: Some(Date), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "l": "VariableInfo { name: \"l\", declared_type: Some(Bool), inferred_type: Some(Bool), is_inferred: false, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_typed_everything|recover
success=false
E [DATA3F7] TYPE_MISMATCH | Property 'g' is declared as <string> but the value is <int>. These types are not compatible. | Either change the type annotation to <int> or provide a value of type <string>. | Some(Position { line: 2, column: 1 })
E [DATA3F7] TYPE_MISMATCH | Property 'j' is declared as <date> but the value is <timestamp>. These types are not compatible. | Either change the type annotation to <timestamp> or provide a value of type <date>. | Some(Position { line: 2, column: 1 })
E [DATA3F7] TYPE_MISMATCH | Property 'k' is declared as <timestamp> but the value is <date>. These types are not compatible. | Either change the type annotation to <date> or provide a value of type <timestamp>. | Some(Position { line: 2, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"], "e": ["DATA.e"], "f": ["DATA.f"], "g": ["DATA.g"], "h": ["DATA.h"], "i": ["DATA.i"], "j": ["DATA.j"], "k": ["DATA.k"], "l": ["DATA.l"]}
types={"DATA.a": "Int", "DATA.b": "Long", "DATA.c": "Float", "DATA.d": "Double", "DATA.e": "Int", "DATA.f": "String", "DATA.g": "Int", "DATA.h": "Array", "DATA.i": "Object", "DATA.j": "Timestamp", "DATA.k": "Date", "DATA.l": "Bool"}
vars={"a": "VariableInfo { name: \"a\", declared_type: Some(Int), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: Some(Long), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: Some(Float), inferred_type: Some(Double), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: Some(Double), inferred_type: Some(Float), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "e": "VariableInfo { name: \"e\", declared_type: Some(Hex), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "f": "VariableInfo { name: \"f\", declared_type: Some(Any), inferred_type: Some(String), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "g": "VariableInfo { name: \"g\", declared_type: Some(String), inferred_type: Some(Int), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "h": "VariableInfo { name: \"h\", declared_type: Some(Array), inferred_type: Some(Array), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "i": "VariableInfo { name: \"i\", declared_type: Some(Object), inferred_type: Some(Object), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "j": "VariableInfo { name: \"j\", declared_type: Some(Date), inferred_type: Some(Timestamp), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "k": "VariableInfo { name: \"k\", declared_type: Some(Timestamp), inferred_type: Some(Date), is_inferred: false, scope: \"global\", line: 2, column: 1 }", "l": "VariableInfo { name: \"l\", declared_type: Some(Bool), inferred_type: Some(Bool), is_inferred: false, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_empty_section|halt
success=true
short={}
types={}
vars={}
logged=[]
@hand_empty_section|continue
success=true
short={}
types={}
vars={}
logged=[]
@hand_empty_section|recover
success=true
short={}
types={}
vars={}
logged=[]
@hand_empty_group|halt
success=true
W [DATAW86C] Group array 'g' is empty | Some(Position { line: 4, column: 1 })
short={}
types={}
vars={}
logged=[]
@hand_empty_group|continue
success=true
W [DATAW86C] Group array 'g' is empty | Some(Position { line: 4, column: 1 })
short={}
types={}
vars={}
logged=[]
@hand_empty_group|recover
success=true
W [DATAW86C] Group array 'g' is empty | Some(Position { line: 4, column: 1 })
short={}
types={}
vars={}
logged=[]
@hand_long_double_sci|halt
success=false
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Array in property 'd' contains mixed types: Long and Double | All array elements must be the same type. | Some(Position { line: 1, column: 1 })
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Array in property 'd' contains mixed types: Long and Int | All array elements must be the same type. | Some(Position { line: 1, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"]}
types={"DATA.a": "Long", "DATA.b": "Double", "DATA.c": "Hex", "DATA.d": "Array"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Long), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Double), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Hex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_long_double_sci|continue
success=false
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Array in property 'd' contains mixed types: Long and Double | All array elements must be the same type. | Some(Position { line: 1, column: 1 })
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Array in property 'd' contains mixed types: Long and Int | All array elements must be the same type. | Some(Position { line: 1, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"]}
types={"DATA.a": "Long", "DATA.b": "Double", "DATA.c": "Hex", "DATA.d": "Array"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Long), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Double), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Hex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
@hand_long_double_sci|recover
success=false
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Array in property 'd' contains mixed types: Long and Double | All array elements must be the same type. | Some(Position { line: 1, column: 1 })
E [DATA677] ARRAY_NOT_HOMOGENEOUS | Array in property 'd' contains mixed types: Long and Int | All array elements must be the same type. | Some(Position { line: 1, column: 1 })
short={"a": ["DATA.a"], "b": ["DATA.b"], "c": ["DATA.c"], "d": ["DATA.d"]}
types={"DATA.a": "Long", "DATA.b": "Double", "DATA.c": "Hex", "DATA.d": "Array"}
vars={"a": "VariableInfo { name: \"a\", declared_type: None, inferred_type: Some(Long), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "b": "VariableInfo { name: \"b\", declared_type: None, inferred_type: Some(Double), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "c": "VariableInfo { name: \"c\", declared_type: None, inferred_type: Some(Hex), is_inferred: true, scope: \"global\", line: 2, column: 1 }", "d": "VariableInfo { name: \"d\", declared_type: None, inferred_type: Some(Array), is_inferred: true, scope: \"global\", line: 2, column: 1 }"}
logged=[]
"##;

/// The index and variable lines of a result, in the form `RECORDING` stores them.
fn index_lines(result: &SemanticAnalysisResult) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(idx) = &result.short_name_index {
        let sorted: BTreeMap<_, _> = idx
            .iter()
            .map(|(k, v)| {
                let mut v = v.clone();
                v.sort();
                (k.clone(), v)
            })
            .collect();
        lines.push(format!("short_name_index={:?}", sorted));
    }
    if let Some(idx) = &result.type_index {
        let sorted: BTreeMap<_, _> = idx.iter().map(|(k, v)| (k.clone(), format!("{:?}", v))).collect();
        lines.push(format!("type_index={:?}", sorted));
    }
    if let Some(table) = &result.symbol_table {
        let vars: BTreeMap<_, _> = table.data_variables.iter().map(|(k, v)| (k.clone(), format!("{:?}", v))).collect();
        lines.push(format!("data_variables={:?}", vars));
    }
    lines
}

/// The @DATA lines of a result as `RECORDING` stores them: the section's diagnostics sorted,
/// then the indexes and variables.
fn data_case_lines(result: &SemanticAnalysisResult) -> Vec<String> {
    let mut lines = section_lines(result, "DATA");
    lines.extend(index_lines(result));
    lines
}

// ==================== the two paths agree ====================

#[test]
fn document_path_and_owned_path_give_identical_diagnostics() {
    let mut compared = 0;
    for (name, text) in SOURCES {
        for strategy_name in STRATEGIES {
            let parsed = parse(text, strategy(strategy_name))
                .unwrap_or_else(|| panic!("{name} [{strategy_name}] should parse"));
            let (with_doc, logged_doc) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
            let (without, logged_owned) = analyze(&parsed.ast, &parsed.settings, None);
            assert_eq!(
                describe(&with_doc, &logged_doc),
                describe(&without, &logged_owned),
                "{name} [{strategy_name}]: the document path and the owned path disagree"
            );
            assert_eq!(index_lines(&with_doc), index_lines(&without), "{name} [{strategy_name}]: indexes");
            compared += 1;
        }
    }
    assert_eq!(compared, SOURCES.len() * 3);
}

// ==================== the recorded behaviour ====================

#[test]
fn every_source_has_a_recorded_case() {
    let recorded = recorded_cases();
    assert_eq!(recorded.len(), EXPECTED_CASES);
    for (name, _) in SOURCES {
        assert!(recorded.iter().any(|(n, _, _, _)| n == name), "{name} has no recorded case");
    }
}

#[test]
fn data_diagnostics_indexes_and_variables_match_the_recording() {
    for (name, strategy_name, success, expected) in recorded_cases() {
        let parsed = parse(source(&name), strategy(&strategy_name)).expect("parses");
        let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
        let got = data_case_lines(&result);
        assert_eq!(got, expected, "{name} [{strategy_name}]");
        assert_eq!(result.is_success, success, "{name} [{strategy_name}]: overall success");
    }
}

/// The @DATA lines of one source under `continue`, read through the document.
fn lines_of(name: &str) -> Vec<String> {
    let parsed = parse(source(name), ErrorHandlingStrategy::Continue).expect("parses");
    let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
    section_lines(&result, "DATA")
}

#[test]
fn nesting_is_allowed_to_twelve_levels_and_refused_at_thirteen() {
    assert_eq!(lines_of("data_nesting_11"), Vec::<String>::new());
    assert_eq!(lines_of("data_nesting_12"), Vec::<String>::new());
    assert!(lines_of("data_nesting_13").iter().any(|l| l.starts_with("E|DATA") && l.contains("Nesting depth exceeds maximum of 12")));
    assert!(lines_of("data_nesting_obj_13").iter().any(|l| l.contains("Nesting depth exceeds maximum of 12")));
    assert_eq!(lines_of("data_nesting_obj_6"), Vec::<String>::new());
}

#[test]
fn the_first_duplicate_flat_name_is_the_one_kept() {
    // `a = 1, a = 2` reports the second; the object and the plain value of the same name are
    // one flat tier, so `b = {x = 1}` then `b = 3` is a duplicate as well.
    let lines = lines_of("data_dup_flat");
    assert_eq!(lines.iter().filter(|l| l.contains("is defined more than once in @DATA")).count(), 3, "{lines:?}");
}

#[test]
fn tuples_are_limited_to_six_elements() {
    let lines = lines_of("data_tuple_sizes");
    assert_eq!(lines.iter().filter(|l| l.contains("has 7 elements (maximum is 6)")).count(), 1, "{lines:?}");
    assert!(!lines.iter().any(|l| l.contains("has 6 elements")), "{lines:?}");
}

// ==================== sections built by hand ====================

fn p() -> Position {
    Position::new(1, 1)
}

fn int(n: i32) -> Value { Value::Integer { value: n, position: Position::new(1, 2) } }
fn st(t: &str) -> Value { Value::String { value: t.to_string(), position: Position::new(1, 3) } }
fn simple(name: &str, value: Value) -> DataEntry { DataEntry::SimpleProperty { name: name.to_string(), data_type: None, value, position: Position::new(2, 1) } }
fn typed(name: &str, t: DataType, value: Value) -> DataEntry { DataEntry::SimpleProperty { name: name.to_string(), data_type: Some(t), value, position: Position::new(2, 1) } }
fn table(path: &[&str], props: Vec<(&str, Value)>) -> DataEntry {
    DataEntry::TableProperty {
        path: TablePath::new(path.iter().map(|x| x.to_string()).collect()),
        properties: props.into_iter().map(|(n, v)| PropertyAssignment::new(n.to_string(), None, v, Position::new(3, 1))).collect(),
        position: Position::new(3, 1),
    }
}
fn group(path: &[&str], items: Vec<Value>) -> DataEntry {
    DataEntry::GroupArray { path: TablePath::new(path.iter().map(|x| x.to_string()).collect()), items, position: Position::new(4, 1) }
}
fn obj(props: Vec<(&str, Value)>) -> Value {
    Value::Object { properties: props.into_iter().map(|(k, v)| ObjectProperty { key: k.to_string(), value: v, position: Position::new(5, 1) }).collect(), position: Position::new(5, 0) }
}
fn arr(values: Vec<Value>) -> Value { Value::Array { values, position: Position::new(6, 1) } }
fn nested(values: Vec<Value>) -> Value { Value::NestedArray { values, level: 1, position: Position::new(6, 2) } }
fn ctor(prefix: &str, arguments: Vec<Value>) -> Value { Value::PrefixedConstructor { prefix: prefix.to_string(), arguments, position: Position::new(7, 1) } }
fn expr_value() -> Value { Value::Expression { expr: Box::new(Expression::Identifier { name: "x".to_string(), position: Position::new(8, 2) }), position: Position::new(8, 1) } }
fn enum_val(name: &str, v: &str) -> Value { Value::EnumValue { enum_name: name.to_string(), value: v.to_string(), position: Position::new(9, 1) } }
fn call(name: &str) -> Value { Value::QuickFuncCall { function_name: name.to_string(), arguments: vec![], position: Position::new(10, 1) } }

fn handbuilt_cases() -> Vec<(&'static str, Vec<DataEntry>)> {
    vec![
        ("hand_ordering_table_then_simple", vec![table(&["t"], vec![("x", int(1))]), simple("a", int(2)), DataEntry::ObjectProperty { name: "o".into(), data_type: None, object: Box::new(obj(vec![("k", int(1))])), position: Position::new(11, 1) }]),
        ("hand_ordering_group_then_simple", vec![group(&["g"], vec![int(1), int(2)]), simple("a", int(3))]),
        ("hand_ordering_ok", vec![simple("a", int(1)), table(&["t"], vec![("x", int(1))]), group(&["g"], vec![int(1)])]),
        ("hand_expression_values", vec![simple("a", expr_value()), simple("b", arr(vec![int(1), expr_value()])), simple("c", obj(vec![("k", expr_value())])), table(&["t"], vec![("x", expr_value())])]),
        ("hand_blob_forms", vec![simple("b0", ctor("b", vec![])), simple("b1", ctor("b", vec![int(1)])), simple("b2", ctor("b", vec![st("a"), st("b")])), simple("b3", ctor("b", vec![st("SGVsbG8=")])), simple("b4", ctor("B", vec![st("%%")]))]),
        ("hand_regex_forms", vec![simple("r0", ctor("r", vec![])), simple("r1", ctor("r", vec![int(5)])), simple("r2", ctor("r", vec![st("a"), st("b")])), simple("r3", ctor("r", vec![st("^a+$")])), simple("r4", ctor("R", vec![st("(")]))]),
        ("hand_tuple_forms", vec![simple("t0", ctor("t", vec![])), simple("t6", ctor("t", vec![int(1), int(2), int(3), int(4), int(5), int(6)])), simple("t7", ctor("T", vec![int(1), int(2), int(3), int(4), int(5), int(6), int(7)])), simple("tn", ctor("t", vec![arr(vec![int(1)]), expr_value()]))]),
        ("hand_unknown_prefix", vec![simple("z", ctor("z", vec![int(1)])), typed("zz", DataType::Int, ctor("q", vec![]))]),
        ("hand_nested_array", vec![simple("a", nested(vec![int(1), int(2)])), simple("b", nested(vec![int(1), st("x")])), simple("c", arr(vec![nested(vec![int(1)]), nested(vec![st("y")])])), simple("d", nested(vec![]))]),
        ("hand_enum_forms", vec![simple("a", enum_val("LogLevel", "INFO")), simple("b", enum_val("LogLevel", "NOPE")), simple("c", enum_val("Nope", "A")), simple("d", enum_val("Base.Rarity", "COMMON")), simple("e", enum_val("Base.Rarity.COMMON", "")), simple("f", enum_val("A.B.C.D", "X")), simple("g", enum_val("", "X"))]),
        ("hand_function_forms", vec![simple("a", call("f")), simple("b", call("Ns.f")), simple("c", call("A.B.c")), simple("d", call("")), typed("e", DataType::Int, call("f"))]),
        ("hand_misc_values", vec![simple("a", Value::Identifier { value: "x".into(), position: p() }), simple("b", Value::Range { start: Box::new(int(1)), end: Box::new(int(5)), position: p() }), simple("c", Value::Null { position: p() }), simple("d", Value::ParseError { message: "bad".into(), position: p() }), simple("e", Value::Lambda { parameters: vec!["x".into()], body: Box::new(Expression::Identifier { name: "x".into(), position: p() }), statements: vec![], position: p() }), simple("f", Value::InterpolatedString { template: "a{b}".into(), expressions: vec![], position: p() })]),
        ("hand_deep_object", vec![simple("o", obj(vec![("a", obj(vec![("b", obj(vec![("c", obj(vec![("d", int(1))]))]))]))]))]),
        ("hand_object_in_group", vec![group(&["g"], vec![obj(vec![("a", int(1)), ("b", st("s"))]), obj(vec![("a", int(2))]), int(3)])]),
        ("hand_table_with_object_values", vec![table(&["w", "q"], vec![("o", obj(vec![("a", int(1)), ("b", obj(vec![("c", int(2))]))])), ("n", int(7)), ("arr", arr(vec![int(1), st("x")]))])]),
        ("hand_dup_names_across_forms", vec![simple("a", int(1)), simple("a", int(2)), DataEntry::ObjectProperty { name: "a".into(), data_type: None, object: Box::new(obj(vec![])), position: Position::new(11, 1) }, table(&["a"], vec![("x", int(1))]), table(&["a"], vec![("x", int(1))]), group(&["a"], vec![int(1)])]),
        ("hand_reserved_names", vec![simple("if", int(1)), simple("return", int(2)), table(&["t"], vec![("let", int(1)), ("ok", int(2))])]),
        ("hand_typed_everything", vec![typed("a", DataType::Int, int(1)), typed("b", DataType::Long, int(1)), typed("c", DataType::Float, Value::Double { value: 1.5, position: p() }), typed("d", DataType::Double, Value::Float { value: 1.5, position: p() }), typed("e", DataType::Hex, int(1)), typed("f", DataType::Any, st("s")), typed("g", DataType::String, int(1)), typed("h", DataType::Array, arr(vec![])), typed("i", DataType::Object, obj(vec![])), typed("j", DataType::Date, Value::Timestamp { value: "2026-01-01T00:00:00Z".into(), position: p() }), typed("k", DataType::Timestamp, Value::Date { value: "2026-01-01".into(), position: p() }), typed("l", DataType::Bool, Value::Boolean { value: true, position: p() })]),
        ("hand_empty_section", vec![]),
        ("hand_empty_group", vec![group(&["g"], vec![])]),
        ("hand_long_double_sci", vec![simple("a", Value::Long { value: 5, position: p() }), simple("b", Value::ScientificNotation { value: 1e3, position: p() }), simple("c", Value::HexColor { value: "#FF".into(), position: p() }), simple("d", arr(vec![Value::Long { value: 1, position: p() }, Value::Integer { value: 2, position: p() }, Value::Double { value: 2.5, position: p() }]))]),
    ]
}

/// Analyzes a hand-built section and prints it the way `HAND_RECORDING` stores it.
fn hand_text(name: &str, entries: &[DataEntry], strategy_name: &str, through_arena: bool) -> String {
    let em = ErrorManager::new_isolated_silent();
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = strategy(strategy_name);
    em.update_settings(settings.clone());
    let section = DataSection::new(entries.to_vec(), Position::new(1, 1));
    let mut table = SymbolTable::new();
    let mut enums = HashMap::new();
    enums.insert("DEBUG".to_string(), 0);
    enums.insert("INFO".to_string(), 1);
    table.add_enum("LogLevel".to_string(), enums);

    let mut analyzer = DataSectionAnalyzer::new_with_error_manager(&settings, em.clone());
    let r = if through_arena {
        let mut arena = Arena::new();
        let doc = DocData::from_owned(&mut arena, &section);
        analyzer.analyze_doc(&doc, &arena, &mut table)
    } else {
        analyzer.analyze(&section, &mut table)
    };

    let mut out = String::new();
    out.push_str(&format!("@{name}|{strategy_name}\n"));
    out.push_str(&format!("success={}\n", r.is_success));
    let mut e: Vec<String> = r
        .errors
        .iter()
        .map(|e| format!("[{}] {} | {} | {} | {:?}", e.error_id, e.error_type, e.message, e.suggestion, e.position))
        .collect();
    e.sort();
    for x in &e {
        out.push_str(&format!("E {x}\n"));
    }
    let mut w: Vec<String> = r.warnings.iter().map(|w| format!("[{}] {} | {:?}", w.warning_id, w.message, w.position)).collect();
    w.sort();
    for x in &w {
        out.push_str(&format!("W {x}\n"));
    }
    let (short, types) = analyzer.get_indexes();
    let short: BTreeMap<_, _> = short.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let types: BTreeMap<_, _> = types.iter().map(|(k, v)| (k.clone(), format!("{:?}", v))).collect();
    out.push_str(&format!("short={:?}\ntypes={:?}\n", short, types));
    let vars: BTreeMap<_, _> = table.data_variables.iter().map(|(k, v)| (k.clone(), format!("{:?}", v))).collect();
    out.push_str(&format!("vars={:?}\n", vars));
    let mut logged: Vec<String> = em.get_semantic_errors().iter().map(|x| format!("{}:{}:{}", x.line, x.column, x.message)).collect();
    logged.sort();
    out.push_str(&format!("logged={:?}", logged));
    out
}

/// The recorded blocks of `HAND_RECORDING`: the text of each, keyed by `name|strategy`.
fn hand_blocks() -> Vec<(String, String)> {
    let mut blocks: Vec<(String, String)> = Vec::new();
    for line in HAND_RECORDING.lines().filter(|l| !l.is_empty() || true) {
        if let Some(header) = line.strip_prefix('@') {
            blocks.push((header.to_string(), line.to_string()));
        } else if let Some(last) = blocks.last_mut() {
            last.1.push('\n');
            last.1.push_str(line);
        }
    }
    // The raw string starts with a newline and each block may end with a blank line.
    blocks.into_iter().map(|(k, v)| (k, v.trim_end_matches('\n').to_string())).collect()
}

#[test]
fn hand_built_sections_match_the_recording_through_both_entry_points() {
    let blocks = hand_blocks();
    assert_eq!(blocks.len(), EXPECTED_HAND_BLOCKS);
    let cases = handbuilt_cases();
    let mut checked = 0;
    for (key, expected) in &blocks {
        let (name, strategy_name) = key.split_once('|').expect("name|strategy");
        let entries = &cases.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("unknown case {name}")).1;
        for through_arena in [false, true] {
            let got = hand_text(name, entries, strategy_name, through_arena);
            assert_eq!(&got, expected, "{name} [{strategy_name}] (through the arena: {through_arena})");
            checked += 1;
        }
    }
    assert_eq!(checked, EXPECTED_HAND_BLOCKS * 2);
    assert_eq!(cases.len() * 3, EXPECTED_HAND_BLOCKS, "every hand-built case is recorded under three strategies");
}

#[test]
fn branches_the_parser_cannot_reach_report_what_the_recording_says() {
    // A reading of the recording that fails if the hand-built cases stop reaching their branch.
    let text = |name: &str| {
        let cases = handbuilt_cases();
        let entries = &cases.iter().find(|(n, _)| *n == name).expect("case").1;
        hand_text(name, entries, "continue", true)
    };
    assert!(text("hand_ordering_table_then_simple").contains("ORDERING_VIOLATION"));
    assert!(text("hand_ordering_group_then_simple").contains("ORDERING_VIOLATION"));
    assert!(!text("hand_ordering_ok").contains("ORDERING_VIOLATION"));
    assert!(text("hand_expression_values").contains("INVALID_EXPRESSION"));
    assert!(text("hand_blob_forms").contains("INVALID_BLOB_CONTENT"));
    assert!(text("hand_regex_forms").contains("INVALID_REGEX_PATTERN"));
    assert!(text("hand_tuple_forms").contains("TUPLE_TOO_LARGE"));
    assert!(text("hand_enum_forms").contains("ENUM_VALUE_NOT_FOUND"));
    assert!(text("hand_function_forms").contains("FUNCTION_NOT_FOUND"));
}

// ==================== which tree is read ====================

#[test]
fn the_analyzer_reads_the_document_when_one_is_attached() {
    // The document holds a section with errors and indexes. The owned tree handed to the
    // analyzer has no @DATA section at all, so anything reported can only come from the document.
    let parsed = parse(source("data_dup_flat"), ErrorHandlingStrategy::Continue).expect("parses");
    let empty = DixScript::new();

    let (with_doc, _) = analyze(&empty, &parsed.settings, Some(&parsed.document));
    assert_eq!(section_lines(&with_doc, "DATA").len(), 3);
    assert!(with_doc.short_name_index.is_some(), "the indexes come from the document's section");
    assert!(with_doc.type_index.is_some());

    let (without, _) = analyze(&empty, &parsed.settings, None);
    assert!(!without.section_results.contains_key("DATA"));
    assert!(without.short_name_index.is_none());
}

#[test]
fn the_analyzer_reads_the_owned_tree_when_no_document_is_attached() {
    // The mirror image: the document has no section and the owned tree has one.
    let parsed = parse(source("data_dup_flat"), ErrorHandlingStrategy::Continue).expect("parses");
    let empty_document = Document::from_owned(&DixScript::new());

    let (without, _) = analyze(&parsed.ast, &parsed.settings, None);
    assert_eq!(section_lines(&without, "DATA").len(), 3);

    let (with_empty, _) = analyze(&parsed.ast, &parsed.settings, Some(&empty_document));
    assert!(!with_empty.section_results.contains_key("DATA"));
    assert!(with_empty.short_name_index.is_none());
}

#[test]
fn the_indexes_describe_the_section_that_was_read() {
    // Two different files: take the document of one and the owned tree of the other. The
    // short-name index must name the document's properties.
    let objects = parse(source("data_short_names"), ErrorHandlingStrategy::Continue).expect("parses");
    let scalars = parse(source("data_scalars"), ErrorHandlingStrategy::Continue).expect("parses");

    let (result, _) = analyze(&scalars.ast, &scalars.settings, Some(&objects.document));
    let index = result.short_name_index.expect("an index");
    assert!(index.contains_key("x"), "{index:?}");
    assert!(!index.contains_key("e"), "{index:?}");
}
