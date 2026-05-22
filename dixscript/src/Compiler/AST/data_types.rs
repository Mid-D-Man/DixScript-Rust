// dixscript/src/Compiler/AST/data_types.rs

// ─────────────────────────────────────────────────────────────────────────────
// ElemType — element/field type for typed collections
//
// Mirrors DataType but is deliberately flat (no recursive Box) so it remains
// Copy, small, and Hashable.  Nested collections (array-of-arrays, etc.) appear
// as ElemType::Array / ElemType::Tuple — the inner element type of the nested
// collection is not tracked at this level (first-iteration limitation).
// ─────────────────────────────────────────────────────────────────────────────

/// Element type for typed collections (`<array<int>>`, `<tuple<int,bool>>`).
///
/// All variants are primitive or base-collection references so that the type
/// itself stays `Copy` and can be stored inline in `DataType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElemType {
    Int,
    Long,
    Float,
    Double,
    String,
    Bool,
    Hex,
    Blob,
    Regex,
    Object,
    Date,
    Timestamp,
    Enum,
    Any,
    /// Nested untyped array (inner element type not tracked at this depth)
    Array,
    /// Nested untyped tuple (element types not tracked at this depth)
    Tuple,
}

impl ElemType {
    /// Convert to the equivalent base `DataType`.
    pub fn to_data_type(self) -> DataType {
        match self {
            ElemType::Int       => DataType::Int,
            ElemType::Long      => DataType::Long,
            ElemType::Float     => DataType::Float,
            ElemType::Double    => DataType::Double,
            ElemType::String    => DataType::String,
            ElemType::Bool      => DataType::Bool,
            ElemType::Hex       => DataType::Hex,
            ElemType::Blob      => DataType::Blob,
            ElemType::Regex     => DataType::Regex,
            ElemType::Object    => DataType::Object,
            ElemType::Date      => DataType::Date,
            ElemType::Timestamp => DataType::Timestamp,
            ElemType::Enum      => DataType::Enum,
            ElemType::Any       => DataType::Any,
            ElemType::Array     => DataType::Array,
            ElemType::Tuple     => DataType::Tuple,
        }
    }

    /// Convert from a `DataType`.  Returns `None` for `Function` and `Range`
    /// (they cannot be element types), and strips typed-collection wrappers to
    /// `Array`/`Tuple` for `TypedArray`/`TypedTuple`.
    pub fn from_data_type(dt: DataType) -> Option<Self> {
        match dt {
            DataType::Int                    => Some(ElemType::Int),
            DataType::Long                   => Some(ElemType::Long),
            DataType::Float                  => Some(ElemType::Float),
            DataType::Double                 => Some(ElemType::Double),
            DataType::String                 => Some(ElemType::String),
            DataType::Bool                   => Some(ElemType::Bool),
            DataType::Hex                    => Some(ElemType::Hex),
            DataType::Blob                   => Some(ElemType::Blob),
            DataType::Regex                  => Some(ElemType::Regex),
            DataType::Object                 => Some(ElemType::Object),
            DataType::Date                   => Some(ElemType::Date),
            DataType::Timestamp              => Some(ElemType::Timestamp),
            DataType::Enum                   => Some(ElemType::Enum),
            DataType::Any                    => Some(ElemType::Any),
            DataType::Array | DataType::TypedArray(_) => Some(ElemType::Array),
            DataType::Tuple | DataType::TypedTuple(_) => Some(ElemType::Tuple),
            DataType::Function | DataType::Range      => None,
        }
    }

    /// Parse from a lowercase keyword string.
    pub fn from_keyword(s: &str) -> Option<Self> {
        match s {
            "int"       => Some(ElemType::Int),
            "long"      => Some(ElemType::Long),
            "float"     => Some(ElemType::Float),
            "double"    => Some(ElemType::Double),
            "string"    => Some(ElemType::String),
            "bool"      => Some(ElemType::Bool),
            "hex"       => Some(ElemType::Hex),
            "blob"      => Some(ElemType::Blob),
            "regex"     => Some(ElemType::Regex),
            "object"    => Some(ElemType::Object),
            "date"      => Some(ElemType::Date),
            "timestamp" => Some(ElemType::Timestamp),
            "enum"      => Some(ElemType::Enum),
            "any"       => Some(ElemType::Any),
            "array"     => Some(ElemType::Array),
            "tuple"     => Some(ElemType::Tuple),
            _           => None,
        }
    }
}

