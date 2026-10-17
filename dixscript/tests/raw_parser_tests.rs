// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/raw_parser_tests.rs"
// ============================================================================
//! Tests for the `@RAW` section parser.
//!
//! `raw_section_tests.rs` covers the lexer and the analyzer through the loader. These tests
//! pin the parser itself, which is the second parser built on the arena AST: what a block
//! parses to (including source positions), what each error strategy does with malformed
//! input, and which diagnostics are reported. Every expectation was taken from the output of
//! the original, pre-arena parser.
//!
//! The helper runs the loader's first four stages (tokenize, split `@CONFIG`, process
//! `@CONFIG`, parse) with an isolated error manager, so the parse diagnostics can be read back.

use dixscript::Compiler::AST::data_types::CompatibilityMode;
use dixscript::Compiler::AST::{Position, RawBlock, RawField, Value};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::GeneralParser;
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::ErrorManager::ErrorManager;

/// The parsed `@RAW` blocks, the parse diagnostics as text, and the source that was parsed.
struct Parsed {
    raw: Vec<RawBlock>,
    parse_errors: String,
    source: String,
}

fn parse_source(source: String) -> Parsed {
    let label = "raw-parser-test";
    let em = ErrorManager::new_isolated_silent();
    let initial = OperationalSettings {
        source_file_path: Some(label.to_string()),
        compatibility_mode: CompatibilityMode::BestEffort,
        ..OperationalSettings::default()
    };
    let tokens = Tokenizer::new_with_error_manager_from_bytes(source.as_bytes(), &initial, em.clone()).tokenize();
    let split = split_config_tokens(tokens.tokens);
    let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
    let config = handler.process_config_tokens(&split.config_tokens);
    let mut settings = config.operational_settings;
    settings.source_file_path = Some(label.to_string());
    em.update_settings(settings.clone());
    let ast = GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &settings, em.clone())
        .expect("the parser starts")
        .parse()
        .expect("the parse returns an AST");
    Parsed { raw: ast.raw, parse_errors: em.get_parse_errors_as_string(), source }
}

/// Parses `section` as a whole file under the default (halting) error strategy. The
/// three-line `@CONFIG` header puts the section's first line on line 4.
fn parse(section: &str) -> Parsed {
    parse_source(format!("@CONFIG(\n  features -> \"data,raw,schema\"\n)\n{section}"))
}

/// Parses `section` under `strategy` ("continue" or "recover"). The four-line `@CONFIG`
/// header puts the section's first line on line 5.
fn parse_with(strategy: &str, section: &str) -> Parsed {
    parse_source(format!(
        "@CONFIG(\n  features -> \"data,raw,schema\"\n  error_handling -> \"{strategy}\"\n)\n{section}"
    ))
}

fn pos(line: usize, column: usize) -> Position {
    Position::new(line, column)
}

fn text(t: &str, line: usize, column: usize) -> Value {
    Value::String { value: t.to_string(), position: pos(line, column) }
}

fn field(key: &str, value: Value, line: usize, column: usize) -> RawField {
    RawField { key: key.to_string(), value, position: pos(line, column) }
}

/// The bytes a block's `content` token points at in the parsed source.
fn payload<'a>(parsed: &'a Parsed, block: &RawBlock) -> &'a str {
    let content = block.content.as_ref().expect("the block has content");
    std::str::from_utf8(&parsed.source.as_bytes()[content.start..content.end]).expect("payload is UTF-8")
}

#[test]
fn a_full_block_keeps_fields_content_and_positions() {
    let parsed = parse(
        "@RAW(\n  meta_data -> { id = \"img1\", kind = \"png\" }\n  using -> { codec = \"gzip\", level = 3 }\n  content -> {\n    ---tag001\nhello world\n---tag001\n  }\n)",
    );
    assert_eq!(parsed.raw.len(), 1);
    let block = &parsed.raw[0];

    assert_eq!(
        block.meta_data,
        vec![field("id", text("img1", 5, 23), 5, 18), field("kind", text("png", 5, 38), 5, 31)]
    );
    assert_eq!(
        block.using,
        vec![
            field("codec", text("gzip", 6, 22), 6, 14),
            field("level", Value::Integer { value: 3, position: pos(6, 38) }, 6, 30),
        ]
    );
    let content = block.content.as_ref().expect("content");
    assert_eq!(content.tag, "tag001");
    assert_eq!(content.position, pos(7, 3));
    assert_eq!(payload(&parsed, block), "hello world\n");
    assert_eq!(block.position, pos(4, 5));
    assert_eq!(parsed.parse_errors, "");
}

