// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "tests/arena_roundtrip.rs"
// ============================================================================
//! Proves the index arena (`Compiler::AST::arena`) holds the `Value`, `Expression` and
//! `QuickFuncStatement` family completely: importing a tree and exporting it again gives back
//! an equal tree.
//!
//! Three layers. Hand-built samples cover every variant of every type, plus the cases a
//! lossy copy would get wrong (`None` against an empty list, a switch with and without a
//! default). The same samples go into one shared arena to catch ids or spans that collide.
//! Then every `.mdix` file under `mdix_files/` is compiled with the real compiler and every
//! `Value`, `Expression` and statement reachable from its sections is round-tripped, which
//! covers the shapes the compiler really produces.

use dixscript::Compiler::AST::arena::{Arena, ExprId, ExprNode, StmtId, StmtNode, ValueId, ValueNode};
use dixscript::Compiler::AST::{
    DataEntry, DataType, DeclarationType, DixScript, ElemType, Expression, ObjectProperty,
    Position, QuickFuncStatement, SwitchCase, Value,
};
use dixscript::Compiler::VersionControl::CompatibilityResult;
use dixscript::Runtime::DixLoader;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn pos(n: usize) -> Position {
    Position::new(n, n + 1)
}

fn ident(name: &str, n: usize) -> Expression {
    Expression::Identifier { name: name.to_string(), position: pos(n) }
}

fn int(v: i32, n: usize) -> Value {
    Value::Integer { value: v, position: pos(n) }
}

// -----------------------------------------------------------------------------
// Samples: one of every variant
// -----------------------------------------------------------------------------

fn sample_values() -> Vec<Value> {
    let call_args = vec![ident("a", 1), Expression::Value { value: int(2, 2), position: pos(2) }];
    vec![
        Value::Integer { value: -7, position: pos(1) },
        Value::Long { value: 9_000_000_000, position: pos(2) },
        Value::Float { value: 1.5, position: pos(3) },
        Value::Double { value: 4.0, position: pos(4) },
        Value::ScientificNotation { value: 1.5e10, position: pos(5) },
        Value::String { value: "h\u{e9}llo \"quoted\" \u{1f600}".to_string(), position: pos(6) },
        Value::Boolean { value: true, position: pos(7) },
        Value::InterpolatedString {
            template: "a {x} b".to_string(),
            expressions: vec![ident("x", 8)],
            position: pos(8),
        },
        Value::HexColor { value: "#FF00AA".to_string(), position: pos(9) },
        Value::Date { value: "2026-01-02".to_string(), position: pos(10) },
        Value::Timestamp { value: "2026-01-02T03:04:05Z".to_string(), position: pos(11) },
        Value::Null { position: pos(12) },
        Value::Array {
            values: vec![int(1, 13), Value::String { value: "s".into(), position: pos(13) }, Value::Array { values: vec![], position: pos(13) }],
            position: pos(13),
        },
        Value::NestedArray { values: vec![int(1, 14), int(2, 14)], level: 2, position: pos(14) },
        Value::Object {
            properties: vec![
                ObjectProperty::new("a".into(), int(1, 15), pos(15)),
                ObjectProperty::new(
                    "b".into(),
                    Value::Object {
                        properties: vec![ObjectProperty::new("c".into(), Value::Null { position: pos(15) }, pos(15))],
                        position: pos(15),
                    },
                    pos(15),
                ),
            ],
            position: pos(15),
        },
        Value::PrefixedConstructor { prefix: "rgb".into(), arguments: vec![int(1, 16), int(2, 16), int(3, 16)], position: pos(16) },
        Value::EnumValue { enum_name: "Color".into(), value: "Red".into(), position: pos(17) },
        Value::Identifier { value: "x".into(), position: pos(18) },
        Value::QuickFuncCall { function_name: "f".into(), arguments: call_args.clone(), position: pos(19) },
        Value::Expression { expr: Box::new(ident("e", 20)), position: pos(20) },
        Value::Range { start: Box::new(int(1, 21)), end: Box::new(int(9, 21)), position: pos(21) },
        Value::Lambda {
            parameters: vec!["a".into(), "b".into()],
            body: Box::new(ident("a", 22)),
            statements: vec![QuickFuncStatement::Return { value: ident("b", 22), position: pos(22) }],
            position: pos(22),
        },
        Value::ParseError { message: "unexpected token".into(), position: pos(23) },
        Value::Error { message: "boom".into(), position: pos(24) },
        Value::Unknown {
            element_type: "widget".into(),
            element_name: "w".into(),
            element_data: "{1, 2}".into(),
            compatibility_result: CompatibilityResult::Incompatible {
                reason: "too new".into(),
                required_version: Some("3.0.0".into()),
            },
            position: pos(25),
        },
    ]
}

