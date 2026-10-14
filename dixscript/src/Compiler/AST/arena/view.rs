// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena"
// ============================================================================
//! Borrowed handles that pair a node id with its arena.
//!
//! A bare id cannot be printed or compared, because the text and the children live in the
//! arena. [`ValueRef`], [`ExprRef`] and [`StmtRef`] carry both, so they can implement
//! `Display`, `Debug` and `PartialEq` the way the owned types do. They are `Copy` and cost two
//! words.
//!
//! * `Display` prints exactly what the owned type prints (see `display.rs`).
//! * `Debug` prints exactly what the owned type's derived `Debug` prints. It exports the node
//!   and delegates, so it allocates; it is for diagnostics, not for hot paths.
//! * `PartialEq` is structural, works across two different arenas, and follows the owned types'
//!   rules (floats compare as IEEE numbers, so `NaN != NaN`). See `equality.rs`.

use super::equality::{expr_eq, stmt_eq, value_eq};
use super::nodes::{ExprNode, StmtNode, ValueNode};
use super::{Arena, ExprId, StmtId, ValueId};
use crate::Compiler::AST::Position;
use std::fmt;

macro_rules! ref_type {
    ($(#[$meta:meta])* $name:ident, $id:ident, $node:ident, $get:ident, $export:ident, $eq:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy)]
        pub struct $name<'a> {
            arena: &'a Arena,
            id: $id,
        }

        impl<'a> $name<'a> {
            #[inline]
            pub fn id(self) -> $id {
                self.id
            }

            #[inline]
            pub fn arena(self) -> &'a Arena {
                self.arena
            }

            #[inline]
            pub fn node(self) -> &'a $node {
                self.arena.$get(self.id)
            }

            #[inline]
            pub fn position(self) -> Position {
                self.node().position()
            }
        }

        impl PartialEq for $name<'_> {
            fn eq(&self, other: &Self) -> bool {
                $eq(self.arena, self.id, other.arena, other.id)
            }
        }

        impl fmt::Debug for $name<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Debug::fmt(&self.arena.$export(self.id), f)
            }
        }
    };
}

ref_type!(
    /// A [`ValueNode`] with its arena.
    ValueRef, ValueId, ValueNode, value, export_value, value_eq
);
ref_type!(
    /// An [`ExprNode`] with its arena.
    ExprRef, ExprId, ExprNode, expr, export_expr, expr_eq
);
ref_type!(
    /// A [`StmtNode`] with its arena.
    StmtRef, StmtId, StmtNode, stmt, export_stmt, stmt_eq
);

impl Arena {
    #[inline]
    pub fn value_ref(&self, id: ValueId) -> ValueRef<'_> {
        ValueRef { arena: self, id }
    }

    #[inline]
    pub fn expr_ref(&self, id: ExprId) -> ExprRef<'_> {
        ExprRef { arena: self, id }
    }

    #[inline]
    pub fn stmt_ref(&self, id: StmtId) -> StmtRef<'_> {
        StmtRef { arena: self, id }
    }
}
