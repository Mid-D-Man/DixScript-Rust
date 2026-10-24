// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "tests/type_inference_arena.rs"
// ============================================================================
//! Differential test for the arena type inference visitor.
//!
//! `TypeInferenceVisitor` has two implementations: the original, which reads the owned
//! `Value` and `Expression` types, and `type_inference_arena.rs`, which reads the same trees in
//! an `Arena`. They must agree on every node. The test compiles every file in `mdix_files/` and a
//! set of crafted sources, takes every value and expression node of each document, and compares
//! the two implementations on it:
//!
//! * for the type of the node and for its element type;
//! * under three visitor configurations: no local variables, local variables of assorted types,
//!   and local variables with element-type hints, so that the branches that depend on them are
//!   reached;
//! * with the symbol table the semantic analysis produced for the file, so that functions, data
//!   variables, enums and imported namespaces resolve.
//!
//! The nodes come from `Arena::value_ids` and `expr_ids`, so the walk includes nodes a failed
//! sub-expression left behind and cannot miss a nested one. The owned side is the original code
//! and is not changed by the arena work, so it is the oracle.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use dixscript::Compiler::AST::arena::{Arena, Document, ExprNode, ValueNode};
use dixscript::Compiler::AST::data_types::{CompatibilityMode, ElemType, ErrorHandlingStrategy};
use dixscript::Compiler::AST::{DataType, Expression, Position, Value};
use dixscript::Compiler::AST::Visitors::TypeInferenceVisitor;
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::Tokenizer::{split_config_tokens, Tokenizer};
use dixscript::Compiler::Core::{GeneralParser, GeneralSemanticAnalyzer};
use dixscript::Compiler::Utilities::{SymbolTable, VariableInfo};
use dixscript::ErrorManager::ErrorManager;

/// Parses and analyzes one source. Returns the document and the symbol table, or `None` when the
/// parse fails.
fn compile(bytes: &[u8], path: &str) -> Option<(Document, SymbolTable)> {
    let em = ErrorManager::new_isolated_silent();
    let initial = OperationalSettings {
        source_file_path: Some(path.to_string()),
        compatibility_mode: CompatibilityMode::Strict,
        ..OperationalSettings::default()
    };
    let tokens = Tokenizer::new_with_error_manager_from_bytes(bytes, &initial, em.clone()).tokenize();
    let split = split_config_tokens(tokens.tokens);
    let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
    let config = handler.process_config_tokens(&split.config_tokens);
    let mut settings = config.operational_settings;
    settings.source_file_path = Some(path.to_string());
    // Continue, so that the symbol table is filled in even when a section has errors.
    settings.error_handling_strategy = ErrorHandlingStrategy::Continue;
    em.update_settings(settings.clone());
    let document = GeneralParser::new_for_lsp(split.rest_tokens, &config.config_section, &settings, em.clone())
        .ok()?
        .parse_document()
        .ok()?;
    let ast = document.export();
    let result = GeneralSemanticAnalyzer::new_with_error_manager(&ast, &settings, em)
        .with_document(&document)
        .analyze();
    Some((document, result.symbol_table.unwrap_or_else(SymbolTable::new)))
}

/// The types the synthesized local variables cycle through. They cover plain scalars, untyped and
/// typed collections, and the kinds the method tables care about.
fn local_types() -> Vec<Option<DataType>> {
    vec![
        Some(DataType::Int),
        Some(DataType::TypedArray(ElemType::Int)),
        Some(DataType::TypedArray(ElemType::String)),
        Some(DataType::TypedTuple([Some(ElemType::Int), Some(ElemType::String), None, None, None, None])),
        Some(DataType::TypedTuple([Some(ElemType::Float), Some(ElemType::Float), None, None, None, None])),
        Some(DataType::String),
        Some(DataType::Object),
        Some(DataType::Array),
        Some(DataType::Tuple),
        Some(DataType::Regex),
        Some(DataType::Float),
        Some(DataType::Bool),
        Some(DataType::Enum),
        None,
    ]
}

