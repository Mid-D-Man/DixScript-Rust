// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/schema_parser_tests.rs"
// ============================================================================
//! Tests for the `@SCHEMA` section parser.
//!
//! `schema_section_tests.rs` covers the lexer, the analyzer and the builtin through the
//! loader. These tests pin the parser itself, which is the third parser built on the arena
//! AST: what a section parses to (including source positions), how the `Schema.<Method>(...)`
//! literals are read, what each error strategy does with malformed input, and the message
//! `GeneralParser` raises under the halt strategy. Every expectation was taken from the output
//! of the original, pre-arena parser.
//!
//! The helper runs the loader's first four stages (tokenize, split `@CONFIG`, process
//! `@CONFIG`, parse) with an isolated error manager, so the parse diagnostics can be read back.

use dixscript::Compiler::AST::data_types::CompatibilityMode;
use dixscript::Compiler::AST::{Position, SchemaBlock, TablePath, Value};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::GeneralParser;
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::ErrorManager::ErrorManager;

/// The parsed `@SCHEMA` section (if any) and the parse diagnostics as text.
struct Parsed {
    schema: Option<SchemaBlock>,
    parse_errors: String,
}

/// Runs the parse. `Err` carries the error the parser raised, which is what the halt strategy
/// does with a malformed `@SCHEMA`.
fn parse_source(source: String) -> Result<Parsed, String> {
    let label = "schema-parser-test";
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
        .map_err(|e| format!("{e:?}"))?;
    Ok(Parsed { schema: ast.schema, parse_errors: em.get_parse_errors_as_string() })
}

/// Parses `section` as a whole file under the default (halting) error strategy. The
/// three-line `@CONFIG` header puts the section's first line on line 4.
fn parse(section: &str) -> Result<Parsed, String> {
    parse_source(format!("@CONFIG(\n  features -> \"data,raw,schema\"\n)\n{section}"))
}

/// Parses `section` under `strategy` ("continue" or "recover"). The four-line `@CONFIG`
/// header puts the section's first line on line 5.
fn parse_with(strategy: &str, section: &str) -> Parsed {
    parse_source(format!(
        "@CONFIG(\n  features -> \"data,raw,schema\"\n  error_handling -> \"{strategy}\"\n)\n{section}"
    ))
    .expect("the parse returns an AST")
}

fn pos(line: usize, column: usize) -> Position {
    Position::new(line, column)
}

fn text(t: &str, line: usize, column: usize) -> Value {
    Value::String { value: t.to_string(), position: pos(line, column) }
}

fn boolean(v: bool, line: usize, column: usize) -> Value {
    Value::Boolean { value: v, position: pos(line, column) }
}

fn path(segments: &[&str]) -> TablePath {
    TablePath::new(segments.iter().map(|s| s.to_string()).collect())
}

/// The method names of a section's fields, in order.
fn methods(block: &SchemaBlock) -> Vec<&str> {
    block.fields.iter().map(|f| f.method.as_str()).collect()
}

#[test]
fn literal_arguments_of_every_kind_are_read() {
    let parsed = parse(
        "@SCHEMA(\n  a = Schema.String(true, \"x\", 'y', 1, 9000000000L, 1.5f, 2.5, 1e3, true, false)\n)",
    )
    .expect("a well formed schema parses");
    let block = parsed.schema.expect("the schema section is present");

    assert_eq!(block.position, pos(4, 8));
    assert_eq!(block.fields.len(), 1);
    let f = &block.fields[0];
    assert_eq!(f.path, path(&["a"]));
    assert_eq!(f.method, "String");
    assert_eq!(f.position, pos(5, 3));
    assert_eq!(
        f.arguments,
        vec![
            boolean(true, 5, 21),
            text("x", 5, 27),
            text("y", 5, 32),
            Value::Integer { value: 1, position: pos(5, 37) },
            Value::Long { value: 9_000_000_000, position: pos(5, 40) },
            Value::Float { value: 1.5, position: pos(5, 53) },
            Value::Double { value: 2.5, position: pos(5, 59) },
            Value::ScientificNotation { value: 1000.0, position: pos(5, 64) },
            boolean(true, 5, 69),
            boolean(false, 5, 75),
        ]
    );
    assert_eq!(parsed.parse_errors, "");
}

