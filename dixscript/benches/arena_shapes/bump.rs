// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Measurement 5: option A against
// option B"
// ============================================================================
//! Option A: a bumpalo arena with `&'a` references.
//!
//! `&'a str` instead of `String`, `&'a [T]` instead of `Vec<T>`, `&'a T` instead
//! of `Box<T>`. Every field is `Copy`, so `Expression<'a>` and `Statement<'a>`
//! derive `Copy` as well. This is the shape Ubel Stratum uses.
//!
//! Operators are `&'static str` literals in the builder, so construction does not
//! allocate them. The rebuild copies them into the destination arena, which is the
//! conservative reading of "an independently owned copy".

use super::{Fingerprint, Position, Profile};
use bumpalo::collections::Vec as BumpVec;
use bumpalo::Bump;

pub struct AstArena {
    bump: Bump,
}

impl AstArena {
    pub fn with_capacity(cap: usize) -> Self {
        AstArena { bump: Bump::with_capacity(cap) }
    }
    #[inline]
    pub fn alloc<T>(&self, val: T) -> &T {
        self.bump.alloc(val)
    }
    #[inline]
    pub fn alloc_str(&self, s: &str) -> &str {
        self.bump.alloc_str(s)
    }
    #[inline]
    pub fn vec<T>(&self) -> BumpVec<'_, T> {
        BumpVec::new_in(&self.bump)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Expression<'a> {
    Identifier { name: &'a str, position: Position },
    Value { value: i64, position: Position },
    FunctionCall { name: &'a str, arguments: &'a [Expression<'a>], position: Position },
    UnaryOp { operator: &'a str, operand: &'a Expression<'a>, position: Position },
    ArithmeticOp { left: &'a Expression<'a>, operator: &'a str, right: &'a Expression<'a>, position: Position },
    ComparisonOp { left: &'a Expression<'a>, operator: &'a str, right: &'a Expression<'a>, position: Position },
    // Mirrors the real AST's single-Box shape. The generator never builds one, but
    // the walkers must still handle it so all three stay shaped like the real thing.
    #[allow(dead_code)]
    PropertyAccess { object: &'a Expression<'a>, property: &'a str, position: Position },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Statement<'a> {
    Return { value: Expression<'a>, position: Position },
    Assignment { variable: &'a str, value: Expression<'a>, position: Position },
    VariableDeclaration { variable_name: &'a str, value: Expression<'a>, position: Position },
    ExpressionStatement { expression: Expression<'a>, position: Position },
    If {
        condition: Expression<'a>,
        then_branch: &'a [Statement<'a>],
        else_branch: Option<&'a [Statement<'a>]>,
        position: Position,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct Function<'a> {
    pub name: &'a str,
    pub body: &'a [Statement<'a>],
}

#[derive(Debug, Clone, Copy)]
pub struct Program<'a> {
    pub functions: &'a [Function<'a>],
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
            for a in arguments.iter() {
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
            for s in then_branch.iter() {
                fingerprint_stmt(f, s);
            }
            if let Some(eb) = else_branch {
                for s in eb.iter() {
                    fingerprint_stmt(f, s);
                }
            }
        }
    }
}

pub fn fingerprint_program(p: &Program) -> Fingerprint {
    let mut f = Fingerprint::default();
    for func in p.functions.iter() {
        f.nodes += 1;
        f.name_bytes += func.name.len() as u64;
        for s in func.body.iter() {
            fingerprint_stmt(&mut f, s);
        }
    }
    f
}

// -----------------------------------------------------------------------------
// Rebuild: the arena side of `ast.clone()` followed by an edit.
//
// References are shared and immutable, so an independently owned and editable
// copy means a full walk that re-allocates every node into a fresh arena. There is
// no structural sharing, so the same node count is visited as an owned `.clone()`.
// A real implementation could rebuild only the modified spine and share the rest,
// which would beat this; the benchmark deliberately does not take that shortcut.
// `delta` is added to every `Value` on the way through (0 is a plain copy).
// -----------------------------------------------------------------------------

pub fn rebuild_expr<'a>(dst: &'a AstArena, e: &Expression, delta: i64) -> &'a Expression<'a> {
    let rebuilt = match e {
        Expression::Identifier { name, position } => Expression::Identifier {
            name: dst.alloc_str(name),
            position: *position,
        },
        Expression::Value { value, position } => Expression::Value {
            value: *value + delta,
            position: *position,
        },
        Expression::FunctionCall { name, arguments, position } => {
            let mut v = dst.vec::<Expression>();
            for a in arguments.iter() {
                v.push(*rebuild_expr(dst, a, delta));
            }
            Expression::FunctionCall {
                name: dst.alloc_str(name),
                arguments: v.into_bump_slice(),
                position: *position,
            }
        }
        Expression::UnaryOp { operator, operand, position } => Expression::UnaryOp {
            operator: dst.alloc_str(operator),
            operand: rebuild_expr(dst, operand, delta),
            position: *position,
        },
        Expression::ArithmeticOp { left, operator, right, position } => Expression::ArithmeticOp {
            left: rebuild_expr(dst, left, delta),
            operator: dst.alloc_str(operator),
            right: rebuild_expr(dst, right, delta),
            position: *position,
        },
        Expression::ComparisonOp { left, operator, right, position } => Expression::ComparisonOp {
            left: rebuild_expr(dst, left, delta),
            operator: dst.alloc_str(operator),
            right: rebuild_expr(dst, right, delta),
            position: *position,
        },
        Expression::PropertyAccess { object, property, position } => Expression::PropertyAccess {
            object: rebuild_expr(dst, object, delta),
            property: dst.alloc_str(property),
            position: *position,
        },
    };
    dst.alloc(rebuilt)
}

pub fn rebuild_stmt<'a>(dst: &'a AstArena, s: &Statement, delta: i64) -> Statement<'a> {
    match s {
        Statement::Return { value, position } => Statement::Return {
            value: *rebuild_expr(dst, value, delta),
            position: *position,
        },
        Statement::Assignment { variable, value, position } => Statement::Assignment {
            variable: dst.alloc_str(variable),
            value: *rebuild_expr(dst, value, delta),
            position: *position,
        },
        Statement::VariableDeclaration { variable_name, value, position } => Statement::VariableDeclaration {
            variable_name: dst.alloc_str(variable_name),
            value: *rebuild_expr(dst, value, delta),
            position: *position,
        },
        Statement::ExpressionStatement { expression, position } => Statement::ExpressionStatement {
            expression: *rebuild_expr(dst, expression, delta),
            position: *position,
        },
        Statement::If { condition, then_branch, else_branch, position } => {
            let mut tb = dst.vec::<Statement>();
            for s in then_branch.iter() {
                tb.push(rebuild_stmt(dst, s, delta));
            }
            let eb = else_branch.map(|branch| {
                let mut v = dst.vec::<Statement>();
                for s in branch.iter() {
                    v.push(rebuild_stmt(dst, s, delta));
                }
                v.into_bump_slice() as &[Statement]
            });
            Statement::If {
                condition: *rebuild_expr(dst, condition, delta),
                then_branch: tb.into_bump_slice(),
                else_branch: eb,
                position: *position,
            }
        }
    }
}

pub fn rebuild_program<'a>(dst: &'a AstArena, p: &Program, delta: i64) -> Program<'a> {
    let mut fns = dst.vec::<Function>();
    for f in p.functions.iter() {
        let mut body = dst.vec::<Statement>();
        for s in f.body.iter() {
            body.push(rebuild_stmt(dst, s, delta));
        }
        fns.push(Function { name: dst.alloc_str(f.name), body: body.into_bump_slice() });
    }
    Program { functions: fns.into_bump_slice() }
}

