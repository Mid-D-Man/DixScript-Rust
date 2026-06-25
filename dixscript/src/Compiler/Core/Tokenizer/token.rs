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

#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // ── Static-string variants ───────────────────────────────────────────────
    Keyword(&'static str),
    MultiCharSymbol(&'static str),
    ArithmeticOp(&'static str),
    ArithmeticAssignOp(&'static str),
    ComparisonOp(&'static str),
    LogicalOp(&'static str),
    BitwiseOp(&'static str),
    DataType(&'static str),

    // ── Owned-string / primitive variants ───────────────────────────────────
    Identifier(String),
    Integer(i32),
    /// 64-bit integer literal. Suffix `L`/`l`, or auto-promoted from overflowing i32.
    /// Covers plain decimal (`9_000_000_000L`), hex (`0xDEAD_BEEFL`),
    /// and binary (`0b1111_0000_1111_0000L`) forms.
    Long(i64),
    Float(f32),
    Double(f64),
    ScientificNotation(f64),
    String(String),
    StringSingle(String),
    InterpolatedString(String),
    Bool(bool),

    // ── Symbols ──────────────────────────────────────────────────────────────
    Symbol(char),

    // ── Special data types ───────────────────────────────────────────────────
    HexColor(String),
    HexLiteral(i32),
    Date(String),
    Timestamp(String),

    // ── Table / group syntax ─────────────────────────────────────────────────
    TablePath(String),
    DoubleColon,
    Arrow,
    SwitchCase,

    // ── Function / control-flow markers ──────────────────────────────────────
    ControlFlowColon,

    // ── Prefixed constructors ─────────────────────────────────────────────────
    PrefixedConstructor { prefix: String, value: String },
    BlobConstructor(String),
    TupleConstructor(String),
    RegexConstructor(String),

    // ── Section keywords ──────────────────────────────────────────────────────
    SectionConfig,
    SectionImports,
    SectionDLM,
    SectionEnums,
    SectionQuickFuncs,
    SectionData,
    SectionSecurity,

    // ── Access / scope ────────────────────────────────────────────────────────
    ConfigAccess(String),
    EnumAccess { enum_name: String, value: String },
    ObjectAccess(Vec<String>),
    ScopeDeclaration(String),

    // ── Built-in function categories ──────────────────────────────────────────
    StaticFunction { class: String, method: String },
    DixFunction(String),
    BuiltinMethod(String),

    // ── Diagnostic / special tokens ───────────────────────────────────────────
    Comment(String),
    Error(String),
    EndOfFile,
    ParseContext(String),
}

// =============================================================================
// TokenType methods
// =============================================================================

impl TokenType {
    #[inline] pub fn double_colon()       -> Self { TokenType::DoubleColon }
    #[inline] pub fn arrow()              -> Self { TokenType::Arrow }
    #[inline] pub fn switch_case()        -> Self { TokenType::SwitchCase }
    #[inline] pub fn control_flow_colon() -> Self { TokenType::ControlFlowColon }
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

    /// Returns `true` for any numeric token type.
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            TokenType::Integer(_)
                | TokenType::Long(_)
                | TokenType::Float(_)
                | TokenType::Double(_)
                | TokenType::ScientificNotation(_)
                | TokenType::HexLiteral(_)
        )
    }
}

// =============================================================================
// Display
// =============================================================================

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenType::Keyword(k)                        => write!(f, "Keyword({})", k),
            TokenType::MultiCharSymbol(ms)               => write!(f, "MultiCharSymbol({})", ms),
            TokenType::ArithmeticOp(ao)                  => write!(f, "ArithmeticOp({})", ao),
            TokenType::ArithmeticAssignOp(aao)           => write!(f, "ArithmeticAssignOp({})", aao),
            TokenType::ComparisonOp(co)                  => write!(f, "ComparisonOp({})", co),
            TokenType::LogicalOp(lo)                     => write!(f, "LogicalOp({})", lo),
            TokenType::BitwiseOp(bo)                     => write!(f, "BitwiseOp({})", bo),
            TokenType::DataType(dt)                      => write!(f, "DataType(<{}>)", dt),
            TokenType::Identifier(i)                     => write!(f, "Identifier({})", i),
            TokenType::Integer(i)                        => write!(f, "Integer({})", i),
            TokenType::Long(l)                           => write!(f, "Long({}L)", l),
            TokenType::Float(fl)                         => write!(f, "Float({})", fl),
            TokenType::Double(d)                         => write!(f, "Double({})", d),
            TokenType::ScientificNotation(sn)            => write!(f, "ScientificNotation({})", sn),
            TokenType::String(s)                         => write!(f, "String(\"{}\")", s),
            TokenType::StringSingle(ss)                  => write!(f, "StringSingle('{}')", ss),
            TokenType::Bool(b)                           => write!(f, "Bool({})", b),
            TokenType::InterpolatedString(ist)           => write!(f, "InterpolatedString($\"{}\")", ist),
            TokenType::Symbol(s)                         => write!(f, "Symbol({})", s),
            TokenType::HexColor(hc)                      => write!(f, "HexColor({})", hc),
            TokenType::HexLiteral(hl)                    => write!(f, "HexLiteral(0x{:X})", hl),
            TokenType::Date(d)                           => write!(f, "Date({})", d),
            TokenType::Timestamp(t)                      => write!(f, "Timestamp({})", t),
            TokenType::TablePath(tp)                     => write!(f, "TablePath({})", tp),
            TokenType::DoubleColon                       => write!(f, "DoubleColon(::)"),
            TokenType::Arrow                             => write!(f, "Arrow(=>)"),
            TokenType::SwitchCase                        => write!(f, "SwitchCase(->)"),
            TokenType::ControlFlowColon                  => write!(f, "ControlFlowColon(:)"),
            TokenType::PrefixedConstructor { prefix, value } => {
                write!(f, "PrefixedConstructor({}:{})", prefix, value)
            }
            TokenType::BlobConstructor(bc)               => write!(f, "BlobConstructor(b:{})", bc),
            TokenType::TupleConstructor(tc)              => write!(f, "TupleConstructor(t:{})", tc),
            TokenType::RegexConstructor(rc)              => write!(f, "RegexConstructor(r:{})", rc),
            TokenType::SectionConfig                     => write!(f, "SectionConfig(@CONFIG)"),
            TokenType::SectionDLM                        => write!(f, "SectionDLM(@DLM)"),
            TokenType::SectionEnums                      => write!(f, "SectionEnums(@ENUMS)"),
            TokenType::SectionQuickFuncs                 => write!(f, "SectionQuickFuncs(@QUICKFUNCS)"),
            TokenType::SectionData                       => write!(f, "SectionData(@DATA)"),
            TokenType::SectionSecurity                   => write!(f, "SectionSecurity(@SECURITY)"),
            TokenType::SectionImports                    => write!(f, "SectionImports(@IMPORTS)"),
            TokenType::ConfigAccess(ca)                  => write!(f, "ConfigAccess(config.{})", ca),
            TokenType::EnumAccess { enum_name, value }   => {
                write!(f, "EnumAccess({}.{})", enum_name, value)
            }
            TokenType::ObjectAccess(oa)                  => write!(f, "ObjectAccess({})", oa.join(".")),
            TokenType::ScopeDeclaration(sd)              => write!(f, "ScopeDeclaration(=> {})", sd),
            TokenType::StaticFunction { class, method }  => {
                write!(f, "StaticFunction({}.{})", class, method)
            }
            TokenType::DixFunction(df)                   => write!(f, "DixFunction(Dix.{})", df),
            TokenType::BuiltinMethod(bm)                 => write!(f, "BuiltinMethod(.{})", bm),
            TokenType::Comment(c)                        => write!(f, "Comment({})", c),
            TokenType::Error(e)                          => write!(f, "Error({})", e),
            TokenType::EndOfFile                         => write!(f, "EndOfFile"),
            TokenType::ParseContext(pc)                  => write!(f, "ParseContext({})", pc),
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
            TokenType::MultiCharSymbol(ms)     => ms.to_string(),
            TokenType::ArithmeticOp(ao)        => ao.to_string(),
            TokenType::ArithmeticAssignOp(aao) => aao.to_string(),
            TokenType::ComparisonOp(co)        => co.to_string(),
            TokenType::LogicalOp(lo)           => lo.to_string(),
            TokenType::BitwiseOp(bo)           => bo.to_string(),
            TokenType::DataType(dt)            => dt.to_string(),
            TokenType::Identifier(i)           => i.clone(),
            TokenType::Integer(i)              => i.to_string(),
            TokenType::Long(l)                 => format!("{}L", l),
            TokenType::Float(f)                => f.to_string(),
            TokenType::Double(d)               => d.to_string(),
            TokenType::ScientificNotation(sn)  => format!("{:e}", sn),
            TokenType::String(s)               => s.clone(),
            TokenType::StringSingle(ss)        => ss.clone(),
            TokenType::Bool(b)                 => b.to_string().to_lowercase(),
            TokenType::Symbol(s)               => s.to_string(),
            TokenType::HexColor(hc)            => hc.clone(),
            TokenType::HexLiteral(hl)          => format!("0x{:X}", hl),
            TokenType::Date(d)                 => d.clone(),
            TokenType::Timestamp(t)            => t.clone(),
            TokenType::InterpolatedString(ist) => ist.clone(),
            TokenType::TablePath(tp)           => tp.clone(),
            TokenType::PrefixedConstructor { prefix, value } => format!("{}:{}", prefix, value),
            TokenType::BlobConstructor(bc)     => format!("b:{}", bc),
            TokenType::TupleConstructor(tc)    => format!("t:{}", tc),
            TokenType::RegexConstructor(rc)    => format!("r:{}", rc),
            TokenType::DoubleColon             => "::".to_string(),
            TokenType::Arrow                   => "=>".to_string(),
            TokenType::SwitchCase              => "->".to_string(),
            TokenType::ControlFlowColon        => ":".to_string(),
            TokenType::ConfigAccess(ca)        => format!("config.{}", ca),
            TokenType::EnumAccess { enum_name, value } => format!("{}.{}", enum_name, value),
            TokenType::Comment(c)              => c.clone(),
            TokenType::Error(e)                => e.clone(),
            TokenType::EndOfFile               => "EOF".to_string(),
            _                                  => self.token_type.to_string(),
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
