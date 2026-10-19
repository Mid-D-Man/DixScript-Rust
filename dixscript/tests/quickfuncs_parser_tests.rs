// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/quickfuncs_parser_tests.rs"
// ============================================================================
//! Tests for the `@QUICKFUNCS` section parser.
//!
//! The loader tests reach the parser only through `DixLoader`, which cannot show what each error
//! strategy does with malformed input or which diagnostics are reported. These tests pin the
//! parser itself, which is the fifth and last section parser built on the arena AST: what a
//! section parses to (printed with the owned `Display`), the diagnostic messages, and, for one
//! case, the source positions. The cases cover the function signatures, every statement kind,
//! the `if`/`elif`/`else` chain, `chk` switches with their `miss` case, operator precedence,
//! postfix chains, every literal and container, interpolated strings, lambdas, the nesting depth
//! limits, and malformed input under all three error strategies.
//!
//! A malformed list inside a list is the case the scratch stacks exist for: the `does_not_leak`
//! tests check that the entries of a list that failed part way never end up in the list that
//! encloses it, and the `keep_their_own` tests check the same for lists that did not fail.
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
    let label = "quickfuncs-parser-test";
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
            Ok(ast) => match &ast.quick_functions {
                None => ("NONE".to_string(), None),
                Some(q) => (q.to_string(), Some(format!("{:?}", q))),
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
fn functions_and_signatures() {
    check(
        "functions_and_signatures",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn 1\n}\n~g<int> => data.users, data.posts (a<int> = 1, b = a + 2, c) {\nreturn a\n}\n~h<string> => global (x<array>, y<object> = [1, 2], z<tuple> = t:(1, 2)) {\nreturn x\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return 1\n  },\n  ~g<int> => data.users,data.posts(a<int> = 1, b = (a + 2), c) {\n    return a\n  },\n  ~h<string> => global(x<array>, y<object> = [1, 2], z<tuple> = t:(<ToEvaluate: 1>, <ToEvaluate: 2>)) {\n    return x\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn statements_of_every_kind() {
    check(
        "statements_of_every_kind",
        "halt",
        "@QUICKFUNCS(\n~f() {\nlet x = 1\nconst y<int> = 2\nlet mut z<string> = \"s\"\nx = 3\nz += 1\nz -= 2\nz *= 3\nz /= 4\nz |= 5\nz &= 6\nz ^= 7\nlog: \"hello\"\nf(1)\nlib.g(2)\nreturn x;\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    let x = 1\n    const y<int> = 2\n    let mut z<string> = \"s\"\n    x = 3\n    z += 1\n    z -= 2\n    z *= 3\n    z /= 4\n    z |= 5\n    z &= 6\n    z ^= 7\n    log: \"hello\"\n    f(1)\n    lib.g(2)\n    return x\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn empty_return_forms() {
    check(
        "empty_return_forms",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn\n}\n~g() {\nreturn;\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return null\n  },\n  ~g => global() {\n    return null\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn if_elif_else_chain() {
    check(
        "if_elif_else_chain",
        "halt",
        "@QUICKFUNCS(\n~f() {\nif: a { log: \"a\" } elif: b { log: \"b\" } elif: c { log: \"c\" } else { log: \"d\" }\nif: x then return 1\nif: y { return 2 } else return 3\nreturn 4\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    if: a { log: \"a\"; } else { if: b { log: \"b\"; } else { if: c { log: \"c\"; } else { log: \"d\"; }; }; }\n    if: x { return 1; }\n    if: y { return 2; } else { return 3; }\n    return 4\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn nested_if_blocks() {
    check(
        "nested_if_blocks",
        "halt",
        "@QUICKFUNCS(\n~f() {\nif: a { if: b { log: \"x\" } else { if: c { log: \"y\" } log: \"z\" } log: \"w\" }\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    if: a { if: b { log: \"x\"; } else { if: c { log: \"y\"; }; log: \"z\"; }; log: \"w\"; }\n    return 1\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn switch_cases_and_miss() {
    check(
        "switch_cases_and_miss",
        "halt",
        "@QUICKFUNCS(\n~f() {\nchk: x { -> 1 then log: \"one\" -> 2 { log: \"two\" log: \"again\" } -> \"s\" => log: \"str\" -> miss { log: \"other\" } }\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    chk: x { -> 1 { log: \"one\"; } -> 2 { log: \"two\"; log: \"again\"; } -> \"s\" { log: \"str\"; } -> null { log: \"other\"; } }\n    return 1\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn switch_nested_and_value_kinds() {
    check(
        "switch_nested_and_value_kinds",
        "halt",
        "@QUICKFUNCS(\n~f() {\nchk: x { -> 1 { chk: y { -> 2 then log: \"n\" -> miss then log: \"m\" } } -> [1, 2] then log: \"a\" -> {k = 1} then log: \"b\" -> a.b.c then log: \"c\" -> null then log: \"n\" -> #FF00AA then log: \"h\" }\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    chk: x { -> 1 { chk: y { -> 2 { log: \"n\"; } -> null { log: \"m\"; } }; } -> [1, 2] { log: \"a\"; } -> {k = 1} { log: \"b\"; } -> a.b.c { log: \"c\"; } -> null { log: \"n\"; } -> #FF00AA { log: \"h\"; } }\n    return 1\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn operator_precedence() {
    check(
        "operator_precedence",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn 1 + 2 * 3 - 4 / 5 % 6 ** 2\nreturn a && b || c and d or e\nreturn a == b != c < d > e <= f >= g\nreturn a & b | c ^ d << 1 >> 2\nreturn a %% b %& c &% d\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return ((1 + (2 * 3)) - ((4 / 5) % (6 ** 2)))\n    return (((a && b) || (c and d)) or e)\n    return ((a == b) != ((((c < d) > e) <= f) >= g))\n    return ((a & b) | (c ^ ((d << 1) >> 2)))\n    return (((a %% b) %& c) &% d)\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn unary_operators_and_ternary() {
    check(
        "unary_operators_and_ternary",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn -x + !y + +z\nreturn not a\nreturn ~?a\nreturn --x\nreturn ++x\nreturn a ? b ? 1 : 2 : c ? 3 : 4\nreturn a ? (b ? 1 : 2) : 3\nreturn (a + b) * (c - d)\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return ((-x + !y) + +z)\n    return nota\n    return ~?a\n    return --x\n    return ++x\n    return a ? b ? 1 : 2 : c ? 3 : 4\n    return a ? (b ? 1 : 2) : 3\n    return (((a + b)) * ((c - d)))\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn postfix_chains() {
    check(
        "postfix_chains",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn a.b.c.d\nreturn a.b()\nreturn a.b(1, 2)\nreturn a[0][1][2]\nreturn a.b[2].c(3)\nreturn a.b().c()\nreturn a[0].b\nreturn a[0](1)\nreturn a.b.c[0].d\nreturn x.y.z(1, a.b, [1], {k = 1})\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return a.b.c.d\n    return a.b()\n    return a.b(1, 2)\n    return a[0][1][2]\n    return a.b[2].c(3)\n    return a.b().c()\n    return a[0].b\n    return a[0].__call__(1)\n    return a.b.c[0].d\n    return x.y.z(1, a.b, [1], {k = 1})\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn calls_and_nested_calls() {
    check(
        "calls_and_nested_calls",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn f()\nreturn f(1, 2, 3)\nreturn f(g(1), h(2, k(3)))\nreturn lib.f(1)(2)\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return f()\n    return f(1, 2, 3)\n    return f(g(1), h(2, k(3)))\n    return lib.f(1)\n    (2)\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn literals_of_every_kind() {
    check(
        "literals_of_every_kind",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn [1, \"s\", 2.5, true, null, 2L, 1.5f, 1e3, 't', #FF00AA, 2026-09-06, 2026-09-06T00:00:00Z]\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return [1, \"s\", 2.5, true, null, 2L, 1.5f, 1e3, \"t\", #FF00AA, 2026-09-06, 2026-09-06T00:00:00Z]\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn containers_and_constructors() {
    check(
        "containers_and_constructors",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn []\nreturn {}\nreturn {a = 1, b = [1, 2], c = {d = 3}}\nreturn {\"quoted\" = 1, a: 2, b<int> = 3}\nreturn [a + 1, f(2), x.y]\nreturn {k = a + 1, j = f(2), i = x.y, h = [1]}\nreturn t:(1, 2, a + 1)\nreturn t:()\nreturn b:(\"data\")\nreturn b:()\nreturn r:(\"^[a-z]+$\")\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return []\n    return {}\n    return {a = 1, b = [1, 2], c = {d = 3}}\n    return {quoted = 1, a = 2, b = 3}\n    return [<ToEvaluate: (a + 1)>, <ToEvaluate: f(2)>, <ToEvaluate: x.y>]\n    return {k = <ToEvaluate: (a + 1)>, j = <ToEvaluate: f(2)>, i = x.y, h = [1]}\n    return t:(<ToEvaluate: 1>, <ToEvaluate: 2>, <ToEvaluate: (a + 1)>)\n    return t:()\n    return b:(\"data\")\n    return b:(\"\")\n    return r:(\"regex_pattern\")\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn interpolated_strings() {
    check(
        "interpolated_strings",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn $\"x = {x}\"\nreturn $\"sum {a + b} and {c * 2} then {d.e} {f.g()} {7} {2.5} {true} {'s'}\"\nreturn $'single {x}'\nreturn $\"no placeholders\"\nreturn $\"{}\"\nreturn $\"{ }\"\nreturn [$\"in {a}\", {k = $\"obj {b}\"}]\nreturn f($\"arg {c + 1}\")\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return $\"x = {0}\"\n    return $\"sum {0} and {1} then {2} {3} {4} {5} {6} {7}\"\n    return $\"single {0}\"\n    return $\"no placeholders\"\n    return $\"{}\"\n    return $\"{0}\"\n    return [$\"in {0}\", {k = $\"obj {0}\"}]\n    return f($\"arg {0}\")\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn lambdas() {
    check(
        "lambdas",
        "halt",
        "@QUICKFUNCS(\n~f() {\nlet f = (x) => x + 1\nlet g = (a, b) => a * b\nlet h = () => 1\nlet i = (a, b) => { let c = a + b\nreturn c }\nlet j = (a<int>, b<string>) => a\nreturn map(xs, (x) => x * 2)\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    let f = (x) => (x + 1)\n    let g = (a, b) => (a * b)\n    let h = () => 1\n    let i = (a, b) => null\n    let j = (a, b) => a\n    return map(xs, (x) => (x * 2))\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn lambda_block_inside_blocks() {
    check(
        "lambda_block_inside_blocks",
        "halt",
        "@QUICKFUNCS(\n~f() {\nif: a { let f = (x) => { let y = x\nif: y { log: \"in\" } return y } log: \"after\" }\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    if: a { let f = (x) => null; log: \"after\"; }\n    return 1\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn calls_through_containers_keep_their_own_arguments() {
    check(
        "calls_through_containers_keep_their_own_arguments",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn f(1, [g(2), h(3)], {k = i(4)}, 5)\nreturn lib.j(6, [k(7)], 8)\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return f(1, [<ToEvaluate: g(2)>, <ToEvaluate: h(3)>], {k = <ToEvaluate: i(4)>}, 5)\n    return lib.j(6, [<ToEvaluate: k(7)>], 8)\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn nested_containers_keep_their_own_elements() {
    check(
        "nested_containers_keep_their_own_elements",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn [1, [2, [3]], {x = [4, 5]}, 6]\nreturn {a = {b = 1, c = {d = 2}}, e = 3, f = {g = 4}}\nreturn [t:(1, [2]), t:(3)]\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return [1, [2, [3]], {x = [4, 5]}, 6]\n    return {a = {b = 1, c = {d = 2}}, e = 3, f = {g = 4}}\n    return [t:(<ToEvaluate: 1>, <ToEvaluate: [2]>), t:(<ToEvaluate: 3>)]\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn statement_blocks_keep_their_own_statements() {
    check(
        "statement_blocks_keep_their_own_statements",
        "halt",
        "@QUICKFUNCS(\n~f() {\nif: a { log: \"1\" if: b { log: \"2\" } log: \"3\" }\nlog: \"4\"\nchk: x { -> 1 { log: \"5\" if: c { log: \"6\" } } }\nreturn 7\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    if: a { log: \"1\"; if: b { log: \"2\"; }; log: \"3\"; }\n    log: \"4\"\n    chk: x { -> 1 { log: \"5\"; if: c { log: \"6\"; }; } }\n    return 7\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn failed_nested_array_does_not_leak() {
    check(
        "failed_nested_array_does_not_leak",
        "continue",
        "@QUICKFUNCS(\n~f() {\nreturn [1, [2 3], 4]\nlog: \"after\"\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return [1, [2]]\n    3\n    null\n    null\n    4\n    null\n    log: \"after\"\n  }\n)",
        None,
        &[],
    );
    check(
        "failed_nested_array_does_not_leak",
        "recover",
        "@QUICKFUNCS(\n~f() {\nreturn [1, [2 3], 4]\nlog: \"after\"\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return [1, [2]]\n    3\n    null\n    null\n    4\n    null\n    log: \"after\"\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn failed_object_in_object_does_not_leak() {
    check(
        "failed_object_in_object_does_not_leak",
        "continue",
        "@QUICKFUNCS(\n~f() {\nreturn {a = {b = }, c = 1}\nlog: \"after\"\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return {a = {b = null, c = 1}, log = \"after\"}\n    null\n  }\n)",
        None,
        &[],
    );
    check(
        "failed_object_in_object_does_not_leak",
        "recover",
        "@QUICKFUNCS(\n~f() {\nreturn {a = {b = }, c = 1}\nlog: \"after\"\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return {a = {b = null, c = 1}, log = \"after\"}\n    null\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn failed_call_in_call_does_not_leak() {
    check(
        "failed_call_in_call_does_not_leak",
        "continue",
        "@QUICKFUNCS(\n~f() {\nreturn f(1, g(2 3), 4)\nlog: \"after\"\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return f(1, g(2))\n    3\n    null\n    null\n    4\n    null\n    log: \"after\"\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments"],
    );
    check(
        "failed_call_in_call_does_not_leak",
        "recover",
        "@QUICKFUNCS(\n~f() {\nreturn f(1, g(2 3), 4)\nlog: \"after\"\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return f(1, g(2))\n    3\n    null\n    null\n    4\n    null\n    log: \"after\"\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments"],
    );
}

#[test]
fn failed_call_in_container_in_call_does_not_leak() {
    check(
        "failed_call_in_container_in_call_does_not_leak",
        "continue",
        "@QUICKFUNCS(\n~f() {\nreturn f(1, [g(2 3)], 4)\nlog: \"after\"\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return f(1, [<ToEvaluate: g(2)>])\n    3\n    null\n    null\n    null\n    4\n    null\n    log: \"after\"\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments"],
    );
    check(
        "failed_call_in_container_in_call_does_not_leak",
        "recover",
        "@QUICKFUNCS(\n~f() {\nreturn f(1, [g(2 3)], 4)\nlog: \"after\"\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return f(1, [<ToEvaluate: g(2)>])\n    3\n    null\n    null\n    null\n    4\n    null\n    log: \"after\"\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments"],
    );
}

#[test]
fn failed_tuple_in_array_does_not_leak() {
    check(
        "failed_tuple_in_array_does_not_leak",
        "continue",
        "@QUICKFUNCS(\n~f() {\nreturn [t:(1 2), 3]\nlog: \"after\"\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return [null]\n    2\n    null\n    null\n    3\n    null\n    log: \"after\"\n  }\n)",
        None,
        &["Expected ')'"],
    );
    check(
        "failed_tuple_in_array_does_not_leak",
        "recover",
        "@QUICKFUNCS(\n~f() {\nreturn [t:(1 2), 3]\nlog: \"after\"\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return [null]\n    2\n    null\n    null\n    3\n    null\n    log: \"after\"\n  }\n)",
        None,
        &["Expected ')'"],
    );
}

#[test]
fn failed_block_in_block_does_not_leak() {
    check(
        "failed_block_in_block_does_not_leak",
        "continue",
        "@QUICKFUNCS(\n~f() {\nif: a { if: b { log: } log: \"x\" }\nlog: \"after\"\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    if: a { if: b { log: null; log: \"x\"; }; log: \"after\"; return 1; }\n    null\n  }\n)",
        None,
        &[],
    );
    check(
        "failed_block_in_block_does_not_leak",
        "recover",
        "@QUICKFUNCS(\n~f() {\nif: a { if: b { log: } log: \"x\" }\nlog: \"after\"\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    if: a { if: b { log: null; log: \"x\"; }; log: \"after\"; return 1; }\n    null\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn failed_lambda_block_does_not_leak() {
    check(
        "failed_lambda_block_does_not_leak",
        "continue",
        "@QUICKFUNCS(\n~f() {\nlet f = (x) => { let y = \nreturn y }\nlog: \"after\"\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    let f = (x) => null\n    log: \"after\"\n    return 1\n  }\n)",
        None,
        &[],
    );
    check(
        "failed_lambda_block_does_not_leak",
        "recover",
        "@QUICKFUNCS(\n~f() {\nlet f = (x) => { let y = \nreturn y }\nlog: \"after\"\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    let f = (x) => null\n    log: \"after\"\n    return 1\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn function_structure_errors() {
    check(
        "function_structure_errors",
        "halt",
        "@QUICKFUNCS(\n~f\n~g(\n~h() {\n~i() { return 1\n~j() return 1 }\n~k(a b) { return 1 }\n~l(a,) { return 1 }\n~m(a<) { return 1 }\n~n<unknown>() { return 1 }\n~o => (a) { return 1 }\n)\n",
        "NONE",
        None,
        &["Expected '{' to open body of function 'f'"],
    );
    check(
        "function_structure_errors",
        "continue",
        "@QUICKFUNCS(\n~f\n~g(\n~h() {\n~i() { return 1\n~j() return 1 }\n~k(a b) { return 1 }\n~l(a,) { return 1 }\n~m(a<) { return 1 }\n~n<unknown>() { return 1 }\n~o => (a) { return 1 }\n)\n",
        "@QUICKFUNCS(\n  ~h => global() {\n    null\n    i()\n    {}\n    null\n    k(a)\n    b\n    null\n    {}\n    null\n    l(a)\n    {}\n    null\n    m((a < null))\n    {}\n    null\n    ((n < unknown) > null)\n    {}\n    null\n    o\n    null\n    (a)\n    {}\n    null\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ':' or '=' after property 'j'",
        "Expected '{' to open body of function 'f'",
        "Expected '{' to open body of function 'g'",
        "Expected parameter name",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found ~",
        "Trailing comma in function arguments"],
    );
    check(
        "function_structure_errors",
        "recover",
        "@QUICKFUNCS(\n~f\n~g(\n~h() {\n~i() { return 1\n~j() return 1 }\n~k(a b) { return 1 }\n~l(a,) { return 1 }\n~m(a<) { return 1 }\n~n<unknown>() { return 1 }\n~o => (a) { return 1 }\n)\n",
        "@QUICKFUNCS(\n  ~h => global() {\n    null\n    i()\n    {}\n    null\n    k(a)\n    b\n    null\n    {}\n    null\n    l(a)\n    {}\n    null\n    m((a < null))\n    {}\n    null\n    ((n < unknown) > null)\n    {}\n    null\n    o\n    null\n    (a)\n    {}\n    null\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ':' or '=' after property 'j'",
        "Expected '{' to open body of function 'f'",
        "Expected '{' to open body of function 'g'",
        "Expected parameter name",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found ~",
        "Trailing comma in function arguments"],
    );
}

#[test]
fn statement_errors() {
    check(
        "statement_errors",
        "halt",
        "@QUICKFUNCS(\n~f() {\nlog \"x\"\nlog:\nif x { return 1 }\nif: { return 1 }\nif: x return 1\nlet = 1\nlet x\nconst mut y = 1\nlet mut = 1\nx =\nreturn 1\n}\n)\n",
        "NONE",
        None,
        &["Expected ':' after 'log'",
        "Expected '~' to start function definition, found x"],
    );
    check(
        "statement_errors",
        "continue",
        "@QUICKFUNCS(\n~f() {\nlog \"x\"\nlog:\nif x { return 1 }\nif: { return 1 }\nif: x return 1\nlet = 1\nlet x\nconst mut y = 1\nlet mut = 1\nx =\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    \"x\"\n    log: null\n    x\n    {}\n    if: {} { }\n    if: x { }\n    return 1\n    null\n    null\n    1\n    null\n    const y = 1\n    null\n    null\n    1\n    x = null\n    1\n  }\n)",
        None,
        &["'const' declarations cannot be mutable — use 'let mut' instead",
        "Expected ':' after 'log'",
        "Expected '=' after variable declaration 'x'",
        "Expected '{' or 'then' after if condition",
        "Expected '{' or 'then' after if condition",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected variable name after 'let'",
        "Expected variable name after 'let'"],
    );
    check(
        "statement_errors",
        "recover",
        "@QUICKFUNCS(\n~f() {\nlog \"x\"\nlog:\nif x { return 1 }\nif: { return 1 }\nif: x return 1\nlet = 1\nlet x\nconst mut y = 1\nlet mut = 1\nx =\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    \"x\"\n    log: null\n    x\n    {}\n    if: {} { }\n    if: x { }\n    return 1\n    null\n    null\n    1\n    null\n    const y = 1\n    null\n    null\n    1\n    x = null\n    1\n  }\n)",
        None,
        &["'const' declarations cannot be mutable — use 'let mut' instead",
        "Expected ':' after 'log'",
        "Expected '=' after variable declaration 'x'",
        "Expected '{' or 'then' after if condition",
        "Expected '{' or 'then' after if condition",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected variable name after 'let'",
        "Expected variable name after 'let'"],
    );
}

#[test]
fn if_and_switch_errors() {
    check(
        "if_and_switch_errors",
        "halt",
        "@QUICKFUNCS(\n~f() {\nif: x { return 1 } elif x { return 2 }\nif: x { return 1 } elif: y return 2\nif: x { return 1 } else\nchk x { -> 1 then return 1 }\nchk: x { 1 then return 1 }\nchk: x { -> 1 return 1 }\nchk: x { -> miss }\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    if: x { return 1; }\n    x\n    {}\n    if: x { return 1; }\n    return 2\n    if: x { return 1; } else { chk: null { }; }\n    x\n    {}\n    chk: x { }\n    chk: x { -> 1 { } }\n    chk: x { -> null { } }\n    return 1\n  }\n)",
        None,
        &["Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected ':'",
        "Expected ':'",
        "Expected 'then', '=>', or '{' after switch case value",
        "Expected 'then', '=>', or '{' after switch case value",
        "Expected '{' after elif condition",
        "Expected property name, found ->",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found 2",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found then"],
    );
    check(
        "if_and_switch_errors",
        "continue",
        "@QUICKFUNCS(\n~f() {\nif: x { return 1 } elif x { return 2 }\nif: x { return 1 } elif: y return 2\nif: x { return 1 } else\nchk x { -> 1 then return 1 }\nchk: x { 1 then return 1 }\nchk: x { -> 1 return 1 }\nchk: x { -> miss }\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    if: x { return 1; }\n    x\n    {}\n    if: x { return 1; }\n    return 2\n    if: x { return 1; } else { chk: null { }; }\n    x\n    {}\n    chk: x { }\n    chk: x { -> 1 { } }\n    chk: x { -> null { } }\n    return 1\n  }\n)",
        None,
        &["Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected ':'",
        "Expected ':'",
        "Expected 'then', '=>', or '{' after switch case value",
        "Expected 'then', '=>', or '{' after switch case value",
        "Expected '{' after elif condition",
        "Expected property name, found ->",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found 2",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found then"],
    );
    check(
        "if_and_switch_errors",
        "recover",
        "@QUICKFUNCS(\n~f() {\nif: x { return 1 } elif x { return 2 }\nif: x { return 1 } elif: y return 2\nif: x { return 1 } else\nchk x { -> 1 then return 1 }\nchk: x { 1 then return 1 }\nchk: x { -> 1 return 1 }\nchk: x { -> miss }\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    if: x { return 1; }\n    x\n    {}\n    if: x { return 1; }\n    return 2\n    if: x { return 1; } else { chk: null { }; }\n    x\n    {}\n    chk: x { }\n    chk: x { -> 1 { } }\n    chk: x { -> null { } }\n    return 1\n  }\n)",
        None,
        &["Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected '->' to begin switch case",
        "Expected ':'",
        "Expected ':'",
        "Expected 'then', '=>', or '{' after switch case value",
        "Expected 'then', '=>', or '{' after switch case value",
        "Expected '{' after elif condition",
        "Expected property name, found ->",
        "Expected property name, found 1",
        "Expected property name, found 1",
        "Expected property name, found 2",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found then"],
    );
}

#[test]
fn expression_errors() {
    check(
        "expression_errors",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn a ? b\nreturn a ? b :\nreturn (1\nreturn (1 + )\nreturn 1 +\nreturn * 2\nreturn @\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return a\n    return a ? b : null\n    null\n    return null\n    return (1 + (null * 2))\n    return null\n    return 1\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ':' in ternary expression"],
    );
    check(
        "expression_errors",
        "continue",
        "@QUICKFUNCS(\n~f() {\nreturn a ? b\nreturn a ? b :\nreturn (1\nreturn (1 + )\nreturn 1 +\nreturn * 2\nreturn @\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return a\n    return a ? b : null\n    null\n    return null\n    return (1 + (null * 2))\n    return null\n    return 1\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ':' in ternary expression"],
    );
    check(
        "expression_errors",
        "recover",
        "@QUICKFUNCS(\n~f() {\nreturn a ? b\nreturn a ? b :\nreturn (1\nreturn (1 + )\nreturn 1 +\nreturn * 2\nreturn @\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return a\n    return a ? b : null\n    null\n    return null\n    return (1 + (null * 2))\n    return null\n    return 1\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ':' in ternary expression"],
    );
}

#[test]
fn postfix_and_call_errors() {
    check(
        "postfix_and_call_errors",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn f(1\nreturn f(1,\nreturn f(1,)\nreturn f(,)\nreturn f(1 2)\nreturn a.\nreturn a..b\nreturn a.5\nreturn a[\nreturn a[1\nreturn a[]\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return f(1)\n    return f(1, null)\n    f(1)\n    return f(null)\n    return f(1)\n    2\n    null\n    return a\n    return a\n    null\n    b\n    return a\n    5\n    return a\n    a\n    return a\n    return 1\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected identifier after '.'",
        "Expected identifier after '.'",
        "Expected identifier after '.'",
        "Trailing comma in function arguments"],
    );
    check(
        "postfix_and_call_errors",
        "continue",
        "@QUICKFUNCS(\n~f() {\nreturn f(1\nreturn f(1,\nreturn f(1,)\nreturn f(,)\nreturn f(1 2)\nreturn a.\nreturn a..b\nreturn a.5\nreturn a[\nreturn a[1\nreturn a[]\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return f(1)\n    return f(1, null)\n    f(1)\n    return f(null)\n    return f(1)\n    2\n    null\n    return a\n    return a\n    null\n    b\n    return a\n    5\n    return a\n    a\n    return a\n    return 1\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected identifier after '.'",
        "Expected identifier after '.'",
        "Expected identifier after '.'",
        "Trailing comma in function arguments"],
    );
    check(
        "postfix_and_call_errors",
        "recover",
        "@QUICKFUNCS(\n~f() {\nreturn f(1\nreturn f(1,\nreturn f(1,)\nreturn f(,)\nreturn f(1 2)\nreturn a.\nreturn a..b\nreturn a.5\nreturn a[\nreturn a[1\nreturn a[]\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return f(1)\n    return f(1, null)\n    f(1)\n    return f(null)\n    return f(1)\n    2\n    null\n    return a\n    return a\n    null\n    b\n    return a\n    5\n    return a\n    a\n    return a\n    return 1\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected identifier after '.'",
        "Expected identifier after '.'",
        "Expected identifier after '.'",
        "Trailing comma in function arguments"],
    );
}

#[test]
fn container_and_constructor_errors() {
    check(
        "container_and_constructor_errors",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn [1, 2\nreturn [1 2]\nreturn [,]\nreturn {\nreturn {k}\nreturn {k = }\nreturn {k = 1 j = 2}\nreturn {5 = 1}\nreturn t:(\nreturn t:(1\nreturn b:(\nreturn r:(\"x\"\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return [1, 2]\n    return [1]\n    2\n    null\n    return [null]\n    return {}\n    return {k = null, k = 1, j = 2}\n    return {}\n    return null\n    null\n    return null\n    null\n    return 1\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ':' or '=' after property 'k'",
        "Expected property name, found 1",
        "Expected property name, found 5",
        "Expected property name, found =",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found {",
        "Expected property name, found {"],
    );
    check(
        "container_and_constructor_errors",
        "continue",
        "@QUICKFUNCS(\n~f() {\nreturn [1, 2\nreturn [1 2]\nreturn [,]\nreturn {\nreturn {k}\nreturn {k = }\nreturn {k = 1 j = 2}\nreturn {5 = 1}\nreturn t:(\nreturn t:(1\nreturn b:(\nreturn r:(\"x\"\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return [1, 2]\n    return [1]\n    2\n    null\n    return [null]\n    return {}\n    return {k = null, k = 1, j = 2}\n    return {}\n    return null\n    null\n    return null\n    null\n    return 1\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ':' or '=' after property 'k'",
        "Expected property name, found 1",
        "Expected property name, found 5",
        "Expected property name, found =",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found {",
        "Expected property name, found {"],
    );
    check(
        "container_and_constructor_errors",
        "recover",
        "@QUICKFUNCS(\n~f() {\nreturn [1, 2\nreturn [1 2]\nreturn [,]\nreturn {\nreturn {k}\nreturn {k = }\nreturn {k = 1 j = 2}\nreturn {5 = 1}\nreturn t:(\nreturn t:(1\nreturn b:(\nreturn r:(\"x\"\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return [1, 2]\n    return [1]\n    2\n    null\n    return [null]\n    return {}\n    return {k = null, k = 1, j = 2}\n    return {}\n    return null\n    null\n    return null\n    null\n    return 1\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ':' or '=' after property 'k'",
        "Expected property name, found 1",
        "Expected property name, found 5",
        "Expected property name, found =",
        "Expected property name, found return",
        "Expected property name, found return",
        "Expected property name, found {",
        "Expected property name, found {"],
    );
}

#[test]
fn lambda_and_interpolation_errors() {
    check(
        "lambda_and_interpolation_errors",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn (x) =>\nreturn (x) => {\nreturn (x\nreturn (1,) => 2\nreturn $\"{x\"\nreturn $\"{a +}\"\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return (x) => null\n    (x) => null\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')' after lambda parameters",
        "Expected '=>' after lambda parameters",
        "Expected '}' to close lambda block",
        "Expected parameter name in lambda"],
    );
    check(
        "lambda_and_interpolation_errors",
        "continue",
        "@QUICKFUNCS(\n~f() {\nreturn (x) =>\nreturn (x) => {\nreturn (x\nreturn (1,) => 2\nreturn $\"{x\"\nreturn $\"{a +}\"\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return (x) => null\n    (x) => null\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')' after lambda parameters",
        "Expected '=>' after lambda parameters",
        "Expected '}' to close lambda block",
        "Expected parameter name in lambda"],
    );
    check(
        "lambda_and_interpolation_errors",
        "recover",
        "@QUICKFUNCS(\n~f() {\nreturn (x) =>\nreturn (x) => {\nreturn (x\nreturn (1,) => 2\nreturn $\"{x\"\nreturn $\"{a +}\"\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return (x) => null\n    (x) => null\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')' after lambda parameters",
        "Expected '=>' after lambda parameters",
        "Expected '}' to close lambda block",
        "Expected parameter name in lambda"],
    );
}

#[test]
fn deep_parentheses() {
    let source = format!("@QUICKFUNCS(\n~f() {{\nreturn {o}1{c}\nlog: \"after\"\n}}\n)\n", o = "(".repeat(66), c = ")".repeat(66));
    check(
        "deep_parentheses",
        "continue",
        &source,
        "@QUICKFUNCS(\n  ~f => global() {\n    return null\n    null\n    ((1))\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    log: \"after\"\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression."],
    );
}

#[test]
fn deep_arrays() {
    let source = format!("@QUICKFUNCS(\n~f() {{\nreturn {o}1{c}\nlog: \"after\"\n}}\n)\n", o = "[".repeat(66), c = "]".repeat(66));
    check(
        "deep_arrays",
        "continue",
        &source,
        "@QUICKFUNCS(\n  ~f => global() {\n    return [[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[null]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]\n    1\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    log: \"after\"\n  }\n)",
        None,
        &["Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Expected ']'",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening your data structure.",
        "Maximum nesting depth (64) exceeded. Consider flattening your data structure.",
        "Maximum nesting depth (64) exceeded. Consider flattening your data structure.",
        "Maximum nesting depth (64) exceeded. Consider flattening your data structure.",
        "Maximum nesting depth (64) exceeded. Consider flattening your data structure.",
        "Maximum nesting depth (64) exceeded. Consider flattening your data structure.",
        "Maximum nesting depth (64) exceeded. Consider flattening your data structure.",
        "Maximum nesting depth (64) exceeded. Consider flattening your data structure.",
        "Maximum nesting depth (64) exceeded. Consider flattening your data structure."],
    );
}

#[test]
fn deep_calls() {
    let source = format!("@QUICKFUNCS(\n~f() {{\nreturn {o}1{c}\nlog: \"after\"\n}}\n)\n", o = "f(".repeat(66), c = ")".repeat(66));
    check(
        "deep_calls",
        "continue",
        &source,
        "@QUICKFUNCS(\n  ~f => global() {\n    return f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(null))))))))))))))))))))))))))))))))\n    f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(null))))))))))))))))))))))))))))))))\n    f(f(1))\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    null\n    log: \"after\"\n  }\n)",
        None,
        &["Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')'",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Expected ')' after function arguments",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression.",
        "Maximum nesting depth (64) exceeded. Consider flattening this expression."],
    );
}

#[test]
fn unclosed_section() {
    check(
        "unclosed_section",
        "halt",
        "@QUICKFUNCS(\n~f() {\nreturn 1\n}",
        "@QUICKFUNCS(\n  ~f => global() {\n    return 1\n  }\n)",
        None,
        &[],
    );
    check(
        "unclosed_section",
        "continue",
        "@QUICKFUNCS(\n~f() {\nreturn 1\n}",
        "@QUICKFUNCS(\n  ~f => global() {\n    return 1\n  }\n)",
        None,
        &[],
    );
    check(
        "unclosed_section",
        "recover",
        "@QUICKFUNCS(\n~f() {\nreturn 1\n}",
        "@QUICKFUNCS(\n  ~f => global() {\n    return 1\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn missing_open_paren() {
    check(
        "missing_open_paren",
        "halt",
        "@QUICKFUNCS\n~f() {\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return 1\n  }\n)",
        None,
        &[],
    );
    check(
        "missing_open_paren",
        "continue",
        "@QUICKFUNCS\n~f() {\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return 1\n  }\n)",
        None,
        &[],
    );
    check(
        "missing_open_paren",
        "recover",
        "@QUICKFUNCS\n~f() {\nreturn 1\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return 1\n  }\n)",
        None,
        &[],
    );
}

#[test]
fn stray_tokens() {
    check(
        "stray_tokens",
        "halt",
        "@QUICKFUNCS(\n;\n,\n~f() {\nreturn 1\n}\nx\n@@@\n~g() {\nreturn 2\n}\n)\n",
        "NONE",
        None,
        &["Expected '~' to start function definition, found x"],
    );
    check(
        "stray_tokens",
        "continue",
        "@QUICKFUNCS(\n;\n,\n~f() {\nreturn 1\n}\nx\n@@@\n~g() {\nreturn 2\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return 1\n  },\n  ~g => global() {\n    return 2\n  }\n)",
        None,
        &["Expected '~' to start function definition, found @",
        "Expected '~' to start function definition, found @",
        "Expected '~' to start function definition, found @",
        "Expected '~' to start function definition, found x"],
    );
    check(
        "stray_tokens",
        "recover",
        "@QUICKFUNCS(\n;\n,\n~f() {\nreturn 1\n}\nx\n@@@\n~g() {\nreturn 2\n}\n)\n",
        "@QUICKFUNCS(\n  ~f => global() {\n    return 1\n  },\n  ~g => global() {\n    return 2\n  }\n)",
        None,
        &["Expected '~' to start function definition, found @",
        "Expected '~' to start function definition, found @",
        "Expected '~' to start function definition, found @",
        "Expected '~' to start function definition, found x"],
    );
}

#[test]
fn empty_section() {
    check(
        "empty_section",
        "halt",
        "@QUICKFUNCS(\n\n)\n",
        "@QUICKFUNCS(\n)",
        None,
        &[],
    );
    check(
        "empty_section",
        "continue",
        "@QUICKFUNCS(\n\n)\n",
        "@QUICKFUNCS(\n)",
        None,
        &[],
    );
    check(
        "empty_section",
        "recover",
        "@QUICKFUNCS(\n\n)\n",
        "@QUICKFUNCS(\n)",
        None,
        &[],
    );
}

#[test]
fn positions() {
    check(
        "positions",
        "halt",
        "@QUICKFUNCS(\n  ~f<int> => g (a = 1) {\n    let x = a + 2\n    if: x > 1 { log: \"big\" } else { return [x, f(1)] }\n    return x\n  }\n)\n",
        "@QUICKFUNCS(\n  ~f<int> => g(a = 1) {\n    let x = (a + 2)\n    if: (x > 1) { log: \"big\"; } else { return [<ToEvaluate: x>, <ToEvaluate: f(1)>]; }\n    return x\n  }\n)",
        Some("QuickFuncsSection { functions: [QuickFunction { name: \"f\", return_type: Some(Int), scope_list: Some([\"g\"]), parameters: [QuickFuncParam { name: \"a\", data_type: None, default_value: Some(Value { value: Integer { value: 1, position: Position { line: 2, column: 21 } }, position: Position { line: 2, column: 21 } }), position: Position { line: 2, column: 17 } }], body: [VariableDeclaration { declaration_type: Let, is_mutable: false, variable_name: \"x\", data_type: None, value: ArithmeticOp { left: Identifier { name: \"a\", position: Position { line: 3, column: 13 } }, operator: \"+\", right: Value { value: Integer { value: 2, position: Position { line: 3, column: 17 } }, position: Position { line: 3, column: 17 } }, position: Position { line: 3, column: 15 } }, position: Position { line: 3, column: 5 } }, If { condition: ComparisonOp { left: Identifier { name: \"x\", position: Position { line: 4, column: 9 } }, operator: \">\", right: Value { value: Integer { value: 1, position: Position { line: 4, column: 13 } }, position: Position { line: 4, column: 13 } }, position: Position { line: 4, column: 11 } }, then_branch: [Log { value: Value { value: String { value: \"big\", position: Position { line: 4, column: 22 } }, position: Position { line: 4, column: 22 } }, position: Position { line: 4, column: 17 } }], else_branch: Some([Return { value: Value { value: Array { values: [Expression { expr: Identifier { name: \"x\", position: Position { line: 4, column: 45 } }, position: Position { line: 4, column: 46 } }, Expression { expr: QuickFuncCall { name: \"f\", arguments: [Value { value: Integer { value: 1, position: Position { line: 4, column: 50 } }, position: Position { line: 4, column: 50 } }], position: Position { line: 4, column: 48 } }, position: Position { line: 4, column: 52 } }], position: Position { line: 4, column: 44 } }, position: Position { line: 4, column: 44 } }, position: Position { line: 4, column: 37 } }]), position: Position { line: 4, column: 5 } }, Return { value: Identifier { name: \"x\", position: Position { line: 5, column: 12 } }, position: Position { line: 5, column: 5 } }], position: Position { line: 2, column: 3 } }], position: Position { line: 1, column: 12 } }"),
        &[],
    );
}
