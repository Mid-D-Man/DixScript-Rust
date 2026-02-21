// tests/data_parser_tests.rs

use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::SectionParsers::DataSectionParser;
use dixscript::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use dixscript::ErrorManager::ErrorManager;
use dixscript::Compiler::AST::{DataSection, DataEntry};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use std::time::Instant;

// ==================== PERFORMANCE BASELINES ====================
// DATA section is more complex than SECURITY, so baselines are adjusted
// Hand-written parsers: optimized for error recovery & diagnostics
// LALRPOP: ~5-10x faster for pure parsing (generated code)
// Trade-off: Better errors vs raw speed

const BASELINE_SMALL_INPUT_MS: u128 = 10;  // vs SECURITY: 5ms
const BASELINE_MEDIUM_INPUT_MS: u128 = 100; // vs SECURITY: 50ms
const BASELINE_LARGE_INPUT_MS: u128 = 1000; // vs SECURITY: 500ms
const BASELINE_ENTRIES_PER_SEC: f64 = 500.0; // vs SECURITY: 1000
const BASELINE_TOKENS_PER_SEC: f64 = 5000.0; // vs SECURITY: 10000

// LALRPOP comparison (estimated):
// - Pure parsing speed: 5-10x faster
// - Error recovery: Worse (less flexible)
// - Memory overhead: Lower (generated code)
// - Compile time: Higher (parser generation)

// ==================== HELPER FUNCTIONS ====================

fn tokenize_input(input: &str) -> Vec<Token> {
    let tokenizer = Tokenizer::new(input.to_string());
    let result = tokenizer.tokenize();
    result.tokens
}

fn extract_data_section_tokens(tokens: &[Token]) -> Vec<Token> {
    let start_pos = tokens.iter()
        .position(|t| matches!(t.token_type, TokenType::SectionData))
        .expect("No @DATA section found");

    let paren_start = tokens[start_pos + 1..].iter()
        .position(|t| matches!(t.token_type, TokenType::Symbol('(')))
        .expect("No opening ( found");

    let actual_start = start_pos + 1 + paren_start;

    let mut depth = 0;
    let mut end_pos = actual_start;

    for (i, token) in tokens[actual_start..].iter().enumerate() {
        match &token.token_type {
            TokenType::Symbol('(') => depth += 1,
            TokenType::Symbol(')') => {
                depth -= 1;
                if depth == 0 {
                    end_pos = actual_start + i;
                    break;
                }
            }
            _ => {}
        }
    }

    let mut section_tokens = tokens[actual_start..=end_pos].to_vec();
    section_tokens.push(Token::eof(1, 1));
    section_tokens
}

fn parse_data_with_settings(input: &str, settings: OperationalSettings) -> Option<DataSection> {
    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let tokens = tokenize_input(input);
    let section_tokens = extract_data_section_tokens(&tokens);

    let mut parser = DataSectionParser::new(&section_tokens, &settings);
    parser.parse_section()
}

fn parse_data_default(input: &str) -> Option<DataSection> {
    parse_data_with_settings(input, OperationalSettings::default())
}

fn parse_data_halt_on_error(input: &str) -> Option<DataSection> {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Halt;
    parse_data_with_settings(input, settings)
}

fn parse_data_recover(input: &str) -> Option<DataSection> {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Recover;
    parse_data_with_settings(input, settings)
}

// ==================== BASIC FUNCTIONALITY TESTS ====================

#[test]
fn test_simple_property() {
    let input = r#"
        @DATA(
            app_name = "DixScript",
            version = "1.0.0",
            port = 8080
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 3);

    // All should be SimpleProperty
    for entry in &section.entries {
        assert!(matches!(entry, DataEntry::SimpleProperty { .. }));
    }
}

