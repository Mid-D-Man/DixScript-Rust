// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/orchestrator_sections_document.rs"
// ============================================================================
//! Tests for the orchestrator reading `@IMPORTS`, `@ENUMS` and `@DLM` from a `Document`.
//!
//! `GeneralSemanticAnalyzer` can be given the arena form of a file with `with_document`. These three
//! sections hold nothing from the `Value` family, so the `Document` keeps them as the owned types,
//! and the orchestrator used to read them from the owned tree whether or not a document was
//! attached. It now reads them from the tree that is read, as it does for every other section: the
//! document's when one is attached, the owned tree's otherwise. The tests give the analyzer an owned
//! tree and a document that disagree on purpose, in both directions, and check which of the two each
//! section came from. They cover:
//!
//! * `@ENUMS`: the section result, the enums in the symbol table and the feature check;
//! * `@IMPORTS`: the feature check, and the resolution of the imported file;
//! * `@DLM`: the section result, and the `@SECURITY` section that `DEncryptor` makes necessary.
//!
//! A byte-for-byte comparison of the whole semantic result against the code from before the change,
//! over the corpus and 75 crafted sources with the features on and off, was made once with a
//! throwaway harness and is not repeated here.

use dixscript::Compiler::AST::arena::Document;
use dixscript::Compiler::AST::data_types::{CompatibilityMode, ErrorHandlingStrategy};
use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::{GeneralParser, GeneralSemanticAnalyzer, SemanticAnalysisResult};
use dixscript::ErrorManager::ErrorManager;

const PLAIN: &str = "@DATA(\n  a = 1\n)\n";
const ENUMS: &str = "@ENUMS(\n  Color { RED = 0, GREEN = 1 }\n)\n@DATA(\n  c<enum> = Color.RED\n)\n";
const DLM_ENCRYPTOR: &str = "@DLM(\n  DEncryptor.aes256\n)\n@DATA(\n  a = 1\n)\n";
const DLM_PLAIN: &str = "@DLM(\n  DCompressor.gzip\n)\n@DATA(\n  a = 1\n)\n";
const IMPORTS: &str = "@IMPORTS(\n  lib from \"no_such_file.mdix\"\n)\n@DATA(\n  a = 1\n)\n";

struct Parsed {
    document: Document,
    ast: DixScript,
    settings: OperationalSettings,
}

