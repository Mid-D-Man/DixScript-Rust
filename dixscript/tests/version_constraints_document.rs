// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/version_constraints_document.rs"
// ============================================================================
//! Tests for `VersionConstraints::validate_script_doc`.
//!
//! `validate_script` checks a script against the version and feature rules: the `@CONFIG` entries,
//! the sections that need a feature, the `@DLM` modules, the parameters of every quick function and
//! the top-level kind of every value in `@DATA`. `validate_script_doc` does the same for a file held
//! as a `Document`; only `@QUICKFUNCS` and `@DATA` are read through the arena, the other sections being
//! the same owned types in both. The tests check:
//!
//! * the result for six hand-built scripts under 24 `@CONFIG` variants (feature lists and strings,
//!   versions, debug modes, no config) is the one the code before the change gave, through the owned
//!   form and through the document form (`RECORDING`, a fingerprint per script, recorded by running
//!   the original code);
//! * the two forms agree on every file of `mdix_files/` under the same 24 variants;
//! * a quick function whose default value cannot convert to its parameter's type is reported through
//!   both forms;
//! * the orchestrator's version phase reads the document when one is attached.

use dixscript::Compiler::AST::arena::Document;
use dixscript::Compiler::AST::data_types::{CompatibilityMode, ErrorHandlingStrategy};
use dixscript::Compiler::AST::{
    ConfigEntry, ConfigSection, ConfigValue, DLMModule, DLMModuleSubtype, DLMModuleType, DLMSection, DataEntry,
    DataSection, DataType, DixScript, EnumsSection, Expression, ImportsSection, Position, PropertyAssignment,
    QuickFuncParam, QuickFuncsSection, QuickFunction, TablePath, Value,
};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::{GeneralParser, GeneralSemanticAnalyzer};
use dixscript::Compiler::VersionControl::{ValidationResult, VersionConstraints};
use dixscript::ErrorManager::ErrorManager;
use std::path::{Path, PathBuf};

fn pos(line: usize, column: usize) -> Position {
    Position::new(line, column)
}

