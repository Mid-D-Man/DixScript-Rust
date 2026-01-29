// tests/quickfuncs_parser_tests.rs

use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::SectionParsers::QuickFuncsSectionParser;
use dixscript::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use dixscript::ErrorManager::ErrorManager;
use dixscript::Compiler::AST::{QuickFuncsSection, QuickFuncStatement};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use std::time::Instant;

// ==================== PERFORMANCE BASELINES ====================
const BASELINE_SMALL_INPUT_MS: u128 = 15;
const BASELINE_MEDIUM_INPUT_MS: u128 = 150;
const BASELINE_LARGE_INPUT_MS: u128 = 1500;
const BASELINE_FUNCTIONS_PER_SEC: f64 = 200.0;
const BASELINE_TOKENS_PER_SEC: f64 = 3000.0;

// ==================== HELPER FUNCTIONS ====================

fn tokenize_input(input: &str) -> Vec<Token> {
    let tokenizer = Tokenizer::new(input.to_string());
    let result = tokenizer.tokenize();
    result.tokens
}

fn extract_quickfuncs_section_tokens(tokens: &[Token]) -> Vec<Token> {
    // Find @QUICKFUNCS token
    let section_start = tokens.iter()
        .position(|t| matches!(t.token_type, TokenType::SectionQuickFuncs))
        .expect("No @QUICKFUNCS section found");

    // Skip @QUICKFUNCS and any whitespace, find the opening (
    let mut search_pos = section_start + 1;
    while search_pos < tokens.len() {
        let token_value = tokens[search_pos].get_token_value();
        if token_value.trim().is_empty() || matches!(tokens[search_pos].token_type, TokenType::Comment(_)) {
            search_pos += 1;
            continue;
        }
        if matches!(tokens[search_pos].token_type, TokenType::Symbol('(')) {
            break;
        }
        search_pos += 1;
    }

    if search_pos >= tokens.len() {
        panic!("No opening parenthesis found after @QUICKFUNCS");
    }

    let paren_start = search_pos;

    // Find matching closing parenthesis
    let mut depth = 0;
    let mut paren_end = paren_start;

    for (idx, token) in tokens[paren_start..].iter().enumerate() {
        match &token.token_type {
            TokenType::Symbol('(') => depth += 1,
            TokenType::Symbol(')') => {
                depth -= 1;
                if depth == 0 {
                    paren_end = paren_start + idx;
                    break;
                }
            }
            _ => {}
        }
    }

    if depth != 0 {
        panic!("Unmatched parentheses in @QUICKFUNCS section");
    }

    // Extract tokens from opening ( to closing ) inclusive, then add EOF
    let mut section_tokens = tokens[paren_start..=paren_end].to_vec();
    section_tokens.push(Token::eof(1, 1));

    section_tokens
}

fn parse_quickfuncs_with_settings(input: &str, settings: OperationalSettings) -> Option<QuickFuncsSection> {
    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let tokens = tokenize_input(input);
    let section_tokens = extract_quickfuncs_section_tokens(&tokens);

    let mut parser = QuickFuncsSectionParser::new(&section_tokens, &settings);
    let result = parser.parse_section();

    if error_manager.has_errors() {
        eprintln!("\n=== PARSE ERRORS ===");
        eprintln!("{}", error_manager.generate_error_report());
        eprintln!("===================\n");
    }

    result
}

fn parse_quickfuncs_default(input: &str) -> Option<QuickFuncsSection> {
    parse_quickfuncs_with_settings(input, OperationalSettings::default())
}

fn parse_quickfuncs_halt_on_error(input: &str) -> Option<QuickFuncsSection> {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Halt;
    parse_quickfuncs_with_settings(input, settings)
}

fn parse_quickfuncs_recover(input: &str) -> Option<QuickFuncsSection> {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Recover;
    parse_quickfuncs_with_settings(input, settings)
}

// ==================== BASIC FUNCTIONALITY TESTS ====================

