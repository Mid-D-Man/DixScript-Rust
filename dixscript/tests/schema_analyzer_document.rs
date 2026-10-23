// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/schema_analyzer_document.rs"
// ============================================================================
//! Tests for the `@SCHEMA` analyzer reading a `Document`.
//!
//! `GeneralSemanticAnalyzer` can be given the arena form of a file with `with_document`, and the
//! `@SCHEMA` analyzer then reads the schema and the `@DATA` section it checks from the
//! `Document`, where it used to read the owned tree. It must report exactly what it reported
//! before. The tests check three things:
//!
//! * the two paths, and the three analyzer entry points (`analyze`, `analyze_doc` and the
//!   post-resolution `validate_resolved`), give identical diagnostics, positions and section
//!   results for every source under all three error strategies;
//! * the diagnostics are the ones the owned-tree analyzer recorded before the change (the
//!   `RECORDING` table, produced by running the original code): every error code, every value
//!   kind, every path form and the string length rule;
//! * the analyzer really reads the `Document` when it has one, for the schema and for the data,
//!   and the owned tree when it does not. A pair whose two halves disagree shows which half was
//!   read.
//!
//! The recording stores one case per source under `continue`, and one more under `halt` or
//! `recover` where that strategy changes the outcome.

use dixscript::Compiler::AST::arena::{Arena, DocData, DocSchema};
use dixscript::Compiler::AST::{DataEntry, DataSection, SchemaBlock, SchemaField, TablePath, Value};
use dixscript::Compiler::Core::SectionAnalyzers::SchemaSectionAnalyzer;

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
    let label = "schema-analyzer-document-test";
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
    ("sch_empty", r##"@DATA(
  a = 1
)
@SCHEMA(
)
"##),
    ("sch_valid_basic", r##"@DATA(
  port = 8080
  name = "svc"
  debug = true
)
@SCHEMA(
  port = Schema.Int(true, 1, 65535)
  name = Schema.String(true, 1, 10)
  debug = Schema.Bool(true)
)
"##),
    ("sch_missing_required", r##"@DATA(
  a = 1
)
@SCHEMA(
  b = Schema.Int(true)
  c = Schema.String(false)
)
"##),
    ("sch_no_data_section", r##"@SCHEMA(
  a = Schema.Int(true)
  b = Schema.Int(false)
)
"##),
    ("sch_duplicate_paths", r##"@DATA(
  a = 1
)
@SCHEMA(
  a = Schema.Int(true)
  a = Schema.String(true)
  a = Schema.Bool(false)
)
"##),
    ("sch_bad_descriptors", r##"@DATA(
  a = 1
  b = 2
  c = 3
  d = 4
)
@SCHEMA(
  a = Schema.Int(1)
  b = Schema.Int()
  c = Schema.Enum(true)
  d = Schema.Foo(true)
)
"##),
    ("sch_bad_bounds", r##"@DATA(
  a = 1
)
@SCHEMA(
  a = Schema.Int(true, 10, 1)
  b = Schema.String(true, "x")
  c = Schema.Array(true, 1.5)
)
"##),
    ("sch_type_mismatches", r##"@DATA(
  i = "s"
  s = 5
  b = 1
  f = true
  a = 3
  o = 2
)
@SCHEMA(
  i = Schema.Int(true)
  s = Schema.String(true)
  b = Schema.Bool(true)
  f = Schema.Float(true)
  a = Schema.Array(true)
  o = Schema.Object(true)
)
"##),
    ("sch_numeric_compat", r##"@DATA(
  i = 1
  l = 5000000000L
  f = 1.5f
  d = 2.5
  x = 1.5e3
  frac = 1.5
  big = 5000000000L
)
@SCHEMA(
  i = Schema.Int(true)
  l = Schema.Long(true)
  f = Schema.Float(true)
  d = Schema.Double(true)
  x = Schema.Double(true)
  frac = Schema.Int(true)
  big = Schema.Int(true)
)
"##),
    ("sch_long_accepts_int", r##"@DATA(
  i = 7
)
@SCHEMA(
  i = Schema.Long(true)
)
"##),
    ("sch_range_edges", r##"@DATA(
  lo = 1
  hi = 10
  below = 0
  above = 11
  neg = -5
  fl = 0.5
)
@SCHEMA(
  lo = Schema.Int(true, 1, 10)
  hi = Schema.Int(true, 1, 10)
  below = Schema.Int(true, 1, 10)
  above = Schema.Int(true, 1, 10)
  neg = Schema.Int(true, -10, -1)
  fl = Schema.Double(true, 0.5, 0.75)
)
"##),
    ("sch_only_min_only_max", r##"@DATA(
  a = 5
  b = 5
)
@SCHEMA(
  a = Schema.Int(true, 6)
  b = Schema.Int(true, 1, 4)
)
"##),
    ("sch_string_lengths", r##"@DATA(
  empty = ""
  short = "ab"
  long = "abcdefghij"
  uni = "héllo wörld"
  exact = "abc"
)
@SCHEMA(
  empty = Schema.String(true, 1, 5)
  short = Schema.String(true, 3, 5)
  long = Schema.String(true, 1, 5)
  uni = Schema.String(true, 11, 11)
  exact = Schema.String(true, 3, 3)
)
"##),
    ("sch_array_counts", r##"@DATA(
  none = []
  two = [1, 2]
  four = [1, 2, 3, 4]
  nested = [[1], [2], [3]]
)
@SCHEMA(
  none = Schema.Array(true, 1, 3)
  two = Schema.Array(true, 1, 3)
  four = Schema.Array(true, 1, 3)
  nested = Schema.Array(true, 1, 2)
)
"##),
    ("sch_group_arrays", r##"@DATA(
  origins:: "a", "b"
  one:: "x"
  grp.sub:: 1, 2, 3
)
@SCHEMA(
  origins = Schema.Array(true, 1, 5)
  one = Schema.Array(true, 2, 5)
  grp.sub = Schema.Array(true, 1, 2)
  grp = Schema.Object(true)
  missing = Schema.Array(true)
)
"##),
    ("sch_tables", r##"@DATA(
  db.primary: host = "h", port = 5432, ssl<bool> = true
  db.replica: host = "r"
)
@SCHEMA(
  db.primary.host = Schema.String(true)
  db.primary.port = Schema.Int(true, 1, 1000)
  db.primary.ssl = Schema.Bool(true)
  db.primary.nope = Schema.Int(true)
  db.primary = Schema.Object(true)
  db = Schema.Object(true)
  db.replica.host.deeper = Schema.String(true)
  db.primary.host = Schema.Int(false)
)
"##),
    ("sch_objects", r##"@DATA(
  server = { host = "x", port = 80, inner = { deep = 1 } }
  plain = 3
)
@SCHEMA(
  server = Schema.Object(true)
  server.host = Schema.String(true)
  server.port = Schema.Int(true, 100, 200)
  server.inner.deep = Schema.Int(true)
  server.inner.gone = Schema.Int(true)
  server.host.letters = Schema.String(true)
  plain.sub = Schema.Int(true)
)
"##),
    ("sch_first_match_wins", r##"@DATA(
  a = 1
  a = { b = 2 }
  c = "first"
  c = 9
)
@SCHEMA(
  a = Schema.Int(true)
  a.b = Schema.Int(true)
  c = Schema.String(true)
)
"##),
    ("sch_declared_types", r##"@DATA(
  age<long> = 5
  name<string> = "n"
  flag<bool> = true
  n<int> = 3
  d<double> = 2.5
)
@SCHEMA(
  age = Schema.Int(true)
  name = Schema.String(true)
  flag = Schema.Bool(true)
  n = Schema.Long(true)
  d = Schema.Float(true)
)
"##),
    ("sch_null_values", r##"@DATA(
  a = null
  b = null
)
@SCHEMA(
  a = Schema.Int(true)
  b = Schema.Int(false)
)
"##),
    ("sch_deferred_values", r##"@QUICKFUNCS(
  ~dbl<int>(n) {
    return n * 2
  }
)
@DATA(
  computed = dbl(21)
  base = 4
  ref = base
)
@SCHEMA(
  computed = Schema.Int(true, 0, 10)
  ref = Schema.String(true)
  base = Schema.Int(true)
)
"##),
    ("sch_enums", r##"@ENUMS(
  LogLevel { DEBUG = 0, INFO = 1 }
  Environment { DEV = 1, PROD = 3 }
)
@DATA(
  level<enum> = LogLevel.INFO
  env<enum> = Environment.PROD
  also<enum> = LogLevel.DEBUG
)
@SCHEMA(
  level = Schema.Enum(true, "LogLevel")
  env = Schema.Enum(true, "LogLevel")
  also = Schema.Enum(true, "Alias.LogLevel")
)
"##),
    ("sch_enum_wrong_kind", r##"@ENUMS(
  LogLevel { DEBUG = 0, INFO = 1 }
)
@DATA(
  level = 3
  other<enum> = LogLevel.INFO
)
@SCHEMA(
  level = Schema.Enum(true, "LogLevel")
  other = Schema.Int(true)
)
"##),
    ("sch_other_kinds", r##"@DATA(
  color = #FF00AA
  nothing = null
)
@SCHEMA(
  color = Schema.Hex(true)
  color2 = Schema.String(false)
  nothing = Schema.Hex(false)
)
"##),
    ("sch_many_errors", r##"@DATA(
  a = "s"
  b = 50
  c = "toolongvalue"
  e = [1]
)
@SCHEMA(
  a = Schema.Int(true)
  b = Schema.Int(true, 1, 10)
  c = Schema.String(true, 1, 3)
  d = Schema.Int(true)
  e = Schema.Array(true, 2, 3)
  a = Schema.Int(true)
  f = Schema.Nope(true)
)
"##),
    ("sch_with_security_and_raw", r##"@DATA(
  a = 1
)
@SCHEMA(
  a = Schema.Int(true, 5, 9)
)
@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "xor" }
)
@RAW(
  meta_data -> { id = "sr", format = "TXT" }
  content -> {
---srt
p
---srt
  }
)
"##),
    ("sch_kinds_vs_int", r##"@DATA(
  arr = [1]
  obj = {k = 1}
  hexc = #FF00AA
  ts = 2026-09-06T00:00:00Z
  dt = 2026-09-06
  bl = b:("eA==")
  rx = r:("y")
  tp = t:(1, 2)
  st = "s"
  bo = true
  fl = 1.5f
  db = 2.5
  sc = 1e3
  lg = 5000000000L
  ii = 7
)
@SCHEMA(
  arr = Schema.Int(true)
  obj = Schema.Int(true)
  hexc = Schema.Int(true)
  ts = Schema.Int(true)
  dt = Schema.Int(true)
  bl = Schema.Int(true)
  rx = Schema.Int(true)
  tp = Schema.Int(true)
  st = Schema.Int(true)
  bo = Schema.Int(true)
  fl = Schema.Int(true)
  db = Schema.Int(true)
  sc = Schema.Int(true)
  lg = Schema.Int(true)
)
"##),
    ("sch_kinds_matching", r##"@DATA(
  arr = [1]
  obj = {k = 1}
  hexc = #FF00AA
  ts = 2026-09-06T00:00:00Z
  dt = 2026-09-06
  bl = b:("eA==")
  rx = r:("y")
  tp = t:(1, 2)
  st = "s"
  bo = true
  fl = 1.5f
  db = 2.5
  sc = 1e3
  lg = 5000000000L
  ii = 7
)
@SCHEMA(
  arr = Schema.Array(true)
  obj = Schema.Object(true)
  hexc = Schema.Hex(true)
  ts = Schema.Timestamp(true)
  dt = Schema.Date(true)
  bl = Schema.Blob(true)
  rx = Schema.Regex(true)
  tp = Schema.Tuple(true)
  st = Schema.String(true)
  bo = Schema.Bool(true)
  fl = Schema.Float(true)
  db = Schema.Double(true)
  sc = Schema.Double(true)
  lg = Schema.Long(true)
  ii = Schema.Long(true)
)
"##),
    ("sch_kinds_crossed", r##"@DATA(
  arr = [1]
  obj = {k = 1}
  hexc = #FF00AA
  ts = 2026-09-06T00:00:00Z
  dt = 2026-09-06
  bl = b:("eA==")
  rx = r:("y")
  tp = t:(1, 2)
  st = "s"
  bo = true
  fl = 1.5f
  db = 2.5
  sc = 1e3
  lg = 5000000000L
  ii = 7
)
@SCHEMA(
  arr = Schema.Object(true)
  obj = Schema.Array(true)
  hexc = Schema.String(true)
  ts = Schema.Date(true)
  dt = Schema.Timestamp(true)
  bl = Schema.Regex(true)
  rx = Schema.Tuple(true)
  tp = Schema.Blob(true)
  st = Schema.Hex(true)
  bo = Schema.String(true)
  fl = Schema.Long(true)
  ii = Schema.Float(true)
)
"##),
    ("sch_declared_vs_kind", r##"@DATA(
  a<array> = [1, 2]
  o<object> = {k = 1}
  h<hex> = #FF00AA
  s<string> = "text"
  e<int> = 5
)
@SCHEMA(
  a = Schema.Array(true, 1, 1)
  o = Schema.Object(true)
  h = Schema.Hex(true)
  s = Schema.String(true, 1, 2)
  e = Schema.String(true)
)
"##),
    ("sch_table_object_values", r##"@DATA(
  db.p: cfg = {a = 1, inner = {z = "s"}}, n = 5
  srv<object> = {host = "h", port = 9}
)
@SCHEMA(
  db.p.cfg.a = Schema.Int(true, 2, 5)
  db.p.cfg.inner.z = Schema.Int(true)
  db.p.cfg = Schema.Object(true)
  db.p.n = Schema.Int(true, 1, 4)
  srv = Schema.Object(true)
  srv.port = Schema.Int(true, 10, 20)
  srv.host = Schema.String(true, 5)
)
"##),
];

