// tests/tokenizer_tests.rs

use dixscript::Compiler::Core::Tokenizer::{Tokenizer, TokenizationResult};
use dixscript::Utilities::{Token, TokenType};
use dixscript::ErrorManager::ErrorManager;
use std::time::Instant;

// ==================== HELPER FUNCTIONS ====================

fn tokenize_input(input: &str) -> TokenizationResult {
    let tokenizer = Tokenizer::new(input.to_string());
    tokenizer.tokenize()
}

fn assert_token_type(token: &Token, expected: &str) {
    let actual = format!("{:?}", token.token_type);
    assert!(
        actual.contains(expected),
        "Expected token type to contain '{}', got: {}",
        expected,
        actual
    );
}

fn count_token_type(tokens: &[Token], type_name: &str) -> usize {
    tokens.iter()
        .filter(|t| format!("{:?}", t.token_type).contains(type_name))
        .count()
}

// ==================== BASIC TOKENIZATION ====================

#[test]
fn test_empty_input() {
    let result = tokenize_input("");

    // Should only have EOF token
    assert_eq!(result.tokens.len(), 1);
    assert!(matches!(result.tokens[0].token_type, TokenType::EndOfFile));
}

#[test]
fn test_whitespace_only() {
    let result = tokenize_input("   \n\t  \r\n  ");

    // Should only have EOF token (whitespace is skipped)
    assert_eq!(result.tokens.len(), 1);
    assert!(matches!(result.tokens[0].token_type, TokenType::EndOfFile));
}

#[test]
fn test_single_identifier() {
    let result = tokenize_input("hello");

    assert_eq!(result.tokens.len(), 2); // identifier + EOF
    assert!(matches!(&result.tokens[0].token_type, TokenType::Identifier(id) if id.as_str() == "hello"));
}

#[test]
fn test_multiple_identifiers() {
    let result = tokenize_input("hello world test");

    assert_eq!(result.tokens.len(), 4); // 3 identifiers + EOF
    assert!(matches!(&result.tokens[0].token_type, TokenType::Identifier(id) if id.as_str() == "hello"));
    assert!(matches!(&result.tokens[1].token_type, TokenType::Identifier(id) if id.as_str() == "world"));
    assert!(matches!(&result.tokens[2].token_type, TokenType::Identifier(id) if id.as_str() == "test"));
}

// ==================== KEYWORDS ====================

#[test]
fn test_control_flow_keywords() {
    let input = "if elif else chk miss then return";
    let result = tokenize_input(input);

    let keywords = count_token_type(&result.tokens, "Keyword");
    assert_eq!(keywords, 7);
}

#[test]
fn test_logical_keywords() {
    let input = "and or not";
    let result = tokenize_input(input);

    assert_eq!(result.tokens.len(), 4); // 3 keywords + EOF
    assert_token_type(&result.tokens[0], "and");
    assert_token_type(&result.tokens[1], "or");
    assert_token_type(&result.tokens[2], "not");
}

#[test]
fn test_boolean_keywords() {
    let input = "true false";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::Bool(true)));
    assert!(matches!(&result.tokens[1].token_type, TokenType::Bool(false)));
}

#[test]
fn test_data_type_keywords() {
    let input = "int float double string bool array tuple hex blob regex object timestamp date enum any";
    let result = tokenize_input(input);

    let keywords = count_token_type(&result.tokens, "Keyword");
    assert_eq!(keywords, 15);
}

#[test]
fn test_variable_declaration_keywords() {
    let input = "let const mut";
    let result = tokenize_input(input);

    assert_eq!(result.tokens.len(), 4); // 3 keywords + EOF
    assert_token_type(&result.tokens[0], "let");
    assert_token_type(&result.tokens[1], "const");
    assert_token_type(&result.tokens[2], "mut");
}

// ==================== NUMBERS ====================

#[test]
fn test_integers() {
    let input = "42 -100 0 2147483647";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::Integer(42)));
    assert!(matches!(&result.tokens[1].token_type, TokenType::Integer(-100)));
    assert!(matches!(&result.tokens[2].token_type, TokenType::Integer(0)));
    assert!(matches!(&result.tokens[3].token_type, TokenType::Integer(2147483647)));
}

