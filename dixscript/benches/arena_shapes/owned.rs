// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "benches/arena_shapes/owned.rs"
// ============================================================================
//! The owned representation: the shape of the current DixScript AST.
//!
//! `String` for text, `Vec` for lists, `Box` for recursion. The three recursion
//! shapes of `Compiler::AST::expressions.rs` and `statements.rs` are all present:
//! Vec-only (`FunctionCall` arguments, `If` branches), single-Box (`UnaryOp`,
//! `PropertyAccess`) and double-Box (`ArithmeticOp`, `ComparisonOp`).

use super::{Fingerprint, Position, Profile};

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Identifier { name: String, position: Position },
    Value { value: i64, position: Position },
    FunctionCall { name: String, arguments: Vec<Expression>, position: Position },
    UnaryOp { operator: String, operand: Box<Expression>, position: Position },
    ArithmeticOp { left: Box<Expression>, operator: String, right: Box<Expression>, position: Position },
    ComparisonOp { left: Box<Expression>, operator: String, right: Box<Expression>, position: Position },
    // Mirrors the real AST's single-Box shape. The generator never builds one, but
    // the walkers must still handle it so all three stay shaped like the real thing.
    #[allow(dead_code)]
    PropertyAccess { object: Box<Expression>, property: String, position: Position },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Return { value: Expression, position: Position },
    Assignment { variable: String, value: Expression, position: Position },
    VariableDeclaration { variable_name: String, value: Expression, position: Position },
    ExpressionStatement { expression: Expression, position: Position },
    If {
        condition: Expression,
        then_branch: Vec<Statement>,
        else_branch: Option<Vec<Statement>>,
        position: Position,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub functions: Vec<Function>,
}

// -----------------------------------------------------------------------------
// Read-only walks
// -----------------------------------------------------------------------------

pub fn count_expr(e: &Expression) -> u64 {
    match e {
        Expression::Identifier { .. } | Expression::Value { .. } => 1,
        Expression::FunctionCall { arguments, .. } => {
            1 + arguments.iter().map(count_expr).sum::<u64>()
        }
        Expression::UnaryOp { operand, .. } => 1 + count_expr(operand),
        Expression::ArithmeticOp { left, right, .. }
        | Expression::ComparisonOp { left, right, .. } => 1 + count_expr(left) + count_expr(right),
        Expression::PropertyAccess { object, .. } => 1 + count_expr(object),
    }
}

pub fn count_stmt(s: &Statement) -> u64 {
    match s {
        Statement::Return { value, .. }
        | Statement::Assignment { value, .. }
        | Statement::VariableDeclaration { value, .. }
        | Statement::ExpressionStatement { expression: value, .. } => 1 + count_expr(value),
        Statement::If { condition, then_branch, else_branch, .. } => {
            let mut n = 1 + count_expr(condition);
            n += then_branch.iter().map(count_stmt).sum::<u64>();
            if let Some(eb) = else_branch {
                n += eb.iter().map(count_stmt).sum::<u64>();
            }
            n
        }
    }
}

pub fn count_program(p: &Program) -> u64 {
    p.functions
        .iter()
        .map(|f| 1 + f.body.iter().map(count_stmt).sum::<u64>())
        .sum()
}

fn fingerprint_expr(f: &mut Fingerprint, e: &Expression) {
    f.nodes += 1;
    match e {
        Expression::Identifier { name, position } => {
            f.name_bytes += name.len() as u64;
            f.add_position(*position);
        }
        Expression::Value { value, position } => {
            f.value_nodes += 1;
            f.value_sum += *value;
            f.add_position(*position);
        }
        Expression::FunctionCall { name, arguments, position } => {
            f.name_bytes += name.len() as u64;
            f.add_position(*position);
            for a in arguments {
                fingerprint_expr(f, a);
            }
        }
        Expression::UnaryOp { operator, operand, position } => {
            f.add_operator(operator);
            f.add_position(*position);
            fingerprint_expr(f, operand);
        }
        Expression::ArithmeticOp { left, operator, right, position }
        | Expression::ComparisonOp { left, operator, right, position } => {
            f.add_operator(operator);
            f.add_position(*position);
            fingerprint_expr(f, left);
            fingerprint_expr(f, right);
        }
        Expression::PropertyAccess { object, property, position } => {
            f.name_bytes += property.len() as u64;
            f.add_position(*position);
            fingerprint_expr(f, object);
        }
    }
}

fn fingerprint_stmt(f: &mut Fingerprint, s: &Statement) {
    f.nodes += 1;
    match s {
        Statement::Return { value, position } | Statement::ExpressionStatement { expression: value, position } => {
            f.add_position(*position);
            fingerprint_expr(f, value);
        }
        Statement::Assignment { variable: name, value, position }
        | Statement::VariableDeclaration { variable_name: name, value, position } => {
            f.name_bytes += name.len() as u64;
            f.add_position(*position);
            fingerprint_expr(f, value);
        }
        Statement::If { condition, then_branch, else_branch, position } => {
            f.add_position(*position);
            fingerprint_expr(f, condition);
            for s in then_branch {
                fingerprint_stmt(f, s);
            }
            if let Some(eb) = else_branch {
                for s in eb {
                    fingerprint_stmt(f, s);
                }
            }
        }
    }
}