/// The number of cases in `RECORDING`.
const EXPECTED_CASES: usize = 49;

/// What the owned-tree analyzer reported for each source before it moved to the arena,
/// captured by running the original code. A line `@name|strategy|success=..` starts a case; the
/// `E|id|message|line:col` and `W|id|message|line:col` lines after it are that section's
/// diagnostics (`-` for no position).
const RECORDING: &str = r##"
@sch_empty|continue|success=true
W|SCHEMA_WARN001|@SCHEMA declares no fields, so it constrains nothing|4:8
@sch_valid_basic|continue|success=true
@sch_missing_required|continue|success=false
E|SCH003|Required @DATA field 'b' is missing|5:3
@sch_no_data_section|continue|success=false
E|SCH003|Required @DATA field 'a' is missing|2:3
@sch_duplicate_paths|continue|success=false
E|SCH002|@SCHEMA path 'a' is declared more than once (first at line 5)|6:3
E|SCH002|@SCHEMA path 'a' is declared more than once (first at line 5)|7:3
@sch_duplicate_paths|halt|success=false
E|SCH002|@SCHEMA path 'a' is declared more than once (first at line 5)|6:3
@sch_bad_descriptors|continue|success=false
E|SCH001|Invalid descriptor for 'a': Error calling Schema.Int: Schema field descriptor: first argument (required) must be a bool|8:3
E|SCH001|Invalid descriptor for 'b': Error calling Schema.Int: Invalid arguments for method Int|9:3
E|SCH001|Invalid descriptor for 'c': Error calling Schema.Enum: Invalid arguments for method Enum|10:3
E|SCH001|Invalid descriptor for 'd': Error calling Schema.Foo: Schema object has no method: Foo|11:3
@sch_bad_descriptors|halt|success=false
E|SCH001|Invalid descriptor for 'a': Error calling Schema.Int: Schema field descriptor: first argument (required) must be a bool|8:3
@sch_bad_bounds|continue|success=false
E|SCH001|Invalid descriptor for 'b': Error calling Schema.String: Schema field descriptor: 'minLength' must be an int or long|6:3
E|SCH001|Invalid descriptor for 'c': Error calling Schema.Array: Schema field descriptor: 'minItems' must be an int or long|7:3
E|SCH005|@DATA field 'a' = 1 is below the minimum 10|2:7
@sch_bad_bounds|halt|success=false
E|SCH001|Invalid descriptor for 'b': Error calling Schema.String: Schema field descriptor: 'minLength' must be an int or long|6:3
@sch_type_mismatches|continue|success=false
E|SCH004|@DATA field 'a' is int, but @SCHEMA expects array|6:7
E|SCH004|@DATA field 'b' is int, but @SCHEMA expects bool|4:7
E|SCH004|@DATA field 'f' is bool, but @SCHEMA expects float|5:7
E|SCH004|@DATA field 'i' is string, but @SCHEMA expects int|2:7
E|SCH004|@DATA field 'o' is int, but @SCHEMA expects object|7:7
E|SCH004|@DATA field 's' is int, but @SCHEMA expects string|3:7
@sch_type_mismatches|halt|success=false
E|SCH004|@DATA field 'i' is string, but @SCHEMA expects int|2:7
@sch_numeric_compat|continue|success=false
E|SCH004|@DATA field 'big' is long, but @SCHEMA expects int|8:9
E|SCH004|@DATA field 'frac' is double, but @SCHEMA expects int|7:10
@sch_numeric_compat|halt|success=false
E|SCH004|@DATA field 'frac' is double, but @SCHEMA expects int|7:10
@sch_long_accepts_int|continue|success=true
@sch_range_edges|continue|success=false
E|SCH005|@DATA field 'above' = 11 is above the maximum 10|5:11
E|SCH005|@DATA field 'below' = 0 is below the minimum 1|4:11
@sch_range_edges|halt|success=false
E|SCH005|@DATA field 'below' = 0 is below the minimum 1|4:11
@sch_only_min_only_max|continue|success=false
E|SCH005|@DATA field 'a' = 5 is below the minimum 6|2:7
E|SCH005|@DATA field 'b' = 5 is above the maximum 4|3:7
@sch_only_min_only_max|halt|success=false
E|SCH005|@DATA field 'a' = 5 is below the minimum 6|2:7
@sch_string_lengths|continue|success=false
E|SCH006|@DATA field 'empty' has length 0, below the minimum 1|2:11
E|SCH006|@DATA field 'long' has length 10, above the maximum 5|4:10
E|SCH006|@DATA field 'short' has length 2, below the minimum 3|3:11
@sch_string_lengths|halt|success=false
E|SCH006|@DATA field 'empty' has length 0, below the minimum 1|2:11
@sch_array_counts|continue|success=false
E|SCH006|@DATA field 'four' has 4 item(s), above the maximum 3|4:10
E|SCH006|@DATA field 'nested' has 3 item(s), above the maximum 2|5:12
E|SCH006|@DATA field 'none' has 0 item(s), below the minimum 1|2:10
@sch_array_counts|halt|success=false
E|SCH006|@DATA field 'none' has 0 item(s), below the minimum 1|2:10
@sch_group_arrays|continue|success=false
E|SCH003|Required @DATA field 'missing' is missing|11:3
E|SCH006|@DATA field 'grp.sub' has 3 item(s), above the maximum 2|4:3
E|SCH006|@DATA field 'one' has 1 item(s), below the minimum 2|3:3
@sch_group_arrays|halt|success=false
E|SCH006|@DATA field 'one' has 1 item(s), below the minimum 2|3:3
@sch_tables|continue|success=false
E|SCH002|@SCHEMA path 'db.primary.host' is declared more than once (first at line 6)|13:3
E|SCH003|Required @DATA field 'db.primary.nope' is missing|9:3
E|SCH003|Required @DATA field 'db.replica.host.deeper' is missing|12:3
E|SCH005|@DATA field 'db.primary.port' = 5432 is above the maximum 1000|2:27
@sch_tables|halt|success=false
E|SCH002|@SCHEMA path 'db.primary.host' is declared more than once (first at line 6)|13:3
@sch_objects|continue|success=false
E|SCH003|Required @DATA field 'plain.sub' is missing|12:3
E|SCH003|Required @DATA field 'server.host.letters' is missing|11:3
E|SCH003|Required @DATA field 'server.inner.gone' is missing|10:3
E|SCH005|@DATA field 'server.port' = 80 is below the minimum 100|2:33
@sch_objects|halt|success=false
E|SCH005|@DATA field 'server.port' = 80 is below the minimum 100|2:33
@sch_first_match_wins|continue|success=false
@sch_declared_types|continue|success=false
E|SCH004|@DATA field 'age' is long, but @SCHEMA expects int|2:15
@sch_null_values|continue|success=false
E|SCH003|Required @DATA field 'a' is null|2:7
@sch_deferred_values|continue|success=true
@sch_enums|continue|success=false
E|SCH007|@DATA field 'env' is a value of enum 'Environment', but @SCHEMA expects enum 'LogLevel'|7:15
@sch_enum_wrong_kind|continue|success=false
E|SCH004|@DATA field 'level' is int, but @SCHEMA expects enum|5:11
E|SCH004|@DATA field 'other' is enum, but @SCHEMA expects int|6:17
@sch_enum_wrong_kind|halt|success=false
E|SCH004|@DATA field 'level' is int, but @SCHEMA expects enum|5:11
@sch_other_kinds|continue|success=true
@sch_many_errors|continue|success=false
E|SCH001|Invalid descriptor for 'f': Error calling Schema.Nope: Schema object has no method: Nope|14:3
E|SCH002|@SCHEMA path 'a' is declared more than once (first at line 8)|13:3
E|SCH003|Required @DATA field 'd' is missing|11:3
E|SCH004|@DATA field 'a' is string, but @SCHEMA expects int|2:7
E|SCH005|@DATA field 'b' = 50 is above the maximum 10|3:7
E|SCH006|@DATA field 'c' has length 12, above the maximum 3|4:7
E|SCH006|@DATA field 'e' has 1 item(s), below the minimum 2|5:7
@sch_many_errors|halt|success=false
E|SCH002|@SCHEMA path 'a' is declared more than once (first at line 8)|13:3
@sch_with_security_and_raw|continue|success=false
E|SCH005|@DATA field 'a' = 1 is below the minimum 5|2:7
@sch_kinds_vs_int|continue|success=false
E|SCH004|@DATA field 'arr' is array, but @SCHEMA expects int|2:9
E|SCH004|@DATA field 'bl' is blob, but @SCHEMA expects int|7:8
E|SCH004|@DATA field 'bo' is bool, but @SCHEMA expects int|11:8
E|SCH004|@DATA field 'db' is double, but @SCHEMA expects int|13:8
E|SCH004|@DATA field 'dt' is date, but @SCHEMA expects int|6:8
E|SCH004|@DATA field 'fl' is float, but @SCHEMA expects int|12:8
E|SCH004|@DATA field 'hexc' is hex, but @SCHEMA expects int|4:10
E|SCH004|@DATA field 'lg' is long, but @SCHEMA expects int|15:8
E|SCH004|@DATA field 'obj' is object, but @SCHEMA expects int|3:9
E|SCH004|@DATA field 'rx' is regex, but @SCHEMA expects int|8:8
E|SCH004|@DATA field 'sc' is double, but @SCHEMA expects int|14:8
E|SCH004|@DATA field 'st' is string, but @SCHEMA expects int|10:8
E|SCH004|@DATA field 'tp' is tuple, but @SCHEMA expects int|9:8
E|SCH004|@DATA field 'ts' is timestamp, but @SCHEMA expects int|5:8
@sch_kinds_vs_int|halt|success=false
E|SCH004|@DATA field 'arr' is array, but @SCHEMA expects int|2:9
@sch_kinds_matching|continue|success=true
@sch_kinds_crossed|continue|success=false
E|SCH004|@DATA field 'arr' is array, but @SCHEMA expects object|2:9
E|SCH004|@DATA field 'bl' is blob, but @SCHEMA expects regex|7:8
E|SCH004|@DATA field 'bo' is bool, but @SCHEMA expects string|11:8
E|SCH004|@DATA field 'dt' is date, but @SCHEMA expects timestamp|6:8
E|SCH004|@DATA field 'fl' is float, but @SCHEMA expects long|12:8
E|SCH004|@DATA field 'hexc' is hex, but @SCHEMA expects string|4:10
E|SCH004|@DATA field 'obj' is object, but @SCHEMA expects array|3:9
E|SCH004|@DATA field 'rx' is regex, but @SCHEMA expects tuple|8:8
E|SCH004|@DATA field 'st' is string, but @SCHEMA expects hex|10:8
E|SCH004|@DATA field 'tp' is tuple, but @SCHEMA expects blob|9:8
E|SCH004|@DATA field 'ts' is timestamp, but @SCHEMA expects date|5:8
@sch_kinds_crossed|halt|success=false
E|SCH004|@DATA field 'arr' is array, but @SCHEMA expects object|2:9
@sch_declared_vs_kind|continue|success=false
E|SCH004|@DATA field 'e' is int, but @SCHEMA expects string|6:12
E|SCH006|@DATA field 'a' has 2 item(s), above the maximum 1|2:14
E|SCH006|@DATA field 's' has length 4, above the maximum 2|5:15
@sch_declared_vs_kind|halt|success=false
E|SCH006|@DATA field 'a' has 2 item(s), above the maximum 1|2:14
@sch_table_object_values|continue|success=false
E|SCH003|Required @DATA field 'srv' is missing|10:3
E|SCH003|Required @DATA field 'srv.host' is missing|12:3
E|SCH003|Required @DATA field 'srv.port' is missing|11:3
E|SCH004|@DATA field 'db.p.cfg.inner.z' is string, but @SCHEMA expects int|2:36
E|SCH005|@DATA field 'db.p.cfg.a' = 1 is below the minimum 2|2:20
E|SCH005|@DATA field 'db.p.n' = 5 is above the maximum 4|2:43
@sch_table_object_values|halt|success=false
E|SCH005|@DATA field 'db.p.cfg.a' = 1 is below the minimum 2|2:20
"##;

