/// String extension methods (C# style)
pub trait StringExtensions {
    fn IsNullOrEmpty(&self) -> bool;
    fn IsNullOrWhiteSpace(&self) -> bool;
    fn ToUpperInvariant(&self) -> String;
    fn ToLowerInvariant(&self) -> String;
    fn Contains(&self, value: &str) -> bool;
    fn StartsWith(&self, value: &str) -> bool;
    fn EndsWith(&self, value: &str) -> bool;
    fn Trim(&self) -> String;
    fn TrimStart(&self) -> String;
    fn TrimEnd(&self) -> String;
    fn Replace(&self, old: &str, new: &str) -> String;
    fn Split(&self, separator: char) -> Vec<String>;
    fn Join(separator: &str, values: &[String]) -> String;
    fn Substring(&self, start: usize, length: Option<usize>) -> String;
    fn IndexOf(&self, value: &str) -> Option<usize>;
    fn LastIndexOf(&self, value: &str) -> Option<usize>;
}

impl StringExtensions for str {
    fn IsNullOrEmpty(&self) -> bool {
        self.is_empty()
    }

    fn IsNullOrWhiteSpace(&self) -> bool {
        self.trim().is_empty()
    }

    fn ToUpperInvariant(&self) -> String {
        self.to_uppercase()
    }

    fn ToLowerInvariant(&self) -> String {
        self.to_lowercase()
    }

    fn Contains(&self, value: &str) -> bool {
        self.contains(value)
    }

    fn StartsWith(&self, value: &str) -> bool {
        self.starts_with(value)
    }

    fn EndsWith(&self, value: &str) -> bool {
        self.ends_with(value)
    }

    fn Trim(&self) -> String {
        self.trim().to_string()
    }

    fn TrimStart(&self) -> String {
        self.trim_start().to_string()
    }

    fn TrimEnd(&self) -> String {
        self.trim_end().to_string()
    }

    fn Replace(&self, old: &str, new: &str) -> String {
        self.replace(old, new)
    }

    fn Split(&self, separator: char) -> Vec<String> {
        self.split(separator).map(|s| s.to_string()).collect()
    }

    fn Join(separator: &str, values: &[String]) -> String {
        values.join(separator)
    }

    fn Substring(&self, start: usize, length: Option<usize>) -> String {
        let chars: Vec<char> = self.chars().collect();
        let end = length.map(|len| (start + len).min(chars.len())).unwrap_or(chars.len());

        if start >= chars.len() {
            return String::new();
        }

        chars[start..end].iter().collect()
    }

    fn IndexOf(&self, value: &str) -> Option<usize> {
        self.find(value)
    }

    fn LastIndexOf(&self, value: &str) -> Option<usize> {
        self.rfind(value)
    }
}

impl StringExtensions for String {
    fn IsNullOrEmpty(&self) -> bool {
        self.as_str().IsNullOrEmpty()
    }

    fn IsNullOrWhiteSpace(&self) -> bool {
        self.as_str().IsNullOrWhiteSpace()
    }

    fn ToUpperInvariant(&self) -> String {
        self.as_str().ToUpperInvariant()
    }

    fn ToLowerInvariant(&self) -> String {
        self.as_str().ToLowerInvariant()
    }

    fn Contains(&self, value: &str) -> bool {
        self.as_str().Contains(value)
    }

    fn StartsWith(&self, value: &str) -> bool {
        self.as_str().StartsWith(value)
    }

    fn EndsWith(&self, value: &str) -> bool {
        self.as_str().EndsWith(value)
    }

    fn Trim(&self) -> String {
        self.as_str().Trim()
    }

    fn TrimStart(&self) -> String {
        self.as_str().TrimStart()
    }

    fn TrimEnd(&self) -> String {
        self.as_str().TrimEnd()
    }

    fn Replace(&self, old: &str, new: &str) -> String {
        self.as_str().Replace(old, new)
    }

    fn Split(&self, separator: char) -> Vec<String> {
        self.as_str().Split(separator)
    }

    fn Join(separator: &str, values: &[String]) -> String {
        str::Join(separator, values)
    }

    fn Substring(&self, start: usize, length: Option<usize>) -> String {
        self.as_str().Substring(start, length)
    }

    fn IndexOf(&self, value: &str) -> Option<usize> {
        self.as_str().IndexOf(value)
    }

    fn LastIndexOf(&self, value: &str) -> Option<usize> {
        self.as_str().LastIndexOf(value)
    }
}

/// Object extension methods (C# style)
pub trait ObjectExtensions {
    fn ToString(&self) -> String;
    fn GetHashCode(&self) -> u64;
}

impl<T: std::fmt::Display> ObjectExtensions for T {
    fn ToString(&self) -> String {
        format!("{}", self)
    }

    fn GetHashCode(&self) -> u64 {
        // Note: This is a placeholder. Proper implementation would use std::hash::Hash
        // For now, we convert to string and hash that
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let s = format!("{}", self);
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }
}

// ========== Tests ==========

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_extensions() {
        assert!("".IsNullOrEmpty());
        assert!(!"hello".IsNullOrEmpty());

        assert!("   ".IsNullOrWhiteSpace());
        assert!(!"hello".IsNullOrWhiteSpace());

        assert_eq!("hello".ToUpperInvariant(), "HELLO");
        assert_eq!("HELLO".ToLowerInvariant(), "hello");

        assert!("hello world".Contains("world"));
        assert!(!"hello world".Contains("foo"));

        assert!("hello".StartsWith("hel"));
        assert!("hello".EndsWith("llo"));

        assert_eq!("  hello  ".Trim(), "hello");
        assert_eq!("hello world".Replace("world", "rust"), "hello rust");

        let parts = "a,b,c".Split(',');
        assert_eq!(parts, vec!["a", "b", "c"]);

        assert_eq!(str::Join(",", &["a".to_string(), "b".to_string()]), "a,b");

        assert_eq!("hello".Substring(1, Some(3)), "ell");
        assert_eq!("hello".IndexOf("ll"), Some(2));
    }
}