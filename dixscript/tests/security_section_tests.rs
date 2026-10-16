// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/security_section_tests.rs"
// ============================================================================
//! Tests for the `@SECURITY` section parser.
//!
//! The parser had no tests of its own, and it is the first parser built on the arena AST, so
//! these tests pin its behaviour: what a section parses to (including source positions), what
//! the error strategies do with malformed input, and which diagnostics it reports. Every
//! expectation was taken from the original, pre-arena parser's output.
//!
//! The helper runs the loader's first four stages (tokenize, split `@CONFIG`, process
//! `@CONFIG`, parse) with an isolated error manager, so the parse diagnostics can be read back.

use dixscript::Compiler::AST::data_types::CompatibilityMode;
use dixscript::Compiler::AST::{Position, SecurityEntry, SecurityField, SecuritySection, Value};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::GeneralParser;
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::ErrorManager::ErrorManager;
use std::path::{Path, PathBuf};

/// The parsed `@SECURITY` section (if any) and the parse diagnostics as text.
struct Parsed {
    security: Option<SecuritySection>,
    parse_errors: String,
}

fn parse_bytes(label: &str, bytes: &[u8]) -> Parsed {
    let em = ErrorManager::new_isolated_silent();
    let initial = OperationalSettings {
        source_file_path: Some(label.to_string()),
        compatibility_mode: CompatibilityMode::BestEffort,
        ..OperationalSettings::default()
    };
    let tokens = Tokenizer::new_with_error_manager_from_bytes(bytes, &initial, em.clone()).tokenize();
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
    Parsed { security: ast.security, parse_errors: em.get_parse_errors_as_string() }
}

/// Parses `section` as a whole file under the default (halting) error strategy.
fn parse(section: &str) -> Parsed {
    parse_bytes("security-test", section.as_bytes())
}

/// Parses `section` under `strategy` ("continue" or "recover"). The three-line `@CONFIG`
/// header shifts every source line by three.
fn parse_with(strategy: &str, section: &str) -> Parsed {
    let source = format!("@CONFIG(\n  error_handling -> \"{strategy}\"\n)\n{section}");
    parse_bytes("security-test", source.as_bytes())
}

fn pos(line: usize, column: usize) -> Position {
    Position::new(line, column)
}

fn text(t: &str, line: usize, column: usize) -> Value {
    Value::String { value: t.to_string(), position: pos(line, column) }
}

fn int(v: i32, line: usize, column: usize) -> Value {
    Value::Integer { value: v, position: pos(line, column) }
}

fn missing(key: &str, line: usize, column: usize) -> Value {
    Value::Error { message: format!("Missing value for key '{key}'"), position: pos(line, column) }
}

#[test]
fn a_plain_section_parses_with_exact_positions() {
    let parsed = parse("@SECURITY(\n  encryption -> { mode = \"password\" }\n)\n");
    let expected = SecuritySection::new(
        vec![SecurityEntry::new(
            "encryption".into(),
            vec![SecurityField::new("mode".into(), text("password", 2, 26), pos(2, 19))],
            pos(2, 3),
        )],
        pos(1, 10),
    );
    assert_eq!(parsed.security, Some(expected));
    assert!(parsed.parse_errors.is_empty(), "unexpected diagnostics: {}", parsed.parse_errors);
}