fn fnv(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ==================== the config variants and the hand-built scripts ====================

fn cv(entries: Vec<(&str, ConfigValue)>) -> Option<ConfigSection> {
    Some(ConfigSection::new(
        entries.into_iter().map(|(k, v)| ConfigEntry::new(k.to_string(), v, pos(1, 1))).collect(),
        pos(1, 1),
    ))
}

/// None = keep the parsed config; Some(x) = replace it with x.
fn variants() -> Vec<(&'static str, Option<Option<ConfigSection>>)> {
    let st = |s: &str| ConfigValue::String(s.to_string());
    let list = |v: &[&str]| ConfigValue::Features(v.iter().map(|s| s.to_string()).collect());
    vec![
        ("as_parsed", None),
        ("no_config", Some(None)),
        ("features_data_list", Some(cv(vec![("features", list(&["data"]))]))),
        ("features_basic", Some(cv(vec![("features", st("basic"))]))),
        ("features_advanced", Some(cv(vec![("FEATURES", st("Advanced"))]))),
        ("features_quickfuncs", Some(cv(vec![("features", st("quickfuncs"))]))),
        ("features_qf_enums", Some(cv(vec![("features", st("quickfuncs, enums"))]))),
        ("features_imports", Some(cv(vec![("features", st("imports"))]))),
        ("features_all_list", Some(cv(vec![("features", list(&["quickfuncs", "enums", "imports", "dlm"]))]))),
        ("features_bogus", Some(cv(vec![("features", st("bogus"))]))),
        ("features_bogus_list", Some(cv(vec![("features", list(&["quickfuncs", "nope"]))]))),
        ("features_int", Some(cv(vec![("features", ConfigValue::Integer(3))]))),
        ("features_single_advanced_list", Some(cv(vec![("features", list(&["advanced"]))]))),
        ("features_single_other_list", Some(cv(vec![("features", list(&["nope"]))]))),
        ("features_first_wins", Some(cv(vec![("features", list(&["data"])), ("features", st("advanced"))]))),
        ("version_ok", Some(cv(vec![("version", st("1.0.0"))]))),
        ("version_2", Some(cv(vec![("version", st("2.0.0"))]))),
        ("version_bad", Some(cv(vec![("version", st("9.9.9"))]))),
        ("version_int", Some(cv(vec![("version", ConfigValue::Integer(2))]))),
        ("debug_bad", Some(cv(vec![("debug_mode", st("loud"))]))),
        ("debug_ok", Some(cv(vec![("debug_mode", st("verbose"))]))),
        ("debug_int", Some(cv(vec![("debug_mode", ConfigValue::Integer(1))]))),
        ("unknown_key", Some(cv(vec![("zzz", st("x"))]))),
        ("empty_config", Some(cv(vec![]))),
    ]
}

fn _unused(document: &Document) -> dixscript::Compiler::VersionControl::ValidationResult {
    VersionConstraints::new().validate_script_doc(document)
}


fn qcall(name: &str) -> Value {
    Value::QuickFuncCall { function_name: name.to_string(), arguments: vec![], position: pos(5, 1) }
}
fn vint(n: i32) -> Value {
    Value::Integer { value: n, position: pos(5, 2) }
}
fn path(segs: &[&str]) -> TablePath {
    TablePath::new(segs.iter().map(|s| s.to_string()).collect())
}
fn simple(name: &str, v: Value) -> DataEntry {
    DataEntry::SimpleProperty { name: name.to_string(), data_type: None, value: v, position: pos(6, 1) }
}
fn table(p: &[&str], props: Vec<(&str, Value)>) -> DataEntry {
    DataEntry::TableProperty { path: path(p), properties: props.into_iter().map(|(n, v)| PropertyAssignment::new(n.to_string(), None, v, pos(6, 2))).collect(), position: pos(6, 3) }
}
fn group(p: &[&str], items: Vec<Value>) -> DataEntry {
    DataEntry::GroupArray { path: path(p), items, position: pos(6, 4) }
}
fn hfunc(name: &str, params: Vec<QuickFuncParam>) -> QuickFunction {
    QuickFunction::new(name.to_string(), Some(DataType::Int), Some(vec!["global".to_string()]), params, vec![], pos(7, 1))
}
fn hparam(name: &str, t: Option<DataType>, d: Option<Expression>) -> QuickFuncParam {
    QuickFuncParam::new(name.to_string(), t, d, pos(7, 2))
}
fn dval(v: Value) -> Expression {
    Expression::Value { value: v, position: pos(7, 3) }
}
fn dlm(mods: Vec<(DLMModuleType, Option<DLMModuleSubtype>)>) -> DLMSection {
    DLMSection::new(mods.into_iter().map(|(t, s)| DLMModule::new(t, s, pos(8, 1))).collect(), pos(8, 0))
}

fn hand_scripts() -> Vec<(&'static str, DixScript)> {
    let base = || DixScript::new();
    let mut v: Vec<(&'static str, DixScript)> = Vec::new();
    let mut s = base();
    s.data = Some(DataSection::new(vec![
        simple("plain", vint(1)), simple("called", qcall("f")),
        table(&["t"], vec![("a", vint(1)), ("b", qcall("g"))]), table(&[], vec![("c", qcall("g"))]), table(&["u", "v"], vec![]),
        group(&["g"], vec![vint(1), qcall("h")]), group(&[], vec![qcall("h")]), group(&["e"], vec![]),
        DataEntry::ObjectProperty { name: "o".into(), data_type: None, object: Box::new(Value::Object { properties: vec![], position: pos(5, 3) }), position: pos(6, 5) },
    ], pos(6, 0)));
    v.push(("data_forms", s));
    let mut s = base();
    s.quick_functions = Some(QuickFuncsSection::new(vec![
        hfunc("ok", vec![hparam("a", Some(DataType::Int), Some(dval(vint(1)))), hparam("b", Some(DataType::Double), Some(dval(vint(2)))), hparam("u", None, Some(dval(vint(3)))), hparam("n", Some(DataType::Int), None)]),
        hfunc("bad", vec![hparam("a", Some(DataType::Int), Some(dval(Value::String { value: "s".into(), position: pos(5, 4) }))), hparam("b", Some(DataType::Bool), Some(dval(vint(1)))), hparam("c", Some(DataType::String), Some(dval(Value::Boolean { value: true, position: pos(5, 5) })))]),
        hfunc("exprs", vec![hparam("a", Some(DataType::Int), Some(Expression::Identifier { name: "x".into(), position: pos(7, 4) })), hparam("b", Some(DataType::Array), Some(dval(Value::Array { values: vec![], position: pos(5, 6) }))), hparam("c", Some(DataType::Hex), Some(dval(Value::HexColor { value: "#FF".into(), position: pos(5, 7) }))), hparam("d", Some(DataType::Blob), Some(dval(Value::PrefixedConstructor { prefix: "b".into(), arguments: vec![], position: pos(5, 8) }))), hparam("e", Some(DataType::Long), Some(dval(Value::Long { value: 1, position: pos(5, 9) }))), hparam("f", Some(DataType::Float), Some(dval(Value::Null { position: pos(5, 10) })))]),
        hfunc("none", vec![]),
    ], pos(7, 0)));
    v.push(("quickfunc_signatures", s));
    let mut s = base();
    s.dlm = Some(dlm(vec![
        (DLMModuleType::DCompressor, Some(DLMModuleSubtype::Gzip)), (DLMModuleType::DCompressor, None), (DLMModuleType::DCompressor, Some(DLMModuleSubtype::Bzip2)), (DLMModuleType::DCompressor, Some(DLMModuleSubtype::Lzma)),
        (DLMModuleType::DAuditor, Some(DLMModuleSubtype::Diy)), (DLMModuleType::DAuditor, Some(DLMModuleSubtype::Enhanced)), (DLMModuleType::DAuditor, None), (DLMModuleType::DAuditor, Some(DLMModuleSubtype::Gzip)),
        (DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Aes128)), (DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Aes256)), (DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Chacha20)), (DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Xor)), (DLMModuleType::DEncryptor, None), (DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Gzip)),
        (DLMModuleType::ParseError, Some(DLMModuleSubtype::ParseError)), (DLMModuleType::ParseError, None), (DLMModuleType::ParseError, Some(DLMModuleSubtype::Gzip)),
    ]));
    v.push(("dlm_modules", s));
    let mut s = base();
    s.imports = Some(ImportsSection::new(vec![], pos(9, 0)));
    s.enums = Some(EnumsSection::new(vec![], pos(9, 1)));
    s.security = None;
    v.push(("imports_and_enums", s));
    let mut s = base();
    s.imports = Some(ImportsSection::new(vec![], pos(9, 0)));
    s.enums = Some(EnumsSection::new(vec![], pos(9, 1)));
    s.dlm = Some(dlm(vec![(DLMModuleType::DCompressor, Some(DLMModuleSubtype::Lzma))]));
    s.quick_functions = Some(QuickFuncsSection::new(vec![hfunc("bad", vec![hparam("a", Some(DataType::Int), Some(dval(Value::String { value: "s".into(), position: pos(5, 4) })))])], pos(7, 0)));
    s.data = Some(DataSection::new(vec![simple("called", qcall("f")), table(&[], vec![("c", qcall("g"))]), group(&[], vec![qcall("h")])], pos(6, 0)));
    v.push(("everything", s));
    v.push(("empty", base()));
    v
}


