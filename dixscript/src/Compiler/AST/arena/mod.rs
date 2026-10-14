// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena"
// ============================================================================
//! Index arena for the `Value`, `Expression` and `QuickFuncStatement` family.
//!
//! These three types and their helper structs (`ObjectProperty`, `SwitchCase`) are one
//! mutually recursive family: a `Value` holds `Expression`s, an `Expression` holds a
//! `Value`, and a lambda `Value` holds `QuickFuncStatement`s that hold both. They are also
//! where nearly every allocation of a compile lives. This module stores the whole family in
//! one owned [`Arena`]: a `Vec` per node kind, `u32` ids for children, `(start, len)` spans
//! for lists, and one text buffer for every string.
//!
//! The arena has no lifetime, so it is an ordinary `'static`, `Send + Sync`, `Clone` value.
//! Every node is `Copy`. Nothing in the compiler uses it yet: [`Arena::import_value`] and
//! friends copy an owned tree in, and [`Arena::export_value`] and friends copy a tree back
//! out as the original owned types, losslessly. That pair is what lets consumers move over
//! one at a time while the rest keeps using the owned tree.

mod export;
mod import;
mod nodes;

pub use nodes::{ExprNode, ObjectPropNode, StmtNode, SwitchCaseNode, UnknownData, ValueNode};

use std::ops::Range;

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).expect("arena exceeds the u32 id space")
}

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(u32);

        impl $name {
            #[inline]
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

macro_rules! span_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name {
            start: u32,
            len: u32,
        }

        impl $name {
            #[inline]
            pub(super) fn new(start: usize, len: usize) -> Self {
                $name { start: to_u32(start), len: to_u32(len) }
            }

            #[inline]
            pub fn len(self) -> usize {
                self.len as usize
            }

            #[inline]
            pub fn is_empty(self) -> bool {
                self.len == 0
            }

            #[inline]
            fn range(self) -> Range<usize> {
                let start = self.start as usize;
                start..start + self.len as usize
            }
        }
    };
}

id_type!(
    /// Handle to a [`ValueNode`].
    ValueId
);
id_type!(
    /// Handle to an [`ExprNode`].
    ExprId
);
id_type!(
    /// Handle to a [`StmtNode`].
    StmtId
);
id_type!(
    /// Handle to a [`SwitchCaseNode`] (used for the `default` case of a switch).
    CaseId
);
id_type!(
    /// Handle to an [`UnknownData`] record.
    UnknownId
);

span_type!(
    /// A span of the arena's text buffer.
    Str
);
span_type!(
    /// A run of [`Str`] in the arena (lambda parameters, qualified-identifier parts, paths).
    StrList
);
span_type!(
    /// A run of [`ValueId`].
    ValueList
);
span_type!(
    /// A run of [`ExprId`].
    ExprList
);
span_type!(
    /// A run of [`StmtId`].
    StmtList
);
span_type!(
    /// A run of [`ObjectPropNode`].
    PropList
);
span_type!(
    /// A run of [`SwitchCaseNode`].
    CaseList
);

/// Owns every node of the family. See the module documentation.
#[derive(Debug, Clone, Default)]
pub struct Arena {
    values: Vec<ValueNode>,
    exprs: Vec<ExprNode>,
    stmts: Vec<StmtNode>,
    props: Vec<ObjectPropNode>,
    cases: Vec<SwitchCaseNode>,
    unknowns: Vec<UnknownData>,
    value_lists: Vec<ValueId>,
    expr_lists: Vec<ExprId>,
    stmt_lists: Vec<StmtId>,
    str_lists: Vec<Str>,
    text: String,
}

impl Arena {
    pub fn new() -> Self {
        Self::default()
    }

    // -- allocation ---------------------------------------------------------

    #[inline]
    pub fn alloc_value(&mut self, node: ValueNode) -> ValueId {
        let id = ValueId(to_u32(self.values.len()));
        self.values.push(node);
        id
    }

    #[inline]
    pub fn alloc_expr(&mut self, node: ExprNode) -> ExprId {
        let id = ExprId(to_u32(self.exprs.len()));
        self.exprs.push(node);
        id
    }

    #[inline]
    pub fn alloc_stmt(&mut self, node: StmtNode) -> StmtId {
        let id = StmtId(to_u32(self.stmts.len()));
        self.stmts.push(node);
        id
    }

    #[inline]
    pub fn alloc_str(&mut self, s: &str) -> Str {
        let start = self.text.len();
        self.text.push_str(s);
        Str::new(start, s.len())
    }

    // -- access -------------------------------------------------------------

    #[inline]
    pub fn value(&self, id: ValueId) -> &ValueNode {
        &self.values[id.index()]
    }

    #[inline]
    pub fn expr(&self, id: ExprId) -> &ExprNode {
        &self.exprs[id.index()]
    }

    #[inline]
    pub fn stmt(&self, id: StmtId) -> &StmtNode {
        &self.stmts[id.index()]
    }

    #[inline]
    pub fn case(&self, id: CaseId) -> &SwitchCaseNode {
        &self.cases[id.index()]
    }

    #[inline]
    pub fn unknown(&self, id: UnknownId) -> &UnknownData {
        &self.unknowns[id.index()]
    }

    #[inline]
    pub fn text(&self, s: Str) -> &str {
        &self.text[s.range()]
    }

    #[inline]
    pub fn value_list(&self, list: ValueList) -> &[ValueId] {
        &self.value_lists[list.range()]
    }

    #[inline]
    pub fn expr_list(&self, list: ExprList) -> &[ExprId] {
        &self.expr_lists[list.range()]
    }

    #[inline]
    pub fn stmt_list(&self, list: StmtList) -> &[StmtId] {
        &self.stmt_lists[list.range()]
    }

    #[inline]
    pub fn str_list(&self, list: StrList) -> &[Str] {
        &self.str_lists[list.range()]
    }

    #[inline]
    pub fn prop_list(&self, list: PropList) -> &[ObjectPropNode] {
        &self.props[list.range()]
    }

    #[inline]
    pub fn case_list(&self, list: CaseList) -> &[SwitchCaseNode] {
        &self.cases[list.range()]
    }

    // -- size ---------------------------------------------------------------

    pub fn value_count(&self) -> usize {
        self.values.len()
    }

    pub fn expr_count(&self) -> usize {
        self.exprs.len()
    }

    pub fn stmt_count(&self) -> usize {
        self.stmts.len()
    }

    /// Bytes held by the node, list and text buffers (length, not capacity).
    pub fn used_bytes(&self) -> usize {
        use std::mem::size_of;
        self.values.len() * size_of::<ValueNode>()
            + self.exprs.len() * size_of::<ExprNode>()
            + self.stmts.len() * size_of::<StmtNode>()
            + self.props.len() * size_of::<ObjectPropNode>()
            + self.cases.len() * size_of::<SwitchCaseNode>()
            + self.unknowns.len() * size_of::<UnknownData>()
            + self.value_lists.len() * size_of::<ValueId>()
            + self.expr_lists.len() * size_of::<ExprId>()
            + self.stmt_lists.len() * size_of::<StmtId>()
            + self.str_lists.len() * size_of::<Str>()
            + self.text.len()
    }
}
