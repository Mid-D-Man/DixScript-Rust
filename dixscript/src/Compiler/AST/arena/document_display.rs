// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena/document_display.rs"
// ============================================================================
//! `Display` for [`Document`] and for the `Doc*` section twins, written to produce exactly the
//! text the owned `DixScript` and its section types produce. The wording is copied from
//! `root.rs`, `data.rs`, `raw.rs`, `security.rs`, `schema.rs` and `quickfuncs.rs`;
//! `tests/arena_document_display.rs` compares the two outputs on hand-built sections, on real
//! compiler output and on the whole corpus, so a drift in either shows up there.
//!
//! A [`Document`] owns its arena, so it implements `Display` directly. A `Doc*` section holds
//! only ids, which mean nothing without the arena they point into, so a bare `Doc*` value
//! cannot implement `Display`. [`Arena::display`] pairs one with its arena and returns an
//! [`ArenaDisplay`], which does. It plays the part for the section types that
//! [`ValueRef`](super::ValueRef) plays for values.
//!
//! Every `match` lists every variant with no wildcard arm.

use super::{
    Arena, DocData, DocDataEntry, DocPropertyAssignment, DocQuickFuncParam, DocQuickFuncs,
    DocQuickFunction, DocRawBlock, DocRawField, DocSchema, DocSchemaField, DocSecurity,
    DocSecurityEntry, DocSecurityField, Document, ValueList,
};
use crate::Compiler::AST::DataType;
use std::fmt::{self, Display, Formatter};

/// A `Doc*` item paired with the arena its ids point into, so that it can be printed.
///
/// Make one with [`Arena::display`]. `Display` is implemented for every `Doc*` section type
/// (`DocData`, `DocDataEntry`, `DocPropertyAssignment`, `DocRawBlock`, `DocRawField`,
/// `DocSecurity`, `DocSecurityEntry`, `DocSecurityField`, `DocSchema`, `DocSchemaField`,
/// `DocQuickFuncs`, `DocQuickFunction` and `DocQuickFuncParam`), and each prints exactly what
/// the owned type it mirrors prints.
///
/// The handle is `Copy` and costs two words.
pub struct ArenaDisplay<'a, T> {
    arena: &'a Arena,
    item: &'a T,
}

impl<T> Clone for ArenaDisplay<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ArenaDisplay<'_, T> {}

impl Arena {
    /// Pairs a `Doc*` item with this arena so that it can be printed with `{}`.
    ///
    /// The result prints exactly what the owned type the item mirrors prints. `item` must have
    /// been built in this arena; if it was not, printing it reads the wrong nodes or panics.
    /// A type that has no `Display` here (anything other than the `Doc*` section types) gives a
    /// compile error where it is printed.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, DocData};
    /// use dixscript::Compiler::AST::{DataEntry, DataSection, Position, Value};
    ///
    /// let owned = DataSection::new(
    ///     vec![DataEntry::SimpleProperty {
    ///         name: "port".to_string(),
    ///         data_type: None,
    ///         value: Value::Integer { value: 8080, position: Position::new(2, 8) },
    ///         position: Position::new(2, 1),
    ///     }],
    ///     Position::new(1, 1),
    /// );
    ///
    /// let mut arena = Arena::new();
    /// let data = DocData::from_owned(&mut arena, &owned);
    ///
    /// assert_eq!(arena.display(&data).to_string(), owned.to_string());
    /// assert_eq!(arena.display(&data).to_string(), "@DATA(\n  port = 8080\n)");
    /// ```
    #[inline]
    pub fn display<'a, T>(&'a self, item: &'a T) -> ArenaDisplay<'a, T> {
        ArenaDisplay { arena: self, item }
    }
}

/// Writes `<type>` when there is a data type, and nothing when there is not.
fn data_type(f: &mut Formatter<'_>, dt: &Option<DataType>) -> fmt::Result {
    if let Some(dt) = dt {
        write!(f, "<{}>", dt)?;
    }
    Ok(())
}

/// Writes the values of `list` separated by `", "`.
fn comma_values(a: &Arena, f: &mut Formatter<'_>, list: ValueList) -> fmt::Result {
    for (i, id) in a.value_list(list).iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{}", a.value_ref(*id))?;
    }
    Ok(())
}

// -- @DATA --------------------------------------------------------------------

