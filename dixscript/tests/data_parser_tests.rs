// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/data_parser_tests.rs"
// ============================================================================
//! Tests for the `@DATA` section parser.
//!
//! The loader tests reach the parser only through `DixLoader`, which cannot show what each error
//! strategy does with malformed input or which diagnostics are reported. These tests pin the
//! parser itself, which is the fourth parser built on the arena AST: what a section parses to
//! (printed with the owned `Display`), the diagnostic messages, and, for one case, the source
//! positions. The cases cover every value kind, the nested lists (arrays, objects, tuples, group
//! arrays and call arguments), the depth limits, and malformed input under all three error
//! strategies.
//!
//! A malformed list inside a list is the case the scratch stacks exist for: the `does_not_leak`
//! tests check that the entries of a list that failed part way never end up in the list that
//! encloses it.
//!
//! Every expectation was recorded from the parser's output and checked against the output of
//! the original, pre-arena parser on the same sources. The strategy is set on the settings after
//! `@CONFIG` is processed, so the sources carry no `error_handling` key.

use dixscript::Compiler::AST::data_types::CompatibilityMode;
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::ErrorHandlingStrategy;
use dixscript::Compiler::Core::GeneralParser;
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::ErrorManager::ErrorManager;

/// What one parse produced: the section printed with `Display` (or a marker when there is none),
/// the compact `Debug` of the section, and the sorted diagnostic messages.
struct Outcome {
    display: String,
    debug: Option<String>,
    messages: Vec<String>,
}

fn run(strategy: &str, source: &str) -> Outcome {
    let strategy = match strategy {
        "halt" => ErrorHandlingStrategy::Halt,
        "continue" => ErrorHandlingStrategy::Continue,
        "recover" => ErrorHandlingStrategy::Recover,
        other => panic!("unknown strategy {other}"),
    };
    let label = "data-parser-test";
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
    settings.error_handling_strategy = strategy;
    em.update_settings(settings.clone());

    let (display, debug) = match GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &settings, em.clone()) {
        Err(_) => ("START ERR".to_string(), None),
        Ok(parser) => match parser.parse() {
            Err(_) => ("PARSE ERR".to_string(), None),
            Ok(ast) => match &ast.data {
                None => ("NONE".to_string(), None),
                Some(data) => (data.to_string(), Some(format!("{:?}", data))),
            },
        },
    };
    let mut messages: Vec<String> = em
        .get_parse_errors_as_string()
        .lines()
        .filter_map(|l| l.trim().strip_prefix("Message: ").map(|m| m.to_string()))
        .collect();
    messages.sort();
    Outcome { display, debug, messages }
}

fn check(case: &str, strategy: &str, source: &str, display: &str, debug: Option<&str>, messages: &[&str]) {
    let got = run(strategy, source);
    assert_eq!(got.display, display, "{case} / {strategy}: the parsed section");
    if let Some(expected) = debug {
        assert_eq!(got.debug.as_deref(), Some(expected), "{case} / {strategy}: the section with positions");
    }
    let expected: Vec<String> = messages.iter().map(|m| m.to_string()).collect();
    assert_eq!(got.messages, expected, "{case} / {strategy}: the diagnostic messages");
}

#[test]
fn scalar_kinds_and_signs() {
    check(
        "scalar_kinds_and_signs",
        "halt",
        "@DATA(\na = 1, b = 2L, c = 1.5f, d = 2.5, e = 1e3, f = \"s\", g = 't', h = true, i = false, j = #FF00AA, k = null, l = -5, m = -2.5\n)\n",
        "@DATA(\n  a = 1,\n  b = 2L,\n  c = 1.5f,\n  d = 2.5,\n  e = 1e3,\n  f = \"s\",\n  g = \"t\",\n  h = true,\n  i = false,\n  j = #FF00AA,\n  k = null,\n  l = -5,\n  m = -2.5\n)",
        None,
        &[],
    );
}

