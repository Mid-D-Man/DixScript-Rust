//! Binary serialization for .mdix files
//!
//! This module provides binary serialization/deserialization for DixScript AST
//! into the .mdix.enc binary format with SHA-256 checksums and compression.
//!
//! ## Architecture
//! - `BinaryPacker` - Serializes AST to binary format
//! - `BinaryUnpacker` - Deserializes binary format to AST
//! - `ValueEncoder/ValueDecoder` - Handles value-level encoding/decoding
//! - `SectionWriters` - Section-specific binary writers
//! - `SectionReaders` - Section-specific binary readers
//! - `ChecksumValidator` - SHA-256 integrity validation
//!
//! ## Binary Format Structure
//! ```text
//! [Header: 16 bytes]
//! [Sections: variable]
//! [Offset Table: 12 bytes per section]
//! [Checksum: 32 bytes SHA-256]
//! ```

// Core format definitions
pub mod binary_format;
pub mod binary_header;
pub mod section_offset;

// Serialization/Deserialization orchestrators
pub mod binary_packer;
pub mod binary_unpacker;

// Value encoding/decoding
pub mod value_encoder;
pub mod value_decoder;

// Section-specific handlers
pub mod SectionReaders;
pub mod SectionWriters;

// Context and state management
pub mod binary_serialization_context;
pub mod binary_serialization_error;
pub mod binary_serialization_result;

// Validation
pub mod checksum_validator;

// Re-exports for convenience
pub use binary_format::{
    MAGIC_NUMBER, VERSION_MAJOR, VERSION_MINOR, VERSION_PATCH,
    SectionId, SectionFlags, ValueTypeTag, BlobEncoding,
    MAX_STRING_LENGTH, MAX_ARRAY_LENGTH, MAX_OBJECT_PROPERTIES, MAX_NESTING_DEPTH,
};

pub use binary_header::BinaryHeader;
pub use section_offset::SectionOffset;

pub use binary_packer::BinaryPacker;
pub use binary_unpacker::BinaryUnpacker;

pub use value_encoder::ValueEncoder;
pub use value_decoder::ValueDecoder;

pub use binary_serialization_context::{
    BinarySerializationContext,
    BinarySerializationStatistics,
    BinaryDeserializationStatistics,
};

pub use binary_serialization_error::BinarySerializationError;

pub use binary_serialization_result::{
    BinarySerializationResult,
    BinaryDeserializationResult,
};

pub use checksum_validator::ChecksumValidator;

// Re-export section writers
pub use SectionWriters::{
    ConfigSectionWriter,
    EnumsSectionWriter,
    DataSectionWriter,
    SecuritySectionWriter,
    ImportsSectionWriter,
};

// Re-export section readers
pub use SectionReaders::{
    ConfigSectionReader,
    EnumsSectionReader,
    DataSectionReader,
    SecuritySectionReader,
    ImportsSectionReader,
};