impl Display for ArenaDisplay<'_, DocData> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let (a, section) = (self.arena, self.item);
        writeln!(f, "@DATA(")?;
        for (i, entry) in section.entries.iter().enumerate() {
            write!(f, "  {}", a.display(entry))?;
            if i < section.entries.len() - 1 {
                writeln!(f, ",")?;
            } else {
                writeln!(f)?;
            }
        }
        write!(f, ")")
    }
}

impl Display for ArenaDisplay<'_, DocDataEntry> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let a = self.arena;
        match self.item {
            DocDataEntry::SimpleProperty { name, data_type: dt, value, .. } => {
                write!(f, "{}", name)?;
                data_type(f, dt)?;
                write!(f, " = {}", a.value_ref(*value))
            }
            DocDataEntry::TableProperty { path, properties, .. } => {
                write!(f, "{}: ", path)?;
                for (i, prop) in properties.iter().enumerate() {
                    write!(f, "{}", a.display(prop))?;
                    if i < properties.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                Ok(())
            }
            DocDataEntry::GroupArray { path, items, .. } => {
                write!(f, "{}:: ", path)?;
                comma_values(a, f, *items)
            }
            DocDataEntry::ObjectProperty { name, data_type: dt, object, .. } => {
                write!(f, "{}", name)?;
                data_type(f, dt)?;
                write!(f, " = {}", a.value_ref(*object))
            }
        }
    }
}

impl Display for ArenaDisplay<'_, DocPropertyAssignment> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let item = self.item;
        write!(f, "{}", item.name)?;
        data_type(f, &item.data_type)?;
        write!(f, " = {}", self.arena.value_ref(item.value))
    }
}

// -- @RAW ---------------------------------------------------------------------

impl Display for ArenaDisplay<'_, DocRawBlock> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let (a, block) = (self.arena, self.item);
        writeln!(f, "@RAW(")?;
        writeln!(f, "  meta_data -> {{")?;
        for field in &block.meta_data {
            writeln!(f, "    {}", a.display(field))?;
        }
        writeln!(f, "  }}")?;
        writeln!(f, "  using -> {{")?;
        for field in &block.using {
            writeln!(f, "    {}", a.display(field))?;
        }
        writeln!(f, "  }}")?;
        match &block.content {
            Some(content) => writeln!(
                f,
                "  content -> {{ ---{}--- <{} bytes> ---{}--- }}",
                content.tag,
                content.byte_len(),
                content.tag,
            )?,
            None => writeln!(f, "  content -> {{ /* missing */ }}")?,
        }
        write!(f, ")")
    }
}

impl Display for ArenaDisplay<'_, DocRawField> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} = {}", self.item.key, self.arena.value_ref(self.item.value))
    }
}

// -- @SECURITY ----------------------------------------------------------------

impl Display for ArenaDisplay<'_, DocSecurity> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let (a, section) = (self.arena, self.item);
        writeln!(f, "@SECURITY(")?;
        for (i, entry) in section.entries.iter().enumerate() {
            writeln!(f, "  {} -> {{", entry.block_key)?;
            for (j, field) in entry.fields.iter().enumerate() {
                write!(f, "    {} = {}", field.key, a.value_ref(field.value))?;
                if j < entry.fields.len() - 1 {
                    writeln!(f, ",")?;
                } else {
                    writeln!(f)?;
                }
            }
            write!(f, "  }}")?;
            if i < section.entries.len() - 1 {
                writeln!(f, ",")?;
            } else {
                writeln!(f)?;
            }
        }
        write!(f, ")")
    }
}

impl Display for ArenaDisplay<'_, DocSecurityEntry> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let (a, entry) = (self.arena, self.item);
        write!(f, "{} -> {{ ", entry.block_key)?;
        for (i, field) in entry.fields.iter().enumerate() {
            write!(f, "{}", a.display(field))?;
            if i < entry.fields.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, " }}")
    }
}

impl Display for ArenaDisplay<'_, DocSecurityField> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} = {}", self.item.key, self.arena.value_ref(self.item.value))
    }
}

// -- @SCHEMA ------------------------------------------------------------------

impl Display for ArenaDisplay<'_, DocSchema> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "@SCHEMA(")?;
        for field in &self.item.fields {
            writeln!(f, "  {}", self.arena.display(field))?;
        }
        write!(f, ")")
    }
}

impl Display for ArenaDisplay<'_, DocSchemaField> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let item = self.item;
        write!(f, "{} = Schema.{}(", item.path, item.method)?;
        comma_values(self.arena, f, item.arguments)?;
        write!(f, ")")
    }
}

