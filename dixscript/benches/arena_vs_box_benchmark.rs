//! Arena (bumpalo) vs Box/Vec/Clone AST — prototype benchmark
//!
//! This does NOT exercise dixscript's real Compiler::AST types. It's a
//! standalone mirror used to decide, before any migration work starts,
//! whether an arena-based AST is worth the rework the real one would need.
//! The `owned` module below reproduces the current AST's three recursion
//! shapes exactly (Compiler::AST::expressions.rs / statements.rs):
//!   - Vec-only    (FunctionCall args, If branches — no Box needed)
//!   - single-Box  (UnaryOp operand, PropertyAccess object)
//!   - double-Box  (ArithmeticOp/ComparisonOp left/right)
//! `arena` mirrors the same shapes with a bumpalo::Bump backing store:
//! &'a str instead of String, &'a [T] instead of Vec<T>, &'a T instead of
//! Box<T>. Every field ends up Copy, so Expression<'a>/Statement<'a>
//! themselves derive Copy.
//!
//! Profile sizes are grounded in mdix_files/advanced/FullFunctionTest.mdix
//! (162 functions, ~2.6 statements/function, mostly depth-1/2 expressions)
//! — "realistic" reproduces that shape; "deep_expressions" and
//! "large_file" stress the two axes (expression depth, function count)
//! separately to see how the gap scales along each.
//!
//! Groups:
//!   arena_vs_box_construct       — build one fresh tree from scratch
//!   arena_vs_box_clone_rebuild   — the ast.clone() operation from
//!                                  general_ast_enhancer.rs::enhance().
//!                                  Arena's side is a full walk-and-
//!                                  reallocate into a FRESH arena — no
//!                                  structural sharing — so this is the
//!                                  fair, conservative case, not the best
//!                                  case arena could achieve.
//!   arena_vs_box_traverse        — walk an already-built tree (no
//!                                  allocation), repeatedly
//!   arena_vs_box_churn           — build-then-drop in one measured unit,
//!                                  simulating mdix-lsp's reparse-on-
//!                                  keystroke pattern. Uses iter_custom
//!                                  (not plain iter) because plain iter
//!                                  defers drops in a batch after timing
//!                                  to avoid drop noise — here the drop
//!                                  cost is specifically what we want
//!                                  measured.
//!
//! See docs/arena-vs-box-findings.md for the sandboxed pre-check numbers
//! (rustc 1.75, manual Instant-based timing — criterion needs edition2024
//! / rustc 1.85+ so it couldn't run there) this file is meant to confirm
//! or correct with real criterion statistics on this crate's actual CI
//! toolchain.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::{Duration, Instant};

// =============================================================================
// Shared: Position — Copy, 16 bytes, identical in both representations
// (mirrors the real dixscript Position struct exactly).
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

// =============================================================================
// OWNED representation — current DixScript shape.
// =============================================================================

mod owned {
    use super::Position;

    #[derive(Debug, Clone, PartialEq)]
    pub enum Expression {
        Identifier { name: String, position: Position },
        Value { value: i64, position: Position },
        FunctionCall { name: String, arguments: Vec<Expression>, position: Position },
        UnaryOp { operator: String, operand: Box<Expression>, position: Position },
        ArithmeticOp { left: Box<Expression>, operator: String, right: Box<Expression>, position: Position },
        ComparisonOp { left: Box<Expression>, operator: String, right: Box<Expression>, position: Position },
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