#[test]
fn negative_numbers_are_single_literals() {
    let parsed = parse("@SCHEMA(\n  a = Schema.Int(true, -5, -1)\n  b = Schema.Double(false, -2.5)\n)").expect("parses");
    let block = parsed.schema.expect("schema");
    assert_eq!(methods(&block), vec!["Int", "Double"]);
    assert_eq!(
        block.fields[0].arguments,
        vec![
            boolean(true, 5, 18),
            Value::Integer { value: -5, position: pos(5, 24) },
            Value::Integer { value: -1, position: pos(5, 28) },
        ]
    );
    assert_eq!(
        block.fields[1].arguments,
        vec![boolean(false, 6, 21), Value::Double { value: -2.5, position: pos(6, 28) }]
    );
    assert_eq!(block.fields[1].position, pos(6, 3));
}

#[test]
fn an_empty_argument_list_and_an_empty_section_are_allowed() {
    let one = parse("@SCHEMA(\n  a = Schema.Int()\n)").expect("parses").schema.expect("schema");
    assert_eq!(one.fields.len(), 1);
    assert!(one.fields[0].arguments.is_empty());
    assert_eq!(one.fields[0].position, pos(5, 3));

    let none = parse("@SCHEMA()").expect("parses").schema.expect("schema");
    assert!(none.fields.is_empty());
    assert_eq!(none.position, pos(4, 8));
}

#[test]
fn dotted_paths_split_into_segments_and_keywords_are_valid_segments() {
    let block = parse(
        "@SCHEMA(\n  database.primary.host = Schema.String(true)\n  database.primary.port = Schema.Int(false)\n)",
    )
    .expect("parses")
    .schema
    .expect("schema");
    assert_eq!(block.fields[0].path, path(&["database", "primary", "host"]));
    assert_eq!(block.fields[1].path, path(&["database", "primary", "port"]));
    assert_eq!(methods(&block), vec!["String", "Int"]);
    assert!(block.fields.iter().all(|f| f.arguments.len() == 1));

    let keywords = parse("@SCHEMA(\n  string.date = Schema.Int(true)\n)").expect("parses").schema.expect("schema");
    assert_eq!(keywords.fields[0].path, path(&["string", "date"]));
    assert_eq!(keywords.fields[0].arguments, vec![boolean(true, 5, 28)]);
}

#[test]
fn commas_between_fields_are_optional() {
    let block = parse("@SCHEMA(\n  a = Schema.Int(true),\n  b = Schema.Bool(false),\n)")
        .expect("parses")
        .schema
        .expect("schema");
    assert_eq!(methods(&block), vec!["Int", "Bool"]);
}

#[test]
fn many_fields_keep_their_order() {
    let mut section = String::from("@SCHEMA(\n");
    for name in ["a", "b", "c", "d", "e", "f", "g", "h", "i"] {
        section.push_str(&format!("  {name} = Schema.Int(true)\n"));
    }
    section.push_str("  j = Schema.Int(true, 1, 2)\n)");
    let block = parse(&section).expect("parses").schema.expect("schema");
    let names: Vec<String> = block.fields.iter().map(|f| f.path.segments.join(".")).collect();
    assert_eq!(names, vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]);
    assert_eq!(block.fields[9].arguments.len(), 3);
}