fn sample_exprs() -> Vec<Expression> {
    let a = || ident("a", 1);
    let args = || vec![ident("p", 2), ident("q", 3)];
    let bin = |op: &str| (Box::new(a()), op.to_string(), Box::new(ident("b", 4)));
    let (l1, o1, r1) = bin("+");
    let (l2, o2, r2) = bin("&");
    let (l3, o3, r3) = bin(">");
    let (l4, o4, r4) = bin("&&");
    vec![
        ident("x", 1),
        Expression::QualifiedIdentifier { parts: vec!["a".into(), "b".into()], arguments: Some(args()), position: pos(2) },
        Expression::FunctionCall { name: "f".into(), arguments: args(), position: pos(3) },
        Expression::QuickFuncCall { name: "g".into(), arguments: args(), position: pos(4) },
        Expression::DixFunctionCall { function_name: "h".into(), arguments: args(), position: pos(5) },
        Expression::StaticMethodCall { object_name: "Math".into(), method_name: "max".into(), arguments: args(), position: pos(6) },
        Expression::InstanceMethodCall { instance: Box::new(a()), method_name: "len".into(), arguments: args(), position: pos(7) },
        Expression::BuiltinFunction { target: Box::new(a()), method: "upper".into(), arguments: Some(args()), position: pos(8) },
        Expression::StaticFunction { class_name: "Str".into(), method: "join".into(), arguments: args(), position: pos(9) },
        Expression::ImportedFunctionCall { namespace_name: "ns".into(), function_name: "fn1".into(), arguments: args(), position: pos(10) },
        Expression::ArithmeticOp { left: l1, operator: o1, right: r1, position: pos(11) },
        Expression::BitwiseOp { left: l2, operator: o2, right: r2, position: pos(12) },
        Expression::ComparisonOp { left: l3, operator: o3, right: r3, position: pos(13) },
        Expression::LogicalOp { left: l4, operator: o4, right: r4, position: pos(14) },
        Expression::UnaryOp { operator: "-".into(), operand: Box::new(a()), position: pos(15) },
        Expression::ConfigAccess { key: "version".into(), position: pos(16) },
        Expression::EnumAccess { namespace_name: Some("ns".into()), enum_name: "Color".into(), value: "Red".into(), position: pos(17) },
        Expression::ObjectAccess { path: vec!["a".into(), "b".into(), "c".into()], position: pos(18) },
        Expression::PropertyAccess { object: Box::new(a()), property: "p".into(), position: pos(19) },
        Expression::IndexAccess { object: Box::new(a()), index: Box::new(ident("i", 20)), position: pos(20) },
        Expression::Value { value: sample_values().remove(14), position: pos(21) },
        Expression::Parenthesized { expression: Box::new(a()), position: pos(22) },
        Expression::Conditional {
            condition: Box::new(a()),
            true_value: Box::new(ident("t", 23)),
            false_value: Box::new(ident("f", 23)),
            position: pos(23),
        },
        Expression::TypeCast {
            expression: Box::new(a()),
            target_type: DataType::TypedTuple([Some(ElemType::Int), None, Some(ElemType::Bool), None, None, None]),
            position: pos(24),
        },
    ]
}

fn sample_stmts() -> Vec<QuickFuncStatement> {
    let e = |n: &str| ident(n, 1);
    let case = |v: i32| SwitchCase::new(int(v, 5), vec![QuickFuncStatement::Log { value: e("c"), position: pos(5) }], pos(5));
    vec![
        QuickFuncStatement::Return { value: e("r"), position: pos(1) },
        QuickFuncStatement::If {
            condition: e("c"),
            then_branch: vec![QuickFuncStatement::Return { value: e("t"), position: pos(2) }],
            else_branch: Some(vec![QuickFuncStatement::Return { value: e("f"), position: pos(2) }]),
            position: pos(2),
        },
        QuickFuncStatement::Switch {
            expression: e("s"),
            cases: vec![case(1), case(2)],
            default_case: Some(case(0)),
            position: pos(3),
        },
        QuickFuncStatement::Assignment { variable: "v".into(), value: e("a"), position: pos(4) },
        QuickFuncStatement::ArithmeticAssignment { variable: "v".into(), operator: "+=".into(), value: e("a"), position: pos(5) },
        QuickFuncStatement::ObjectCreation { variable: "o".into(), object: sample_values().remove(14), position: pos(6) },
        QuickFuncStatement::Log { value: e("l"), position: pos(7) },
        QuickFuncStatement::ExpressionStatement { expression: e("x"), position: pos(8) },
        QuickFuncStatement::VariableDeclaration {
            declaration_type: DeclarationType::Const,
            is_mutable: false,
            variable_name: "k".into(),
            data_type: Some(DataType::TypedArray(ElemType::Int)),
            value: e("v"),
            position: pos(9),
        },
    ]
}

