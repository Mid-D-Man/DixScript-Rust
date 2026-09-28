// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md (referenced from the Compiler modules it
// tests), section "Compiler/Core/SectionAnalyzers/schema_section_analyzer.rs"
// ============================================================================
//! Tests for the `@SCHEMA` section: lexer token recognition, parsing into
//! `SchemaBlock`, `schema_section_analyzer.rs`'s descriptor validation and
//! data checks, the post-resolution pass in `DixLoader`, and feature gating.
//!
//! Every source string here uses ONLY syntax already exercised elsewhere in
//! this crate's tests (simple `name = value` properties, `a.b: k = v` tables,
//! `name:: a, b` group arrays, `name<enum> = Enum.FIELD`, `~fn<type>(..) {..}`
//! QuickFuncs) -- the grammar was checked against `runtime_tests.rs` fixtures
//! BEFORE writing cases, not after, because `raw_section_tests.rs` once baked
//! one wrong delimiter assumption into eleven tests.
//!
//! `DixLoader::new_silent()` throughout: `DixLoader::new()` logs every line
//! through unbuffered `eprintln!`, which is noise in test output.
//!
//! Run with:
//!   cargo test --test schema_section_tests -- --nocapture

use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::Config::OperationalSettings;
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Runtime::DixLoader;

fn compile(source: &str, label: &str) -> Result<DixScript, String> {
    DixLoader::new_silent().compile_to_resolved_ast_from_str(source, label)
}

fn expect_err(source: &str, label: &str) -> String {
    match compile(source, label) {
        Ok(_)  => panic!("[{}] expected a compile error but compilation succeeded", label),
        Err(e) => e,
    }
}

// ==================== Lexer ====================

#[test]
fn schema_section_keyword_is_recognized() {
    let settings = OperationalSettings::default();
    let result = Tokenizer::new("@SCHEMA( port = Schema.Int(true) )", &settings).tokenize();
    assert!(
        result.tokens.iter().any(|t| matches!(t.token_type, TokenType::SectionSchema)),
        "expected a SectionSchema token, got: {:?}",
        result.tokens.iter().map(|t| &t.token_type).collect::<Vec<_>>()
    );
}

#[test]
fn schema_tokens_carry_the_schema_section_id() {
    let settings = OperationalSettings::default();
    let result = Tokenizer::new("@SCHEMA( port = Schema.Int(true) )", &settings).tokenize();
    let ident = result.tokens.iter()
        .find(|t| matches!(&t.token_type, TokenType::Identifier(id) if id == "port"))
        .expect("expected an Identifier(port) token");
    assert_eq!(ident.section, SectionId::Schema);
}

#[test]
fn capitalised_type_names_lex_as_plain_identifiers() {
    // The whole reason the Schema methods are capitalised: `Int` is an
    // Identifier (accepted after '.'), `int` is a Keyword (not accepted).
    let settings = OperationalSettings::default();

    let upper = Tokenizer::new("@SCHEMA( a = Schema.Int(true) )", &settings).tokenize();
    assert!(upper.tokens.iter().any(|t| matches!(&t.token_type, TokenType::Identifier(id) if id == "Int")));
    assert!(!upper.tokens.iter().any(|t| matches!(&t.token_type, TokenType::Keyword("int"))));

    let lower = Tokenizer::new("@SCHEMA( a = Schema.int(true) )", &settings).tokenize();
    assert!(lower.tokens.iter().any(|t| matches!(&t.token_type, TokenType::Keyword("int"))));
}

// ==================== Parsing ====================

