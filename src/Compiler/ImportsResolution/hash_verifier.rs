// src/Compiler/ImportsResolution/hash_verifier.rs

use sha2::{Sha256, Sha512, Digest};
use std::fmt;

/// Hash verification utility for import file integrity
///
/// Supports SHA256 and SHA512 algorithms
/// v1.0.0 - No external packages (uses sha2 crate)
pub struct HashVerifier;

impl HashVerifier {
    /// Verify that file content matches expected hash
    ///
    /// Expected format: "algorithm:hexstring" (e.g., "sha256:abc123...")
    ///
    /// # Errors
    /// - Invalid hash format
    /// - Unsupported algorithm
    /// - Invalid hex string
    /// - Hash mismatch
    pub fn verify_hash(
        content: &str,
        expected_hash: &str,
        alias: &str,
        file_path: &str,
    ) -> Result<(), HashVerificationError> {
        if content.is_empty() {
            return Err(HashVerificationError::new(
                "Content cannot be empty",
                alias,
                file_path,
                expected_hash,
                None,
            ));
        }

        if expected_hash.is_empty() {
            return Err(HashVerificationError::new(
                "Expected hash cannot be empty",
                alias,
                file_path,
                expected_hash,
                None,
            ));
        }

        // Parse hash format: "algorithm:hexstring"
        let parts: Vec<&str> = expected_hash.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(HashVerificationError::new(
                &format!(
                    "Invalid hash format '{}'. Expected format: 'algorithm:hexstring'",
                    expected_hash
                ),
                alias,
                file_path,
                expected_hash,
                None,
            ));
        }

        let algorithm = parts[0].to_lowercase();
        let expected_hex_hash = parts[1].to_lowercase();

        // Validate algorithm
        if algorithm != "sha256" && algorithm != "sha512" {
            return Err(HashVerificationError::new(
                &format!(
                    "Unsupported hash algorithm '{}'. Supported algorithms: sha256, sha512",
                    algorithm
                ),
                alias,
                file_path,
                expected_hash,
                None,
            ));
        }

        // Validate hex string format
        if !Self::is_valid_hex_string(&expected_hex_hash) {
            return Err(HashVerificationError::new(
                &format!("Invalid hex string in hash: '{}'", expected_hex_hash),
                alias,
                file_path,
                expected_hash,
                None,
            ));
        }

        // Validate hex string length
        let expected_length = if algorithm == "sha256" { 64 } else { 128 };
        if expected_hex_hash.len() != expected_length {
            return Err(HashVerificationError::new(
                &format!(
                    "Invalid {} hash length: expected {} hex chars, got {}",
                    algorithm,
                    expected_length,
                    expected_hex_hash.len()
                ),
                alias,
                file_path,
                expected_hash,
                None,
            ));
        }

        // Compute actual hash
        let actual_hash = Self::compute_hash(content, &algorithm);

        // Compare (case-insensitive)
        if !actual_hash.eq_ignore_ascii_case(&expected_hex_hash) {
            return Err(HashVerificationError::new(
                &format!("Hash verification failed for '{}'", alias),
                alias,
                file_path,
                expected_hash,
                Some(&format!("{}:{}", algorithm, actual_hash)),
            ));
        }

        Ok(())
    }

    /// Compute hash of content using specified algorithm
    fn compute_hash(content: &str, algorithm: &str) -> String {
        let content_bytes = content.as_bytes();

        let hash_bytes = if algorithm == "sha256" {
            let mut hasher = Sha256::new();
            hasher.update(content_bytes);
            hasher.finalize().to_vec()
        } else if algorithm == "sha512" {
            let mut hasher = Sha512::new();
            hasher.update(content_bytes);
            hasher.finalize().to_vec()
        } else {
            panic!("Unsupported algorithm: {}", algorithm);
        };

        // Convert to lowercase hex string
        hash_bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    }

    /// Check if string is valid hexadecimal
    fn is_valid_hex_string(hex: &str) -> bool {
        !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit())
    }

    /// Compute hash for a file (for generating verify hashes)
    /// Utility method for developers
    pub fn compute_file_hash(content: &str, algorithm: &str) -> Result<String, String> {
        if algorithm != "sha256" && algorithm != "sha512" {
            return Err(format!("Unsupported algorithm: {}", algorithm));
        }

        Ok(Self::compute_hash(content, algorithm))
    }
}

/// Exception thrown when hash verification fails
#[derive(Debug, Clone)]
pub struct HashVerificationError {
    pub message: String,
    pub import_alias: String,
    pub file_path: String,
    pub expected_hash: String,
    pub actual_hash: Option<String>,
}

impl HashVerificationError {
    pub fn new(
        message: &str,
        import_alias: &str,
        file_path: &str,
        expected_hash: &str,
        actual_hash: Option<&str>,
    ) -> Self {
        HashVerificationError {
            message: message.to_string(),
            import_alias: import_alias.to_string(),
            file_path: file_path.to_string(),
            expected_hash: expected_hash.to_string(),
            actual_hash: actual_hash.map(String::from),
        }
    }
}

impl fmt::Display for HashVerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref actual) = self.actual_hash {
            write!(
                f,
                "{}\n  Expected: {}\n  Actual:   {}",
                self.message, self.expected_hash, actual
            )
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for HashVerificationError {}