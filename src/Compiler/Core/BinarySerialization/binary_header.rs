//! Binary file header structure (16 bytes fixed)

use std::io::{Read, Write, Result as IoResult};
use super::binary_format::{
    MAGIC_NUMBER, VERSION_MAJOR, VERSION_MINOR, VERSION_PATCH,
    HEADER_SIZE, SectionFlags, is_valid_magic_number, is_valid_version,
};

/// Binary file header (16 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryHeader {
    /// Magic number: 0x4D444958 ("MDIX")
    pub magic_number: u32,
    
    /// Format version
    pub version_major: u8,
    pub version_minor: u8,
    pub version_patch: u8,
    
    /// Section flags (which sections are present)
    pub flags: SectionFlags,
    
    /// Number of sections in file
    pub section_count: i32,
    
    /// Position of offset table in file
    pub offset_table_position: i32,
}

impl BinaryHeader {
    /// Create new header with defaults
    pub fn new() -> Self {
        BinaryHeader {
            magic_number: MAGIC_NUMBER,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            version_patch: VERSION_PATCH,
            flags: SectionFlags::NONE,
            section_count: 0,
            offset_table_position: 0,
        }
    }

    /// Add section flag
    pub fn add_section(&mut self, section: SectionFlags) {
        self.flags.insert(section);
    }

    /// Check if section is present
    pub fn has_section(&self, section: SectionFlags) -> bool {
        self.flags.contains(section)
    }

    /// Validate header integrity
    pub fn validate(&self) -> Result<(), String> {
        if !is_valid_magic_number(self.magic_number) {
            return Err(format!("Invalid magic number: 0x{:08X}", self.magic_number));
        }

        if !is_valid_version(self.version_major, self.version_minor, self.version_patch) {
            return Err(format!(
                "Invalid version: {}.{}.{}",
                self.version_major, self.version_minor, self.version_patch
            ));
        }

        if self.section_count < 0 || self.section_count > 10 {
            return Err(format!("Invalid section count: {}", self.section_count));
        }

        if self.offset_table_position < HEADER_SIZE as i32 {
            return Err(format!(
                "Invalid offset table position: {}",
                self.offset_table_position
            ));
        }

        Ok(())
    }

    /// Write header to binary writer
    pub fn write_to<W: Write>(&self, writer: &mut W) -> IoResult<()> {
        // Total: 16 bytes
        writer.write_all(&self.magic_number.to_le_bytes())?; // 4 bytes
        writer.write_all(&[self.version_major])?; // 1 byte
        writer.write_all(&[self.version_minor])?; // 1 byte
        writer.write_all(&[self.version_patch])?; // 1 byte
        writer.write_all(&[self.flags.bits()])?; // 1 byte
        writer.write_all(&self.section_count.to_le_bytes())?; // 4 bytes
        writer.write_all(&self.offset_table_position.to_le_bytes())?; // 4 bytes
        Ok(())
    }

    /// Read header from binary reader
    pub fn read_from<R: Read>(reader: &mut R) -> IoResult<Self> {
        let mut buf4 = [0u8; 4];
        let mut buf1 = [0u8; 1];

        // Read magic number (4 bytes)
        reader.read_exact(&mut buf4)?;
        let magic_number = u32::from_le_bytes(buf4);

        // Read version (3 bytes)
        reader.read_exact(&mut buf1)?;
        let version_major = buf1[0];
        reader.read_exact(&mut buf1)?;
        let version_minor = buf1[0];
        reader.read_exact(&mut buf1)?;
        let version_patch = buf1[0];

        // Read flags (1 byte)
        reader.read_exact(&mut buf1)?;
        let flags = SectionFlags::from_bits_truncate(buf1[0]);

        // Read section count (4 bytes)
        reader.read_exact(&mut buf4)?;
        let section_count = i32::from_le_bytes(buf4);

        // Read offset table position (4 bytes)
        reader.read_exact(&mut buf4)?;
        let offset_table_position = i32::from_le_bytes(buf4);

        Ok(BinaryHeader {
            magic_number,
            version_major,
            version_minor,
            version_patch,
            flags,
            section_count,
            offset_table_position,
        })
    }
}

impl Default for BinaryHeader {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BinaryHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MDIX Binary Header v{}.{}.{} (Sections: {}, Flags: {:?})",
            self.version_major,
            self.version_minor,
            self.version_patch,
            self.section_count,
            self.flags
        )
    }
  }
