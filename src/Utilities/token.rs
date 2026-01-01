//! Token - Lexical token with C# style API

// No need to import ImmutableArray since we're not using it in Token currently

/// TokenType - All token types in DixScript
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Keywords
    Keyword(String),

    // Identifiers
    Identifier(String),

    // Literals - Numbers
    Integer(i32),
    Float(f32),
    Double(f64),
    ScientificNotation(f64),
    HexLiteral(i32),

    // Literals - Strings
    String(String),
    StringSingle(String),
    InterpolatedString(String),

    // Literals - Other
    Bool(bool),
    HexColor(String),
    Date(String),
    Timestamp(String),

    // Symbols
    Symbol(char),
    MultiCharSymbol(String),

    // Operators
    ArithmeticOp(String),
    ArithmeticAssignOp(String),
    ComparisonOp(String),
    LogicalOp(String),
    BitwiseOp(String),

    // Special syntax
    TablePath(String),
    DoubleColon,
    Arrow,
    SwitchCase,
    FunctionPrefix,
    ControlFlowColon,

    // Constructors
    PrefixedConstructor(String, String), // (prefix, value)
    BlobConstructor(String),
    TupleConstructor(String),
    RegexConstructor(String),

    // Section keywords
    SectionConfig,
    SectionDLM,
    SectionEnums,
    SectionQuickFuncs,
    SectionData,
    SectionSecurity,

    // Access expressions
    ConfigAccess(String),
    EnumAccess(String, String), // (enum_name, value)
    ObjectAccess(Vec<String>),
    ScopeDeclaration(String),

    // Functions
    StaticFunction(String, String), // (class, method)
    DixFunction(String),
    BuiltinMethod(String),

    // Types
    DataType(String),

    // Special
    Comment(String),
    Error(String),
    EndOfFile,
    ParseContext(String),
}

impl TokenType {
    /// Returns a string representation of the token type
    pub fn ToString(&self) -> String {
        match self {
            TokenType::Keyword(s) => format!("Keyword({})", s),
            TokenType::Identifier(s) => format!("Identifier({})", s),
            TokenType::Integer(n) => format!("Integer({})", n),
            TokenType::Float(f) => format!("Float({})", f),
            TokenType::Double(d) => format!("Double({})", d),
            TokenType::ScientificNotation(sn) => format!("ScientificNotation({:e})", sn),
            TokenType::String(s) => format!("String(\"{}\")", s),
            TokenType::StringSingle(s) => format!("StringSingle('{}')", s),
            TokenType::InterpolatedString(s) => format!("InterpolatedString($\"{}\")", s),
            TokenType::Bool(b) => format!("Bool({})", b),
            TokenType::HexColor(h) => format!("HexColor({})", h),
            TokenType::HexLiteral(h) => format!("HexLiteral(0x{:X})", h),
            TokenType::Date(d) => format!("Date({})", d),
            TokenType::Timestamp(t) => format!("Timestamp({})", t),
            TokenType::Symbol(c) => format!("Symbol({})", c),
            TokenType::MultiCharSymbol(s) => format!("MultiCharSymbol({})", s),
            TokenType::ArithmeticOp(op) => format!("ArithmeticOp({})", op),
            TokenType::ArithmeticAssignOp(op) => format!("ArithmeticAssignOp({})", op),
            TokenType::ComparisonOp(op) => format!("ComparisonOp({})", op),
            TokenType::LogicalOp(op) => format!("LogicalOp({})", op),
            TokenType::BitwiseOp(op) => format!("BitwiseOp({})", op),
            TokenType::TablePath(p) => format!("TablePath({})", p),
            TokenType::DoubleColon => "DoubleColon(::)".to_string(),
            TokenType::Arrow => "Arrow(=>)".to_string(),
            TokenType::SwitchCase => "SwitchCase(->)".to_string(),
            TokenType::FunctionPrefix => "FunctionPrefix(~)".to_string(),
            TokenType::ControlFlowColon => "ControlFlowColon(:)".to_string(),
            TokenType::PrefixedConstructor(prefix, value) => {
                format!("PrefixedConstructor({}:{})", prefix, value)
            }
            TokenType::BlobConstructor(v) => format!("BlobConstructor(b:{})", v),
            TokenType::TupleConstructor(v) => format!("TupleConstructor(t:{})", v),
            TokenType::RegexConstructor(v) => format!("RegexConstructor(r:{})", v),
            TokenType::SectionConfig => "SectionConfig(@CONFIG)".to_string(),
            TokenType::SectionDLM => "SectionDLM(@DLM)".to_string(),
            TokenType::SectionEnums => "SectionEnums(@ENUMS)".to_string(),
            TokenType::SectionQuickFuncs => "SectionQuickFuncs(@QUICKFUNCS)".to_string(),
            TokenType::SectionData => "SectionData(@DATA)".to_string(),
            TokenType::SectionSecurity => "SectionSecurity(@SECURITY)".to_string(),
            TokenType::ConfigAccess(key) => format!("ConfigAccess(config.{})", key),
            TokenType::EnumAccess(enum_name, value) => {
                format!("EnumAccess({}.{})", enum_name, value)
            }
            TokenType::ObjectAccess(path) => format!("ObjectAccess({})", path.join(".")),
            TokenType::ScopeDeclaration(scope) => format!("ScopeDeclaration(=> {})", scope),
            TokenType::StaticFunction(class, method) => {
                format!("StaticFunction({}.{})", class, method)
            }
            TokenType::DixFunction(func) => format!("DixFunction(Dix.{})", func),
            TokenType::BuiltinMethod(method) => format!("BuiltinMethod(.{})", method),
            TokenType::DataType(dt) => format!("DataType(<{}>)", dt),
            TokenType::Comment(c) => format!("Comment({})", c),
            TokenType::Error(e) => format!("Error({})", e),
            TokenType::EndOfFile => "EndOfFile".to_string(),
            TokenType::ParseContext(ctx) => format!("ParseContext({})", ctx),
        }
    }