/// Every identifier-like name in the arena, with a stable type assignment.
fn synthesized_locals(arena: &Arena) -> HashMap<String, Option<DataType>> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for id in arena.expr_ids() {
        match arena.expr(id) {
            ExprNode::Identifier { name, .. } => {
                names.insert(arena.text(*name).to_string());
            }
            ExprNode::QualifiedIdentifier { parts, .. } => {
                if let Some(first) = arena.str_list(*parts).first() {
                    names.insert(arena.text(*first).to_string());
                }
            }
            _ => {}
        }
    }
    for id in arena.value_ids() {
        if let ValueNode::Identifier { value, .. } = arena.value(id) {
            names.insert(arena.text(*value).to_string());
        }
    }
    let types = local_types();
    names
        .into_iter()
        .enumerate()
        .map(|(i, name)| (name, types[i % types.len()]))
        .collect()
}

fn hints_for(locals: &HashMap<String, Option<DataType>>) -> HashMap<String, DataType> {
    let hint_types = [DataType::Int, DataType::String, DataType::Float, DataType::Bool];
    let mut names: Vec<&String> = locals.keys().collect();
    names.sort();
    names
        .into_iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .map(|(i, name)| (name.clone(), hint_types[i % hint_types.len()]))
        .collect()
}

/// What one comparison run saw, to show the test was not vacuous.
#[derive(Default)]
struct Tally {
    nodes: usize,
    some: usize,
    none: usize,
    kinds: BTreeSet<String>,
}

/// Compares the owned and arena visitors on every node of `document`.
fn compare(label: &str, document: &Document, table: &SymbolTable, tally: &mut Tally) {
    let arena = &document.arena;
    let locals = synthesized_locals(arena);
    let hints = hints_for(&locals);
    let configs: [(&str, Option<HashMap<String, Option<DataType>>>, Option<HashMap<String, DataType>>); 3] = [
        ("empty", None, None),
        ("locals", Some(locals.clone()), None),
        ("locals+hints", Some(locals.clone()), Some(hints)),
    ];

    for (config_name, local_types, element_hints) in configs {
        let visitor = TypeInferenceVisitor::new_with_element_hints(table, local_types, element_hints);

        for id in arena.value_ids() {
            let owned = arena.export_value(id);
            let by_owned = visitor.infer_type_from_value(&owned);
            let by_arena = visitor.infer_type_from_value_id(arena, id);
            assert_eq!(by_owned, by_arena, "{label} [{config_name}]: type of value {id:?}: {owned:?}");
            let elem_owned = visitor.infer_element_type_from_value(&owned);
            let elem_arena = visitor.infer_element_type_from_value_id(arena, id);
            assert_eq!(elem_owned, elem_arena, "{label} [{config_name}]: element type of value {id:?}: {owned:?}");

            tally.nodes += 1;
            match by_arena {
                Some(t) => {
                    tally.some += 1;
                    tally.kinds.insert(format!("{t:?}"));
                }
                None => tally.none += 1,
            }
            if elem_arena.is_some() {
                tally.kinds.insert("element-of-value".to_string());
            }
        }

        for id in arena.expr_ids() {
            let owned = arena.export_expr(id);
            let by_owned = visitor.infer_type_from_expression(&owned);
            let by_arena = visitor.infer_type_from_expression_id(arena, id);
            assert_eq!(by_owned, by_arena, "{label} [{config_name}]: type of expression {id:?}: {owned:?}");
            let elem_owned = visitor.infer_element_type_from_expression(&owned);
            let elem_arena = visitor.infer_element_type_from_expression_id(arena, id);
            assert_eq!(elem_owned, elem_arena, "{label} [{config_name}]: element type of expression {id:?}: {owned:?}");

            tally.nodes += 1;
            match by_arena {
                Some(t) => {
                    tally.some += 1;
                    tally.kinds.insert(format!("{t:?}"));
                }
                None => tally.none += 1,
            }
            if elem_arena.is_some() {
                tally.kinds.insert("element-of-expression".to_string());
            }
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).expect("readable corpus").filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.path());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().map(|x| x == "mdix").unwrap_or(false) {
            out.push(p);
        }
    }
}