#[test]
fn test_floats() {
    let input = "3.14f -2.5f 0.0f 42f";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::Float(_)));
    assert!(matches!(&result.tokens[1].token_type, TokenType::Float(_)));
    assert!(matches!(&result.tokens[2].token_type, TokenType::Float(_)));
    assert!(matches!(&result.tokens[3].token_type, TokenType::Float(_)));
}

#[test]
fn test_doubles() {
    let input = "3.141592 -2.718281 0.0";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::Double(_)));
    assert!(matches!(&result.tokens[1].token_type, TokenType::Double(_)));
    assert!(matches!(&result.tokens[2].token_type, TokenType::Double(_)));
}

#[test]
fn test_scientific_notation() {
    let input = "1.23e10 -4.56e-5 7.89e+12 1.5e3f";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::ScientificNotation(_)));
    assert!(matches!(&result.tokens[1].token_type, TokenType::ScientificNotation(_)));
    assert!(matches!(&result.tokens[2].token_type, TokenType::ScientificNotation(_)));
    assert!(matches!(&result.tokens[3].token_type, TokenType::Float(_)));
}

#[test]
fn test_hex_literals() {
    let input = "0xFF 0xDEADBEEF 0x0 0xABCDEF";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::Integer(255)));
    assert!(matches!(&result.tokens[1].token_type, TokenType::Integer(_)));
    assert!(matches!(&result.tokens[2].token_type, TokenType::Integer(0)));
    assert!(matches!(&result.tokens[3].token_type, TokenType::Integer(_)));
}

// ==================== STRINGS ====================

#[test]
fn test_double_quoted_strings() {
    let input = r#""Hello World" "test" """#;
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::String(s) if s.as_str() == "Hello World"));
    assert!(matches!(&result.tokens[1].token_type, TokenType::String(s) if s.as_str() == "test"));
    assert!(matches!(&result.tokens[2].token_type, TokenType::String(s) if s.as_str() == ""));
}

#[test]
fn test_single_quoted_strings() {
    let input = "'Hello' 'test' ''";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::StringSingle(s) if s.as_str() == "Hello"));
    assert!(matches!(&result.tokens[1].token_type, TokenType::StringSingle(s) if s.as_str() == "test"));
    assert!(matches!(&result.tokens[2].token_type, TokenType::StringSingle(s) if s.as_str() == ""));
}

#[test]
fn test_string_escapes() {
    let input = r#""Line1\nLine2\tTabbed" "Quote: \"Hello\"" "Backslash: \\""#;
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::String(s) if s.contains('\n')));
    assert!(matches!(&result.tokens[1].token_type, TokenType::String(s) if s.contains('"')));
    assert!(matches!(&result.tokens[2].token_type, TokenType::String(s) if s.contains('\\')));
}

#[test]
fn test_interpolated_strings_in_quickfuncs() {
    let input = r#"@QUICKFUNCS(
        ~test<string> => global() {
            let x = $"Value: {42}";
            return x;
        }
    )"#;
    let result = tokenize_input(input);

    let interpolated = count_token_type(&result.tokens, "InterpolatedString");
    assert_eq!(interpolated, 1);
}

// ==================== DATES AND TIMESTAMPS ====================

#[test]
fn test_dates() {
    let input = "2025-01-15 2024-02-29 2025-12-31";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::Date(d) if d.as_str() == "2025-01-15"));
    assert!(matches!(&result.tokens[1].token_type, TokenType::Date(d) if d.as_str() == "2024-02-29"));
    assert!(matches!(&result.tokens[2].token_type, TokenType::Date(d) if d.as_str() == "2025-12-31"));
}

#[test]
fn test_timestamps() {
    let input = "2025-01-15T10:30:00Z 2025-01-15T10:30:00.123Z 2025-01-15T10:30:00+05:30";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::Timestamp(_)));
    assert!(matches!(&result.tokens[1].token_type, TokenType::Timestamp(_)));
    assert!(matches!(&result.tokens[2].token_type, TokenType::Timestamp(_)));
}

// ==================== HEX COLORS ====================

