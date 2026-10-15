// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena"
// ============================================================================
//! The arena-resident form of a whole `DixScript`.
//!
//! A [`Document`] owns one [`Arena`] and the sections of a file. Four sections hold nothing
//! from the `Value` family (`@CONFIG`, `@IMPORTS`, `@DLM`, `@ENUMS`), so the document reuses
//! the owned types for them unchanged. The five that do (`@DATA`, `@RAW`, `@SECURITY`,
//! `@SCHEMA`, `@QUICKFUNCS`) have twins here, named `Doc*`, that hold ids and spans into the
//! arena where the owned type holds a `Value`, `Expression` or statement. Everything else in
//! them (names, paths, positions, data types) is carried over as it is.
//!
//! [`Document::from_owned`] and [`Document::export`] convert in both directions and are exact:
//! `Document::from_owned(&ds).export() == ds`. The document has no lifetime, so it is an
//! ordinary `'static`, `Send + Sync`, `Clone` value, which is what lets `mdix-lsp` keep one per
//! open file.
//!
//! The `Doc*` types do not implement `PartialEq`, because their ids mean nothing without the
//! arena. [`Document`] implements it structurally, across arenas.

use super::{Arena, ExprId, StmtList, ValueId, ValueList};
use crate::Compiler::AST::{
    DataEntry, DataSection, DataType, DixScript, Position, PropertyAssignment, QuickFuncParam,
    QuickFuncsSection, QuickFunction, RawBlock, RawContent, RawField, SchemaBlock, SchemaField,
    SecurityEntry, SecurityField, SecuritySection, TablePath,
};
use crate::Compiler::AST::config::ConfigSection;
use crate::Compiler::AST::dlm::DLMSection;
use crate::Compiler::AST::enums::EnumsSection;
use crate::Compiler::AST::imports::ImportsSection;

// -- @DATA --------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DocData {
    pub entries: Vec<DocDataEntry>,
    pub position: Position,
}

#[derive(Debug, Clone)]
pub enum DocDataEntry {
    SimpleProperty { name: String, data_type: Option<DataType>, value: ValueId, position: Position },
    TableProperty { path: TablePath, properties: Vec<DocPropertyAssignment>, position: Position },
    GroupArray { path: TablePath, items: ValueList, position: Position },
    ObjectProperty { name: String, data_type: Option<DataType>, object: ValueId, position: Position },
}

#[derive(Debug, Clone)]
pub struct DocPropertyAssignment {
    pub name: String,
    pub data_type: Option<DataType>,
    pub value: ValueId,
    pub position: Position,
}

// -- @RAW ---------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DocRawBlock {
    pub meta_data: Vec<DocRawField>,
    pub using: Vec<DocRawField>,
    pub content: Option<RawContent>,
    pub position: Position,
}

#[derive(Debug, Clone)]
pub struct DocRawField {
    pub key: String,
    pub value: ValueId,
    pub position: Position,
}

// -- @SECURITY ----------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DocSecurity {
    pub entries: Vec<DocSecurityEntry>,
    pub position: Position,
}

#[derive(Debug, Clone)]
pub struct DocSecurityEntry {
    pub block_key: String,
    pub fields: Vec<DocSecurityField>,
    pub position: Position,
}

#[derive(Debug, Clone)]
pub struct DocSecurityField {
    pub key: String,
    pub value: ValueId,
    pub position: Position,
}

// -- @SCHEMA ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DocSchema {
    pub fields: Vec<DocSchemaField>,
    pub position: Position,
}

#[derive(Debug, Clone)]
pub struct DocSchemaField {
    pub path: TablePath,
    pub method: String,
    pub arguments: ValueList,
    pub position: Position,
}

// -- @QUICKFUNCS --------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DocQuickFuncs {
    pub functions: Vec<DocQuickFunction>,
    pub position: Position,
}

#[derive(Debug, Clone)]
pub struct DocQuickFunction {
    pub name: String,
    pub return_type: Option<DataType>,
    pub scope_list: Option<Vec<String>>,
    pub parameters: Vec<DocQuickFuncParam>,
    pub body: StmtList,
    pub position: Position,
}

#[derive(Debug, Clone)]
pub struct DocQuickFuncParam {
    pub name: String,
    pub data_type: Option<DataType>,
    pub default_value: Option<ExprId>,
    pub position: Position,
}

// -- the document -------------------------------------------------------------

