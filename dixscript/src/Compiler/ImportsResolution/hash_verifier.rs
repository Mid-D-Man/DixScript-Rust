// src/Compiler/ImportsResolution/hash_verifier.rs
//! SHA-256 and SHA-512 hash verification for import file integrity.

use sha2::{Digest, Sha256, Sha512};
use std::fmt;

pub struct HashVerifier;

impl HashVerifier {
    /// Verify file content against an expected hash string.
    ///
    /// Expected format: `"algorithm:hexstring"` — e.g. `"sha256:abc123..."`.
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
        let expected_hex = parts[1].to_lowercase();

        if algorithm != "sha256" && algorithm != "sha512" {
            return Err(HashVerificationError::new(
                &format!(
                    "Unsupported hash algorithm '{}'. Supported: sha256, sha512",
                    algorithm
                ),
                alias,
                file_path,
                expected_hash,
                None,
            ));
        }

        if !Self::is_valid_hex_string(&expected_hex) {
            return Err(HashVerificationError::new(
                &format!("Invalid hex string in hash: '{}'", expected_hex),
                alias,
                file_path,
                expected_hash,
                None,
            ));
        }

        let expected_length = if algorithm == "sha256" { 64 } else { 128 };
        if expected_hex.len() != expected_length {
            return Err(HashVerificationError::new(
                &format!(
                    "Invalid {} hash length: expected {} hex chars, got {}",
                    algorithm,
                    expected_length,
                    expected_hex.len()
                ),
                alias,
                file_path,
                expected_hash,
                None,
            ));
        }

        let actual_hash = Self::compute_hash(content, &algorithm);

        if !actual_hash.eq_ignore_ascii_case(&expected_hex) {
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

    /// Compute the hash of `content` using the named algorithm.
    pub fn compute_file_hash(content: &str, algorithm: &str) -> Result<String, String> {
        if algorithm != "sha256" && algorithm != "sha512" {
            return Err(format!("Unsupported algorithm: {}", algorithm));
        }
        Ok(Self::compute_hash(content, algorithm))
    }

    fn compute_hash(content: &str, algorithm: &str) -> String {
        match algorithm {
            "sha256" => {
                let mut h = Sha256::new();
                h.update(content.as_bytes());
                h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
            }
            "sha512" => {
                let mut h = Sha512::new();
                h.update(content.as_bytes());
                h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
            }
            other => unreachable!("compute_hash called with unsupported algorithm: {}", other),
        }
    }

    fn is_valid_hex_string(hex: &str) -> bool {
        !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit())
    }
}

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