#[test]
fn every_scalar_value_kind_is_read() {
    let parsed = parse(
        "@RAW(\n  meta_data -> { s = \"a\", t = 'b', i = 1, l = 9000000000L, f = 1.5f, d = 2.5, b = true, c = false, h = #FF00AA, n = null, neg = -7 }\n)",
    );
    let expected = vec![
        field("s", text("a", 5, 22), 5, 18),
        field("t", text("b", 5, 31), 5, 27),
        field("i", Value::Integer { value: 1, position: pos(5, 40) }, 5, 36),
        field("l", Value::Long { value: 9_000_000_000, position: pos(5, 47) }, 5, 43),
        field("f", Value::Float { value: 1.5, position: pos(5, 64) }, 5, 60),
        field("d", Value::Double { value: 2.5, position: pos(5, 74) }, 5, 70),
        field("b", Value::Boolean { value: true, position: pos(5, 83) }, 5, 79),
        field("c", Value::Boolean { value: false, position: pos(5, 93) }, 5, 89),
        field("h", Value::HexColor { value: "#FF00AA".to_string(), position: pos(5, 104) }, 5, 100),
        field("n", Value::Null { position: pos(5, 117) }, 5, 113),
        field("neg", Value::Integer { value: -7, position: pos(5, 129) }, 5, 123),
    ];
    assert_eq!(parsed.raw.len(), 1);
    assert_eq!(parsed.raw[0].meta_data, expected);
    assert!(parsed.raw[0].using.is_empty());
    assert!(parsed.raw[0].content.is_none());
    assert_eq!(parsed.parse_errors, "");
}

#[test]
fn an_empty_section_gives_an_empty_block() {
    let parsed = parse("@RAW()");
    assert_eq!(parsed.raw.len(), 1);
    let block = &parsed.raw[0];
    assert!(block.meta_data.is_empty() && block.using.is_empty() && block.content.is_none());
    assert_eq!(block.position, pos(4, 5));
}

#[test]
fn commas_between_entries_and_after_the_last_field_are_allowed() {
    let parsed = parse("@RAW(\n  meta_data -> { id = \"x\" },\n  using -> { a = 1 },\n)");
    assert_eq!(parsed.raw.len(), 1);
    assert_eq!(parsed.raw[0].meta_data, vec![field("id", text("x", 5, 23), 5, 18)]);
    assert_eq!(
        parsed.raw[0].using,
        vec![field("a", Value::Integer { value: 1, position: pos(6, 18) }, 6, 14)]
    );
    assert_eq!(parsed.parse_errors, "");

    let trailing = parse("@RAW(\n  meta_data -> { id = \"x\", }\n)");
    assert_eq!(trailing.raw[0].meta_data, vec![field("id", text("x", 5, 23), 5, 18)]);
    assert_eq!(trailing.parse_errors, "");
}

#[test]
fn two_sections_give_two_blocks() {
    let parsed = parse("@RAW(\n  meta_data -> { id = \"one\" }\n)\n@RAW(\n  meta_data -> { id = \"two\", n = 2 }\n)");
    assert_eq!(parsed.raw.len(), 2);
    let keys = |b: &RawBlock| b.meta_data.iter().map(|f| f.key.clone()).collect::<Vec<_>>();
    assert_eq!(keys(&parsed.raw[0]), vec!["id".to_string()]);
    assert_eq!(keys(&parsed.raw[1]), vec!["id".to_string(), "n".to_string()]);
    assert_eq!(parsed.raw[1].meta_data[0].value, text("two", 8, 23));
}

#[test]
fn a_missing_value_becomes_an_error_value_under_continue_and_recover() {
    for strategy in ["continue", "recover"] {
        let parsed = parse_with(strategy, "@RAW(\n  meta_data -> { id = , kind = \"y\" }\n)");
        assert_eq!(parsed.raw.len(), 1, "{strategy}");
        let block = &parsed.raw[0];
        assert_eq!(
            block.meta_data,
            vec![
                field(
                    "id",
                    Value::Error { message: "Missing value for key 'id'".to_string(), position: pos(6, 18) },
                    6,
                    18
                ),
                field("kind", text("y", 6, 32), 6, 25),
            ],
            "{strategy}"
        );
        assert_eq!(block.position, pos(5, 5), "{strategy}");
        assert!(parsed.parse_errors.contains("Expected value for field 'id', found ,"), "{strategy}");
    }

    let last = parse_with("continue", "@RAW(\n  meta_data -> { id = }\n)");
    assert_eq!(
        last.raw[0].meta_data,
        vec![field(
            "id",
            Value::Error { message: "Missing value for key 'id'".to_string(), position: pos(6, 18) },
            6,
            18
        )]
    );
}

