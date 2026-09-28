// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/AST/schema.rs"
// ============================================================================
use super::data::TablePath;
use super::position::Position;
use super::values::Value;

/// The `@SCHEMA(...)` section: a set of field descriptors that constrain
/// the shape of `@DATA`.
///
/// `Option<SchemaBlock>` at the AST root (a singleton, like `@SECURITY` /
/// `@DLM`), not a `Vec` like `@RAW` — a file has one schema describing its
/// one `@DATA` section, so there is nothing to keep independent the way
/// `@RAW`'s per-asset blocks are.
///
/// The parser stores each descriptor as written — the `Schema.<Method>`
/// name plus its literal arguments — and does NOT evaluate it. Building the
/// canonical descriptor Object (`type` / `required` / constraint keys) is
/// `schema_section_analyzer.rs`'s job, by calling the `Schema` builtin
/// static object, so the builtin remains the single source of truth for
/// which methods exist and what arguments each accepts.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaBlock {
    pub fields: Vec<SchemaField>,
    pub position: Position,
}

impl SchemaBlock {
    pub fn new(fields: Vec<SchemaField>, position: Position) -> Self {
        SchemaBlock { fields, position }
    }
}

impl std::fmt::Display for SchemaBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "@SCHEMA(")?;
        for field in &self.fields {
            writeln!(f, "  {}", field)?;
        }
        write!(f, ")")
    }
}

/// One `path = Schema.<Method>(args...)` entry.
///
/// `path` uses the same `TablePath` type `@DATA`'s own dotted addressing
/// uses, so `elements.hydrogen.identity` here names the same location
/// `elements.hydrogen: identity = "H"` defines in `@DATA`. The last
/// segment is the property name; everything before it is the table /
/// object it lives in. A single-segment path names a top-level `@DATA`
/// property.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaField {
    pub path: TablePath,
    /// The method name exactly as written after `Schema.` (`"Int"`,
    /// `"String"`, ...). Validity is checked semantically, not here.
    pub method: String,
    /// Literal arguments as written: `required` first, then any
    /// type-specific constraints.
    pub arguments: Vec<Value>,
    pub position: Position,
}

impl SchemaField {
    pub fn new(path: TablePath, method: String, arguments: Vec<Value>, position: Position) -> Self {
        SchemaField { path, method, arguments, position }
    }

    /// Dotted form of `path`, for diagnostics.
    pub fn path_string(&self) -> String {
        self.path.to_string()
    }
}

impl std::fmt::Display for SchemaField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} = Schema.{}(", self.path, self.method)?;
        for (i, arg) in self.arguments.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", arg)?;
        }
        write!(f, ")")
    }
}
