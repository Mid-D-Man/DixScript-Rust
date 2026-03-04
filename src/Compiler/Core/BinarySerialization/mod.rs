//! Binary serialization for .mdix files.
//!
//! Serialized sections: CONFIG, ENUMS, DATA, SECURITY.
//! IMPORTS, QUICKFUNCS, and DLM are compile-time only and are not part of the binary format.
//!
//! ## Binary format layout
//! ```text
//! [Header: 16 bytes]
//! [Sections: variable — CONFIG, ENUMS, DATA, SECURITY in canonical order]
//! [Offset Table: 12 bytes per section]
//! [Checksum: 32 bytes SHA-256]
//! ```

pub mod binary_format;
pub mod binary_header;
pub mod section_offset;
pub mod binary_packer;
pub mod binary_unpacker;
pub mod value_encoder;
pub mod value_decoder;
pub mod SectionReaders;
pub mod SectionWriters;
pub mod binary_serialization_context;
pub mod binary_serialization_error;
pub mod binary_serialization_result;
pub mod checksum_validator;

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
pub use binary_serialization_result::{BinarySerializationResult, BinaryDeserializationResult};
pub use checksum_validator::ChecksumValidator;

pub use SectionWriters::{
    ConfigSectionWriter,
    EnumsSectionWriter,
    DataSectionWriter,
    SecuritySectionWriter,
};
pub use SectionReaders::{
    ConfigSectionReader,
    EnumsSectionReader,
    DataSectionReader,
    SecuritySectionReader,
};