#[test]
fn test_hex_colors() {
    let input = "#F00 #FF0000 #F00F #FF0000FF #80FFFFFF";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::HexColor(c) if c.as_str() == "#F00"));
    assert!(matches!(&result.tokens[1].token_type, TokenType::HexColor(c) if c.as_str() == "#FF0000"));
    assert!(matches!(&result.tokens[2].token_type, TokenType::HexColor(c) if c.as_str() == "#F00F"));
    assert!(matches!(&result.tokens[3].token_type, TokenType::HexColor(c) if c.as_str() == "#FF0000FF"));
    assert!(matches!(&result.tokens[4].token_type, TokenType::HexColor(c) if c.as_str() == "#80FFFFFF"));
}

// ==================== COMMENTS ====================

#[test]
fn test_single_line_comments() {
    let input = "// This is a comment\nlet x = 42";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::Comment(_)));
    assert!(matches!(&result.tokens[1].token_type, TokenType::Keyword(_)));
}

#[test]
fn test_multi_line_comments() {
    let input = "/* This is a\nmulti-line\ncomment */\nlet x = 42";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::Comment(_)));
    assert!(matches!(&result.tokens[1].token_type, TokenType::Keyword(_)));
}

#[test]
fn test_unterminated_comment_halt_mode() {
    let input = "/* This comment never ends";
    let result = tokenize_input(input);

    // Should have partial comment + EOF
    assert!(result.tokens.len() >= 1);
}

// ==================== SECTION KEYWORDS ====================

#[test]
fn test_section_keywords() {
    let input = "@CONFIG @IMPORTS @DLM @ENUMS @QUICKFUNCS @DATA @SECURITY";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::SectionConfig));
    assert!(matches!(&result.tokens[1].token_type, TokenType::SectionImports));
    assert!(matches!(&result.tokens[2].token_type, TokenType::SectionDLM));
    assert!(matches!(&result.tokens[3].token_type, TokenType::SectionEnums));
    assert!(matches!(&result.tokens[4].token_type, TokenType::SectionQuickFuncs));
    assert!(matches!(&result.tokens[5].token_type, TokenType::SectionData));
    assert!(matches!(&result.tokens[6].token_type, TokenType::SectionSecurity));
}

#[test]
fn test_section_context_tracking() {
    let input = "@CONFIG version -> \"1.0.0\"";
    let result = tokenize_input(input);

    // First token should be SectionConfig
    assert!(matches!(&result.tokens[0].token_type, TokenType::SectionConfig));

    // Subsequent tokens should have CONFIG section context
    assert_eq!(result.tokens[1].section.as_deref(), Some("CONFIG"));
}

// ==================== OPERATORS ====================

#[test]
fn test_arithmetic_operators() {
    let input = "+ - * / % ** ++ -- += -= *= /= %= %% %& &%";
    let result = tokenize_input(input);

    let arithmetic = count_token_type(&result.tokens, "ArithmeticOp");
    let arithmetic_assign = count_token_type(&result.tokens, "ArithmeticAssignOp");

    assert!(arithmetic + arithmetic_assign >= 10);
}

#[test]
fn test_comparison_operators() {
    let input = "== != < > <= >=";
    let result = tokenize_input(input);
    print_tokens(&result.tokens); // <-- ADD THIS

    let comparisons = count_token_type(&result.tokens, "ComparisonOp");
    assert_eq!(comparisons, 6);
}

#[test]
fn test_logical_operators() {
    let input = "&& ||";
    let result = tokenize_input(input);

    let logical = count_token_type(&result.tokens, "LogicalOp");
    assert_eq!(logical, 2);
}

#[test]
fn test_bitwise_operators() {
    let input = "& | ^ ~ << >> <<= >>= &= |= ^= ~? >_<";
    let result = tokenize_input(input);

    let bitwise = count_token_type(&result.tokens, "BitwiseOp");
    assert!(bitwise >= 10);
}

#[test]
fn test_special_operators() {
    let input = "=> :: ->";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::Arrow));
    assert!(matches!(&result.tokens[1].token_type, TokenType::DoubleColon));
    assert!(matches!(&result.tokens[2].token_type, TokenType::SwitchCase));
}

// ==================== PREFIXED CONSTRUCTORS ====================