/// Cases where a copy that merges `None` with an empty list, or loses a default, would
/// still pass the plain samples.
fn edge_exprs() -> Vec<Expression> {
    vec![
        Expression::QualifiedIdentifier { parts: vec![], arguments: None, position: pos(1) },
        Expression::QualifiedIdentifier { parts: vec!["a".into()], arguments: Some(vec![]), position: pos(2) },
        Expression::BuiltinFunction { target: Box::new(ident("t", 3)), method: "m".into(), arguments: None, position: pos(3) },
        Expression::BuiltinFunction { target: Box::new(ident("t", 4)), method: "m".into(), arguments: Some(vec![]), position: pos(4) },
        Expression::EnumAccess { namespace_name: None, enum_name: "E".into(), value: "V".into(), position: pos(5) },
        Expression::EnumAccess { namespace_name: Some(String::new()), enum_name: "E".into(), value: "V".into(), position: pos(6) },
        Expression::FunctionCall { name: String::new(), arguments: vec![], position: pos(7) },
    ]
}

fn edge_stmts() -> Vec<QuickFuncStatement> {
    let cond = || ident("c", 1);
    vec![
        QuickFuncStatement::If { condition: cond(), then_branch: vec![], else_branch: None, position: pos(1) },
        QuickFuncStatement::If { condition: cond(), then_branch: vec![], else_branch: Some(vec![]), position: pos(2) },
        QuickFuncStatement::Switch { expression: cond(), cases: vec![], default_case: None, position: pos(3) },
        QuickFuncStatement::Switch {
            expression: cond(),
            cases: vec![],
            default_case: Some(SwitchCase::new(int(0, 4), vec![], pos(4))),
            position: pos(4),
        },
        QuickFuncStatement::VariableDeclaration {
            declaration_type: DeclarationType::Let,
            is_mutable: true,
            variable_name: "m".into(),
            data_type: None,
            value: cond(),
            position: pos(5),
        },
    ]
}

fn edge_values() -> Vec<Value> {
    vec![
        Value::Array { values: vec![], position: pos(1) },
        Value::Object { properties: vec![], position: pos(2) },
        Value::String { value: String::new(), position: pos(3) },
        Value::Lambda { parameters: vec![], body: Box::new(ident("b", 4)), statements: vec![], position: pos(4) },
        Value::PrefixedConstructor { prefix: "p".into(), arguments: vec![], position: pos(5) },
        Value::Unknown {
            element_type: String::new(),
            element_name: String::new(),
            element_data: String::new(),
            compatibility_result: CompatibilityResult::Compatible,
            position: pos(6),
        },
        Value::Unknown {
            element_type: "t".into(),
            element_name: "n".into(),
            element_data: "d".into(),
            compatibility_result: CompatibilityResult::Deprecated { message: "old".into(), suggested_alternative: None },
            position: pos(7),
        },
        Value::Unknown {
            element_type: "t".into(),
            element_name: "n".into(),
            element_data: "d".into(),
            compatibility_result: CompatibilityResult::CompatibleWithWarnings { warnings: vec!["w1".into(), "w2".into()] },
            position: pos(8),
        },
    ]
}

// -----------------------------------------------------------------------------
// Variant coverage: the names come from matches with no wildcard arm, so a new variant
// stops this file compiling until a sample is added, and the sample count assertions below
// stop a variant being listed without one.
// -----------------------------------------------------------------------------

