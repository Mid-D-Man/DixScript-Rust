// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/quickfuncs_analyzer_document.rs"
// ============================================================================
//! Tests for the `@QUICKFUNCS` analyzer reading a `Document`.
//!
//! `GeneralSemanticAnalyzer` can be given the arena form of a file with `with_document`, and the
//! `@QUICKFUNCS` analyzer then reads the `Document` where it used to read the owned tree. It must
//! report exactly what it reported before, record the same qualified identifier resolutions (the
//! enhancer's input) and register the same functions. The tests check five things:
//!
//! * the two paths give identical diagnostics, positions, resolutions and registered functions
//!   for every source under all three error strategies;
//! * those are the ones the owned-tree analyzer recorded before the change (the `RECORDING`
//!   table, produced by running the original code);
//! * the checks the parser prevents (invalid names, operators and types, object creation
//!   statements, imported namespaces and enums, deep expressions) give the recorded output through
//!   both entry points, `analyze` and `analyze_doc`, on sections built by hand (`HAND_RECORDING`);
//! * the analyzer really reads the `Document` when it has one and the owned tree when it does not;
//! * the things the recording does not reach, found by putting deliberate bugs into the analyzer, the
//!   call collector, the cycle detector and the orchestrator: the exact edge of each size limit and of
//!   the expression depth budget, a call in every kind of construct reaching the call graph, a
//!   variable used in every kind of construct not being reported as unused, the error manager's records
//!   for undefined callees and cycles, the owned entry points of the call graph tools, and two
//!   branches of the orchestrator (the last group of tests in this file).
//!
//! Two things in the analyzer's output depend on hash order and are normalized here, as they are
//! in the harness that made the recording: the lists in "Valid values", "Available functions" and
//! "Available enums" suggestions, and the function a reported cycle starts from. Neither is
//! changed by the arena work.
//!
//! `QFUNC003B`, `QFUNC007` and `QFUNC073` are not tested because they cannot be reached: every data
//! type is valid, and the statement nesting limit stops the recursion before the depth budget runs
//! out.

use std::collections::BTreeMap;

use dixscript::Compiler::AST::arena::{Arena, DocQuickFuncs};
use dixscript::Compiler::AST::{
    DeclarationType, ObjectProperty, QuickFuncParam, QuickFuncStatement, QuickFuncsSection, QuickFunction,
    SwitchCase,
};
use dixscript::Compiler::AST::{DataType, Expression, Value};
use dixscript::Compiler::Core::SectionAnalyzers::QuickFuncsSectionAnalyzer;
use dixscript::Compiler::Utilities::{FunctionSignature, ParameterInfo, QuickFunctionInfo};

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
    let label = "quickfuncs-analyzer-document-test";
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
        .map(|e| norm(&format!("{}:{}:{}:{:?}:{:?}", e.line, e.column, e.message, e.section_name, e.suggestion)))
        .collect();
    logged.sort();
    (result, logged)
}

