// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/raw_analyzer_document.rs"
// ============================================================================
//! Tests for the `@RAW` analyzer reading a `Document`.
//!
//! `GeneralSemanticAnalyzer` can be given the arena form of a file with `with_document`. The
//! `@RAW` analyzer then reads the `Document` where it used to read the owned tree, and must
//! report exactly what it reported before. The tests check three things:
//!
//! * the two paths give identical diagnostics, positions and section results, for every `@RAW`
//!   error and warning code, under all three error strategies;
//! * the diagnostics are the ones the owned-tree analyzer recorded before the change (the table
//!   in `raw_diagnostic_matrix_matches_the_recording`), including that `Halt` stops after the
//!   first block with errors and skips the cross-block checks;
//! * the analyzer really reads the `Document` when it has one and the owned tree when it does
//!   not. A pair whose two halves disagree shows which half was read.

use dixscript::Compiler::AST::arena::Document;
use dixscript::Compiler::AST::data_types::{CompatibilityMode, ErrorHandlingStrategy};
use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::{GeneralParser, GeneralSemanticAnalyzer, SemanticAnalysisResult};
use dixscript::ErrorManager::ErrorManager;

/// One parsed file: the document, its export, and the settings the parse produced.
struct Parsed {
    document: Document,
    ast: DixScript,
    settings: OperationalSettings,
}

/// Runs the loader's first four stages and returns the document, or `None` when the parse
/// fails (for example when `@RAW` is not allowed by the file's features under `Halt`).
fn parse(source: &str, strategy: ErrorHandlingStrategy) -> Option<Parsed> {
    let label = "raw-analyzer-document-test";
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
        .map(|(name, s)| {
            let mut e: Vec<String> = s.errors.iter().map(|e| format!("{}:{}:{:?}", e.error_id, e.message, e.position)).collect();
            e.sort();
            let mut w: Vec<String> = s.warnings.iter().map(|w| format!("{}:{}:{:?}", w.warning_id, w.message, w.position)).collect();
            w.sort();
            format!("{name} ok={} errors={e:?} warnings={w:?}", s.is_success)
        })
        .collect();
    sections.sort();
    format!(
        "success={}\nerrors={errors:#?}\nwarnings={warnings:#?}\nsections={sections:#?}\nlogged={logged:#?}",
        result.is_success
    )
}

/// A well-formed block. The tag must be unique across a file, so callers pass distinct ones.
fn block(id: &str, format: &str, tag: &str, payload: &str) -> String {
    format!(
        "@RAW(\n  meta_data -> {{ id = \"{id}\", format = \"{format}\" }}\n  content -> {{\n---{tag}\n{payload}\n---{tag}\n  }}\n)\n"
    )
}