#[test]
fn parses_flat_and_dotted_paths() {
    let src = r#"
@DATA(
  port = 8080
  database.primary: host = "db.local", port = 5432
)
@SCHEMA(
  port = Schema.Int(true, 1, 65535)
  database.primary.host = Schema.String(true)
  database.primary.port = Schema.Int(false)
)
"#;
    let ast = compile(src, "schema-parse-paths").expect("should compile");
    let schema = ast.schema.expect("schema section should be present");
    assert_eq!(schema.fields.len(), 3);

    assert_eq!(schema.fields[0].path.segments, vec!["port".to_string()]);
    assert_eq!(schema.fields[0].method, "Int");
    assert_eq!(schema.fields[0].arguments.len(), 3);

    assert_eq!(
        schema.fields[1].path.segments,
        vec!["database".to_string(), "primary".to_string(), "host".to_string()]
    );
    assert_eq!(schema.fields[1].method, "String");
}

#[test]
fn commas_between_fields_are_optional() {
    let src = r#"
@DATA(
  a = 1
  b = 2
)
@SCHEMA( a = Schema.Int(true), b = Schema.Int(true) )
"#;
    let ast = compile(src, "schema-commas").expect("should compile");
    assert_eq!(ast.schema.unwrap().fields.len(), 2);
}

#[test]
fn keyword_named_path_segments_are_accepted() {
    // A @DATA property may be called `string` or `date`; the position in a
    // schema path is unambiguous, so keywords are fine there.
    let src = r#"
@DATA( date = "2026-01-01" )
@SCHEMA( date = Schema.String(true) )
"#;
    let ast = compile(src, "schema-keyword-path").expect("should compile");
    assert_eq!(ast.schema.unwrap().fields[0].path.segments, vec!["date".to_string()]);
}

#[test]
fn negative_bounds_parse() {
    let src = r#"
@DATA( temp = -5 )
@SCHEMA( temp = Schema.Int(true, -40, 60) )
"#;
    compile(src, "schema-negative-bounds").expect("temp = -5 is within -40..60");
}

#[test]
fn lowercase_type_name_is_rejected_with_a_hint() {
    let src = r#"
@DATA( port = 1 )
@SCHEMA( port = Schema.int(true) )
"#;
    let err = expect_err(src, "schema-lowercase");
    assert!(err.contains("capitalised"), "error should explain the capitalisation rule: {}", err);
    assert!(err.contains("Schema.Int"), "error should suggest the corrected spelling: {}", err);
}

#[test]
fn malformed_schema_does_not_silently_disable_validation() {
    // Missing '=' -- the section must fail the compile, not vanish (an
    // absent schema would mean no validation at all).
    let src = r#"
@DATA( port = 1 )
@SCHEMA( port Schema.Int(true) )
"#;
    let err = expect_err(src, "schema-malformed");
    assert!(err.contains("@SCHEMA is malformed"), "got: {}", err);
}

#[test]
fn non_literal_argument_is_rejected() {
    let src = r#"
@DATA( port = 1 )
@SCHEMA( port = Schema.Int(true, someVar) )
"#;
    let err = expect_err(src, "schema-nonliteral-arg");
    assert!(err.contains("literal argument"), "got: {}", err);
}

// ==================== Descriptor validation ====================

#[test]
fn unknown_type_name_is_a_descriptor_error() {
    let src = r#"
@DATA( port = 1 )
@SCHEMA( port = Schema.Bogus(true) )
"#;
    let err = expect_err(src, "schema-unknown-type");
    assert!(err.contains("Invalid descriptor"), "got: {}", err);
}

#[test]
fn required_must_be_a_bool() {
    let src = r#"
@DATA( port = 1 )
@SCHEMA( port = Schema.Int(1) )
"#;
    let err = expect_err(src, "schema-required-not-bool");
    assert!(err.contains("Invalid descriptor"), "got: {}", err);
}

#[test]
fn missing_arguments_is_a_descriptor_error() {
    let src = r#"
@DATA( port = 1 )
@SCHEMA( port = Schema.Int() )
"#;
    let err = expect_err(src, "schema-no-args");
    assert!(err.contains("Invalid descriptor"), "got: {}", err);
}