#[test]
fn every_scalar_value_kind_parses() {
    let parsed = parse(
        "@SECURITY(\n  encryption -> { mode = \"keyfile\", iterations = 100000, flag = true, off = false, salt = #FFAA00, name = 'single', big = 5000000000L, f = 1.5f, d = 2.5, nothing = null, k = auto }\n  validation -> { level = \"strict\" }\n)\n",
    );
    let section = parsed.security.expect("a section");
    assert_eq!(section.entries.len(), 2);

    let shown: Vec<(String, String)> = section.entries[0]
        .fields
        .iter()
        .map(|f| (f.key.clone(), f.value.to_string()))
        .collect();
    let expected: Vec<(&str, &str)> = vec![
        ("mode", "\"keyfile\""),
        ("iterations", "100000"),
        ("flag", "true"),
        ("off", "false"),
        ("salt", "#FFAA00"),
        ("name", "\"single\""),
        ("big", "5000000000L"),
        ("f", "1.5f"),
        ("d", "2.5"),
        ("nothing", "null"),
        ("k", "\"auto\""),
    ];
    let expected: Vec<(String, String)> = expected.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    assert_eq!(shown, expected);

    // The value kinds, not only their text.
    let kinds: Vec<&str> = section.entries[0]
        .fields
        .iter()
        .map(|f| match f.value {
            Value::String { .. } => "string",
            Value::Integer { .. } => "integer",
            Value::Boolean { .. } => "boolean",
            Value::HexColor { .. } => "hex",
            Value::Long { .. } => "long",
            Value::Float { .. } => "float",
            Value::Double { .. } => "double",
            Value::Null { .. } => "null",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        ["string", "integer", "boolean", "boolean", "hex", "string", "long", "float", "double", "null", "string"]
    );
    assert_eq!(section.entries[1].block_key, "validation");
    assert_eq!(section.entries[1].fields[0].value, text("strict", 3, 27));
}

#[test]
fn commas_between_blocks_are_optional_and_a_trailing_field_comma_is_accepted() {
    for source in [
        "@SECURITY(\n  encryption -> { a = 1, b = 2 },\n  validation -> { c = 3 }\n)\n",
        "@SECURITY(\n  encryption -> { a = 1, b = 2, }\n  validation -> { c = 3 }\n)\n",
    ] {
        let parsed = parse(source);
        let section = parsed.security.expect("a section");
        let shape: Vec<(String, Vec<String>)> = section
            .entries
            .iter()
            .map(|e| (e.block_key.clone(), e.fields.iter().map(|f| f.key.clone()).collect()))
            .collect();
        assert_eq!(
            shape,
            vec![
                ("encryption".to_string(), vec!["a".to_string(), "b".to_string()]),
                ("validation".to_string(), vec!["c".to_string()]),
            ],
            "{source:?}"
        );
        assert!(parsed.parse_errors.is_empty(), "{source:?}: {}", parsed.parse_errors);
    }
}

#[test]
fn an_empty_section_and_an_empty_block_parse() {
    let empty = parse("@SECURITY()\n").security.expect("a section");
    assert!(empty.entries.is_empty());
    assert_eq!(empty.position, pos(1, 10));

    let block = parse("@SECURITY(\n  encryption -> { }\n)\n").security.expect("a section");
    assert_eq!(block.entries.len(), 1);
    assert!(block.entries[0].fields.is_empty());
}

#[test]
fn the_parser_does_not_validate_block_names() {
    // An unknown block name is accepted here; rejecting it is the semantic analyzer's job.
    let section = parse("@SECURITY(\n  bogus -> { a = 1 }\n)\n").security.expect("a section");
    assert_eq!(section.entries[0].block_key, "bogus");
}

#[test]
fn malformed_input_gives_no_section_under_halt_and_reports_why() {
    let cases: [(&str, &str, &str); 7] = [
        ("missing value", "@SECURITY(\n  encryption -> { mode = }\n)\n", "Expected value for field 'mode'"),
        ("unclosed brace", "@SECURITY(\n  encryption -> { mode = \"x\" \n)\n", "Expected ',' between fields in security block 'encryption'"),
        ("missing equals", "@SECURITY(\n  encryption -> { mode \"x\" }\n)\n", "Expected '=' after field key 'mode'"),
        ("missing arrow", "@SECURITY(\n  encryption { a = 1 }\n)\n", "Expected '->' after security block"),
        ("array value", "@SECURITY(\n  encryption -> { a = [1, 2] }\n)\n", "Expected value for field 'a'"),
        ("identifier value", "@SECURITY(\n  encryption -> { a = foo }\n)\n", "Expected value for field 'a'"),
        ("keyword as key", "@SECURITY(\n  encryption -> { true = 1 }\n)\n", "Expected field key identifier"),
    ];
    for (label, source, message) in cases {
        let parsed = parse(source);
        assert_eq!(parsed.security, None, "{label}: halt keeps no section");
        assert!(parsed.parse_errors.contains(message), "{label}: expected {message:?} in {}", parsed.parse_errors);
    }
}

#[test]
fn a_missing_value_becomes_an_error_value_when_parsing_continues() {
    let source = "@SECURITY(\n  encryption -> { mode = , level = 2 }\n  validation -> { a = 1 }\n)\n";
    assert_eq!(parse(source).security, None, "halt keeps nothing");

    for strategy in ["continue", "recover"] {
        let parsed = parse_with(strategy, source);
        let expected = SecuritySection::new(
            vec![
                SecurityEntry::new(
                    "encryption".into(),
                    vec![
                        SecurityField::new("mode".into(), missing("mode", 5, 19), pos(5, 19)),
                        SecurityField::new("level".into(), int(2, 5, 36), pos(5, 28)),
                    ],
                    pos(5, 3),
                ),
                SecurityEntry::new(
                    "validation".into(),
                    vec![SecurityField::new("a".into(), int(1, 6, 23), pos(6, 19))],
                    pos(6, 3),
                ),
            ],
            pos(4, 10),
        );
        assert_eq!(parsed.security, Some(expected), "{strategy}");
        assert!(parsed.parse_errors.contains("Expected value for field 'mode'"), "{strategy}");
    }
}

#[test]
fn recovery_skips_to_the_next_good_entry() {
    let parsed = parse_with("recover", "@SECURITY(\n  bogus junk -> -> { }\n  encryption -> { a = 1 }\n)\n");
    let expected = SecuritySection::new(
        vec![
            SecurityEntry::new("bogus".into(), vec![], pos(5, 3)),
            SecurityEntry::new(
                "encryption".into(),
                vec![SecurityField::new("a".into(), int(1, 6, 23), pos(6, 19))],
                pos(6, 3),
            ),
        ],
        pos(4, 10),
    );
    assert_eq!(parsed.security, Some(expected));
    assert!(!parsed.parse_errors.is_empty(), "the skipped tokens are reported");
}

#[test]
fn recovery_keeps_every_field_around_two_bad_values() {
    let parsed = parse_with("recover", "@SECURITY(\n  encryption -> { a = foo, b = [1], c = 3 }\n)\n");
    let section = parsed.security.expect("a section");
    let fields = &section.entries[0].fields;
    // The bad value `foo` is not consumed, so it is read as the next field's key; the shape
    // below is exactly what the original parser produced.
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0], SecurityField::new("a".into(), missing("a", 5, 19), pos(5, 19)));
    assert_eq!(fields[1], SecurityField::new("foo".into(), missing("foo", 5, 23), pos(5, 23)));
    assert_eq!(fields[2], SecurityField::new("c".into(), int(3, 5, 41), pos(5, 37)));
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().map_or(false, |x| x == "mdix") {
            out.push(path);
        }
    }
}

#[test]
fn every_security_section_in_the_corpus_parses_without_error_values() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("mdix_files");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();

    let mut with_security = 0;
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        if !String::from_utf8_lossy(&bytes).contains("@SECURITY(") {
            continue;
        }
        let parsed = parse_bytes(&path.display().to_string(), &bytes);
        let Some(section) = parsed.security else { continue };
        with_security += 1;
        assert!(!section.entries.is_empty(), "{}: no entries", path.display());
        for field in section.entries.iter().flat_map(|e| e.fields.iter()) {
            assert!(
                !matches!(field.value, Value::Error { .. }),
                "{}: error value for '{}'",
                path.display(),
                field.key
            );
        }
    }
    assert!(with_security >= 10, "expected the corpus to hold @SECURITY sections, found {with_security}");
}
