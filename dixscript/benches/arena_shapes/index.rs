// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "benches/arena_shapes/index.rs"
// ============================================================================
//! Option B: an owned index arena.
//!
//! One `Arena` owns a `Vec` per node kind. Children are `u32` ids, lists are
//! `(start, len)` ranges into side `Vec`s of ids, and text lives in one append-only
//! `String` addressed by `(start, len)` spans. There is no lifetime anywhere, so
//! `Arena` is an ordinary `'static`, `Send + Sync`, `Clone` value, which is the
//! point of the shape.
//!
//! Every node is `Copy`, like option A's. Access goes through the arena:
//! `arena.expr(id)` instead of following a reference.
//!
//! The AST is append-only, so none of the machinery a removal-capable arena needs
//! (free lists, generations) is present.
//!
//! Operators are `&'static str` literals, the same cost model as option A's
//! builder. A real migration would use an operator enum instead (a smaller node,
//! and nothing to copy or compare as text).
//!
//! Lists are built through two scratch stacks: children are pushed as they finish,
//! and `finish_*_list` moves the pushed ids into the side `Vec` in one contiguous
//! run. Nested lists work because each one finishes before its parent pushes.

use super::{Fingerprint, Position, Profile};
use std::mem::size_of;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExprId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StmtId(u32);

/// A span of the arena's text buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Str {
    start: u32,
    len: u32,
}

/// A run of `ExprId`s in the arena's expression-list buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExprList {
    start: u32,
    len: u32,
}

