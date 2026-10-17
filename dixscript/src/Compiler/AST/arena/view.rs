// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena/view.rs"
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
            /// The id this handle names.
            #[inline]
            pub fn id(self) -> $id {
                self.id
            }

            /// The arena this handle reads from.
            #[inline]
            pub fn arena(self) -> &'a Arena {
                self.arena
            }

            /// The node this handle names.
            #[inline]
            pub fn node(self) -> &'a $node {
                self.arena.$get(self.id)
            }

            /// The source position of the node.
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
    /// A [`ValueNode`] together with the [`Arena`] it lives in.
    ///
    /// Made with [`Arena::value_ref`]. It is `Copy` and two words wide. `Display` prints what the
    /// owned `Value` prints, `Debug` prints what the owned `Value`'s derived `Debug` prints
    /// (by exporting the node, so it allocates), and `==` compares trees structurally, across
    /// arenas, with the owned rules: floats compare as IEEE numbers, so a tree holding `NaN`
    /// is not equal to itself.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::Arena;
    /// use dixscript::Compiler::AST::{Position, Value};
    ///
    /// let value = Value::Long { value: 5, position: Position::new(1, 1) };
    /// let (mut first, mut second) = (Arena::new(), Arena::new());
    /// second.import_value(&Value::Null { position: Position::new(9, 9) }); // shifts the ids
    /// let a = first.import_value(&value);
    /// let b = second.import_value(&value);
    ///
    /// assert_eq!(first.value_ref(a).to_string(), "5L");
    /// assert!(first.value_ref(a) == second.value_ref(b)); // equal across arenas
    /// ```
    ValueRef, ValueId, ValueNode, value, export_value, value_eq
);
ref_type!(
    /// An [`ExprNode`] together with the [`Arena`] it lives in.
    ///
    /// Made with [`Arena::expr_ref`]. It has the same `Display`, `Debug` and `==` behaviour as
    /// [`ValueRef`], matching the owned `Expression`.
    ExprRef, ExprId, ExprNode, expr, export_expr, expr_eq
);
ref_type!(
    /// A [`StmtNode`] together with the [`Arena`] it lives in.
    ///
    /// Made with [`Arena::stmt_ref`]. It has the same `Display`, `Debug` and `==` behaviour as
    /// [`ValueRef`], matching the owned `QuickFuncStatement`.
    StmtRef, StmtId, StmtNode, stmt, export_stmt, stmt_eq
);

impl Arena {
    /// Pairs a value id with this arena so it can be printed and compared.
    ///
    /// The id is not checked here; using the handle panics if `id` is not from this arena.
    #[inline]
    pub fn value_ref(&self, id: ValueId) -> ValueRef<'_> {
        ValueRef { arena: self, id }
    }

    /// Pairs an expression id with this arena so it can be printed and compared.
    ///
    /// The id is not checked here; using the handle panics if `id` is not from this arena.
    #[inline]
    pub fn expr_ref(&self, id: ExprId) -> ExprRef<'_> {
        ExprRef { arena: self, id }
    }

    /// Pairs a statement id with this arena so it can be printed and compared.
    ///
    /// The id is not checked here; using the handle panics if `id` is not from this arena.
    #[inline]
    pub fn stmt_ref(&self, id: StmtId) -> StmtRef<'_> {
        StmtRef { arena: self, id }
    }
}