fn value_variant(v: &Value) -> &'static str {
    match v {
        Value::Integer { .. } => "Integer",
        Value::Long { .. } => "Long",
        Value::Float { .. } => "Float",
        Value::Double { .. } => "Double",
        Value::ScientificNotation { .. } => "ScientificNotation",
        Value::String { .. } => "String",
        Value::Boolean { .. } => "Boolean",
        Value::InterpolatedString { .. } => "InterpolatedString",
        Value::HexColor { .. } => "HexColor",
        Value::Date { .. } => "Date",
        Value::Timestamp { .. } => "Timestamp",
        Value::Null { .. } => "Null",
        Value::Array { .. } => "Array",
        Value::NestedArray { .. } => "NestedArray",
        Value::Object { .. } => "Object",
        Value::PrefixedConstructor { .. } => "PrefixedConstructor",
        Value::EnumValue { .. } => "EnumValue",
        Value::Identifier { .. } => "Identifier",
        Value::QuickFuncCall { .. } => "QuickFuncCall",
        Value::Expression { .. } => "Expression",
        Value::Range { .. } => "Range",
        Value::Lambda { .. } => "Lambda",
        Value::ParseError { .. } => "ParseError",
        Value::Error { .. } => "Error",
        Value::Unknown { .. } => "Unknown",
    }
}

fn expr_variant(e: &Expression) -> &'static str {
    match e {
        Expression::Identifier { .. } => "Identifier",
        Expression::QualifiedIdentifier { .. } => "QualifiedIdentifier",
        Expression::FunctionCall { .. } => "FunctionCall",
        Expression::QuickFuncCall { .. } => "QuickFuncCall",
        Expression::DixFunctionCall { .. } => "DixFunctionCall",
        Expression::StaticMethodCall { .. } => "StaticMethodCall",
        Expression::InstanceMethodCall { .. } => "InstanceMethodCall",
        Expression::BuiltinFunction { .. } => "BuiltinFunction",
        Expression::StaticFunction { .. } => "StaticFunction",
        Expression::ImportedFunctionCall { .. } => "ImportedFunctionCall",
        Expression::ArithmeticOp { .. } => "ArithmeticOp",
        Expression::BitwiseOp { .. } => "BitwiseOp",
        Expression::ComparisonOp { .. } => "ComparisonOp",
        Expression::LogicalOp { .. } => "LogicalOp",
        Expression::UnaryOp { .. } => "UnaryOp",
        Expression::ConfigAccess { .. } => "ConfigAccess",
        Expression::EnumAccess { .. } => "EnumAccess",
        Expression::ObjectAccess { .. } => "ObjectAccess",
        Expression::PropertyAccess { .. } => "PropertyAccess",
        Expression::IndexAccess { .. } => "IndexAccess",
        Expression::Value { .. } => "Value",
        Expression::Parenthesized { .. } => "Parenthesized",
        Expression::Conditional { .. } => "Conditional",
        Expression::TypeCast { .. } => "TypeCast",
    }
}

fn stmt_variant(s: &QuickFuncStatement) -> &'static str {
    match s {
        QuickFuncStatement::Return { .. } => "Return",
        QuickFuncStatement::If { .. } => "If",
        QuickFuncStatement::Switch { .. } => "Switch",
        QuickFuncStatement::Assignment { .. } => "Assignment",
        QuickFuncStatement::ArithmeticAssignment { .. } => "ArithmeticAssignment",
        QuickFuncStatement::ObjectCreation { .. } => "ObjectCreation",
        QuickFuncStatement::Log { .. } => "Log",
        QuickFuncStatement::ExpressionStatement { .. } => "ExpressionStatement",
        QuickFuncStatement::VariableDeclaration { .. } => "VariableDeclaration",
    }
}

#[test]
fn the_samples_cover_every_variant_once() {
    let values: BTreeSet<_> = sample_values().iter().map(value_variant).collect();
    let exprs: BTreeSet<_> = sample_exprs().iter().map(expr_variant).collect();
    let stmts: BTreeSet<_> = sample_stmts().iter().map(stmt_variant).collect();
    assert_eq!((values.len(), sample_values().len()), (25, 25), "Value variants");
    assert_eq!((exprs.len(), sample_exprs().len()), (24, 24), "Expression variants");
    assert_eq!((stmts.len(), sample_stmts().len()), (9, 9), "QuickFuncStatement variants");
}

// -----------------------------------------------------------------------------
// Hand-built round trips
// -----------------------------------------------------------------------------

#[test]
fn every_value_variant_round_trips() {
    for v in sample_values().into_iter().chain(edge_values()) {
        let mut arena = Arena::new();
        let id = arena.import_value(&v);
        assert_eq!(arena.export_value(id), v, "value variant {}", value_variant(&v));
    }
}

