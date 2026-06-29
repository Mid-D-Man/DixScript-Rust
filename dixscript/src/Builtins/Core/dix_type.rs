
use crate::Compiler::AST::DataType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DixType {
    Int,
    Long,     // ← NEW: 64-bit integer
    Float,
    Double,
    String,
    Bool,
    Array,
    Tuple,
    Object,
    Hex,
    Blob,
    Regex,
    Date,
    Timestamp,
    Enum,
    Null,
    Void,
    Any,
}

impl DixType {
    #[inline]
    pub const fn is_numeric(self) -> bool {
        matches!(self, DixType::Int | DixType::Long | DixType::Float | DixType::Double)
    }

    #[inline]
    pub const fn is_collection(self) -> bool {
        matches!(self, DixType::Array | DixType::Tuple | DixType::Object)
    }

    #[inline]
    pub const fn is_indexable(self) -> bool {
        matches!(self, DixType::Array | DixType::Object | DixType::String | DixType::Tuple)
    }

    #[inline]
    pub const fn is_comparable(self) -> bool {
        matches!(
            self,
            DixType::Int | DixType::Long | DixType::Float | DixType::Double
                | DixType::String | DixType::Date | DixType::Timestamp | DixType::Bool
        )
    }

    #[inline]
    pub const fn is_arithmetic(self) -> bool { self.is_numeric() }

    pub fn get_type_name(self) -> &'static str {
        match self {
            DixType::Int       => "int",
            DixType::Long      => "long",
            DixType::Float     => "float",
            DixType::Double    => "double",
            DixType::String    => "string",
            DixType::Bool      => "bool",
            DixType::Array     => "array",
            DixType::Tuple     => "tuple",
            DixType::Object    => "object",
            DixType::Hex       => "hex",
            DixType::Blob      => "blob",
            DixType::Regex     => "regex",
            DixType::Date      => "date",
            DixType::Timestamp => "timestamp",
            DixType::Enum      => "enum",
            DixType::Null      => "null",
            DixType::Void      => "void",
            DixType::Any       => "any",
        }
    }

    pub fn from_ast_data_type(ast_type: DataType) -> Self {
        match ast_type {
            DataType::Int       => DixType::Int,
            DataType::Long      => DixType::Long,
            DataType::Float     => DixType::Float,
            DataType::Double    => DixType::Double,
            DataType::String    => DixType::String,
            DataType::Bool      => DixType::Bool,
            DataType::Array     => DixType::Array,
            DataType::Tuple     => DixType::Tuple,
            DataType::Object    => DixType::Object,
            DataType::Hex       => DixType::Hex,
            DataType::Blob      => DixType::Blob,
            DataType::Regex     => DixType::Regex,
            DataType::Date      => DixType::Date,
            DataType::Timestamp => DixType::Timestamp,
            DataType::Enum      => DixType::Enum,
            _                   => DixType::Null,
        }
    }

    pub fn to_ast_data_type(self) -> Result<DataType, String> {
        match self {
            DixType::Int       => Ok(DataType::Int),
            DixType::Long      => Ok(DataType::Long),
            DixType::Float     => Ok(DataType::Float),
            DixType::Double    => Ok(DataType::Double),
            DixType::String    => Ok(DataType::String),
            DixType::Bool      => Ok(DataType::Bool),
            DixType::Array     => Ok(DataType::Array),
            DixType::Tuple     => Ok(DataType::Tuple),
            DixType::Object    => Ok(DataType::Object),
            DixType::Hex       => Ok(DataType::Hex),
            DixType::Blob      => Ok(DataType::Blob),
            DixType::Regex     => Ok(DataType::Regex),
            DixType::Date      => Ok(DataType::Date),
            DixType::Timestamp => Ok(DataType::Timestamp),
            DixType::Enum      => Ok(DataType::Enum),
            DixType::Any       => Ok(DataType::Any),
            DixType::Null | DixType::Void => {
                Err(format!("Cannot convert DixType::{:?} to AST DataType", self))
            }
        }
    }

    pub fn can_convert_to(self, to: DixType) -> bool {
        if self == to              { return true; }
        if self == DixType::Null   { return true; }
        if self.is_numeric() && to.is_numeric() { return true; }
        if to == DixType::String   { return true; }
        matches!(
            (self, to),
            (DixType::String, DixType::Regex)
                | (DixType::String, DixType::Date)
                | (DixType::String, DixType::Timestamp)
                | (DixType::Date,      DixType::Timestamp)
                | (DixType::Timestamp, DixType::Date)
        )
    }

    pub fn get_common_type(self, other: DixType) -> DixType {
        if self == other           { return self; }
        if self == DixType::Null   { return other; }
        if other == DixType::Null  { return self; }

        if self.is_numeric() && other.is_numeric() {
            if self == DixType::Double || other == DixType::Double { return DixType::Double; }
            if self == DixType::Float  || other == DixType::Float  { return DixType::Float; }
            if self == DixType::Long   || other == DixType::Long   { return DixType::Long; }
            return DixType::Int;
        }

        if (self == DixType::Date && other == DixType::Timestamp)
            || (self == DixType::Timestamp && other == DixType::Date)
        {
            return DixType::Timestamp;
        }

        DixType::String
    }
}

impl std::fmt::Display for DixType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_type_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_numeric_includes_long() {
        assert!(DixType::Long.is_numeric());
        assert!(DixType::Int.is_numeric());
        assert!(!DixType::String.is_numeric());
    }

    #[test]
    fn test_common_type_long_wins_over_int() {
        assert_eq!(DixType::Int.get_common_type(DixType::Long), DixType::Long);
        assert_eq!(DixType::Long.get_common_type(DixType::Int), DixType::Long);
    }

    #[test]
    fn test_common_type_double_wins_over_long() {
        assert_eq!(DixType::Long.get_common_type(DixType::Double), DixType::Double);
    }

    #[test]
    fn test_long_can_convert_to_double() {
        assert!(DixType::Long.can_convert_to(DixType::Double));
        assert!(DixType::Long.can_convert_to(DixType::String));
    }

    #[test]
    fn test_type_name() {
        assert_eq!(DixType::Long.get_type_name(), "long");
        assert_eq!(DixType::Int.get_type_name(), "int");
    }
}
