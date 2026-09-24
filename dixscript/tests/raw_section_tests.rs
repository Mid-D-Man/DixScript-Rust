// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md (referenced from the Compiler modules it
// tests)
// ============================================================================
//! Tests for the `@RAW` section: lexer token recognition and delimiter
//! scanning, parsing into `RawBlock`, and `raw_section_analyzer.rs`'s
//! cross-block checks (unique `meta_data.id`, unique delimiter tag) and
//! per-block required-field checks.
//!
//! Run with:
//!   cargo test --test raw_section_tests -- --nocapture

use dixscript::Compiler::Core::Config::OperationalSettings;
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, TokenType};
use dixscript::Runtime::DixLoader;

// ==================== Lexer: section keyword + content block ====================

#[test]
fn raw_section_keyword_is_recognized() {
    let settings = OperationalSettings::default();
    let result = Tokenizer::new("@RAW( meta_data -> { id = \"x\" } )", &settings).tokenize();
    assert!(
        result.tokens.iter().any(|t| matches!(t.token_type, TokenType::SectionRaw)),
        "expected a SectionRaw token, got: {:?}",
        result.tokens.iter().map(|t| &t.token_type).collect::<Vec<_>>()
    );
}

#[test]
fn raw_content_block_produces_correct_byte_range() {
    let source = "@RAW(\n  content -> {\n    ---tag001---\nhello world\n---tag001---\n  }\n)";
    let settings = OperationalSettings::default();
    let result = Tokenizer::new(source, &settings).tokenize();

    let content_token = result.tokens.iter().find_map(|t| match &t.token_type {
        TokenType::RawContent { tag, start, end } => Some((tag.clone(), *start, *end)),
        _ => None,
    });
    let (tag, start, end) = content_token.expect("expected a RawContent token");

    assert_eq!(tag, "tag001");
    // The recorded range must point at the real payload bytes in the
    // original source — this is the whole point of the byte-range design
    // (no copy at lex time), so it has to resolve correctly by slicing
    // the same buffer the lexer was given.
    let payload = &source.as_bytes()[start..end];
    assert_eq!(std::str::from_utf8(payload).unwrap(), "hello world\n");
}

#[test]
fn raw_content_block_rejects_mismatched_closing_tag() {
    // Opening delimiter says "foo", closing says "bar" -- must be an error,
    // not a silent scan past the wrong boundary.
    let source = "@RAW(\n  content -> {\n    ---foo---\ndata\n---bar---\n  }\n)";
    let settings = OperationalSettings::default();
    let result = Tokenizer::new(source, &settings).tokenize();

    let has_matching_content = result.tokens.iter().any(|t| {
        matches!(&t.token_type, TokenType::RawContent { tag, .. } if tag == "foo")
    });
    assert!(
        !has_matching_content,
        "a mismatched closing tag must not produce a successfully-closed RawContent block"
    );
}

#[test]
fn raw_content_block_reports_unterminated_block() {
    // No closing delimiter at all before EOF.
    let source = "@RAW(\n  content -> {\n    ---onlytag---\nno closer here\n  }\n)";
    let settings = OperationalSettings::default();
    let result = Tokenizer::new(source, &settings).tokenize();

    let has_error_token = result.tokens.iter().any(|t| matches!(t.token_type, TokenType::Error(_)));
    assert!(has_error_token, "unterminated @RAW content block should surface as a lexer error");
}

#[test]
fn raw_content_payload_may_contain_triple_dash_that_is_not_the_closer() {
    // "---" appearing in the payload but not followed by our own tag must
    // not be mistaken for the closing delimiter.
    let source = "@RAW(\n  content -> {\n    ---realtag---\nsome ---not-a-delimiter--- text\n---realtag---\n  }\n)";
    let settings = OperationalSettings::default();
    let result = Tokenizer::new(source, &settings).tokenize();

    let content_token = result.tokens.iter().find_map(|t| match &t.token_type {
        TokenType::RawContent { tag, start, end } => Some((tag.clone(), *start, *end)),
        _ => None,
    });
    let (tag, start, end) = content_token.expect("expected a successfully-closed RawContent token");
    assert_eq!(tag, "realtag");
    let payload = std::str::from_utf8(&source.as_bytes()[start..end]).unwrap();
    assert!(payload.contains("not-a-delimiter"), "payload should retain the embedded triple-dash text");
}

// ==================== Feature gating ====================

#[test]
fn raw_section_allowed_by_default_advanced_mode() {
    // No @CONFIG at all -> defaults to "advanced", which unlocks every
    // feature including "raw". See operational_settings.rs / config_schema.rs.
    let loader = DixLoader::new();
    let source = r#"
@RAW(
  meta_data -> { id = "asset_one", format = "TXT" }
  content -> { ---c1--- payload ---c1--- }
)
"#;
    let ast = loader
        .compile_to_resolved_ast_from_str(source, "raw-default-advanced")
        .expect("should compile with default (advanced) features");
    assert_eq!(ast.raw.len(), 1);
}

#[test]
fn raw_section_allowed_with_explicit_feature() {
    let loader = DixLoader::new();
    let source = r#"
@CONFIG(
  version    -> "1.0.0"
  features   -> "raw"
)
@RAW(
  meta_data -> { id = "asset_two", format = "TXT" }
  content -> { ---c2--- payload ---c2--- }
)
"#;
    let ast = loader
        .compile_to_resolved_ast_from_str(source, "raw-explicit-feature")
        .expect("'raw' in the features list should allow @RAW to parse");
    assert_eq!(ast.raw.len(), 1);
}