#[test]
fn every_expression_variant_round_trips() {
    for e in sample_exprs().into_iter().chain(edge_exprs()) {
        let mut arena = Arena::new();
        let id = arena.import_expr(&e);
        assert_eq!(arena.export_expr(id), e, "expression variant {}", expr_variant(&e));
    }
}

#[test]
fn every_statement_variant_round_trips() {
    for s in sample_stmts().into_iter().chain(edge_stmts()) {
        let mut arena = Arena::new();
        let id = arena.import_stmt(&s);
        assert_eq!(arena.export_stmt(id), s, "statement variant {}", stmt_variant(&s));
    }
}

#[test]
fn none_and_an_empty_list_stay_different() {
    let mut arena = Arena::new();
    let none = arena.import_expr(&edge_exprs()[0]);
    let some_empty = arena.import_expr(&edge_exprs()[1]);
    match (arena.expr(none), arena.expr(some_empty)) {
        (ExprNode::QualifiedIdentifier { arguments: a, .. }, ExprNode::QualifiedIdentifier { arguments: b, .. }) => {
            assert!(a.is_none());
            assert!(b.map_or(false, |list| list.is_empty()));
        }
        other => panic!("unexpected nodes {:?}", other),
    }
}

#[test]
fn everything_imported_into_one_arena_exports_unchanged() {
    // Importing interleaved into one arena is what the compiler will do; spans and ids from
    // different trees must not collide.
    let values: Vec<Value> = sample_values().into_iter().chain(edge_values()).collect();
    let exprs: Vec<Expression> = sample_exprs().into_iter().chain(edge_exprs()).collect();
    let stmts: Vec<QuickFuncStatement> = sample_stmts().into_iter().chain(edge_stmts()).collect();

    let mut arena = Arena::new();
    let mut value_ids = Vec::new();
    let mut expr_ids = Vec::new();
    let mut stmt_ids = Vec::new();
    for round in 0..3 {
        for v in &values {
            value_ids.push((round, arena.import_value(v)));
        }
        for e in &exprs {
            expr_ids.push((round, arena.import_expr(e)));
        }
        for s in &stmts {
            stmt_ids.push((round, arena.import_stmt(s)));
        }
    }
    for (k, (_, id)) in value_ids.iter().enumerate() {
        assert_eq!(arena.export_value(*id), values[k % values.len()]);
    }
    for (k, (_, id)) in expr_ids.iter().enumerate() {
        assert_eq!(arena.export_expr(*id), exprs[k % exprs.len()]);
    }
    for (k, (_, id)) in stmt_ids.iter().enumerate() {
        assert_eq!(arena.export_stmt(*id), stmts[k % stmts.len()]);
    }
}

#[test]
fn a_cloned_arena_exports_the_same_tree_and_is_send_sync() {
    fn assert_owned_and_shareable<T: Send + Sync + 'static + Clone>() {}
    assert_owned_and_shareable::<Arena>();

    let mut arena = Arena::new();
    let ids: Vec<_> = sample_values().iter().map(|v| arena.import_value(v)).collect();
    let copy = arena.clone();
    for (v, id) in sample_values().iter().zip(ids) {
        assert_eq!(copy.export_value(id), *v);
    }
}

#[test]
fn positions_are_readable_without_exporting() {
    let mut arena = Arena::new();
    let v = arena.import_value(&sample_values()[21]);
    let e = arena.import_expr(&sample_exprs()[11]);
    let s = arena.import_stmt(&sample_stmts()[2]);
    assert_eq!(arena.value(v).position(), pos(22));
    assert_eq!(arena.expr(e).position(), pos(12));
    assert_eq!(arena.stmt(s).position(), pos(3));
    assert!(matches!(arena.value(v), ValueNode::Lambda { .. }));
    assert!(matches!(arena.stmt(s), StmtNode::Switch { default_case: Some(_), .. }));
}

// -----------------------------------------------------------------------------
// Layout
// -----------------------------------------------------------------------------

#[test]
fn arena_nodes_are_smaller_than_the_owned_types() {
    use std::mem::size_of;
    let rows = [
        ("Value", size_of::<Value>(), "ValueNode", size_of::<ValueNode>()),
        ("Expression", size_of::<Expression>(), "ExprNode", size_of::<ExprNode>()),
        ("QuickFuncStatement", size_of::<QuickFuncStatement>(), "StmtNode", size_of::<StmtNode>()),
    ];
    for (owned_name, owned, node_name, node) in rows {
        println!("{owned_name}: {owned} bytes, {node_name}: {node} bytes");
        assert!(node < owned, "{node_name} ({node}) must be smaller than {owned_name} ({owned})");
    }
}

