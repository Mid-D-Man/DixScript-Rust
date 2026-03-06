//! Section readers for binary format deserialization.
//!
//! Each reader handles one DixScript section: CONFIG, ENUMS, DATA, SECURITY.
//! IMPORTS, QUICKFUNCS, and DLM are compile-time only and are not stored in binary.

mod config_section_reader;
mod enums_section_reader;
mod data_section_reader;
mod security_section_reader;

pub use config_section_reader::ConfigSectionReader;
pub use enums_section_reader::EnumsSectionReader;
pub use data_section_reader::DataSectionReader;
pub use security_section_reader::SecuritySectionReader;

pub(crate) use super::binary_format;
pub(crate) use super::section_offset;
pub(crate) use super::binary_serialization_context;
pub(crate) use super::binary_serialization_error;
pub(crate) use super::value_decoder;
