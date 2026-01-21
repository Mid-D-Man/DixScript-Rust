// src/Builtins/Core/dix_type.rs
//! DixType - Type system for DixScript runtime values
//!
//! Maps to DataType from AST but provides runtime/builtin compatibility.
//! This is a Copy type (4 bytes) so passing by value is zero-cost.

use crate::Compiler::AST::DataType;

/// Type system for DixScript values at runtime
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DixType {
    // Numeric types
    Int,
    Float,
    Double,

    // String type
    String,

    // Boolean type
    Bool,

    // Collection types
    Array,
    Tuple,
    Object,

    // Special data types
    Hex,
    Blob,
    Regex,

    // Date/Time types
    Date,
    Timestamp,

    // Enum type
    Enum,

    // Special types
    Null,
    Void,
}

impl DixType {
    /// Check if type is numeric (int, float, double)
    #[inline]
    pub const fn is_numeric(self) -> bool {
        matches!(self, DixType::Int | DixType::Float | DixType::Double)
    }

    /// Check if type is a collection (array, tuple, object)
    #[inline]
    pub const fn is_collection(self) -> bool {
        matches!(self, DixType::Array | DixType::Tuple | DixType::Object)
    }

    /// Check if type supports indexing/property access
    #[inline]
    pub const fn is_indexable(self) -> bool {
        matches!(
            self,
            DixType::Array | DixType::Object | DixType::String | DixType::Tuple
        )
    }

    /// Check if type is comparable (supports <, >, ==, etc.)
    #[inline]
    pub const fn is_comparable(self) -> bool {
        matches!(
            self,
            DixType::Int
                | DixType::Float
                | DixType::Double
                | DixType::String
                | DixType::Date
                | DixType::Timestamp
                | DixType::Bool
        )
    }

    /// Check if type supports arithmetic operations
    #[inline]
    pub const fn is_arithmetic(self) -> bool {
        self.is_numeric()
    }

