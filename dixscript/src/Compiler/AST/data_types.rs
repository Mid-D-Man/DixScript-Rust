
/// All supported data types for DixScript
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataType {
    Int,
    Float,
    Double,
    String,
    Bool,
    Array,
    Tuple,
    Hex,
    Blob,
    Regex,
    Object,
    Timestamp,
    Date,
    Enum,
    Any,
    Function,
    Range, // Not implemented (probably won't - we have array.range() built-in but im to lazy to deal with it)
}

impl std::fmt::Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DataType::Int => "int",
            DataType::Float => "float",
            DataType::Double => "double",
            DataType::String => "string",
            DataType::Bool => "bool",
            DataType::Array => "array",
            DataType::Tuple => "tuple",
            DataType::Hex => "hex",
            DataType::Blob => "blob",
            DataType::Regex => "regex",
            DataType::Object => "object",
            DataType::Timestamp => "timestamp",
            DataType::Date => "date",
            DataType::Enum => "enum",
            DataType::Any => "any",
            DataType::Function => "function",
            DataType::Range => "range",
        };
        write!(f, "{}", s)
    }
}

/// Error handling strategies for CONFIG section
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorHandlingStrategy {
    Halt,
    Continue,
    Recover,
}

impl std::fmt::Display for ErrorHandlingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ErrorHandlingStrategy::Halt => "halt",
            ErrorHandlingStrategy::Continue => "continue",
            ErrorHandlingStrategy::Recover => "recover",
        };
        write!(f, "{}", s)
    }
}

/// Compatibility modes for CONFIG section
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompatibilityMode {
    Strict,
    BestEffort,
    Permissive,
}

impl std::fmt::Display for CompatibilityMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CompatibilityMode::Strict => "strict",
            CompatibilityMode::BestEffort => "best_effort",
            CompatibilityMode::Permissive => "permissive",
        };
        write!(f, "{}", s)
    }
}

/// Debug modes for CONFIG section
/// Ordered from least verbose to most verbose
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DebugMode {
    Off,
    Regular,
    Verbose,
}

impl std::fmt::Display for DebugMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DebugMode::Off => "off",
            DebugMode::Regular => "regular",
            DebugMode::Verbose => "verbose",
        };
        write!(f, "{}", s)
    }
}

/// DLM module types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DLMModuleType {
    DCompressor,
    DAuditor,
    DEncryptor,
    ParseError,
}

impl std::fmt::Display for DLMModuleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DLMModuleType::DCompressor => "DCompressor",
            DLMModuleType::DAuditor => "DAuditor",
            DLMModuleType::DEncryptor => "DEncryptor",
            DLMModuleType::ParseError => "ParseError",
        };
        write!(f, "{}", s)
    }
}

/// DLM module subtypes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DLMModuleSubtype {
    Gzip,
    Bzip2,
    Lzma,
    Diy,
    Enhanced,
    Xor,
    Aes128,
    Aes256,
    Chacha20,
    ParseError,
}

impl std::fmt::Display for DLMModuleSubtype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DLMModuleSubtype::Gzip => "gzip",
            DLMModuleSubtype::Bzip2 => "bzip2",
            DLMModuleSubtype::Lzma => "lzma",
            DLMModuleSubtype::Diy => "diy",
            DLMModuleSubtype::Enhanced => "enhanced",
            DLMModuleSubtype::Xor => "xor",
            DLMModuleSubtype::Aes128 => "aes128",
            DLMModuleSubtype::Aes256 => "aes256",
            DLMModuleSubtype::Chacha20 => "chacha20",
            DLMModuleSubtype::ParseError => "ParseError",
        };
        write!(f, "{}", s)
    }
}

/// Variable declaration types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclarationType {
    Let,   // let x = 5 (immutable by default)
    Const, // const x = 5 (always immutable)
}

impl std::fmt::Display for DeclarationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DeclarationType::Let => "let",
            DeclarationType::Const => "const",
        };
        write!(f, "{}", s)
    }
}
