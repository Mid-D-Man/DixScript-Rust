// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/AST/raw.rs"
// ============================================================================
use super::position::Position;
use super::values::Value;

/// A single `@RAW(...)` block. `Vec<RawBlock>` at the AST root, not
/// `Option<RawSection>` — see docs/dixscript/compiler.md for why. `content`
/// is `Option` rather than required; a missing one is a semantic error from
/// `raw_section_analyzer.rs`, not a parse failure.
#[derive(Debug, Clone, PartialEq)]
pub struct RawBlock {
    pub meta_data: Vec<RawField>,
    pub using: Vec<RawField>,
    pub content: Option<RawContent>,
    pub position: Position,
}

impl RawBlock {
    pub fn new(meta_data: Vec<RawField>, using: Vec<RawField>, content: Option<RawContent>, position: Position) -> Self {
        RawBlock { meta_data, using, content, position }
    }

    /// `meta_data.id` — required by the spec and checked for presence/
    /// uniqueness by `raw_section_analyzer.rs`, but AST construction itself
    /// doesn't enforce that, matching how every other section separates
    /// parsing from semantic validation.
    pub fn id(&self) -> Option<&str> {
        self.field_str(&self.meta_data, "id")
    }

    /// `meta_data.format` — the hint the embedding application dispatches
    /// on (`match raw.format.as_str() { "MPX" => ..., ... }`). DixScript
    /// itself never interprets this value.
    pub fn format(&self) -> Option<&str> {
        self.field_str(&self.meta_data, "format")
    }

    fn field_str<'a>(&'a self, fields: &'a [RawField], key: &str) -> Option<&'a str> {
        fields.iter().find(|f| f.key == key).and_then(|f| match &f.value {
            Value::String { value, .. } => Some(value.as_str()),
            _ => None,
        })
    }
}

impl std::fmt::Display for RawBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "@RAW(")?;
        writeln!(f, "  meta_data -> {{")?;
        for field in &self.meta_data {
            writeln!(f, "    {}", field)?;
        }
        writeln!(f, "  }}")?;
        writeln!(f, "  using -> {{")?;
        for field in &self.using {
            writeln!(f, "    {}", field)?;
        }
        writeln!(f, "  }}")?;
        match &self.content {
            Some(content) => writeln!(
                f, "  content -> {{ ---{}--- <{} bytes> ---{}--- }}",
                content.tag, content.byte_len(), content.tag,
            )?,
            None => writeln!(f, "  content -> {{ /* missing */ }}")?,
        }
        write!(f, ")")
    }
}

/// A single `key = value` entry inside `meta_data` or `using`. The AST
/// shape here is deliberately generic (any key, any `Value`) — it's
/// `raw_section_analyzer.rs` that knows which keys `meta_data`/`using`
/// actually recognize, same separation of concerns `SecurityField` uses
/// for `@SECURITY`.
#[derive(Debug, Clone, PartialEq)]
pub struct RawField {
    pub key: String,
    pub value: Value,
    pub position: Position,
}

impl RawField {
    pub fn new(key: String, value: Value, position: Position) -> Self {
        RawField { key, value, position }
    }
}

impl std::fmt::Display for RawField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} = {}", self.key, self.value)
    }
}

/// The `content -> { ---tag--- … ---tag--- }` block. `start`/`end` are byte
/// offsets into the source buffer the lexer scanned — never copied here,
/// never validated as UTF-8. Resolving them into actual bytes needs that
/// same buffer still alive, which is Runtime-layer plumbing, not something
/// this AST node owns.
#[derive(Debug, Clone, PartialEq)]
pub struct RawContent {
    pub tag: String,
    pub start: usize,
    pub end: usize,
    pub position: Position,
}

impl RawContent {
    pub fn new(tag: String, start: usize, end: usize, position: Position) -> Self {
        RawContent { tag, start, end, position }
    }

    #[inline]
    pub fn byte_len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}