#[test]
fn type_annotations() {
    check(
        "type_annotations",
        "halt",
        "@DATA(\na<int> = 1, b<array<int>> = [1, 2, 3], c<tuple<float,float>> = t:(x, y), d<array<array<int>>> = [[1], [2, 3]]\n)\n",
        "@DATA(\n  a<int> = 1,\n  b<array<int>> = [1, 2, 3],\n  c<tuple<float,float>> = t:(x, y),\n  d<array<array>> = [[1], [2, 3]]\n)",
        None,
        &[],
    );
}

#[test]
fn fused_annotation_end() {
    check(
        "fused_annotation_end",
        "halt",
        "@DATA(\nx<array<int>>= [1]\ny<int>=2\n)\n",
        "@DATA(\n  x<array<int>> = [1],\n  y<int> = 2\n)",
        None,
        &[],
    );
}

#[test]
fn dates() {
    check(
        "dates",
        "halt",
        "@DATA(\nd = 2026-09-06T00:00:00Z, e = 2026-09-06\n)\n",
        "@DATA(\n  d = 2026-09-06T00:00:00Z,\n  e = 2026-09-06\n)",
        None,
        &[],
    );
}

#[test]
fn nested_containers() {
    check(
        "nested_containers",
        "halt",
        "@DATA(\na = [1, [2, [3]], {x = [4, 5]}, 6]\no = {a = 1, b = {c = [1, {d = 2}]}}\n)\n",
        "@DATA(\n  a = [1, [2, [3]], {x = [4, 5]}, 6],\n  o = {a = 1, b = {c = [1, {d = 2}]}}\n)",
        None,
        &[],
    );
}

#[test]
fn empty_containers() {
    check(
        "empty_containers",
        "halt",
        "@DATA(\na = []\nb = {}\n)\n",
        "@DATA(\n  a = [],\n  b = {}\n)",
        None,
        &[],
    );
}

#[test]
fn trailing_commas_in_containers() {
    check(
        "trailing_commas_in_containers",
        "halt",
        "@DATA(\na = [1, 2,]\no = {k = 1,}\n)\n",
        "@DATA(\n  a = [1, 2],\n  o = {k = 1}\n)",
        None,
        &[],
    );
    check(
        "trailing_commas_in_containers",
        "continue",
        "@DATA(\na = [1, 2,]\no = {k = 1,}\n)\n",
        "@DATA(\n  a = [1, 2],\n  o = {k = 1}\n)",
        None,
        &[],
    );
    check(
        "trailing_commas_in_containers",
        "recover",
        "@DATA(\na = [1, 2,]\no = {k = 1,}\n)\n",
        "@DATA(\n  a = [1, 2],\n  o = {k = 1}\n)",
        None,
        &[],
    );
}

#[test]
fn constructors() {
    check(
        "constructors",
        "halt",
        "@DATA(\nb = b:(\"SGVsbG8=\")\nr = r:(\"^[a-z]+$\")\nt = t:(a, 1, \"s\")\n)\n",
        "@DATA(\n  b = b:(\"SGVsbG8=\"),\n  r = r:(\"^[a-z]+$\"),\n  t = t:(a, 1, \"s\")\n)",
        None,
        &[],
    );
}

#[test]
fn identifiers_enums_and_calls() {
    check(
        "identifiers_enums_and_calls",
        "halt",
        "@DATA(\na = SomeName, b = Colors.Red, c = fn(1, \"s\", true, null), d = lib.fn(1, [2], {k = 3}), e = lib.Colors.Red\n)\n",
        "@DATA(\n  a = SomeName,\n  b = Colors.Red,\n  c = fn(1, \"s\", true, null),\n  d = lib.fn(1, [2], {k = 3}),\n  e = lib.Colors.Red\n)",
        None,
        &[],
    );
}

#[test]
fn call_arguments_of_every_kind() {
    check(
        "call_arguments_of_every_kind",
        "halt",
        "@DATA(\na = outer(inner(1), lib.f(2), Colors.Red, lib.Colors.Red, name, [1], {k = v}, b:(\"x\"), r:(\"y\"), t:(p, q), -1, 2026-09-06T00:00:00Z, #FF00AA, 1e3, 2L, 1.5f, 2.5, \"s\", true, false, null)\n)\n",
        "@DATA(\n  a = outer(inner(1), lib.f(2), Colors.Red, lib.Colors.Red, name, [1], {k = v}, b:(\"x\"), r:(\"y\"), t:(p, q), -1, 2026-09-06T00:00:00Z, #FF00AA, 1e3, 2L, 1.5f, 2.5, \"s\", true, false, null)\n)",
        None,
        &[],
    );
}