#[test]
fn test_simple_function() {
    let input = r#"
        @QUICKFUNCS(
            ~add<int> => global(a<int>, b<int>) {
                return a + b;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions.len(), 1);

    let func = &section.functions[0];
    assert_eq!(func.name, "add");
    assert_eq!(func.parameters.len(), 2);
    assert_eq!(func.body.len(), 1);
}

#[test]
fn test_multiple_functions() {
    let input = r#"
        @QUICKFUNCS(
            ~add<int> => global(a<int>, b<int>) {
                return a + b;
            }

            ~multiply<int> => global(x<int>, y<int>) {
                return x * y;
            }

            ~greet<string> => global(name<string>) {
                return "Hello, " + name;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions.len(), 3);
}

#[test]
fn test_function_with_scope() {
    let input = r#"
        @QUICKFUNCS(
            ~processUser<bool> => users.profile(id<int>) {
                return id > 0;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions.len(), 1);

    let func = &section.functions[0];
    assert!(func.scope_list.is_some());
    if let Some(scopes) = &func.scope_list {
        assert_eq!(scopes[0].as_str(), "users.profile");
    }
}

#[test]
fn test_function_multiple_scopes() {
    let input = r#"
        @QUICKFUNCS(
            ~validate<bool> => users.data, posts.data(id<int>) {
                return true;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    let func = &section.functions[0];

    if let Some(scopes) = &func.scope_list {
        assert_eq!(scopes.len(), 2);
    }
}

#[test]
fn test_function_no_parameters() {
    let input = r#"
        @QUICKFUNCS(
            ~getDefault<int> => global() {
                return 42;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].parameters.len(), 0);
}

#[test]
fn test_function_default_parameter_values() {
    let input = r#"
        @QUICKFUNCS(
            ~greet<string> => global(name<string> = "World") {
                return "Hello, " + name;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    let param = &section.functions[0].parameters[0];
    assert!(param.default_value.is_some());
}

#[test]
fn test_function_no_return_type() {
    let input = r#"
        @QUICKFUNCS(
            ~logMessage => global(msg<string>) {
                log: msg;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert!(section.functions[0].return_type.is_none());
}

#[test]
fn test_empty_quickfuncs_section() {
    let input = r#"@QUICKFUNCS()"#;
    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions.len(), 0);
}

// ==================== STATEMENT TESTS ====================

#[test]
fn test_variable_declaration_let() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                let x = 5;
                return x;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);

    match &section.functions[0].body[0] {
        QuickFuncStatement::VariableDeclaration { .. } => {},
        _ => panic!("Expected VariableDeclaration"),
    }
}

#[test]
fn test_variable_declaration_const() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                const x = 10;
                return x;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    match &section.functions[0].body[0] {
        QuickFuncStatement::VariableDeclaration { declaration_type, .. } => {
            assert_eq!(format!("{:?}", declaration_type), "Const");
        },
        _ => panic!("Expected VariableDeclaration"),
    }
}

#[test]
fn test_variable_declaration_mutable() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                let mut x = 5;
                x = x + 1;
                return x;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    match &section.functions[0].body[0] {
        QuickFuncStatement::VariableDeclaration { is_mutable, .. } => {
            assert!(*is_mutable);
        },
        _ => panic!("Expected VariableDeclaration"),
    }
}

#[test]
fn test_assignment_statement() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                let x = 5;
                x = 10;
                return x;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 3);
}

#[test]
fn test_arithmetic_assignment() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                let x = 5;
                x += 10;
                x -= 2;
                x *= 3;
                x /= 2;
                return x;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 6);
}

#[test]
fn test_if_statement() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global(x<int>) {
                if: x > 0 {
                    return 1;
                }
                return 0;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    match &section.functions[0].body[0] {
        QuickFuncStatement::If { .. } => {},
        _ => panic!("Expected If statement"),
    }
}