#[test]
fn duplicate_paths_are_rejected() {
    let src = r#"
@DATA( port = 1 )
@SCHEMA(
  port = Schema.Int(true)
  port = Schema.Int(false)
)
"#;
    let err = expect_err(src, "schema-duplicate-path");
    assert!(err.contains("more than once"), "got: {}", err);
}

#[test]
fn empty_schema_compiles() {
    let src = "@DATA( port = 1 )\n@SCHEMA()\n";
    let ast = compile(src, "schema-empty").expect("an empty @SCHEMA is a warning, not an error");
    assert!(ast.schema.unwrap().fields.is_empty());
}

// ==================== Data validation: presence ====================

#[test]
fn valid_data_passes() {
    let src = r#"
@DATA(
  app_name = "Demo"
  port = 8080
  debug = false
)
@SCHEMA(
  app_name = Schema.String(true, 1, 32)
  port     = Schema.Int(true, 1, 65535)
  debug    = Schema.Bool(true)
)
"#;
    compile(src, "schema-valid").expect("all three fields satisfy their descriptors");
}

#[test]
fn missing_required_field_fails() {
    let src = r#"
@DATA( port = 8080 )
@SCHEMA(
  port     = Schema.Int(true)
  app_name = Schema.String(true)
)
"#;
    let err = expect_err(src, "schema-missing-required");
    assert!(err.contains("Required @DATA field 'app_name' is missing"), "got: {}", err);
}

#[test]
fn missing_optional_field_passes() {
    let src = r#"
@DATA( port = 8080 )
@SCHEMA(
  port     = Schema.Int(true)
  app_name = Schema.String(false)
)
"#;
    compile(src, "schema-missing-optional").expect("an optional field may be absent");
}

#[test]
fn schema_without_data_fails_on_required_fields() {
    let src = "@SCHEMA( port = Schema.Int(true) )\n";
    let err = expect_err(src, "schema-no-data");
    assert!(err.contains("Required @DATA field 'port' is missing"), "got: {}", err);
}

// ==================== Data validation: types ====================

#[test]
fn wrong_type_fails() {
    let src = r#"
@DATA( port = "8080" )
@SCHEMA( port = Schema.Int(true) )
"#;
    let err = expect_err(src, "schema-wrong-type");
    assert!(err.contains("is string, but @SCHEMA expects int"), "got: {}", err);
}

#[test]
fn int_schema_rejects_a_fractional_value() {
    let src = r#"
@DATA( ratio = 1.5 )
@SCHEMA( ratio = Schema.Int(true) )
"#;
    let err = expect_err(src, "schema-int-vs-double");
    assert!(err.contains("expects int"), "got: {}", err);
}

#[test]
fn double_schema_accepts_integer_and_fractional_values() {
    // Widening only: a bare `1` and a bare `1.5` both satisfy Schema.Double.
    let src = r#"
@DATA(
  a = 1
  b = 1.5
)
@SCHEMA( a = Schema.Double(true), b = Schema.Double(true) )
"#;
    compile(src, "schema-double-widening").expect("integers and doubles both satisfy Double");
}

#[test]
fn long_schema_accepts_an_integer() {
    let src = r#"
@DATA( big = 5 )
@SCHEMA( big = Schema.Long(true) )
"#;
    compile(src, "schema-long-widening").expect("an int literal widens to long");
}

#[test]
fn consistent_type_annotation_passes() {
    let src = r#"
@DATA( count<int> = 5 )
@SCHEMA( count = Schema.Int(true) )
"#;
    compile(src, "schema-annotation-consistent").expect("an <int> annotation on an int value satisfies Schema.Int");
}

// ==================== Data validation: constraints ====================

#[test]
fn numeric_below_minimum_fails() {
    let src = r#"
@DATA( port = 0 )
@SCHEMA( port = Schema.Int(true, 1, 65535) )
"#;
    let err = expect_err(src, "schema-below-min");
    assert!(err.contains("below the minimum"), "got: {}", err);
}

