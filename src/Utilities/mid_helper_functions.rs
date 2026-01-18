use rand::Rng;
use std::collections::HashSet;
use std::fmt::Write;

/// Helper functions for common operations
/// Maintains C# naming convention (PascalCase)
pub struct MID_HelperFunctions;

impl MID_HelperFunctions {
    // Validation constants
    const INVALID_STRING_VALUES: &'static [&'static str] = &["NULL", "UNDEFINED", "NONE", "N/A"];

    // Debug utilities constants
    const MAX_DEPTH: usize = 10;
    const MAX_COLLECTION_ITEMS: usize = 100;

    // ========== String Validation ==========

    /// Validates if a string is not null, empty, or variations of invalid values
    pub fn IsValidString(input: Option<&str>) -> bool {
        match input {
            None => false,
            Some(s) => {
                if s.trim().is_empty() {
                    return false;
                }

                let upper_input = s.trim().to_uppercase();
                !Self::INVALID_STRING_VALUES.contains(&upper_input.as_str())
            }
        }
    }

    // ========== Utility Methods ==========

    /// Get the current environment (Development, Production, etc.)
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

    /// Generate a cryptographically secure random string with specified length
    pub fn GenerateRandomString(length: usize, use_special_characters: bool) -> String {
        if length == 0 {
            return String::new();
        }

        const BASIC_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        const SPECIAL_CHARS: &[u8] = b"!@#$%^&*()_+-=[]{}|;:,.<>?";

        let chars = if use_special_characters {
            let mut combined = BASIC_CHARS.to_vec();
            combined.extend_from_slice(SPECIAL_CHARS);
            combined
        } else {
            BASIC_CHARS.to_vec()
        };

        let mut rng = rand::thread_rng();
        (0..length)
            .map(|_| {
                let idx = rng.gen_range(0..chars.len());
                chars[idx] as char
            })
            .collect()
    }

    // ========== Debug Utilities ==========

    /// Get detailed member values of a struct or any type
    /// This is a simplified version - full reflection would require proc macros
    pub fn GetStructOrClassMemberValues<T: std::fmt::Debug>(instance: &T) -> String {
        format!("{:#?}", instance)
    }

    /// Get indentation string
    fn get_indentation(depth: usize) -> String {
        " ".repeat(depth * 4)
    }

    /// Get arrow indentation
    fn get_arrow_indentation(depth: usize) -> &'static str {
        match depth {
            0 => "",
            1 => "->",
            2 => "-->",
            3 => "--->",
            _ => "---->", // Simplified
        }
    }
}

// ========== Tests ==========

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_string() {
        assert!(MID_HelperFunctions::IsValidString(Some("hello")));
        assert!(!MID_HelperFunctions::IsValidString(Some("")));
        assert!(!MID_HelperFunctions::IsValidString(Some("   ")));
        assert!(!MID_HelperFunctions::IsValidString(Some("NULL")));
        assert!(!MID_HelperFunctions::IsValidString(Some("null")));
        assert!(!MID_HelperFunctions::IsValidString(Some("UNDEFINED")));
        assert!(!MID_HelperFunctions::IsValidString(None));
    }

    #[test]
    fn test_get_environment() {
        let env = MID_HelperFunctions::GetEnvironment();
        #[cfg(debug_assertions)]
        assert_eq!(env, "Development");
        #[cfg(not(debug_assertions))]
        assert_eq!(env, "Production");
    }

    #[test]
    fn test_generate_random_string() {
        let s1 = MID_HelperFunctions::GenerateRandomString(10, false);
        assert_eq!(s1.len(), 10);
        assert!(s1.chars().all(|c| c.is_alphanumeric()));

        let s2 = MID_HelperFunctions::GenerateRandomString(20, true);
        assert_eq!(s2.len(), 20);

        let s3 = MID_HelperFunctions::GenerateRandomString(0, false);
        assert_eq!(s3.len(), 0);
    }
}