pub fn fingerprint_program(p: &Program) -> Fingerprint {
    let mut f = Fingerprint::default();
    for func in &p.functions {
        f.nodes += 1;
        f.name_bytes += func.name.len() as u64;
        for s in &func.body {
            fingerprint_stmt(&mut f, s);
        }
    }
    f
}

// -----------------------------------------------------------------------------
// In-place mutation: the pass the clone-and-mutate group applies to the copy.
// -----------------------------------------------------------------------------

fn bump_expr(e: &mut Expression, delta: i64) {
    match e {
        Expression::Value { value, .. } => *value += delta,
        Expression::Identifier { .. } => {}
        Expression::FunctionCall { arguments, .. } => {
            for a in arguments.iter_mut() {
                bump_expr(a, delta);
            }
        }
        Expression::UnaryOp { operand, .. } => bump_expr(operand, delta),
        Expression::ArithmeticOp { left, right, .. }
        | Expression::ComparisonOp { left, right, .. } => {
            bump_expr(left, delta);
            bump_expr(right, delta);
        }
        Expression::PropertyAccess { object, .. } => bump_expr(object, delta),
    }
}

fn bump_stmt(s: &mut Statement, delta: i64) {
    match s {
        Statement::Return { value, .. }
        | Statement::Assignment { value, .. }
        | Statement::VariableDeclaration { value, .. }
        | Statement::ExpressionStatement { expression: value, .. } => bump_expr(value, delta),
        Statement::If { condition, then_branch, else_branch, .. } => {
            bump_expr(condition, delta);
            for s in then_branch.iter_mut() {
                bump_stmt(s, delta);
            }
            if let Some(eb) = else_branch {
                for s in eb.iter_mut() {
                    bump_stmt(s, delta);
                }
            }
        }
    }
}

/// Adds `delta` to every `Value` node by walking the tree.
pub fn bump_values(p: &mut Program, delta: i64) {
    for func in p.functions.iter_mut() {
        for s in func.body.iter_mut() {
            bump_stmt(s, delta);
        }
    }
}

// -----------------------------------------------------------------------------
// Synthetic program generator: deterministic, no RNG. The three representations
// build identical trees per profile.
// -----------------------------------------------------------------------------

fn build_expr(depth: usize, seed: usize) -> Expression {
    use Expression as E;
    let pos = Position { line: seed, column: seed };
    if depth == 0 {
        return if seed % 2 == 0 {
            E::Identifier { name: format!("var_{seed}"), position: pos }
        } else {
            E::Value { value: seed as i64, position: pos }
        };
    }
    match seed % 4 {
        0 => E::ArithmeticOp {
            left: Box::new(build_expr(depth - 1, seed + 1)),
            operator: "+".to_string(),
            right: Box::new(build_expr(depth - 1, seed + 2)),
            position: pos,
        },
        1 => E::ComparisonOp {
            left: Box::new(build_expr(depth - 1, seed + 1)),
            operator: ">".to_string(),
            right: Box::new(build_expr(depth - 1, seed + 2)),
            position: pos,
        },
        2 => E::UnaryOp {
            operator: "-".to_string(),
            operand: Box::new(build_expr(depth - 1, seed + 1)),
            position: pos,
        },
        _ => E::FunctionCall {
            name: format!("fn_{seed}"),
            arguments: vec![build_expr(depth - 1, seed + 1), build_expr(depth - 1, seed + 2)],
            position: pos,
        },
    }
}

pub fn build_program(p: Profile) -> Program {
    use Statement as S;
    let mut functions = Vec::with_capacity(p.num_functions);
    let mut seed = 0usize;
    for fi in 0..p.num_functions {
        let mut body = Vec::with_capacity(p.stmts_per_function);
        for si in 0..p.stmts_per_function {
            seed += 1;
            let pos = Position { line: seed, column: 0 };
            let stmt = if p.if_every_n_stmts != 0 && si % p.if_every_n_stmts == p.if_every_n_stmts - 1 {
                S::If {
                    condition: build_expr(p.expr_depth, seed),
                    then_branch: vec![S::ExpressionStatement {
                        expression: build_expr(p.expr_depth, seed + 100),
                        position: pos,
                    }],
                    else_branch: Some(vec![S::ExpressionStatement {
                        expression: build_expr(p.expr_depth, seed + 200),
                        position: pos,
                    }]),
                    position: pos,
                }
            } else {
                match si % 3 {
                    0 => S::Return { value: build_expr(p.expr_depth, seed), position: pos },
                    1 => S::VariableDeclaration {
                        variable_name: format!("v{si}"),
                        value: build_expr(p.expr_depth, seed),
                        position: pos,
                    },
                    _ => S::Assignment {
                        variable: format!("v{si}"),
                        value: build_expr(p.expr_depth, seed),
                        position: pos,
                    },
                }
            };
            body.push(stmt);
        }
        functions.push(Function { name: format!("func_{fi}"), body });
    }
    Program { functions }
}