/// Everything about a result that the tests compare, in a stable order.
fn describe(result: &SemanticAnalysisResult, logged: &[String]) -> String {
    let mut errors: Vec<String> = result
        .errors
        .iter()
        .map(|e| norm(&format!("[{}] {} | {} | {} | {} | {:?}", e.error_id, e.error_type, e.section_name, e.message, e.suggestion, e.position)))
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
    let mut e: Vec<String> = s.errors.iter().map(|e| norm(&format!("{}:{}:{:?}:{}", e.error_id, e.message, e.position, e.suggestion))).collect();
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

/// The set of functions in a reported cycle, sorted, or `None` for any other message.
fn cycle_set(message: &str) -> Option<String> {
    let i = message.find("Circular function call detected: ")?;
    let rest = &message[i + "Circular function call detected: ".len()..];
    let end = rest.find(":Some(").unwrap_or(rest.len());
    let mut names: Vec<&str> = rest[..end].split(" \u{2192} ").collect();
    names.sort();
    names.dedup();
    Some(format!("[{}]", names.join(", ")))
}

/// The diagnostics of one section as `E|id|message|line:col` and `W|id|message|line:col`
/// lines, sorted.
fn section_lines(result: &SemanticAnalysisResult, section: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for e in result.errors.iter().filter(|e| e.section_name == section) {
        match cycle_set(&e.message) {
            Some(set) => lines.push(format!("E|CYCLE|{set}|-")),
            None => lines.push(format!("E|{}|{}|{}", e.error_id, e.message, position(e.position))),
        }
    }
    for w in result.warnings.iter().filter(|w| w.section_name == section) {
        lines.push(format!("W|{}|{}|{}", w.warning_id, w.message, position(w.position)));
    }
    lines.sort();
    lines
}

fn source(name: &str) -> String {
    all_sources().into_iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("unknown source {name}")).1
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
    ("qf_valid_basic", r##"@QUICKFUNCS(
  ~g<int> => global(n<int>) {
    return n
  }
  ~f<int> => global() {
    let a = 1
    const b<int> = 2
    let mut c<string> = "s"
    c += "t"
    let mut n = 3
    n += 1
    n *= 2
    if: a == 1 { log: "one" } else { log: "other" }
    chk: a { -> 1 { log: "x" } -> miss { log: "y" } }
    let d = g(a)
    log: c
    log: b
    log: d
    log: n
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_dup_names", r##"@QUICKFUNCS(
  ~f<int> => global() {
    return 1
  }
  ~f<int> => global() {
    return 2
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_no_return_type", r##"@QUICKFUNCS(
  ~f => global() {
    return 1
  }
  ~g<int> => global() {
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_reserved_name_log", r##"@QUICKFUNCS(
  ~log<int> => global() {
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_reserved_name_let", r##"@QUICKFUNCS(
  ~let<int> => global() {
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_many_params", r##"@QUICKFUNCS(
  ~f<int> => global(p0<int>, p1<int>, p2<int>, p3<int>, p4<int>, p5<int>, p6<int>, p7<int>, p8<int>, p9<int>, p10<int>, p11<int>, p12<int>, p13<int>, p14<int>, p15<int>, p16<int>, p17<int>, p18<int>, p19<int>, p20<int>, p21<int>, p22<int>, p23<int>, p24<int>, p25<int>, p26<int>, p27<int>, p28<int>, p29<int>, p30<int>, p31<int>, p32<int>, p33<int>, p34<int>, p35<int>, p36<int>, p37<int>, p38<int>, p39<int>, p40<int>, p41<int>, p42<int>, p43<int>, p44<int>, p45<int>, p46<int>, p47<int>, p48<int>, p49<int>, p50<int>, p51<int>, p52<int>, p53<int>, p54<int>, p55<int>, p56<int>, p57<int>, p58<int>, p59<int>, p60<int>, p61<int>, p62<int>, p63<int>, p64<int>, p65<int>, p66<int>, p67<int>, p68<int>, p69<int>, p70<int>, p71<int>, p72<int>, p73<int>, p74<int>, p75<int>, p76<int>, p77<int>, p78<int>, p79<int>, p80<int>, p81<int>, p82<int>, p83<int>, p84<int>, p85<int>, p86<int>, p87<int>, p88<int>, p89<int>, p90<int>, p91<int>, p92<int>, p93<int>, p94<int>, p95<int>, p96<int>, p97<int>, p98<int>, p99<int>, p100<int>) {
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_dup_params", r##"@QUICKFUNCS(
  ~f<int> => global(a<int>, a<int>) {
    return a
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_param_type_keyword", r##"@QUICKFUNCS(
  ~f<int> => global(int<int>, string<int>) {
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_param_reserved", r##"@QUICKFUNCS(
  ~f<int> => global(log<int>, return<int>) {
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_param_default_mismatch", r##"@QUICKFUNCS(
  ~f<int> => global(a<int> = "s", b<float> = 1, c<string> = 1 + 2, d<bool> = 5, e<array> = [1, 2]) {
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_param_order", r##"@QUICKFUNCS(
  ~f<int> => global(a<int> = 1, b<int>, c<int> = 2, d<int>) {
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_no_scope", r##"@QUICKFUNCS(
  ~f<int>() {
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_scope_forms", r##"@QUICKFUNCS(
  ~f<int> => global, data.users() {
    return 1
  }
  ~g<int> => user.profile() {
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_empty_body", r##"@QUICKFUNCS(
  ~f<int> => global() {
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_nesting_deep", r##"@QUICKFUNCS(
  ~f<int> => global() {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    log: "x"
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_nesting_ok", r##"@QUICKFUNCS(
  ~f<int> => global() {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    log: "x"
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_not_all_paths", r##"@QUICKFUNCS(
  ~f<int> => global() {
    if: true {
      return 1
    }
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_all_paths_ok", r##"@QUICKFUNCS(
  ~f<int> => global() {
    if: true {
      return 1
    } else {
      return 2
    }
  }
  ~g<int> => global() {
    chk: 1 {
      -> 1 { return 1 }
      -> miss { return 2 }
    }
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_chk_no_miss", r##"@QUICKFUNCS(
  ~f<int> => global() {
    chk: 1 {
      -> 1 { return 1 }
      -> 2 { return 2 }
    }
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_if_non_bool", r##"@QUICKFUNCS(
  ~f<int> => global() {
    if: 1 { log: "a" }
    if: "s" { log: "b" }
    if: 1 == 1 { log: "c" }
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_return_mismatch", r##"@QUICKFUNCS(
  ~f<int> => global() {
    return "s"
  }
  ~g<string> => global() {
    return 1
  }
  ~h<float> => global() {
    return 1
  }
  ~i<bool> => global() {
    return true
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_assign_errors", r##"@QUICKFUNCS(
  ~f<int> => global() {
    x = 1
    const c = 1
    c = 2
    let mut s<string> = "a"
    s = 1
    let mut u = 1
    u = "s"
    let mut ok = 1
    ok = 2
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_arith_assign", r##"@QUICKFUNCS(
  ~f<int> => global() {
    y += 1
    const c = 1
    c += 2
    let mut s = "a"
    s -= 1
    s *= 2
    s += "b"
    let mut n = 1
    n += "s"
    n &= 1.5
    n <<= "a"
    n |= 2
    n -= 1
    n /= 2
    n %= 2
    n **= 2
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_var_decl", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let log = 1
    let int = 2
    let a = 1
    let a = 2
    let b<int> = "s"
    let c<float> = 1
    let d<string> = "s"
    let e<bool> = 1
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_arith_expr", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = 1 + "a"
    let b = "a" + 1
    let c = "a" - "b"
    let d = true * 2
    let e = 1 + true
    let f = 1 + 2.5
    let g = "a" + "b"
    let h = 5 ** 2
    log: a
    log: b
    log: c
    log: d
    log: e
    log: f
    log: g
    log: h
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_bitwise_expr", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = "a" & 1
    let b = 1 | 2.5
    let c = 1 ^ true
    let d = 1 << 2
    let e = 5L & 3L
    log: a
    log: b
    log: c
    log: d
    log: e
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_comparison_expr", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = "a" < 1
    let b = 1 == "a"
    let c = 1 != 2
    let d = "a" == "b"
    let e = true >= false
    let f = 1 < 2
    log: a
    log: b
    log: c
    log: d
    log: e
    log: f
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_logical_expr", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = 1 && true
    let b = true || "s"
    let c = true and false
    let d = true && true
    log: a
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
    ("qf_unary_expr", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = !1
    let b = -"a"
    let c = +true
    let d = ~?"a"
    let e = not 5
    let f = -5
    let g = !true
    let h = ~?5
    log: a
    log: b
    log: c
    log: d
    log: e
    log: f
    log: g
    log: h
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_ternary_expr", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = 1 ? 2 : 3
    let b = true ? 1 : "a"
    let c = true ? 1 : 2
    let d = true ? [1] : 2
    log: a
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
    ("qf_calls", r##"@QUICKFUNCS(
  ~g<int> => global(n<int>) {
    return n
  }
  ~f<int> => global() {
    let a = nofunc(1)
    let b = g(1, 2)
    let c = g()
    let d = g(1)
    let e = g(g(2))
    log: a
    log: b
    log: c
    log: d
    log: e
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_instance_methods", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    let a = [1, 2]
    let b = s.nomethod()
    let c = a.first()
    let d = s.upper()
    let e = a.nope()
    let n = 5
    let f = n.upper()
    log: b
    log: c
    log: d
    log: e
    log: f
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_method_chain_deep", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    let t = s.upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper()
    log: t
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_method_chain_ok", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    let t = s.upper().upper().upper().upper().upper().upper().upper().upper()
    log: t
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_static_calls", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = Nope.f()
    let b = Math.nomethod(1)
    let c = Math.sqrt(4)
    let d = Array.of(1, 2, 3)
    let e = Random.choice(d)
    let f = DateTime.now()
    log: a
    log: b
    log: c
    log: d
    log: e
    log: f
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_enums", r##"@ENUMS(
  LogLevel { DEBUG = 0, INFO = 1 }
)
@QUICKFUNCS(
  ~f<int> => global() {
    let a = LogLevel.INFO
    let b = LogLevel.NOPE
    let c = Missing.A
    let d<enum> = LogLevel.DEBUG
    log: a
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
    ("qf_value_limits", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = t:(1, 2, 3, 4, 5, 6, 7)
    let b = {k = 1, k = 2}
    let c = [1, "a"]
    let d = [1, 2.5]
    let e = [1, x]
    let f = [1, 2, 3]
    let g = t:(1, 2, 3, 4, 5, 6)
    log: a
    log: b
    log: c
    log: d
    log: e
    log: f
    log: g
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_unknown_identifiers", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = y
    let b = foo.bar
    let c = foo.baz(1)
    let d = x
    let e = x.field
    log: a
    log: b
    log: c
    log: d
    log: e
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_qualified_forms", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    let a = s.length
    let b = Math.PI
    let c = DateTime.now().year
    let t = t:(1, 2)
    let d = t.first()
    let o = {k = 1}
    let e = o.k
    let i = [1, 2][0]
    log: a
    log: b
    log: c
    log: d
    log: e
    log: i
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_expr_depth", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = ((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((1))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))
    log: a
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_cycles", r##"@QUICKFUNCS(
  ~a<int> => global() {
    return b()
  }
  ~b<int> => global() {
    return a()
  }
  ~c<int> => global() {
    return c()
  }
  ~d<int> => global() {
    return undefined_fn()
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_no_cycle", r##"@QUICKFUNCS(
  ~a<int> => global() {
    return b()
  }
  ~b<int> => global() {
    return c()
  }
  ~c<int> => global() {
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_lambdas_interpolation", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let f = (n) => n + 1
    let s = $"v = {x} and {x + 1}"
    let r = f(2)
    log: s
    log: r
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_defaults_used", r##"@QUICKFUNCS(
  ~f<int> => global(a<int> = 1, b<int> = 2 + 3, c<string> = "s") {
    return a + b
  }
  ~g<int> => global() {
    return f()
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_unused_vars", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let unused1 = 1
    let used = 2
    let unused2 = used
    log: used
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_typed_collections", r##"@QUICKFUNCS(
  ~f<int> => global(xs<array<int>>, p<tuple<int,string>>) {
    let a = xs.first()
    let b = p.first()
    let c = xs.reverse()
    let d = xs[0]
    log: b
    log: c
    return a + d
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_types_mixed", r##"@QUICKFUNCS(
  ~f<float> => global() {
    return 1 + 2.5
  }
  ~g<long> => global() {
    return 5L + 1
  }
  ~h<double> => global() {
    return 1.5f * 2
  }
  ~i<string> => global() {
    return "a" + "b"
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_many_errors", r##"@QUICKFUNCS(
  ~f => global() {
    return 1
  }
  ~g<int> => global(a<int> = "s") {
    let a = 1
    x = 2
    return "s"
  }
  ~h<int> => global() {
    if: 1 { log: "x" }
    return nofn()
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_with_enums_and_data", r##"@ENUMS(
  Mode { A = 0, B = 1 }
)
@DATA(
  limit = 10
  name = "n"
  nums = [1, 2, 3]
)
@QUICKFUNCS(
  ~f<int> => global() {
    let a = limit + 1
    let b = nums.first()
    let c = Mode.A
    log: name
    log: c
    log: b
    return a
  }
)
"##),
    ("qf_bitwise_assign_float", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let mut f = 1.5
    f &= 1
    f |= 2
    f <<= 1
    let mut s = "a"
    s ^= 1
    log: f
    log: s
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_method_chain_30", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    let t = s.upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper().upper()
    log: t
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_instance_chain_unknown", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let s = "abc"
    let a = s.upper().nomethod()
    let b = s.upper().lower().first()
    let c = [1, 2].reverse().nomethod2()
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
    ("qf_nesting_125", r##"@QUICKFUNCS(
  ~f<int> => global() {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    if: true {
    log: "x"
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    }
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_arith_depth_200", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = 1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1 + (1))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))
    log: a
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_ternary_depth", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? true ? 1 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2 : 2
    log: a
    return 1
  }
)
@DATA(
  x = 1
)
"##),
    ("qf_call_depth", r##"@QUICKFUNCS(
  ~g<int> => global(n<int>) {
    return n
  }
  ~f<int> => global() {
    let a = g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(g(1))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))
    log: a
    return 1
  }
)
@DATA(
  x = 1
)
"##),
];

/// The number of cases in `RECORDING`.
const EXPECTED_CASES: usize = 78;

/// What the owned-tree analyzer reported for each source before it moved to the arena,
/// captured by running the original code. A line `@name|strategy|success=..` starts a case. The
/// `E|id|message|line:col` and `W|id|message|line:col` lines after it are the section's diagnostics
/// (`-` for no position; a cycle is `E|CYCLE|[functions]|-`), followed by the `QID` lines (the
/// qualified identifier resolutions) and the functions registered in the symbol table.
const RECORDING: &str = r##"
@qf_valid_basic|continue|success=true
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 5, column: 3 }", "g": "FunctionSignature { name: \"g\", return_type: Some(Int), parameters: [ParameterInfo { name: \"n\", param_type: Some(Int), has_default_value: false, default_value: None }], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_dup_names|continue|success=false
E|QFUNC001|Function 'f' is defined multiple times|5:3
functions={}
@qf_no_return_type|continue|success=false
E|QFUNC003|Function 'f' must specify a return type|2:3
functions={"f": "FunctionSignature { name: \"f\", return_type: None, parameters: [], scopes: [\"global\"], line: 2, column: 3 }", "g": "FunctionSignature { name: \"g\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 5, column: 3 }"}
@qf_reserved_name_log|continue|success=true
functions={"log": "FunctionSignature { name: \"log\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_reserved_name_let|continue|success=true
functions={}
@qf_many_params|continue|success=false
E|QFUNC004|Function 'f' has 101 parameters, exceeds limit of 100|2:3
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [ParameterInfo { name: \"p0\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p1\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p2\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p3\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p4\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p5\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p6\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p7\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p8\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p9\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p10\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p11\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p12\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p13\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p14\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p15\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p16\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p17\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p18\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p19\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p20\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p21\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p22\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p23\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p24\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p25\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p26\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p27\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p28\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p29\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p30\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p31\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p32\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p33\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p34\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p35\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p36\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p37\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p38\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p39\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p40\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p41\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p42\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p43\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p44\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p45\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p46\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p47\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p48\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p49\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p50\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p51\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p52\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p53\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p54\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p55\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p56\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p57\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p58\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p59\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p60\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p61\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p62\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p63\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p64\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p65\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p66\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p67\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p68\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p69\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p70\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p71\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p72\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p73\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p74\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p75\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p76\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p77\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p78\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p79\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p80\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p81\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p82\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p83\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p84\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p85\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p86\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p87\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p88\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p89\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p90\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p91\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p92\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p93\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p94\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p95\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p96\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p97\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p98\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p99\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"p100\", param_type: Some(Int), has_default_value: false, default_value: None }], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_dup_params|continue|success=false
E|QFUNC005|Parameter 'a' is defined multiple times in function 'f'|2:29
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [ParameterInfo { name: \"a\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"a\", param_type: Some(Int), has_default_value: false, default_value: None }], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_param_type_keyword|continue|success=true
functions={}
@qf_param_reserved|continue|success=true
functions={}
@qf_param_default_mismatch|continue|success=false
E|QFUNC009|Default value type (Int) does not match parameter type (Bool) for 'd' in function 'f'|2:68
E|QFUNC009|Default value type (Int) does not match parameter type (String) for 'c' in function 'f'|2:49
E|QFUNC009|Default value type (String) does not match parameter type (Int) for 'a' in function 'f'|2:21
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [ParameterInfo { name: \"a\", param_type: Some(Int), has_default_value: true, default_value: Some(Value { value: String { value: \"s\", position: Position { line: 2, column: 30 } }, position: Position { line: 2, column: 30 } }) }, ParameterInfo { name: \"b\", param_type: Some(Float), has_default_value: true, default_value: Some(Value { value: Integer { value: 1, position: Position { line: 2, column: 46 } }, position: Position { line: 2, column: 46 } }) }, ParameterInfo { name: \"c\", param_type: Some(String), has_default_value: true, default_value: Some(ArithmeticOp { left: Value { value: Integer { value: 1, position: Position { line: 2, column: 61 } }, position: Position { line: 2, column: 61 } }, operator: \"+\", right: Value { value: Integer { value: 2, position: Position { line: 2, column: 65 } }, position: Position { line: 2, column: 65 } }, position: Position { line: 2, column: 63 } }) }, ParameterInfo { name: \"d\", param_type: Some(Bool), has_default_value: true, default_value: Some(Value { value: Integer { value: 5, position: Position { line: 2, column: 78 } }, position: Position { line: 2, column: 78 } }) }, ParameterInfo { name: \"e\", param_type: Some(Array), has_default_value: true, default_value: Some(Value { value: Array { values: [Integer { value: 1, position: Position { line: 2, column: 93 } }, Integer { value: 2, position: Position { line: 2, column: 96 } }], position: Position { line: 2, column: 92 } }, position: Position { line: 2, column: 92 } }) }], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_param_default_mismatch|halt|success=false
functions={}
@qf_param_order|continue|success=false
E|QFUNC008|Non-default parameter 'b' cannot follow default parameters in function 'f'|2:33
E|QFUNC008|Non-default parameter 'd' cannot follow default parameters in function 'f'|2:53
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [ParameterInfo { name: \"a\", param_type: Some(Int), has_default_value: true, default_value: Some(Value { value: Integer { value: 1, position: Position { line: 2, column: 30 } }, position: Position { line: 2, column: 30 } }) }, ParameterInfo { name: \"b\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"c\", param_type: Some(Int), has_default_value: true, default_value: Some(Value { value: Integer { value: 2, position: Position { line: 2, column: 50 } }, position: Position { line: 2, column: 50 } }) }, ParameterInfo { name: \"d\", param_type: Some(Int), has_default_value: false, default_value: None }], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_param_order|halt|success=false
E|QFUNC008|Non-default parameter 'b' cannot follow default parameters in function 'f'|2:33
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [ParameterInfo { name: \"a\", param_type: Some(Int), has_default_value: true, default_value: Some(Value { value: Integer { value: 1, position: Position { line: 2, column: 30 } }, position: Position { line: 2, column: 30 } }) }, ParameterInfo { name: \"b\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"c\", param_type: Some(Int), has_default_value: true, default_value: Some(Value { value: Integer { value: 2, position: Position { line: 2, column: 50 } }, position: Position { line: 2, column: 50 } }) }, ParameterInfo { name: \"d\", param_type: Some(Int), has_default_value: false, default_value: None }], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_no_scope|continue|success=true
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_scope_forms|continue|success=true
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\", \"data.users\"], line: 2, column: 3 }", "g": "FunctionSignature { name: \"g\", return_type: Some(Int), parameters: [], scopes: [\"user.profile\"], line: 5, column: 3 }"}
@qf_empty_body|continue|success=false
E|QFUNC011|Function 'f' has an empty body but declares return type Int|2:3
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_body_too_large|continue|success=false
E|QFUNC012|Function 'f' has 1002 statements, exceeds limit of 1000|2:3
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_nesting_deep|continue|success=false
E|QFUNC014|Function 'f' exceeds maximum nesting depth of 50|54:5
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_nesting_ok|continue|success=true
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_not_all_paths|continue|success=false
E|QFUNC013|Function 'f' (return type Int) does not return on all code paths|2:3
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_all_paths_ok|continue|success=true
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }", "g": "FunctionSignature { name: \"g\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 9, column: 3 }"}
@qf_chk_no_miss|continue|success=false
E|QFUNC013|Function 'f' (return type Int) does not return on all code paths|2:3
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_if_non_bool|continue|success=false
E|QFUNC016|If statement condition must be boolean, got Int|3:9
E|QFUNC016|If statement condition must be boolean, got String|4:9
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_if_non_bool|halt|success=false
E|QFUNC016|If statement condition must be boolean, got Int|3:9
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_return_mismatch|continue|success=false
E|QFUNC015|Function 'f' returns String but declared return type is Int|3:12
E|QFUNC015|Function 'g' returns Int but declared return type is String|6:12
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }", "g": "FunctionSignature { name: \"g\", return_type: Some(String), parameters: [], scopes: [\"global\"], line: 5, column: 3 }", "h": "FunctionSignature { name: \"h\", return_type: Some(Float), parameters: [], scopes: [\"global\"], line: 8, column: 3 }", "i": "FunctionSignature { name: \"i\", return_type: Some(Bool), parameters: [], scopes: [\"global\"], line: 11, column: 3 }"}
@qf_return_mismatch|halt|success=false
E|QFUNC015|Function 'f' returns String but declared return type is Int|3:12
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }", "g": "FunctionSignature { name: \"g\", return_type: Some(String), parameters: [], scopes: [\"global\"], line: 5, column: 3 }", "h": "FunctionSignature { name: \"h\", return_type: Some(Float), parameters: [], scopes: [\"global\"], line: 8, column: 3 }", "i": "FunctionSignature { name: \"i\", return_type: Some(Bool), parameters: [], scopes: [\"global\"], line: 11, column: 3 }"}
@qf_assign_errors|continue|success=false
E|QFUNC018|Cannot reassign const variable 'c' in function 'f'|5:9
E|QFUNC019|Cannot assign Int to variable 's' of type String|7:9
E|QFUNC019|Cannot assign String to variable 'u' of type Int|9:9
E|QFUNC072|Variable 'x' used before declaration in function 'f'|3:9
W|QFUNC_WARN005|Variable 'c' declared but never used in function 'f'|2:3
W|QFUNC_WARN005|Variable 'ok' declared but never used in function 'f'|2:3
W|QFUNC_WARN005|Variable 's' declared but never used in function 'f'|2:3
W|QFUNC_WARN005|Variable 'u' declared but never used in function 'f'|2:3
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_assign_errors|halt|success=false
E|QFUNC072|Variable 'x' used before declaration in function 'f'|3:9
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_arith_assign|continue|success=false
E|QFUNC020|Variable 'y' used before assignment in function 'f'|3:10
E|QFUNC021|Cannot modify const variable 'c' with '+='|5:10
E|QFUNC061|Cannot use '+=' to concatenate string with non-string in function 'f'|11:10
E|QFUNC062|Arithmetic assignment '*=' requires numeric type, left operand is String|8:10
E|QFUNC062|Arithmetic assignment '-=' requires numeric type, left operand is String|7:10
E|QFUNC063|Arithmetic assignment '<<=' requires numeric type, right operand is String|13:11
E|QFUNC065|Bitwise assignment '&=' requires int or long, got Double|12:10
E|QFUNC065|Bitwise assignment '<<=' requires int or long, got String|13:11
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_arith_assign|halt|success=false
E|QFUNC020|Variable 'y' used before assignment in function 'f'|3:10
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_var_decl|continue|success=false
E|QFUNC069|Variable 'a' already declared in function 'f'|6:5
E|QFUNC071|Variable 'b' declared as Int but assigned value of type String|7:5
E|QFUNC071|Variable 'e' declared as Bool but assigned value of type Int|10:5
W|QFUNC_WARN005|Variable 'a' declared but never used in function 'f'|2:3
W|QFUNC_WARN005|Variable 'b' declared but never used in function 'f'|2:3
W|QFUNC_WARN005|Variable 'c' declared but never used in function 'f'|2:3
W|QFUNC_WARN005|Variable 'd' declared but never used in function 'f'|2:3
W|QFUNC_WARN005|Variable 'e' declared but never used in function 'f'|2:3
W|QFUNC_WARN005|Variable 'log' declared but never used in function 'f'|2:3
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_var_decl|halt|success=false
E|QFUNC069|Variable 'a' already declared in function 'f'|6:5
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_arith_expr|continue|success=false
E|QFUNC026|Cannot concatenate string with Int in function 'f'|3:13
E|QFUNC026|Cannot concatenate string with Int in function 'f'|4:13
E|QFUNC027|Left operand of '*' must be numeric, got Bool in 'f'|6:13
E|QFUNC027|Left operand of '-' must be numeric, got String in 'f'|5:13
E|QFUNC028|Right operand of '+' must be numeric, got Bool in 'f'|7:17
E|QFUNC028|Right operand of '-' must be numeric, got String in 'f'|5:19
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_arith_expr|halt|success=false
E|QFUNC026|Cannot concatenate string with Int in function 'f'|3:13
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_bitwise_expr|continue|success=false
E|QFUNC030|Left operand of '&' must be int or long, got String in 'f'|3:13
E|QFUNC031|Right operand of '^' must be int or long, got Bool in 'f'|5:17
E|QFUNC031|Right operand of '|' must be int or long, got Double in 'f'|4:17
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_bitwise_expr|halt|success=false
E|QFUNC030|Left operand of '&' must be int or long, got String in 'f'|3:13
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_comparison_expr|continue|success=false
E|QFUNC033|Operator '<' requires numeric types, got String and Int in 'f'|3:13
E|QFUNC033|Operator '>=' requires numeric types, got Bool and Bool in 'f'|7:13
W|QFUNC_WARN002|Comparing incompatible types Int and String in function 'f'|4:13
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_comparison_expr|halt|success=false
E|QFUNC033|Operator '<' requires numeric types, got String and Int in 'f'|3:13
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_logical_expr|continue|success=false
E|QFUNC035|Left operand of '&&' must be bool, got Int in 'f'|3:13
E|QFUNC036|Right operand of '||' must be bool, got String in 'f'|4:21
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_logical_expr|halt|success=false
E|QFUNC035|Left operand of '&&' must be bool, got Int in 'f'|3:13
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_unary_expr|continue|success=false
E|QFUNC038|Logical NOT requires bool, got Int in 'f'|3:14
E|QFUNC038|Logical NOT requires bool, got Int in 'f'|7:17
E|QFUNC039|Bitwise NOT (~?) requires int or long, got String in 'f'|6:15
E|QFUNC040|Unary '+' requires numeric type, got Bool in 'f'|5:14
E|QFUNC040|Unary '-' requires numeric type, got String in 'f'|4:14
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_unary_expr|halt|success=false
E|QFUNC038|Logical NOT requires bool, got Int in 'f'|3:14
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_ternary_expr|continue|success=false
E|QFUNC041|Ternary condition must be bool, got Int in 'f'|3:13
W|QFUNC_WARN003|Ternary branches have incompatible types Int and String in 'f'|4:13
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_ternary_expr|halt|success=false
E|QFUNC041|Ternary condition must be bool, got Int in 'f'|3:13
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_calls|continue|success=false
E|QFUNC042|Function 'nofunc' is not defined in @QUICKFUNCS|0:0
E|QFUNC043|Function 'g' expects 1 arguments, got 0|0:0
E|QFUNC043|Function 'g' expects 1 arguments, got 2|0:0
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 5, column: 3 }", "g": "FunctionSignature { name: \"g\", return_type: Some(Int), parameters: [ParameterInfo { name: \"n\", param_type: Some(Int), has_default_value: false, default_value: None }], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_calls|halt|success=false
E|QFUNC042|Function 'nofunc' is not defined in @QUICKFUNCS|0:0
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 5, column: 3 }", "g": "FunctionSignature { name: \"g\", return_type: Some(Int), parameters: [ParameterInfo { name: \"n\", param_type: Some(Int), has_default_value: false, default_value: None }], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_instance_methods|continue|success=true
QID QualifiedIdentifierKey { position: Position { line: 10, column: 13 }, parts: ["n", "upper"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["n", "upper"], is_call: true, position: Position { line: 10, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 5, column: 13 }, parts: ["s", "nomethod"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["s", "nomethod"], is_call: true, position: Position { line: 5, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 6, column: 13 }, parts: ["a", "first"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["a", "first"], is_call: true, position: Position { line: 6, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 13 }, parts: ["s", "upper"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["s", "upper"], is_call: true, position: Position { line: 7, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 8, column: 13 }, parts: ["a", "nope"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["a", "nope"], is_call: true, position: Position { line: 8, column: 13 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_method_chain_deep|continue|success=true
QID QualifiedIdentifierKey { position: Position { line: 4, column: 13 }, parts: ["s", "upper"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["s", "upper"], is_call: true, position: Position { line: 4, column: 13 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_method_chain_ok|continue|success=true
QID QualifiedIdentifierKey { position: Position { line: 4, column: 13 }, parts: ["s", "upper"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["s", "upper"], is_call: true, position: Position { line: 4, column: 13 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_static_calls|continue|success=false
E|QFUNC050|Static object 'Math' has no method 'nomethod'|0:0
W|QFUNC_WARN001|Identifier 'Nope' not found in scope — will be resolved at runtime|3:13
QID QualifiedIdentifierKey { position: Position { line: 3, column: 13 }, parts: ["Nope", "f"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("unknown"), parts: ["Nope", "f"], is_call: true, position: Position { line: 3, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 4, column: 13 }, parts: ["Math", "nomethod"], is_call: true } => QualifiedIdentifierResolution { resolved_type: StaticObjectAccess, context: Some("Math"), parts: ["Math", "nomethod"], is_call: true, position: Position { line: 4, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 5, column: 13 }, parts: ["Math", "sqrt"], is_call: true } => QualifiedIdentifierResolution { resolved_type: StaticObjectAccess, context: Some("Math"), parts: ["Math", "sqrt"], is_call: true, position: Position { line: 5, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 6, column: 13 }, parts: ["Array", "of"], is_call: true } => QualifiedIdentifierResolution { resolved_type: StaticObjectAccess, context: Some("Array"), parts: ["Array", "of"], is_call: true, position: Position { line: 6, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 13 }, parts: ["Random", "choice"], is_call: true } => QualifiedIdentifierResolution { resolved_type: StaticObjectAccess, context: Some("Random"), parts: ["Random", "choice"], is_call: true, position: Position { line: 7, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 8, column: 13 }, parts: ["DateTime", "now"], is_call: true } => QualifiedIdentifierResolution { resolved_type: StaticObjectAccess, context: Some("DateTime"), parts: ["DateTime", "now"], is_call: true, position: Position { line: 8, column: 13 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_static_calls|halt|success=false
E|QFUNC050|Static object 'Math' has no method 'nomethod'|0:0
W|QFUNC_WARN001|Identifier 'Nope' not found in scope — will be resolved at runtime|3:13
QID QualifiedIdentifierKey { position: Position { line: 3, column: 13 }, parts: ["Nope", "f"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("unknown"), parts: ["Nope", "f"], is_call: true, position: Position { line: 3, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 4, column: 13 }, parts: ["Math", "nomethod"], is_call: true } => QualifiedIdentifierResolution { resolved_type: StaticObjectAccess, context: Some("Math"), parts: ["Math", "nomethod"], is_call: true, position: Position { line: 4, column: 13 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_enums|continue|success=false
E|QFUNC052|Enum 'LogLevel' does not have value 'NOPE'|7:13
W|QFUNC_WARN001|Identifier 'Missing' not found in scope — will be resolved at runtime|8:13
QID QualifiedIdentifierKey { position: Position { line: 6, column: 13 }, parts: ["LogLevel", "INFO"], is_call: false } => QualifiedIdentifierResolution { resolved_type: LocalEnumAccess, context: Some("LogLevel"), parts: ["LogLevel", "INFO"], is_call: false, position: Position { line: 6, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 13 }, parts: ["LogLevel", "NOPE"], is_call: false } => QualifiedIdentifierResolution { resolved_type: LocalEnumAccess, context: Some("LogLevel"), parts: ["LogLevel", "NOPE"], is_call: false, position: Position { line: 7, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 8, column: 13 }, parts: ["Missing", "A"], is_call: false } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("unknown"), parts: ["Missing", "A"], is_call: false, position: Position { line: 8, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 9, column: 19 }, parts: ["LogLevel", "DEBUG"], is_call: false } => QualifiedIdentifierResolution { resolved_type: LocalEnumAccess, context: Some("LogLevel"), parts: ["LogLevel", "DEBUG"], is_call: false, position: Position { line: 9, column: 19 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 5, column: 3 }"}
@qf_enums|halt|success=false
E|QFUNC052|Enum 'LogLevel' does not have value 'NOPE'|7:13
QID QualifiedIdentifierKey { position: Position { line: 6, column: 13 }, parts: ["LogLevel", "INFO"], is_call: false } => QualifiedIdentifierResolution { resolved_type: LocalEnumAccess, context: Some("LogLevel"), parts: ["LogLevel", "INFO"], is_call: false, position: Position { line: 6, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 13 }, parts: ["LogLevel", "NOPE"], is_call: false } => QualifiedIdentifierResolution { resolved_type: LocalEnumAccess, context: Some("LogLevel"), parts: ["LogLevel", "NOPE"], is_call: false, position: Position { line: 7, column: 13 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 5, column: 3 }"}
@qf_value_limits|continue|success=false
E|QFUNC059|Tuple has 7 arguments, exceeds limit of 6|3:13
E|QFUNC060|Duplicate object key 'k' in function 'f'|4:21
E|QFUNC077|Array element 2 has type String but array expects Int (from first element) in function 'f'|5:13
W|QFUNC_WARN001|Identifier 'x' not found in local scope or symbol table in function 'f'|7:17
W|QFUNC_WARN008|Cannot infer type of array element 2 in function 'f'|7:13
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_value_limits|halt|success=false
E|QFUNC059|Tuple has 7 arguments, exceeds limit of 6|3:13
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_big_array|continue|success=false
E|QFUNC057|Array has 10001 elements, exceeds limit of 10000|3:13
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_big_object|continue|success=false
E|QFUNC058|Object has 1001 properties, exceeds limit of 1000|3:13
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_unknown_identifiers|continue|success=true
W|QFUNC_WARN001|Identifier 'foo' not found in scope — will be resolved at runtime|4:13
W|QFUNC_WARN001|Identifier 'foo' not found in scope — will be resolved at runtime|5:13
W|QFUNC_WARN001|Identifier 'x' not found in local scope or symbol table in function 'f'|6:13
W|QFUNC_WARN001|Identifier 'x' not found in scope — will be resolved at runtime|7:13
W|QFUNC_WARN001|Identifier 'y' not found in local scope or symbol table in function 'f'|3:13
QID QualifiedIdentifierKey { position: Position { line: 4, column: 13 }, parts: ["foo", "bar"], is_call: false } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("unknown"), parts: ["foo", "bar"], is_call: false, position: Position { line: 4, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 5, column: 13 }, parts: ["foo", "baz"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("unknown"), parts: ["foo", "baz"], is_call: true, position: Position { line: 5, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 13 }, parts: ["x", "field"], is_call: false } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("unknown"), parts: ["x", "field"], is_call: false, position: Position { line: 7, column: 13 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_qualified_forms|continue|success=true
QID QualifiedIdentifierKey { position: Position { line: 10, column: 13 }, parts: ["o", "k"], is_call: false } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["o", "k"], is_call: false, position: Position { line: 10, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 4, column: 13 }, parts: ["s", "length"], is_call: false } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["s", "length"], is_call: false, position: Position { line: 4, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 5, column: 13 }, parts: ["Math", "PI"], is_call: false } => QualifiedIdentifierResolution { resolved_type: StaticObjectAccess, context: Some("Math"), parts: ["Math", "PI"], is_call: false, position: Position { line: 5, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 6, column: 13 }, parts: ["DateTime", "now"], is_call: true } => QualifiedIdentifierResolution { resolved_type: StaticObjectAccess, context: Some("DateTime"), parts: ["DateTime", "now"], is_call: true, position: Position { line: 6, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 8, column: 13 }, parts: ["t", "first"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["t", "first"], is_call: true, position: Position { line: 8, column: 13 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_expr_depth|continue|success=true
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_cycles|continue|success=false
E|QFUNC042|Function 'undefined_fn' is not defined in @QUICKFUNCS|0:0
functions={"a": "FunctionSignature { name: \"a\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }", "b": "FunctionSignature { name: \"b\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 5, column: 3 }", "c": "FunctionSignature { name: \"c\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 8, column: 3 }", "d": "FunctionSignature { name: \"d\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 11, column: 3 }"}
@qf_no_cycle|continue|success=true
functions={"a": "FunctionSignature { name: \"a\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }", "b": "FunctionSignature { name: \"b\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 5, column: 3 }", "c": "FunctionSignature { name: \"c\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 8, column: 3 }"}
@qf_lambdas_interpolation|continue|success=true
W|QFUNC_WARN001|Identifier 'x' not found in local scope or symbol table in function 'f'|1:1
W|QFUNC_WARN001|Identifier 'x' not found in local scope or symbol table in function 'f'|1:1
W|QFUNC_WARN005|Variable 'f' declared but never used in function 'f'|2:3
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_defaults_used|continue|success=false
E|QFUNC043|Function 'f' expects 3 arguments, got 0|0:0
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [ParameterInfo { name: \"a\", param_type: Some(Int), has_default_value: true, default_value: Some(Value { value: Integer { value: 1, position: Position { line: 2, column: 30 } }, position: Position { line: 2, column: 30 } }) }, ParameterInfo { name: \"b\", param_type: Some(Int), has_default_value: true, default_value: Some(ArithmeticOp { left: Value { value: Integer { value: 2, position: Position { line: 2, column: 42 } }, position: Position { line: 2, column: 42 } }, operator: \"+\", right: Value { value: Integer { value: 3, position: Position { line: 2, column: 46 } }, position: Position { line: 2, column: 46 } }, position: Position { line: 2, column: 44 } }) }, ParameterInfo { name: \"c\", param_type: Some(String), has_default_value: true, default_value: Some(Value { value: String { value: \"s\", position: Position { line: 2, column: 61 } }, position: Position { line: 2, column: 61 } }) }], scopes: [\"global\"], line: 2, column: 3 }", "g": "FunctionSignature { name: \"g\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 5, column: 3 }"}
@qf_unused_vars|continue|success=true
W|QFUNC_WARN005|Variable 'unused1' declared but never used in function 'f'|2:3
W|QFUNC_WARN005|Variable 'unused2' declared but never used in function 'f'|2:3
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_typed_collections|continue|success=true
QID QualifiedIdentifierKey { position: Position { line: 3, column: 13 }, parts: ["xs", "first"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["xs", "first"], is_call: true, position: Position { line: 3, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 4, column: 13 }, parts: ["p", "first"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["p", "first"], is_call: true, position: Position { line: 4, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 5, column: 13 }, parts: ["xs", "reverse"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["xs", "reverse"], is_call: true, position: Position { line: 5, column: 13 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [ParameterInfo { name: \"xs\", param_type: Some(TypedArray(Int)), has_default_value: false, default_value: None }, ParameterInfo { name: \"p\", param_type: Some(TypedTuple([Some(Int), Some(String), None, None, None, None])), has_default_value: false, default_value: None }], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_types_mixed|continue|success=true
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Float), parameters: [], scopes: [\"global\"], line: 2, column: 3 }", "g": "FunctionSignature { name: \"g\", return_type: Some(Long), parameters: [], scopes: [\"global\"], line: 5, column: 3 }", "h": "FunctionSignature { name: \"h\", return_type: Some(Double), parameters: [], scopes: [\"global\"], line: 8, column: 3 }", "i": "FunctionSignature { name: \"i\", return_type: Some(String), parameters: [], scopes: [\"global\"], line: 11, column: 3 }"}
@qf_many_errors|continue|success=false
E|QFUNC003|Function 'f' must specify a return type|2:3
E|QFUNC009|Default value type (String) does not match parameter type (Int) for 'a' in function 'g'|5:21
E|QFUNC015|Function 'g' returns String but declared return type is Int|8:12
E|QFUNC016|If statement condition must be boolean, got Int|11:9
E|QFUNC042|Function 'nofn' is not defined in @QUICKFUNCS|0:0
E|QFUNC069|Variable 'a' already declared in function 'g'|6:5
E|QFUNC072|Variable 'x' used before declaration in function 'g'|7:9
functions={"f": "FunctionSignature { name: \"f\", return_type: None, parameters: [], scopes: [\"global\"], line: 2, column: 3 }", "g": "FunctionSignature { name: \"g\", return_type: Some(Int), parameters: [ParameterInfo { name: \"a\", param_type: Some(Int), has_default_value: true, default_value: Some(Value { value: String { value: \"s\", position: Position { line: 5, column: 30 } }, position: Position { line: 5, column: 30 } }) }], scopes: [\"global\"], line: 5, column: 3 }", "h": "FunctionSignature { name: \"h\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 10, column: 3 }"}
@qf_many_errors|halt|success=false
functions={}
@qf_with_enums_and_data|continue|success=true
W|QFUNC_WARN001|Identifier 'limit' not found in local scope or symbol table in function 'f'|11:13
W|QFUNC_WARN001|Identifier 'name' not found in local scope or symbol table in function 'f'|14:10
W|QFUNC_WARN001|Identifier 'nums' not found in scope — will be resolved at runtime|12:13
QID QualifiedIdentifierKey { position: Position { line: 12, column: 13 }, parts: ["nums", "first"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("unknown"), parts: ["nums", "first"], is_call: true, position: Position { line: 12, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 13, column: 13 }, parts: ["Mode", "A"], is_call: false } => QualifiedIdentifierResolution { resolved_type: LocalEnumAccess, context: Some("Mode"), parts: ["Mode", "A"], is_call: false, position: Position { line: 13, column: 13 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 10, column: 3 }"}
@qf_bitwise_assign_float|continue|success=false
E|QFUNC062|Arithmetic assignment '^=' requires numeric type, left operand is String|8:10
E|QFUNC064|Bitwise assignment '&=' requires int or long, got Double|4:10
E|QFUNC064|Bitwise assignment '<<=' requires int or long, got Double|6:11
E|QFUNC064|Bitwise assignment '^=' requires int or long, got String|8:10
E|QFUNC064|Bitwise assignment '|=' requires int or long, got Double|5:10
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_bitwise_assign_float|halt|success=false
E|QFUNC064|Bitwise assignment '&=' requires int or long, got Double|4:10
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_method_chain_30|continue|success=false
E|QFUNC066|Method chain depth (28) exceeds maximum of 10 in function 'f'|4:238
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_instance_chain_unknown|continue|success=false
E|QFUNC047|Type 'TypedArray(Int)' has no instance method 'nomethod2'|6:19
QID QualifiedIdentifierKey { position: Position { line: 4, column: 13 }, parts: ["s", "upper"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["s", "upper"], is_call: true, position: Position { line: 4, column: 13 } }
QID QualifiedIdentifierKey { position: Position { line: 5, column: 13 }, parts: ["s", "upper"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ObjectPropertyAccess, context: Some("local"), parts: ["s", "upper"], is_call: true, position: Position { line: 5, column: 13 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_nesting_125|continue|success=false
E|QFUNC014|Function 'f' exceeds maximum nesting depth of 50|54:5
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_arith_depth_200|continue|success=true
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_ternary_depth|continue|success=true
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 3 }"}
@qf_call_depth|continue|success=true
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 5, column: 3 }", "g": "FunctionSignature { name: \"g\", return_type: Some(Int), parameters: [ParameterInfo { name: \"n\", param_type: Some(Int), has_default_value: false, default_value: None }], scopes: [\"global\"], line: 2, column: 3 }"}
"##;

/// The number of blocks in `HAND_RECORDING`.
const EXPECTED_HAND_BLOCKS: usize = 27;

/// What the owned-tree analyzer reported for each hand-built section before it moved to the arena.
/// A line `@name|strategy` starts a block, followed by the success flag, the errors and warnings,
/// the resolutions, the registered functions and the records left in the error manager.
const HAND_RECORDING: &str = r##"
@hqf_names_and_types|halt
success=false
E [QFUNC002] INVALID_FUNCTION_NAME | Function name '1bad' is not a valid identifier | Function names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 2, column: 1 })
functions={"1bad": "FunctionSignature { name: \"1bad\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }", "badscope": "FunctionSignature { name: \"badscope\", return_type: Some(Int), parameters: [], scopes: [\"bad scope!\"], line: 2, column: 1 }", "if": "FunctionSignature { name: \"if\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }", "noscope": "FunctionSignature { name: \"noscope\", return_type: Some(Int), parameters: [], scopes: [], line: 2, column: 1 }", "params": "FunctionSignature { name: \"params\", return_type: Some(Int), parameters: [ParameterInfo { name: \"if\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"int\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"pt\", param_type: Some(Function), has_default_value: false, default_value: None }], scopes: [\"global\"], line: 2, column: 1 }", "rt": "FunctionSignature { name: \"rt\", return_type: Some(Function), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_names_and_types|continue
success=false
E [QFUNC002B] RESERVED_KEYWORD_AS_NAME | 'if' is a reserved keyword and cannot be used as an identifier | Choose a different name for function 'if' | Some(Position { line: 2, column: 1 })
E [QFUNC002] INVALID_FUNCTION_NAME | Function name '1bad' is not a valid identifier | Function names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 2, column: 1 })
E [QFUNC006B] RESERVED_KEYWORD_AS_PARAMETER | 'if' is a reserved keyword and cannot be used as an identifier | Choose a different name for parameter 'if' in function 'params' | Some(Position { line: 2, column: 9 })
E [QFUNC006C] DATA_TYPE_KEYWORD_AS_PARAMETER | Parameter 'int' in function 'params' cannot use a data type keyword as name | Use a different name like 'myInt' or 'intValue' | Some(Position { line: 2, column: 9 })
E [QFUNC006] INVALID_PARAMETER_NAME | Parameter '1x' in function 'params' is not a valid identifier | Parameter names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 2, column: 9 })
E [QFUNC010] INVALID_SCOPE_SYNTAX | Function 'badscope' has invalid scope syntax: 'bad scope!' | Scope must be 'global' or a valid dotted path (e.g. 'user.profile'). | Some(Position { line: 2, column: 1 })
E [QFUNC015] RETURN_TYPE_MISMATCH | Function 'rt' returns Int but declared return type is Function | Change the return value to match Function or update the function return type | Some(Position { line: 1, column: 5 })
W [QFUNC_WARN006] Function 'noscope' has no scope declaration — callable only within its definition context | Some(Position { line: 2, column: 1 })
functions={"1bad": "FunctionSignature { name: \"1bad\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }", "badscope": "FunctionSignature { name: \"badscope\", return_type: Some(Int), parameters: [], scopes: [\"bad scope!\"], line: 2, column: 1 }", "if": "FunctionSignature { name: \"if\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }", "noscope": "FunctionSignature { name: \"noscope\", return_type: Some(Int), parameters: [], scopes: [], line: 2, column: 1 }", "params": "FunctionSignature { name: \"params\", return_type: Some(Int), parameters: [ParameterInfo { name: \"if\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"int\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"pt\", param_type: Some(Function), has_default_value: false, default_value: None }], scopes: [\"global\"], line: 2, column: 1 }", "rt": "FunctionSignature { name: \"rt\", return_type: Some(Function), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_names_and_types|recover
success=false
E [QFUNC002B] RESERVED_KEYWORD_AS_NAME | 'if' is a reserved keyword and cannot be used as an identifier | Choose a different name for function 'if' | Some(Position { line: 2, column: 1 })
E [QFUNC002] INVALID_FUNCTION_NAME | Function name '1bad' is not a valid identifier | Function names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 2, column: 1 })
E [QFUNC006B] RESERVED_KEYWORD_AS_PARAMETER | 'if' is a reserved keyword and cannot be used as an identifier | Choose a different name for parameter 'if' in function 'params' | Some(Position { line: 2, column: 9 })
E [QFUNC006C] DATA_TYPE_KEYWORD_AS_PARAMETER | Parameter 'int' in function 'params' cannot use a data type keyword as name | Use a different name like 'myInt' or 'intValue' | Some(Position { line: 2, column: 9 })
E [QFUNC006] INVALID_PARAMETER_NAME | Parameter '1x' in function 'params' is not a valid identifier | Parameter names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 2, column: 9 })
E [QFUNC010] INVALID_SCOPE_SYNTAX | Function 'badscope' has invalid scope syntax: 'bad scope!' | Scope must be 'global' or a valid dotted path (e.g. 'user.profile'). | Some(Position { line: 2, column: 1 })
E [QFUNC015] RETURN_TYPE_MISMATCH | Function 'rt' returns Int but declared return type is Function | Change the return value to match Function or update the function return type | Some(Position { line: 1, column: 5 })
W [QFUNC_WARN006] Function 'noscope' has no scope declaration — callable only within its definition context | Some(Position { line: 2, column: 1 })
functions={"1bad": "FunctionSignature { name: \"1bad\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }", "badscope": "FunctionSignature { name: \"badscope\", return_type: Some(Int), parameters: [], scopes: [\"bad scope!\"], line: 2, column: 1 }", "if": "FunctionSignature { name: \"if\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }", "noscope": "FunctionSignature { name: \"noscope\", return_type: Some(Int), parameters: [], scopes: [], line: 2, column: 1 }", "params": "FunctionSignature { name: \"params\", return_type: Some(Int), parameters: [ParameterInfo { name: \"if\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"int\", param_type: Some(Int), has_default_value: false, default_value: None }, ParameterInfo { name: \"pt\", param_type: Some(Function), has_default_value: false, default_value: None }], scopes: [\"global\"], line: 2, column: 1 }", "rt": "FunctionSignature { name: \"rt\", return_type: Some(Function), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_variable_names|halt
success=false
E [QFUNC067] INVALID_VARIABLE_NAME | Invalid variable name '1bad' in function 'f' | Variable names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 3, column: 3 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_variable_names|continue
success=false
E [QFUNC017] INVALID_VARIABLE_NAME | Invalid variable name '1x' in function 'f' | Variable names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 1, column: 5 })
E [QFUNC020] UNDEFINED_VARIABLE | Variable '1y' used before assignment in function 'f' | Declare the variable before using arithmetic assignment. | Some(Position { line: 1, column: 5 })
E [QFUNC022] INVALID_ARITHMETIC_ASSIGN_OP | Invalid arithmetic assignment operator '??=' | Valid operators: +=, -=, *=, /=, %=, **=, &=, |=, ^=, <<=, >>= | Some(Position { line: 1, column: 5 })
E [QFUNC067B] DATA_TYPE_KEYWORD_AS_VARIABLE | Variable 'int' in function 'f' cannot use a data type keyword as name | Use a different name like 'myInt' or 'intValue' | Some(Position { line: 3, column: 3 })
E [QFUNC067] INVALID_VARIABLE_NAME | Invalid variable name '1bad' in function 'f' | Variable names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 3, column: 3 })
E [QFUNC068] RESERVED_KEYWORD_AS_VARIABLE | 'if' is a reserved keyword and cannot be used as an identifier | Choose a different name for variable 'if' | Some(Position { line: 3, column: 3 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_variable_names|recover
success=false
E [QFUNC017] INVALID_VARIABLE_NAME | Invalid variable name '1x' in function 'f' | Variable names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 1, column: 5 })
E [QFUNC020] UNDEFINED_VARIABLE | Variable '1y' used before assignment in function 'f' | Declare the variable before using arithmetic assignment. | Some(Position { line: 1, column: 5 })
E [QFUNC022] INVALID_ARITHMETIC_ASSIGN_OP | Invalid arithmetic assignment operator '??=' | Valid operators: +=, -=, *=, /=, %=, **=, &=, |=, ^=, <<=, >>= | Some(Position { line: 1, column: 5 })
E [QFUNC067B] DATA_TYPE_KEYWORD_AS_VARIABLE | Variable 'int' in function 'f' cannot use a data type keyword as name | Use a different name like 'myInt' or 'intValue' | Some(Position { line: 3, column: 3 })
E [QFUNC067] INVALID_VARIABLE_NAME | Invalid variable name '1bad' in function 'f' | Variable names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 3, column: 3 })
E [QFUNC068] RESERVED_KEYWORD_AS_VARIABLE | 'if' is a reserved keyword and cannot be used as an identifier | Choose a different name for variable 'if' | Some(Position { line: 3, column: 3 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_object_creation|halt
success=false
E [QFUNC023] INVALID_VARIABLE_NAME | Invalid variable name '1bad' in function 'f' | Variable names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 6, column: 0 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_object_creation|continue
success=false
E [QFUNC023] INVALID_VARIABLE_NAME | Invalid variable name '1bad' in function 'f' | Variable names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 6, column: 0 })
E [QFUNC024] CONST_REASSIGNMENT | Cannot reassign const variable 'c' in function 'f' | Remove 'const' to allow reassignment. | Some(Position { line: 6, column: 0 })
W [QFUNC_WARN005] Variable 'c' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'o' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_object_creation|recover
success=false
E [QFUNC023] INVALID_VARIABLE_NAME | Invalid variable name '1bad' in function 'f' | Variable names must start with a letter and contain only alphanumeric characters and underscores. | Some(Position { line: 6, column: 0 })
E [QFUNC024] CONST_REASSIGNMENT | Cannot reassign const variable 'c' in function 'f' | Remove 'const' to allow reassignment. | Some(Position { line: 6, column: 0 })
W [QFUNC_WARN005] Variable 'c' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'o' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_operators|halt
success=false
E [QFUNC025] INVALID_ARITHMETIC_OPERATOR | Invalid arithmetic operator '??' in function 'f' | Valid operators: +, -, *, /, %, **, %%, %&, &% | Some(Position { line: 1, column: 5 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_operators|continue
success=false
E [QFUNC025] INVALID_ARITHMETIC_OPERATOR | Invalid arithmetic operator '??' in function 'f' | Valid operators: +, -, *, /, %, **, %%, %&, &% | Some(Position { line: 1, column: 5 })
E [QFUNC029] INVALID_BITWISE_OPERATOR | Invalid bitwise operator '??' in function 'f' | Valid operators: &, |, ^, <<, >> | Some(Position { line: 1, column: 5 })
E [QFUNC032] INVALID_COMPARISON_OPERATOR | Invalid comparison operator '??' in function 'f' | Valid operators: ==, !=, >, <, >=, <= | Some(Position { line: 1, column: 5 })
E [QFUNC034] INVALID_LOGICAL_OPERATOR | Invalid logical operator '??' in function 'f' | Valid operators: &&, ||, and, or | Some(Position { line: 1, column: 5 })
E [QFUNC037] INVALID_UNARY_OPERATOR | Invalid unary operator '??' in function 'f' | Valid operators: !, not, -, +, ~? | Some(Position { line: 1, column: 5 })
W [QFUNC_WARN005] Variable 'a' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'b' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'c' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'd' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'e' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_operators|recover
success=false
E [QFUNC025] INVALID_ARITHMETIC_OPERATOR | Invalid arithmetic operator '??' in function 'f' | Valid operators: +, -, *, /, %, **, %%, %&, &% | Some(Position { line: 1, column: 5 })
E [QFUNC029] INVALID_BITWISE_OPERATOR | Invalid bitwise operator '??' in function 'f' | Valid operators: &, |, ^, <<, >> | Some(Position { line: 1, column: 5 })
E [QFUNC032] INVALID_COMPARISON_OPERATOR | Invalid comparison operator '??' in function 'f' | Valid operators: ==, !=, >, <, >=, <= | Some(Position { line: 1, column: 5 })
E [QFUNC034] INVALID_LOGICAL_OPERATOR | Invalid logical operator '??' in function 'f' | Valid operators: &&, ||, and, or | Some(Position { line: 1, column: 5 })
E [QFUNC037] INVALID_UNARY_OPERATOR | Invalid unary operator '??' in function 'f' | Valid operators: !, not, -, +, ~? | Some(Position { line: 1, column: 5 })
W [QFUNC_WARN005] Variable 'a' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'b' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'c' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'd' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'e' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_namespaces|halt
success=false
E [QFUNC044] NAMESPACE_NOT_FOUND | Namespace 'nope' not found. Did you import it in @IMPORTS? | Add to @IMPORTS: nope from "path/to/file.mdix" | Some(Position { line: 0, column: 0 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_namespaces|continue
success=false
E [QFUNC044] NAMESPACE_NOT_FOUND | Namespace 'nope' not found. Did you import it in @IMPORTS? | Add to @IMPORTS: nope from "path/to/file.mdix" | Some(Position { line: 0, column: 0 })
E [QFUNC045] IMPORTED_FUNCTION_NOT_FOUND | Function 'missing' not found in namespace 'lib' |  | Some(Position { line: 0, column: 0 })
E [QFUNC045] IMPORTED_FUNCTION_NOT_FOUND | Function 'missing' not found in namespace 'lib' | Available functions: greet | Some(Position { line: 0, column: 0 })
E [QFUNC046] PARAMETER_COUNT_MISMATCH | Function 'lib.greet' expects 1 parameter(s) but got 2 |  | Some(Position { line: 0, column: 0 })
E [QFUNC046] PARAMETER_COUNT_MISMATCH | Function 'lib.greet' expects 1 parameter(s) but got 2 | Expected: ~greet<Some(Int)> => global(n<Some(Int)>) | Some(Position { line: 0, column: 0 })
E [QFUNC054] IMPORTED_ENUM_NOT_FOUND | Namespace 'lib' does not have enum 'Nope' | Check the imported file for available enums. | Some(Position { line: 0, column: 0 })
E [QFUNC055] NAMESPACE_MEMBER_NOT_FOUND | Namespace 'lib' does not have member 'Nope' | Check the imported file for available functions and enums. | Some(Position { line: 0, column: 0 })
E [QFUNC056] ENUM_VALUE_NOT_FOUND | Enum 'lib.Color' does not have value 'NOPE' | Valid values: BLUE, RED | Some(Position { line: 0, column: 0 })
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "Color", "NOPE"], is_call: false } => QualifiedIdentifierResolution { resolved_type: ImportedEnumAccess, context: Some("lib"), parts: ["lib", "Color", "NOPE"], is_call: false, position: Position { line: 7, column: 1 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "Color", "RED"], is_call: false } => QualifiedIdentifierResolution { resolved_type: ImportedEnumAccess, context: Some("lib"), parts: ["lib", "Color", "RED"], is_call: false, position: Position { line: 7, column: 1 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "Color"], is_call: false } => QualifiedIdentifierResolution { resolved_type: NamespaceEnumReference, context: Some("lib"), parts: ["lib", "Color"], is_call: false, position: Position { line: 7, column: 1 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "Nope", "RED"], is_call: false } => QualifiedIdentifierResolution { resolved_type: ImportedEnumAccess, context: Some("lib"), parts: ["lib", "Nope", "RED"], is_call: false, position: Position { line: 7, column: 1 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "Nope"], is_call: false } => QualifiedIdentifierResolution { resolved_type: NamespaceEnumReference, context: Some("lib"), parts: ["lib", "Nope"], is_call: false, position: Position { line: 7, column: 1 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "greet"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ImportedFunctionCall, context: Some("lib"), parts: ["lib", "greet"], is_call: true, position: Position { line: 7, column: 1 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "missing"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ImportedFunctionCall, context: Some("lib"), parts: ["lib", "missing"], is_call: true, position: Position { line: 7, column: 1 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_namespaces|recover
success=false
E [QFUNC044] NAMESPACE_NOT_FOUND | Namespace 'nope' not found. Did you import it in @IMPORTS? | Add to @IMPORTS: nope from "path/to/file.mdix" | Some(Position { line: 0, column: 0 })
E [QFUNC045] IMPORTED_FUNCTION_NOT_FOUND | Function 'missing' not found in namespace 'lib' |  | Some(Position { line: 0, column: 0 })
E [QFUNC045] IMPORTED_FUNCTION_NOT_FOUND | Function 'missing' not found in namespace 'lib' | Available functions: greet | Some(Position { line: 0, column: 0 })
E [QFUNC046] PARAMETER_COUNT_MISMATCH | Function 'lib.greet' expects 1 parameter(s) but got 2 |  | Some(Position { line: 0, column: 0 })
E [QFUNC046] PARAMETER_COUNT_MISMATCH | Function 'lib.greet' expects 1 parameter(s) but got 2 | Expected: ~greet<Some(Int)> => global(n<Some(Int)>) | Some(Position { line: 0, column: 0 })
E [QFUNC054] IMPORTED_ENUM_NOT_FOUND | Namespace 'lib' does not have enum 'Nope' | Check the imported file for available enums. | Some(Position { line: 0, column: 0 })
E [QFUNC055] NAMESPACE_MEMBER_NOT_FOUND | Namespace 'lib' does not have member 'Nope' | Check the imported file for available functions and enums. | Some(Position { line: 0, column: 0 })
E [QFUNC056] ENUM_VALUE_NOT_FOUND | Enum 'lib.Color' does not have value 'NOPE' | Valid values: BLUE, RED | Some(Position { line: 0, column: 0 })
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "Color", "NOPE"], is_call: false } => QualifiedIdentifierResolution { resolved_type: ImportedEnumAccess, context: Some("lib"), parts: ["lib", "Color", "NOPE"], is_call: false, position: Position { line: 7, column: 1 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "Color", "RED"], is_call: false } => QualifiedIdentifierResolution { resolved_type: ImportedEnumAccess, context: Some("lib"), parts: ["lib", "Color", "RED"], is_call: false, position: Position { line: 7, column: 1 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "Color"], is_call: false } => QualifiedIdentifierResolution { resolved_type: NamespaceEnumReference, context: Some("lib"), parts: ["lib", "Color"], is_call: false, position: Position { line: 7, column: 1 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "Nope", "RED"], is_call: false } => QualifiedIdentifierResolution { resolved_type: ImportedEnumAccess, context: Some("lib"), parts: ["lib", "Nope", "RED"], is_call: false, position: Position { line: 7, column: 1 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "Nope"], is_call: false } => QualifiedIdentifierResolution { resolved_type: NamespaceEnumReference, context: Some("lib"), parts: ["lib", "Nope"], is_call: false, position: Position { line: 7, column: 1 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "greet"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ImportedFunctionCall, context: Some("lib"), parts: ["lib", "greet"], is_call: true, position: Position { line: 7, column: 1 } }
QID QualifiedIdentifierKey { position: Position { line: 7, column: 1 }, parts: ["lib", "missing"], is_call: true } => QualifiedIdentifierResolution { resolved_type: ImportedFunctionCall, context: Some("lib"), parts: ["lib", "missing"], is_call: true, position: Position { line: 7, column: 1 } }
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_static_instance_enum|halt
success=false
E [QFUNC049] STATIC_OBJECT_NOT_FOUND | Static object 'Nope' is not defined | Available static objects: Math, DateTime, Array, Random, Enum, Guid, IpAddress, Dix | Some(Position { line: 0, column: 0 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_static_instance_enum|continue
success=false
E [QFUNC047] INSTANCE_METHOD_NOT_FOUND | Type 'String' has no instance method 'nomethod' | Type 'String' has no such method. | Some(Position { line: 1, column: 6 })
E [QFUNC047] INSTANCE_METHOD_NOT_FOUND | Type 'String' has no instance method 'upper' | Type 'String' has no such method. | Some(Position { line: 1, column: 6 })
E [QFUNC049] STATIC_OBJECT_NOT_FOUND | Static object 'Nope' is not defined | Available static objects: Math, DateTime, Array, Random, Enum, Guid, IpAddress, Dix | Some(Position { line: 0, column: 0 })
E [QFUNC050] STATIC_METHOD_NOT_FOUND | Static object 'Math' has no method 'nomethod' |  | Some(Position { line: 0, column: 0 })
E [QFUNC052] ENUM_NOT_FOUND | Enum 'Missing' not defined in @ENUMS section | Define the enum in @ENUMS or check the spelling. | Some(Position { line: 11, column: 8 })
E [QFUNC053] ENUM_VALUE_NOT_FOUND | Enum 'LogLevel' does not have value 'NOPE' called in 'f' | Valid values: DEBUG, INFO | Some(Position { line: 11, column: 6 })
E [QFUNC055] IMPORTED_ENUM_NOT_FOUND | Enum 'Color' not found in namespace 'nope' |  | Some(Position { line: 11, column: 10 })
E [QFUNC056] ENUM_VALUE_NOT_FOUND | Enum 'lib.Color' does not have value 'NOPE' | Valid values: BLUE, RED | Some(Position { line: 11, column: 9 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_static_instance_enum|recover
success=false
E [QFUNC047] INSTANCE_METHOD_NOT_FOUND | Type 'String' has no instance method 'nomethod' | Type 'String' has no such method. | Some(Position { line: 1, column: 6 })
E [QFUNC047] INSTANCE_METHOD_NOT_FOUND | Type 'String' has no instance method 'upper' | Type 'String' has no such method. | Some(Position { line: 1, column: 6 })
E [QFUNC049] STATIC_OBJECT_NOT_FOUND | Static object 'Nope' is not defined | Available static objects: Math, DateTime, Array, Random, Enum, Guid, IpAddress, Dix | Some(Position { line: 0, column: 0 })
E [QFUNC050] STATIC_METHOD_NOT_FOUND | Static object 'Math' has no method 'nomethod' |  | Some(Position { line: 0, column: 0 })
E [QFUNC052] ENUM_NOT_FOUND | Enum 'Missing' not defined in @ENUMS section | Define the enum in @ENUMS or check the spelling. | Some(Position { line: 11, column: 8 })
E [QFUNC053] ENUM_VALUE_NOT_FOUND | Enum 'LogLevel' does not have value 'NOPE' called in 'f' | Valid values: DEBUG, INFO | Some(Position { line: 11, column: 6 })
E [QFUNC055] IMPORTED_ENUM_NOT_FOUND | Enum 'Color' not found in namespace 'nope' |  | Some(Position { line: 11, column: 10 })
E [QFUNC056] ENUM_VALUE_NOT_FOUND | Enum 'lib.Color' does not have value 'NOPE' | Valid values: BLUE, RED | Some(Position { line: 11, column: 9 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_expression_shapes|halt
success=false
E [QFUNC041] NON_BOOL_TERNARY_CONDITION | Ternary condition must be bool, got Int in 'f' | Use comparison operators to create a boolean condition. | Some(Position { line: 1, column: 5 })
W [QFUNC_WARN003] Ternary branches have incompatible types Int and String in 'f' | Some(Position { line: 1, column: 5 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_expression_shapes|continue
success=false
E [QFUNC041] NON_BOOL_TERNARY_CONDITION | Ternary condition must be bool, got Int in 'f' | Use comparison operators to create a boolean condition. | Some(Position { line: 1, column: 5 })
E [QFUNC042] FUNCTION_NOT_FOUND | Function 'nofn' is not defined in @QUICKFUNCS | Define the function in @QUICKFUNCS or check the spelling. | Some(Position { line: 0, column: 0 })
W [QFUNC_WARN003] Ternary branches have incompatible types Int and String in 'f' | Some(Position { line: 1, column: 5 })
W [QFUNC_WARN005] Variable 'b' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'c' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'd' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'e' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'g' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'h' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'i' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'j' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'k' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'l' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'm' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'n' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=["12:8:Function 'f' calls undefined function 'nofn'"]
@hqf_expression_shapes|recover
success=false
E [QFUNC041] NON_BOOL_TERNARY_CONDITION | Ternary condition must be bool, got Int in 'f' | Use comparison operators to create a boolean condition. | Some(Position { line: 1, column: 5 })
E [QFUNC042] FUNCTION_NOT_FOUND | Function 'nofn' is not defined in @QUICKFUNCS | Define the function in @QUICKFUNCS or check the spelling. | Some(Position { line: 0, column: 0 })
W [QFUNC_WARN003] Ternary branches have incompatible types Int and String in 'f' | Some(Position { line: 1, column: 5 })
W [QFUNC_WARN005] Variable 'b' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'c' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'd' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'e' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'g' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'h' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'i' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'j' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'k' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'l' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'm' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'n' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=["12:8:Function 'f' calls undefined function 'nofn'"]
@hqf_depth|halt
success=false
E [QFUNC074] EXPRESSION_DEPTH_EXCEEDED | Maximum expression depth exceeded in function 'f' | Simplify deeply nested expressions. | Some(Position { line: 15, column: 1 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_depth|continue
success=false
E [QFUNC074] EXPRESSION_DEPTH_EXCEEDED | Maximum expression depth exceeded in function 'f' | Simplify deeply nested expressions. | Some(Position { line: 15, column: 1 })
W [QFUNC_WARN005] Variable 'a' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'b' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_depth|recover
success=false
E [QFUNC074] EXPRESSION_DEPTH_EXCEEDED | Maximum expression depth exceeded in function 'f' | Simplify deeply nested expressions. | Some(Position { line: 15, column: 1 })
W [QFUNC_WARN005] Variable 'a' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'b' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_switch_forms|halt
success=false
E [QFUNC016] NON_BOOLEAN_CONDITION | If statement condition must be boolean, got Int | Use comparison operators (==, !=, >, <, etc.) to create boolean conditions. | Some(Position { line: 1, column: 5 })
E [QFUNC069] VARIABLE_REDECLARATION | Variable 'q' already declared in function 'f' | Each variable must be declared only once. Use assignment to change its value. | Some(Position { line: 3, column: 3 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_switch_forms|continue
success=false
E [QFUNC016] NON_BOOLEAN_CONDITION | If statement condition must be boolean, got Int | Use comparison operators (==, !=, >, <, etc.) to create boolean conditions. | Some(Position { line: 1, column: 5 })
E [QFUNC069] VARIABLE_REDECLARATION | Variable 'q' already declared in function 'f' | Each variable must be declared only once. Use assignment to change its value. | Some(Position { line: 3, column: 3 })
W [QFUNC_WARN005] Variable 'q' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'z' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
@hqf_switch_forms|recover
success=false
E [QFUNC016] NON_BOOLEAN_CONDITION | If statement condition must be boolean, got Int | Use comparison operators (==, !=, >, <, etc.) to create boolean conditions. | Some(Position { line: 1, column: 5 })
E [QFUNC069] VARIABLE_REDECLARATION | Variable 'q' already declared in function 'f' | Each variable must be declared only once. Use assignment to change its value. | Some(Position { line: 3, column: 3 })
W [QFUNC_WARN005] Variable 'q' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
W [QFUNC_WARN005] Variable 'z' declared but never used in function 'f' | Some(Position { line: 2, column: 1 })
functions={"f": "FunctionSignature { name: \"f\", return_type: Some(Int), parameters: [], scopes: [\"global\"], line: 2, column: 1 }"}
logged=[]
"##;

fn norm(s: &str) -> String {
    // The cycle detector walks a hash map, so the same cycle is reported from a different
    // starting function (with a different position and suggestion) on each run. Keep the set.
    if let Some(i) = s.find("Circular function call detected: ") {
        let rest = &s[i + "Circular function call detected: ".len()..];
        let end = rest.find(":Some(").unwrap_or(rest.len());
        let mut names: Vec<&str> = rest[..end].split(" \u{2192} ").collect();
        names.sort();
        names.dedup();
        return format!("Circular function call among [{}]", names.join(", "));
    }
    let mut out = s.to_string();
    for prefix in ["Valid values: ", "Available functions: ", "Available enums: "] {
        let mut from = 0;
        while let Some(i) = out[from..].find(prefix) {
            let start = from + i + prefix.len();
            let rest = &out[start..];
            let end = [rest.find(" | "), rest.find('"'), Some(rest.len())].into_iter().flatten().min().unwrap();
            let mut items: Vec<&str> = rest[..end].split(", ").collect();
            items.sort();
            let sorted = items.join(", ");
            out.replace_range(start..start + end, &sorted);
            from = start + sorted.len();
        }
    }
    out
}

fn all_sources() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = SOURCES.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect();
    v.extend(big_sources());
    v
}

/// The three sources that are too large to write out. The recording was made from the same text.
fn big_sources() -> Vec<(String, String)> {
    vec![
        ("qf_big_array", format!("{}{}{}", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = ["##, (0..10001).map(|i| i.to_string()).collect::<Vec<_>>().join(", "), r##"]
    log: a
    return 1
  }
)
@DATA(
  x = 1
)
"##)),
        ("qf_big_object", format!("{}{}{}", r##"@QUICKFUNCS(
  ~f<int> => global() {
    let a = {"##, (0..1001).map(|i| format!("k{i} = {i}")).collect::<Vec<_>>().join(", "), r##"}
    log: a
    return 1
  }
)
@DATA(
  x = 1
)
"##)),
        ("qf_body_too_large", format!("{}{}{}", r##"@QUICKFUNCS(
  ~f<int> => global() {
"##, "    log: \"x\"\n".repeat(1001), r##"
    return 1
  }
)
@DATA(
  x = 1
)
"##)),
    ]
    .into_iter()
    .map(|(n, t): (&str, String)| (n.to_string(), t))
    .collect()
}

/// The `QID` lines of a result: the qualified identifier resolutions the enhancer reads.
fn resolution_lines(result: &SemanticAnalysisResult) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for (name, section) in &result.section_results {
        if name == "QUICKFUNCS" {
            for (k, v) in &section.qualified_id_resolutions {
                lines.push(format!("QID {:?} => {:?}", k, v));
            }
        }
    }
    lines.sort();
    lines
}

/// The line `RECORDING` stores for the functions registered in the symbol table.
fn function_line(result: &SemanticAnalysisResult) -> Option<String> {
    let table = result.symbol_table.as_ref()?;
    let fns: BTreeMap<_, _> = table.functions.iter().map(|(k, v)| (k.clone(), format!("{:?}", v))).collect();
    Some(format!("functions={:?}", fns))
}

/// Everything `RECORDING` stores for one case.
fn case_lines(result: &SemanticAnalysisResult) -> Vec<String> {
    let mut lines = section_lines(result, "QUICKFUNCS");
    lines.extend(resolution_lines(result));
    lines.extend(function_line(result));
    lines
}

// ==================== the two paths agree ====================

#[test]
fn document_path_and_owned_path_give_identical_output() {
    let mut compared = 0;
    for (name, text) in all_sources() {
        for strategy_name in STRATEGIES {
            let parsed = parse(&text, strategy(strategy_name))
                .unwrap_or_else(|| panic!("{name} [{strategy_name}] should parse"));
            let (with_doc, logged_doc) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
            let (without, logged_owned) = analyze(&parsed.ast, &parsed.settings, None);
            assert_eq!(
                describe(&with_doc, &logged_doc),
                describe(&without, &logged_owned),
                "{name} [{strategy_name}]: the document path and the owned path disagree"
            );
            assert_eq!(resolution_lines(&with_doc), resolution_lines(&without), "{name} [{strategy_name}]: resolutions");
            assert_eq!(function_line(&with_doc), function_line(&without), "{name} [{strategy_name}]: functions");
            compared += 1;
        }
    }
    assert_eq!(compared, all_sources().len() * 3);
}

// ==================== the recorded behaviour ====================

#[test]
fn every_source_has_a_recorded_case() {
    let recorded = recorded_cases();
    assert_eq!(recorded.len(), EXPECTED_CASES);
    for (name, _) in all_sources() {
        assert!(recorded.iter().any(|(n, _, _, _)| *n == name), "{name} has no recorded case");
    }
}

#[test]
fn diagnostics_resolutions_and_functions_match_the_recording() {
    for (name, strategy_name, success, expected) in recorded_cases() {
        let parsed = parse(&source(&name), strategy(&strategy_name)).expect("parses");
        let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
        assert_eq!(case_lines(&result), expected, "{name} [{strategy_name}]");
        assert_eq!(result.is_success, success, "{name} [{strategy_name}]: overall success");
    }
}

/// The @QUICKFUNCS lines of one source under `continue`, read through the document.
fn lines_of(name: &str) -> Vec<String> {
    let parsed = parse(&source(name), ErrorHandlingStrategy::Continue).expect("parses");
    let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
    section_lines(&result, "QUICKFUNCS")
}

fn count(lines: &[String], code: &str) -> usize {
    lines.iter().filter(|l| l.starts_with(&format!("E|{code}|")) || l.starts_with(&format!("W|{code}|"))).count()
}

#[test]
fn statement_nesting_is_refused_beyond_fifty_levels() {
    assert_eq!(count(&lines_of("qf_nesting_ok"), "QFUNC014"), 0);
    assert!(count(&lines_of("qf_nesting_deep"), "QFUNC014") >= 1);
    // The cap stops the recursion before the depth budget, so QFUNC073 never appears.
    assert_eq!(count(&lines_of("qf_nesting_125"), "QFUNC073"), 0);
}

#[test]
fn a_method_chain_is_limited_to_ten_calls() {
    assert_eq!(count(&lines_of("qf_method_chain_ok"), "QFUNC066"), 0);
    assert!(count(&lines_of("qf_method_chain_30"), "QFUNC066") >= 1);
}

/// What reached the error manager for one source under `continue`, normalized.
fn logged_of(name: &str) -> Vec<String> {
    let parsed = parse(&source(name), ErrorHandlingStrategy::Continue).expect("parses");
    let (_, logged) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
    logged
}

#[test]
fn cycles_are_reported_to_the_error_manager_as_a_set_of_functions() {
    // The cycle detector reports through the error manager, not through the section's
    // diagnostics, and starts each cycle from a different function on each run.
    let logged = logged_of("qf_cycles");
    assert!(logged.iter().any(|l| l == "Circular function call among [a, b]"), "{logged:?}");
    assert!(logged.iter().any(|l| l == "Circular function call among [c]"), "{logged:?}");
    assert!(!logged_of("qf_no_cycle").iter().any(|l| l.contains("Circular")));
    // A call to a function that does not exist is a diagnostic of the section.
    assert_eq!(count(&lines_of("qf_cycles"), "QFUNC042"), 1);
}

#[test]
fn huge_arrays_bodies_and_objects_are_refused() {
    assert_eq!(count(&lines_of("qf_big_array"), "QFUNC057"), 1);
    assert_eq!(count(&lines_of("qf_big_object"), "QFUNC058"), 1);
    assert_eq!(count(&lines_of("qf_body_too_large"), "QFUNC012"), 1);
    assert_eq!(count(&lines_of("qf_many_params"), "QFUNC004"), 1);
}

// ==================== sections built by hand ====================

fn qp(l: usize, c: usize) -> Position { Position::new(l, c) }
fn eint(n: i32) -> Expression { Expression::Value { value: Value::Integer { value: n, position: qp(1, 5) }, position: qp(1, 5) } }
fn estr(t: &str) -> Expression { Expression::Value { value: Value::String { value: t.to_string(), position: qp(1, 6) }, position: qp(1, 6) } }
fn eid(n: &str) -> Expression { Expression::Identifier { name: n.to_string(), position: qp(1, 7) } }
fn bx(e: Expression) -> Box<Expression> { Box::new(e) }
fn sret(e: Expression) -> QuickFuncStatement { QuickFuncStatement::Return { value: e, position: qp(2, 3) } }
fn sdecl(name: &str, e: Expression) -> QuickFuncStatement {
    QuickFuncStatement::VariableDeclaration { declaration_type: DeclarationType::Let, is_mutable: true, variable_name: name.to_string(), data_type: None, value: e, position: qp(3, 3) }
}
fn slog(e: Expression) -> QuickFuncStatement { QuickFuncStatement::Log { value: e, position: qp(4, 3) } }
fn sexpr(e: Expression) -> QuickFuncStatement { QuickFuncStatement::ExpressionStatement { expression: e, position: qp(4, 4) } }
fn func(name: &str, ret: Option<DataType>, scopes: Option<Vec<&str>>, params: Vec<QuickFuncParam>, body: Vec<QuickFuncStatement>) -> QuickFunction {
    QuickFunction::new(name.to_string(), ret, scopes.map(|v| v.into_iter().map(String::from).collect()), params, body, qp(2, 1))
}
fn good(name: &str, body: Vec<QuickFuncStatement>) -> QuickFunction {
    func(name, Some(DataType::Int), Some(vec!["global"]), vec![], body)
}
fn param(name: &str, t: Option<DataType>) -> QuickFuncParam { QuickFuncParam::new(name.to_string(), t, None, qp(2, 9)) }
fn argop(l: Expression, op: &str, r: Expression) -> Expression { Expression::ArithmeticOp { left: bx(l), operator: op.to_string(), right: bx(r), position: qp(5, 1) } }
fn bitop(l: Expression, op: &str, r: Expression) -> Expression { Expression::BitwiseOp { left: bx(l), operator: op.to_string(), right: bx(r), position: qp(5, 2) } }
fn cmpop(l: Expression, op: &str, r: Expression) -> Expression { Expression::ComparisonOp { left: bx(l), operator: op.to_string(), right: bx(r), position: qp(5, 3) } }
fn logop(l: Expression, op: &str, r: Expression) -> Expression { Expression::LogicalOp { left: bx(l), operator: op.to_string(), right: bx(r), position: qp(5, 4) } }
fn unop(op: &str, e: Expression) -> Expression { Expression::UnaryOp { operator: op.to_string(), operand: bx(e), position: qp(5, 5) } }
fn args(v: Vec<Expression>) -> Vec<Expression> { v }
fn deep_paren(n: usize) -> Expression {
    let mut e = eint(1);
    for _ in 0..n { e = Expression::Parenthesized { expression: bx(e), position: qp(15, 1) }; }
    e
}
fn obj_value() -> Value { Value::Object { properties: vec![ObjectProperty { key: "k".into(), value: Value::Integer { value: 1, position: qp(6, 1) }, position: qp(6, 2) }], position: qp(6, 0) } }

fn hand_qf_table() -> SymbolTable {
    let mut table = SymbolTable::new();
    let mut e = std::collections::HashMap::new();
    e.insert("DEBUG".to_string(), 0);
    e.insert("INFO".to_string(), 1);
    table.add_enum("LogLevel".to_string(), e);
    let greet_ast = good("greet", vec![sret(eint(1))]);
    let mut fns = std::collections::HashMap::new();
    fns.insert("greet".to_string(), QuickFunctionInfo {
        signature: FunctionSignature {
            name: "greet".to_string(), return_type: Some(DataType::Int),
            parameters: vec![ParameterInfo { name: "n".to_string(), param_type: Some(DataType::Int), has_default_value: false, default_value: None }],
            scopes: vec!["global".to_string()], line: 1, column: 1,
        },
        ast: greet_ast,
    });
    let mut enums = std::collections::HashMap::new();
    let mut colors = std::collections::HashMap::new();
    colors.insert("RED".to_string(), 0);
    colors.insert("BLUE".to_string(), 1);
    enums.insert("Color".to_string(), colors);
    table.register_namespace("lib".to_string(), "lib.mdix".to_string(), fns, enums, std::collections::HashMap::new());
    table
}

fn qf_hand_cases() -> Vec<(&'static str, Vec<QuickFunction>)> {
    let qid = |parts: &[&str], arguments: Option<Vec<Expression>>| Expression::QualifiedIdentifier { parts: parts.iter().map(|x| x.to_string()).collect(), arguments, position: qp(7, 1) };
    vec![
        ("hqf_names_and_types", vec![
            func("1bad", Some(DataType::Int), Some(vec!["global"]), vec![], vec![sret(eint(1))]),
            func("if", Some(DataType::Int), Some(vec!["global"]), vec![], vec![sret(eint(1))]),
            func("rt", Some(DataType::Function), Some(vec!["global"]), vec![], vec![sret(eint(1))]),
            func("noscope", Some(DataType::Int), None, vec![], vec![sret(eint(1))]),
            func("badscope", Some(DataType::Int), Some(vec!["bad scope!"]), vec![], vec![sret(eint(1))]),
            func("params", Some(DataType::Int), Some(vec!["global"]), vec![param("1x", Some(DataType::Int)), param("if", Some(DataType::Int)), param("int", Some(DataType::Int)), param("pt", Some(DataType::Function))], vec![sret(eint(1))]),
        ]),
        ("hqf_variable_names", vec![good("f", vec![
            sdecl("1bad", eint(1)), sdecl("int", eint(2)), sdecl("if", eint(3)), sdecl("ok", eint(4)),
            QuickFuncStatement::Assignment { variable: "1x".into(), value: eint(1), position: qp(8, 1) },
            QuickFuncStatement::ArithmeticAssignment { variable: "1y".into(), operator: "+=".into(), value: eint(1), position: qp(8, 2) },
            QuickFuncStatement::ArithmeticAssignment { variable: "ok".into(), operator: "??=".into(), value: eint(1), position: qp(8, 3) },
            sret(eint(1)),
        ])]),
        ("hqf_object_creation", vec![good("f", vec![
            QuickFuncStatement::ObjectCreation { variable: "o".into(), object: obj_value(), position: qp(9, 1) },
            QuickFuncStatement::ObjectCreation { variable: "1bad".into(), object: obj_value(), position: qp(9, 2) },
            QuickFuncStatement::VariableDeclaration { declaration_type: DeclarationType::Const, is_mutable: false, variable_name: "c".into(), data_type: None, value: eint(1), position: qp(9, 3) },
            QuickFuncStatement::ObjectCreation { variable: "c".into(), object: obj_value(), position: qp(9, 4) },
            QuickFuncStatement::ObjectCreation { variable: "o".into(), object: Value::Integer { value: 1, position: qp(9, 5) }, position: qp(9, 6) },
            sret(eint(1)),
        ])]),
        ("hqf_operators", vec![good("f", vec![
            sdecl("a", argop(eint(1), "??", eint(2))),
            sdecl("b", bitop(eint(1), "??", eint(2))),
            sdecl("c", cmpop(eint(1), "??", eint(2))),
            sdecl("d", logop(eint(1), "??", eint(2))),
            sdecl("e", unop("??", eint(1))),
            sret(eint(1)),
        ])]),
        ("hqf_namespaces", vec![good("f", vec![
            sexpr(Expression::ImportedFunctionCall { namespace_name: "nope".into(), function_name: "greet".into(), arguments: args(vec![eint(1)]), position: qp(10, 1) }),
            sexpr(Expression::ImportedFunctionCall { namespace_name: "lib".into(), function_name: "missing".into(), arguments: args(vec![]), position: qp(10, 2) }),
            sexpr(Expression::ImportedFunctionCall { namespace_name: "lib".into(), function_name: "greet".into(), arguments: args(vec![eint(1), eint(2)]), position: qp(10, 3) }),
            sexpr(Expression::ImportedFunctionCall { namespace_name: "lib".into(), function_name: "greet".into(), arguments: args(vec![eint(1)]), position: qp(10, 4) }),
            sexpr(qid(&["lib", "missing"], Some(vec![]))),
            sexpr(qid(&["lib", "greet"], Some(vec![eint(1), eint(2)]))),
            sexpr(qid(&["lib", "greet"], Some(vec![eint(1)]))),
            sexpr(qid(&["lib", "Color", "NOPE"], None)),
            sexpr(qid(&["lib", "Nope", "RED"], None)),
            sexpr(qid(&["lib", "Color", "RED"], None)),
            sexpr(qid(&["lib", "Nope"], None)),
            sexpr(qid(&["lib", "Color"], None)),
            sret(eint(1)),
        ])]),
        ("hqf_static_instance_enum", vec![good("f", vec![
            sexpr(Expression::StaticMethodCall { object_name: "Nope".into(), method_name: "f".into(), arguments: args(vec![]), position: qp(11, 1) }),
            sexpr(Expression::StaticMethodCall { object_name: "Math".into(), method_name: "nomethod".into(), arguments: args(vec![eint(1)]), position: qp(11, 2) }),
            sexpr(Expression::StaticMethodCall { object_name: "Math".into(), method_name: "sqrt".into(), arguments: args(vec![eint(4)]), position: qp(11, 3) }),
            sexpr(Expression::InstanceMethodCall { instance: bx(estr("abc")), method_name: "nomethod".into(), arguments: args(vec![]), position: qp(11, 4) }),
            sexpr(Expression::InstanceMethodCall { instance: bx(estr("abc")), method_name: "upper".into(), arguments: args(vec![]), position: qp(11, 5) }),
            sexpr(Expression::EnumAccess { namespace_name: None, enum_name: "LogLevel".into(), value: "NOPE".into(), position: qp(11, 6) }),
            sexpr(Expression::EnumAccess { namespace_name: None, enum_name: "LogLevel".into(), value: "INFO".into(), position: qp(11, 7) }),
            sexpr(Expression::EnumAccess { namespace_name: None, enum_name: "Missing".into(), value: "A".into(), position: qp(11, 8) }),
            sexpr(Expression::EnumAccess { namespace_name: Some("lib".into()), enum_name: "Color".into(), value: "NOPE".into(), position: qp(11, 9) }),
            sexpr(Expression::EnumAccess { namespace_name: Some("nope".into()), enum_name: "Color".into(), value: "RED".into(), position: qp(11, 10) }),
            sret(eint(1)),
        ])]),
        ("hqf_expression_shapes", vec![good("f", vec![
            sdecl("a", Expression::ConfigAccess { key: "k".into(), position: qp(12, 1) }),
            sdecl("b", Expression::ObjectAccess { path: vec!["x".into(), "y".into()], position: qp(12, 2) }),
            sdecl("c", Expression::PropertyAccess { object: bx(eid("a")), property: "p".into(), position: qp(12, 3) }),
            sdecl("d", Expression::IndexAccess { object: bx(eid("a")), index: bx(eint(0)), position: qp(12, 4) }),
            sdecl("e", Expression::TypeCast { expression: bx(eint(1)), target_type: DataType::Float, position: qp(12, 5) }),
            sdecl("g", Expression::Conditional { condition: bx(eint(1)), true_value: bx(eint(2)), false_value: bx(estr("s")), position: qp(12, 6) }),
            sdecl("h", Expression::FunctionCall { name: "greet".into(), arguments: args(vec![eint(1)]), position: qp(12, 7) }),
            sdecl("i", Expression::QuickFuncCall { name: "nofn".into(), arguments: args(vec![]), position: qp(12, 8) }),
            sdecl("j", Expression::DixFunctionCall { function_name: "dix".into(), arguments: args(vec![]), position: qp(12, 9) }),
            sdecl("k", Expression::StaticFunction { class_name: "Math".into(), method: "abs".into(), arguments: args(vec![eint(-1)]), position: qp(12, 10) }),
            sdecl("l", Expression::BuiltinFunction { target: bx(estr("s")), method: "len".into(), arguments: None, position: qp(12, 11) }),
            sdecl("m", Expression::BuiltinFunction { target: bx(estr("s")), method: "upper".into(), arguments: Some(vec![]), position: qp(12, 12) }),
            sdecl("n", Expression::Parenthesized { expression: bx(eint(1)), position: qp(12, 13) }),
            sret(eint(1)),
        ])]),
        ("hqf_depth", vec![good("f", vec![
            sdecl("a", deep_paren(130)),
            sdecl("b", deep_paren(100)),
            sret(eint(1)),
        ])]),
        ("hqf_switch_forms", vec![good("f", vec![
            QuickFuncStatement::Switch { expression: eint(1), cases: vec![
                SwitchCase::new(Value::Integer { value: 1, position: qp(13, 1) }, vec![sret(eint(1))], qp(13, 2)),
                SwitchCase::new(Value::String { value: "s".into(), position: qp(13, 3) }, vec![sdecl("z", eint(1))], qp(13, 4)),
            ], default_case: Some(SwitchCase::new(Value::Null { position: qp(13, 5) }, vec![sret(eint(2))], qp(13, 6))), position: qp(13, 0) },
            QuickFuncStatement::If { condition: eint(1), then_branch: vec![sdecl("q", eint(1))], else_branch: Some(vec![sdecl("q", eint(2))]), position: qp(14, 1) },
            sret(eint(1)),
        ])]),
    ]
}

fn hand_table() -> SymbolTable {
    hand_qf_table()
}

/// Analyzes a hand-built section and prints it the way `HAND_RECORDING` stores it.
fn hand_text(name: &str, funcs: &[QuickFunction], strategy_name: &str, through_arena: bool) -> String {
    let em = ErrorManager::new_isolated_silent();
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = strategy(strategy_name);
    em.update_settings(settings.clone());
    let section = QuickFuncsSection::new(funcs.to_vec(), qp(1, 1));
    let mut table = hand_table();
    let mut analyzer = QuickFuncsSectionAnalyzer::new_with_error_manager(&settings, em.clone());
    let r = if through_arena {
        let mut arena = Arena::new();
        let doc = DocQuickFuncs::from_owned(&mut arena, &section);
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
        .map(|e| norm(&format!("[{}] {} | {} | {} | {:?}", e.error_id, e.error_type, e.message, e.suggestion, e.position)))
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
    let mut q: Vec<String> = r.qualified_id_resolutions.iter().map(|(k, v)| format!("{:?} => {:?}", k, v)).collect();
    q.sort();
    for x in &q {
        out.push_str(&format!("QID {x}\n"));
    }
    let fns: BTreeMap<_, _> = table.functions.iter().map(|(k, v)| (k.clone(), format!("{:?}", v))).collect();
    out.push_str(&format!("functions={:?}\n", fns));
    let mut logged: Vec<String> = em.get_semantic_errors().iter().map(|x| norm(&format!("{}:{}:{}", x.line, x.column, x.message))).collect();
    logged.sort();
    out.push_str(&format!("logged={:?}", logged));
    out
}

/// The recorded blocks of `HAND_RECORDING`, keyed by `name|strategy`.
fn hand_blocks() -> Vec<(String, String)> {
    let mut blocks: Vec<(String, String)> = Vec::new();
    for line in HAND_RECORDING.lines() {
        if let Some(header) = line.strip_prefix('@') {
            blocks.push((header.to_string(), line.to_string()));
        } else if let Some(last) = blocks.last_mut() {
            last.1.push('\n');
            last.1.push_str(line);
        }
    }
    blocks.into_iter().map(|(k, v)| (k, v.trim_end_matches('\n').to_string())).collect()
}

#[test]
fn hand_built_sections_match_the_recording_through_both_entry_points() {
    let blocks = hand_blocks();
    assert_eq!(blocks.len(), EXPECTED_HAND_BLOCKS);
    let cases = qf_hand_cases();
    for (key, expected) in &blocks {
        let (name, strategy_name) = key.split_once('|').expect("name|strategy");
        let funcs = &cases.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("unknown case {name}")).1;
        for through_arena in [false, true] {
            let got = hand_text(name, funcs, strategy_name, through_arena);
            assert_eq!(&got, expected, "{name} [{strategy_name}] (through the arena: {through_arena})");
        }
    }
    assert_eq!(cases.len() * 3, EXPECTED_HAND_BLOCKS, "every hand-built case is recorded under three strategies");
}

#[test]
fn the_checks_the_parser_prevents_reach_their_codes() {
    // A reading of the recording that fails if a hand-built case stops reaching its branch.
    let text = |name: &str| {
        let cases = qf_hand_cases();
        let funcs = &cases.iter().find(|(n, _)| *n == name).expect("case").1;
        hand_text(name, funcs, "continue", true)
    };
    for (case, codes) in [
        ("hqf_names_and_types", &["QFUNC002", "QFUNC002B", "QFUNC006", "QFUNC006B", "QFUNC006C", "QFUNC010", "QFUNC015", "QFUNC_WARN006"][..]),
        ("hqf_variable_names", &["QFUNC017", "QFUNC020", "QFUNC022", "QFUNC067", "QFUNC067B", "QFUNC068"][..]),
        ("hqf_object_creation", &["QFUNC023", "QFUNC024"][..]),
        ("hqf_operators", &["QFUNC025", "QFUNC029", "QFUNC032", "QFUNC034", "QFUNC037"][..]),
        ("hqf_namespaces", &["QFUNC044", "QFUNC045", "QFUNC046", "QFUNC054", "QFUNC055", "QFUNC056"][..]),
        ("hqf_static_instance_enum", &["QFUNC047", "QFUNC049", "QFUNC050", "QFUNC052", "QFUNC053"][..]),
        ("hqf_expression_shapes", &["QFUNC041", "QFUNC042"][..]),
        ("hqf_switch_forms", &["QFUNC016", "QFUNC069"][..]),
        ("hqf_depth", &["QFUNC074"][..]),
    ] {
        let t = text(case);
        for code in codes {
            assert!(t.contains(&format!("[{code}]")), "{case} no longer reaches {code}");
        }
    }
}

// ==================== which tree is read ====================

#[test]
fn the_analyzer_reads_the_document_when_one_is_attached() {
    // The document holds two functions with the same name. The owned tree handed to the analyzer
    // has no @QUICKFUNCS section at all, so the error can only come from the document.
    let parsed = parse(&source("qf_dup_names"), ErrorHandlingStrategy::Continue).expect("parses");
    let empty = DixScript::new();

    let (with_doc, _) = analyze(&empty, &parsed.settings, Some(&parsed.document));
    assert_eq!(count(&section_lines(&with_doc, "QUICKFUNCS"), "QFUNC001"), 1);

    let (without, _) = analyze(&empty, &parsed.settings, None);
    assert!(!without.section_results.contains_key("QUICKFUNCS"));
}

#[test]
fn functions_are_registered_from_the_section_that_was_read() {
    // A function name defined twice is reported and not registered; a valid one is registered.
    let valid = parse(&source("qf_valid_basic"), ErrorHandlingStrategy::Continue).expect("parses");
    let empty = DixScript::new();
    let (with_doc, _) = analyze(&empty, &valid.settings, Some(&valid.document));
    let table = with_doc.symbol_table.as_ref().expect("a symbol table");
    assert!(table.functions.contains_key("f") && table.functions.contains_key("g"));
    let (without, _) = analyze(&empty, &valid.settings, None);
    assert!(!without.symbol_table.as_ref().is_some_and(|t| t.functions.contains_key("f")));

    let duplicated = parse(&source("qf_dup_names"), ErrorHandlingStrategy::Continue).expect("parses");
    let (result, _) = analyze(&duplicated.ast, &duplicated.settings, Some(&duplicated.document));
    assert!(!result.symbol_table.as_ref().is_some_and(|t| t.functions.contains_key("f")));
}

#[test]
fn the_analyzer_reads_the_owned_tree_when_no_document_is_attached() {
    // The mirror image: the document has no section and the owned tree has one.
    let parsed = parse(&source("qf_dup_names"), ErrorHandlingStrategy::Continue).expect("parses");
    let empty_document = Document::from_owned(&DixScript::new());

    let (without, _) = analyze(&parsed.ast, &parsed.settings, None);
    assert_eq!(count(&section_lines(&without, "QUICKFUNCS"), "QFUNC001"), 1);

    let (with_empty, _) = analyze(&parsed.ast, &parsed.settings, Some(&empty_document));
    assert!(!with_empty.section_results.contains_key("QUICKFUNCS"));
}

#[test]
fn resolutions_come_from_the_section_that_was_read() {
    // The enhancer reads these. A document with qualified identifiers and a tree without any
    // must give resolutions only when the document is attached.
    let parsed = parse(&source("qf_qualified_forms"), ErrorHandlingStrategy::Continue).expect("parses");
    let empty = DixScript::new();
    let (with_doc, _) = analyze(&empty, &parsed.settings, Some(&parsed.document));
    assert!(!resolution_lines(&with_doc).is_empty());
    let (without, _) = analyze(&empty, &parsed.settings, None);
    assert!(resolution_lines(&without).is_empty());
}

// ==================== boundaries, constructs and phases the recording does not reach ====================
//
// The recording pins what the analyzer reported for the sources it holds. Deliberate bugs put
// into the analyzer, the call collector, the cycle detector and the orchestrator showed what it
// does not pin: the exact edge of each limit, the call graph and the unused variable check in
// each kind of construct, the error manager's records for undefined callees, the owned entry
// points of the call graph tools, and two branches of the orchestrator. The tests below pin them.
//
// Three groups of changes cannot be seen from outside and are left alone on purpose:
// the `.min(500)` cap of the depth budget (a body is limited to 1000 statements, so the budget
// never gets above 200), the statement nesting check against the depth budget (the nesting cap
// of 50 always stops first, which is also why `QFUNC073` is unreachable), and the `is_success`
// flag set after the cycle check (it is overwritten by the error count one line later).

use dixscript::Compiler::Core::Functions::{CallGraph, CycleDetectionValidator, FunctionCallCollector};

/// A `@QUICKFUNCS` section followed by a one line `@DATA` section.
fn quick(body: &str) -> String {
    format!("@QUICKFUNCS(\n{body}\n)\n@DATA(\n  x = 1\n)\n")
}

/// One `~name<int> => global(params)` function: `inner`, then `return 1`.
fn quick_fn(name: &str, params: &str, inner: &str) -> String {
    format!("  ~{name}<int> => global({params}) {{\n{inner}\n    return 1\n  }}")
}

/// A function the others can call.
const CALLEE: &str = "  ~g<int> => global(n<int>) {\n    return n\n  }\n";

/// The `@QUICKFUNCS` lines and the error manager's records (`line:column:message`, sorted) of one
/// source under `continue`, read through the document.
fn run_continue(source: &str) -> (Vec<String>, Vec<String>) {
    let parsed = parse(source, ErrorHandlingStrategy::Continue).expect("parses");
    let em = ErrorManager::new_isolated_silent();
    let analyzer = GeneralSemanticAnalyzer::new_with_error_manager(&parsed.ast, &parsed.settings, em.clone());
    let result = analyzer.with_document(&parsed.document).analyze();
    let mut records: Vec<String> =
        em.get_semantic_errors().iter().map(|e| format!("{}:{}:{}", e.line, e.column, e.message)).collect();
    records.sort();
    (section_lines(&result, "QUICKFUNCS"), records)
}

/// Error codes and warning messages of a hand-built section, through the owned entry point.
fn hand_run(funcs: &[QuickFunction]) -> (Vec<String>, Vec<String>) {
    let em = ErrorManager::new_isolated_silent();
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Continue;
    em.update_settings(settings.clone());
    let section = QuickFuncsSection::new(funcs.to_vec(), qp(1, 1));
    let mut table = hand_table();
    let mut analyzer = QuickFuncsSectionAnalyzer::new_with_error_manager(&settings, em);
    let r = analyzer.analyze(&section, &mut table);
    (r.errors.iter().map(|e| e.error_id.clone()).collect(), r.warnings.iter().map(|w| w.message.clone()).collect())
}

fn hand_has(funcs: &[QuickFunction], code: &str) -> bool {
    hand_run(funcs).0.iter().any(|c| c == code)
}

// ---- limits ----

#[test]
fn each_size_limit_accepts_its_maximum_and_refuses_one_more() {
    let params = |n: usize| (0..n).map(|i| format!("p{i}<int>")).collect::<Vec<_>>().join(", ");
    let body = |logs: usize| format!("  ~f<int> => global() {{\n{}    return 1\n  }}", "    log: \"x\"\n".repeat(logs));
    let array = |n: usize| {
        quick(&quick_fn("f", "", &format!("    let a = [{}]\n    log: a", (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(", "))))
    };
    let object = |n: usize| {
        quick(&quick_fn("f", "", &format!("    let a = {{{}}}\n    log: a", (0..n).map(|i| format!("k{i} = {i}")).collect::<Vec<_>>().join(", "))))
    };
    // A method chain of n calls has a depth of n - 2: the first call is a qualified identifier.
    let chain = |n: usize| quick(&quick_fn("f", "", &format!("    let s = \"abc\"\n    let t = s{}\n    log: t", ".upper()".repeat(n))));
    let tuple = |n: usize| {
        quick(&quick_fn("f", "", &format!("    let t = t:({})\n    log: t", (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(", "))))
    };

    // (code, source at the limit, source one past it)
    let cases: Vec<(&str, String, String)> = vec![
        ("QFUNC004", quick(&quick_fn("f", &params(100), "")), quick(&quick_fn("f", &params(101), ""))),
        // The return statement counts as one of the 1000.
        ("QFUNC012", quick(&body(999)), quick(&body(1000))),
        ("QFUNC057", array(10_000), array(10_001)),
        ("QFUNC058", object(1_000), object(1_001)),
        ("QFUNC066", chain(12), chain(13)),
        ("QFUNC059", tuple(6), tuple(7)),
    ];
    for (code, at_limit, past) in cases {
        assert_eq!(count(&run_continue(&at_limit).0, code), 0, "{code} at the limit");
        assert_eq!(count(&run_continue(&past).0, code), 1, "{code} one past the limit");
    }
}

#[test]
fn property_accesses_count_toward_the_method_chain_limit() {
    // The parser never builds a property access in front of a method call, so the section is built
    // by hand: `s.p.p. ... .upper()` with n property accesses and one call.
    let call_on = |props: usize| {
        let mut chain = eid("s");
        for _ in 0..props {
            chain = Expression::PropertyAccess { object: bx(chain), property: "p".to_string(), position: qp(12, 3) };
        }
        vec![good("f", vec![
            sdecl("s", estr("abc")),
            sexpr(Expression::InstanceMethodCall { instance: bx(chain), method_name: "upper".to_string(), arguments: vec![], position: qp(12, 4) }),
            sret(eint(1)),
        ])]
    };
    // The call itself is not counted, only what it is called on.
    assert!(!hand_has(&call_on(10), "QFUNC066"), "ten links are allowed");
    assert!(hand_has(&call_on(11), "QFUNC066"), "eleven links are refused");
}

/// The smallest nesting at which `QFUNC074` appears.
fn first_failing_depth(build: impl Fn(usize) -> Vec<QuickFunction>) -> usize {
    (1..=300).find(|&k| hand_has(&build(k), "QFUNC074")).expect("a nesting below 300 is refused")
}

#[test]
fn the_expression_depth_budget_follows_the_length_of_the_body() {
    // Declarations, returns and assignments use the budget of a 100 statement body: 100 + 100 / 10.
    assert_eq!(first_failing_depth(|k| vec![good("f", vec![sdecl("a", deep_paren(k)), sret(eint(1))])]), 110);
    assert_eq!(first_failing_depth(|k| vec![good("f", vec![sret(deep_paren(k))])]), 110);
    // A log uses the budget of its own body: 100 + length / 10.
    assert_eq!(first_failing_depth(|k| vec![good("f", vec![slog(deep_paren(k)), sret(eint(1))])]), 100);
    let long_body = |k: usize| {
        let mut body: Vec<QuickFuncStatement> = (0..198).map(|_| slog(eint(0))).collect();
        body.push(slog(deep_paren(k)));
        body.push(sret(eint(1)));
        vec![good("f", body)]
    };
    assert_eq!(first_failing_depth(long_body), 120);
}

// ---- the call graph ----

/// Undefined callees in each construct: the source line and the function it calls.
const CALL_SITES: &[(&str, &str)] = &[
    ("    let r = [z1()]", "z1"),
    ("    let r = {k = z2()}", "z2"),
    ("    let r = t:(z3(), 1)", "z3"),
    ("    let r = $\"{z4()}\"", "z4"),
    ("    let r = true ? 1 : z5()", "z5"),
    ("    let r = true ? z5b() : 1", "z5b"),
    ("    let r = z5c() ? 1 : 2", "z5c"),
    ("    let xs = [1]\n    let r = xs[z6()]", "z6"),
    ("    let r = z7().upper()", "z7"),
    ("    let r = Math.abs(z8())", "z8"),
    ("    let r = !z10()", "z10"),
    ("    let r = foo.bar(z11())", "z11"),
    ("    chk: z12() {\n      -> 1 { log: 0 }\n      -> miss { log: 0 }\n    }", "z12"),
    ("    chk: 1 {\n      -> 1 { log: 0 }\n      -> miss { log: z13() }\n    }", "z13"),
    ("    chk: 1 {\n      -> 1 { log: z14() }\n      -> miss { log: 0 }\n    }", "z14"),
    ("    if: z15() == 1 { log: 0 }", "z15"),
    ("    if: true { log: z16() }", "z16"),
    ("    if: true { log: 0 } else { log: z17() }", "z17"),
    ("    let mut s = 0\n    s += z18()", "z18"),
    ("    let f = (n) => z19()", "z19"),
    ("    let r = z20() as float", "z20"),
    ("    let r = 1..z21()", "z21"),
    ("    z22()", "z22"),
    ("    log: z23()", "z23"),
    ("    let r = z24()", "z24"),
    ("    let r = (z25())", "z25"),
    ("    let r = g(z26())", "z26"),
];

#[test]
fn a_call_in_any_construct_is_a_call_in_the_graph() {
    for (snippet, callee) in CALL_SITES {
        let (_, records) = run_continue(&quick(&format!("{CALLEE}{}", quick_fn("a", "", snippet))));
        let expected = format!("Function 'a' calls undefined function '{callee}'");
        let hits: Vec<&String> = records.iter().filter(|r| r.ends_with(&expected)).collect();
        assert_eq!(hits.len(), 1, "{snippet:?}: {records:?}");
        // The call site's position is recorded with it.
        let mut parts = hits[0].splitn(3, ':');
        let line: i32 = parts.next().and_then(|p| p.parse().ok()).expect("a line");
        let column: i32 = parts.next().and_then(|p| p.parse().ok()).expect("a column");
        assert!(line > 0 && column > 0, "{snippet:?}: no position in {}", hits[0]);
    }
}

#[test]
fn a_local_name_called_like_a_function_is_not_a_call_in_the_graph() {
    for (snippet, params, name) in [
        ("    let f = (loc1) => loc1(1)", "", "loc1"),
        ("    let loc2 = (n) => n\n    let r = loc2(1)", "", "loc2"),
        ("    loc3 = (n) => n\n    let r = loc3(1)", "", "loc3"),
        ("    let loc4 = (n) => n\n    let r = [loc4(1)]", "", "loc4"),
        ("    let r = [(loc5) => loc5(1)]", "", "loc5"),
        ("    let r = loc6(1)", "loc6<int>", "loc6"),
    ] {
        let (_, records) = run_continue(&quick(&format!("{CALLEE}{}", quick_fn("a", params, snippet))));
        assert!(!records.iter().any(|r| r.contains(&format!("'{name}'"))), "{snippet:?}: {records:?}");
    }
}

#[test]
fn calls_to_defined_functions_are_not_reported_as_undefined() {
    // `g` is the first function of the section and is called from the second.
    let own = quick(&format!("{CALLEE}{}", quick_fn("a", "", "    let r = g(1)\n    let s = [g(2)]\n    log: r\n    log: s")));
    for text in [own, source("qf_valid_basic"), source("qf_no_cycle"), source("qf_defaults_used")] {
        let (_, records) = run_continue(&text);
        assert!(!records.iter().any(|r| r.contains("undefined function")), "{records:?}");
    }
}

#[test]
fn a_reported_cycle_carries_the_position_of_a_call_in_it() {
    let (_, records) = run_continue(&source("qf_cycles"));
    let cycles: Vec<&String> = records.iter().filter(|r| r.contains("Circular function call detected")).collect();
    assert_eq!(cycles.len(), 2, "one cycle between a and b, one function calling itself: {records:?}");
    for record in cycles {
        let line: i32 = record.split(':').next().and_then(|p| p.parse().ok()).expect("a line");
        assert!(line > 0, "no position in {record}");
    }
}

#[test]
fn the_owned_entry_points_of_the_call_graph_tools_agree_with_the_arena_ones() {
    let em = ErrorManager::new_isolated_silent();
    let settings = OperationalSettings::default();
    let validator = CycleDetectionValidator::new_with_error_manager(&settings, em);

    let cyclic = parse(&quick("  ~a<int> => global() {\n    return b()\n  }\n  ~b<int> => global() {\n    return a()\n  }"), ErrorHandlingStrategy::Continue).expect("parses");
    let owned = cyclic.ast.quick_functions.as_ref().expect("section");
    let arena_section = cyclic.document.quick_functions.as_ref().expect("section");
    assert!(!validator.validate_function_calls(owned));
    assert!(!validator.validate_function_calls_doc(arena_section, &cyclic.document.arena));
    assert_eq!(validator.get_execution_order(owned), None);
    assert_eq!(validator.get_execution_order_doc(arena_section, &cyclic.document.arena), None);

    let chain = parse(&quick("  ~a<int> => global() {\n    return b()\n  }\n  ~b<int> => global() {\n    return c()\n  }\n  ~c<int> => global() {\n    return 1\n  }"), ErrorHandlingStrategy::Continue).expect("parses");
    let owned = chain.ast.quick_functions.as_ref().expect("section");
    let arena_section = chain.document.quick_functions.as_ref().expect("section");
    assert!(validator.validate_function_calls(owned));
    assert!(validator.validate_function_calls_doc(arena_section, &chain.document.arena));
    let order = validator.get_execution_order(owned).expect("an order");
    assert_eq!(order.len(), 3);
    assert_eq!(Some(order.clone()), validator.get_execution_order_doc(arena_section, &chain.document.arena));
    // Callers come before the functions they call.
    assert_eq!(order, vec!["a", "b", "c"]);
}

#[test]
fn the_call_collector_reads_an_owned_function_like_an_arena_one() {
    let chain = parse(&quick("  ~a<int> => global() {\n    return b()\n  }\n  ~b<int> => global() {\n    return c()\n  }\n  ~c<int> => global() {\n    return 1\n  }"), ErrorHandlingStrategy::Continue).expect("parses");
    let owned = chain.ast.quick_functions.as_ref().expect("section");
    let mut graph = CallGraph::new();
    {
        let mut collector = FunctionCallCollector::new(&mut graph);
        for function in &owned.functions {
            collector.analyze_function(function);
        }
    }
    assert_eq!(graph.get_callees("a"), vec!["b"]);
    assert_eq!(graph.get_callees("b"), vec!["c"]);
    assert!(graph.get_callees("c").is_empty());
}

fn evalue(value: Value) -> Expression {
    Expression::Value { value, position: qp(9, 2) }
}

fn call_value(name: &str) -> Value {
    Value::QuickFuncCall { function_name: name.to_string(), arguments: vec![], position: qp(9, 1) }
}

fn ecall(name: &str) -> Expression {
    Expression::QuickFuncCall { name: name.to_string(), arguments: vec![], position: qp(9, 3) }
}

#[test]
fn calls_the_parser_cannot_write_reach_the_call_graph() {
    // Hand-built statements: an object creation (its value holds a call, its variable is a local
    // name), a call written as a value (defined, then local), and calls inside a cast, a static
    // method call, a built-in function target and the end of a range.
    let funcs = vec![good("a", vec![
        QuickFuncStatement::ObjectCreation {
            variable: "loc7".to_string(),
            object: Value::Object {
                properties: vec![ObjectProperty { key: "k".to_string(), value: call_value("z27"), position: qp(9, 4) }],
                position: qp(9, 0),
            },
            position: qp(9, 5),
        },
        sexpr(ecall("loc7")),
        sdecl("loc8", estr("x")),
        sexpr(evalue(call_value("loc8"))),
        sexpr(evalue(call_value("z28"))),
        sdecl("r", Expression::TypeCast { expression: bx(ecall("z29")), target_type: DataType::Float, position: qp(9, 6) }),
        sdecl("m", Expression::StaticMethodCall { object_name: "Math".to_string(), method_name: "abs".to_string(), arguments: vec![ecall("z30")], position: qp(9, 7) }),
        sdecl("b", Expression::BuiltinFunction { target: bx(ecall("z31")), method: "len".to_string(), arguments: None, position: qp(9, 8) }),
        sexpr(evalue(Value::Range { start: Box::new(Value::Integer { value: 1, position: qp(9, 9) }), end: Box::new(call_value("z32")), position: qp(9, 10) })),
        sret(eint(1)),
    ])];
    let settings = OperationalSettings::default();
    let section = QuickFuncsSection::new(funcs, qp(1, 1));
    let mut arena = Arena::new();
    let doc = DocQuickFuncs::from_owned(&mut arena, &section);
    for through_arena in [false, true] {
        let em = ErrorManager::new_isolated_silent();
        let validator = CycleDetectionValidator::new_with_error_manager(&settings, em.clone());
        let ok = if through_arena { validator.validate_function_calls_doc(&doc, &arena) } else { validator.validate_function_calls(&section) };
        assert!(!ok);
        let mut undefined: Vec<String> = em
            .get_semantic_errors()
            .iter()
            .filter_map(|e| e.message.strip_prefix("Function 'a' calls undefined function '").map(|m| m.trim_end_matches('\'').to_string()))
            .collect();
        undefined.sort();
        assert_eq!(undefined, vec!["z27", "z28", "z29", "z30", "z31", "z32"], "through the arena: {through_arena}");
    }
}

// ---- the unused variable check ----

/// Source lines that use the variable `v` and nothing else uses it.
const USES_OF_V: &[&str] = &[
    "    let mut s = 0\n    s += v",
    "    let mut s = 0\n    s = v",
    "    let a = [v]",
    "    let o = {k = v}",
    "    let a = g(v)",
    "    g(v)",
    "    chk: 1 {\n      -> 1 { log: v }\n      -> miss { log: 0 }\n    }",
    "    chk: 1 {\n      -> 1 { log: 0 }\n      -> miss { log: v }\n    }",
    "    chk: v {\n      -> 1 { log: 0 }\n      -> miss { log: 0 }\n    }",
    "    let a = true ? v : 0",
    "    let a = true ? 0 : v",
    "    let a = (v == 1) ? 2 : 3",
    "    let a = t:(v, 1)",
    "    if: v == 1 { log: 0 }",
    "    if: true { log: v }",
    "    if: true { log: 0 } else { log: v }",
    "    let xs = [1, 2]\n    let a = xs[v]",
    "    let a = \"abc\".foo(v)",
    "    let a = (v).upper()",
    "    let a = $\"x {v}\"",
    "    let f = (n) => n + v",
    "    let a = (v)",
    "    let a = (v).length",
    "    let a = foo.bar(v)",
    "    let a = v.length",
    "    let a = !v",
    "    let a = v + 1",
    "    let a = v == 1",
    "    let f = (n) => n\n    let a = f(v)",
    "    let a = Math.abs(v)",
    "    let a = v",
    "    log: v",
];

#[test]
fn a_variable_used_in_any_construct_is_not_reported_as_unused() {
    for use_of_v in USES_OF_V {
        let (lines, _) = run_continue(&quick(&format!("{CALLEE}{}", quick_fn("f", "", &format!("    let v = 1\n{use_of_v}")))));
        assert!(!lines.iter().any(|l| l.contains("Variable 'v' declared")), "{use_of_v:?}: {lines:?}");
    }
    // The same source without a use of `v` is reported, so the check does run on these functions.
    let (lines, _) = run_continue(&quick(&format!("{CALLEE}{}", quick_fn("f", "", "    let v = 1\n    let a = 5"))));
    assert_eq!(lines.iter().filter(|l| l.contains("Variable 'v' declared")).count(), 1, "{lines:?}");
}

#[test]
fn a_variable_used_only_in_a_node_the_parser_cannot_write_is_not_reported_as_unused() {
    let unused = |uses: Vec<QuickFuncStatement>| {
        let mut body = vec![sdecl("v", eint(1))];
        body.extend(uses);
        body.push(sret(eint(1)));
        hand_run(&[good("f", body)]).1.iter().any(|w| w.contains("Variable 'v' declared"))
    };
    assert!(unused(vec![]), "control: a variable nobody uses is reported");
    assert!(!unused(vec![sdecl("r", Expression::TypeCast { expression: bx(eid("v")), target_type: DataType::Float, position: qp(9, 6) })]));
    assert!(!unused(vec![QuickFuncStatement::ObjectCreation {
        variable: "o".to_string(),
        object: Value::Object { properties: vec![ObjectProperty { key: "k".to_string(), value: Value::Identifier { value: "v".to_string(), position: qp(9, 1) }, position: qp(9, 2) }], position: qp(9, 0) },
        position: qp(9, 3),
    }]));
    assert!(!unused(vec![sexpr(evalue(Value::Expression { expr: Box::new(eid("v")), position: qp(9, 1) }))]));
}

// ---- parameters, scopes and duplicates ----

#[test]
fn typed_collection_parameters_give_their_element_type_to_first() {
    // `xs.first()` is a string, so returning it from a function declared `<int>` is refused, a
    // method a string does not have is refused, and adding an integer to it is refused.
    for params in ["xs<array<string>>", "xs<tuple<string,int>>"] {
        let (lines, _) = run_continue(&quick(&format!("  ~f<int> => global({params}) {{\n    let a = xs.first()\n    return a\n  }}")));
        assert_eq!(count(&lines, "QFUNC015"), 1, "{params}: {lines:?}");
        let (lines, _) = run_continue(&quick(&format!("  ~f<int> => global({params}) {{\n    log: xs.first().nomethod()\n    return 1\n  }}")));
        assert_eq!(count(&lines, "QFUNC047"), 1, "{params}: {lines:?}");
        let (lines, _) = run_continue(&quick(&format!("  ~f<int> => global({params}) {{\n    let c = xs.first() + 1\n    log: c\n    return 1\n  }}")));
        assert_eq!(count(&lines, "QFUNC026"), 1, "{params}: {lines:?}");
    }
}

#[test]
fn a_parameter_cannot_be_assigned() {
    let (lines, _) = run_continue(&quick("  ~f<int> => global(a<int>) {\n    a = 2\n    return a\n  }"));
    assert_eq!(count(&lines, "QFUNC018"), 1, "{lines:?}");
}

#[test]
fn the_parameters_of_one_function_are_not_in_scope_in_the_next() {
    let (lines, _) = run_continue(&quick("  ~f1<int> => global(p<int>) {\n    return p\n  }\n  ~f2<int> => global() {\n    log: p\n    return 1\n  }"));
    assert!(lines.iter().any(|l| l.contains("Identifier 'p' not found") && l.contains("'f2'")), "{lines:?}");
    assert!(!lines.iter().any(|l| l.contains("'f1'")), "{lines:?}");
}

#[test]
fn a_function_defined_twice_is_reported_and_neither_body_is_checked() {
    let (lines, _) = run_continue(&quick("  ~f<int> => global() {\n    return 1\n  }\n  ~f<int> => global() {\n    return \"s\"\n  }"));
    assert_eq!(count(&lines, "QFUNC001"), 1, "{lines:?}");
    assert_eq!(count(&lines, "QFUNC015"), 0, "{lines:?}");
}

// ---- the orchestrator ----

#[test]
fn an_error_in_quickfuncs_stops_the_later_phases_only_under_halt() {
    let text = "@QUICKFUNCS(\n  ~f<int> => global() {\n    return \"s\"\n  }\n)\n@DATA(\n  a<int> = \"s\"\n)\n";
    for (strategy_name, data_error_expected) in [("halt", false), ("continue", true)] {
        let parsed = parse(text, strategy(strategy_name)).expect("parses");
        let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
        assert!(result.errors.iter().any(|e| e.error_id == "QFUNC015"), "{strategy_name}");
        assert_eq!(result.errors.iter().any(|e| e.section_name == "DATA"), data_error_expected, "{strategy_name}");
    }
}

#[test]
fn quickfuncs_are_refused_when_their_feature_is_not_enabled() {
    let mut parsed = parse(&source("qf_valid_basic"), ErrorHandlingStrategy::Continue).expect("parses");
    parsed.settings.enabled_features = vec!["data".to_string()];
    for document in [Some(&parsed.document), None] {
        let (result, _) = analyze(&parsed.ast, &parsed.settings, document);
        assert!(result.errors.iter().any(|e| e.error_id == "SEM_FEATURE"), "{:?}", result.errors);
        assert!(!result.section_results.contains_key("QUICKFUNCS"));
    }
}
