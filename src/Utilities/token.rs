use std::fmt;

/// Token types for DixScript v1.0.0
/// Represents all possible token types in the language
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Core primitive types
    Keyword(String),
    Identifier(String),
    Integer(i32),
    Float(f32),
    Double(f64),
    ScientificNotation(f64),
    String(String),
    Bool(bool),

    // Enhanced string types
    InterpolatedString(String),
    StringSingle(String),

    // Symbols and operators
    Symbol(char),
    MultiCharSymbol(String),

    // Special data types
    HexColor(String),
    HexLiteral(i32),
    Date(String),
    Timestamp(String),

    // Table/Group syntax tokens
    TablePath(String),
    DoubleColon,
    Arrow,
    SwitchCase,

    // Function and control flow tokens
    FunctionPrefix,
    ControlFlowColon,

    // Prefixed constructor tokens
    PrefixedConstructor { prefix: String, value: String },
    BlobConstructor(String),
    TupleConstructor(String),
    RegexConstructor(String),

    // Operator categories
    ArithmeticOp(String),
    ArithmeticAssignOp(String),
    ComparisonOp(String),
    LogicalOp(String),
    BitwiseOp(String),

    // Section keywords
    SectionConfig,
    SectionImports,
    SectionDLM,
    SectionEnums,
    SectionQuickFuncs,
    SectionData,
    SectionSecurity,

    // Access and scope tokens
    ConfigAccess(String),
    EnumAccess { enum_name: String, value: String },
    ObjectAccess(Vec<String>),
    ScopeDeclaration(String),

    // Built-in function categories
    StaticFunction { class: String, method: String },
    DixFunction(String),
    BuiltinMethod(String),

    // Data type tokens
    DataType(String),

    // Comments and special tokens
    Comment(String),
    Error(String),
    EndOfFile,

    // Context-sensitive tokens
    ParseContext(String),
}

impl TokenType {
    // Singleton instances for common tokens (zero-allocation optimization)
    pub fn double_colon() -> Self {
        TokenType::DoubleColon
    }

    pub fn arrow() -> Self {
        TokenType::Arrow
    }

    pub fn switch_case() -> Self {
        TokenType::SwitchCase
    }

    pub fn function_prefix() -> Self {
        TokenType::FunctionPrefix
    }

    pub fn control_flow_colon() -> Self {
        TokenType::ControlFlowColon
    }

    pub fn end_of_file() -> Self {
        TokenType::EndOfFile
    }

    pub fn section_config() -> Self {
        TokenType::SectionConfig
    }

    pub fn section_imports() -> Self {
        TokenType::SectionImports
    }

    pub fn section_dlm() -> Self {
        TokenType::SectionDLM
    }

    pub fn section_enums() -> Self {
        TokenType::SectionEnums
    }

    pub fn section_quickfuncs() -> Self {
        TokenType::SectionQuickFuncs
    }

    pub fn section_data() -> Self {
        TokenType::SectionData
    }

    pub fn section_security() -> Self {
        TokenType::SectionSecurity
    }

    pub fn bool_true() -> Self {
        TokenType::Bool(true)
    }

    pub fn bool_false() -> Self {
        TokenType::Bool(false)
    }

    /// Get symbol token (optimized for ASCII)
    pub fn get_symbol(c: char) -> Self {
        TokenType::Symbol(c)
    }

    /// Check if token is a section keyword
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