fn parse(source: &str) -> Parsed {
    let label = "orchestrator-sections-document-test";
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

/// Analyzes the owned tree of `owned` with the document of `other` attached (or not).
///
/// With `with_document` false the analyzer reads the owned tree for every section, so the result
/// is the one for `owned`; with it true the sections that moved to the arena, and now these three
/// as well, come from `other`.
fn analyze_pair(owned: &str, other: &str, with_document: bool, only_data_feature: bool) -> SemanticAnalysisResult {
    analyze_prepared(owned, other, with_document, only_data_feature, |_, _| {})
}

/// Like [`analyze_pair`], after `prepare` has had a look at both parsed files.
fn analyze_prepared(
    owned: &str,
    other: &str,
    with_document: bool,
    only_data_feature: bool,
    prepare: impl FnOnce(&mut Parsed, &mut Parsed),
) -> SemanticAnalysisResult {
    let mut owned = parse(owned);
    let mut other = parse(other);
    prepare(&mut owned, &mut other);
    let mut settings = other.settings.clone();
    if only_data_feature {
        // The parser refuses a section whose feature is off, so the analyzer's own check is reached
        // by switching the features after parsing.
        settings.enabled_features = vec!["data".to_string()];
    }
    let em = ErrorManager::new_isolated_silent();
    let mut analyzer = GeneralSemanticAnalyzer::new_with_error_manager(&owned.ast, &settings, em);
    if with_document {
        analyzer = analyzer.with_document(&other.document);
    }
    analyzer.analyze()
}

fn has_section(result: &SemanticAnalysisResult, name: &str) -> bool {
    result.section_results.contains_key(name)
}

fn errors_of(result: &SemanticAnalysisResult, section: &str) -> Vec<String> {
    result.errors.iter().filter(|e| e.section_name == section).map(|e| format!("{}: {}", e.error_id, e.message)).collect()
}

fn registers_enum(result: &SemanticAnalysisResult, name: &str) -> bool {
    result.symbol_table.as_ref().map_or(false, |t| t.enums.contains_key(name))
}

// ==================== @ENUMS ====================

#[test]
fn enums_come_from_the_document_when_one_is_attached() {
    // The document has the section, the owned tree does not.
    let attached = analyze_pair(PLAIN, ENUMS, true, false);
    assert!(has_section(&attached, "ENUMS"), "{:?}", attached.errors);
    assert!(registers_enum(&attached, "Color"));

    let detached = analyze_pair(PLAIN, ENUMS, false, false);
    assert!(!has_section(&detached, "ENUMS"));
    assert!(!registers_enum(&detached, "Color"));
}

#[test]
fn enums_come_from_the_owned_tree_when_no_document_is_attached() {
    // The owned tree has the section, the document does not.
    let detached = analyze_pair(ENUMS, PLAIN, false, false);
    assert!(has_section(&detached, "ENUMS"));
    assert!(registers_enum(&detached, "Color"));

    let attached = analyze_pair(ENUMS, PLAIN, true, false);
    assert!(!has_section(&attached, "ENUMS"));
    assert!(!registers_enum(&attached, "Color"));
}

#[test]
fn the_enums_feature_is_checked_on_the_section_that_is_read() {
    let attached = analyze_pair(PLAIN, ENUMS, true, true);
    assert_eq!(errors_of(&attached, "ENUMS").len(), 1, "{:?}", attached.errors);
    assert!(errors_of(&attached, "ENUMS")[0].starts_with("SEM_FEATURE"));

    assert!(errors_of(&analyze_pair(PLAIN, ENUMS, false, true), "ENUMS").is_empty());
    assert!(errors_of(&analyze_pair(ENUMS, PLAIN, true, true), "ENUMS").is_empty());
    assert_eq!(errors_of(&analyze_pair(ENUMS, PLAIN, false, true), "ENUMS").len(), 1);
}

// ==================== @IMPORTS ====================

#[test]
fn imports_come_from_the_tree_that_is_read() {
    // Resolving the import of a file that does not exist is an error in the `IMPORTS` section.
    let attached = analyze_pair(PLAIN, IMPORTS, true, false);
    assert!(!errors_of(&attached, "IMPORTS").is_empty(), "{:?}", attached.errors);
    assert!(errors_of(&analyze_pair(PLAIN, IMPORTS, false, false), "IMPORTS").is_empty());

    let detached = analyze_pair(IMPORTS, PLAIN, false, false);
    assert!(!errors_of(&detached, "IMPORTS").is_empty());
    assert!(errors_of(&analyze_pair(IMPORTS, PLAIN, true, false), "IMPORTS").is_empty());
}

#[test]
fn the_imports_feature_is_checked_on_the_section_that_is_read() {
    let feature_errors = |r: &SemanticAnalysisResult| -> usize {
        r.errors.iter().filter(|e| e.section_name == "IMPORTS" && e.error_id == "SEM_FEATURE").count()
    };
    // The check is made in two phases, the semantic one and the one that resolves the files, and
    // each reports the section once.
    assert_eq!(feature_errors(&analyze_pair(PLAIN, IMPORTS, true, true)), 2);
    assert_eq!(feature_errors(&analyze_pair(PLAIN, IMPORTS, false, true)), 0);
    assert_eq!(feature_errors(&analyze_pair(IMPORTS, PLAIN, true, true)), 0);
    assert_eq!(feature_errors(&analyze_pair(IMPORTS, PLAIN, false, true)), 2);
}

// ==================== @DLM ====================

#[test]
fn dlm_comes_from_the_tree_that_is_read() {
    assert!(has_section(&analyze_pair(PLAIN, DLM_PLAIN, true, false), "DLM"));
    assert!(!has_section(&analyze_pair(PLAIN, DLM_PLAIN, false, false), "DLM"));
    assert!(has_section(&analyze_pair(DLM_PLAIN, PLAIN, false, false), "DLM"));
    assert!(!has_section(&analyze_pair(DLM_PLAIN, PLAIN, true, false), "DLM"));
}

#[test]
fn an_encryptor_in_the_dlm_that_is_read_requires_a_security_section() {
    // The parser generates a default `@SECURITY` section for an encryptor, so the requirement is
    // only reached when that section is taken out of the tree that is read.
    let required = |r: &SemanticAnalysisResult| errors_of(r, "SECURITY").iter().any(|e| e.starts_with("SEM0002"));
    let no_security_in_document = |_: &mut Parsed, other: &mut Parsed| other.document.security = None;
    let no_security_in_owned = |owned: &mut Parsed, _: &mut Parsed| owned.ast.security = None;

    // The document has the encryptor, the owned tree has no DLM at all.
    assert!(required(&analyze_prepared(PLAIN, DLM_ENCRYPTOR, true, false, no_security_in_document)));
    assert!(!required(&analyze_prepared(PLAIN, DLM_ENCRYPTOR, false, false, no_security_in_document)));
    // The owned tree has the encryptor, the document has no DLM at all.
    assert!(required(&analyze_prepared(DLM_ENCRYPTOR, PLAIN, false, false, no_security_in_owned)));
    assert!(!required(&analyze_prepared(DLM_ENCRYPTOR, PLAIN, true, false, no_security_in_owned)));
    // A module that is not an encryptor does not require one.
    assert!(!required(&analyze_prepared(PLAIN, DLM_PLAIN, true, false, no_security_in_document)));
    // With the generated section left in, the requirement is met.
    assert!(!required(&analyze_pair(PLAIN, DLM_ENCRYPTOR, true, false)));
}
