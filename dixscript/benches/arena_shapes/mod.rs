// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Measurement 5: option A against
// option B"
// ============================================================================
//! Shared pieces for `arena_vs_box_benchmark.rs`.
//!
//! The benchmark compares three representations of the same mirror AST:
//!   - `owned`: the shape of the current AST (`String`, `Vec`, `Box`).
//!   - `bump`:  option A, a bumpalo arena with `&'a` references.
//!   - `index`: option B, an owned arena with `u32` ids into per-kind `Vec`s.
//!
//! None of them uses the real `Compiler::AST` types. They reproduce its three
//! recursion shapes (Vec-only, single-Box, double-Box) so the gap between the
//! representations can be measured before any migration work starts.
//!
//! This file holds what the three share: the profiles, the tree fingerprint, the
//! equivalence checks that run before any timing, and the layout report.

pub mod bump;
pub mod index;
pub mod owned;

use std::mem::size_of;

/// Source position, identical in all three representations (mirrors the real
/// `Position`: two `usize`, 16 bytes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// Shape of one generated program. Both axes (function count, expression depth)
/// can be scaled separately to see how a gap grows along each.
#[derive(Clone, Copy)]
pub struct Profile {
    pub name: &'static str,
    pub num_functions: usize,
    pub stmts_per_function: usize,
    pub expr_depth: usize,
    pub if_every_n_stmts: usize,
    /// Byte budget handed to the bump arena. The index arena derives its Vec
    /// reservations from the same number (see `index::Arena::with_hint`) so both
    /// arenas start with a comparable amount of reserved memory.
    pub arena_capacity: usize,
}

pub const PROFILES: &[Profile] = &[
    // Matches mdix_files/advanced/FullFunctionTest.mdix: 162 functions, about
    // 2.6 statements per function, shallow (depth 1 to 2) expressions.
    Profile {
        name: "realistic",
        num_functions: 162,
        stmts_per_function: 3,
        expr_depth: 2,
        if_every_n_stmts: 5,
        arena_capacity: 1 << 20,
    },
    // Same function and statement count, much deeper expression nesting.
    Profile {
        name: "deep_expressions",
        num_functions: 162,
        stmts_per_function: 3,
        expr_depth: 12,
        if_every_n_stmts: 5,
        arena_capacity: 1 << 26,
    },
    // 25 times the realistic function count at the same shallow shape (the
    // scale of mdix_files/chemistry_db).
    Profile {
        name: "large_file",
        num_functions: 4000,
        stmts_per_function: 3,
        expr_depth: 2,
        if_every_n_stmts: 5,
        arena_capacity: 1 << 24,
    },
];

/// Used by `verify_representations` only, never timed. The timed profiles above
/// never emit an `If` (their `stmts_per_function` is below `if_every_n_stmts`), so
/// this one exercises the branch paths of all three builders and walkers.
const BRANCHY: Profile = Profile {
    name: "branches",
    num_functions: 24,
    stmts_per_function: 6,
    expr_depth: 3,
    if_every_n_stmts: 3,
    arena_capacity: 1 << 18,
};

/// Amount added to every `Value` node by the clone-and-mutate benchmark group.
pub const MUTATE_DELTA: i64 = 3;

/// A structural summary of a tree. Two trees with the same fingerprint have the
/// same node count, the same values, the same name and operator bytes and the
/// same positions. It is order-insensitive on purpose: the representations
/// allocate in different orders but must describe the same program.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Fingerprint {
    pub nodes: u64,
    pub value_nodes: u64,
    pub value_sum: i64,
    pub name_bytes: u64,
    pub operator_sum: u64,
    pub position_sum: u64,
}

impl Fingerprint {
    pub fn add_position(&mut self, p: Position) {
        let folded = (p.line as u64).wrapping_mul(31).wrapping_add(p.column as u64);
        self.position_sum = self.position_sum.wrapping_add(folded);
    }

    pub fn add_operator(&mut self, operator: &str) {
        self.operator_sum += operator.bytes().map(u64::from).sum::<u64>();
    }

    /// The fingerprint a tree must have after every `Value` gained `delta`.
    pub fn after_bump(self, delta: i64) -> Fingerprint {
        Fingerprint {
            value_sum: self.value_sum + delta * self.value_nodes as i64,
            ..self
        }
    }
}