impl std::fmt::Display for ElemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ElemType::Int       => "int",
            ElemType::Long      => "long",
            ElemType::Float     => "float",
            ElemType::Double    => "double",
            ElemType::String    => "string",
            ElemType::Bool      => "bool",
            ElemType::Hex       => "hex",
            ElemType::Blob      => "blob",
            ElemType::Regex     => "regex",
            ElemType::Object    => "object",
            ElemType::Date      => "date",
            ElemType::Timestamp => "timestamp",
            ElemType::Enum      => "enum",
            ElemType::Any       => "any",
            ElemType::Array     => "array",
            ElemType::Tuple     => "tuple",
        };
        write!(f, "{}", s)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DataType
// ─────────────────────────────────────────────────────────────────────────────

/// All supported data types for DixScript.
///
/// ## Typed-collection variants (v1.0.0+)
///
/// `TypedArray(ElemType)` and `TypedTuple([Option<ElemType>; 6])` are added to
/// support the `<array<int>>` / `<tuple<int,bool>>` annotation syntax.
///
/// Both variants **remain `Copy`** because `ElemType: Copy` and
/// `[Option<ElemType>; 6]: Copy` (fixed arrays of Copy types are Copy).
/// The largest variant (`TypedTuple`) adds 6 bytes to the enum size —
/// acceptable for a stack-only Copy type.
///
/// ### Compatibility rules
/// - `Array`  is compatible with any `TypedArray(_)` and vice-versa.
/// - `Tuple`  is compatible with any `TypedTuple(_)` and vice-versa.
/// - `TypedArray(T)` is compatible with `TypedArray(U)` only when `T == U`
///   or either side is `ElemType::Any`.
/// - `TypedTuple` → `TypedTuple` is loosely compatible (structural checking
///   of individual element types is deferred to runtime for the first
///   iteration).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataType {
    Int,
    Long,
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
    Range,

    // ── Typed collections ──────────────────────────────────────────────────
    /// `<array<int>>`, `<array<string>>`, `<array<tuple>>`, …
    TypedArray(ElemType),
    /// `<tuple<int,bool>>`, `<tuple<int,bool,string>>`, …  (max 6 elements)
    /// `None` slots are absent / unspecified element positions.
    TypedTuple([Option<ElemType>; 6]),
}

impl DataType {
    // ── Constructors ────────────────────────────────────────────────────────

    /// Create a typed array annotation from an element type.
    ///
    /// ```
    /// let dt = DataType::typed_array(ElemType::Int);  // represents <array<int>>
    /// ```
    #[inline]
    pub fn typed_array(elem: ElemType) -> Self {
        DataType::TypedArray(elem)
    }

    /// Create a typed tuple annotation from a slice of element types (max 6).
    ///
    /// ```
    /// let dt = DataType::typed_tuple_from(&[ElemType::Int, ElemType::Bool]);
    /// // represents <tuple<int,bool>>
    /// ```
    pub fn typed_tuple_from(elems: &[ElemType]) -> Self {
        let mut arr = [None; 6];
        for (i, &e) in elems.iter().enumerate().take(6) {
            arr[i] = Some(e);
        }
        DataType::TypedTuple(arr)
    }

    // ── Classification helpers ──────────────────────────────────────────────

    /// Returns `true` for any array type — typed (`TypedArray`) or plain (`Array`).
    #[inline]
    pub fn is_array(self) -> bool {
        matches!(self, DataType::Array | DataType::TypedArray(_))
    }

    /// Returns `true` for any tuple type — typed (`TypedTuple`) or plain (`Tuple`).
    #[inline]
    pub fn is_tuple(self) -> bool {
        matches!(self, DataType::Tuple | DataType::TypedTuple(_))
    }

    /// Strip typed-collection wrapper; `TypedArray(_)` → `Array`,
    /// `TypedTuple(_)` → `Tuple`, everything else unchanged.
    #[inline]
    pub fn base_collection_type(self) -> DataType {
        match self {
            DataType::TypedArray(_) => DataType::Array,
            DataType::TypedTuple(_) => DataType::Tuple,
            other => other,
        }
    }