// -- @QUICKFUNCS --------------------------------------------------------------

impl Display for ArenaDisplay<'_, DocQuickFuncs> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let (a, section) = (self.arena, self.item);
        writeln!(f, "@QUICKFUNCS(")?;
        for (i, func) in section.functions.iter().enumerate() {
            write!(f, "  ~{}", func.name)?;
            if let Some(ref rt) = func.return_type {
                write!(f, "<{}>", rt)?;
            }
            if let Some(ref scope) = func.scope_list {
                write!(f, " => {}", scope.join(","))?;
            }
            write!(f, "(")?;
            for (j, param) in func.parameters.iter().enumerate() {
                write!(f, "{}", a.display(param))?;
                if j < func.parameters.len() - 1 {
                    write!(f, ", ")?;
                }
            }
            writeln!(f, ") {{")?;
            for stmt in a.stmt_list(func.body) {
                writeln!(f, "    {}", a.stmt_ref(*stmt))?;
            }
            write!(f, "  }}")?;
            if i < section.functions.len() - 1 {
                writeln!(f, ",")?;
            } else {
                writeln!(f)?;
            }
        }
        write!(f, ")")
    }
}

impl Display for ArenaDisplay<'_, DocQuickFunction> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let (a, func) = (self.arena, self.item);
        write!(f, "~{}", func.name)?;
        if let Some(ref rt) = func.return_type {
            write!(f, "<{}>", rt)?;
        }
        if let Some(ref scope) = func.scope_list {
            write!(f, " => {}", scope.join(","))?;
        }
        write!(f, "(")?;
        for (i, param) in func.parameters.iter().enumerate() {
            write!(f, "{}", a.display(param))?;
            if i < func.parameters.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, ") {{ ")?;
        for stmt in a.stmt_list(func.body) {
            write!(f, "{}; ", a.stmt_ref(*stmt))?;
        }
        write!(f, "}}")
    }
}

impl Display for ArenaDisplay<'_, DocQuickFuncParam> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let item = self.item;
        write!(f, "{}", item.name)?;
        data_type(f, &item.data_type)?;
        if let Some(default) = item.default_value {
            write!(f, " = {}", self.arena.expr_ref(default))?;
        }
        Ok(())
    }
}

// -- the document -------------------------------------------------------------

/// Prints the whole file exactly as the owned `DixScript` prints it: the sections in the same
/// order, each followed by a blank line, with the same quirk that `@SECURITY` is not followed
/// by one.
///
/// # Examples
///
/// ```
/// use dixscript::Compiler::AST::arena::Document;
/// use dixscript::Compiler::AST::{DataEntry, DataSection, DixScript, Position, Value};
///
/// let mut ds = DixScript::new();
/// ds.data = Some(DataSection::new(
///     vec![DataEntry::SimpleProperty {
///         name: "port".to_string(),
///         data_type: None,
///         value: Value::Integer { value: 8080, position: Position::new(2, 8) },
///         position: Position::new(2, 1),
///     }],
///     Position::new(1, 1),
/// ));
///
/// let document = Document::from_owned(&ds);
/// assert_eq!(document.to_string(), ds.to_string());
/// assert_eq!(document.to_string(), "@DATA(\n  port = 8080\n)\n\n");
/// ```
impl Display for Document {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let a = &self.arena;

        if let Some(ref config) = self.config {
            writeln!(f, "{}", config)?;
            writeln!(f)?;
        }

        if let Some(ref imports) = self.imports {
            writeln!(f, "{}", imports)?;
            writeln!(f)?;
        }

        if let Some(ref dlm) = self.dlm {
            writeln!(f, "{}", dlm)?;
            writeln!(f)?;
        }

        if let Some(ref enums) = self.enums {
            writeln!(f, "{}", enums)?;
            writeln!(f)?;
        }

        if let Some(ref quick_funcs) = self.quick_functions {
            writeln!(f, "{}", a.display(quick_funcs))?;
            writeln!(f)?;
        }

        if let Some(ref data) = self.data {
            writeln!(f, "{}", a.display(data))?;
            writeln!(f)?;
        }

        if let Some(ref security) = self.security {
            writeln!(f, "{}", a.display(security))?;
        }

        for raw_block in &self.raw {
            writeln!(f, "{}", a.display(raw_block))?;
            writeln!(f)?;
        }

        if let Some(ref schema) = self.schema {
            writeln!(f, "{}", a.display(schema))?;
            writeln!(f)?;
        }

        Ok(())
    }
}