#[test]
fn tables_and_group_arrays() {
    check(
        "tables_and_group_arrays",
        "halt",
        "@DATA(\nt.u: x<int> = 1, y = \"s\"\ng:: {a = 1}, {a = 2}\nk.h:: 1, 2, 3\nm:: [1, 2], f(1), lib.g(2), Colors.Red\n)\n",
        "@DATA(\n  t.u: x<int> = 1, y = \"s\",\n  g:: {a = 1}, {a = 2},\n  k.h:: 1, 2, 3,\n  m:: [1, 2], f(1), lib.g(2), Colors.Red\n)",
        None,
        &[],
    );
}

#[test]
fn entries_without_commas() {
    check(
        "entries_without_commas",
        "halt",
        "@DATA(\nt: x = 1 y = 2\nu: z = 3\n)\n",
        "@DATA(\n  t: x = 1, y = 2,\n  u: z = 3\n)",
        None,
        &[],
    );
}

#[test]
fn group_array_items_on_lines() {
    check(
        "group_array_items_on_lines",
        "halt",
        "@DATA(\ng.h::\n  1,\n  2,\n  3\nk:: \"a\", \"b\"\n)\n",
        "@DATA(\n  g.h:: 1, 2, 3,\n  k:: \"a\", \"b\"\n)",
        None,
        &[],
    );
}

#[test]
fn flat_property_after_grouped_data() {
    check(
        "flat_property_after_grouped_data",
        "halt",
        "@DATA(\nt: x = 1\na = 2\n)\n",
        "@DATA(\n  t: x = 1, a = 2\n)",
        None,
        &[],
    );
    check(
        "flat_property_after_grouped_data",
        "continue",
        "@DATA(\nt: x = 1\na = 2\n)\n",
        "@DATA(\n  t: x = 1, a = 2\n)",
        None,
        &[],
    );
    check(
        "flat_property_after_grouped_data",
        "recover",
        "@DATA(\nt: x = 1\na = 2\n)\n",
        "@DATA(\n  t: x = 1, a = 2\n)",
        None,
        &[],
    );
}

#[test]
fn failed_nested_array_does_not_leak() {
    check(
        "failed_nested_array_does_not_leak",
        "continue",
        "@DATA(\na = [1, [2 3], 4]\nb = 5\n)\n",
        "@DATA(\n  a = [1, [2], 4],\n  b = 5\n)",
        None,
        &["Expected ',' or ']' in array literal"],
    );
    check(
        "failed_nested_array_does_not_leak",
        "recover",
        "@DATA(\na = [1, [2 3], 4]\nb = 5\n)\n",
        "@DATA(\n  a = [1, [2], 4],\n  b = 5\n)",
        None,
        &["Expected ',' or ']' in array literal"],
    );
}

#[test]
fn failed_object_in_array_does_not_leak() {
    check(
        "failed_object_in_array_does_not_leak",
        "continue",
        "@DATA(\na = [1, {k = }, 2]\nb = 5\n)\n",
        "@DATA(\n  a = [1, {k = null}, 2],\n  b = 5\n)",
        None,
        &["Expected property value after '=' for key 'k'"],
    );
    check(
        "failed_object_in_array_does_not_leak",
        "recover",
        "@DATA(\na = [1, {k = }, 2]\nb = 5\n)\n",
        "@DATA(\n  a = [1, {k = null}, 2],\n  b = 5\n)",
        None,
        &["Expected property value after '=' for key 'k'"],
    );
}

