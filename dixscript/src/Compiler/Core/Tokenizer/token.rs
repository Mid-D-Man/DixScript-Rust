use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SectionId {
    #[default]
    None,
    Config,
    Imports,
    Dlm,
    Enums,
    QuickFuncs,
    Data,
    Security,
}

impl SectionId {
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            SectionId::None       => "",
            SectionId::Config     => "CONFIG",
            SectionId::Imports    => "IMPORTS",
            SectionId::Dlm        => "DLM",
            SectionId::Enums      => "ENUMS",
            SectionId::QuickFuncs => "QUICKFUNCS",
            SectionId::Data       => "DATA",
            SectionId::Security   => "SECURITY",
        }
    }

    #[inline]
    pub const fn to_option(self) -> Option<&'static str> {
        match self {
            SectionId::None => None,
            other           => Some(other.as_str()),
        }
    }

    #[inline] pub const fn is_some(self) -> bool { !matches!(self, SectionId::None) }
    #[inline] pub const fn is_none(self) -> bool {  matches!(self, SectionId::None) }

    #[inline]
    pub fn from_context_str(s: &str) -> Self {
        match s {
            "CONFIG"     => SectionId::Config,
            "IMPORTS"    => SectionId::Imports,
            "DLM"        => SectionId::Dlm,
            "ENUMS"      => SectionId::Enums,
            "QUICKFUNCS" => SectionId::QuickFuncs,
            "DATA"       => SectionId::Data,
            "SECURITY"   => SectionId::Security,
            _            => SectionId::None,
        }
    }
}

