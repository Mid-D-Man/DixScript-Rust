// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena/mod.rs"
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
//! Every node is `Copy`. Nothing in the compiler reads it yet: [`Arena::import_value`] and
//! its siblings copy an owned tree in, and [`Arena::export_value`] and its siblings copy a
//! tree back out as the original owned types, losslessly. That pair is what lets consumers
//! move over one at a time while the rest keeps using the owned tree.
//!
//! # What is in this module
//!
//! * [`Arena`] holds the nodes. Its ids ([`ValueId`], [`ExprId`], [`StmtId`], [`CaseId`],
//!   [`UnknownId`]) and spans ([`Str`], [`StrList`], [`ValueList`], [`ExprList`],
//!   [`StmtList`], [`PropList`], [`CaseList`]) are plain `Copy` handles into it.
//! * [`ValueNode`], [`ExprNode`], [`StmtNode`], [`ObjectPropNode`], [`SwitchCaseNode`] and
//!   [`UnknownData`] are the nodes.
//! * [`ValueRef`], [`ExprRef`] and [`StmtRef`] pair an id with its arena, because a bare id
//!   cannot be printed or compared. They implement `Display`, `Debug` and `PartialEq` with
//!   the same output and rules as the owned types.
//! * [`Document`] is the arena-resident form of a whole `DixScript`: one `Arena` plus the
//!   sections, with `Doc*` twins of the five sections that hold values. `Document` implements
//!   `Display` itself, and [`Arena::display`] pairs a `Doc*` section with its arena as an
//!   [`ArenaDisplay`] that prints. Both print exactly what the owned types print.
//!
//! # Examples
//!
//! Copy a tree in, read it back through a handle, and copy it out again:
//!
//! ```
//! use dixscript::Compiler::AST::arena::Arena;
//! use dixscript::Compiler::AST::{Position, Value};
//!
//! let value = Value::Array {
//!     values: vec![
//!         Value::Integer { value: 1, position: Position::new(1, 2) },
//!         Value::String { value: "two".to_string(), position: Position::new(1, 5) },
//!     ],
//!     position: Position::new(1, 1),
//! };
//!
//! let mut arena = Arena::new();
//! let id = arena.import_value(&value);
//!
//! // A handle prints and compares like the owned value.
//! assert_eq!(arena.value_ref(id).to_string(), value.to_string());
//! // Exporting returns an equal owned tree.
//! assert_eq!(arena.export_value(id), value);
//! ```

#![warn(missing_docs)]

mod display;
mod document;
mod document_display;
mod equality;
mod export;
mod import;
mod merge;
mod nodes;
mod view;

pub use document::{
    DocData, DocDataEntry, DocPropertyAssignment, DocQuickFuncParam, DocQuickFuncs,
    DocQuickFunction, DocRawBlock, DocRawField, DocSchema, DocSchemaField, DocSecurity,
    DocSecurityEntry, DocSecurityField, Document,
};
pub use document_display::ArenaDisplay;
pub use merge::ArenaOffsets;
pub use nodes::{ExprNode, ObjectPropNode, StmtNode, SwitchCaseNode, UnknownData, ValueNode};
pub use view::{ExprRef, StmtRef, ValueRef};

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
            /// The position of the node this id names in its arena's buffer for that kind.
            ///
            /// Ids from different arenas are not comparable.
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

            /// The number of entries in the run.
            #[inline]
            pub fn len(self) -> usize {
                self.len as usize
            }

            /// Whether the run has no entries.
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
    /// Handle to a [`ValueNode`] in an [`Arena`].
    ValueId
);
id_type!(
    /// Handle to an [`ExprNode`] in an [`Arena`].
    ExprId
);
id_type!(
    /// Handle to a [`StmtNode`] in an [`Arena`].
    StmtId
);
id_type!(
    /// Handle to a [`SwitchCaseNode`] in an [`Arena`]. Used for the default case of a switch.
    CaseId
);
id_type!(
    /// Handle to an [`UnknownData`] record in an [`Arena`].
    UnknownId
);