    // ── Element-type accessors ──────────────────────────────────────────────

    /// Returns the element type if `self` is `TypedArray(_)`.
    #[inline]
    pub fn array_element_type(self) -> Option<ElemType> {
        if let DataType::TypedArray(e) = self { Some(e) } else { None }
    }

    /// Returns the defined element types if `self` is `TypedTuple(_)`.
    /// Absent (None) slots are omitted from the returned `Vec`.
    pub fn tuple_element_types(self) -> Option<Vec<ElemType>> {
        if let DataType::TypedTuple(arr) = self {
            Some(arr.iter().filter_map(|&e| e).collect())
        } else {
            None
        }
    }

    /// Returns the element count of a `TypedTuple` (None slots excluded),
    /// or `None` if `self` is not a `TypedTuple`.
    pub fn typed_tuple_len(self) -> Option<usize> {
        if let DataType::TypedTuple(arr) = self {
            Some(arr.iter().filter(|e| e.is_some()).count())
        } else {
            None
        }
    }
}

impl std::fmt::Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataType::Int       => write!(f, "int"),
            DataType::Long      => write!(f, "long"),
            DataType::Float     => write!(f, "float"),
            DataType::Double    => write!(f, "double"),
            DataType::String    => write!(f, "string"),
            DataType::Bool      => write!(f, "bool"),
            DataType::Array     => write!(f, "array"),
            DataType::Tuple     => write!(f, "tuple"),
            DataType::Hex       => write!(f, "hex"),
            DataType::Blob      => write!(f, "blob"),
            DataType::Regex     => write!(f, "regex"),
            DataType::Object    => write!(f, "object"),
            DataType::Timestamp => write!(f, "timestamp"),
            DataType::Date      => write!(f, "date"),
            DataType::Enum      => write!(f, "enum"),
            DataType::Any       => write!(f, "any"),
            DataType::Function  => write!(f, "function"),
            DataType::Range     => write!(f, "range"),

            DataType::TypedArray(elem) => write!(f, "array<{}>", elem),

            DataType::TypedTuple(arr) => {
                let types: Vec<ElemType> = arr.iter().filter_map(|&e| e).collect();
                write!(f, "tuple<")?;
                for (i, t) in types.iter().enumerate() {
                    if i > 0 { write!(f, ",")?; }
                    write!(f, "{}", t)?;
                }
                write!(f, ">")
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Error handling strategies
// ─────────────────────────────────────────────────────────────────────────────

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
            ErrorHandlingStrategy::Halt     => "halt",
            ErrorHandlingStrategy::Continue => "continue",
            ErrorHandlingStrategy::Recover  => "recover",
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
            CompatibilityMode::Strict      => "strict",
            CompatibilityMode::BestEffort  => "best_effort",
            CompatibilityMode::Permissive  => "permissive",
        };
        write!(f, "{}", s)
    }
}

/// Debug modes for CONFIG section
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DebugMode {
    Off,
    Regular,
    Verbose,
}

impl std::fmt::Display for DebugMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DebugMode::Off     => "off",
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
            DLMModuleType::DAuditor    => "DAuditor",
            DLMModuleType::DEncryptor  => "DEncryptor",
            DLMModuleType::ParseError  => "ParseError",
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
            DLMModuleSubtype::Gzip       => "gzip",
            DLMModuleSubtype::Bzip2      => "bzip2",
            DLMModuleSubtype::Lzma       => "lzma",
            DLMModuleSubtype::Diy        => "diy",
            DLMModuleSubtype::Enhanced   => "enhanced",
            DLMModuleSubtype::Xor        => "xor",
            DLMModuleSubtype::Aes128     => "aes128",
            DLMModuleSubtype::Aes256     => "aes256",
            DLMModuleSubtype::Chacha20   => "chacha20",
            DLMModuleSubtype::ParseError => "ParseError",
        };
        write!(f, "{}", s)
    }
}

/// Variable declaration types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclarationType {
    Let,
    Const,
}

impl std::fmt::Display for DeclarationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DeclarationType::Let   => "let",
            DeclarationType::Const => "const",
        };
        write!(f, "{}", s)
    }
}