#[test]
fn the_two_implementations_agree_on_every_node_of_the_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("mdix_files");
    let mut files = Vec::new();
    walk(&root, &mut files);
    assert!(files.len() >= 50, "expected the corpus under {}, found {}", root.display(), files.len());

    let mut tally = Tally::default();
    let mut compiled = 0;
    for path in &files {
        let bytes = std::fs::read(path).expect("readable file");
        let Some((document, table)) = compile(&bytes, &path.to_string_lossy()) else { continue };
        compare(&path.display().to_string(), &document, &table, &mut tally);
        compiled += 1;
    }

    println!(
        "{compiled} files, {} comparisons ({} with a type, {} without), kinds seen: {:?}",
        tally.nodes, tally.some, tally.none, tally.kinds
    );
    assert!(compiled >= 60, "only {compiled} corpus files compiled");
    assert!(tally.nodes >= 20_000, "only {} comparisons", tally.nodes);
    assert!(tally.some > 5_000 && tally.none > 1_000, "the comparison was lopsided: {} typed, {} untyped", tally.some, tally.none);
    // The comparison must have seen results of several different kinds, not only integers.
    for kind in ["Int", "String", "Bool", "Double", "Enum", "Object", "Array"] {
        assert!(tally.kinds.contains(kind), "no node was inferred as {kind}; saw {:?}", tally.kinds);
    }
    assert!(
        tally.kinds.iter().any(|k| k.starts_with("TypedArray")) && tally.kinds.iter().any(|k| k.starts_with("TypedTuple")),
        "no typed collection was inferred; saw {:?}",
        tally.kinds
    );
}

/// Sources that reach branches the corpus is unlikely to: builtin static calls whose result
/// depends on their arguments, instance methods on typed collections, conditionals, casts,
/// property and index access.
const CRAFTED: &[(&str, &str)] = &[
    (
        "static_calls",
        "@QUICKFUNCS(\n  ~f() {\n    let a = Array.of(1, 2, 3)\n    let b = Array.of(1, \"x\")\n    let c = Array.range(1, 5)\n    let d = Array.fill(0, 3)\n    let e = Array.concat(a, c)\n    let g = Array.reverse(a)\n    let h = Array.repeat(a, 2)\n    let i = Random.choice(a)\n    let j = Random.choices(a, 2)\n    let k = Array.empty()\n    let l = Array.fromString(\"a,b\", \",\")\n    let m = Array.flatten(a)\n    let n = Math.sqrt(4)\n    return a\n  }\n)\n@DATA(\n  x = 1\n)\n",
    ),
    (
        "instance_methods",
        "@QUICKFUNCS(\n  ~f() {\n    let a = [1, 2, 3]\n    let s = \"a b\"\n    let t = t:(1, \"x\", true)\n    let r = r:(\"a+\")\n    let o = {k = 1}\n    let a1 = a.first()\n    let a2 = a.reverse()\n    let a3 = a.flatten()\n    let s1 = s.split(\" \")\n    let s2 = s.upper()\n    let t1 = t.first()\n    let t2 = t.get(1)\n    let t3 = t.toArray()\n    let t4 = t.reverse()\n    let r1 = r.match(\"aa\")\n    let r2 = r.matchAll(\"aa\")\n    let o1 = o.keys()\n    let o2 = o.values()\n    let o3 = o.count()\n    return a1\n  }\n)\n@DATA(\n  x = 1\n)\n",
    ),
    (
        "operators",
        "@QUICKFUNCS(\n  ~f(n) {\n    let a = 1 + 2\n    let b = 1 + 2.5\n    let c = 1L + 2\n    let d = \"a\" + 1\n    let e = 1.5f + 2\n    let g = !true\n    let h = 1 < 2\n    let i = true && false\n    let j = 5 & 3\n    let k = -n\n    let l = true ? 1 : 2\n    let m = (1 + 2) * 3\n    return a\n  }\n)\n@DATA(\n  x = 1\n)\n",
    ),
    (
        "data_values_and_access",
        "@ENUMS(\n  LogLevel { DEBUG = 0, INFO = 1 }\n)\n@DATA(\n  level<enum> = LogLevel.INFO\n  nums = [1, 2, 3]\n  mixed = [1, \"a\"]\n  pair = t:(1, \"x\")\n  triple = t:(1.5, 2.5, 3.5)\n  blob = b:(\"SGVsbG8=\")\n  re = r:(\"a+\")\n  obj = {a = 1, b = {c = 2}}\n  empty = []\n  n = null\n  nested = [[1, 2], [3]]\n)\n@QUICKFUNCS(\n  ~g() {\n    let x = nums[0]\n    let y = obj.b.c\n    let z = DATA.nums\n    let p = pair.first()\n    return x\n  }\n)\n",
    ),
];

