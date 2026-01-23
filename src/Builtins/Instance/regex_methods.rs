// src/Builtins/Instance/regex_methods.rs
//! Regex instance methods for DixScript
//! Provides pattern matching and string manipulation via regex

use crate::Builtins::Core::{
    DixType, DixValue, IBuiltinMethod, BuiltinMethod, validation_helpers,
};
use regex::Regex;
use std::collections::HashMap;

/// Get all regex instance methods
pub fn get_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // Regex.test(string) - Test if pattern matches
    methods.insert(
        "test".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "test".to_string(),
            2,
            DixType::Bool,
            test_impl,
            "Tests if the regex pattern matches the given string".to_string(),
            |args| {
                args.len() == 2
                    && args[0].get_type() == DixType::Regex
                    && args[1].get_type() == DixType::String
            },
        )),
    );

    // Regex.match(string) - Get first match + capture groups
    methods.insert(
        "match".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "match".to_string(),
            2,
            DixType::Array,
            match_impl,
            "Returns the first match and capture groups as an array".to_string(),
            |args| {
                args.len() == 2
                    && args[0].get_type() == DixType::Regex
                    && args[1].get_type() == DixType::String
            },
        )),
    );

    // Regex.matchAll(string) - Get all matches
    methods.insert(
        "matchAll".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "matchAll".to_string(),
            2,
            DixType::Array,
            match_all_impl,
            "Returns all matches as an array of arrays (each with capture groups)".to_string(),
            |args| {
                args.len() == 2
                    && args[0].get_type() == DixType::Regex
                    && args[1].get_type() == DixType::String
            },
        )),
    );

    // Regex.replace(string, replacement) - Replace matches
    methods.insert(
        "replace".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "replace".to_string(),
            3,
            DixType::String,
            replace_impl,
            "Replaces all matches with the replacement string".to_string(),
            |args| {
                args.len() == 3
                    && args[0].get_type() == DixType::Regex
                    && args[1].get_type() == DixType::String
                    && args[2].get_type() == DixType::String
            },
        )),
    );

    // Regex.split(string) - Split string by pattern
    methods.insert(
        "split".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "split".to_string(),
            2,
            DixType::Array,
            split_impl,
            "Splits the string by the regex pattern".to_string(),
            |args| {
                args.len() == 2
                    && args[0].get_type() == DixType::Regex
                    && args[1].get_type() == DixType::String
            },
        )),
    );

    // Regex.isValid() - Check if pattern is valid
    methods.insert(
        "isValid".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isValid".to_string(),
            1,
            DixType::Bool,
            is_valid_impl,
            "Checks if the regex pattern is valid".to_string(),
            |args| args.len() == 1 && args[0].get_type() == DixType::Regex,
        )),
    );

    methods
}

// ==================== METHOD IMPLEMENTATIONS ====================

/// Regex.test(string) - Test if pattern matches
fn test_impl(args: &[DixValue]) -> Result<DixValue, String> {
    let pattern = args[0].as_string();
    let text = args[1].as_string();

    let re = Regex::new(&pattern)
        .map_err(|e| format!("Invalid regex pattern: {}", e))?;

    Ok(DixValue::from_bool(re.is_match(&text)))
}

/// Regex.match(string) - Get first match + capture groups
fn match_impl(args: &[DixValue]) -> Result<DixValue, String> {
    let pattern = args[0].as_string();
    let text = args[1].as_string();

    let re = Regex::new(&pattern)
        .map_err(|e| format!("Invalid regex pattern: {}", e))?;

    if let Some(captures) = re.captures(&text) {
        let mut result = Vec::new();

        // First element is the full match
        if let Some(full_match) = captures.get(0) {
            result.push(DixValue::from_string(full_match.as_str().to_string()));
        }

        // Add capture groups
        for i in 1..captures.len() {
            if let Some(capture) = captures.get(i) {
                result.push(DixValue::from_string(capture.as_str().to_string()));
            } else {
                result.push(DixValue::null());
            }
        }

        Ok(DixValue::from_array(result))
    } else {
        // No match - return empty array
        Ok(DixValue::from_array(Vec::new()))
    }
}

/// Regex.matchAll(string) - Get all matches
fn match_all_impl(args: &[DixValue]) -> Result<DixValue, String> {
    let pattern = args[0].as_string();
    let text = args[1].as_string();

    let re = Regex::new(&pattern)
        .map_err(|e| format!("Invalid regex pattern: {}", e))?;

    let mut all_matches = Vec::new();

    for captures in re.captures_iter(&text) {
        let mut match_array = Vec::new();

        // First element is the full match
        if let Some(full_match) = captures.get(0) {
            match_array.push(DixValue::from_string(full_match.as_str().to_string()));
        }

        // Add capture groups
        for i in 1..captures.len() {
            if let Some(capture) = captures.get(i) {
                match_array.push(DixValue::from_string(capture.as_str().to_string()));
            } else {
                match_array.push(DixValue::null());
            }
        }

        all_matches.push(DixValue::from_array(match_array));
    }

    Ok(DixValue::from_array(all_matches))
}