    pub fn count_expr(e: &Expression) -> u64 {
        match e {
            Expression::Identifier { .. } | Expression::Value { .. } => 1,
            Expression::FunctionCall { arguments, .. } => {
                1 + arguments.iter().map(count_expr).sum::<u64>()
            }
            Expression::UnaryOp { operand, .. } => 1 + count_expr(operand),
            Expression::ArithmeticOp { left, right, .. }
            | Expression::ComparisonOp { left, right, .. } => {
                1 + count_expr(left) + count_expr(right)
            }
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
}

// =============================================================================
// ARENA representation — bumpalo-backed.
// =============================================================================

mod arena {
    use super::Position;
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

    pub fn count_expr(e: &Expression) -> u64 {
        match e {
            Expression::Identifier { .. } | Expression::Value { .. } => 1,
            Expression::FunctionCall { arguments, .. } => {
                1 + arguments.iter().map(count_expr).sum::<u64>()
            }
            Expression::UnaryOp { operand, .. } => 1 + count_expr(operand),
            Expression::ArithmeticOp { left, right, .. }
            | Expression::ComparisonOp { left, right, .. } => {
                1 + count_expr(left) + count_expr(right)
            }
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

    /// Fair, CONSERVATIVE equivalent of `ast.clone()` from
    /// general_ast_enhancer.rs: real code needs an independently-owned,
    /// independently-mutable copy to run enhancement passes over. Since
    /// arena refs are shared/immutable, the honest worst-case equivalent is
    /// a full walk that re-allocates every node into a FRESH arena — no
    /// structural sharing, same total node count visited as owned .clone().
    /// A real implementation could rebuild only the modified spine and
    /// share the rest, which would beat this — this deliberately does NOT
    /// take that shortcut, so the comparison is fair-to-Box, not tilted.
    pub fn rebuild_expr<'a>(dst: &'a AstArena, e: &Expression) -> &'a Expression<'a> {
        let rebuilt = match e {
            Expression::Identifier { name, position } => Expression::Identifier {
                name: dst.alloc_str(name),
                position: *position,
            },
            Expression::Value { value, position } => Expression::Value { value: *value, position: *position },
            Expression::FunctionCall { name, arguments, position } => {
                let mut v = dst.vec::<Expression>();
                for a in arguments.iter() {
                    v.push(*rebuild_expr(dst, a));
                }
                Expression::FunctionCall {
                    name: dst.alloc_str(name),
                    arguments: v.into_bump_slice(),
                    position: *position,
                }
            }
            Expression::UnaryOp { operator, operand, position } => Expression::UnaryOp {
                operator: dst.alloc_str(operator),
                operand: rebuild_expr(dst, operand),
                position: *position,
            },
            Expression::ArithmeticOp { left, operator, right, position } => Expression::ArithmeticOp {
                left: rebuild_expr(dst, left),
                operator: dst.alloc_str(operator),
                right: rebuild_expr(dst, right),
                position: *position,
            },
            Expression::ComparisonOp { left, operator, right, position } => Expression::ComparisonOp {
                left: rebuild_expr(dst, left),
                operator: dst.alloc_str(operator),
                right: rebuild_expr(dst, right),
                position: *position,
            },
            Expression::PropertyAccess { object, property, position } => Expression::PropertyAccess {
                object: rebuild_expr(dst, object),
                property: dst.alloc_str(property),
                position: *position,
            },
        };
        dst.alloc(rebuilt)
    }

    pub fn rebuild_stmt<'a>(dst: &'a AstArena, s: &Statement) -> Statement<'a> {
        match s {
            Statement::Return { value, position } => {
                Statement::Return { value: *rebuild_expr(dst, value), position: *position }
            }
            Statement::Assignment { variable, value, position } => Statement::Assignment {
                variable: dst.alloc_str(variable),
                value: *rebuild_expr(dst, value),
                position: *position,
            },
            Statement::VariableDeclaration { variable_name, value, position } => Statement::VariableDeclaration {
                variable_name: dst.alloc_str(variable_name),
                value: *rebuild_expr(dst, value),
                position: *position,
            },
            Statement::ExpressionStatement { expression, position } => Statement::ExpressionStatement {
                expression: *rebuild_expr(dst, expression),
                position: *position,
            },
            Statement::If { condition, then_branch, else_branch, position } => {
                let mut tb = dst.vec::<Statement>();
                for s in then_branch.iter() {
                    tb.push(rebuild_stmt(dst, s));
                }
                let eb = else_branch.map(|branch| {
                    let mut v = dst.vec::<Statement>();
                    for s in branch.iter() {
                        v.push(rebuild_stmt(dst, s));
                    }
                    v.into_bump_slice() as &[Statement]
                });
                Statement::If {
                    condition: *rebuild_expr(dst, condition),
                    then_branch: tb.into_bump_slice(),
                    else_branch: eb,
                    position: *position,
                }
            }
        }
    }

    pub fn rebuild_program<'a>(dst: &'a AstArena, p: &Program) -> Program<'a> {
        let mut fns = dst.vec::<Function>();
        for f in p.functions.iter() {
            let mut body = dst.vec::<Statement>();
            for s in f.body.iter() {
                body.push(rebuild_stmt(dst, s));
            }
            fns.push(Function { name: dst.alloc_str(f.name), body: body.into_bump_slice() });
        }
        Program { functions: fns.into_bump_slice() }
    }
}

