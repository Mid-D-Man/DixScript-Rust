//! MID_HelperFunctions - Helper functions with Rust idioms

use rand::Rng;
use std::fmt::Debug;

/// Helper functions for common operations
pub struct MID_HelperFunctions;

impl MID_HelperFunctions {
    // ========== String Validation ==========

    /// Validates if a string is not null, empty, or invalid values
    pub fn IsValidString(input: &str) -> bool {
        if input.trim().is_empty() {
            return false;
        }

        let upper_input = input.trim().to_uppercase();
        let invalid_values = ["NULL", "UNDEFINED", "NONE", "N/A"];

        !invalid_values.contains(&upper_input.as_str())
    }

    // ========== Utility Methods ==========

    /// Get the current environment (Development, Production)
    pub fn GetEnvironment() -> &'static str {
        #[cfg(debug_assertions)]
        {
            "Development"
        }
        #[cfg(not(debug_assertions))]
        {
            "Production"
        }
    }

    /// Generate a random string with specified length
    /// Note: Uses thread_rng for simplicity. For crypto-secure, use rand::rngs::OsRng
    pub fn GenerateRandomString(length: usize, use_special_characters: bool) -> String {
        if length == 0 {
            return String::new();
        }

        let basic_chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let special_chars = "!@#$%^&*()_+-=[]{}|;:,.<>?";

        let chars = if use_special_characters {
            format!("{}{}", basic_chars, special_chars)
        } else {
            basic_chars.to_string()
        };

        let mut rng = rand::thread_rng();
        (0..length)
            .map(|_| {
                let idx = rng.gen_range(0..chars.len());
                chars.chars().nth(idx).unwrap()
            })
            .collect()
    }

    // ========== Debug Utilities (Simplified) ==========

    /// Get a debug representation of any value implementing Debug
    /// This is a simplified version of C#'s reflection-based method
    /// For complex types, use #[derive(Debug)] on your structs
    pub fn GetDebugString<T: Debug>(value: &T) -> String {
        format!("{:#?}", value)
    }

    /// Get a compact debug representation
    pub fn GetCompactDebugString<T: Debug>(value: &T) -> String {
        format!("{:?}", value)
    }
}//has issues