span_type!(
    /// A span of an [`Arena`]'s text buffer. Read it with [`Arena::text`].
    Str
);
span_type!(
    /// A run of [`Str`] in an [`Arena`], such as lambda parameters or a qualified identifier's
    /// parts. Read it with [`Arena::str_list`].
    StrList
);
span_type!(
    /// A run of [`ValueId`] in an [`Arena`]. Read it with [`Arena::value_list`].
    ValueList
);
span_type!(
    /// A run of [`ExprId`] in an [`Arena`]. Read it with [`Arena::expr_list`].
    ExprList
);
span_type!(
    /// A run of [`StmtId`] in an [`Arena`]. Read it with [`Arena::stmt_list`].
    StmtList
);
span_type!(
    /// A run of [`ObjectPropNode`] in an [`Arena`]. Read it with [`Arena::prop_list`].
    PropList
);
span_type!(
    /// A run of [`SwitchCaseNode`] in an [`Arena`]. Read it with [`Arena::case_list`].
    CaseList
);

/// Owns every node of the `Value` family.
///
/// An arena holds one buffer per node kind, one per list kind, and one `String` for all text.
/// Nodes refer to each other with ids and spans (see the [module documentation](self)), so the
/// arena is a plain owned value with no lifetime: it is `Send`, `Sync`, `Clone` and
/// `'static`.
///
/// Ids and spans are only meaningful for the arena that issued them. Using one with another
/// arena reads the wrong node or panics; see the `# Panics` section of each accessor.
///
/// # Examples
///
/// ```
/// use dixscript::Compiler::AST::arena::{Arena, ValueNode};
/// use dixscript::Compiler::AST::{Position, Value};
///
/// let mut arena = Arena::new();
/// let id = arena.import_value(&Value::Boolean { value: true, position: Position::new(1, 1) });
///
/// assert!(matches!(arena.value(id), ValueNode::Boolean { value: true, .. }));
/// assert_eq!(arena.value_count(), 1);
/// ```
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
    /// Creates an empty arena.
    pub fn new() -> Self {
        Self::default()
    }

    // -- allocation ---------------------------------------------------------

    /// Adds a value node and returns its id.
    ///
    /// # Panics
    ///
    /// Panics if the arena already holds more value nodes than a `u32` id can name.
    #[inline]
    pub fn alloc_value(&mut self, node: ValueNode) -> ValueId {
        let id = ValueId(to_u32(self.values.len()));
        self.values.push(node);
        id
    }

    /// Adds an expression node and returns its id.
    ///
    /// # Panics
    ///
    /// Panics if the arena already holds more expression nodes than a `u32` id can name.
    #[inline]
    pub fn alloc_expr(&mut self, node: ExprNode) -> ExprId {
        let id = ExprId(to_u32(self.exprs.len()));
        self.exprs.push(node);
        id
    }

    /// Adds a statement node and returns its id.
    ///
    /// # Panics
    ///
    /// Panics if the arena already holds more statement nodes than a `u32` id can name.
    #[inline]
    pub fn alloc_stmt(&mut self, node: StmtNode) -> StmtId {
        let id = StmtId(to_u32(self.stmts.len()));
        self.stmts.push(node);
        id
    }

    /// Copies `s` into the text buffer and returns its span.
    ///
    /// The text is not deduplicated: allocating the same string twice stores it twice.
    ///
    /// # Panics
    ///
    /// Panics if the text buffer would grow past what a `u32` offset can address (4 GiB).
    #[inline]
    pub fn alloc_str(&mut self, s: &str) -> Str {
        let start = self.text.len();
        self.text.push_str(s);
        Str::new(start, s.len())
    }

    /// Copies `ids` into the value-list store and returns the run that names them.
    ///
    /// A parser uses this for a list whose length is only known once it has been read, such as
    /// the arguments of a call: build each child, collect the ids, and store the run once.
    /// The run is stored as one contiguous block, so it is read back with
    /// [`value_list`](Self::value_list).
    ///
    /// # Panics
    ///
    /// Panics if the store would grow past what a `u32` offset can address.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, ValueNode};
    /// use dixscript::Compiler::AST::Position;
    ///
    /// let mut arena = Arena::new();
    /// let a = arena.alloc_value(ValueNode::Integer { value: 1, position: Position::new(1, 1) });
    /// let b = arena.alloc_value(ValueNode::Boolean { value: true, position: Position::new(1, 4) });
    /// let list = arena.alloc_value_list(&[a, b]);
    ///
    /// assert_eq!(list.len(), 2);
    /// assert_eq!(arena.value_list(list), &[a, b]);
    /// ```
    #[inline]
    pub fn alloc_value_list(&mut self, ids: &[ValueId]) -> ValueList {
        let start = self.value_lists.len();
        self.value_lists.extend_from_slice(ids);
        ValueList::new(start, ids.len())
    }

    /// Copies `ids` into the expression-list store and returns the run that names them.
    ///
    /// The counterpart of [`alloc_value_list`](Self::alloc_value_list) for lists of expressions,
    /// such as the arguments of a function call. Read the run back with
    /// [`expr_list`](Self::expr_list).
    ///
    /// # Panics
    ///
    /// Panics if the store would grow past what a `u32` offset can address.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, ExprNode};
    /// use dixscript::Compiler::AST::Position;
    ///
    /// let mut arena = Arena::new();
    /// let name = arena.alloc_str("x");
    /// let a = arena.alloc_expr(ExprNode::Identifier { name, position: Position::new(1, 1) });
    /// let b = arena.alloc_expr(ExprNode::Identifier { name, position: Position::new(1, 4) });
    /// let list = arena.alloc_expr_list(&[a, b]);
    ///
    /// assert_eq!(list.len(), 2);
    /// assert_eq!(arena.expr_list(list), &[a, b]);
    /// ```
    #[inline]
    pub fn alloc_expr_list(&mut self, ids: &[ExprId]) -> ExprList {
        let start = self.expr_lists.len();
        self.expr_lists.extend_from_slice(ids);
        ExprList::new(start, ids.len())
    }

    /// Copies `props` into the object-property store and returns the run that names them.
    ///
    /// An object literal collects its finished properties and stores the run once, when its
    /// closing brace is reached. Read the run back with [`prop_list`](Self::prop_list).
    ///
    /// # Panics
    ///
    /// Panics if the store would grow past what a `u32` offset can address.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, ObjectPropNode, ValueNode};
    /// use dixscript::Compiler::AST::Position;
    ///
    /// let mut arena = Arena::new();
    /// let key = arena.alloc_str("port");
    /// let value = arena.alloc_value(ValueNode::Integer { value: 8080, position: Position::new(1, 8) });
    /// let prop = ObjectPropNode { key, value, position: Position::new(1, 1) };
    /// let list = arena.alloc_prop_list(&[prop]);
    ///
    /// assert_eq!(list.len(), 1);
    /// assert_eq!(arena.text(arena.prop_list(list)[0].key), "port");
    /// ```
    #[inline]
    pub fn alloc_prop_list(&mut self, props: &[ObjectPropNode]) -> PropList {
        let start = self.props.len();
        self.props.extend_from_slice(props);
        PropList::new(start, props.len())
    }

    /// Copies `ids` into the statement-list store and returns the run that names them.
    ///
    /// A block of statements is only known to be complete at its closing brace, so a parser
    /// collects the ids of the finished statements and stores the run once. Read the run back
    /// with [`stmt_list`](Self::stmt_list).
    ///
    /// # Panics
    ///
    /// Panics if the store would grow past what a `u32` offset can address.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, ExprNode, StmtNode, ValueNode};
    /// use dixscript::Compiler::AST::Position;
    ///
    /// let mut arena = Arena::new();
    /// let at = Position::new(1, 1);
    /// let value = arena.alloc_value(ValueNode::Null { position: at });
    /// let expr = arena.alloc_expr(ExprNode::Value { value, position: at });
    /// let a = arena.alloc_stmt(StmtNode::Return { value: expr, position: at });
    /// let b = arena.alloc_stmt(StmtNode::Log { value: expr, position: at });
    /// let list = arena.alloc_stmt_list(&[a, b]);
    ///
    /// assert_eq!(list.len(), 2);
    /// assert_eq!(arena.stmt_list(list), &[a, b]);
    /// ```
    #[inline]
    pub fn alloc_stmt_list(&mut self, ids: &[StmtId]) -> StmtList {
        let start = self.stmt_lists.len();
        self.stmt_lists.extend_from_slice(ids);
        StmtList::new(start, ids.len())
    }

    /// Copies each string of `items` into the text buffer and returns the run of spans that names
    /// them.
    ///
    /// Used for lists of names, such as the parameters of a lambda. Read the run back with
    /// [`str_list`](Self::str_list) and [`text`](Self::text).
    ///
    /// # Panics
    ///
    /// Panics if the text buffer or the list store would grow past what a `u32` offset can
    /// address.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::Arena;
    ///
    /// let mut arena = Arena::new();
    /// let list = arena.alloc_str_list(&["a", "b"]);
    ///
    /// let names: Vec<&str> = arena.str_list(list).iter().map(|s| arena.text(*s)).collect();
    /// assert_eq!(names, ["a", "b"]);
    /// ```
    pub fn alloc_str_list<S: AsRef<str>>(&mut self, items: &[S]) -> StrList {
        let start = self.str_lists.len();
        for item in items {
            let span = self.alloc_str(item.as_ref());
            self.str_lists.push(span);
        }
        StrList::new(start, items.len())
    }

    /// Stores one switch case on its own and returns its id.
    ///
    /// This is how the default (`miss`) case of a switch is stored: it is named by a [`CaseId`]
    /// in the statement, not by the run of ordinary cases made with
    /// [`alloc_case_list`](Self::alloc_case_list). Read it back with [`case`](Self::case).
    ///
    /// # Panics
    ///
    /// Panics if the store would grow past what a `u32` id can address.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, SwitchCaseNode, ValueNode};
    /// use dixscript::Compiler::AST::Position;
    ///
    /// let mut arena = Arena::new();
    /// let at = Position::new(1, 1);
    /// let case_value = arena.alloc_value(ValueNode::Null { position: at });
    /// let statements = arena.alloc_stmt_list(&[]);
    /// let id = arena.alloc_case(SwitchCaseNode { case_value, statements, position: at });
    ///
    /// assert!(arena.case(id).statements.is_empty());
    /// ```
    #[inline]
    pub fn alloc_case(&mut self, node: SwitchCaseNode) -> CaseId {
        let id = CaseId(to_u32(self.cases.len()));
        self.cases.push(node);
        id
    }

    /// Copies `cases` into the case store and returns the run that names them.
    ///
    /// The ordinary cases of a switch are stored as one run, in source order. Read the run back
    /// with [`case_list`](Self::case_list).
    ///
    /// # Panics
    ///
    /// Panics if the store would grow past what a `u32` offset can address.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, SwitchCaseNode, ValueNode};
    /// use dixscript::Compiler::AST::Position;
    ///
    /// let mut arena = Arena::new();
    /// let at = Position::new(1, 1);
    /// let statements = arena.alloc_stmt_list(&[]);
    /// let mut cases = Vec::new();
    /// for n in 1..=2 {
    ///     let case_value = arena.alloc_value(ValueNode::Integer { value: n, position: at });
    ///     cases.push(SwitchCaseNode { case_value, statements, position: at });
    /// }
    /// let list = arena.alloc_case_list(&cases);
    ///
    /// assert_eq!(list.len(), 2);
    /// assert_eq!(arena.case_list(list).len(), 2);
    /// ```
    #[inline]
    pub fn alloc_case_list(&mut self, cases: &[SwitchCaseNode]) -> CaseList {
        let start = self.cases.len();
        self.cases.extend_from_slice(cases);
        CaseList::new(start, cases.len())
    }

    // -- access -------------------------------------------------------------

    /// Returns the value node `id` names.
    ///
    /// # Panics
    ///
    /// Panics if `id` is out of range for this arena, for example an id from another arena.
    #[inline]
    pub fn value(&self, id: ValueId) -> &ValueNode {
        &self.values[id.index()]
    }

    /// Returns the expression node `id` names.
    ///
    /// # Panics
    ///
    /// Panics if `id` is out of range for this arena, for example an id from another arena.
    #[inline]
    pub fn expr(&self, id: ExprId) -> &ExprNode {
        &self.exprs[id.index()]
    }

    /// Returns the statement node `id` names.
    ///
    /// # Panics
    ///
    /// Panics if `id` is out of range for this arena, for example an id from another arena.
    #[inline]
    pub fn stmt(&self, id: StmtId) -> &StmtNode {
        &self.stmts[id.index()]
    }

    /// Returns the switch case `id` names. Switch cases are normally read through
    /// [`Arena::case_list`]; this is for the default case of a switch.
    ///
    /// # Panics
    ///
    /// Panics if `id` is out of range for this arena, for example an id from another arena.
    #[inline]
    pub fn case(&self, id: CaseId) -> &SwitchCaseNode {
        &self.cases[id.index()]
    }

    /// Returns the out-of-line record of a [`ValueNode::Unknown`].
    ///
    /// # Panics
    ///
    /// Panics if `id` is out of range for this arena, for example an id from another arena.
    #[inline]
    pub fn unknown(&self, id: UnknownId) -> &UnknownData {
        &self.unknowns[id.index()]
    }

    /// Returns the text a span names.
    ///
    /// # Panics
    ///
    /// Panics if the span is out of range for this arena, or does not fall on `char`
    /// boundaries of the text buffer. Both mean the span came from another arena.
    #[inline]
    pub fn text(&self, s: Str) -> &str {
        &self.text[s.range()]
    }

    /// Returns the ids a [`ValueList`] names.
    ///
    /// # Panics
    ///
    /// Panics if the list is out of range for this arena, for example a list from another
    /// arena.
    #[inline]
    pub fn value_list(&self, list: ValueList) -> &[ValueId] {
        &self.value_lists[list.range()]
    }

    /// Returns the ids an [`ExprList`] names.
    ///
    /// # Panics
    ///
    /// Panics if the list is out of range for this arena, for example a list from another
    /// arena.
    #[inline]
    pub fn expr_list(&self, list: ExprList) -> &[ExprId] {
        &self.expr_lists[list.range()]
    }

    /// Returns the ids a [`StmtList`] names.
    ///
    /// # Panics
    ///
    /// Panics if the list is out of range for this arena, for example a list from another
    /// arena.
    #[inline]
    pub fn stmt_list(&self, list: StmtList) -> &[StmtId] {
        &self.stmt_lists[list.range()]
    }

    /// Returns the spans a [`StrList`] names.
    ///
    /// # Panics
    ///
    /// Panics if the list is out of range for this arena, for example a list from another
    /// arena.
    #[inline]
    pub fn str_list(&self, list: StrList) -> &[Str] {
        &self.str_lists[list.range()]
    }

    /// Returns the properties a [`PropList`] names.
    ///
    /// # Panics
    ///
    /// Panics if the list is out of range for this arena, for example a list from another
    /// arena.
    #[inline]
    pub fn prop_list(&self, list: PropList) -> &[ObjectPropNode] {
        &self.props[list.range()]
    }

    /// Returns the switch cases a [`CaseList`] names.
    ///
    /// # Panics
    ///
    /// Panics if the list is out of range for this arena, for example a list from another
    /// arena.
    #[inline]
    pub fn case_list(&self, list: CaseList) -> &[SwitchCaseNode] {
        &self.cases[list.range()]
    }

    // -- size ---------------------------------------------------------------

    /// The number of value nodes in the arena.
    pub fn value_count(&self) -> usize {
        self.values.len()
    }

    /// The id of every value node, in allocation order.
    ///
    /// The arena is append-only, so this includes nodes that no section refers to any more, such
    /// as the ones a failed sub-expression left behind. It exists so that a test or a tool can
    /// visit every node without walking the tree.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, ValueNode};
    /// use dixscript::Compiler::AST::Position;
    ///
    /// let mut arena = Arena::new();
    /// let pos = Position::new(1, 1);
    /// arena.alloc_value(ValueNode::Null { position: pos });
    /// arena.alloc_value(ValueNode::Boolean { value: true, position: pos });
    ///
    /// let ids: Vec<_> = arena.value_ids().collect();
    /// assert_eq!(ids.len(), 2);
    /// assert!(matches!(arena.value(ids[1]), ValueNode::Boolean { value: true, .. }));
    /// ```
    pub fn value_ids(&self) -> impl Iterator<Item = ValueId> {
        (0..self.values.len()).map(|i| ValueId(to_u32(i)))
    }

    /// The id of every expression node, in allocation order. See [`value_ids`](Self::value_ids).
    pub fn expr_ids(&self) -> impl Iterator<Item = ExprId> {
        (0..self.exprs.len()).map(|i| ExprId(to_u32(i)))
    }

    /// The id of every statement node, in allocation order. See [`value_ids`](Self::value_ids).
    pub fn stmt_ids(&self) -> impl Iterator<Item = StmtId> {
        (0..self.stmts.len()).map(|i| StmtId(to_u32(i)))
    }

    /// The number of expression nodes in the arena.
    pub fn expr_count(&self) -> usize {
        self.exprs.len()
    }

    /// The number of statement nodes in the arena.
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