// =============================================================================
// Synthetic program generator — deterministic, no RNG. Both representations
// get identical tree shapes per profile so the comparison is fair.
// =============================================================================

#[derive(Clone, Copy)]
struct Profile {
    name: &'static str,
    num_functions: usize,
    stmts_per_function: usize,
    expr_depth: usize,
    if_every_n_stmts: usize,
    arena_capacity: usize,
}

const PROFILES: &[Profile] = &[
    // Matches mdix_files/advanced/FullFunctionTest.mdix: 162 functions,
    // ~2.6 statements/function, shallow (depth ~1-2) expressions.
    Profile {
        name: "realistic",
        num_functions: 162,
        stmts_per_function: 3,
        expr_depth: 2,
        if_every_n_stmts: 5,
        arena_capacity: 1 << 20,
    },
    // Same function/statement count, much deeper expression nesting —
    // isolates how the gap scales with expression depth specifically.
    Profile {
        name: "deep_expressions",
        num_functions: 162,
        stmts_per_function: 3,
        expr_depth: 12,
        if_every_n_stmts: 5,
        arena_capacity: 1 << 26,
    },
    // 25x the realistic function count at the same shallow shape —
    // isolates how the gap scales with overall file size (relevant to
    // mdix_files/chemistry_db-scale files).
    Profile {
        name: "large_file",
        num_functions: 4000,
        stmts_per_function: 3,
        expr_depth: 2,
        if_every_n_stmts: 5,
        arena_capacity: 1 << 24,
    },
];

fn build_owned_expr(depth: usize, seed: usize) -> owned::Expression {
    use owned::Expression as E;
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
            left: Box::new(build_owned_expr(depth - 1, seed + 1)),
            operator: "+".to_string(),
            right: Box::new(build_owned_expr(depth - 1, seed + 2)),
            position: pos,
        },
        1 => E::ComparisonOp {
            left: Box::new(build_owned_expr(depth - 1, seed + 1)),
            operator: ">".to_string(),
            right: Box::new(build_owned_expr(depth - 1, seed + 2)),
            position: pos,
        },
        2 => E::UnaryOp {
            operator: "-".to_string(),
            operand: Box::new(build_owned_expr(depth - 1, seed + 1)),
            position: pos,
        },
        _ => E::FunctionCall {
            name: format!("fn_{seed}"),
            arguments: vec![build_owned_expr(depth - 1, seed + 1), build_owned_expr(depth - 1, seed + 2)],
            position: pos,
        },
    }
}