/// (script, lines recorded, fingerprint of the lines), recorded from the code before the change.
const RECORDING: &[(&str, usize, u64)] = &[
    ("data_forms", 24, 0xfc43527e1fde4441),
    ("quickfunc_signatures", 24, 0xc506871a59f15147),
    ("dlm_modules", 24, 0x7a136682d11c9d39),
    ("imports_and_enums", 24, 0x74dc8a10d966ea28),
    ("everything", 24, 0x6ff2e82ddfc55bb2),
    ("empty", 24, 0x3a98d57bcbd3072a),
];

/// Applies a variant to both forms of a script.
fn apply(variant: &Option<Option<ConfigSection>>, ast: &mut DixScript, document: &mut Document) {
    if let Some(config) = variant {
        ast.config = config.clone();
        document.config = config.clone();
    }
}

fn lines_of(name: &str, script: &DixScript, through_document: bool) -> Vec<String> {
    let mut ast = script.clone();
    let mut document = Document::from_owned(&ast);
    let constraints = VersionConstraints::new();
    variants()
        .iter()
        .map(|(variant_name, variant)| {
            apply(variant, &mut ast, &mut document);
            let result: ValidationResult =
                if through_document { constraints.validate_script_doc(&document) } else { constraints.validate_script(&ast) };
            format!("VH {name} {variant_name}: {result:?}")
        })
        .collect()
}

#[test]
fn every_script_has_a_recording() {
    let scripts = hand_scripts();
    assert_eq!(scripts.len(), RECORDING.len());
    for (name, _) in &scripts {
        assert!(RECORDING.iter().any(|(n, _, _)| n == name), "{name}");
    }
}

#[test]
fn hand_built_scripts_give_the_recorded_results_through_both_forms() {
    for (name, script) in hand_scripts() {
        let (_, count, hash) = *RECORDING.iter().find(|(n, _, _)| *n == name).expect("a recording");
        for through_document in [false, true] {
            let lines = lines_of(name, &script, through_document);
            assert_eq!(lines.len(), count, "{name}");
            assert_eq!(fnv(&lines.join("\n")), hash, "{name} (document: {through_document}):\n{}", lines.join("\n"));
        }
    }
}