#[test]
fn halt_gives_up_on_a_malformed_block_and_reports_the_error() {
    let parsed = parse("@RAW(\n  meta_data -> { id = , kind = \"y\" }\n)");
    assert!(parsed.raw.is_empty());
    assert!(parsed.parse_errors.contains("Expected value for field 'id', found ,"));

    let brace = parse("@RAW(\n  meta_data -> id = \"x\" }\n)");
    assert!(brace.raw.is_empty());
    assert!(brace.parse_errors.contains("Expected '{' after 'meta_data ->'"));
}

#[test]
fn a_missing_brace_leaves_an_empty_block_under_continue_and_recover() {
    for strategy in ["continue", "recover"] {
        let parsed = parse_with(strategy, "@RAW(\n  meta_data -> id = \"x\" }\n)");
        assert_eq!(parsed.raw.len(), 1, "{strategy}");
        assert!(parsed.raw[0].meta_data.is_empty(), "{strategy}");
        assert!(parsed.parse_errors.contains("Expected '{' after 'meta_data ->'"), "{strategy}");
    }
}

#[test]
fn a_duplicate_meta_data_block_is_reported_and_the_later_one_wins() {
    let parsed = parse_with("continue", "@RAW(\n  meta_data -> { id = \"x\" }\n  meta_data -> { id = \"y\" }\n)");
    assert_eq!(parsed.raw.len(), 1);
    assert_eq!(parsed.raw[0].meta_data, vec![field("id", text("y", 7, 23), 7, 18)]);
    assert!(parsed.parse_errors.contains("Duplicate 'meta_data' block in the same @RAW section"));
}

#[test]
fn a_duplicate_content_block_is_reported_and_the_last_one_wins() {
    let parsed = parse("@RAW(\n  content -> {\n    ---a\none\n---a\n  }\n  content -> {\n    ---b\ntwo\n---b\n  }\n)");
    assert_eq!(parsed.raw.len(), 1);
    let block = &parsed.raw[0];
    assert_eq!(block.content.as_ref().expect("content").tag, "b");
    assert_eq!(payload(&parsed, block), "two\n");
    assert!(parsed.parse_errors.contains("Duplicate 'content' block in the same @RAW section"));
}

#[test]
fn an_unknown_key_is_reported_and_the_rest_still_parses() {
    let halted = parse("@RAW(\n  bogus -> { a = 1 }\n  meta_data -> { id = \"x\" }\n)");
    assert!(halted.raw.is_empty());
    assert!(halted.parse_errors.contains("Unrecognized RAW block key 'bogus'"));

    for strategy in ["continue", "recover"] {
        let parsed = parse_with(strategy, "@RAW(\n  bogus -> { a = 1 }\n  meta_data -> { id = \"x\" }\n)");
        assert_eq!(parsed.raw.len(), 1, "{strategy}");
        let keys: Vec<_> = parsed.raw[0].meta_data.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, vec!["id"], "{strategy}");
        assert!(parsed.parse_errors.contains("Unrecognized RAW block key 'bogus'"), "{strategy}");
    }
}

#[test]
fn an_unclosed_section_is_reported_and_continue_keeps_what_was_read() {
    let halted = parse("@RAW(\n  meta_data -> { id = \"x\" }\n");
    assert!(halted.raw.is_empty());
    assert!(halted.parse_errors.contains("Expected ')' to close RAW section"));

    let parsed = parse_with("continue", "@RAW(\n  meta_data -> { id = \"x\" }\n");
    assert_eq!(parsed.raw.len(), 1);
    assert_eq!(parsed.raw[0].meta_data.len(), 1);
    assert!(parsed.parse_errors.contains("Expected ')' to close RAW section"));
}

#[test]
fn a_content_that_is_not_a_raw_block_is_reported() {
    let parsed = parse_with("continue", "@RAW(\n  meta_data -> { id = \"x\" }\n  content -> \"oops\"\n)");
    assert_eq!(parsed.raw.len(), 1);
    assert_eq!(parsed.raw[0].meta_data.len(), 1);
    assert!(parsed.raw[0].content.is_none());
    assert!(parsed.parse_errors.contains("Expected a raw content block after 'content ->', found oops"));
}

#[test]
fn a_field_without_a_comma_is_reported_under_continue() {
    let parsed = parse_with("continue", "@RAW(\n  meta_data -> { id = \"x\" kind = \"y\" }\n)");
    assert_eq!(parsed.raw.len(), 1);
    assert!(parsed.parse_errors.contains("Expected ',' between fields in 'meta_data' block"));
}