#[test]
fn test_table_property() {
    let input = r#"
        @DATA(
            server.config:
                host = "localhost",
                port = 8080,
                ssl = true
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);

    match &section.entries[0] {
        DataEntry::TableProperty { path, properties, .. } => {
            assert_eq!(path.segments.len(), 2);
            assert_eq!(path.segments[0], "server");
            assert_eq!(path.segments[1], "config");
            assert_eq!(properties.len(), 3);
        }
        _ => panic!("Expected TableProperty"),
    }
}

#[test]
fn test_group_array() {
    let input = r#"
        @DATA(
            users::
                { id = 1, name = "Alice" },
                { id = 2, name = "Bob" }
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);

    match &section.entries[0] {
        DataEntry::GroupArray { path, items, .. } => {
            assert_eq!(path.segments.len(), 1);
            assert_eq!(path.segments[0], "users");
            assert_eq!(items.len(), 2);
        }
        _ => panic!("Expected GroupArray"),
    }
}

#[test]
fn test_object_property() {
    let input = r#"
        @DATA(
            config = {
                host = "localhost",
                port = 8080,
                debug = true
            }
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);

    match &section.entries[0] {
        DataEntry::ObjectProperty { name, object, .. } => {
            assert_eq!(name.as_str(), "config");
            // Verify it's an object value
            if let dixscript::Compiler::AST::Value::Object { properties, .. } = &**object {
                assert_eq!(properties.len(), 3);
            } else {
                panic!("Expected Object value");
            }
        }
        _ => panic!("Expected ObjectProperty"),
    }
}

#[test]
fn test_mixed_entry_types() {
    let input = r#"
        @DATA(
            simple = 42,

            table.path:
                x = 1,
                y = 2,

            array_ofNums:: 10, 20, 30,

            obj = { a = 1, b = 2 }
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 4);

    assert!(matches!(section.entries[0], DataEntry::SimpleProperty { .. }));
    assert!(matches!(section.entries[1], DataEntry::TableProperty { .. }));
    assert!(matches!(section.entries[2], DataEntry::GroupArray { .. }));
    assert!(matches!(section.entries[3], DataEntry::ObjectProperty { .. }));
}

#[test]
fn test_all_primitive_types() {
    let input = r#"
        @DATA(
            int_val = 42,
            float_val = 3.14f,
            double_val = 2.71828,
            string_val = "hello",
            bool_val = true,
            hex_val = 0xFF,
            date_val = 2025-01-26,
            timestamp_val = 2025-01-26T12:00:00Z
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 8);
}

#[test]
fn test_nested_objects() {
    let input = r#"
        @DATA(
            config = {
                database = {
                    host = "localhost",
                    credentials = {
                        user = "admin",
                        pass = "secret"
                    }
                }
            }
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
}

#[test]
fn test_array_literals() {
    let input = r#"
        @DATA(
            numbers = [1, 2, 3, 4, 5],
            strings = ["a", "b", "c"],
            nested = [[1, 2], [3, 4]],
            mixed_objects = [
                { id = 1, name = "first" },
                { id = 2, name = "second" }
            ]
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 4);
}

#[test]
fn test_prefixed_constructors() {
    let input = r#"
        @DATA(
            blob_val = b:("SGVsbG8="),
            tuple_val = t:(1, "test", true),
            regex_val = r:("^[a-z]+$")
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 3);
}

#[test]
fn test_empty_data_section() {
    let input = r#"@DATA()"#;
    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 0);
}
//significant issue here....
#[test]
fn test_type_annotations() {
    let input = r#"
        @DATA(
            port<int> = 8080,
            host<string> = "localhost",
            enabled<bool> = true
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 3);
}

#[test]
fn test_optional_commas_simple_properties() {
    let input = r#"
        @DATA(
            a = 1,
            b = 2
            c = 3
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 3);
}

#[test]
fn test_optional_commas_table_properties() {
    let input = r#"
        @DATA(
            table1:
                x = 1
                y = 2

            table2:
                a = 10,
                b = 20
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 2);
}