#[test]
fn failed_call_in_call_does_not_leak() {
    check(
        "failed_call_in_call_does_not_leak",
        "continue",
        "@DATA(\na = f(1, g(2 3), 4)\nb = 5\n)\n",
        "@DATA(\n  a = f(1, g(2), 4),\n  b = 5\n)",
        None,
        &["Expected ',' or ')' in function arguments for 'g'"],
    );
    check(
        "failed_call_in_call_does_not_leak",
        "recover",
        "@DATA(\na = f(1, g(2 3), 4)\nb = 5\n)\n",
        "@DATA(\n  a = f(1, g(2), 4),\n  b = 5\n)",
        None,
        &["Expected ',' or ')' in function arguments for 'g'"],
    );
}

#[test]
fn failed_imported_call_in_array_does_not_leak() {
    check(
        "failed_imported_call_in_array_does_not_leak",
        "continue",
        "@DATA(\na = [lib.f(1 2), 3]\nb = 5\n)\n",
        "@DATA(\n  a = [lib.f(1), 3],\n  b = 5\n)",
        None,
        &["Expected ',' or ')' in function arguments for 'lib.f'"],
    );
    check(
        "failed_imported_call_in_array_does_not_leak",
        "recover",
        "@DATA(\na = [lib.f(1 2), 3]\nb = 5\n)\n",
        "@DATA(\n  a = [lib.f(1), 3],\n  b = 5\n)",
        None,
        &["Expected ',' or ')' in function arguments for 'lib.f'"],
    );
}

#[test]
fn failed_tuple_in_array_does_not_leak() {
    check(
        "failed_tuple_in_array_does_not_leak",
        "continue",
        "@DATA(\na = [t:(1 2), 3]\nb = 5\n)\n",
        "@DATA(\n  a = [t:(1), 3],\n  b = 5\n)",
        None,
        &["Expected ',' or ')' in tuple constructor"],
    );
    check(
        "failed_tuple_in_array_does_not_leak",
        "recover",
        "@DATA(\na = [t:(1 2), 3]\nb = 5\n)\n",
        "@DATA(\n  a = [t:(1), 3],\n  b = 5\n)",
        None,
        &["Expected ',' or ')' in tuple constructor"],
    );
}

#[test]
fn failed_group_array_item() {
    check(
        "failed_group_array_item",
        "continue",
        "@DATA(\ng:: 1 2\nh:: 3\n)\n",
        "@DATA(\n  g:: 1, 2,\n  h:: 3\n)",
        None,
        &[],
    );
    check(
        "failed_group_array_item",
        "recover",
        "@DATA(\ng:: 1 2\nh:: 3\n)\n",
        "@DATA(\n  g:: 1, 2,\n  h:: 3\n)",
        None,
        &[],
    );
}

#[test]
fn missing_values() {
    check(
        "missing_values",
        "halt",
        "@DATA(\na =\nb = ,\nc = 3\n)\n",
        "NONE",
        None,
        &["Unexpected token in DATA section: Symbol('=')"],
    );
    check(
        "missing_values",
        "continue",
        "@DATA(\na =\nb = ,\nc = 3\n)\n",
        "@DATA(\n  a = b,\n  c = 3\n)",
        None,
        &["Unable to determine data entry type from token: Symbol(',')",
        "Unexpected token in DATA section: Symbol('=')"],
    );
    check(
        "missing_values",
        "recover",
        "@DATA(\na =\nb = ,\nc = 3\n)\n",
        "@DATA(\n  a = b,\n  c = 3\n)",
        None,
        &["Unable to determine data entry type from token: Symbol(',')",
        "Unexpected token in DATA section: Symbol('=')"],
    );
}

#[test]
fn unclosed_array_object_and_call() {
    check(
        "unclosed_array_object_and_call",
        "halt",
        "@DATA(\na = [1, 2\nb = {k = 1\nc = f(1,\n)\n",
        "NONE",
        None,
        &["Expected ',' or ']' in array literal"],
    );
    check(
        "unclosed_array_object_and_call",
        "continue",
        "@DATA(\na = [1, 2\nb = {k = 1\nc = f(1,\n)\n",
        "@DATA(\n  a = [1, 2]\n)",
        None,
        &["Expected ')' to close DATA section",
        "Expected ',' or ']' in array literal",
        "Expected ']' to close array literal",
        "Failed to parse array value",
        "Failed to parse array value"],
    );
    check(
        "unclosed_array_object_and_call",
        "recover",
        "@DATA(\na = [1, 2\nb = {k = 1\nc = f(1,\n)\n",
        "@DATA(\n  a = [1, 2]\n)",
        None,
        &["Expected ')' to close DATA section",
        "Expected ',' or ']' in array literal",
        "Expected ']' to close array literal",
        "Failed to parse array value",
        "Failed to parse array value"],
    );
}