/// The sources the tests run. Each exercises a different `@RAW` check.
fn sources() -> Vec<(&'static str, String)> {
    vec![
        ("valid_two", format!("{}{}", block("a", "MPX", "ta", "AAAA"), block("b", "MPX", "tb", "BBBB"))),
        ("dup_id", format!("{}{}", block("same", "TXT", "t1", "A"), block("same", "TXT", "t2", "B"))),
        ("dup_tag", format!("{}{}", block("i1", "TXT", "same", "A"), block("i2", "TXT", "same", "B"))),
        (
            "dup_three",
            format!(
                "{}{}{}{}",
                block("x", "T", "p", "1"),
                block("x", "T", "q", "2"),
                block("y", "T", "p", "3"),
                block("x", "T", "p", "4")
            ),
        ),
        (
            "missing_id",
            "@RAW(\n  meta_data -> { format = \"TXT\" }\n  content -> {\n---c\ndata\n---c\n  }\n)\n".into(),
        ),
        (
            "missing_format",
            "@RAW(\n  meta_data -> { id = \"nf\" }\n  content -> {\n---c\ndata\n---c\n  }\n)\n".into(),
        ),
        ("missing_content", "@RAW(\n  meta_data -> { id = \"nc\", format = \"TXT\" }\n)\n".into()),
        (
            "empty_payload",
            "@RAW(\n  meta_data -> { id = \"ep\", format = \"TXT\" }\n  content -> {\n---e\n---e\n  }\n)\n".into(),
        ),
        (
            "blank_id",
            "@RAW(\n  meta_data -> { id = \"   \", format = \"TXT\" }\n  content -> {\n---c\nx\n---c\n  }\n)\n".into(),
        ),
        (
            "non_string_id_format",
            "@RAW(\n  meta_data -> { id = 5, format = 7 }\n  content -> {\n---c\nx\n---c\n  }\n)\n".into(),
        ),
        (
            "wrong_types",
            "@RAW(\n  meta_data -> { id = \"wt\", format = \"TXT\", size = \"big\", checksum = 5 }\n  using -> { threads = \"x\", filter = 5, compression = 6, module = 7, other = 1 }\n  content -> {\n---c\nx\n---c\n  }\n)\n".into(),
        ),
        (
            "good_types",
            "@RAW(\n  meta_data -> { id = \"gt\", format = \"TXT\", size = 9_000_000_000L, checksum = \"0xFA\" }\n  using -> { threads = 4, filter = \"f\", compression = \"gzip\", module = \"m\", other = true }\n  content -> {\n---c\nx\n---c\n  }\n)\n".into(),
        ),
        (
            "good_long_threads",
            "@RAW(\n  meta_data -> { id = \"gl\", format = \"TXT\" }\n  using -> { threads = 5_000_000_000L }\n  content -> {\n---c\nx\n---c\n  }\n)\n".into(),
        ),
        (
            "good_int_size",
            "@RAW(\n  meta_data -> { id = \"gi\", format = \"TXT\", size = 12 }\n  content -> {\n---c\nx\n---c\n  }\n)\n".into(),
        ),
        (
            "many_errors",
            format!(
                "{}{}{}",
                "@RAW(\n  meta_data -> { size = \"s\" }\n  using -> { threads = \"t\" }\n)\n",
                "@RAW(\n  meta_data -> { id = \"m\", format = \"F\", checksum = 1 }\n  content -> {\n---k\n---k\n  }\n)\n",
                block("m", "F", "k", "z"),
            ),
        ),
        ("with_data", format!("@DATA(\n  a = 1\n  b = \"two\"\n)\n{}", block("rd", "TXT", "rt", "payload"))),
        (
            "with_data_dup",
            format!("@DATA(\n  a = 1\n)\n{}{}", block("rd", "TXT", "rt", "p"), block("rd", "TXT", "rt2", "q")),
        ),
        (
            "with_quickfuncs",
            format!(
                "@CONFIG(\n  version -> \"1.0.0\"\n  features -> \"quickfuncs,data,raw\"\n)\n@QUICKFUNCS(\n  ~f<int>(_a<int>) {{\n    return _a + 1\n  }}\n)\n@DATA(\n  v = f(2)\n)\n{}",
                block("rq", "TXT", "rqt", "p")
            ),
        ),
        (
            "repeated_key",
            "@RAW(\n  meta_data -> { id = \"first\", id = \"second\", format = \"F\", format = 9 }\n  content -> {\n---c\nx\n---c\n  }\n)\n".into(),
        ),
        ("data_only", "@DATA(\n  a = 1\n  b = [1, 2, 3]\n)\n".into()),
    ]
}

const STRATEGIES: [(&str, ErrorHandlingStrategy); 3] = [
    ("halt", ErrorHandlingStrategy::Halt),
    ("continue", ErrorHandlingStrategy::Continue),
    ("recover", ErrorHandlingStrategy::Recover),
];

/// The ids of a result's `@RAW` section, errors then warnings, each sorted.
fn raw_ids(result: &SemanticAnalysisResult) -> (Vec<String>, Vec<String>) {
    let Some(section) = result.section_results.get("RAW") else {
        return (vec![], vec![]);
    };
    let mut errors: Vec<String> = section.errors.iter().map(|e| e.error_id.clone()).collect();
    errors.sort();
    let mut warnings: Vec<String> = section.warnings.iter().map(|w| w.warning_id.clone()).collect();
    warnings.sort();
    (errors, warnings)
}

