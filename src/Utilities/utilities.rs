//! Utilities - String extensions and utility functions

/// String extension methods (C# style)
pub struct StringExtensions;

impl StringExtensions {
    /// Splits a string by a delimiter (C# Split)
    pub fn Split(input: &str, delimiter: char) -> Vec<String> {
        input.split(delimiter).map(|s| s.to_string()).collect()
    }

    /// Joins strings with a separator (C# String.Join)
    pub fn Join(separator: &str, parts: &[String]) -> String {
        parts.join(separator)
    }

    /// Checks if string starts with prefix (C# StartsWith)
    pub fn StartsWith(input: &str, prefix: &str) -> bool {
        input.starts_with(prefix)
    }

    /// Checks if string ends with suffix (C# EndsWith)
    pub fn EndsWith(input: &str, suffix: &str) -> bool {
        input.ends_with(suffix)
    }

    /// Converts to lowercase (C# ToLower)
    pub fn ToLower(input: &str) -> String {
        input.to_lowercase()
    }

    /// Converts to uppercase (C# ToUpper)
    pub fn ToUpper(input: &str) -> String {
        input.to_uppercase()
    }

    /// Trims whitespace (C# Trim)
    pub fn Trim(input: &str) -> String {
        input.trim().to_string()
    }

    /// Checks if string contains substring (C# Contains)
    pub fn Contains(input: &str, substring: &str) -> bool {
        input.contains(substring)
    }

    /// Replaces all occurrences (C# Replace)
    pub fn Replace(input: &str, from: &str, to: &str) -> String {
        input.replace(from, to)
    }

    /// Gets substring (C# Substring)
    pub fn Substring(input: &str, start: usize, length: Option<usize>) -> String {
        let chars: Vec<char> = input.chars().collect();
        let end = match length {
            Some(len) => (start + len).min(chars.len()),
            None => chars.len(),
        };
        chars[start..end].iter().collect()
    }

    /// Checks if string is null or empty (C# IsNullOrEmpty)
    pub fn IsNullOrEmpty(input: &str) -> bool {
        input.is_empty()
    }

    /// Checks if string is null or whitespace (C# IsNullOrWhiteSpace)
    pub fn IsNullOrWhiteSpace(input: &str) -> bool {
        input.trim().is_empty()
    }
}

/// Object extension methods
pub struct ObjectExtensions;

impl ObjectExtensions {
    /// Converts value to string (C# ToString)
    pub fn ToString<T: std::fmt::Display>(value: &T) -> String {
        format!("{}", value)
    }
}