#[test]
fn object_property_errors() {
    check(
        "object_property_errors",
        "halt",
        "@DATA(\na = {k}\nb = {k = }\nc = {1 = 1}\nd = {k = 1 j = 2}\n)\n",
        "NONE",
        None,
        &["Expected '=' after property key 'k' in object literal"],
    );
    check(
        "object_property_errors",
        "continue",
        "@DATA(\na = {k}\nb = {k = }\nc = {1 = 1}\nd = {k = 1 j = 2}\n)\n",
        "@DATA(\n  a = {},\n  b = {k = null},\n  c = {},\n  d = {k = 1}\n)",
        None,
        &["Expected ',' or '}' in object literal",
        "Expected ',' or '}' in object literal",
        "Expected ',' or '}' in object literal",
        "Expected ',' or '}' in object literal",
        "Expected ',' or '}' in object literal",
        "Expected ',' or '}' in object literal",
        "Expected '=' after property key 'k' in object literal",
        "Expected property name identifier",
        "Expected property name identifier",
        "Expected property name identifier",
        "Expected property name identifier",
        "Expected property name identifier",
        "Expected property value after '=' for key 'k'"],
    );
    check(
        "object_property_errors",
        "recover",
        "@DATA(\na = {k}\nb = {k = }\nc = {1 = 1}\nd = {k = 1 j = 2}\n)\n",
        "@DATA(\n  a = {},\n  b = {k = null},\n  c = {},\n  d = {k = 1}\n)",
        None,
        &["Expected ',' or '}' in object literal",
        "Expected ',' or '}' in object literal",
        "Expected ',' or '}' in object literal",
        "Expected ',' or '}' in object literal",
        "Expected ',' or '}' in object literal",
        "Expected ',' or '}' in object literal",
        "Expected '=' after property key 'k' in object literal",
        "Expected property name identifier",
        "Expected property name identifier",
        "Expected property name identifier",
        "Expected property name identifier",
        "Expected property name identifier",
        "Expected property value after '=' for key 'k'"],
    );
}

#[test]
fn bad_type_annotations() {
    check(
        "bad_type_annotations",
        "halt",
        "@DATA(\na<unknown> = 1\nb<int = 2\nc<array<>> = [3]\n)\n",
        "NONE",
        None,
        &["Expected '=' after property name 'a'",
        "Expected '>' to close type annotation",
        "Expected data type in type annotation"],
    );
    check(
        "bad_type_annotations",
        "continue",
        "@DATA(\na<unknown> = 1\nb<int = 2\nc<array<>> = [3]\n)\n",
        "@DATA(\n  a = unknown,\n  int = 2,\n  c<array> = [3]\n)",
        None,
        &["Expected '=' after property name 'a'",
        "Expected '>' to close type annotation",
        "Expected data type in type annotation",
        "Expected element type keyword (int, string, bool, …) in typed collection annotation",
        "Unable to determine data entry type from token: Identifier(\"b\")",
        "Unable to determine data entry type from token: Symbol('=')",
        "Unexpected token in DATA section: Integer(1)",
        "Unexpected token in DATA section: Symbol('<')",
        "Unexpected token in DATA section: Symbol('>')"],
    );
    check(
        "bad_type_annotations",
        "recover",
        "@DATA(\na<unknown> = 1\nb<int = 2\nc<array<>> = [3]\n)\n",
        "@DATA(\n  a = unknown,\n  int = 2,\n  c<array> = [3]\n)",
        None,
        &["Expected '=' after property name 'a'",
        "Expected '>' to close type annotation",
        "Expected data type in type annotation",
        "Expected element type keyword (int, string, bool, …) in typed collection annotation",
        "Unable to determine data entry type from token: Identifier(\"b\")",
        "Unable to determine data entry type from token: Identifier(\"b\")",
        "Unable to determine data entry type from token: Identifier(\"b\")",
        "Unable to determine data entry type from token: Symbol('<')",
        "Unable to determine data entry type from token: Symbol('=')",
        "Unexpected token in DATA section: Symbol('>')"],
    );
}

