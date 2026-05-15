//! Binary format constants and type definitions for DixScript v1.0.0
//! Defines the structure and type tags for .dixscript.enc files

use crate::Compiler::AST::{DataType, Position};

// ==================== MAGIC NUMBER AND VERSION ====================

/// Magic number for MDIX binary files: "MDIX" in ASCII
pub const MAGIC_NUMBER: u32 = 0x4D444958;

/// Binary format version (1.0.0)
pub const VERSION_MAJOR: u8 = 1;
pub const VERSION_MINOR: u8 = 0;
pub const VERSION_PATCH: u8 = 0;

// ==================== HEADER SIZES ====================

pub const HEADER_SIZE: usize = 16;
pub const FOOTER_SIZE: usize = 32; // SHA-256 checksum
pub const OFFSET_ENTRY_SIZE: usize = 12;

// ==================== SECTION FLAGS (1 byte) ====================

bitflags::bitflags! {
    /// Section presence flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SectionFlags: u8 {
        const NONE     = 0x00;
        const CONFIG   = 0x01;  // Bit 0
        const ENUMS    = 0x02;  // Bit 1
        const DATA     = 0x04;  // Bit 2
        const SECURITY = 0x08;  // Bit 3
        const IMPORTS  = 0x10;  // Bit 4
        const RESERVED_6 = 0x40;
        const RESERVED_7 = 0x80;
    }
}

// ==================== SECTION IDS ====================

/// Section identifiers
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SectionId {
    Config = 0x00000001,
    Enums = 0x00000002,
    Data = 0x00000003,
    Security = 0x00000004,
    Imports = 0x00000005,
}

impl SectionId {
    /// Get section name for debugging
    pub fn name(&self) -> &'static str {
        match self {
            SectionId::Config => "@CONFIG",
            SectionId::Enums => "@ENUMS",
            SectionId::Data => "@DATA",
            SectionId::Security => "@SECURITY",
            SectionId::Imports => "@IMPORTS",
        }
    }

    /// Try to convert u32 to SectionId
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0x00000001 => Some(SectionId::Config),
            0x00000002 => Some(SectionId::Enums),
            0x00000003 => Some(SectionId::Data),
            0x00000004 => Some(SectionId::Security),
            0x00000005 => Some(SectionId::Imports),
            _ => None,
        }
    }
}

// ==================== VALUE TYPE TAGS ====================

/// Binary type tags for values
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueTypeTag {
    Int32   = 0x01,
    Float32 = 0x02,
    Float64 = 0x03,
    String  = 0x04,
    Bool    = 0x05,
    Null    = 0x06,
    Array   = 0x07,
    Object  = 0x08,
    Date    = 0x09,
    Timestamp = 0x0A,
    Hex     = 0x0B,
    Tuple   = 0x0C,
    Blob    = 0x0D,
    Regex   = 0x0E,
    Int64   = 0x10,   // Long (i64)
    Reserved15 = 0x0F,
    Invalid = 0xFF,
}