fn build_owned_program(p: Profile) -> owned::Program {
    use owned::{Function, Program, Statement as S};
    let mut functions = Vec::with_capacity(p.num_functions);
    let mut seed = 0usize;
    for fi in 0..p.num_functions {
        let mut body = Vec::with_capacity(p.stmts_per_function);
        for si in 0..p.stmts_per_function {
            seed += 1;
            let pos = Position { line: seed, column: 0 };
            let stmt = if p.if_every_n_stmts != 0 && si % p.if_every_n_stmts == p.if_every_n_stmts - 1 {
                S::If {
                    condition: build_owned_expr(p.expr_depth, seed),
                    then_branch: vec![S::ExpressionStatement { expression: build_owned_expr(p.expr_depth, seed + 100), position: pos }],
                    else_branch: Some(vec![S::ExpressionStatement { expression: build_owned_expr(p.expr_depth, seed + 200), position: pos }]),
                    position: pos,
                }
            } else {
                match si % 3 {
                    0 => S::Return { value: build_owned_expr(p.expr_depth, seed), position: pos },
                    1 => S::VariableDeclaration {
                        variable_name: format!("v{si}"),
                        value: build_owned_expr(p.expr_depth, seed),
                        position: pos,
                    },
                    _ => S::Assignment {
                        variable: format!("v{si}"),
                        value: build_owned_expr(p.expr_depth, seed),
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

fn build_arena_expr<'a>(a: &'a arena::AstArena, depth: usize, seed: usize) -> arena::Expression<'a> {
    use arena::Expression as E;
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
            left: a.alloc(build_arena_expr(a, depth - 1, seed + 1)),
            operator: "+",
            right: a.alloc(build_arena_expr(a, depth - 1, seed + 2)),
            position: pos,
        },
        1 => E::ComparisonOp {
            left: a.alloc(build_arena_expr(a, depth - 1, seed + 1)),
            operator: ">",
            right: a.alloc(build_arena_expr(a, depth - 1, seed + 2)),
            position: pos,
        },
        2 => E::UnaryOp {
            operator: "-",
            operand: a.alloc(build_arena_expr(a, depth - 1, seed + 1)),
            position: pos,
        },
        _ => {
            let mut v = a.vec::<E>();
            v.push(build_arena_expr(a, depth - 1, seed + 1));
            v.push(build_arena_expr(a, depth - 1, seed + 2));
            E::FunctionCall {
                name: a.alloc_str(&format!("fn_{seed}")),
                arguments: v.into_bump_slice(),
                position: pos,
            }
        }
    }
}

fn build_arena_program<'a>(a: &'a arena::AstArena, p: Profile) -> arena::Program<'a> {
    use arena::{Function, Program, Statement as S};
    let mut functions = a.vec::<Function>();
    let mut seed = 0usize;
    for fi in 0..p.num_functions {
        let mut body = a.vec::<S>();
        for si in 0..p.stmts_per_function {
            seed += 1;
            let pos = Position { line: seed, column: 0 };
            let stmt = if p.if_every_n_stmts != 0 && si % p.if_every_n_stmts == p.if_every_n_stmts - 1 {
                let mut tb = a.vec::<S>();
                tb.push(S::ExpressionStatement { expression: build_arena_expr(a, p.expr_depth, seed + 100), position: pos });
                let mut eb = a.vec::<S>();
                eb.push(S::ExpressionStatement { expression: build_arena_expr(a, p.expr_depth, seed + 200), position: pos });
                S::If {
                    condition: build_arena_expr(a, p.expr_depth, seed),
                    then_branch: tb.into_bump_slice(),
                    else_branch: Some(eb.into_bump_slice()),
                    position: pos,
                }
            } else {
                match si % 3 {
                    0 => S::Return { value: build_arena_expr(a, p.expr_depth, seed), position: pos },
                    1 => S::VariableDeclaration {
                        variable_name: a.alloc_str(&format!("v{si}")),
                        value: build_arena_expr(a, p.expr_depth, seed),
                        position: pos,
                    },
                    _ => S::Assignment {
                        variable: a.alloc_str(&format!("v{si}")),
                        value: build_arena_expr(a, p.expr_depth, seed),
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

/// (sample_size, measurement_time) tuned per profile, same philosophy as
/// stress_test_benchmark.rs: fewer samples / longer windows as tree size
/// (and therefore per-iteration cost) grows.
fn tuning_for(profile: &Profile) -> (usize, Duration) {
    match profile.name {
        "large_file" => (20, Duration::from_secs(15)),
        "deep_expressions" => (30, Duration::from_secs(15)),
        _ => (60, Duration::from_secs(8)),
    }
}

// =============================================================================
// Benchmark 1 — construct (build one fresh tree from scratch)
// =============================================================================

fn bench_construct(c: &mut Criterion) {
    let mut group = c.benchmark_group("arena_vs_box_construct");

    for profile in PROFILES {
        let (sample_size, measurement_time) = tuning_for(profile);
        group.sample_size(sample_size);
        group.measurement_time(measurement_time);

        group.bench_with_input(BenchmarkId::new("owned", profile.name), profile, |b, p| {
            b.iter(|| black_box(build_owned_program(*p)));
        });

        group.bench_with_input(BenchmarkId::new("arena", profile.name), profile, |b, p| {
            b.iter(|| {
                let a = arena::AstArena::with_capacity(p.arena_capacity);
                black_box(build_arena_program(&a, *p));
            });
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 2 — clone/rebuild (the ast.clone() operation in
// general_ast_enhancer.rs::enhance()). Tree built once outside the timed
// region; only the copy operation itself is measured.
// =============================================================================

fn bench_clone_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("arena_vs_box_clone_rebuild");

    for profile in PROFILES {
        let (sample_size, measurement_time) = tuning_for(profile);
        group.sample_size(sample_size);
        group.measurement_time(measurement_time);

        let owned_prog = build_owned_program(*profile);
        group.bench_with_input(BenchmarkId::new("owned_clone", profile.name), profile, |b, _| {
            b.iter(|| black_box(owned_prog.clone()));
        });

        let src_arena = arena::AstArena::with_capacity(profile.arena_capacity);
        let src_prog = build_arena_program(&src_arena, *profile);
        group.bench_with_input(BenchmarkId::new("arena_rebuild", profile.name), profile, |b, p| {
            b.iter(|| {
                let dst = arena::AstArena::with_capacity(p.arena_capacity);
                black_box(arena::rebuild_program(&dst, &src_prog));
            });
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 3 — traverse (walk an already-built tree, no allocation)
// =============================================================================

fn bench_traverse(c: &mut Criterion) {
    let mut group = c.benchmark_group("arena_vs_box_traverse");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(8));

    for profile in PROFILES {
        let owned_prog = build_owned_program(*profile);
        group.throughput(Throughput::Elements(owned::count_program(&owned_prog)));
        group.bench_with_input(BenchmarkId::new("owned", profile.name), profile, |b, _| {
            b.iter(|| black_box(owned::count_program(&owned_prog)));
        });

        let src_arena = arena::AstArena::with_capacity(profile.arena_capacity);
        let src_prog = build_arena_program(&src_arena, *profile);
        group.bench_with_input(BenchmarkId::new("arena", profile.name), profile, |b, _| {
            b.iter(|| black_box(arena::count_program(&src_prog)));
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark 4 — build+drop churn (mdix-lsp reparse-on-keystroke simulation)
//
// Uses iter_custom rather than plain iter: plain iter defers drops in a
// batch *after* timing stops specifically to keep drop cost out of the
// measurement. Here the drop cost (many small deallocations for owned vs.
// one bulk chunk free for arena) is the thing being measured, so it has to
// be timed manually, inside the loop, before the next sample starts.
// =============================================================================

fn bench_churn(c: &mut Criterion) {
    let mut group = c.benchmark_group("arena_vs_box_churn");

    for profile in PROFILES {
        let (sample_size, measurement_time) = tuning_for(profile);
        group.sample_size(sample_size);
        group.measurement_time(measurement_time);

        group.bench_with_input(BenchmarkId::new("owned", profile.name), profile, |b, p| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let start = Instant::now();
                    let prog = build_owned_program(*p);
                    black_box(&prog);
                    drop(prog);
                    total += start.elapsed();
                }
                total
            });
        });

        group.bench_with_input(BenchmarkId::new("arena", profile.name), profile, |b, p| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let start = Instant::now();
                    let a = arena::AstArena::with_capacity(p.arena_capacity);
                    let prog = build_arena_program(&a, *p);
                    black_box(&prog);
                    // `prog` is Copy (just references into `a`) — nothing to
                    // drop there. `a` is what actually holds the allocation;
                    // dropping it is the real deallocation cost being timed.
                    drop(a);
                    total += start.elapsed();
                }
                total
            });
        });
    }

    group.finish();
}

// =============================================================================
// Registration
// =============================================================================

criterion_group!(benches, bench_construct, bench_clone_rebuild, bench_traverse, bench_churn);
criterion_main!(benches);