#[test]
fn halt_refuses_a_malformed_schema_with_the_first_error() {
    let cases: [(&str, &str); 11] = [
        (
            "a = Schema.int(true)\n  b = Schema.Bool(false)",
            "@SCHEMA is malformed: Schema type names are capitalised: write 'Schema.Int(...)', not 'Schema.int(...)' (line 5, column 14)",
        ),
        (
            "a Schema.Int(true)\n  b = Schema.Bool(false)",
            "@SCHEMA is malformed: Expected '=' after schema path 'a' (line 5, column 5)",
        ),
        (
            "a = Int(true)\n  b = Schema.Bool(false)",
            "@SCHEMA is malformed: Expected 'Schema.<Type>(required, ...)' for 'a', found Int (line 5, column 7)",
        ),
        (
            "a = Schema Int(true)\n  b = Schema.Bool(false)",
            "@SCHEMA is malformed: Expected '.' after 'Schema' in the descriptor for 'a' (line 5, column 14)",
        ),
        (
            "a = Schema.(true)\n  b = Schema.Bool(false)",
            "@SCHEMA is malformed: Expected a schema type name after 'Schema.' for 'a', found ( (line 5, column 14)",
        ),
        (
            "a = Schema.Int(true, foo)\n  b = Schema.Bool(false)",
            "@SCHEMA is malformed: Expected a literal argument (bool, number or string) in 'Schema.Int(...)', found foo (line 5, column 24)",
        ),
        (
            "a = Schema.Int(true 5)\n  b = Schema.Bool(false)",
            "@SCHEMA is malformed: Expected ',' or ')' in 'Schema.Int(...)' arguments, found 5 (line 5, column 23)",
        ),
        (
            "a = Schema.Int(true, 5\n  b = Schema.Bool(false)",
            "@SCHEMA is malformed: Expected ',' or ')' in 'Schema.Int(...)' arguments, found b (line 6, column 3)",
        ),
        (
            "a = Schema.Int(true)\n",
            "@SCHEMA is malformed: Expected ')' to close SCHEMA section (line 5, column 23)",
        ),
        (
            "5 = Schema.Int(true)\n  b = Schema.Bool(false)",
            "@SCHEMA is malformed: Expected a @DATA path (e.g. 'name' or 'table.key') at the start of a SCHEMA field (line 5, column 3)",
        ),
        (
            "a. = Schema.Int(true)\n  b = Schema.Bool(false)",
            "@SCHEMA is malformed: Expected an identifier after '.' in a SCHEMA path (line 5, column 6)",
        ),
    ];
    for (body, expected) in cases {
        let section = if body.ends_with('\n') {
            format!("@SCHEMA(\n  {body}")
        } else {
            format!("@SCHEMA(\n  {body}\n)")
        };
        let err = parse(&section).err().unwrap_or_else(|| panic!("halt should refuse: {body}"));
        assert!(err.contains(expected), "for {body:?}\nexpected: {expected}\nactual: {err}");
    }
}

#[test]
fn continue_and_recover_read_what_they_can_after_a_bad_field() {
    let section = "@SCHEMA(\n  a = Schema.int(true)\n  b = Schema.Bool(false)\n)";

    let continued = parse_with("continue", section);
    assert!(continued.schema.expect("schema").fields.is_empty());
    assert!(continued
        .parse_errors
        .contains("Schema type names are capitalised: write 'Schema.Int(...)', not 'Schema.int(...)'"));

    let recovered = parse_with("recover", section);
    let block = recovered.schema.expect("schema");
    assert_eq!(methods(&block), vec!["Bool"]);
    assert_eq!(block.fields[0].path, path(&["b"]));
    assert_eq!(block.fields[0].arguments, vec![boolean(false, 7, 19)]);
}

#[test]
fn a_bad_argument_drops_the_field_under_continue_and_recover() {
    for strategy in ["continue", "recover"] {
        let parsed = parse_with(strategy, "@SCHEMA(\n  a = Schema.Int(true, foo)\n  b = Schema.Bool(false)\n)");
        assert!(parsed.schema.expect("schema").fields.is_empty(), "{strategy}");
        assert!(
            parsed.parse_errors.contains(
                "Expected a literal argument (bool, number or string) in 'Schema.Int(...)', found foo"
            ),
            "{strategy}"
        );
    }
}

#[test]
fn an_unclosed_section_keeps_its_fields_under_continue_and_recover() {
    for strategy in ["continue", "recover"] {
        let parsed = parse_with(strategy, "@SCHEMA(\n  a = Schema.Int(true)\n");
        assert!(parsed.parse_errors.contains("Expected ')' to close SCHEMA section"), "{strategy}");
        let block = parsed.schema.expect("schema");
        assert_eq!(block.position, pos(5, 8), "{strategy}");
        assert_eq!(methods(&block), vec!["Int"], "{strategy}");
        assert_eq!(block.fields[0].arguments, vec![boolean(true, 6, 18)], "{strategy}");
    }
}