// ==================== the paths agree ====================

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
            compared += 1;
        }
    }
    assert_eq!(compared, SOURCES.len() * 3);
}

#[test]
fn the_three_entry_points_agree() {
    let mut checked = 0;
    for (name, text) in SOURCES {
        for strategy_name in STRATEGIES {
            let parsed = parse(text, strategy(strategy_name)).expect("parses");
            let (Some(schema), Some(doc_schema)) = (parsed.ast.schema.as_ref(), parsed.document.schema.as_ref()) else {
                continue;
            };
            let fresh = || SchemaSectionAnalyzer::new_with_error_manager(&parsed.settings, ErrorManager::new_isolated_silent());
            let by_owned = fresh().analyze(schema, parsed.ast.data.as_ref(), &mut SymbolTable::new());
            let by_doc = fresh().analyze_doc(
                doc_schema,
                parsed.document.data.as_ref(),
                &parsed.document.arena,
                &mut SymbolTable::new(),
            );
            let by_resolved = fresh().validate_resolved(schema, parsed.ast.data.as_ref());
            assert_eq!(describe_section(&by_owned), describe_section(&by_doc), "{name} [{strategy_name}]: analyze vs analyze_doc");
            assert_eq!(describe_section(&by_owned), describe_section(&by_resolved), "{name} [{strategy_name}]: analyze vs validate_resolved");
            checked += 1;
        }
    }
    assert!(checked >= 90, "only {checked} runs had a @SCHEMA section");
}