#[test]
fn test_blob_constructor() {
    let input = r#"b:("SGVsbG8gV29ybGQ=")"#;
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::BlobConstructor(_)));
    assert_eq!(result.metadata.blob_constructors, 1);
}

#[test]
fn test_tuple_constructor() {
    let input = "t:(42, \"text\", true)";
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::TupleConstructor(_)));
    assert_eq!(result.metadata.tuple_constructors, 1);
}

#[test]
fn test_regex_constructor() {
    let input = r#"r:("^[a-z]+$")"#;
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::RegexConstructor(_)));
    assert_eq!(result.metadata.regex_constructors, 1);
}

#[test]
fn test_all_prefixed_constructors() {
    let input = r#"b:("data") t:(1, 2) r:("pattern")"#;
    let result = tokenize_input(input);

    assert_eq!(result.prefixed_constructors.len(), 3);
    assert_eq!(result.metadata.blob_constructors, 1);
    assert_eq!(result.metadata.tuple_constructors, 1);
    assert_eq!(result.metadata.regex_constructors, 1);
}

// ==================== SYMBOLS ====================

#[test]
fn test_common_symbols() {
    let input = "( ) { } [ ] , ; : . = ! ?";
    let result = tokenize_input(input);

    let symbols = count_token_type(&result.tokens, "Symbol");
    assert_eq!(symbols, 13);
}

// ==================== STATIC CALL DETECTION ====================

#[test]
fn test_static_call_detection() {
    let input = "DateTime.now() Math.abs(x) String.format(s)";
    let result = tokenize_input(input);

    // Should detect 3 potential static calls
    assert_eq!(result.static_calls.len(), 3);
    assert_eq!(result.static_calls[0].object_name.as_str(), "DateTime");
    assert_eq!(result.static_calls[0].method_name.as_str(), "now");
}

#[test]
fn test_builtin_call_detection() {
    let input = "x.length() array.first() obj.toString()";
    let result = tokenize_input(input);

    // Should detect potential builtin calls (. Identifier ()
    assert!(result.metadata.potential_builtin_calls >= 3);
}

// ==================== METADATA ====================

#[test]
fn test_metadata_generation() {
    let input = r#"
        @CONFIG(version -> "1.0.0")
        @DATA(x = 42)
    "#;
    let result = tokenize_input(input);

    assert_eq!(result.metadata.version.as_str(), "1.0.0");
    assert!(result.metadata.sections_detected.contains(&"CONFIG".to_string()));
    assert!(result.metadata.sections_detected.contains(&"DATA".to_string()));
    assert!(result.metadata.total_lines >= 2);
}

// ==================== FILE TESTS ====================

#[test]
fn test_all_datatypes_mdix_file() {
    let file_content = std::fs::read_to_string("mdix_files/advanced/all_datatypes_test.mdix")
        .expect("Failed to read all_datatypes_test.mdix - make sure the file exists at mdix_files/advanced/all_datatypes_test.mdix");

    let result = tokenize_input(&file_content);

    // Should have many tokens
    assert!(result.tokens.len() > 100, "Expected > 100 tokens, got {}", result.tokens.len());

    // Should detect all sections
    assert!(result.metadata.sections_detected.contains(&"CONFIG".to_string()));
    assert!(result.metadata.sections_detected.contains(&"ENUMS".to_string()));
    assert!(result.metadata.sections_detected.contains(&"QUICKFUNCS".to_string()));
    assert!(result.metadata.sections_detected.contains(&"DATA".to_string()));

    // Should have various token types
    assert!(count_token_type(&result.tokens, "Integer") > 0);
    assert!(count_token_type(&result.tokens, "Float") > 0);
    assert!(count_token_type(&result.tokens, "Double") > 0);
    assert!(count_token_type(&result.tokens, "String") > 0);
    assert!(count_token_type(&result.tokens, "Bool") > 0);
}

// ==================== ERROR HANDLING ====================

#[test]
fn test_invalid_character_detection() {
    let input = "let x = @ 42";
    let result = tokenize_input(input);

    // Should have error token or handle gracefully
    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();
}

#[test]
fn test_unterminated_string() {
    let input = r#"let x = "unterminated"#;
    let result = tokenize_input(input);

    // Should handle unterminated string
    assert!(result.tokens.len() > 0);
}