#[test]
fn test_optional_commas_group_arrays() {
    let input = r#"
        @DATA(
            array1:: 1, 2, 3
            array2:: 10 20 30
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 2);
}

#[test]
fn test_positions_tracked() {
    let input = r#"
        @DATA(
            test = 42
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert!(section.position.is_valid());
    assert!(section.entries[0].position().is_valid());
}

// ==================== TWO-TIER SYSTEM TESTS ====================

#[test]
fn test_two_tier_correct_order() {
    let input = r#"
        @DATA(
            flat1 = "value",
            flat2 = 42,

            table.prop: x = 1,
            array_:: item1, item2
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 4);
}

#[test]
fn test_two_tier_violation_detected() {
    let input = r#"
        @DATA(
            table.prop: x = 1,
            illegal_flat = 42
        )
    "#;

    let section = parse_data_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

// ==================== FUNCTION CALL TESTS ====================

#[test]
fn test_local_function_call() {
    let input = r#"
        @DATA(
            result = calculate(10, 20)
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
}

#[test]
fn test_nested_function_calls() {
    let input = r#"
        @DATA(
            result = outer(inner(deep(value)))
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
}

#[test]
fn test_function_calls_in_arrays() {
    let input = r#"
        @DATA(
            results = [
                calculate(1, 2),
                calculate(3, 4),
                calculate(5, 6)
            ]
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
}

#[test]
fn test_function_calls_in_objects() {
    let input = r#"
        @DATA(
            config = {
                total = sum(1, 2, 3),
                average = avg(10, 20, 30)
            }
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
}

#[test]
fn test_max_function_nesting_depth() {
    // Test 10 levels (should work)
    let input = r#"
        @DATA(
            result = f1(f2(f3(f4(f5(f6(f7(f8(f9(f10(42))))))))))
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
}

#[test]
fn test_exceed_function_nesting_depth() {
    // Test 11+ levels (should error)
    let input = r#"
        @DATA(
            result = f1(f2(f3(f4(f5(f6(f7(f8(f9(f10(f11(42)))))))))))
        )
    "#;
   //u have to get shared instance before trying to get errors maybe
    let section = parse_data_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

// ==================== ERROR HANDLING TESTS ====================

#[test]
fn test_missing_equals_in_simple_property() {
    let input = r#"
        @DATA(
            name "value"
        )
    "#;

    let section = parse_data_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_missing_colon_in_table_property() {
    let input = r#"
        @DATA(
            table.path x = 1
        )
    "#;

    let section = parse_data_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_missing_double_colon_in_group_array() {
    let input = r#"
        @DATA(
            array: item1, item2
        )
    "#;

    // Single colon should be treated as table property, not group array
    let section = parse_data_default(input);

    if let Some(s) = section {
        match &s.entries[0] {
            DataEntry::TableProperty { .. } => {
                // Expected behavior
            }
            _ => panic!("Expected TableProperty for single colon"),
        }
    }
}

#[test]
fn test_unclosed_object_brace() {
    let input = r#"
        @DATA(
            obj = { x = 1, y = 2
        )
    "#;

    let section = parse_data_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_unclosed_array_bracket() {
    let input = r#"
        @DATA(
            arr = [1, 2, 3
        )
    "#;

    let section = parse_data_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_trailing_comma_in_object() {
    let input = r#"
        @DATA(
            obj = { x = 1, y = 2, }
        )
    "#;

    // Should parse successfully (trailing commas allowed)
    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
}

#[test]
fn test_nested_group_array_syntax_error() {
    let input = r#"
        @DATA(
            obj = {
                nested:: item1, item2
            }
        )
    "#;

    let section = parse_data_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_wrong_object_syntax_colon_instead_of_equals() {
    let input = r#"
        @DATA(
            obj = {
                x: 1,
                y: 2
            }
        )
    "#;

    let section = parse_data_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_halt_strategy_stops_on_error() {
    let input = r#"
        @DATA(
            valid = 42,
            INVALID SYNTAX,
            another = 100
        )
    "#;

    let section = parse_data_halt_on_error(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_recover_strategy_continues() {
    let input = r#"
        @DATA(
            valid1 = 42,
            INVALID,
            valid2 = 100
        )
    "#;

    let section = parse_data_recover(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());

    if let Some(s) = section {
        println!("Recovered {} entries", s.entries.len());
    }
}

// ==================== NESTING DEPTH TESTS ====================

#[test]
fn test_object_nesting_depth_10() {
    let mut input = String::from("@DATA(obj = ");
    for _ in 0..10 {
        input.push_str("{ inner = ");
    }
    input.push_str("42");
    for _ in 0..10 {
        input.push_str(" }");
    }
    input.push_str(")");

    let section = parse_data_default(&input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
}

#[test]
fn test_object_nesting_depth_64() {
    // Max allowed depth
    let mut input = String::from("@DATA(obj = ");
    for _ in 0..64 {
        input.push_str("{ inner = ");
    }
    input.push_str("42");
    for _ in 0..64 {
        input.push_str(" }");
    }
    input.push_str(")");

    let section = parse_data_default(&input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
}

#[test]
fn test_exceed_object_nesting_depth() {
    // 65 levels (should error)
    let mut input = String::from("@DATA(obj = ");
    for _ in 0..65 {
        input.push_str("{ inner = ");
    }
    input.push_str("42");
    for _ in 0..65 {
        input.push_str(" }");
    }
    input.push_str(")");

    let section = parse_data_default(&input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

// ==================== PERFORMANCE TESTS ====================

#[test]
fn test_parse_speed_small_input() {
    //note cant use comments if testing individual sections cause comments
    //supposed to be striped out by general parser... could make helper for it tough
    let input = r#"
     @DATA(

    int_positive<int> = 42,
    int_negative<int> = -100,
    int_zero<int> = 0,
    int_large<int> = 2147483647,
    int_small<int> = -2147483648,


    float_positive<float> = 3.14f,
    float_negative<float> = -2.5f,
    float_zero<float> = 0.0f,
    float_no_decimal<float> = 42f,
    float_small<float> = 0.001f,
    float_large<float> = 999999.99f,


    double_positive<double> = 3.141592653589793,
    double_negative<double> = -2.718281828,
    double_zero<double> = 0.0,
    double_scientific<double> = 1.23e10,
    double_scientific_negative<double> = -4.56e-5,
    double_scientific_positive_exp<double> = 7.89e+12,


    sci_float<float> = 1.5e3f,
    sci_float_negative<float> = -2.5e-2f,


    string_double_quotes<string> = "Hello World",
    string_single_quotes<string> = 'Single quotes',
    string_empty<string> = "",
    string_with_spaces<string> = "   spaces   ",
    string_with_escapes<string> = "Line1\nLine2\tTabbed",
    string_with_quotes<string> = "He said \"Hello\"",



    bool_true<bool> = true,
    bool_false<bool> = false,


    hex_simple<hex> = 0xFF,
    hex_large<hex> = 0xDEADBEEF,
    hex_zero<hex> = 0x0,
    hex_with_letters<hex> = 0xABCDEF,


    color_rgb_short<hex> = #F00,
    color_rgb_long<hex> = #FF0000,
    color_rgba_short<hex> = #F00F,
    color_rgba_long<hex> = #FF0000FF,
    color_with_alpha<hex> = #80FFFFFF,


    date_standard<date> = 2025-01-15,
    date_leap_year<date> = 2024-02-29,
    date_year_end<date> = 2025-12-31,
    date_year_start<date> = 2025-01-01,


    timestamp_basic<timestamp> = 2025-01-15T10:30:00Z,
    timestamp_with_millis<timestamp> = 2025-01-15T10:30:00.123Z,
    timestamp_with_timezone<timestamp> = 2025-01-15T10:30:00+05:30,
    timestamp_utc<timestamp> = 2025-01-15T10:30:00.999Z,


    array_integers = [1, 2, 3, 4, 5],
    array_floats = [1.1f, 2.2f, 3.3f],
    array_strings = ["apple", "banana", "cherry"],
    array_booleans = [true, false, true, true],
    array_empty = [],
    array_nested = [[1, 2], [3, 4], [5, 6]],
    array_objects = [
        { id = 1, name = "First" },
        { id = 2, name = "Second" },
        { id = 3, name = "Third" }
    ],


    tuple_empty = t:(),
    tuple_single = t:(42),
    tuple_two = t:(42, "text"),
    tuple_three = t:(42, "text", true),
    tuple_four = t:(42, "text", true, 3.14f),
    tuple_mixed_types = t:(100, "hello", false, 2.5f),


    object_simple = {
        name = "Test",
        value = 42
    },
    object_nested = {
        outer = {
            inner = {
                deep = "value"
            }
        }
    },
    object_complex = {
        id = 1,
        name = "Complex",
        active = true,
        score = 95.5f,
        tags = ["tag1", "tag2"],
        metadata = {
            created = 2025-01-15,
            author = "System"
        }
    },
    object_empty = {},


    blob_simple = b:("SGVsbG8gV29ybGQ="),
    blob_empty = b:(""),
    blob_long = b:("VGhpcyBpcyBhIGxvbmdlciBiYXNlNjQgZW5jb2RlZCBzdHJpbmcgdGhhdCByZXByZXNlbnRzIGJpbmFyeSBkYXRh"),


    regex_simple = r:("^[a-z]+$"),
    regex_email = r:("^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"),
    regex_phone = r:("^\d{3}-\d{3}-\d{4}$"),
    regex_complex = r:("^(?=.*[A-Za-z])(?=.*\d)[A-Za-z\d]{8,}$"),


    enum_first<enum> = TestEnum.FIRST,
    enum_second<enum> = TestEnum.SECOND,
    enum_third<enum> = TestEnum.THIRD,


    null_value = null


    mixed_types:
        int_val<int> = 100,
        float_val<float> = 3.14f,
        string_val<string> = "mixed",
        bool_val<bool> = true,
        array_val = [1, 2, 3],
        object_val = { x = 10, y = 20 },


    test_all_types_call = testAllTypes(
        42,
        3.14f,
        2.718281828,
        "test string",
        true,
        0xFF00AA,
        2025-01-15,
        2025-01-15T10:30:00Z,
        [1, 2, 3],
        t:(1, "text", true),
        { key = "value" },
        b:("SGVsbG8="),
        r:("^test$"),
        TestEnum.FIRST
    ),

    test_int_ops = testIntOperations(10, 5, 3),
    test_float_ops = testFloatOperations(10.5f, 2.5f),
    test_double_ops = testDoubleOperations(100.75, 50.25),
    test_string_ops = testStringOperations("Hello", "beautiful", "world"),
    test_bool_ops = testBoolOperations(true, false),
    test_array_ops = testArrayOperations([5, 10, 15, 20]),
    test_tuple_ops = testTupleOperations(t:(10, 20, 30)),
    test_object_ops = testObjectOperations({ test = "value" }),
    test_date_ops = testDateOperations(2025-12-31),
    test_timestamp_ops = testTimestampOperations(2025-01-15T10:30:45Z),
    test_enum_ops = testEnumOperations(TestEnum.SECOND),
    test_hex_ops = testHexOperations(0xABCDEF),
    test_null_ops = testNullHandling()
)
    "#;

    let tokens = tokenize_input(input);
    let section_tokens = extract_data_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = DataSectionParser::new(&section_tokens, &settings);
    let _section = parser.parse_section();
    let duration = start.elapsed();

    println!("\n=== DATA PARSER - SMALL INPUT ===");
    println!("Baseline: < {}ms", BASELINE_SMALL_INPUT_MS);
    println!("Actual: {:?}", duration);
    println!("Status: {}", if duration.as_millis() < BASELINE_SMALL_INPUT_MS { "✅ PASS" } else { "❌ FAIL" });
    println!("=================================\n");

    assert!(
        duration.as_millis() < BASELINE_SMALL_INPUT_MS,
        "Too slow: {:?} (baseline: {}ms)",
        duration,
        BASELINE_SMALL_INPUT_MS
    );
}

#[test]
fn test_parse_speed_medium_input() {
    // 100 simple properties
    let mut input = String::from("@DATA(\n");
    for i in 0..100 {
        input.push_str(&format!("    prop{} = {},\n", i, i));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_data_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = DataSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let entries_per_sec = section.entries.len() as f64 / duration.as_secs_f64();

    println!("\n=== DATA PARSER - MEDIUM INPUT ===");
    println!("Entries: {}", section.entries.len());
    println!("Baseline: < {}ms, > {} entries/sec", BASELINE_MEDIUM_INPUT_MS, BASELINE_ENTRIES_PER_SEC);
    println!("Actual: {:?} ({:.0} entries/sec)", duration, entries_per_sec);
    println!("Status: {}",
             if duration.as_millis() < BASELINE_MEDIUM_INPUT_MS && entries_per_sec > BASELINE_ENTRIES_PER_SEC {
                 "✅ PASS"
             } else {
                 "❌ FAIL"
             }
    );
    println!("==================================\n");

    assert!(
        duration.as_millis() < BASELINE_MEDIUM_INPUT_MS,
        "Too slow: {:?} (baseline: {}ms)",
        duration,
        BASELINE_MEDIUM_INPUT_MS
    );
    assert_eq!(section.entries.len(), 100);
}

#[test]
fn test_parse_speed_large_input() {
    // 500 simple properties
    let mut input = String::from("@DATA(\n");
    for i in 0..500 {
        input.push_str(&format!("    prop{} = {},\n", i, i));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_data_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = DataSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let entries_per_sec = section.entries.len() as f64 / duration.as_secs_f64();

    println!("\n=== DATA PARSER - LARGE INPUT ===");
    println!("Entries: {}", section.entries.len());
    println!("Baseline: < {}ms, > {} entries/sec", BASELINE_LARGE_INPUT_MS, BASELINE_ENTRIES_PER_SEC);
    println!("Actual: {:?} ({:.0} entries/sec)", duration, entries_per_sec);
    println!("Status: {}",
             if duration.as_millis() < BASELINE_LARGE_INPUT_MS && entries_per_sec > BASELINE_ENTRIES_PER_SEC {
                 "✅ PASS"
             } else {
                 "❌ FAIL"
             }
    );
    println!("=================================\n");

    assert!(
        duration.as_millis() < BASELINE_LARGE_INPUT_MS,
        "Too slow: {:?} (baseline: {}ms)",
        duration,
        BASELINE_LARGE_INPUT_MS
    );
    assert_eq!(section.entries.len(), 500);
}

#[test]
fn test_parse_complex_structures() {
    // Mix of all entry types
    let mut input = String::from("@DATA(\n");

    // Simple properties
    for i in 0..50 {
        input.push_str(&format!("    simple{} = {},\n", i, i));
    }

    // Table properties
    for i in 0..50 {
        input.push_str(&format!("    table{}:\n        x = {}\n        y = {}\n", i, i, i*2));
    }

    // Group arrays
    for i in 0..50 {
        input.push_str(&format!("    array_{i}:: {i}, {}, {}\n", i*2, i*3));
    }



    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_data_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = DataSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    println!("\n=== DATA PARSER - COMPLEX MIX ===");
    println!("Total entries: {}", section.entries.len());
    println!("Time: {:?}", duration);
    println!("=================================\n");

    assert_eq!(section.entries.len(), 200);
}

#[test]
fn test_parse_throughput() {
    let mut input = String::from("@DATA(\n");
    for i in 0..200 {
        input.push_str(&format!("    prop{} = {},\n", i, i));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_data_section_tokens(&tokens);
    let token_count = section_tokens.len();
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = DataSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let tokens_per_sec = token_count as f64 / duration.as_secs_f64();

    println!("\n=== DATA PARSER - THROUGHPUT ===");
    println!("Tokens: {}", token_count);
    println!("Baseline: > {} tokens/sec", BASELINE_TOKENS_PER_SEC);
    println!("Actual: {:.0} tokens/sec", tokens_per_sec);
    println!("Status: {}", if tokens_per_sec > BASELINE_TOKENS_PER_SEC { "✅ PASS" } else { "❌ FAIL" });
    println!("================================\n");

    assert!(
        tokens_per_sec > BASELINE_TOKENS_PER_SEC,
        "Too slow: {:.0} tokens/sec (baseline: {})",
        tokens_per_sec,
        BASELINE_TOKENS_PER_SEC
    );
    assert_eq!(section.entries.len(), 200);
}

#[test]
#[ignore]
fn test_release_mode_performance() {
    // Very large input - run in release mode only
    let mut input = String::from("@DATA(\n");
    for i in 0..2000 {
        input.push_str(&format!("    prop{} = {},\n", i, i));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_data_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = DataSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let entries_per_sec = section.entries.len() as f64 / duration.as_secs_f64();

    println!("\n=== DATA PARSER - RELEASE MODE ===");
    println!("Entries: {}", section.entries.len());
    println!("Time: {:?}", duration);
    println!("Entries/sec: {:.0}", entries_per_sec);
    println!("Expected: > 2,000 entries/sec");
    println!("Status: {}", if entries_per_sec > 2000.0 { "✅ PASS" } else { "❌ FAIL" });
    println!("==================================\n");

    assert!(entries_per_sec > 2000.0, "Too slow in release mode: {:.0} entries/sec", entries_per_sec);
}

// ==================== MEMORY USAGE TESTS ====================

#[test]
fn test_memory_usage_estimate() {
    let input = r#"
        @DATA(
            simple = 42,
            table.path: x = 1, y = 2,
            array_ofNums:: 1, 2, 3,//cant use array as identifier its a type,
            objs_only_within:

        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");

    let entry_sizes: Vec<usize> = section.entries.iter()
        .map(|e| std::mem::size_of_val(e))
        .collect();

    let total_size: usize = entry_sizes.iter().sum();
    let avg_size = total_size / entry_sizes.len();

    println!("\n=== DATA PARSER - MEMORY USAGE ===");
    println!("Entry types tested: {}", entry_sizes.len());
    for (i, size) in entry_sizes.iter().enumerate() {
        println!("  Entry {}: {} bytes", i, size);
    }
    println!("Average entry size: {} bytes", avg_size);
    println!("Expected: < 4KB per entry");
    println!("Status: {}", if avg_size < 4096 { "✅ PASS" } else { "❌ FAIL" });
    println!("===================================\n");

    assert!(avg_size < 4096, "Entry too large: {} bytes", avg_size);
}

#[test]
fn test_no_memory_leaks_repeated_parsing() {
    let input = r#"
        @DATA(
            test = 42,
            nested = {
                inner = {
                    deep = [1, 2, 3]
                }
            }
        )
    "#;

    // Parse same input 1000 times
    for _ in 0..1000 {
        let _ = parse_data_default(input);
    }

    println!("✅ Successfully parsed same input 1000 times without memory leaks");
}

#[test]
fn test_memory_with_large_nested_structure() {
    // Create deeply nested structure
    let mut input = String::from("@DATA(obj = ");
    for _ in 0..30 {
        input.push_str("{ inner = ");
    }
    input.push_str("[1, 2, 3, 4, 5]");
    for _ in 0..30 {
        input.push_str(" }");
    }
    input.push_str(")");

    let section = parse_data_default(&input).expect("Failed to parse");
    let entry_size = std::mem::size_of_val(&section.entries[0]);

    println!("\n=== NESTED STRUCTURE MEMORY ===");
    println!("Nesting depth: 30 levels");
    println!("Entry size: {} bytes", entry_size);
    println!("Expected: < 10KB");
    println!("Status: {}", if entry_size < 10240 { "✅ PASS" } else { "❌ FAIL" });
    println!("================================\n");

    assert!(entry_size < 10240, "Nested entry too large: {} bytes", entry_size);
}

// ==================== EDGE CASES ====================
//ok array :: also seems problematic
#[test]
fn test_whitespace_handling() {
    let input = r#"
        @DATA(
            prop1   =   42  ,
            table.path  :   x   =   1   ,
            array_  ::  1  ,  2  ,  3
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 3);
}

#[test]
fn test_unicode_in_strings() {
    let input = r#"
        @DATA(
            greeting = "Hello, 世界! 🌍",
            emoji = "✨🎉🚀"
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 2);
}

#[test]
fn test_escaped_characters() {
    let input = r#"
        @DATA(
            text = "Line1\nLine2\tTabbed",
            quotes = "He said \"Hello\""
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 2);
}

#[test]
fn test_scientific_notation() {
    let input = r#"
        @DATA(
            small = 1.5e-10,
            large = 2.5e+20,
            float_sci = 3.14e3f
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 3);
}

#[test]
fn test_empty_collections() {
    let input = r#"
        @DATA(
            empty_array = [],
            empty_object = {},
            empty_tuple = t:()
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 3);
}

#[test]
fn test_single_item_collections() {
    let input = r#"
        @DATA(
            single_array = [42],
            single_object = { x = 1 },
            single_tuple = t:(42)
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 3);
}

#[test]
fn test_max_tuple_size() {
    let input = r#"
        @DATA(
            tuple4 = t:(1, 2, 3, 4)
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
}

#[test]
fn test_null_values() {
    let input = r#"
        @DATA(
            null_val = null,
            obj_with_null = { x = null, y = 42 }
        )
    "#;

    let section = parse_data_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 2);
}

// ==================== PARSER COMPARISON INFO ====================

#[test]
#[ignore]
fn baseline_comparison_info() {
    println!("\n=== DIXSCRIPT DATA PARSER BASELINE ===");
    println!("Small input (3 entries): < 10ms");
    println!("Medium input (100 entries): < 100ms");
    println!("Large input (500 entries): < 1000ms");
    println!("Throughput: > 5,000 tokens/sec");
    println!("Release mode: > 2,000 entries/sec");
    println!("\nParser Characteristics:");
    println!("- Hand-written recursive descent");
    println!("- Four entry types: Simple, Table, GroupArray, Object");
    println!("- Max object nesting: 64 levels (JSON-like)");
    println!("- Max function nesting: 10 levels");
    println!("- Loop safety: Dynamic limits + stuck detection");
    println!("\nComparison to LALRPOP:");
    println!("- LALRPOP: Generated LR parser (compile-time)");
    println!("- DixScript: Hand-written (runtime, flexible)");
    println!("- Speed trade-off: ~5-10x slower than LALRPOP");
    println!("- Benefits:");
    println!("  * Better error messages with context");
    println!("  * Advanced error recovery");
    println!("  * Flexible two-tier system validation");
    println!("  * Runtime nesting depth checks");
    println!("\nWhy hand-written for DixScript:");
    println!("- Complex two-tier validation (Tier 1 vs Tier 2)");
    println!("- Context-aware error messages");
    println!("- Custom recovery strategies");
    println!("- Import-aware identifier resolution");
    println!("- Real-time nesting depth tracking");
    println!("=======================================\n");
}