/// A run of `StmtId`s in the arena's statement-list buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StmtList {
    start: u32,
    len: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Expression {
    Identifier { name: Str, position: Position },
    Value { value: i64, position: Position },
    FunctionCall { name: Str, arguments: ExprList, position: Position },
    UnaryOp { operator: &'static str, operand: ExprId, position: Position },
    ArithmeticOp { left: ExprId, operator: &'static str, right: ExprId, position: Position },
    ComparisonOp { left: ExprId, operator: &'static str, right: ExprId, position: Position },
    // Mirrors the real AST's single-Box shape. The generator never builds one, but
    // the walkers must still handle it so all three stay shaped like the real thing.
    #[allow(dead_code)]
    PropertyAccess { object: ExprId, property: Str, position: Position },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Statement {
    Return { value: ExprId, position: Position },
    Assignment { variable: Str, value: ExprId, position: Position },
    VariableDeclaration { variable_name: Str, value: ExprId, position: Position },
    ExpressionStatement { expression: ExprId, position: Position },
    If {
        condition: ExprId,
        then_branch: StmtList,
        else_branch: Option<StmtList>,
        position: Position,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Function {
    pub name: Str,
    pub body: StmtList,
}

/// Bytes of a bump arena's budget that hold one expression (plus its share of lists
/// and text) at option A's real node cost. Dividing `Profile::arena_capacity` by it
/// gives the number of expressions the same budget would hold, which is what the
/// index arena reserves for.
const HINT_BYTES_PER_EXPR: usize = 64;

/// The whole program: a plain owned value. `Default` starts empty (no reservation);
/// `with_hint` reserves up front the way a parser that knows the source length would.
#[derive(Debug, Clone, Default)]
pub struct Arena {
    exprs: Vec<Expression>,
    stmts: Vec<Statement>,
    expr_lists: Vec<ExprId>,
    stmt_lists: Vec<StmtId>,
    functions: Vec<Function>,
    text: String,
    expr_scratch: Vec<ExprId>,
    stmt_scratch: Vec<StmtId>,
}

impl Arena {
    /// Reserves capacity from the same byte budget the bump arena gets, so neither
    /// arena starts with a clear advantage in reserved memory.
    pub fn with_hint(p: Profile) -> Self {
        let exprs = p.arena_capacity / HINT_BYTES_PER_EXPR;
        Arena {
            exprs: Vec::with_capacity(exprs),
            stmts: Vec::with_capacity(exprs / 4),
            expr_lists: Vec::with_capacity(exprs / 2),
            stmt_lists: Vec::with_capacity(exprs / 4),
            functions: Vec::with_capacity(exprs / 16),
            text: String::with_capacity(exprs * 8),
            expr_scratch: Vec::with_capacity(64),
            stmt_scratch: Vec::with_capacity(64),
        }
    }

    // -- allocation ----------------------------------------------------------

    #[inline]
    pub fn alloc_expr(&mut self, e: Expression) -> ExprId {
        let id = ExprId(self.exprs.len() as u32);
        self.exprs.push(e);
        id
    }

    #[inline]
    pub fn alloc_stmt(&mut self, s: Statement) -> StmtId {
        let id = StmtId(self.stmts.len() as u32);
        self.stmts.push(s);
        id
    }

    #[inline]
    pub fn alloc_str(&mut self, s: &str) -> Str {
        let start = self.text.len();
        self.text.push_str(s);
        Str { start: start as u32, len: s.len() as u32 }
    }

    /// Moves the ids pushed on the expression scratch stack since `mark` into the
    /// list buffer and returns their range.
    #[inline]
    fn finish_expr_list(&mut self, mark: usize) -> ExprList {
        let start = self.expr_lists.len();
        self.expr_lists.extend(self.expr_scratch.drain(mark..));
        ExprList { start: start as u32, len: (self.expr_lists.len() - start) as u32 }
    }

    #[inline]
    fn finish_stmt_list(&mut self, mark: usize) -> StmtList {
        let start = self.stmt_lists.len();
        self.stmt_lists.extend(self.stmt_scratch.drain(mark..));
        StmtList { start: start as u32, len: (self.stmt_lists.len() - start) as u32 }
    }

    // -- access --------------------------------------------------------------

    #[inline]
    pub fn expr(&self, id: ExprId) -> &Expression {
        &self.exprs[id.0 as usize]
    }

    #[inline]
    pub fn expr_list(&self, list: ExprList) -> &[ExprId] {
        let start = list.start as usize;
        &self.expr_lists[start..start + list.len as usize]
    }

    #[inline]
    pub fn stmt(&self, id: StmtId) -> &Statement {
        &self.stmts[id.0 as usize]
    }

    #[inline]
    pub fn stmt_list(&self, list: StmtList) -> &[StmtId] {
        let start = list.start as usize;
        &self.stmt_lists[start..start + list.len as usize]
    }

    #[inline]
    pub fn text(&self, s: Str) -> &str {
        let start = s.start as usize;
        &self.text[start..start + s.len as usize]
    }

    // -- sizes ---------------------------------------------------------------

    /// Capacities of the six buffers, in a fixed order. The benchmark's
    /// equivalence check compares them before and after a build to prove that the
    /// `with_hint` variant never reallocates.
    pub fn capacities(&self) -> [usize; 6] {
        [
            self.exprs.capacity(),
            self.stmts.capacity(),
            self.expr_lists.capacity(),
            self.stmt_lists.capacity(),
            self.functions.capacity(),
            self.text.capacity(),
        ]
    }

    pub fn function_count(&self) -> usize {
        self.functions.len()
    }

    pub fn expression_count(&self) -> usize {
        self.exprs.len()
    }

    pub fn statement_count(&self) -> usize {
        self.stmts.len()
    }

    /// Bytes held by the node, list and text buffers (length, not capacity).
    pub fn used_bytes(&self) -> usize {
        self.exprs.len() * size_of::<Expression>()
            + self.stmts.len() * size_of::<Statement>()
            + self.expr_lists.len() * size_of::<ExprId>()
            + self.stmt_lists.len() * size_of::<StmtId>()
            + self.functions.len() * size_of::<Function>()
            + self.text.len()
    }
}

// -----------------------------------------------------------------------------
// Read-only walks
// -----------------------------------------------------------------------------

pub fn count_expr(a: &Arena, id: ExprId) -> u64 {
    match a.expr(id) {
        Expression::Identifier { .. } | Expression::Value { .. } => 1,
        Expression::FunctionCall { arguments, .. } => {
            1 + a.expr_list(*arguments).iter().map(|c| count_expr(a, *c)).sum::<u64>()
        }
        Expression::UnaryOp { operand, .. } => 1 + count_expr(a, *operand),
        Expression::ArithmeticOp { left, right, .. }
        | Expression::ComparisonOp { left, right, .. } => {
            1 + count_expr(a, *left) + count_expr(a, *right)
        }
        Expression::PropertyAccess { object, .. } => 1 + count_expr(a, *object),
    }
}

pub fn count_stmt(a: &Arena, id: StmtId) -> u64 {
    match a.stmt(id) {
        Statement::Return { value, .. }
        | Statement::Assignment { value, .. }
        | Statement::VariableDeclaration { value, .. }
        | Statement::ExpressionStatement { expression: value, .. } => 1 + count_expr(a, *value),
        Statement::If { condition, then_branch, else_branch, .. } => {
            let mut n = 1 + count_expr(a, *condition);
            n += a.stmt_list(*then_branch).iter().map(|s| count_stmt(a, *s)).sum::<u64>();
            if let Some(eb) = else_branch {
                n += a.stmt_list(*eb).iter().map(|s| count_stmt(a, *s)).sum::<u64>();
            }
            n
        }
    }
}

pub fn count_program(a: &Arena) -> u64 {
    a.functions
        .iter()
        .map(|f| 1 + a.stmt_list(f.body).iter().map(|s| count_stmt(a, *s)).sum::<u64>())
        .sum()
}

fn fingerprint_expr(a: &Arena, f: &mut Fingerprint, id: ExprId) {
    f.nodes += 1;
    match a.expr(id) {
        Expression::Identifier { name, position } => {
            f.name_bytes += a.text(*name).len() as u64;
            f.add_position(*position);
        }
        Expression::Value { value, position } => {
            f.value_nodes += 1;
            f.value_sum += *value;
            f.add_position(*position);
        }
        Expression::FunctionCall { name, arguments, position } => {
            f.name_bytes += a.text(*name).len() as u64;
            f.add_position(*position);
            for c in a.expr_list(*arguments) {
                fingerprint_expr(a, f, *c);
            }
        }
        Expression::UnaryOp { operator, operand, position } => {
            f.add_operator(operator);
            f.add_position(*position);
            fingerprint_expr(a, f, *operand);
        }
        Expression::ArithmeticOp { left, operator, right, position }
        | Expression::ComparisonOp { left, operator, right, position } => {
            f.add_operator(operator);
            f.add_position(*position);
            fingerprint_expr(a, f, *left);
            fingerprint_expr(a, f, *right);
        }
        Expression::PropertyAccess { object, property, position } => {
            f.name_bytes += a.text(*property).len() as u64;
            f.add_position(*position);
            fingerprint_expr(a, f, *object);
        }
    }
}

fn fingerprint_stmt(a: &Arena, f: &mut Fingerprint, id: StmtId) {
    f.nodes += 1;
    match a.stmt(id) {
        Statement::Return { value, position } | Statement::ExpressionStatement { expression: value, position } => {
            f.add_position(*position);
            fingerprint_expr(a, f, *value);
        }
        Statement::Assignment { variable: name, value, position }
        | Statement::VariableDeclaration { variable_name: name, value, position } => {
            f.name_bytes += a.text(*name).len() as u64;
            f.add_position(*position);
            fingerprint_expr(a, f, *value);
        }
        Statement::If { condition, then_branch, else_branch, position } => {
            f.add_position(*position);
            fingerprint_expr(a, f, *condition);
            for s in a.stmt_list(*then_branch) {
                fingerprint_stmt(a, f, *s);
            }
            if let Some(eb) = else_branch {
                for s in a.stmt_list(*eb) {
                    fingerprint_stmt(a, f, *s);
                }
            }
        }
    }
}

pub fn fingerprint_program(a: &Arena) -> Fingerprint {
    let mut f = Fingerprint::default();
    for func in &a.functions {
        f.nodes += 1;
        f.name_bytes += a.text(func.name).len() as u64;
        for s in a.stmt_list(func.body) {
            fingerprint_stmt(a, &mut f, *s);
        }
    }
    f
}

// -----------------------------------------------------------------------------
// In-place mutation, two ways.
//
// `bump_values_linear` is the pass option B makes cheap: every node of a kind sits
// in one `Vec`, so a pass that does not care about tree structure is one loop with
// no recursion. `bump_values_walk` is the like-for-like version of the owned
// walk, for passes that do depend on structure. Each node is copied out before its
// children are visited, which keeps the arena free to be borrowed mutably.
// -----------------------------------------------------------------------------

pub fn bump_values_linear(a: &mut Arena, delta: i64) {
    for e in a.exprs.iter_mut() {
        if let Expression::Value { value, .. } = e {
            *value += delta;
        }
    }
}

fn bump_expr_walk(a: &mut Arena, id: ExprId, delta: i64) {
    let node = a.exprs[id.0 as usize];
    match node {
        Expression::Value { .. } => {
            if let Expression::Value { value, .. } = &mut a.exprs[id.0 as usize] {
                *value += delta;
            }
        }
        Expression::Identifier { .. } => {}
        Expression::FunctionCall { arguments, .. } => {
            for k in 0..arguments.len {
                let child = a.expr_lists[(arguments.start + k) as usize];
                bump_expr_walk(a, child, delta);
            }
        }
        Expression::UnaryOp { operand, .. } => bump_expr_walk(a, operand, delta),
        Expression::ArithmeticOp { left, right, .. }
        | Expression::ComparisonOp { left, right, .. } => {
            bump_expr_walk(a, left, delta);
            bump_expr_walk(a, right, delta);
        }
        Expression::PropertyAccess { object, .. } => bump_expr_walk(a, object, delta),
    }
}

fn bump_stmt_walk(a: &mut Arena, id: StmtId, delta: i64) {
    let node = a.stmts[id.0 as usize];
    match node {
        Statement::Return { value, .. }
        | Statement::Assignment { value, .. }
        | Statement::VariableDeclaration { value, .. }
        | Statement::ExpressionStatement { expression: value, .. } => bump_expr_walk(a, value, delta),
        Statement::If { condition, then_branch, else_branch, .. } => {
            bump_expr_walk(a, condition, delta);
            for k in 0..then_branch.len {
                let child = a.stmt_lists[(then_branch.start + k) as usize];
                bump_stmt_walk(a, child, delta);
            }
            if let Some(eb) = else_branch {
                for k in 0..eb.len {
                    let child = a.stmt_lists[(eb.start + k) as usize];
                    bump_stmt_walk(a, child, delta);
                }
            }
        }
    }
}

pub fn bump_values_walk(a: &mut Arena, delta: i64) {
    for i in 0..a.functions.len() {
        let body = a.functions[i].body;
        for k in 0..body.len {
            let id = a.stmt_lists[(body.start + k) as usize];
            bump_stmt_walk(a, id, delta);
        }
    }
}

// -----------------------------------------------------------------------------
// Synthetic program generator (same trees as `owned` and `bump`).
// -----------------------------------------------------------------------------

fn build_expr(a: &mut Arena, depth: usize, seed: usize) -> ExprId {
    use Expression as E;
    let pos = Position { line: seed, column: seed };
    if depth == 0 {
        return if seed % 2 == 0 {
            let name = a.alloc_str(&format!("var_{seed}"));
            a.alloc_expr(E::Identifier { name, position: pos })
        } else {
            a.alloc_expr(E::Value { value: seed as i64, position: pos })
        };
    }
    match seed % 4 {
        0 => {
            let left = build_expr(a, depth - 1, seed + 1);
            let right = build_expr(a, depth - 1, seed + 2);
            a.alloc_expr(E::ArithmeticOp { left, operator: "+", right, position: pos })
        }
        1 => {
            let left = build_expr(a, depth - 1, seed + 1);
            let right = build_expr(a, depth - 1, seed + 2);
            a.alloc_expr(E::ComparisonOp { left, operator: ">", right, position: pos })
        }
        2 => {
            let operand = build_expr(a, depth - 1, seed + 1);
            a.alloc_expr(E::UnaryOp { operator: "-", operand, position: pos })
        }
        _ => {
            let mark = a.expr_scratch.len();
            let first = build_expr(a, depth - 1, seed + 1);
            a.expr_scratch.push(first);
            let second = build_expr(a, depth - 1, seed + 2);
            a.expr_scratch.push(second);
            let arguments = a.finish_expr_list(mark);
            let name = a.alloc_str(&format!("fn_{seed}"));
            a.alloc_expr(E::FunctionCall { name, arguments, position: pos })
        }
    }
}

/// Appends the program described by `p` to `a`. The functions end up in
/// `a.functions`, so the arena itself is the program.
pub fn build_program(a: &mut Arena, p: Profile) {
    use Statement as S;
    let mut seed = 0usize;
    for fi in 0..p.num_functions {
        let body_mark = a.stmt_scratch.len();
        for si in 0..p.stmts_per_function {
            seed += 1;
            let pos = Position { line: seed, column: 0 };
            let stmt = if p.if_every_n_stmts != 0 && si % p.if_every_n_stmts == p.if_every_n_stmts - 1 {
                let then_mark = a.stmt_scratch.len();
                let then_expr = build_expr(a, p.expr_depth, seed + 100);
                let then_stmt = a.alloc_stmt(S::ExpressionStatement { expression: then_expr, position: pos });
                a.stmt_scratch.push(then_stmt);
                let then_branch = a.finish_stmt_list(then_mark);

                let else_mark = a.stmt_scratch.len();
                let else_expr = build_expr(a, p.expr_depth, seed + 200);
                let else_stmt = a.alloc_stmt(S::ExpressionStatement { expression: else_expr, position: pos });
                a.stmt_scratch.push(else_stmt);
                let else_branch = a.finish_stmt_list(else_mark);

                let condition = build_expr(a, p.expr_depth, seed);
                S::If { condition, then_branch, else_branch: Some(else_branch), position: pos }
            } else {
                match si % 3 {
                    0 => {
                        let value = build_expr(a, p.expr_depth, seed);
                        S::Return { value, position: pos }
                    }
                    1 => {
                        let variable_name = a.alloc_str(&format!("v{si}"));
                        let value = build_expr(a, p.expr_depth, seed);
                        S::VariableDeclaration { variable_name, value, position: pos }
                    }
                    _ => {
                        let variable = a.alloc_str(&format!("v{si}"));
                        let value = build_expr(a, p.expr_depth, seed);
                        S::Assignment { variable, value, position: pos }
                    }
                }
            };
            let id = a.alloc_stmt(stmt);
            a.stmt_scratch.push(id);
        }
        let body = a.finish_stmt_list(body_mark);
        let name = a.alloc_str(&format!("func_{fi}"));
        a.functions.push(Function { name, body });
    }
}