// ==================== COMPLEX SCENARIOS ====================

#[test]
fn test_complex_expression() {
    let input = "let result = (a + b) * c - d / e ** f;";
    let result = tokenize_input(input);

    assert!(count_token_type(&result.tokens, "Keyword") > 0);
    assert!(count_token_type(&result.tokens, "Identifier") > 0);
    assert!(count_token_type(&result.tokens, "ArithmeticOp") > 0);
    assert!(count_token_type(&result.tokens, "Symbol") > 0);
}

#[test]
fn test_function_definition() {
    let input = r#"
        ~myFunc<int> => global(x<int>, y<int>) {
            return x + y;
        }
    "#;
    let result = tokenize_input(input);

    assert!(count_token_type(&result.tokens, "Identifier") > 0);
    assert!(count_token_type(&result.tokens, "Keyword") > 0);
    assert!(matches!(&result.tokens.iter().find(|t| matches!(t.token_type, TokenType::Arrow)).unwrap().token_type, TokenType::Arrow));
}

#[test]
fn test_data_section_with_objects() {
    let input = r#"
        @DATA(
            user = {
                id = 1,
                name = "John",
                active = true,
                score = 95.5f
            }
        )
    "#;
    let result = tokenize_input(input);

    assert!(matches!(&result.tokens[0].token_type, TokenType::SectionData));
    assert!(count_token_type(&result.tokens, "String") > 0);
    assert!(count_token_type(&result.tokens, "Integer") > 0);
    assert!(count_token_type(&result.tokens, "Float") > 0);
    assert!(count_token_type(&result.tokens, "Bool") > 0);
}

// ==================== PERFORMANCE TESTS ====================

#[test]
fn test_tokenization_speed_small_input() {
    let input = r#"
        @CONFIG(version -> "1.0.0")
        @DATA(x = 42, y = "test", z = true)
    "#;

    let start = Instant::now();
    let result = tokenize_input(input);
    let duration = start.elapsed();

    println!("Small input: {} tokens in {:?} ({:.2} tokens/ms)",
             result.tokens.len(),
             duration,
             result.tokens.len() as f64 / duration.as_secs_f64() / 1000.0
    );

    // Should be very fast (< 1ms)
    assert!(duration.as_millis() < 10);
}

#[test]
fn test_tokenization_speed_medium_input() {
    // Generate medium-sized input (100 statements)
    let mut input = String::from("@DATA(\n");
    for i in 0..100 {
        input.push_str(&format!("    var{} = {},\n", i, i * 2));
    }
    input.push_str(")");

    let start = Instant::now();
    let result = tokenize_input(&input);
    let duration = start.elapsed();

    println!("Medium input: {} tokens in {:?} ({:.2} tokens/ms)",
             result.tokens.len(),
             duration,
             result.tokens.len() as f64 / duration.as_secs_f64() / 1000.0
    );

    // Should complete in < 50ms
    assert!(duration.as_millis() < 50);
    assert!(result.tokens.len() > 300);
}

#[test]
fn test_tokenization_speed_large_input() {
    // Generate large input (1000 statements)
    let mut input = String::from("@DATA(\n");
    for i in 0..1000 {
        input.push_str(&format!("    variable_{} = {},\n", i, i * 2));
    }
    input.push_str(")");

    let start = Instant::now();
    let result = tokenize_input(&input);
    let duration = start.elapsed();

    let tokens_per_sec = result.tokens.len() as f64 / duration.as_secs_f64();

    println!("Large input: {} tokens in {:?} ({:.0} tokens/sec)",
             result.tokens.len(),
             duration,
             tokens_per_sec
    );

    // Should complete in < 500ms
    assert!(duration.as_millis() < 500);
    assert!(result.tokens.len() > 3000);

    // Should process at least 10,000 tokens per second
    assert!(tokens_per_sec > 10000.0, "Too slow: {:.0} tokens/sec", tokens_per_sec);
}