/// Builds each representation from every profile, copies and mutates it the way
/// the benchmark groups do, and panics if any result differs from the owned
/// tree. Runs before any timing, so a builder bug cannot turn into a fast number.
/// One representation is built, checked and dropped at a time to keep peak memory
/// at one deep tree.
pub fn verify_representations() {
    for p in PROFILES.iter().chain(std::iter::once(&BRANCHY)) {
        let expected = {
            let tree = owned::build_program(*p);
            let fp = owned::fingerprint_program(&tree);
            assert_eq!(fp.nodes, owned::count_program(&tree), "owned count, {}", p.name);
            assert_eq!(fp, owned::fingerprint_program(&tree.clone()), "owned clone, {}", p.name);
            let mut mutated = tree.clone();
            owned::bump_values(&mut mutated, MUTATE_DELTA);
            assert_eq!(
                fp.after_bump(MUTATE_DELTA),
                owned::fingerprint_program(&mutated),
                "owned mutate, {}",
                p.name
            );
            fp
        };

        {
            let src = bump::AstArena::with_capacity(p.arena_capacity);
            let tree = bump::build_program(&src, *p);
            assert_eq!(expected, bump::fingerprint_program(&tree), "bump build, {}", p.name);
            assert_eq!(expected.nodes, bump::count_program(&tree), "bump count, {}", p.name);
            let dst = bump::AstArena::with_capacity(p.arena_capacity);
            let copy = bump::rebuild_program(&dst, &tree, 0);
            assert_eq!(expected, bump::fingerprint_program(&copy), "bump rebuild, {}", p.name);
            let dst2 = bump::AstArena::with_capacity(p.arena_capacity);
            let mutated = bump::rebuild_program(&dst2, &tree, MUTATE_DELTA);
            assert_eq!(
                expected.after_bump(MUTATE_DELTA),
                bump::fingerprint_program(&mutated),
                "bump rebuild with mutation, {}",
                p.name
            );
        }

        {
            let mut tree = index::Arena::with_hint(*p);
            let reserved = tree.capacities();
            index::build_program(&mut tree, *p);
            assert_eq!(reserved, tree.capacities(), "index hint too small, a buffer grew, {}", p.name);
            assert_eq!(expected, index::fingerprint_program(&tree), "index build, {}", p.name);
            assert_eq!(expected.nodes, index::count_program(&tree), "index count, {}", p.name);
            let mut bare = index::Arena::default();
            index::build_program(&mut bare, *p);
            assert_eq!(expected, index::fingerprint_program(&bare), "index build without hint, {}", p.name);
            assert_eq!(expected, index::fingerprint_program(&tree.clone()), "index clone, {}", p.name);
            let mut linear = tree.clone();
            index::bump_values_linear(&mut linear, MUTATE_DELTA);
            assert_eq!(
                expected.after_bump(MUTATE_DELTA),
                index::fingerprint_program(&linear),
                "index linear mutation, {}",
                p.name
            );
            let mut walked = tree.clone();
            index::bump_values_walk(&mut walked, MUTATE_DELTA);
            assert_eq!(
                expected.after_bump(MUTATE_DELTA),
                index::fingerprint_program(&walked),
                "index walked mutation, {}",
                p.name
            );
        }

        println!("verified {:<17} {} nodes, all three representations agree", p.name, expected.nodes);
    }
}

/// Prints node sizes and tree sizes. Node size is where the two arena shapes
/// differ most: option B swaps 8-byte pointers for 4-byte ids and moves every
/// child out of line.
pub fn report_layout() {
    println!(
        "node sizes in bytes (Expression / Statement): owned {} / {}, bump {} / {}, index {} / {}",
        size_of::<owned::Expression>(),
        size_of::<owned::Statement>(),
        size_of::<bump::Expression<'static>>(),
        size_of::<bump::Statement<'static>>(),
        size_of::<index::Expression>(),
        size_of::<index::Statement>(),
    );
    for p in PROFILES {
        let mut tree = index::Arena::default();
        index::build_program(&mut tree, *p);
        println!(
            "{:<17} {} functions, {} expressions, {} statements, index arena holds {} bytes",
            p.name,
            tree.function_count(),
            tree.expression_count(),
            tree.statement_count(),
            tree.used_bytes(),
        );
    }
}
