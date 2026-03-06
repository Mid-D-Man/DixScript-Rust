//! Section writers for binary format serialization.
//!
//! Each writer handles one DixScript section: CONFIG, ENUMS, DATA, SECURITY.
//! IMPORTS, QUICKFUNCS, and DLM are compile-time only and are not serialized.

mod config_section_writer;
mod enums_section_writer;
mod data_section_writer;
mod security_section_writer;

pub use config_section_writer::ConfigSectionWriter;
pub use enums_section_writer::EnumsSectionWriter;
pub use data_section_writer::DataSectionWriter;
pub use security_section_writer::SecuritySectionWriter;

pub(crate) use super::binary_format;
pub(crate) use super::section_offset;
pub(crate) use super::binary_serialization_context;
pub(crate) use super::binary_serialization_error;
pub(crate) use super::value_encoder;