    /// Get the string representation of the type name (lowercase)
    pub fn get_type_name(self) -> &'static str {
        match self {
            DixType::Int => "int",
            DixType::Float => "float",
            DixType::Double => "double",
            DixType::String => "string",
            DixType::Bool => "bool",
            DixType::Array => "array",
            DixType::Tuple => "tuple",
            DixType::Object => "object",
            DixType::Hex => "hex",
            DixType::Blob => "blob",
            DixType::Regex => "regex",
            DixType::Date => "date",
            DixType::Timestamp => "timestamp",
            DixType::Enum => "enum",
            DixType::Null => "null",
            DixType::Void => "void",
        }
    }

    /// Convert from AST DataType to DixType
    pub fn from_ast_data_type(ast_type: DataType) -> Self {
        match ast_type {
            DataType::Int => DixType::Int,
            DataType::Float => DixType::Float,
            DataType::Double => DixType::Double,
            DataType::String => DixType::String,
            DataType::Bool => DixType::Bool,
            DataType::Array => DixType::Array,
            DataType::Tuple => DixType::Tuple,
            DataType::Object => DixType::Object,
            DataType::Hex => DixType::Hex,
            DataType::Blob => DixType::Blob,
            DataType::Regex => DixType::Regex,
            DataType::Date => DixType::Date,
            DataType::Timestamp => DixType::Timestamp,
            DataType::Enum => DixType::Enum,
            _ => DixType::Null,
        }
    }

    /// Convert from DixType to AST DataType
    /// Returns Err if conversion not possible (Null, Void)
    pub fn to_ast_data_type(self) -> Result<DataType, String> {
        match self {
            DixType::Int => Ok(DataType::Int),
            DixType::Float => Ok(DataType::Float),
            DixType::Double => Ok(DataType::Double),
            DixType::String => Ok(DataType::String),
            DixType::Bool => Ok(DataType::Bool),
            DixType::Array => Ok(DataType::Array),
            DixType::Tuple => Ok(DataType::Tuple),
            DixType::Object => Ok(DataType::Object),
            DixType::Hex => Ok(DataType::Hex),
            DixType::Blob => Ok(DataType::Blob),
            DixType::Regex => Ok(DataType::Regex),
            DixType::Date => Ok(DataType::Date),
            DixType::Timestamp => Ok(DataType::Timestamp),
            DixType::Enum => Ok(DataType::Enum),
            DixType::Null | DixType::Void => {
                Err(format!("Cannot convert DixType::{:?} to AST DataType", self))
            }
        }
    }

    /// Check if one type can be implicitly converted to another
    pub fn can_convert_to(self, to: DixType) -> bool {
        // Same type
        if self == to {
            return true;
        }

        // Null can convert to anything
        if self == DixType::Null {
            return true;
        }

        // Numeric conversions
        if self.is_numeric() && to.is_numeric() {
            return true;
        }

        // Everything can convert to string
        if to == DixType::String {
            return true;
        }

        // Specific conversions
        matches!(
            (self, to),
            (DixType::String, DixType::Regex)
                | (DixType::String, DixType::Date)
                | (DixType::String, DixType::Timestamp)
                | (DixType::Date, DixType::Timestamp)
                | (DixType::Timestamp, DixType::Date)
        )
    }

    /// Get the most specific common type between two types
    pub fn get_common_type(self, other: DixType) -> DixType {
        if self == other {
            return self;
        }

        // One is null
        if self == DixType::Null {
            return other;
        }
        if other == DixType::Null {
            return self;
        }

        // Both numeric - return most general
        if self.is_numeric() && other.is_numeric() {
            if self == DixType::Double || other == DixType::Double {
                return DixType::Double;
            }
            if self == DixType::Float || other == DixType::Float {
                return DixType::Float;
            }
            return DixType::Int;
        }

        // Date/timestamp combinations
        if (self == DixType::Date && other == DixType::Timestamp)
            || (self == DixType::Timestamp && other == DixType::Date)
        {
            return DixType::Timestamp;
        }

        // Default to string for mixed types
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
    fn test_is_numeric() {
        assert!(DixType::Int.is_numeric());
        assert!(DixType::Float.is_numeric());
        assert!(DixType::Double.is_numeric());
        assert!(!DixType::String.is_numeric());
    }

    #[test]
    fn test_is_collection() {
        assert!(DixType::Array.is_collection());
        assert!(DixType::Tuple.is_collection());
        assert!(DixType::Object.is_collection());
        assert!(!DixType::Int.is_collection());
    }

    #[test]
    fn test_can_convert_to() {
        // Same type
        assert!(DixType::Int.can_convert_to(DixType::Int));

        // Null to anything
        assert!(DixType::Null.can_convert_to(DixType::String));

        // Numeric conversions
        assert!(DixType::Int.can_convert_to(DixType::Float));
        assert!(DixType::Float.can_convert_to(DixType::Double));

        // Everything to string
        assert!(DixType::Int.can_convert_to(DixType::String));
        assert!(DixType::Bool.can_convert_to(DixType::String));

        // Date/Timestamp
        assert!(DixType::Date.can_convert_to(DixType::Timestamp));
        assert!(DixType::Timestamp.can_convert_to(DixType::Date));
    }

    #[test]
    fn test_get_common_type() {
        assert_eq!(DixType::Int.get_common_type(DixType::Int), DixType::Int);
        assert_eq!(DixType::Int.get_common_type(DixType::Float), DixType::Float);
        assert_eq!(
            DixType::Float.get_common_type(DixType::Double),
            DixType::Double
        );
        assert_eq!(
            DixType::String.get_common_type(DixType::Bool),
            DixType::String
        );
    }

    #[test]
    fn test_type_name() {
        assert_eq!(DixType::Int.get_type_name(), "int");
        assert_eq!(DixType::String.get_type_name(), "string");
        assert_eq!(DixType::Array.get_type_name(), "array");
    }
}