#[test]
fn validate_resolved_checks_the_data_it_is_given() {
    // The post-resolution entry takes the data section as an argument, because the loader
    // passes the resolved tree. Passing different data must change the answer.
    let schema_only = parse("@DATA(\n  a = 1\n)\n@SCHEMA(\n  b = Schema.Int(true)\n)\n", ErrorHandlingStrategy::Continue).expect("parses");
    let with_b = parse("@DATA(\n  a = 1\n  b = 2\n)\n@SCHEMA(\n  b = Schema.Int(true)\n)\n", ErrorHandlingStrategy::Continue).expect("parses");
    let schema = schema_only.ast.schema.as_ref().expect("a schema");
    let fresh = || SchemaSectionAnalyzer::new_with_error_manager(&schema_only.settings, ErrorManager::new_isolated_silent());

    let own_data = fresh().validate_resolved(schema, schema_only.ast.data.as_ref());
    assert_eq!(own_data.errors.iter().map(|e| e.error_id.as_str()).collect::<Vec<_>>(), ["SCH003"]);

    let other_data = fresh().validate_resolved(schema, with_b.ast.data.as_ref());
    assert!(other_data.is_success, "{:?}", other_data.errors);

    let no_data = fresh().validate_resolved(schema, None);
    assert_eq!(no_data.errors.iter().map(|e| e.error_id.as_str()).collect::<Vec<_>>(), ["SCH003"]);
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
fn schema_diagnostics_match_the_recording() {
    for (name, strategy_name, success, mut expected) in recorded_cases() {
        let parsed = parse(source(&name), strategy(&strategy_name)).expect("parses");
        let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
        expected.sort();
        assert_eq!(section_lines(&result, "SCHEMA"), expected, "{name} [{strategy_name}]");
        assert_eq!(result.is_success, success, "{name} [{strategy_name}]: overall success");
    }
}

/// The schema lines of one source under `continue`, read through the document.
fn lines_of(name: &str) -> Vec<String> {
    let parsed = parse(source(name), ErrorHandlingStrategy::Continue).expect("parses");
    let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
    section_lines(&result, "SCHEMA")
}

#[test]
fn string_length_counts_characters_not_bytes() {
    // "héllo wörld" is 11 characters and 13 bytes; `Schema.String(true, 11, 11)` accepts it.
    let lines = lines_of("sch_string_lengths");
    assert!(!lines.iter().any(|l| l.contains("'uni'")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("'short'") && l.contains("below the minimum 3")), "{lines:?}");
}

#[test]
fn a_nested_array_counts_its_top_level_items() {
    let lines = lines_of("sch_array_counts");
    assert!(lines.iter().any(|l| l.contains("'nested' has 3 item(s), above the maximum 2")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("'none' has 0 item(s), below the minimum 1")), "{lines:?}");
}

#[test]
fn the_first_matching_data_entry_decides_and_a_failed_walk_continues() {
    // `a = 1` then `a = { b = 2 }`: `a` is the first entry, and `a.b` is found in the second
    // because walking into the integer finds nothing. `c = "first"` then `c = 9`: the first wins.
    assert_eq!(lines_of("sch_first_match_wins"), Vec::<String>::new());
}

#[test]
fn an_explicit_type_annotation_beats_the_literal() {
    // `age<long> = 5` is a long for a `Schema.Int`, so it is rejected; `n<int> = 3` is an int
    // for a `Schema.Long`, which accepts it.
    let lines = lines_of("sch_declared_types");
    assert!(lines.iter().any(|l| l.contains("'age' is long, but @SCHEMA expects int")), "{lines:?}");
    assert!(!lines.iter().any(|l| l.contains("'n'")), "{lines:?}");
}

#[test]
fn a_qualified_enum_name_matches_its_bare_name() {
    // `LogLevel.DEBUG` against `Schema.Enum(true, "Alias.LogLevel")` is accepted, and
    // `Environment.PROD` against `LogLevel` is not.
    let lines = lines_of("sch_enums");
    assert!(lines.iter().any(|l| l.contains("'env'") && l.contains("expects enum 'LogLevel'")), "{lines:?}");
    assert!(!lines.iter().any(|l| l.contains("'also'")), "{lines:?}");
}

#[test]
fn halt_stops_at_the_first_error() {
    let many = |name: &str| {
        let parsed = parse(source("sch_many_errors"), strategy(name)).expect("parses");
        let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
        section_lines(&result, "SCHEMA").iter().filter(|l| l.starts_with("E|")).count()
    };
    assert_eq!(many("halt"), 1);
    assert!(many("continue") > 1);
}

#[test]
fn a_long_value_is_compared_without_losing_its_upper_bits() {
    // 5000000000 does not fit in 32 bits. Compared as the long it is, it is above the maximum;
    // truncated to 32 bits it would be 705032704 and pass.
    let src = "@DATA(
  big = 5000000000L
  fits = 3000000000L
)
@SCHEMA(
  big = Schema.Long(true, 0, 4000000000)
  fits = Schema.Long(true, 0, 4000000000)
)
";
    let lines = {
        let parsed = parse(src, ErrorHandlingStrategy::Continue).expect("parses");
        let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
        section_lines(&result, "SCHEMA")
    };
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("E|SCH005|@DATA field 'big' = 5000000000 is above the maximum 4000000000|"), "{lines:?}");
}