    /// Get section context if this is a section token
    pub fn get_section_context(&self) -> Option<&str> {
        match self {
            TokenType::SectionConfig => Some("CONFIG"),
            TokenType::SectionDLM => Some("DLM"),
            TokenType::SectionEnums => Some("ENUMS"),
            TokenType::SectionImports => Some("IMPORTS"),
            TokenType::SectionQuickFuncs => Some("QUICKFUNCS"),
            TokenType::SectionData => Some("DATA"),
            TokenType::SectionSecurity => Some("SECURITY"),
            TokenType::Keyword(k) if k.starts_with('@') => Some(&k[1..]),
            _ => None,
        }
    }
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenType::Keyword(k) => write!(f, "Keyword({})", k),
            TokenType::Identifier(i) => write!(f, "Identifier({})", i),
            TokenType::Integer(i) => write!(f, "Integer({})", i),
            TokenType::Float(fl) => write!(f, "Float({})", fl),
            TokenType::Double(d) => write!(f, "Double({})", d),
            TokenType::ScientificNotation(sn) => write!(f, "ScientificNotation({})", sn),
            TokenType::String(s) => write!(f, "String(\"{}\")", s),
            TokenType::StringSingle(ss) => write!(f, "StringSingle('{}')", ss),
            TokenType::Bool(b) => write!(f, "Bool({})", b),
            TokenType::InterpolatedString(ist) => write!(f, "InterpolatedString($\"{}\")", ist),
            TokenType::Symbol(s) => write!(f, "Symbol({})", s),
            TokenType::MultiCharSymbol(ms) => write!(f, "MultiCharSymbol({})", ms),
            TokenType::BitwiseOp(bo) => write!(f, "BitwiseOp({})", bo),
            TokenType::HexColor(hc) => write!(f, "HexColor({})", hc),
            TokenType::HexLiteral(hl) => write!(f, "HexLiteral(0x{:X})", hl),
            TokenType::Date(d) => write!(f, "Date({})", d),
            TokenType::Timestamp(t) => write!(f, "Timestamp({})", t),
            TokenType::TablePath(tp) => write!(f, "TablePath({})", tp),
            TokenType::DoubleColon => write!(f, "DoubleColon(::)"),
            TokenType::Arrow => write!(f, "Arrow(=>)"),
            TokenType::SwitchCase => write!(f, "SwitchCase(->)"),
            TokenType::FunctionPrefix => write!(f, "FunctionPrefix(~)"),
            TokenType::ControlFlowColon => write!(f, "ControlFlowColon(:)"),
            TokenType::PrefixedConstructor { prefix, value } => {
                write!(f, "PrefixedConstructor({}:{})", prefix, value)
            }
            TokenType::BlobConstructor(bc) => write!(f, "BlobConstructor(b:{})", bc),
            TokenType::TupleConstructor(tc) => write!(f, "TupleConstructor(t:{})", tc),
            TokenType::RegexConstructor(rc) => write!(f, "RegexConstructor(r:{})", rc),
            TokenType::ArithmeticOp(ao) => write!(f, "ArithmeticOp({})", ao),
            TokenType::ArithmeticAssignOp(aao) => write!(f, "ArithmeticAssignOp({})", aao),
            TokenType::ComparisonOp(co) => write!(f, "ComparisonOp({})", co),
            TokenType::LogicalOp(lo) => write!(f, "LogicalOp({})", lo),
            TokenType::SectionConfig => write!(f, "SectionConfig(@CONFIG)"),
            TokenType::SectionDLM => write!(f, "SectionDLM(@DLM)"),
            TokenType::SectionEnums => write!(f, "SectionEnums(@ENUMS)"),
            TokenType::SectionQuickFuncs => write!(f, "SectionQuickFuncs(@QUICKFUNCS)"),
            TokenType::SectionData => write!(f, "SectionData(@DATA)"),
            TokenType::SectionSecurity => write!(f, "SectionSecurity(@SECURITY)"),
            TokenType::SectionImports => write!(f, "SectionImports(@IMPORTS)"),
            TokenType::ConfigAccess(ca) => write!(f, "ConfigAccess(config.{})", ca),
            TokenType::EnumAccess { enum_name, value } => {
                write!(f, "EnumAccess({}.{})", enum_name, value)
            }
            TokenType::ObjectAccess(oa) => write!(f, "ObjectAccess({})", oa.join(".")),
            TokenType::ScopeDeclaration(sd) => write!(f, "ScopeDeclaration(=> {})", sd),
            TokenType::StaticFunction { class, method } => {
                write!(f, "StaticFunction({}.{})", class, method)
            }
            TokenType::DixFunction(df) => write!(f, "DixFunction(Dix.{})", df),
            TokenType::BuiltinMethod(bm) => write!(f, "BuiltinMethod(.{})", bm),
            TokenType::DataType(dt) => write!(f, "DataType(<{}>)", dt),
            TokenType::Comment(c) => write!(f, "Comment({})", c),
            TokenType::Error(e) => write!(f, "Error({})", e),
            TokenType::EndOfFile => write!(f, "EndOfFile"),
            TokenType::ParseContext(pc) => write!(f, "ParseContext({})", pc),
        }
    }
}

