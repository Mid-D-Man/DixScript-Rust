//! SHA-256 checksum validation for binary files

use sha2::{Sha256, Digest};
use crate::ErrorManager::ErrorTypes::BinarySerializationErrorType;

/// Handles SHA-256 checksum calculation and validation
pub struct ChecksumValidator;

impl ChecksumValidator {
    /// Calculate SHA-256 checksum for byte array
    pub fn calculate(data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    /// Append checksum to binary data
    /// Returns new vector with checksum appended
    pub fn append_checksum(data: &[u8]) -> Vec<u8> {
        let checksum = Self::calculate(data);
        let mut result = Vec::with_capacity(data.len() + 32);
        result.extend_from_slice(data);
        result.extend_from_slice(&checksum);
        result
    }

    /// Extract checksum from end of binary data
    /// Returns (data without checksum, extracted checksum)
    pub fn extract_checksum(data_with_checksum: &[u8]) -> Result<(&[u8], [u8; 32]), String> {
        if data_with_checksum.len() < 32 {
            return Err(format!(
                "Data too short to contain checksum: {} bytes",
                data_with_checksum.len()
            ));
        }

        let split_point = data_with_checksum.len() - 32;
        let data = &data_with_checksum[..split_point];
        let checksum = &data_with_checksum[split_point..];
        
        let mut checksum_array = [0u8; 32];
        checksum_array.copy_from_slice(checksum);

        Ok((data, checksum_array))
    }

    /// Validate checksum of binary data
    pub fn validate(data: &[u8], expected_checksum: &[u8; 32]) -> bool {
        let actual_checksum = Self::calculate(data);
        
        // Constant-time comparison to prevent timing attacks
        let mut result = 0u8;
        for i in 0..32 {
            result |= actual_checksum[i] ^ expected_checksum[i];
        }
        
        result == 0
    }

    /// Validate data with embedded checksum
    /// Returns data without checksum if valid
    pub fn validate_and_extract(data_with_checksum: &[u8]) -> Result<Vec<u8>, String> {
        let (data, embedded_checksum) = Self::extract_checksum(data_with_checksum)?;

        if !Self::validate(data, &embedded_checksum) {
            return Err("Data integrity check failed - checksum mismatch".to_string());
        }

        Ok(data.to_vec())
    }

    /// Convert checksum to hex string
    pub fn to_hex_string(checksum: &[u8; 32]) -> String {
        checksum.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    }

    /// Parse hex string to checksum
    pub fn from_hex_string(hex: &str) -> Result<[u8; 32], String> {
        let hex = hex.replace(['-', ' '], "");
        
        if hex.len() != 64 {
            return Err(format!(
                "Invalid hex string length: {} (expected 64 characters for SHA-256)",
                hex.len()
            ));
        }

        let mut checksum = [0u8; 32];
        for i in 0..32 {
            checksum[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|e| format!("Invalid hex character: {}", e))?;
        }

        Ok(checksum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_round_trip() {
        let data = b"Hello, DixScript!";
        let checksum = ChecksumValidator::calculate(data);
        assert!(ChecksumValidator::validate(data, &checksum));
    }

    #[test]
    fn test_append_and_extract() {
        let data = b"Test data";
        let with_checksum = ChecksumValidator::append_checksum(data);
        let extracted = ChecksumValidator::validate_and_extract(&with_checksum).unwrap();
        assert_eq!(extracted, data);
    }

    #[test]
    fn test_hex_conversion() {
        let checksum = [0xAB; 32];
        let hex = ChecksumValidator::to_hex_string(&checksum);
        assert_eq!(hex.len(), 64);
        let parsed = ChecksumValidator::from_hex_string(&hex).unwrap();
        assert_eq!(parsed, checksum);
    }
      }