fn ids(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// ==================== the two paths agree ====================

#[test]
fn document_path_and_owned_path_give_identical_diagnostics() {
    let mut compared = 0;
    for (name, source) in sources() {
        for (strategy_name, strategy) in STRATEGIES {
            let parsed = parse(&source, strategy)
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
    assert_eq!(compared, sources().len() * 3);
}

// ==================== the recorded behaviour ====================

/// Expected `@RAW` error ids and warning ids per source, from the owned-tree analyzer before it
/// moved to the arena. `None` means the section result is absent.
#[test]
fn raw_diagnostic_matrix_matches_the_recording() {
    // (source, strategy, errors, warnings)
    let table: Vec<(&str, &str, Vec<&str>, Vec<&str>)> = vec![
        ("valid_two", "halt", vec![], vec![]),
        ("valid_two", "continue", vec![], vec![]),
        ("dup_id", "halt", vec!["RAW007"], vec![]),
        ("dup_id", "continue", vec!["RAW007"], vec![]),
        ("dup_tag", "halt", vec!["RAW008"], vec![]),
        ("dup_tag", "recover", vec!["RAW008"], vec![]),
        ("dup_three", "halt", vec!["RAW007", "RAW007", "RAW008", "RAW008"], vec![]),
        ("dup_three", "continue", vec!["RAW007", "RAW007", "RAW008", "RAW008"], vec![]),
        ("missing_id", "halt", vec!["RAW001"], vec![]),
        ("missing_id", "continue", vec!["RAW001"], vec![]),
        ("missing_format", "halt", vec!["RAW002"], vec![]),
        ("missing_content", "halt", vec!["RAW003"], vec![]),
        ("missing_content", "recover", vec!["RAW003"], vec![]),
        ("empty_payload", "halt", vec![], vec!["RAW_WARN001"]),
        ("empty_payload", "continue", vec![], vec!["RAW_WARN001"]),
        ("blank_id", "halt", vec!["RAW001"], vec![]),
        ("non_string_id_format", "halt", vec!["RAW001", "RAW002"], vec![]),
        ("non_string_id_format", "continue", vec!["RAW001", "RAW002"], vec![]),
        (
            "wrong_types",
            "halt",
            vec!["RAW004", "RAW005", "RAW006"],
            vec!["RAW_WARN002", "RAW_WARN002", "RAW_WARN002"],
        ),
        (
            "wrong_types",
            "continue",
            vec!["RAW004", "RAW005", "RAW006"],
            vec!["RAW_WARN002", "RAW_WARN002", "RAW_WARN002"],
        ),
        ("good_types", "halt", vec![], vec![]),
        ("good_int_size", "continue", vec![], vec![]),
        ("good_long_threads", "halt", vec![], vec![]),
        ("good_long_threads", "continue", vec![], vec![]),
        ("with_data", "halt", vec![], vec![]),
        ("with_data_dup", "halt", vec!["RAW007"], vec![]),
        ("with_quickfuncs", "continue", vec![], vec![]),
    ];

    for (name, strategy_name, errors, warnings) in table {
        let source = sources().into_iter().find(|(n, _)| *n == name).expect("known source").1;
        let strategy = STRATEGIES.iter().find(|(n, _)| *n == strategy_name).expect("known strategy").1;
        let parsed = parse(&source, strategy).expect("parses");
        let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
        assert_eq!(raw_ids(&result), (ids(&errors), ids(&warnings)), "{name} [{strategy_name}]");
    }
}

#[test]
fn halt_stops_after_the_first_block_with_errors_and_continue_does_not() {
    let source = sources().into_iter().find(|(n, _)| *n == "many_errors").expect("known source").1;

    let halt = parse(&source, ErrorHandlingStrategy::Halt).expect("parses");
    let (result, _) = analyze(&halt.ast, &halt.settings, Some(&halt.document));
    // Only the first block's own checks ran. The duplicate id and tag, which sit in the second
    // and third blocks, were never looked at.
    assert_eq!(raw_ids(&result).0, ids(&["RAW001", "RAW002", "RAW003", "RAW004", "RAW006"]));

    let keep_going = parse(&source, ErrorHandlingStrategy::Continue).expect("parses");
    let (result, _) = analyze(&keep_going.ast, &keep_going.settings, Some(&keep_going.document));
    assert_eq!(
        raw_ids(&result),
        (
            ids(&["RAW001", "RAW002", "RAW003", "RAW004", "RAW005", "RAW006", "RAW007", "RAW008"]),
            ids(&["RAW_WARN001"])
        )
    );
}

#[test]
fn the_first_seen_line_in_a_duplicate_message_is_the_first_block() {
    let source = sources().into_iter().find(|(n, _)| *n == "dup_id").expect("known source").1;
    let parsed = parse(&source, ErrorHandlingStrategy::Continue).expect("parses");
    let (result, _) = analyze(&parsed.ast, &parsed.settings, Some(&parsed.document));
    let error = &result.section_results["RAW"].errors[0];
    assert!(error.message.contains("'same'"), "{}", error.message);
    assert!(error.message.contains("first seen at line 1"), "{}", error.message);
    // The error points at the second block, which starts on line 9.
    assert_eq!(error.position.map(|p| p.line), Some(9));
}

#[test]
fn a_file_with_no_raw_blocks_has_no_raw_section_result() {
    let source = sources().into_iter().find(|(n, _)| *n == "data_only").expect("known source").1;
    let parsed = parse(&source, ErrorHandlingStrategy::Halt).expect("parses");
    for document in [Some(&parsed.document), None] {
        let (result, _) = analyze(&parsed.ast, &parsed.settings, document);
        assert!(result.is_success);
        assert!(!result.section_results.contains_key("RAW"));
    }
}

// ==================== which tree is read ====================

#[test]
fn the_analyzer_reads_the_document_when_one_is_attached() {
    // The document holds two blocks with the same id. The owned tree handed to the analyzer
    // has no blocks at all, so a duplicate can only come from the document.
    let source = sources().into_iter().find(|(n, _)| *n == "dup_id").expect("known source").1;
    let parsed = parse(&source, ErrorHandlingStrategy::Continue).expect("parses");
    let empty = DixScript::new();

    let (with_doc, _) = analyze(&empty, &parsed.settings, Some(&parsed.document));
    assert_eq!(raw_ids(&with_doc).0, ids(&["RAW007"]));

    let (without, _) = analyze(&empty, &parsed.settings, None);
    assert!(!without.section_results.contains_key("RAW"));
}

#[test]
fn the_analyzer_reads_the_owned_tree_when_no_document_is_attached() {
    // The mirror image: the document has no blocks and the owned tree has a duplicate.
    let source = sources().into_iter().find(|(n, _)| *n == "dup_id").expect("known source").1;
    let parsed = parse(&source, ErrorHandlingStrategy::Continue).expect("parses");
    let empty_document = Document::from_owned(&DixScript::new());

    let (without, _) = analyze(&parsed.ast, &parsed.settings, None);
    assert_eq!(raw_ids(&without).0, ids(&["RAW007"]));

    let (with_empty, _) = analyze(&parsed.ast, &parsed.settings, Some(&empty_document));
    assert!(!with_empty.section_results.contains_key("RAW"));
}

// ==================== DocRawBlock::id and format ====================

#[test]
fn doc_raw_block_id_and_format_match_the_owned_block() {
    let mut blocks_checked = 0;
    for (name, source) in sources() {
        let parsed = parse(&source, ErrorHandlingStrategy::Continue).expect("parses");
        assert_eq!(parsed.document.raw.len(), parsed.ast.raw.len(), "{name}");
        for (doc_block, owned) in parsed.document.raw.iter().zip(&parsed.ast.raw) {
            let arena = &parsed.document.arena;
            assert_eq!(doc_block.id(arena), owned.id(), "{name}: id");
            assert_eq!(doc_block.format(arena), owned.format(), "{name}: format");
            blocks_checked += 1;
        }
    }
    assert!(blocks_checked >= 20, "only {blocks_checked} blocks were compared");
}

#[test]
fn doc_raw_block_id_is_none_for_a_value_that_is_not_a_string() {
    let source = sources().into_iter().find(|(n, _)| *n == "non_string_id_format").expect("known source").1;
    let parsed = parse(&source, ErrorHandlingStrategy::Continue).expect("parses");
    let arena = &parsed.document.arena;
    assert_eq!(parsed.document.raw[0].id(arena), None);
    assert_eq!(parsed.document.raw[0].format(arena), None);
}

#[test]
fn the_first_of_a_repeated_key_wins() {
    // The owned block takes the first field with the key, and so must the arena block. The
    // second `format` is an integer, so taking the last one instead would turn a present
    // format into a missing one.
    let source = sources().into_iter().find(|(n, _)| *n == "repeated_key").expect("known source").1;
    let parsed = parse(&source, ErrorHandlingStrategy::Continue).expect("parses");
    let arena = &parsed.document.arena;
    assert_eq!(parsed.ast.raw[0].id(), Some("first"));
    assert_eq!(parsed.document.raw[0].id(arena), Some("first"));
    assert_eq!(parsed.document.raw[0].format(arena), Some("F"));
}

#[test]
fn doc_raw_block_id_is_the_text_of_the_string() {
    let source = sources().into_iter().find(|(n, _)| *n == "valid_two").expect("known source").1;
    let parsed = parse(&source, ErrorHandlingStrategy::Continue).expect("parses");
    let arena = &parsed.document.arena;
    assert_eq!(parsed.document.raw[0].id(arena), Some("a"));
    assert_eq!(parsed.document.raw[1].id(arena), Some("b"));
    assert_eq!(parsed.document.raw[1].format(arena), Some("MPX"));
}