#[test]
fn deep_arrays() {
    let source = format!("@DATA(\na = {o}1{c}\nb = 2\n)\n", o = "[".repeat(66), c = "]".repeat(66));
    check(
        "deep_arrays",
        "continue",
        &source,
        "@DATA(\n  a = [[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]],\n  b = 2\n)",
        None,
        &["Failed to parse array value",
        "Maximum nesting depth (64) exceeded. Consider flattening your data structure.",
        "Unable to determine data entry type from token: Symbol(']')",
        "Unexpected token in DATA section: Symbol(']')"],
    );
}

#[test]
fn deep_calls() {
    let source = format!("@DATA(\na = {o}1{c}\nb = 2\n)\n", o = "f(".repeat(12), c = ")".repeat(12));
    check(
        "deep_calls",
        "continue",
        &source,
        "@DATA(\n  a = f(f(f(f(f(f(f(f(f(f(f(f(1)))))))))))),\n  b = 2\n)",
        None,
        &[],
    );
}

#[test]
fn deep_imported_calls() {
    let source = format!("@DATA(\na = {o}1{c}\nb = 2\n)\n", o = "lib.f(".repeat(12), c = ")".repeat(12));
    check(
        "deep_imported_calls",
        "continue",
        &source,
        "@DATA(\n  a = lib.f(lib.f(lib.f(lib.f(lib.f(lib.f(lib.f(lib.f(lib.f(lib.f(lib.f(lib.f(1)))))))))))),\n  b = 2\n)",
        None,
        &[],
    );
}

#[test]
fn unclosed_section() {
    check(
        "unclosed_section",
        "halt",
        "@DATA(\n a = 1\n",
        "NONE",
        None,
        &["Expected ')' to close DATA section"],
    );
    check(
        "unclosed_section",
        "continue",
        "@DATA(\n a = 1\n",
        "@DATA(\n  a = 1\n)",
        None,
        &["Expected ')' to close DATA section"],
    );
    check(
        "unclosed_section",
        "recover",
        "@DATA(\n a = 1\n",
        "@DATA(\n  a = 1\n)",
        None,
        &["Expected ')' to close DATA section"],
    );
}

#[test]
fn missing_open_paren() {
    check(
        "missing_open_paren",
        "halt",
        "@DATA\n a = 1\n)\n",
        "@DATA(\n  a = 1\n)",
        None,
        &[],
    );
    check(
        "missing_open_paren",
        "continue",
        "@DATA\n a = 1\n)\n",
        "@DATA(\n  a = 1\n)",
        None,
        &[],
    );
    check(
        "missing_open_paren",
        "recover",
        "@DATA\n a = 1\n)\n",
        "@DATA(\n  a = 1\n)",
        None,
        &[],
    );
}

#[test]
fn stray_tokens() {
    check(
        "stray_tokens",
        "halt",
        "@DATA(\na = 1\n@@@\nb = 2\nc = ;\nd = 4\n)\n",
        "NONE",
        None,
        &["Unexpected token in DATA section: Symbol('@')"],
    );
    check(
        "stray_tokens",
        "continue",
        "@DATA(\na = 1\n@@@\nb = 2\nc = ;\nd = 4\n)\n",
        "@DATA(\n  a = 1,\n  b = 2,\n  c = null,\n  d = 4\n)",
        None,
        &["Expected property value after '=' in property 'c'",
        "Unable to determine data entry type from token: Symbol('@')",
        "Unexpected token in DATA section: Symbol(';')",
        "Unexpected token in DATA section: Symbol('@')",
        "Unexpected token in DATA section: Symbol('@')"],
    );
    check(
        "stray_tokens",
        "recover",
        "@DATA(\na = 1\n@@@\nb = 2\nc = ;\nd = 4\n)\n",
        "@DATA(\n  a = 1,\n  b = 2,\n  c = null,\n  d = 4\n)",
        None,
        &["Expected property value after '=' in property 'c'",
        "Unable to determine data entry type from token: Symbol('@')",
        "Unexpected token in DATA section: Symbol(';')",
        "Unexpected token in DATA section: Symbol('@')"],
    );
}