/// One arena plus the sections of a file. See the module documentation.
#[derive(Debug, Clone, Default)]
pub struct Document {
    pub arena: Arena,
    pub config: Option<ConfigSection>,
    pub imports: Option<ImportsSection>,
    pub dlm: Option<DLMSection>,
    pub enums: Option<EnumsSection>,
    pub quick_functions: Option<DocQuickFuncs>,
    pub data: Option<DocData>,
    pub security: Option<DocSecurity>,
    pub raw: Vec<DocRawBlock>,
    pub schema: Option<DocSchema>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// Copies an owned `DixScript` into a new document. The sections that hold no values are
    /// cloned; the others are imported into the document's arena.
    pub fn from_owned(ds: &DixScript) -> Document {
        let mut arena = Arena::new();
        let data = ds.data.as_ref().map(|s| import_data(&mut arena, s));
        let raw = ds.raw.iter().map(|b| import_raw(&mut arena, b)).collect();
        let security = ds.security.as_ref().map(|s| import_security(&mut arena, s));
        let schema = ds.schema.as_ref().map(|s| import_schema(&mut arena, s));
        let quick_functions = ds.quick_functions.as_ref().map(|s| import_quickfuncs(&mut arena, s));
        Document {
            arena,
            config: ds.config.clone(),
            imports: ds.imports.clone(),
            dlm: ds.dlm.clone(),
            enums: ds.enums.clone(),
            quick_functions,
            data,
            security,
            raw,
            schema,
        }
    }

    /// Copies the document back out as the owned `DixScript` it was made from.
    pub fn export(&self) -> DixScript {
        let a = &self.arena;
        DixScript {
            config: self.config.clone(),
            imports: self.imports.clone(),
            dlm: self.dlm.clone(),
            enums: self.enums.clone(),
            quick_functions: self.quick_functions.as_ref().map(|s| export_quickfuncs(a, s)),
            data: self.data.as_ref().map(|s| export_data(a, s)),
            security: self.security.as_ref().map(|s| export_security(a, s)),
            raw: self.raw.iter().map(|b| export_raw(a, b)).collect(),
            schema: self.schema.as_ref().map(|s| export_schema(a, s)),
        }
    }
}

// -- import -------------------------------------------------------------------

fn import_data(arena: &mut Arena, src: &DataSection) -> DocData {
    let entries = src
        .entries
        .iter()
        .map(|entry| match entry {
            DataEntry::SimpleProperty { name, data_type, value, position } => DocDataEntry::SimpleProperty {
                name: name.clone(),
                data_type: *data_type,
                value: arena.import_value(value),
                position: *position,
            },
            DataEntry::TableProperty { path, properties, position } => DocDataEntry::TableProperty {
                path: path.clone(),
                properties: properties
                    .iter()
                    .map(|p| DocPropertyAssignment {
                        name: p.name.clone(),
                        data_type: p.data_type,
                        value: arena.import_value(&p.value),
                        position: p.position,
                    })
                    .collect(),
                position: *position,
            },
            DataEntry::GroupArray { path, items, position } => DocDataEntry::GroupArray {
                path: path.clone(),
                items: arena.import_value_list(items),
                position: *position,
            },
            DataEntry::ObjectProperty { name, data_type, object, position } => DocDataEntry::ObjectProperty {
                name: name.clone(),
                data_type: *data_type,
                object: arena.import_value(object),
                position: *position,
            },
        })
        .collect();
    DocData { entries, position: src.position }
}

fn import_raw_field(arena: &mut Arena, f: &RawField) -> DocRawField {
    DocRawField { key: f.key.clone(), value: arena.import_value(&f.value), position: f.position }
}

fn import_raw(arena: &mut Arena, src: &RawBlock) -> DocRawBlock {
    DocRawBlock {
        meta_data: src.meta_data.iter().map(|f| import_raw_field(arena, f)).collect(),
        using: src.using.iter().map(|f| import_raw_field(arena, f)).collect(),
        content: src.content.clone(),
        position: src.position,
    }
}

fn import_security(arena: &mut Arena, src: &SecuritySection) -> DocSecurity {
    DocSecurity {
        entries: src
            .entries
            .iter()
            .map(|e| DocSecurityEntry {
                block_key: e.block_key.clone(),
                fields: e
                    .fields
                    .iter()
                    .map(|f| DocSecurityField {
                        key: f.key.clone(),
                        value: arena.import_value(&f.value),
                        position: f.position,
                    })
                    .collect(),
                position: e.position,
            })
            .collect(),
        position: src.position,
    }
}

fn import_schema(arena: &mut Arena, src: &SchemaBlock) -> DocSchema {
    DocSchema {
        fields: src
            .fields
            .iter()
            .map(|f| DocSchemaField {
                path: f.path.clone(),
                method: f.method.clone(),
                arguments: arena.import_value_list(&f.arguments),
                position: f.position,
            })
            .collect(),
        position: src.position,
    }
}