/// Token class with position information
#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub line: usize,
    pub column: usize,
    pub section: Option<String>,
}

impl Token {
    /// Create a new token
    pub fn new(
        token_type: TokenType,
        line: usize,
        column: usize,
        section: Option<String>,
    ) -> Self {
        Token {
            token_type,
            line,
            column,
            section,
        }
    }

    /// Create EOF token
    pub fn eof(line: usize, column: usize) -> Self {
        Token {
            token_type: TokenType::EndOfFile,
            line,
            column,
            section: None,
        }
    }

    /// Get the string value of the token
    pub fn get_token_value(&self) -> String {
        match &self.token_type {
            TokenType::Keyword(k) => k.clone(),
            TokenType::Identifier(i) => i.clone(),
            TokenType::Integer(i) => i.to_string(),
            TokenType::Float(f) => f.to_string(),
            TokenType::Double(d) => d.to_string(),
            TokenType::ScientificNotation(sn) => format!("{:e}", sn),
            TokenType::String(s) => s.clone(),
            TokenType::StringSingle(ss) => ss.clone(),
            TokenType::Bool(b) => b.to_string().to_lowercase(),
            TokenType::Symbol(s) => s.to_string(),
            TokenType::MultiCharSymbol(ms) => ms.clone(),
            TokenType::HexColor(hc) => hc.clone(),
            TokenType::HexLiteral(hl) => format!("0x{:X}", hl),
            TokenType::Date(d) => d.clone(),
            TokenType::Timestamp(t) => t.clone(),
            TokenType::InterpolatedString(ist) => ist.clone(),
            TokenType::TablePath(tp) => tp.clone(),
            TokenType::PrefixedConstructor { prefix, value } => format!("{}:{}", prefix, value),
            TokenType::BlobConstructor(bc) => format!("b:{}", bc),
            TokenType::TupleConstructor(tc) => format!("t:{}", tc),
            TokenType::RegexConstructor(rc) => format!("r:{}", rc),
            TokenType::ArithmeticOp(ao) => ao.clone(),
            TokenType::ArithmeticAssignOp(aao) => aao.clone(),
            TokenType::ComparisonOp(co) => co.clone(),
            TokenType::LogicalOp(lo) => lo.clone(),
            TokenType::DoubleColon => "::".to_string(),
            TokenType::Arrow => "=>".to_string(),
            TokenType::SwitchCase => "->".to_string(),
            TokenType::FunctionPrefix => "~".to_string(),
            TokenType::ControlFlowColon => ":".to_string(),
            TokenType::ConfigAccess(ca) => format!("config.{}", ca),
            TokenType::EnumAccess { enum_name, value } => format!("{}.{}", enum_name, value),
            TokenType::Comment(c) => c.clone(),
            TokenType::Error(e) => e.clone(),
            TokenType::EndOfFile => "EOF".to_string(),
            _ => self.token_type.to_string(),
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let section_info = self
            .section
            .as_ref()
            .map(|s| format!(", Section: {}", s))
            .unwrap_or_default();
        write!(
            f,
            "{{Token: {}, Line: {}, Column: {}{}}}",
            self.token_type, self.line, self.column, section_info
        )
    }
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.token_type == other.token_type && self.line == other.line && self.column == other.column
    }
}

/// Extension methods for Token
pub trait TokenExtensions {
    fn could_be_static_object_name(&self) -> bool;
}

impl TokenExtensions for Token {
    fn could_be_static_object_name(&self) -> bool {
        if let TokenType::Identifier(id) = &self.token_type {
            // Check if first char is uppercase (PascalCase)
            id.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
            // Note: Full check would require StaticObjectRegistry
        } else {
            false
        }
    }
}