#[test]
fn positions() {
    check(
        "positions",
        "halt",
        "@DATA(\n  a = 1,\n  t.u: x = 2\n  g:: [3]\n)\n",
        "@DATA(\n  a = 1,\n  t.u: x = 2,\n  g:: [3]\n)",
        Some("DataSection { entries: [SimpleProperty { name: \"a\", data_type: None, value: Integer { value: 1, position: Position { line: 2, column: 7 } }, position: Position { line: 2, column: 3 } }, TableProperty { path: TablePath { segments: [\"t\", \"u\"] }, properties: [PropertyAssignment { name: \"x\", data_type: None, value: Integer { value: 2, position: Position { line: 3, column: 12 } }, position: Position { line: 3, column: 8 } }], position: Position { line: 3, column: 3 } }, GroupArray { path: TablePath { segments: [\"g\"] }, items: [Array { values: [Integer { value: 3, position: Position { line: 4, column: 8 } }], position: Position { line: 4, column: 7 } }], position: Position { line: 4, column: 3 } }], position: Position { line: 1, column: 6 } }"),
        &[],
    );
}

#[test]
fn nested_objects_keep_their_own_properties() {
    check(
        "nested_objects_keep_their_own_properties",
        "halt",
        "@DATA(\no = {a = {b = 1, c = {d = 2}}, e = 3, f = {g = 4}}\np = 5\n)\n",
        "@DATA(\n  o = {a = {b = 1, c = {d = 2}}, e = 3, f = {g = 4}},\n  p = 5\n)",
        None,
        &[],
    );
}

#[test]
fn failed_object_in_object_does_not_leak() {
    check(
        "failed_object_in_object_does_not_leak",
        "continue",
        "@DATA(\na = {x = {k = }, y = 1}\nb = 5\n)\n",
        "@DATA(\n  a = {x = {k = null}, y = 1},\n  b = 5\n)",
        None,
        &["Expected property value after '=' for key 'k'"],
    );
    check(
        "failed_object_in_object_does_not_leak",
        "recover",
        "@DATA(\na = {x = {k = }, y = 1}\nb = 5\n)\n",
        "@DATA(\n  a = {x = {k = null}, y = 1},\n  b = 5\n)",
        None,
        &["Expected property value after '=' for key 'k'"],
    );
}

#[test]
fn calls_nested_through_containers_keep_their_own_arguments() {
    check(
        "calls_nested_through_containers_keep_their_own_arguments",
        "halt",
        "@DATA(\na = f(1, [g(2), lib.h(3)], {k = i(4)}, 5)\nb = lib.j(6, [k(7)], 8)\n)\n",
        "@DATA(\n  a = f(1, [g(2), lib.h(3)], {k = i(4)}, 5),\n  b = lib.j(6, [k(7)], 8)\n)",
        None,
        &[],
    );
}

#[test]
fn failed_call_in_container_in_call_does_not_leak() {
    check(
        "failed_call_in_container_in_call_does_not_leak",
        "continue",
        "@DATA(\na = f(1, [g(2 3)], 4)\nb = 5\n)\n",
        "@DATA(\n  a = f(1, [g(2)], 4),\n  b = 5\n)",
        None,
        &["Expected ',' or ')' in function arguments for 'g', found 3"],
    );
    check(
        "failed_call_in_container_in_call_does_not_leak",
        "recover",
        "@DATA(\na = f(1, [g(2 3)], 4)\nb = 5\n)\n",
        "@DATA(\n  a = f(1, [g(2)], 4),\n  b = 5\n)",
        None,
        &["Expected ',' or ')' in function arguments for 'g', found 3"],
    );
}