impl fmt::Display for SectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every variant in this enum is actually emitted by the lexer.
/// No dead/speculative variants are kept here — if the lexer does not produce
/// it, it does not belong in this enum.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // ── Keywords ─────────────────────────────────────────────────────────────
    /// Reserved word matched by the KEYWORDS phf map.
    /// e.g. `if`, `return`, `from_cloud`, `true` (Bool), etc.
    /// Note: `true`/`false` produce `Bool(bool)`, not `Keyword`.
    Keyword(&'static str),

    // ── Operators (all emitted by try_scan_multi_char_operator / scan_single_character) ─
    ArithmeticOp(&'static str),        // +  -  *  /  %  **  %%  %&  &%  ++  --
    ArithmeticAssignOp(&'static str),  // +=  -=  *=  /=  %=  **=
    ComparisonOp(&'static str),        // ==  !=  <=  >=
    LogicalOp(&'static str),           // &&  ||
    BitwiseOp(&'static str),           // ^  &  |  <<  >>  >_<  ~?  &=  |=  ^=  <<=  >>=

    // ── Literals ──────────────────────────────────────────────────────────────
    Identifier(String),
    Integer(i32),
    /// 64-bit integer literal.
    /// Emitted for: `L`/`l`-suffixed decimals, hex, or binary; plain decimals
    /// that overflow i32; hex/binary that overflow i32 (auto-promoted).
    Long(i64),
    Float(f32),          // decimal with `.` + `f`/`F` suffix, or integer + `f`/`F`
    Double(f64),         // decimal with `.` and no suffix
    ScientificNotation(f64),  // decimal with `e`/`E` exponent, no `f` suffix
    String(String),           // "…"
    StringSingle(String),     // '…'
    InterpolatedString(String), // $"…" or $'…'  (only inside advanced sections)
    Bool(bool),               // keyword `true` / `false`

    // ── Single-character symbols ──────────────────────────────────────────────
    /// Any character that doesn't map to a more specific variant.
    /// Includes: `(`, `)`, `[`, `]`, `{`, `}`, `,`, `;`, `.`, `_`, `@`
    /// (when not a section keyword), `:`, `<`, `>`, `=`, `!`, etc.
    Symbol(char),

    // ── Special literal forms ─────────────────────────────────────────────────
    HexColor(String),    // #RRGGBB / #RGB — emitted by scan_hex_color
    /// Date literal: YYYY-MM-DD  (no time component)
    Date(String),
    /// Timestamp literal: YYYY-MM-DDThh:mm:ss[.fff][Z|±hh:mm]
    Timestamp(String),

    // ── Structural punctuation ────────────────────────────────────────────────
    /// `::`  — double-colon (group-array separator / qualified name)
    DoubleColon,
    /// `=>`  — scope / lambda arrow
    Arrow,
    /// `->`  — switch-case / config-entry arrow
    SwitchCase,

    // ── Prefixed constructors  (prefix char + `:`) ───────────────────────────
    BlobConstructor(String),   // b:…
    TupleConstructor(String),  // t:…
    RegexConstructor(String),  // r:…

    // ── Section keywords (@…) ─────────────────────────────────────────────────
    SectionConfig,
    SectionImports,
    SectionDLM,
    SectionEnums,
    SectionQuickFuncs,
    SectionData,
    SectionSecurity,

    // ── Diagnostic / structural ───────────────────────────────────────────────
    Comment(String),
    Error(String),
    EndOfFile,
}

// =============================================================================
// TokenType methods
// =============================================================================

impl TokenType {
    // Factory helpers (used by parsers for construction / comparison)
    #[inline] pub fn double_colon()       -> Self { TokenType::DoubleColon }
    #[inline] pub fn arrow()              -> Self { TokenType::Arrow }
    #[inline] pub fn switch_case()        -> Self { TokenType::SwitchCase }
    #[inline] pub fn end_of_file()        -> Self { TokenType::EndOfFile }
    #[inline] pub fn section_config()     -> Self { TokenType::SectionConfig }
    #[inline] pub fn section_imports()    -> Self { TokenType::SectionImports }
    #[inline] pub fn section_dlm()        -> Self { TokenType::SectionDLM }
    #[inline] pub fn section_enums()      -> Self { TokenType::SectionEnums }
    #[inline] pub fn section_quickfuncs() -> Self { TokenType::SectionQuickFuncs }
    #[inline] pub fn section_data()       -> Self { TokenType::SectionData }
    #[inline] pub fn section_security()   -> Self { TokenType::SectionSecurity }
    #[inline] pub fn bool_true()          -> Self { TokenType::Bool(true) }
    #[inline] pub fn bool_false()         -> Self { TokenType::Bool(false) }
    #[inline] pub fn get_symbol(c: char)  -> Self { TokenType::Symbol(c) }

    pub fn is_section_keyword(&self) -> bool {
        matches!(
            self,
            TokenType::SectionConfig
                | TokenType::SectionDLM
                | TokenType::SectionEnums
                | TokenType::SectionImports
                | TokenType::SectionQuickFuncs
                | TokenType::SectionData
                | TokenType::SectionSecurity
        )
    }

    pub fn get_section_context(&self) -> Option<&'static str> {
        match self {
            TokenType::SectionConfig     => Some("CONFIG"),
            TokenType::SectionDLM        => Some("DLM"),
            TokenType::SectionEnums      => Some("ENUMS"),
            TokenType::SectionImports    => Some("IMPORTS"),
            TokenType::SectionQuickFuncs => Some("QUICKFUNCS"),
            TokenType::SectionData       => Some("DATA"),
            TokenType::SectionSecurity   => Some("SECURITY"),
            TokenType::Keyword(k) if k.starts_with('@') => Some(&k[1..]),
            _ => None,
        }
    }

    /// Returns `true` for any numeric literal token type.
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            TokenType::Integer(_)
                | TokenType::Long(_)
                | TokenType::Float(_)
                | TokenType::Double(_)
                | TokenType::ScientificNotation(_)
        )
    }
}

// =============================================================================
// Display
// =============================================================================

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenType::Keyword(k)              => write!(f, "Keyword({})", k),
            TokenType::ArithmeticOp(ao)        => write!(f, "ArithmeticOp({})", ao),
            TokenType::ArithmeticAssignOp(aao) => write!(f, "ArithmeticAssignOp({})", aao),
            TokenType::ComparisonOp(co)        => write!(f, "ComparisonOp({})", co),
            TokenType::LogicalOp(lo)           => write!(f, "LogicalOp({})", lo),
            TokenType::BitwiseOp(bo)           => write!(f, "BitwiseOp({})", bo),
            TokenType::Identifier(i)           => write!(f, "Identifier({})", i),
            TokenType::Integer(i)              => write!(f, "Integer({})", i),
            TokenType::Long(l)                 => write!(f, "Long({}L)", l),
            TokenType::Float(fl)               => write!(f, "Float({})", fl),
            TokenType::Double(d)               => write!(f, "Double({})", d),
            TokenType::ScientificNotation(sn)  => write!(f, "ScientificNotation({})", sn),
            TokenType::String(s)               => write!(f, "String(\"{}\")", s),
            TokenType::StringSingle(ss)        => write!(f, "StringSingle('{}')", ss),
            TokenType::Bool(b)                 => write!(f, "Bool({})", b),
            TokenType::InterpolatedString(ist) => write!(f, "InterpolatedString($\"{}\")", ist),
            TokenType::Symbol(s)               => write!(f, "Symbol({})", s),
            TokenType::HexColor(hc)            => write!(f, "HexColor({})", hc),
            TokenType::Date(d)                 => write!(f, "Date({})", d),
            TokenType::Timestamp(t)            => write!(f, "Timestamp({})", t),
            TokenType::DoubleColon             => write!(f, "DoubleColon(::)"),
            TokenType::Arrow                   => write!(f, "Arrow(=>)"),
            TokenType::SwitchCase              => write!(f, "SwitchCase(->)"),
            TokenType::BlobConstructor(bc)     => write!(f, "BlobConstructor(b:{})",bc),
            TokenType::TupleConstructor(tc)    => write!(f, "TupleConstructor(t:{})",tc),
            TokenType::RegexConstructor(rc)    => write!(f, "RegexConstructor(r:{})",rc),
            TokenType::SectionConfig           => write!(f, "SectionConfig(@CONFIG)"),
            TokenType::SectionDLM              => write!(f, "SectionDLM(@DLM)"),
            TokenType::SectionEnums            => write!(f, "SectionEnums(@ENUMS)"),
            TokenType::SectionQuickFuncs       => write!(f, "SectionQuickFuncs(@QUICKFUNCS)"),
            TokenType::SectionData             => write!(f, "SectionData(@DATA)"),
            TokenType::SectionSecurity         => write!(f, "SectionSecurity(@SECURITY)"),
            TokenType::SectionImports          => write!(f, "SectionImports(@IMPORTS)"),
            TokenType::Comment(c)              => write!(f, "Comment({})", c),
            TokenType::Error(e)                => write!(f, "Error({})", e),
            TokenType::EndOfFile               => write!(f, "EndOfFile"),
        }
    }
}