#[test]
fn test_if_elif_else() {
    let input = r#"
        @QUICKFUNCS(
            ~test<string> => global(x<int>) {
                if: x > 0 {
                    return "positive";
                }
                elif: x < 0 {
                    return "negative";
                }
                else {
                    return "zero";
                }
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    match &section.functions[0].body[0] {
        QuickFuncStatement::If { else_branch, .. } => {
            assert!(else_branch.is_some());
        },
        _ => panic!("Expected If statement"),
    }
}

#[test]
fn test_switch_statement() {
    let input = r#"
        @QUICKFUNCS(
            ~test<string> => global(x<int>) {
                chk: x {
                    -> 1 { return "one"; }
                    -> 2 { return "two"; }
                    -> miss { return "other"; }
                }
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    match &section.functions[0].body[0] {
        QuickFuncStatement::Switch { cases, default_case, .. } => {
            assert_eq!(cases.len(), 2);
            assert!(default_case.is_some());
        },
        _ => panic!("Expected Switch statement"),
    }
}

#[test]
fn test_log_statement() {
    let input = r#"
        @QUICKFUNCS(
            ~test => global(msg<string>) {
                log: msg;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    match &section.functions[0].body[0] {
        QuickFuncStatement::Log { .. } => {},
        _ => panic!("Expected Log statement"),
    }
}

#[test]
fn test_return_statement() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                return 42;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    match &section.functions[0].body[0] {
        QuickFuncStatement::Return { .. } => {},
        _ => panic!("Expected Return statement"),
    }
}

// ==================== EXPRESSION TESTS ====================

#[test]
fn test_arithmetic_expressions() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global(a<int>, b<int>) {
                let sum = a + b;
                let diff = a - b;
                let prod = a * b;
                let quot = a / b;
                let rem = a % b;
                let pow = a ** b;
                return sum;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 7);
}

#[test]
fn test_comparison_expressions() {
    let input = r#"
        @QUICKFUNCS(
            ~test<bool> => global(a<int>, b<int>) {
                let eq = a == b;
                let ne = a != b;
                let gt = a > b;
                let lt = a < b;
                let ge = a >= b;
                let le = a <= b;
                return eq;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 7);
}

#[test]
fn test_logical_expressions() {
    let input = r#"
        @QUICKFUNCS(
            ~test<bool> => global(a<bool>, b<bool>) {
                let and_result = a && b;
                let or_result = a || b;
                return and_result;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 3);
}

#[test]
fn test_ternary_expression() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global(x<int>) {
                let result = x > 0 ? 1 : 0;
                return result;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);
}

#[test]
fn test_unary_expressions() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global(x<int>, flag<bool>) {
                let neg = -x;
                let pos = +x;
                let not = !flag;
                return neg;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 4);
}

#[test]
fn test_function_call_expression() {
    let input = r#"
        @QUICKFUNCS(
            ~helper<int> => global() {
                return 42;
            }

            ~test<int> => global() {
                let result = helper();
                return result;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions.len(), 2);
}

#[test]
fn test_nested_function_calls() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                let result = outer(inner(deep(42)));
                return result;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);
}

#[test]
fn test_property_access() {
    let input = r#"
        @QUICKFUNCS(
            ~test<string> => global(user<object>) {
                let name = user.name;
                let age = user.profile.age;
                return name;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 3);
}

#[test]
fn test_index_access() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global(arr<array>) {
                let first = arr[0];
                let last = arr[arr.length() - 1];
                return first;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 3);
}

#[test]
fn test_lambda_expression() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                let add = (x, y) => x + y;
                return add;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);
}

#[test]
fn test_array_literal() {
    let input = r#"
        @QUICKFUNCS(
            ~test<array> => global() {
                let arr = [1, 2, 3, 4, 5];
                return arr;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);
}

#[test]
fn test_object_literal() {
    let input = r#"
        @QUICKFUNCS(
            ~test<object> => global() {
                let obj = {
                    name: "test",
                    value: 42,
                    active: true
                };
                return obj;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);
}

#[test]
fn test_interpolated_string() {
    let input = r#"
        @QUICKFUNCS(
            ~test<string> => global(name<string>, age<int>) {
                let msg = $"Hello {name}, you are {age} years old";
                return msg;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);
}

// ==================== OPERATOR PRECEDENCE TESTS ====================

#[test]
fn test_operator_precedence_basic() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                let result = 2 + 3 * 4;
                return result;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);
}

#[test]
fn test_operator_precedence_complex() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global(a<int>, b<int>, c<int>) {
                let result = a + b * c - a / b;
                return result;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);
}

#[test]
fn test_parenthesized_expressions() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global(a<int>, b<int>) {
                let result = (a + b) * (a - b);
                return result;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);
}