#[test]
fn the_two_implementations_agree_on_crafted_sources() {
    let mut tally = Tally::default();
    let mut compiled = 0;
    for (name, text) in CRAFTED {
        let Some((document, table)) = compile(text.as_bytes(), name) else { continue };
        compare(name, &document, &table, &mut tally);
        compiled += 1;
    }
    println!("{compiled} crafted sources, {} comparisons, kinds seen: {:?}", tally.nodes, tally.kinds);
    assert!(compiled >= 3, "only {compiled} crafted sources compiled");
    assert!(tally.nodes >= 300, "only {} comparisons", tally.nodes);
}

// ==================== nodes built by hand ====================

fn pos() -> Position {
    Position::new(1, 1)
}

fn int(n: i32) -> Value {
    Value::Integer { value: n, position: pos() }
}

fn text(t: &str) -> Value {
    Value::String { value: t.to_string(), position: pos() }
}

fn ident(name: &str) -> Expression {
    Expression::Identifier { name: name.to_string(), position: pos() }
}

fn ctor(prefix: &str, arguments: Vec<Value>) -> Value {
    Value::PrefixedConstructor { prefix: prefix.to_string(), arguments, position: pos() }
}

fn variable(name: &str, t: DataType) -> VariableInfo {
    VariableInfo {
        name: name.to_string(),
        declared_type: Some(t),
        inferred_type: Some(t),
        is_inferred: false,
        scope: "global".to_string(),
        line: 1,
        column: 1,
    }
}

/// Compares the two implementations on one owned value, and returns what they agreed on.
fn same_value(visitor: &TypeInferenceVisitor, label: &str, value: &Value) -> (Option<DataType>, Option<DataType>) {
    let mut arena = Arena::new();
    let id = arena.import_value(value);
    let owned = (visitor.infer_type_from_value(value), visitor.infer_element_type_from_value(value));
    let arena_side = (visitor.infer_type_from_value_id(&arena, id), visitor.infer_element_type_from_value_id(&arena, id));
    assert_eq!(owned, arena_side, "{label}: {value:?}");
    owned
}

/// Compares the two implementations on one owned expression, and returns what they agreed on.
fn same_expression(visitor: &TypeInferenceVisitor, label: &str, expr: &Expression) -> (Option<DataType>, Option<DataType>) {
    let mut arena = Arena::new();
    let id = arena.import_expr(expr);
    let owned = (visitor.infer_type_from_expression(expr), visitor.infer_element_type_from_expression(expr));
    let arena_side = (
        visitor.infer_type_from_expression_id(&arena, id),
        visitor.infer_element_type_from_expression_id(&arena, id),
    );
    assert_eq!(owned, arena_side, "{label}: {expr:?}");
    owned
}

#[test]
fn constructor_prefixes_are_matched_ignoring_case() {
    let table = SymbolTable::new();
    let visitor = TypeInferenceVisitor::new(&table, None);

    // `T:(1, 2)` and `t:(1, 2)` are the same tuple, and the element type of either is its first
    // argument's type.
    for prefix in ["t", "T"] {
        let (t, element) = same_value(&visitor, "tuple", &ctor(prefix, vec![int(1), int(2)]));
        assert!(matches!(t, Some(DataType::TypedTuple(_))), "{prefix}: {t:?}");
        assert_eq!(element, Some(DataType::Int), "{prefix}");
    }
    for (prefix, expected) in [("b", DataType::Blob), ("B", DataType::Blob), ("r", DataType::Regex), ("R", DataType::Regex)] {
        assert_eq!(same_value(&visitor, "constructor", &ctor(prefix, vec![text("x")])).0, Some(expected), "{prefix}");
    }
    // An unknown prefix has no type.
    assert_eq!(same_value(&visitor, "unknown prefix", &ctor("z", vec![int(1)])).0, None);
    // A tuple of more than six arguments types only the first six.
    let long = ctor("t", (1..=8).map(int).collect());
    assert!(matches!(same_value(&visitor, "long tuple", &long).0, Some(DataType::TypedTuple(_))));
    // A tuple whose arguments have no element type is an untyped tuple.
    let untyped = ctor("t", vec![Value::Null { position: pos() }]);
    assert_eq!(same_value(&visitor, "untyped tuple", &untyped).0, Some(DataType::Tuple));
}