#[test]
fn test_tokenization_speed_complex_input() {
    let input = r#"
        @CONFIG(
            version -> "1.0.0",
            features -> "advanced",
            debug_mode -> "verbose"
        )

        @ENUMS(
            Status { ACTIVE = 1, INACTIVE = 2, PENDING = 3 }
        )

        @QUICKFUNCS(
            ~calculate<double> => global(x<double>, y<double>, z<double>) {
                let sum = x + y + z;
                let product = x * y * z;
                let average = sum / 3.0;
                return average * product;
            },

            ~processData<string> => global(data<array>) {
                let len = data.length();
                if: len > 0 {
                    return $"Processed {len} items";
                }
                else {
                    return "No data";
                }
            }
        )

        @DATA(
            // Various data types
            int_val<int> = 42,
            float_val<float> = 3.14f,
            double_val<double> = 2.718281828,
            string_val<string> = "Hello, World!",
            bool_val<bool> = true,
            hex_val<hex> = 0xFF00AA,
            color<hex> = #FF0000,
            date_val<date> = 2025-01-15,
            timestamp_val<timestamp> = 2025-01-15T10:30:00Z,

            // Collections
            numbers = [1, 2, 3, 4, 5],
            strings = ["apple", "banana", "cherry"],

            // Objects
            user = {
                id = 1,
                name = "John Doe",
                email = "john@example.com",
                active = true,
                score = 95.5f
            },

            // Prefixed constructors
            binary_data = b:("SGVsbG8gV29ybGQ="),
            coordinates = t:(10.5f, 20.3f, 30.1f),
            pattern = r:("^[a-z]+$"),

            // Function calls
            calc_result = calculate(10.5, 20.3, 30.1),
            processed = processData([1, 2, 3, 4, 5])
        )
    "#;

    let start = Instant::now();
    let result = tokenize_input(input);
    let duration = start.elapsed();

    let tokens_per_sec = result.tokens.len() as f64 / duration.as_secs_f64();

    println!("Complex input: {} tokens in {:?} ({:.0} tokens/sec)",
             result.tokens.len(),
             duration,
             tokens_per_sec
    );

    // Should complete quickly
    assert!(duration.as_millis() < 100);

    // Should process at least 5,000 tokens per second
    assert!(tokens_per_sec > 5000.0, "Too slow: {:.0} tokens/sec", tokens_per_sec);
}

#[test]
fn test_tokenization_speed_with_comments() {
    let mut input = String::from("@DATA(\n");
    for i in 0..500 {
        input.push_str(&format!("    // Comment for variable {}\n", i));
        input.push_str(&format!("    var{} = {},\n", i, i));
    }
    input.push_str(")");

    let start = Instant::now();
    let result = tokenize_input(&input);
    let duration = start.elapsed();

    println!("With comments: {} tokens in {:?}", result.tokens.len(), duration);

    // Should handle comments efficiently
    assert!(duration.as_millis() < 200);

    // Should have detected all comments
    let comment_count = count_token_type(&result.tokens, "Comment");
    assert_eq!(comment_count, 500);
}

#[test]
fn test_tokenization_throughput() {
    // Test how many characters per second we can tokenize
    let mut input = String::new();
    for i in 0..1000 {
        input.push_str(&format!("let variable_{} = {} + {} * {};", i, i, i+1, i+2));
    }

    let char_count = input.len();

    let start = Instant::now();
    let result = tokenize_input(&input);
    let duration = start.elapsed();

    let chars_per_sec = char_count as f64 / duration.as_secs_f64();
    let mb_per_sec = chars_per_sec / 1_000_000.0;

    println!("Throughput test:");
    println!("  Input size: {} chars", char_count);
    println!("  Tokens: {}", result.tokens.len());
    println!("  Time: {:?}", duration);
    println!("  Throughput: {:.2} MB/sec", mb_per_sec);
    println!("  Speed: {:.0} chars/sec", chars_per_sec);

    // Should process at least 1 MB/sec
    assert!(mb_per_sec > 1.0, "Too slow: {:.2} MB/sec", mb_per_sec);
}

fn print_tokens(tokens: &[Token]) {
    println!("\n=== TOKENS ({}) ===", tokens.len());
    for (i, token) in tokens.iter().enumerate() {
        println!("{:3}: {:?}", i, token);
    }
    println!("===================\n");
}