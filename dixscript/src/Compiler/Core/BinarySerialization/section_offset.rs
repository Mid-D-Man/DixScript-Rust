//! Section offset table entry (12 bytes)

use std::io::{Read, Write, Result as IoResult};
use super::binary_format::{SectionId, HEADER_SIZE};

/// Section offset table entry (12 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionOffset {
    /// Section identifier
    pub section_id: SectionId,
    
    /// Absolute position in file where section starts
    pub offset: i32,
    
    /// Length of section in bytes
    pub length: i32,
}

impl SectionOffset {
    /// Create new section offset
    pub fn new(section_id: SectionId, offset: i32, length: i32) -> Self {
        SectionOffset {
            section_id,
            offset,
            length,
        }
    }

    /// Validate offset entry
    pub fn validate(&self) -> Result<(), String> {
        if self.offset < HEADER_SIZE as i32 {
            return Err(format!("Invalid offset: {}", self.offset));
        }

        if self.length <= 0 {
            return Err(format!("Invalid length: {}", self.length));
        }

        Ok(())
    }

    /// Write offset entry to binary writer
    pub fn write_to<W: Write>(&self, writer: &mut W) -> IoResult<()> {
        writer.write_all(&(self.section_id as u32).to_le_bytes())?; // 4 bytes
        writer.write_all(&self.offset.to_le_bytes())?; // 4 bytes
        writer.write_all(&self.length.to_le_bytes())?; // 4 bytes
        Ok(())
    }

    /// Read offset entry from binary reader
    pub fn read_from<R: Read>(reader: &mut R) -> IoResult<Self> {
        let mut buf = [0u8; 4];

        // Read section ID
        reader.read_exact(&mut buf)?;
        let section_id_u32 = u32::from_le_bytes(buf);
        let section_id = SectionId::from_u32(section_id_u32)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid section ID: {}", section_id_u32),
                )
            })?;

        // Read offset
        reader.read_exact(&mut buf)?;
        let offset = i32::from_le_bytes(buf);

        // Read length
        reader.read_exact(&mut buf)?;
        let length = i32::from_le_bytes(buf);

        Ok(SectionOffset {
            section_id,
            offset,
            length,
        })
    }
}

impl std::fmt::Display for SectionOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: Offset={}, Length={}",
            self.section_id.name(),
            self.offset,
            self.length
        )
    }
}