#[test]
fn a_nested_array_counts_its_items_through_both_entry_points() {
    // No parser produces `NestedArray` today (only the value resolvers read it), so the
    // recording cannot reach it. The data is built by hand.
    let pos = Position::new(1, 1);
    let item = |n: i32| Value::Integer { value: n, position: pos };
    let schema = SchemaBlock::new(
        vec![SchemaField::new(
            TablePath::new(vec!["grid".to_string()]),
            "Array".to_string(),
            vec![Value::Boolean { value: true, position: pos }, item(1), item(2)],
            pos,
        )],
        pos,
    );
    let data = DataSection::new(
        vec![DataEntry::SimpleProperty {
            name: "grid".to_string(),
            data_type: None,
            value: Value::NestedArray { values: vec![item(1), item(2), item(3)], level: 1, position: pos },
            position: pos,
        }],
        pos,
    );
    let settings = OperationalSettings::default();
    let fresh = || SchemaSectionAnalyzer::new_with_error_manager(&settings, ErrorManager::new_isolated_silent());

    let by_owned = fresh().analyze(&schema, Some(&data), &mut SymbolTable::new());

    let mut arena = Arena::new();
    let doc_schema = DocSchema::from_owned(&mut arena, &schema);
    let doc_data = DocData::from_owned(&mut arena, &data);
    let by_doc = fresh().analyze_doc(&doc_schema, Some(&doc_data), &arena, &mut SymbolTable::new());

    for (which, result) in [("analyze", &by_owned), ("analyze_doc", &by_doc)] {
        assert_eq!(result.errors.len(), 1, "{which}: {:?}", result.errors);
        assert_eq!(result.errors[0].error_id, "SCH006", "{which}");
        assert!(result.errors[0].message.contains("'grid' has 3 item(s), above the maximum 2"), "{which}: {}", result.errors[0].message);
    }
}