// =============================================================================
// Token
// =============================================================================

#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub line:       usize,
    pub column:     usize,
    pub section:    SectionId,
}

impl Token {
    #[inline]
    pub fn new(token_type: TokenType, line: usize, column: usize, section: SectionId) -> Self {
        Token { token_type, line, column, section }
    }

    #[inline]
    pub fn eof(line: usize, column: usize) -> Self {
        Token { token_type: TokenType::EndOfFile, line, column, section: SectionId::None }
    }

    pub fn get_token_value(&self) -> String {
        match &self.token_type {
            TokenType::Keyword(k)              => k.to_string(),
            TokenType::ArithmeticOp(ao)        => ao.to_string(),
            TokenType::ArithmeticAssignOp(aao) => aao.to_string(),
            TokenType::ComparisonOp(co)        => co.to_string(),
            TokenType::LogicalOp(lo)           => lo.to_string(),
            TokenType::BitwiseOp(bo)           => bo.to_string(),
            TokenType::Identifier(i)           => i.clone(),
            TokenType::Integer(i)              => i.to_string(),
            TokenType::Long(l)                 => format!("{}L", l),
            TokenType::Float(f)                => f.to_string(),
            TokenType::Double(d)               => d.to_string(),
            TokenType::ScientificNotation(sn)  => format!("{:e}", sn),
            TokenType::String(s)               => s.clone(),
            TokenType::StringSingle(ss)        => ss.clone(),
            TokenType::Bool(b)                 => b.to_string().to_lowercase(),
            TokenType::InterpolatedString(ist) => ist.clone(),
            TokenType::Symbol(s)               => s.to_string(),
            TokenType::HexColor(hc)            => hc.clone(),
            TokenType::Date(d)                 => d.clone(),
            TokenType::Timestamp(t)            => t.clone(),
            TokenType::DoubleColon             => "::".to_string(),
            TokenType::Arrow                   => "=>".to_string(),
            TokenType::SwitchCase              => "->".to_string(),
            TokenType::BlobConstructor(bc)     => format!("b:{}", bc),
            TokenType::TupleConstructor(tc)    => format!("t:{}", tc),
            TokenType::RegexConstructor(rc)    => format!("r:{}", rc),
            TokenType::Comment(c)              => c.clone(),
            TokenType::Error(e)                => e.clone(),
            TokenType::EndOfFile               => "EOF".to_string(),
            _ => self.token_type.to_string(),
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let section_info = if self.section.is_some() {
            format!(", Section: {}", self.section.as_str())
        } else {
            String::new()
        };
        write!(
            f,
            "{{Token: {}, Line: {}, Column: {}{}}}",
            self.token_type, self.line, self.column, section_info
        )
    }
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.token_type == other.token_type
            && self.line   == other.line
            && self.column == other.column
    }
}

// =============================================================================
// TokenExtensions
// =============================================================================

pub trait TokenExtensions {
    fn could_be_static_object_name(&self) -> bool;
}

impl TokenExtensions for Token {
    fn could_be_static_object_name(&self) -> bool {
        if let TokenType::Identifier(id) = &self.token_type {
            id.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
        } else {
            false
        }
    }
    }