#[test]
fn numeric_above_maximum_fails() {
    let src = r#"
@DATA( port = 70000 )
@SCHEMA( port = Schema.Int(true, 1, 65535) )
"#;
    let err = expect_err(src, "schema-above-max");
    assert!(err.contains("above the maximum"), "got: {}", err);
}

#[test]
fn numeric_bounds_are_inclusive() {
    let src = r#"
@DATA(
  low = 1
  high = 65535
)
@SCHEMA(
  low  = Schema.Int(true, 1, 65535)
  high = Schema.Int(true, 1, 65535)
)
"#;
    compile(src, "schema-inclusive-bounds").expect("min and max themselves are valid");
}

#[test]
fn string_too_short_fails() {
    let src = r#"
@DATA( name = "" )
@SCHEMA( name = Schema.String(true, 1, 10) )
"#;
    let err = expect_err(src, "schema-string-short");
    assert!(err.contains("below the minimum 1"), "got: {}", err);
}

#[test]
fn string_too_long_fails() {
    let src = r#"
@DATA( name = "this is far too long" )
@SCHEMA( name = Schema.String(true, 1, 10) )
"#;
    let err = expect_err(src, "schema-string-long");
    assert!(err.contains("above the maximum 10"), "got: {}", err);
}

#[test]
fn array_item_count_is_checked() {
    let too_few = r#"
@DATA( tags = [] )
@SCHEMA( tags = Schema.Array(true, 1, 3) )
"#;
    let err = expect_err(too_few, "schema-array-few");
    assert!(err.contains("below the minimum 1"), "got: {}", err);

    let too_many = r#"
@DATA( tags = [1, 2, 3, 4] )
@SCHEMA( tags = Schema.Array(true, 1, 3) )
"#;
    let err = expect_err(too_many, "schema-array-many");
    assert!(err.contains("above the maximum 3"), "got: {}", err);

    let just_right = r#"
@DATA( tags = [1, 2] )
@SCHEMA( tags = Schema.Array(true, 1, 3) )
"#;
    compile(just_right, "schema-array-ok").expect("two items is within 1..3");
}

// ==================== Data validation: path forms ====================

#[test]
fn table_properties_are_reachable_by_dotted_path() {
    let src = r#"
@DATA(
  database.primary: host = "db.local", port = 5432
)
@SCHEMA(
  database.primary.host = Schema.String(true)
  database.primary.port = Schema.Int(true, 1, 65535)
)
"#;
    compile(src, "schema-table-path").expect("table members are addressable");
}

#[test]
fn table_property_violation_is_reported() {
    let src = r#"
@DATA(
  database.primary: host = "db.local", port = 99999
)
@SCHEMA(
  database.primary.port = Schema.Int(true, 1, 65535)
)
"#;
    let err = expect_err(src, "schema-table-violation");
    assert!(err.contains("database.primary.port"), "message should name the full path: {}", err);
    assert!(err.contains("above the maximum"), "got: {}", err);
}

#[test]
fn missing_table_member_is_missing() {
    let src = r#"
@DATA(
  database.primary: host = "db.local"
)
@SCHEMA(
  database.primary.port = Schema.Int(true)
)
"#;
    let err = expect_err(src, "schema-table-member-missing");
    assert!(err.contains("Required @DATA field 'database.primary.port' is missing"), "got: {}", err);
}

#[test]
fn group_array_is_addressable_and_counted() {
    let src = r#"
@DATA(
  allowed_origins:: "https://a.example.com", "https://b.example.com"
)
@SCHEMA(
  allowed_origins = Schema.Array(true, 1, 5)
)
"#;
    compile(src, "schema-group-array").expect("two group items is within 1..5");

    let too_few = r#"
@DATA(
  allowed_origins:: "https://a.example.com"
)
@SCHEMA(
  allowed_origins = Schema.Array(true, 2, 5)
)
"#;
    let err = expect_err(too_few, "schema-group-array-few");
    assert!(err.contains("below the minimum 2"), "got: {}", err);
}