// ==================== which tree is read ====================

const MISSING_B: &str = "@DATA(\n  a = 1\n)\n@SCHEMA(\n  b = Schema.Int(true)\n)\n";
const HAS_B: &str = "@DATA(\n  a = 1\n  b = 2\n)\n@SCHEMA(\n  b = Schema.Int(true)\n)\n";

fn error_ids(result: &SemanticAnalysisResult) -> Vec<String> {
    result.errors.iter().map(|e| e.error_id.clone()).collect()
}

#[test]
fn the_analyzer_reads_the_schema_of_the_document_when_one_is_attached() {
    // The document has a schema that fails; the owned tree handed to the analyzer has none.
    let parsed = parse(MISSING_B, ErrorHandlingStrategy::Continue).expect("parses");
    let empty = DixScript::new();

    let (with_doc, _) = analyze(&empty, &parsed.settings, Some(&parsed.document));
    assert_eq!(error_ids(&with_doc), ["SCH003"]);

    let (without, _) = analyze(&empty, &parsed.settings, None);
    assert!(!without.section_results.contains_key("SCHEMA"));
}

#[test]
fn the_analyzer_reads_the_schema_of_the_owned_tree_when_no_document_is_attached() {
    let parsed = parse(MISSING_B, ErrorHandlingStrategy::Continue).expect("parses");
    let empty_document = Document::from_owned(&DixScript::new());

    let (without, _) = analyze(&parsed.ast, &parsed.settings, None);
    assert_eq!(error_ids(&without), ["SCH003"]);

    let (with_empty, _) = analyze(&parsed.ast, &parsed.settings, Some(&empty_document));
    assert!(!with_empty.section_results.contains_key("SCHEMA"));
}