impl ValueTypeTag {
    pub fn name(&self) -> &'static str {
        match self {
            ValueTypeTag::Int32      => "int",
            ValueTypeTag::Int64      => "long",
            ValueTypeTag::Float32    => "float",
            ValueTypeTag::Float64    => "double",
            ValueTypeTag::String     => "string",
            ValueTypeTag::Bool       => "bool",
            ValueTypeTag::Null       => "null",
            ValueTypeTag::Array      => "array",
            ValueTypeTag::Object     => "object",
            ValueTypeTag::Tuple      => "tuple",
            ValueTypeTag::Date       => "date",
            ValueTypeTag::Timestamp  => "timestamp",
            ValueTypeTag::Hex        => "hex",
            ValueTypeTag::Blob       => "blob",
            ValueTypeTag::Regex      => "regex",
            ValueTypeTag::Reserved15 => "reserved",
            ValueTypeTag::Invalid    => "invalid",
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(ValueTypeTag::Int32),
            0x02 => Some(ValueTypeTag::Float32),
            0x03 => Some(ValueTypeTag::Float64),
            0x04 => Some(ValueTypeTag::String),
            0x05 => Some(ValueTypeTag::Bool),
            0x06 => Some(ValueTypeTag::Null),
            0x07 => Some(ValueTypeTag::Array),
            0x08 => Some(ValueTypeTag::Object),
            0x09 => Some(ValueTypeTag::Date),
            0x0A => Some(ValueTypeTag::Timestamp),
            0x0B => Some(ValueTypeTag::Hex),
            0x0C => Some(ValueTypeTag::Tuple),
            0x0D => Some(ValueTypeTag::Blob),
            0x0E => Some(ValueTypeTag::Regex),
            0x0F => Some(ValueTypeTag::Reserved15),
            0x10 => Some(ValueTypeTag::Int64),
            0xFF => Some(ValueTypeTag::Invalid),
            _    => None,
        }
    }

    pub fn from_data_type(data_type: DataType) -> Self {
        match data_type {
            DataType::Int       => ValueTypeTag::Int32,
            DataType::Long      => ValueTypeTag::Int64,
            DataType::Float     => ValueTypeTag::Float32,
            DataType::Double    => ValueTypeTag::Float64,
            DataType::String    => ValueTypeTag::String,
            DataType::Bool      => ValueTypeTag::Bool,
            DataType::Array     => ValueTypeTag::Array,
            DataType::Tuple     => ValueTypeTag::Tuple,
            DataType::Object    => ValueTypeTag::Object,
            DataType::Date      => ValueTypeTag::Date,
            DataType::Timestamp => ValueTypeTag::Timestamp,
            DataType::Hex       => ValueTypeTag::Hex,
            DataType::Blob      => ValueTypeTag::Blob,
            DataType::Regex     => ValueTypeTag::Regex,
            _                   => ValueTypeTag::Invalid,
        }
    }
}
// ==================== VALIDATION CONSTANTS ====================

pub const MAX_STRING_LENGTH: usize = 1024 * 1024; // 1 MB
pub const MAX_ARRAY_LENGTH: usize = 1024 * 1024; // 1M elements
pub const MAX_OBJECT_PROPERTIES: usize = 100_000; // 100K properties
pub const MAX_NESTING_DEPTH: usize = 5; // Match DixScript limit

// ==================== BLOB ENCODING ====================

/// Blob encoding formats
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobEncoding {
    Base64 = 0x01,
    Base32 = 0x02,
    Hex = 0x03,
    Raw = 0x04,
    Auto = 0xFF,
}

impl BlobEncoding {
    /// Detect blob encoding from string content
    pub fn detect(data: &str) -> Self {
        // Hex detection (only 0-9, A-F, a-f)
        if data.chars().all(|c| c.is_ascii_hexdigit()) {
            return BlobEncoding::Hex;
        }

        // Base32 detection (A-Z, 2-7, padding =)
        if data.chars().all(|c| {
            matches!(c, 'A'..='Z' | '2'..='7' | '=')
        }) {
            return BlobEncoding::Base32;
        }

        // Base64 default
        BlobEncoding::Base64
    }

    /// Validate blob data for this encoding
    pub fn validate(&self, data: &str) -> bool {
        match self {
            BlobEncoding::Base64 => {
                use base64::{Engine as _, engine::general_purpose};
                general_purpose::STANDARD.decode(data).is_ok()
            }
            BlobEncoding::Hex => {
                data.len() % 2 == 0 && data.chars().all(|c| c.is_ascii_hexdigit())
            }
            BlobEncoding::Base32 => {
                data.chars().all(|c| matches!(c, 'A'..='Z' | '2'..='7' | '='))
            }
            BlobEncoding::Raw => true,
            BlobEncoding::Auto => false,
        }
    }
}

// ==================== HELPER FUNCTIONS ====================

/// Validate magic number
pub fn is_valid_magic_number(magic: u32) -> bool {
    magic == MAGIC_NUMBER
}

/// Validate format version
pub fn is_valid_version(major: u8, minor: u8, patch: u8) -> bool {
    major == VERSION_MAJOR && minor == VERSION_MINOR && patch == VERSION_PATCH
}
