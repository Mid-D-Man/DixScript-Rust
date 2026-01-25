// tests/enums_parser_tests.rs

use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::SectionParsers::EnumsSectionParser;
use dixscript::Compiler::Core::OperationalSettings;
use dixscript::ErrorManager::ErrorManager;

fn tokenize_and_parse_enums(input: &str) -> Option<dixscript::Compiler::AST::EnumsSection> {
    // Clear errors
    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    // Tokenize
    let tokenizer = Tokenizer::new(input.to_string());
    let result = tokenizer.tokenize();

    // Create operational settings
    let settings = OperationalSettings::default();

    // Parse
    let mut parser = EnumsSectionParser::new(&result.tokens, &settings);
    parser.parse_section()
}

#[test]
fn test_simple_enum() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INACTIVE = 2 }
        )
    "#;

    let section = tokenize_and_parse_enums(input).expect("Failed to parse");

    assert_eq!(section.enums.len(), 1);
    assert_eq!(section.enums[0].name, "Status");
    assert_eq!(section.enums[0].fields.len(), 2);
    assert_eq!(section.enums[0].fields[0].name, "ACTIVE");
    assert_eq!(section.enums[0].fields[0].value, Some(1));
}

#[test]
fn test_multiple_enums() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INACTIVE = 2 }
            Priority { LOW = 1, MEDIUM = 2, HIGH = 3 }
        )
    "#;

    let section = tokenize_and_parse_enums(input).expect("Failed to parse");

    assert_eq!(section.enums.len(), 2);
    assert_eq!(section.enums[0].name, "Status");
    assert_eq!(section.enums[1].name, "Priority");
    assert_eq!(section.enums[1].fields.len(), 3);
}

#[test]
fn test_enum_without_values() {
    let input = r#"
        @ENUMS(
            Color { RED, GREEN, BLUE }
        )
    "#;

    let section = tokenize_and_parse_enums(input).expect("Failed to parse");

    assert_eq!(section.enums.len(), 1);
    assert_eq!(section.enums[0].fields.len(), 3);
    assert_eq!(section.enums[0].fields[0].value, None);
    assert_eq!(section.enums[0].fields[1].value, None);
    assert_eq!(section.enums[0].fields[2].value, None);
}

#[test]
fn test_enum_mixed_values() {
    let input = r#"
        @ENUMS(
            Mixed { FIRST = 10, SECOND, THIRD = 30 }
        )
    "#;

    let section = tokenize_and_parse_enums(input).expect("Failed to parse");

    assert_eq!(section.enums[0].fields.len(), 3);
    assert_eq!(section.enums[0].fields[0].value, Some(10));
    assert_eq!(section.enums[0].fields[1].value, None);
    assert_eq!(section.enums[0].fields[2].value, Some(30));
}

#[test]
fn test_enum_with_trailing_comma() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INACTIVE = 2, }
        )
    "#;

    let section = tokenize_and_parse_enums(input).expect("Failed to parse");

    assert_eq!(section.enums[0].fields.len(), 2);
}

#[test]
fn test_empty_enums_section() {
    let input = r#"
        @ENUMS()
    "#;

    let section = tokenize_and_parse_enums(input).expect("Failed to parse");

    assert_eq!(section.enums.len(), 0);
}

#[test]
fn test_enum_error_recovery() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INVALID FIELD, INACTIVE = 2 }
            Priority { LOW = 1, MEDIUM = 2 }
        )
    "#;

    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let section = tokenize_and_parse_enums(input);

    // Should have errors
    assert!(error_manager.has_errors());

    // But might still parse some enums depending on recovery strategy
    if let Some(s) = section {
        println!("Recovered {} enums despite errors", s.enums.len());
    }
}

#[test]
fn test_enum_with_comments() {
    let input = r#"
        @ENUMS(
            // Main status enum
            Status {
                ACTIVE = 1,    // Active state
                INACTIVE = 2   // Inactive state
            }
        )
    "#;

    let section = tokenize_and_parse_enums(input).expect("Failed to parse");

    assert_eq!(section.enums.len(), 1);
    assert_eq!(section.enums[0].fields.len(), 2);
}

#[test]
fn test_positions_are_tracked() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1 }
        )
    "#;

    let section = tokenize_and_parse_enums(input).expect("Failed to parse");

    // Check positions are valid
    assert!(section.position.is_valid());
    assert!(section.enums[0].position.is_valid());
    assert!(section.enums[0].fields[0].position.is_valid());
}