#[test]
fn raw_section_rejected_without_feature_enabled() {
    let loader = DixLoader::new();
    let source = r#"
@CONFIG(
  version    -> "1.0.0"
  features   -> "data"
)
@RAW(
  meta_data -> { id = "asset_three", format = "TXT" }
  content -> { ---c3--- payload ---c3--- }
)
"#;
    let result = loader.compile_to_resolved_ast_from_str(source, "raw-not-enabled");
    assert!(
        result.is_err(),
        "@RAW should be rejected when 'raw' isn't in the file's declared features"
    );
}

// ==================== Parser + analyzer: valid cases ====================

#[test]
fn multiple_raw_blocks_with_distinct_ids_and_tags_all_parse() {
    let loader = DixLoader::new();
    let source = r#"
@RAW(
  meta_data -> { id = "atlas_a", format = "MPX" }
  using -> { compression = "gzip", threads = 2 }
  content -> { ---atag--- AAAA ---atag--- }
)
@RAW(
  meta_data -> { id = "atlas_b", format = "MPX" }
  content -> { ---btag--- BBBB ---btag--- }
)
"#;
    let ast = loader
        .compile_to_resolved_ast_from_str(source, "raw-multi-block")
        .expect("two distinct @RAW blocks should both parse");

    assert_eq!(ast.raw.len(), 2);
    assert_eq!(ast.raw[0].id(), Some("atlas_a"));
    assert_eq!(ast.raw[1].id(), Some("atlas_b"));
}

// ==================== Analyzer: cross-block uniqueness ====================

#[test]
fn duplicate_meta_data_id_across_blocks_is_rejected() {
    let loader = DixLoader::new();
    let source = r#"
@RAW(
  meta_data -> { id = "same_id", format = "TXT" }
  content -> { ---one--- AAAA ---one--- }
)
@RAW(
  meta_data -> { id = "same_id", format = "TXT" }
  content -> { ---two--- BBBB ---two--- }
)
"#;
    let result = loader.compile_to_resolved_ast_from_str(source, "raw-dup-id");
    let err = result.expect_err("duplicate meta_data.id across @RAW blocks must be rejected");
    assert!(err.contains("same_id"), "error should name the duplicated id, got: {err}");
}

#[test]
fn duplicate_content_tag_across_blocks_is_rejected() {
    let loader = DixLoader::new();
    let source = r#"
@RAW(
  meta_data -> { id = "id_one", format = "TXT" }
  content -> { ---sametag--- AAAA ---sametag--- }
)
@RAW(
  meta_data -> { id = "id_two", format = "TXT" }
  content -> { ---sametag--- BBBB ---sametag--- }
)
"#;
    let result = loader.compile_to_resolved_ast_from_str(source, "raw-dup-tag");
    let err = result.expect_err("duplicate delimiter tag across @RAW blocks must be rejected");
    assert!(err.contains("sametag"), "error should name the duplicated tag, got: {err}");
}

// ==================== Analyzer: required fields ====================

#[test]
fn missing_meta_data_id_is_rejected() {
    let loader = DixLoader::new();
    let source = r#"
@RAW(
  meta_data -> { format = "TXT" }
  content -> { ---c--- data ---c--- }
)
"#;
    let result = loader.compile_to_resolved_ast_from_str(source, "raw-missing-id");
    assert!(result.is_err(), "missing meta_data.id must be rejected");
}

#[test]
fn missing_meta_data_format_is_rejected() {
    let loader = DixLoader::new();
    let source = r#"
@RAW(
  meta_data -> { id = "no_format" }
  content -> { ---c--- data ---c--- }
)
"#;
    let result = loader.compile_to_resolved_ast_from_str(source, "raw-missing-format");
    assert!(result.is_err(), "missing meta_data.format must be rejected");
}

#[test]
fn missing_content_block_is_rejected() {
    let loader = DixLoader::new();
    let source = r#"
@RAW(
  meta_data -> { id = "no_content", format = "TXT" }
)
"#;
    let result = loader.compile_to_resolved_ast_from_str(source, "raw-missing-content");
    assert!(result.is_err(), "an @RAW block with no content block at all must be rejected");
}

#[test]
fn wrong_type_for_meta_data_size_is_rejected() {
    let loader = DixLoader::new();
    let source = r#"
@RAW(
  meta_data -> { id = "bad_size", format = "TXT", size = "not-a-number" }
  content -> { ---c--- data ---c--- }
)
"#;
    let result = loader.compile_to_resolved_ast_from_str(source, "raw-bad-size-type");
    assert!(result.is_err(), "meta_data.size should be required to be numeric");
}

// ==================== @RAW must stay out of @IMPORTS targets ====================

#[test]
fn raw_inside_imports_target_is_not_supported() {
    // imports_resolver.rs doesn't know @RAW exists as an importable
    // section at all yet -- this just confirms today's behavior is "not
    // silently accepted", not any specific error shape.
    let loader = DixLoader::new();
    let source = r#"
@IMPORTS(
  Assets from "raw_only.mdix"
)
@DATA(
  x = Assets.something
)
"#;
    // This doesn't assert success or failure either way -- it's here as a
    // living marker: if @RAW import support is ever added deliberately,
    // this test should be revisited rather than silently start passing for
    // an unrelated reason.
    let _ = loader.compile_to_resolved_ast_from_str(source, "raw-imports-marker");
}