fn import_quickfuncs(arena: &mut Arena, src: &QuickFuncsSection) -> DocQuickFuncs {
    DocQuickFuncs {
        functions: src
            .functions
            .iter()
            .map(|func| DocQuickFunction {
                name: func.name.clone(),
                return_type: func.return_type,
                scope_list: func.scope_list.clone(),
                parameters: func
                    .parameters
                    .iter()
                    .map(|p| DocQuickFuncParam {
                        name: p.name.clone(),
                        data_type: p.data_type,
                        default_value: p.default_value.as_ref().map(|e| arena.import_expr(e)),
                        position: p.position,
                    })
                    .collect(),
                body: arena.import_stmt_list(&func.body),
                position: func.position,
            })
            .collect(),
        position: src.position,
    }
}

// -- export -------------------------------------------------------------------

fn export_data(a: &Arena, src: &DocData) -> DataSection {
    let entries = src
        .entries
        .iter()
        .map(|entry| match entry {
            DocDataEntry::SimpleProperty { name, data_type, value, position } => DataEntry::SimpleProperty {
                name: name.clone(),
                data_type: *data_type,
                value: a.export_value(*value),
                position: *position,
            },
            DocDataEntry::TableProperty { path, properties, position } => DataEntry::TableProperty {
                path: path.clone(),
                properties: properties
                    .iter()
                    .map(|p| PropertyAssignment {
                        name: p.name.clone(),
                        data_type: p.data_type,
                        value: a.export_value(p.value),
                        position: p.position,
                    })
                    .collect(),
                position: *position,
            },
            DocDataEntry::GroupArray { path, items, position } => DataEntry::GroupArray {
                path: path.clone(),
                items: a.value_list(*items).iter().map(|id| a.export_value(*id)).collect(),
                position: *position,
            },
            DocDataEntry::ObjectProperty { name, data_type, object, position } => DataEntry::ObjectProperty {
                name: name.clone(),
                data_type: *data_type,
                object: Box::new(a.export_value(*object)),
                position: *position,
            },
        })
        .collect();
    DataSection { entries, position: src.position }
}

fn export_raw_field(a: &Arena, f: &DocRawField) -> RawField {
    RawField { key: f.key.clone(), value: a.export_value(f.value), position: f.position }
}

fn export_raw(a: &Arena, src: &DocRawBlock) -> RawBlock {
    RawBlock {
        meta_data: src.meta_data.iter().map(|f| export_raw_field(a, f)).collect(),
        using: src.using.iter().map(|f| export_raw_field(a, f)).collect(),
        content: src.content.clone(),
        position: src.position,
    }
}

fn export_security(a: &Arena, src: &DocSecurity) -> SecuritySection {
    SecuritySection {
        entries: src
            .entries
            .iter()
            .map(|e| SecurityEntry {
                block_key: e.block_key.clone(),
                fields: e
                    .fields
                    .iter()
                    .map(|f| SecurityField { key: f.key.clone(), value: a.export_value(f.value), position: f.position })
                    .collect(),
                position: e.position,
            })
            .collect(),
        position: src.position,
    }
}

fn export_schema(a: &Arena, src: &DocSchema) -> SchemaBlock {
    SchemaBlock {
        fields: src
            .fields
            .iter()
            .map(|f| SchemaField {
                path: f.path.clone(),
                method: f.method.clone(),
                arguments: a.value_list(f.arguments).iter().map(|id| a.export_value(*id)).collect(),
                position: f.position,
            })
            .collect(),
        position: src.position,
    }
}

fn export_quickfuncs(a: &Arena, src: &DocQuickFuncs) -> QuickFuncsSection {
    QuickFuncsSection {
        functions: src
            .functions
            .iter()
            .map(|func| QuickFunction {
                name: func.name.clone(),
                return_type: func.return_type,
                scope_list: func.scope_list.clone(),
                parameters: func
                    .parameters
                    .iter()
                    .map(|p| QuickFuncParam {
                        name: p.name.clone(),
                        data_type: p.data_type,
                        default_value: p.default_value.map(|id| a.export_expr(id)),
                        position: p.position,
                    })
                    .collect(),
                body: a.stmt_list(func.body).iter().map(|id| a.export_stmt(*id)).collect(),
                position: func.position,
            })
            .collect(),
        position: src.position,
    }
}

// -- equality -----------------------------------------------------------------

fn list_eq<T>(l: &[T], r: &[T], mut eq: impl FnMut(&T, &T) -> bool) -> bool {
    l.len() == r.len() && l.iter().zip(r).all(|(x, y)| eq(x, y))
}