// -----------------------------------------------------------------------------
// Synthetic program generator (same trees as `owned` and `index`).
// -----------------------------------------------------------------------------

fn build_expr<'a>(a: &'a AstArena, depth: usize, seed: usize) -> Expression<'a> {
    use Expression as E;
    let pos = Position { line: seed, column: seed };
    if depth == 0 {
        return if seed % 2 == 0 {
            E::Identifier { name: a.alloc_str(&format!("var_{seed}")), position: pos }
        } else {
            E::Value { value: seed as i64, position: pos }
        };
    }
    match seed % 4 {
        0 => E::ArithmeticOp {
            left: a.alloc(build_expr(a, depth - 1, seed + 1)),
            operator: "+",
            right: a.alloc(build_expr(a, depth - 1, seed + 2)),
            position: pos,
        },
        1 => E::ComparisonOp {
            left: a.alloc(build_expr(a, depth - 1, seed + 1)),
            operator: ">",
            right: a.alloc(build_expr(a, depth - 1, seed + 2)),
            position: pos,
        },
        2 => E::UnaryOp {
            operator: "-",
            operand: a.alloc(build_expr(a, depth - 1, seed + 1)),
            position: pos,
        },
        _ => {
            let mut v = a.vec::<E>();
            v.push(build_expr(a, depth - 1, seed + 1));
            v.push(build_expr(a, depth - 1, seed + 2));
            E::FunctionCall {
                name: a.alloc_str(&format!("fn_{seed}")),
                arguments: v.into_bump_slice(),
                position: pos,
            }
        }
    }
}

pub fn build_program<'a>(a: &'a AstArena, p: Profile) -> Program<'a> {
    use Statement as S;
    let mut functions = a.vec::<Function>();
    let mut seed = 0usize;
    for fi in 0..p.num_functions {
        let mut body = a.vec::<S>();
        for si in 0..p.stmts_per_function {
            seed += 1;
            let pos = Position { line: seed, column: 0 };
            let stmt = if p.if_every_n_stmts != 0 && si % p.if_every_n_stmts == p.if_every_n_stmts - 1 {
                let mut tb = a.vec::<S>();
                tb.push(S::ExpressionStatement { expression: build_expr(a, p.expr_depth, seed + 100), position: pos });
                let mut eb = a.vec::<S>();
                eb.push(S::ExpressionStatement { expression: build_expr(a, p.expr_depth, seed + 200), position: pos });
                S::If {
                    condition: build_expr(a, p.expr_depth, seed),
                    then_branch: tb.into_bump_slice(),
                    else_branch: Some(eb.into_bump_slice()),
                    position: pos,
                }
            } else {
                match si % 3 {
                    0 => S::Return { value: build_expr(a, p.expr_depth, seed), position: pos },
                    1 => S::VariableDeclaration {
                        variable_name: a.alloc_str(&format!("v{si}")),
                        value: build_expr(a, p.expr_depth, seed),
                        position: pos,
                    },
                    _ => S::Assignment {
                        variable: a.alloc_str(&format!("v{si}")),
                        value: build_expr(a, p.expr_depth, seed),
                        position: pos,
                    },
                }
            };
            body.push(stmt);
        }
        functions.push(Function { name: a.alloc_str(&format!("func_{fi}")), body: body.into_bump_slice() });
    }
    Program { functions: functions.into_bump_slice() }
}