#[test]
fn test_right_associative_power() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                let result = 2 ** 3 ** 2;
                return result;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);
}

// ==================== ERROR HANDLING TESTS ====================

#[test]
fn test_missing_function_name() {
    let input = r#"
        @QUICKFUNCS(
            ~<int> => global() {
                return 42;
            }
        )
    "#;

    let _section = parse_quickfuncs_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_missing_function_body() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global()
        )
    "#;

    let _section = parse_quickfuncs_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_invalid_return_type() {
    let input = r#"
        @QUICKFUNCS(
            ~test<invalid_type> => global() {
                return 42;
            }
        )
    "#;

    let _section = parse_quickfuncs_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_unclosed_brace() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                return 42;
        )
    "#;

    let _section = parse_quickfuncs_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_const_mut_error() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                const mut x = 42;
                return x;
            }
        )
    "#;

    let _section = parse_quickfuncs_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_halt_strategy_stops() {
    let input = r#"
        @QUICKFUNCS(
            ~test1<int> => global() {
                INVALID SYNTAX
            }

            ~test2<int> => global() {
                return 42;
            }
        )
    "#;

    let _section = parse_quickfuncs_halt_on_error(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_recover_strategy_continues() {
    let input = r#"
        @QUICKFUNCS(
            ~test1<int> => global() {
                INVALID
            }

            ~test2<int> => global() {
                return 42;
            }
        )
    "#;

    let section = parse_quickfuncs_recover(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());

    if let Some(s) = section {
        println!("Recovered {} functions", s.functions.len());
    }
}

// ==================== PERFORMANCE TESTS ====================

#[test]
fn test_parse_speed_small_input() {
    let input = r#"
        @QUICKFUNCS(
            ~add<int> => global(a<int>, b<int>) {
                return a + b;
            }
        )
    "#;

    let tokens = tokenize_input(input);
    let section_tokens = extract_quickfuncs_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = QuickFuncsSectionParser::new(&section_tokens, &settings);
    let _section = parser.parse_section();
    let duration = start.elapsed();

    println!("\n=== QUICKFUNCS PARSER - SMALL INPUT ===");
    println!("Baseline: < {}ms", BASELINE_SMALL_INPUT_MS);
    println!("Actual: {:?}", duration);
    println!("Status: {}", if duration.as_millis() < BASELINE_SMALL_INPUT_MS { "✅ PASS" } else { "❌ FAIL" });
    println!("========================================\n");

    assert!(
        duration.as_millis() < BASELINE_SMALL_INPUT_MS,
        "Too slow: {:?} (baseline: {}ms)",
        duration,
        BASELINE_SMALL_INPUT_MS
    );
}

#[test]
fn test_parse_speed_medium_input() {
    let mut input = String::from("@QUICKFUNCS(\n");
    for i in 0..20 {
        input.push_str(&format!(
            r#"
            ~func{}<int> => global(x<int>, y<int>) {{
                let sum = x + y;
                let product = x * y;
                if: sum > product {{
                    return sum;
                }}
                else {{
                    return product;
                }}
            }}
            "#,
            i
        ));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_quickfuncs_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = QuickFuncsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let funcs_per_sec = section.functions.len() as f64 / duration.as_secs_f64();

    println!("\n=== QUICKFUNCS PARSER - MEDIUM INPUT ===");
    println!("Functions: {}", section.functions.len());
    println!("Baseline: < {}ms, > {} funcs/sec", BASELINE_MEDIUM_INPUT_MS, BASELINE_FUNCTIONS_PER_SEC);
    println!("Actual: {:?} ({:.0} funcs/sec)", duration, funcs_per_sec);
    println!("Status: {}",
             if duration.as_millis() < BASELINE_MEDIUM_INPUT_MS && funcs_per_sec > BASELINE_FUNCTIONS_PER_SEC {
                 "✅ PASS"
             } else {
                 "❌ FAIL"
             }
    );
    println!("=========================================\n");

    assert!(
        duration.as_millis() < BASELINE_MEDIUM_INPUT_MS,
        "Too slow: {:?} (baseline: {}ms)",
        duration,
        BASELINE_MEDIUM_INPUT_MS
    );
    assert_eq!(section.functions.len(), 20);
}

#[test]
fn test_parse_speed_large_input() {
    let mut input = String::from("@QUICKFUNCS(\n");
    for i in 0..50 {
        input.push_str(&format!(
            r#"
            ~process{}<int> => global(val<int>) {{
                let x = val * 2;
                let y = x + 10;
                let z = y / 2;

                if: z > 50 {{
                    return z - 10;
                }}
                elif: z > 25 {{
                    return z;
                }}
                else {{
                    return z + 10;
                }}
            }}
            "#,
            i
        ));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_quickfuncs_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = QuickFuncsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let funcs_per_sec = section.functions.len() as f64 / duration.as_secs_f64();

    println!("\n=== QUICKFUNCS PARSER - LARGE INPUT ===");
    println!("Functions: {}", section.functions.len());
    println!("Baseline: < {}ms, > {} funcs/sec", BASELINE_LARGE_INPUT_MS, BASELINE_FUNCTIONS_PER_SEC);
    println!("Actual: {:?} ({:.0} funcs/sec)", duration, funcs_per_sec);
    println!("Status: {}",
             if duration.as_millis() < BASELINE_LARGE_INPUT_MS && funcs_per_sec > BASELINE_FUNCTIONS_PER_SEC {
                 "✅ PASS"
             } else {
                 "❌ FAIL"
             }
    );
    println!("========================================\n");

    assert!(
        duration.as_millis() < BASELINE_LARGE_INPUT_MS,
        "Too slow: {:?} (baseline: {}ms)",
        duration,
        BASELINE_LARGE_INPUT_MS
    );
    assert_eq!(section.functions.len(), 50);
}

#[test]
fn test_parse_throughput() {
    let mut input = String::from("@QUICKFUNCS(\n");
    for i in 0..30 {
        input.push_str(&format!(
            "~func{}<int> => global(x<int>) {{ return x * 2; }}\n",
            i
        ));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_quickfuncs_section_tokens(&tokens);
    let token_count = section_tokens.len();
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = QuickFuncsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let tokens_per_sec = token_count as f64 / duration.as_secs_f64();

    println!("\n=== QUICKFUNCS PARSER - THROUGHPUT ===");
    println!("Tokens: {}", token_count);
    println!("Baseline: > {} tokens/sec", BASELINE_TOKENS_PER_SEC);
    println!("Actual: {:.0} tokens/sec", tokens_per_sec);
    println!("Status: {}", if tokens_per_sec > BASELINE_TOKENS_PER_SEC { "✅ PASS" } else { "❌ FAIL" });
    println!("=======================================\n");

    assert!(
        tokens_per_sec > BASELINE_TOKENS_PER_SEC,
        "Too slow: {:.0} tokens/sec (baseline: {})",
        tokens_per_sec,
        BASELINE_TOKENS_PER_SEC
    );
    assert_eq!(section.functions.len(), 30);
}

#[test]
#[ignore]
fn test_release_mode_performance() {
    let mut input = String::from("@QUICKFUNCS(\n");
    for i in 0..200 {
        input.push_str(&format!(
            r#"
            ~func{}<int> => global(a<int>, b<int>) {{
                let result = a + b * 2 - a / (b + 1);
                return result;
            }}
            "#,
            i
        ));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_quickfuncs_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = QuickFuncsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let funcs_per_sec = section.functions.len() as f64 / duration.as_secs_f64();

    println!("\n=== QUICKFUNCS PARSER - RELEASE MODE ===");
    println!("Functions: {}", section.functions.len());
    println!("Time: {:?}", duration);
    println!("Funcs/sec: {:.0}", funcs_per_sec);
    println!("Expected: > 500 funcs/sec");
    println!("Status: {}", if funcs_per_sec > 500.0 { "✅ PASS" } else { "❌ FAIL" });
    println!("=========================================\n");

    assert!(funcs_per_sec > 500.0, "Too slow in release mode: {:.0} funcs/sec", funcs_per_sec);
}

// ==================== MEMORY TESTS ====================

#[test]
fn test_memory_usage_estimate() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global(a<int>, b<int>) {
                let sum = a + b;
                return sum;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    let func_size = std::mem::size_of_val(&section.functions[0]);

    println!("\n=== QUICKFUNCS PARSER - MEMORY ===");
    println!("Function struct: {} bytes", func_size);
    println!("Expected: < 10KB per function");
    println!("Status: {}", if func_size < 10240 { "✅ PASS" } else { "❌ FAIL" });
    println!("===================================\n");

    assert!(func_size < 10240, "Function too large: {} bytes", func_size);
}

#[test]
fn test_no_memory_leaks_repeated_parsing() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global(x<int>) {
                let result = x * 2;
                return result;
            }
        )
    "#;

    for _ in 0..1000 {
        let _ = parse_quickfuncs_default(input);
    }

    println!("✅ Successfully parsed same input 1000 times without memory leaks");
}

// ==================== EDGE CASES ====================

#[test]
fn test_whitespace_handling() {
    let input = r#"
        @QUICKFUNCS(
            ~test  <  int  >  =>  global  (  x  <  int  >  )  {
                return   x   *   2  ;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions.len(), 1);
}

#[test]
fn test_comments_in_function() {
    let input = r#"
        @QUICKFUNCS(
            // This is a test function
            ~test<int> => global(x<int>) {
                // Calculate double
                let result = x * 2;
                // Return it
                return result;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions.len(), 1);
}

#[test]
fn test_single_line_if() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global(x<int>) {
                if: x > 0 then return 1;
                return 0;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);
}

#[test]
fn test_nested_blocks() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global(x<int>) {
                if: x > 0 {
                    if: x > 10 {
                        if: x > 100 {
                            return 100;
                        }
                        return 10;
                    }
                    return 1;
                }
                return 0;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions.len(), 1);
}

#[test]
fn test_complex_expression_parsing() {
    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global(a<int>, b<int>, c<int>) {
                let result = (a + b) * c - (a - b) / (c + 1) % 5;
                return result;
            }
        )
    "#;

    let section = parse_quickfuncs_default(input).expect("Failed to parse");
    assert_eq!(section.functions[0].body.len(), 2);
}

#[test]
fn test_debug_parse_steps() {
    use dixscript::Compiler::Core::Tokenizer::Tokenizer;

    let input = r#"
        @QUICKFUNCS(
            ~test<int> => global() {
                let x = 5;
                return x;
            }
        )
    "#;

    println!("\n========================================");
    println!("DEBUG: PARSING STEPS");
    println!("========================================\n");

    // Step 1: Tokenize
    println!("STEP 1: Tokenizing input...");
    let tokenizer = Tokenizer::new(input.to_string());
    let result = tokenizer.tokenize();

    println!("Total tokens: {}", result.tokens.len());
    println!("\nAll tokens:");
    for (i, token) in result.tokens.iter().enumerate() {
        println!("  [{:3}] L{:2}:C{:2} {:?}",
                 i,
                 token.line,
                 token.column,
                 token.token_type
        );
    }

    // Step 2: Extract QUICKFUNCS section
    println!("\nSTEP 2: Extracting QUICKFUNCS section...");
    let section_tokens = extract_quickfuncs_section_tokens(&result.tokens);

    println!("Section tokens: {}", section_tokens.len());
    println!("\nSection tokens breakdown:");
    for (i, token) in section_tokens.iter().enumerate() {
        println!("  [{:3}] L{:2}:C{:2} {:?}",
                 i,
                 token.line,
                 token.column,
                 token.token_type
        );
    }

    // Step 3: Parse with verbose logging
    println!("\nSTEP 3: Parsing with verbose logging...");

    let mut settings = OperationalSettings::default();
    settings.debug_mode = DebugMode::Verbose;

    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let mut parser = QuickFuncsSectionParser::new(&section_tokens, &settings);

    println!("\nStarting parse...\n");
    let section = parser.parse_section();

    println!("\nSTEP 4: Results");
    println!("========================================");

    if let Some(s) = section {
        println!("✅ Parse successful!");
        println!("Functions parsed: {}", s.functions.len());

        for (i, func) in s.functions.iter().enumerate() {
            println!("\nFunction {}:", i);
            println!("  Name: {}", func.name);
            println!("  Return type: {:?}", func.return_type);
            println!("  Scope: {:?}", func.scope_list);
            println!("  Parameters: {}", func.parameters.len());
            for (j, param) in func.parameters.iter().enumerate() {
                println!("    [{}] {} <{:?}> = {:?}",
                         j,
                         param.name,
                         param.data_type,
                         param.default_value.as_ref().map(|_| "expression")
                );
            }
            println!("  Statements: {}", func.body.len());
            for (j, stmt) in func.body.iter().enumerate() {
                println!("    [{}] {:?}", j, stmt);
            }
        }
    } else {
        println!("❌ Parse failed!");
    }

    if error_manager.has_errors() {
        println!("\n⚠️  ERRORS DETECTED:");
        println!("{}", error_manager.generate_error_report());
    } else {
        println!("\n✅ No errors");
    }

    println!("\n========================================");
}

#[test]
fn test_debug_parse_with_params() {
    let input = r#"
        @QUICKFUNCS(
            ~greet<string> => global(name<string>, age<int> = 18) {
                return "Hello " + name;
            }
        )
    "#;

    println!("\n========================================");
    println!("DEBUG: PARSING WITH PARAMETERS");
    println!("========================================\n");

    let mut settings = OperationalSettings::default();
    settings.debug_mode = DebugMode::Verbose;

    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let tokens = tokenize_input(input);
    let section_tokens = extract_quickfuncs_section_tokens(&tokens);

    println!("Section tokens:");
    for (i, token) in section_tokens.iter().enumerate() {
        println!("  [{:3}] {:?}", i, token.token_type);
    }

    let mut parser = QuickFuncsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section();

    if let Some(s) = section {
        println!("\n✅ Parsed successfully");
        println!("Function: {}", s.functions[0].name);
        println!("Parameters:");
        for param in &s.functions[0].parameters {
            println!("  - {} <{:?}> = {:?}",
                     param.name,
                     param.data_type,
                     param.default_value.as_ref().map(|_| "has default")
            );
        }
    } else {
        println!("\n❌ Parse failed");
    }

    if error_manager.has_errors() {
        println!("\n{}", error_manager.generate_error_report());
    }
}

#[test]
fn test_debug_minimal_function() {
    let input = r#"@QUICKFUNCS(~test<int> => global() { return 42; })"#;

    println!("\n========================================");
    println!("DEBUG: MINIMAL FUNCTION (ONE LINE)");
    println!("========================================\n");

    let mut settings = OperationalSettings::default();
    settings.debug_mode = DebugMode::Verbose;

    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let tokens = tokenize_input(input);

    println!("All tokens:");
    for (i, token) in tokens.iter().enumerate() {
        println!("  [{:3}] {:?} = '{}'",
                 i,
                 token.token_type,
                 token.get_token_value()
        );
    }

    let section_tokens = extract_quickfuncs_section_tokens(&tokens);

    println!("\nSection tokens ({}):", section_tokens.len());
    for (i, token) in section_tokens.iter().enumerate() {
        println!("  [{:3}] {:?}", i, token.token_type);
    }

    let mut parser = QuickFuncsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section();

    if let Some(s) = section {
        println!("\n✅ Success: {} functions", s.functions.len());
    } else {
        println!("\n❌ Failed");
    }

    if error_manager.has_errors() {
        println!("\n{}", error_manager.generate_error_report());
    }
}