/// Regex.replace(string, replacement) - Replace matches
fn replace_impl(args: &[DixValue]) -> Result<DixValue, String> {
    let pattern = args[0].as_string();
    let text = args[1].as_string();
    let replacement = args[2].as_string();

    let re = Regex::new(&pattern)
        .map_err(|e| format!("Invalid regex pattern: {}", e))?;

    let result = re.replace_all(&text, replacement.as_str());

    Ok(DixValue::from_string(result.to_string()))
}

/// Regex.split(string) - Split string by pattern
fn split_impl(args: &[DixValue]) -> Result<DixValue, String> {
    let pattern = args[0].as_string();
    let text = args[1].as_string();

    let re = Regex::new(&pattern)
        .map_err(|e| format!("Invalid regex pattern: {}", e))?;

    let parts: Vec<DixValue> = re
        .split(&text)
        .map(|s| DixValue::from_string(s.to_string()))
        .collect();

    Ok(DixValue::from_array(parts))
}

/// Regex.isValid() - Check if pattern is valid
fn is_valid_impl(args: &[DixValue]) -> Result<DixValue, String> {
    let pattern = args[0].as_string();

    let is_valid = Regex::new(&pattern).is_ok();

    Ok(DixValue::from_bool(is_valid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_test() {
        let pattern = DixValue::from_regex(r"\d+".to_string()).unwrap();
        let text = DixValue::from_string("abc123def".to_string());

        let result = test_impl(&[pattern, text]).unwrap();
        assert_eq!(result.as_bool(), true);
    }

    #[test]
    fn test_regex_match() {
        let pattern = DixValue::from_regex(r"(\w+)@(\w+)\.(\w+)".to_string()).unwrap();
        let text = DixValue::from_string("user@example.com".to_string());

        let result = match_impl(&[pattern, text]).unwrap();
        let matches = result.as_array();

        assert_eq!(matches.len(), 4); // Full match + 3 groups
        assert_eq!(matches[0].as_string(), "user@example.com");
        assert_eq!(matches[1].as_string(), "user");
        assert_eq!(matches[2].as_string(), "example");
        assert_eq!(matches[3].as_string(), "com");
    }

    #[test]
    fn test_regex_match_all() {
        let pattern = DixValue::from_regex(r"\d+".to_string()).unwrap();
        let text = DixValue::from_string("abc123def456ghi".to_string());

        let result = match_all_impl(&[pattern, text]).unwrap();
        let all_matches = result.as_array();

        assert_eq!(all_matches.len(), 2);
        assert_eq!(all_matches[0].as_array()[0].as_string(), "123");
        assert_eq!(all_matches[1].as_array()[0].as_string(), "456");
    }

    #[test]
    fn test_regex_replace() {
        let pattern = DixValue::from_regex(r"\d+".to_string()).unwrap();
        let text = DixValue::from_string("abc123def456".to_string());
        let replacement = DixValue::from_string("X".to_string());

        let result = replace_impl(&[pattern, text, replacement]).unwrap();
        assert_eq!(result.as_string(), "abcXdefX");
    }

    #[test]
    fn test_regex_split() {
        let pattern = DixValue::from_regex(r",\s*".to_string()).unwrap();
        let text = DixValue::from_string("a, b,c,  d".to_string());

        let result = split_impl(&[pattern, text]).unwrap();
        let parts = result.as_array();

        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0].as_string(), "a");
        assert_eq!(parts[1].as_string(), "b");
        assert_eq!(parts[2].as_string(), "c");
        assert_eq!(parts[3].as_string(), "d");
    }

    #[test]
    fn test_regex_is_valid() {
        let valid_pattern = DixValue::from_regex(r"\d+".to_string()).unwrap();
        let result = is_valid_impl(&[valid_pattern]).unwrap();
        assert_eq!(result.as_bool(), true);

        // Test with invalid pattern stored as string (bypassing from_regex validation)
        let invalid_pattern = DixValue::new(
            crate::Builtins::Core::dix_value::ValueData::Regex("[invalid".to_string()),
            DixType::Regex,
        );
        let result = is_valid_impl(&[invalid_pattern]).unwrap();
        assert_eq!(result.as_bool(), false);
    }
}