#[test]
fn an_object_access_path_is_looked_up_with_dots() {
    let mut table = SymbolTable::new();
    table.add_data_variable("server.port".to_string(), variable("server.port", DataType::Int));
    table.add_data_variable("DATA.config.name".to_string(), variable("DATA.config.name", DataType::String));
    let visitor = TypeInferenceVisitor::new(&table, None);

    let access = |path: &[&str]| Expression::ObjectAccess { path: path.iter().map(|s| s.to_string()).collect(), position: pos() };
    assert_eq!(same_expression(&visitor, "plain path", &access(&["server", "port"])).0, Some(DataType::Int));
    assert_eq!(same_expression(&visitor, "DATA prefix", &access(&["config", "name"])).0, Some(DataType::String));
    assert_eq!(same_expression(&visitor, "unknown path", &access(&["server", "nope"])).0, None);
}

#[test]
fn an_index_access_takes_its_type_from_the_collection() {
    let table = SymbolTable::new();
    let index = |object: Expression| Expression::IndexAccess {
        object: Box::new(object),
        index: Box::new(Expression::Value { value: int(0), position: pos() }),
        position: pos(),
    };

    let tuple = DataType::TypedTuple([Some(ElemType::Int), Some(ElemType::String), None, None, None, None]);
    let cases: [(&str, DataType, Option<DataType>); 7] = [
        ("typed array", DataType::TypedArray(ElemType::Float), Some(DataType::Float)),
        ("typed tuple", tuple, Some(DataType::Int)),
        ("string", DataType::String, Some(DataType::String)),
        ("plain array", DataType::Array, None),
        ("plain tuple", DataType::Tuple, None),
        ("object", DataType::Object, None),
        ("integer", DataType::Int, None),
    ];
    for (label, local, expected) in cases {
        let mut locals = HashMap::new();
        locals.insert("x".to_string(), Some(local));
        let visitor = TypeInferenceVisitor::new(&table, Some(locals));
        assert_eq!(same_expression(&visitor, label, &index(ident("x"))).0, expected, "{label}");
    }

    // With an element-type hint, a plain array has an element type to give.
    let mut locals = HashMap::new();
    locals.insert("x".to_string(), Some(DataType::Array));
    let mut hints = HashMap::new();
    hints.insert("x".to_string(), DataType::Bool);
    let visitor = TypeInferenceVisitor::new_with_element_hints(&table, Some(locals), Some(hints));
    assert_eq!(same_expression(&visitor, "hinted array", &index(ident("x"))).0, Some(DataType::Bool));
}

#[test]
fn collections_and_conditionals_agree_on_hand_built_nodes() {
    let table = SymbolTable::new();
    let visitor = TypeInferenceVisitor::new(&table, None);
    let array = |values: Vec<Value>| Value::Array { values, position: pos() };

    // Uniform, mixed, empty and nested arrays, with and without a typeless item.
    for (label, value) in [
        ("uniform ints", array(vec![int(1), int(2)])),
        ("mixed", array(vec![int(1), text("a")])),
        ("empty", array(vec![])),
        ("nested", array(vec![array(vec![int(1)]), array(vec![int(2)])])),
        ("typeless item", array(vec![int(1), Value::Null { position: pos() }, int(3)])),
    ] {
        same_value(&visitor, label, &value);
    }

    // A conditional takes the first branch that has a type.
    let branch = |v: Value| Expression::Value { value: v, position: pos() };
    let conditional = |a: Value, b: Value| Expression::Conditional {
        condition: Box::new(branch(Value::Boolean { value: true, position: pos() })),
        true_value: Box::new(branch(a)),
        false_value: Box::new(branch(b)),
        position: pos(),
    };
    assert_eq!(same_expression(&visitor, "both typed", &conditional(int(1), text("a"))).0, Some(DataType::Int));
    assert_eq!(same_expression(&visitor, "first untyped", &conditional(Value::Null { position: pos() }, text("a"))).0, Some(DataType::String));
    assert_eq!(same_expression(&visitor, "both untyped", &conditional(Value::Null { position: pos() }, Value::Null { position: pos() })).0, None);
}