    /// Checks if this is a section keyword
    pub fn IsSectionKeyword(&self) -> bool {
        matches!(
            self,
            TokenType::SectionConfig
                | TokenType::SectionDLM
                | TokenType::SectionEnums
                | TokenType::SectionQuickFuncs
                | TokenType::SectionData
                | TokenType::SectionSecurity
        )
    }

    /// Gets the section context name
    pub fn GetSectionContext(&self) -> Option<String> {
        match self {
            TokenType::SectionConfig => Some("CONFIG".to_string()),
            TokenType::SectionDLM => Some("DLM".to_string()),
            TokenType::SectionEnums => Some("ENUMS".to_string()),
            TokenType::SectionQuickFuncs => Some("QUICKFUNCS".to_string()),
            TokenType::SectionData => Some("DATA".to_string()),
            TokenType::SectionSecurity => Some("SECURITY".to_string()),
            TokenType::Keyword(k) if k.starts_with('@') => Some(k[1..].to_string()),
            _ => None,
        }
    }
}

/// Token - Lexical token with position information (C# style)
#[derive(Debug, Clone)]
pub struct Token {
    pub TokenType: TokenType,
    pub Line: usize,
    pub Column: usize,
    pub Section: Option<String>,
}

impl Token {
    /// Creates a new token
    pub fn New(token_type: TokenType, line: usize, column: usize) -> Self {
        Self {
            TokenType: token_type,
            Line: line,
            Column: column,
            Section: None,
        }
    }

    /// Creates a new token with section
    pub fn NewWithSection(
        token_type: TokenType,
        line: usize,
        column: usize,
        section: String,
    ) -> Self {
        Self {
            TokenType: token_type,
            Line: line,
            Column: column,
            Section: Some(section),
        }
    }

    /// Creates an EOF token
    pub fn EOF(line: usize, column: usize) -> Self {
        Self::New(TokenType::EndOfFile, line, column)
    }

    /// Returns a copy with section set
    pub fn WithSection(&self, section: String) -> Self {
        Self {
            TokenType: self.TokenType.clone(),
            Line: self.Line,
            Column: self.Column,
            Section: Some(section),
        }
    }

    /// Gets the token value as a string
    pub fn GetTokenValue(&self) -> String {
        match &self.TokenType {
            TokenType::Keyword(k) => k.clone(),
            TokenType::Identifier(i) => i.clone(),
            TokenType::Integer(n) => n.to_string(),
            TokenType::Float(f) => f.to_string(),
            TokenType::Double(d) => d.to_string(),
            TokenType::String(s) => s.clone(),
            TokenType::Bool(b) => b.to_string(),
            TokenType::Symbol(c) => c.to_string(),
            _ => self.TokenType.ToString(),
        }
    }

    /// Converts token to string (C# ToString)
    pub fn ToString(&self) -> String {
        let section_info = match &self.Section {
            Some(s) => format!(", Section: {}", s),
            None => String::new(),
        };

        format!(
            "{{Token: {}, Line: {}, Column: {}{}}}",
            self.TokenType.ToString(),
            self.Line,
            self.Column,
            section_info
        )
    }
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.TokenType == other.TokenType && self.Line == other.Line && self.Column == other.Column
    }
}