#[test]
fn the_analyzer_reads_the_data_of_the_document_when_one_is_attached() {
    // Both halves have the same schema. Only the data differs: the tree's data lacks `b` and
    // the document's data has it, so the answer says which one was checked.
    let tree = parse(MISSING_B, ErrorHandlingStrategy::Continue).expect("parses");
    let document = parse(HAS_B, ErrorHandlingStrategy::Continue).expect("parses");

    let (with_doc, _) = analyze(&tree.ast, &tree.settings, Some(&document.document));
    assert!(with_doc.section_results["SCHEMA"].is_success, "{:?}", with_doc.errors);

    let (without, _) = analyze(&tree.ast, &tree.settings, None);
    assert_eq!(error_ids(&without), ["SCH003"]);
}

#[test]
fn the_analyzer_reads_the_data_of_the_owned_tree_when_no_document_is_attached() {
    let tree = parse(HAS_B, ErrorHandlingStrategy::Continue).expect("parses");
    let document = parse(MISSING_B, ErrorHandlingStrategy::Continue).expect("parses");

    let (without, _) = analyze(&tree.ast, &tree.settings, None);
    assert!(without.section_results["SCHEMA"].is_success, "{:?}", without.errors);

    let (with_doc, _) = analyze(&tree.ast, &tree.settings, Some(&document.document));
    assert_eq!(error_ids(&with_doc), ["SCH003"]);
}