// ==================== the corpus ====================

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).expect("a readable directory").filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().map_or(false, |x| x == "mdix") {
            out.push(path);
        }
    }
}

struct Parsed {
    document: Document,
    settings: OperationalSettings,
}

fn parse(source: &[u8], label: &str) -> Option<Parsed> {
    let em = ErrorManager::new_isolated_silent();
    let initial = OperationalSettings {
        source_file_path: Some(label.to_string()),
        compatibility_mode: CompatibilityMode::Strict,
        ..OperationalSettings::default()
    };
    let tokens = Tokenizer::new_with_error_manager_from_bytes(source, &initial, em.clone()).tokenize();
    let split = split_config_tokens(tokens.tokens);
    let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
    let config = handler.process_config_tokens(&split.config_tokens);
    let mut settings = config.operational_settings;
    settings.source_file_path = Some(label.to_string());
    settings.error_handling_strategy = ErrorHandlingStrategy::Continue;
    em.update_settings(settings.clone());
    let document = GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &settings, em)
        .ok()?
        .parse_document()
        .ok()?;
    Some(Parsed { document, settings })
}

#[test]
fn the_two_forms_agree_on_every_corpus_file_under_every_config_variant() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("mdix_files");
    let mut files = Vec::new();
    walk(&root, &mut files);
    assert!(files.len() >= 50, "expected the corpus under {}", root.display());

    let constraints = VersionConstraints::new();
    let (mut checked, mut invalid) = (0, 0);
    for path in &files {
        let Some(mut parsed) = parse(&std::fs::read(path).expect("a readable file"), &path.to_string_lossy()) else { continue };
        let mut ast = parsed.document.export();
        for (variant_name, variant) in variants() {
            apply(&variant, &mut ast, &mut parsed.document);
            let owned = constraints.validate_script(&ast);
            let from_document = constraints.validate_script_doc(&parsed.document);
            assert_eq!(format!("{owned:?}"), format!("{from_document:?}"), "{} [{variant_name}]", path.display());
            checked += 1;
            invalid += usize::from(!owned.is_valid);
        }
    }
    assert!(checked >= 50 * 24, "only {checked} comparisons");
    assert!(invalid > 200, "only {invalid} of {checked} results had an error, so the variants did not bite");
}

// ==================== a default that cannot convert ====================

const DEFAULT_MISMATCH: &str = "@QUICKFUNCS(\n  ~f<int> => global(a<int> = \"text\", b<float> = 1, c<string> = 2) {\n    return 1\n  }\n)\n@DATA(\n  x = 1\n)\n";

#[test]
fn a_default_that_cannot_convert_to_its_parameters_type_is_reported_through_both_forms() {
    let parsed = parse(DEFAULT_MISMATCH.as_bytes(), "default-mismatch").expect("parses");
    let ast = parsed.document.export();
    let constraints = VersionConstraints::new();
    for result in [constraints.validate_script(&ast), constraints.validate_script_doc(&parsed.document)] {
        assert!(!result.is_valid);
        // `b` and `c` are fine: an integer converts to a float and to a string.
        assert_eq!(
            result.errors,
            ["QUICKFUNCS: Parameter 'a' default value type String cannot convert to declared type Int"]
        );
    }
}

// ==================== the orchestrator's version phase ====================

#[test]
fn the_version_phase_reads_the_document_when_one_is_attached() {
    // The owned tree is a plain file; the document holds the same file with `@ENUMS` added and a
    // config that does not permit them. Only the tree that is read can report it.
    let plain = parse(b"@DATA(\n  a = 1\n)\n", "plain").expect("parses");
    let mut other = parse(b"@ENUMS(\n  Color { RED = 0 }\n)\n@DATA(\n  a = 1\n)\n", "enums").expect("parses");
    other.document.config = cv(vec![("features", ConfigValue::Features(vec!["data".to_string()]))]);
    let ast = plain.document.export();

    let version_errors = |attach: bool| -> Vec<String> {
        let em = ErrorManager::new_isolated_silent();
        let mut analyzer = GeneralSemanticAnalyzer::new_with_error_manager(&ast, &plain.settings, em);
        if attach {
            analyzer = analyzer.with_document(&other.document);
        }
        analyzer.analyze().errors.iter().filter(|e| e.error_id == "SEM_VERSION").map(|e| e.message.clone()).collect()
    };

    assert_eq!(version_errors(true), ["ENUMS section requires advanced mode or the 'enums' feature"]);
    assert!(version_errors(false).is_empty());
}