fn val_eq(a: &Arena, x: ValueId, b: &Arena, y: ValueId) -> bool {
    a.value_ref(x) == b.value_ref(y)
}

fn vals_eq(a: &Arena, x: ValueList, b: &Arena, y: ValueList) -> bool {
    list_eq(a.value_list(x), b.value_list(y), |s, t| val_eq(a, *s, b, *t))
}

fn raw_field_eq(a: &Arena, x: &DocRawField, b: &Arena, y: &DocRawField) -> bool {
    x.key == y.key && x.position == y.position && val_eq(a, x.value, b, y.value)
}

fn data_entry_eq(a: &Arena, x: &DocDataEntry, b: &Arena, y: &DocDataEntry) -> bool {
    match x {
        DocDataEntry::SimpleProperty { name, data_type, value, position } => matches!(
            y,
            DocDataEntry::SimpleProperty { name: n, data_type: t, value: v, position: p }
                if n == name && t == data_type && p == position && val_eq(a, *value, b, *v)
        ),
        DocDataEntry::TableProperty { path, properties, position } => matches!(
            y,
            DocDataEntry::TableProperty { path: q, properties: props, position: p }
                if q == path
                    && p == position
                    && list_eq(properties, props, |s, t| {
                        s.name == t.name
                            && s.data_type == t.data_type
                            && s.position == t.position
                            && val_eq(a, s.value, b, t.value)
                    })
        ),
        DocDataEntry::GroupArray { path, items, position } => matches!(
            y,
            DocDataEntry::GroupArray { path: q, items: i, position: p }
                if q == path && p == position && vals_eq(a, *items, b, *i)
        ),
        DocDataEntry::ObjectProperty { name, data_type, object, position } => matches!(
            y,
            DocDataEntry::ObjectProperty { name: n, data_type: t, object: o, position: p }
                if n == name && t == data_type && p == position && val_eq(a, *object, b, *o)
        ),
    }
}

impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (&self.arena, &other.arena);

        if self.config != other.config
            || self.imports != other.imports
            || self.dlm != other.dlm
            || self.enums != other.enums
        {
            return false;
        }

        let data = match (&self.data, &other.data) {
            (Some(x), Some(y)) => {
                x.position == y.position && list_eq(&x.entries, &y.entries, |s, t| data_entry_eq(a, s, b, t))
            }
            (None, None) => true,
            _ => false,
        };

        let raw = list_eq(&self.raw, &other.raw, |x, y| {
            x.position == y.position
                && x.content == y.content
                && list_eq(&x.meta_data, &y.meta_data, |s, t| raw_field_eq(a, s, b, t))
                && list_eq(&x.using, &y.using, |s, t| raw_field_eq(a, s, b, t))
        });

        let security = match (&self.security, &other.security) {
            (Some(x), Some(y)) => {
                x.position == y.position
                    && list_eq(&x.entries, &y.entries, |s, t| {
                        s.block_key == t.block_key
                            && s.position == t.position
                            && list_eq(&s.fields, &t.fields, |f, g| {
                                f.key == g.key && f.position == g.position && val_eq(a, f.value, b, g.value)
                            })
                    })
            }
            (None, None) => true,
            _ => false,
        };

        let schema = match (&self.schema, &other.schema) {
            (Some(x), Some(y)) => {
                x.position == y.position
                    && list_eq(&x.fields, &y.fields, |s, t| {
                        s.path == t.path
                            && s.method == t.method
                            && s.position == t.position
                            && vals_eq(a, s.arguments, b, t.arguments)
                    })
            }
            (None, None) => true,
            _ => false,
        };

        let quick = match (&self.quick_functions, &other.quick_functions) {
            (Some(x), Some(y)) => {
                x.position == y.position
                    && list_eq(&x.functions, &y.functions, |s, t| {
                        s.name == t.name
                            && s.return_type == t.return_type
                            && s.scope_list == t.scope_list
                            && s.position == t.position
                            && list_eq(&s.parameters, &t.parameters, |p, q| {
                                p.name == q.name
                                    && p.data_type == q.data_type
                                    && p.position == q.position
                                    && match (p.default_value, q.default_value) {
                                        (Some(m), Some(n)) => a.expr_ref(m) == b.expr_ref(n),
                                        (None, None) => true,
                                        _ => false,
                                    }
                            })
                            && list_eq(a.stmt_list(s.body), b.stmt_list(t.body), |m, n| {
                                a.stmt_ref(*m) == b.stmt_ref(*n)
                            })
                    })
            }
            (None, None) => true,
            _ => false,
        };

        data && raw && security && schema && quick
    }
}