#[test]
fn object_schema_accepts_a_table_prefix() {
    // `cache` is never assigned directly -- it only exists as the prefix of
    // the `cache.redis` table -- and still counts as a present object.
    let src = r#"
@DATA(
  cache.redis: host = "cache.local", ttl = 3600
)
@SCHEMA(
  cache = Schema.Object(true)
)
"#;
    compile(src, "schema-object-presence").expect("a table prefix counts as a present object");
}

// ==================== Data validation: enums ====================

#[test]
fn enum_of_the_expected_type_passes() {
    let src = r#"
@ENUMS(
  LogLevel { DEBUG = 0, INFO = 1 }
  Environment { DEV = 1, PROD = 3 }
)
@DATA(
  log_level<enum> = LogLevel.INFO
)
@SCHEMA(
  log_level = Schema.Enum(true, "LogLevel")
)
"#;
    compile(src, "schema-enum-ok").expect("LogLevel.INFO is a LogLevel");
}

#[test]
fn enum_of_a_different_type_fails() {
    let src = r#"
@ENUMS(
  LogLevel { DEBUG = 0, INFO = 1 }
  Environment { DEV = 1, PROD = 3 }
)
@DATA(
  log_level<enum> = Environment.PROD
)
@SCHEMA(
  log_level = Schema.Enum(true, "LogLevel")
)
"#;
    let err = expect_err(src, "schema-enum-wrong");
    assert!(err.contains("expects enum 'LogLevel'"), "got: {}", err);
}

// ==================== Post-resolution validation ====================

#[test]
fn quickfunc_computed_value_is_checked_after_resolution() {
    // `answer` is 42 only once the QuickFunc runs. The semantic phase can't
    // see that (the value is a call, not a literal), so it must NOT fail
    // there -- and Stage 9 must catch the violation afterwards.
    let src = r#"
@QUICKFUNCS(
  ~double_it<int>(n) {
    return n * 2
  }
)
@DATA(
  answer = double_it(21)
)
@SCHEMA(
  answer = Schema.Int(true, 0, 10)
)
"#;
    let err = expect_err(src, "schema-post-resolution-fail");
    assert!(err.contains("Schema validation failed"), "should fail in the post-resolution stage: {}", err);
    assert!(err.contains("above the maximum"), "got: {}", err);
}

#[test]
fn quickfunc_computed_value_within_bounds_passes() {
    let src = r#"
@QUICKFUNCS(
  ~double_it<int>(n) {
    return n * 2
  }
)
@DATA(
  answer = double_it(21)
)
@SCHEMA(
  answer = Schema.Int(true, 0, 100)
)
"#;
    compile(src, "schema-post-resolution-ok").expect("42 is within 0..100");
}

// ==================== Feature gating ====================

#[test]
fn schema_allowed_by_default_advanced_mode() {
    let src = "@DATA( port = 1 )\n@SCHEMA( port = Schema.Int(true) )\n";
    let ast = compile(src, "schema-default-advanced").expect("default features unlock @SCHEMA");
    assert!(ast.schema.is_some());
}

#[test]
fn schema_allowed_with_explicit_feature() {
    let src = r#"
@CONFIG(
  version  -> "1.0.0"
  features -> "schema"
)
@DATA( port = 1 )
@SCHEMA( port = Schema.Int(true) )
"#;
    let ast = compile(src, "schema-explicit-feature").expect("'schema' in the features list should allow @SCHEMA");
    assert!(ast.schema.is_some());
}

#[test]
fn schema_blocked_in_basic_mode() {
    let src = r#"
@CONFIG(
  version  -> "1.0.0"
  features -> "basic"
)
@DATA( port = 1 )
@SCHEMA( port = Schema.Int(true) )
"#;
    let err = expect_err(src, "schema-basic-mode");
    assert!(err.contains("not allowed"), "got: {}", err);
}