// -----------------------------------------------------------------------------
// The corpus
// -----------------------------------------------------------------------------

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().map_or(false, |x| x == "mdix") {
            out.push(path);
        }
    }
}

enum Root<'a> {
    Value(&'a Value),
    Expr(&'a Expression),
    Stmt(&'a QuickFuncStatement),
}

/// Calls `f` with every top-level `Value`, `Expression` and statement held by a section.
/// Nested nodes are reached through their parents.
fn for_each_root<'a>(ds: &'a DixScript, f: &mut dyn FnMut(Root<'a>)) {
    if let Some(data) = &ds.data {
        for entry in &data.entries {
            match entry {
                DataEntry::SimpleProperty { value, .. } => f(Root::Value(value)),
                DataEntry::TableProperty { properties, .. } => {
                    for p in properties {
                        f(Root::Value(&p.value));
                    }
                }
                DataEntry::GroupArray { items, .. } => {
                    for item in items {
                        f(Root::Value(item));
                    }
                }
                DataEntry::ObjectProperty { object, .. } => f(Root::Value(object)),
            }
        }
    }
    for block in &ds.raw {
        for field in block.meta_data.iter().chain(block.using.iter()) {
            f(Root::Value(&field.value));
        }
    }
    if let Some(security) = &ds.security {
        for entry in &security.entries {
            for field in &entry.fields {
                f(Root::Value(&field.value));
            }
        }
    }
    if let Some(schema) = &ds.schema {
        for field in &schema.fields {
            for arg in &field.arguments {
                f(Root::Value(arg));
            }
        }
    }
    if let Some(quick) = &ds.quick_functions {
        for func in &quick.functions {
            for param in &func.parameters {
                if let Some(default) = &param.default_value {
                    f(Root::Expr(default));
                }
            }
            for stmt in &func.body {
                f(Root::Stmt(stmt));
            }
        }
    }
}

enum Imported {
    Value(ValueId),
    Expr(ExprId),
    Stmt(StmtId),
}

#[test]
fn every_compiled_file_in_the_corpus_round_trips() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("mdix_files");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    assert!(files.len() >= 50, "expected the corpus under {}, found {} files", root.display(), files.len());

    let (mut compiled, mut skipped) = (0usize, 0usize);
    let (mut values, mut exprs, mut stmts) = (0usize, 0usize, 0usize);
    let mut arena_nodes = 0usize;

    for path in &files {
        let loader = DixLoader::new_silent();
        let Ok(ds) = loader.compile_to_resolved_ast(path.to_str().unwrap()) else {
            skipped += 1;
            continue;
        };
        compiled += 1;

        let mut roots: Vec<Root> = Vec::new();
        for_each_root(&ds, &mut |r| roots.push(r));

        // One arena per file, every root imported before any is exported, so interleaved
        // ids and spans are exercised on real data.
        let mut arena = Arena::new();
        let imported: Vec<Imported> = roots
            .iter()
            .map(|r| match r {
                Root::Value(v) => {
                    values += 1;
                    Imported::Value(arena.import_value(v))
                }
                Root::Expr(e) => {
                    exprs += 1;
                    Imported::Expr(arena.import_expr(e))
                }
                Root::Stmt(s) => {
                    stmts += 1;
                    Imported::Stmt(arena.import_stmt(s))
                }
            })
            .collect();
        arena_nodes += arena.value_count() + arena.expr_count() + arena.stmt_count();

        for (root, id) in roots.iter().zip(&imported) {
            match (root, id) {
                (Root::Value(v), Imported::Value(id)) => {
                    assert_eq!(arena.export_value(*id), **v, "{}", path.display())
                }
                (Root::Expr(e), Imported::Expr(id)) => {
                    assert_eq!(arena.export_expr(*id), **e, "{}", path.display())
                }
                (Root::Stmt(s), Imported::Stmt(id)) => {
                    assert_eq!(arena.export_stmt(*id), **s, "{}", path.display())
                }
                _ => unreachable!("import kind always matches root kind"),
            }
        }
    }

    println!(
        "corpus: {} files, {} compiled, {} did not compile; round-tripped {} values, {} expressions, {} statements ({} arena nodes)",
        files.len(), compiled, skipped, values, exprs, stmts, arena_nodes
    );
    assert!(compiled >= 40, "only {} corpus files compiled", compiled);
    assert!(values + exprs + stmts >= 1000, "the corpus produced too few roots to mean anything");
}
