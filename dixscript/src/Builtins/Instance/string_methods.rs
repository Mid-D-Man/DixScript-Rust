// src/Builtins/Instance/string_methods.rs
//! String instance methods for DixScript
//! Provides methods like toUpper, toLower, trim, substring, etc.

use crate::Builtins::Core::{
    DixType, DixValue, IBuiltinMethod, BuiltinMethod, validation_helpers,
};
use std::collections::HashMap;

/// Get all string instance methods
pub fn get_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // String.toUpper() - Convert to uppercase
    methods.insert(
        "toUpper".to_string(),
        Box::new(BuiltinMethod::new(
            "toUpper".to_string(),
            1,
            DixType::String,
            |args| {
                let s = args[0].as_string();
                Ok(DixValue::from_string(s.to_uppercase()))
            },
            "Converts the string to uppercase".to_string(),
        )),
    );

    // String.toLower() - Convert to lowercase
    methods.insert(
        "toLower".to_string(),
        Box::new(BuiltinMethod::new(
            "toLower".to_string(),
            1,
            DixType::String,
            |args| {
                let s = args[0].as_string();
                Ok(DixValue::from_string(s.to_lowercase()))
            },
            "Converts the string to lowercase".to_string(),
        )),
    );

    // String.trim() - Remove whitespace from ends
    methods.insert(
        "trim".to_string(),
        Box::new(BuiltinMethod::new(
            "trim".to_string(),
            1,
            DixType::String,
            |args| {
                let s = args[0].as_string();
                Ok(DixValue::from_string(s.trim().to_string()))
            },
            "Removes whitespace from the beginning and end of the string".to_string(),
        )),
    );

    // String.length() - Get string length
    methods.insert(
        "length".to_string(),
        Box::new(BuiltinMethod::new(
            "length".to_string(),
            1,
            DixType::Int,
            |args| {
                let s = args[0].as_string();
                Ok(DixValue::from_int(s.len() as i32))
            },
            "Returns the length of the string".to_string(),
        )),
    );

    // String.substring(start, length) - Extract substring
    methods.insert(
        "substring".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "substring".to_string(),
            3,
            DixType::String,
            |args| {
                let s = args[0].as_string();
                let mut start = args[1].as_int();
                let mut length = args[2].as_int();

                // Clamp values
                if start < 0 {
                    start = 0;
                }
                if start >= s.len() as i32 {
                    return Ok(DixValue::from_string(String::new()));
                }

                if length < 0 {
                    length = 0;
                }

                let start_idx = start as usize;
                let max_length = s.len() - start_idx;
                let actual_length = (length as usize).min(max_length);

                // Use char indices for proper Unicode handling
                let result: String = s.chars()
                    .skip(start_idx)
                    .take(actual_length)
                    .collect();

                Ok(DixValue::from_string(result))
            },
            "Returns a substring starting at the specified index with the specified length".to_string(),
            |args| {
                args.len() == 3
                    && args[0].get_type() == DixType::String
                    && args[1].is_numeric()
                    && args[2].is_numeric()
            },
        )),
    );

    // String.contains(substring) - Check if contains substring
    methods.insert(
        "contains".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "contains".to_string(),
            2,
            DixType::Bool,
            |args| {
                let s = args[0].as_string();
                let substring = args[1].as_string();
                Ok(DixValue::from_bool(s.contains(&substring)))
            },
            "Checks if the string contains the specified substring".to_string(),
            |args| {
                args.len() == 2
                    && args[0].get_type() == DixType::String
                    && args[1].get_type() == DixType::String
            },
        )),
    );

    // String.startsWith(prefix) - Check if starts with prefix
    methods.insert(
        "startsWith".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "startsWith".to_string(),
            2,
            DixType::Bool,
            |args| {
                let s = args[0].as_string();
                let prefix = args[1].as_string();
                Ok(DixValue::from_bool(s.starts_with(&prefix)))
            },
            "Checks if the string starts with the specified prefix".to_string(),
            |args| {
                args.len() == 2
                    && args[0].get_type() == DixType::String
                    && args[1].get_type() == DixType::String
            },
        )),
    );

    // String.endsWith(suffix) - Check if ends with suffix
    methods.insert(
        "endsWith".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "endsWith".to_string(),
            2,
            DixType::Bool,
            |args| {
                let s = args[0].as_string();
                let suffix = args[1].as_string();
                Ok(DixValue::from_bool(s.ends_with(&suffix)))
            },
            "Checks if the string ends with the specified suffix".to_string(),
            |args| {
                args.len() == 2
                    && args[0].get_type() == DixType::String
                    && args[1].get_type() == DixType::String
            },
        )),
    );

    // String.replace(oldValue, newValue) - Replace substring
    methods.insert(
        "replace".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "replace".to_string(),
            3,
            DixType::String,
            |args| {
                let s = args[0].as_string();
                let old_value = args[1].as_string();
                let new_value = args[2].as_string();
                Ok(DixValue::from_string(s.replace(&old_value, &new_value)))
            },
            "Replaces all occurrences of the old value with the new value".to_string(),
            |args| {
                args.len() == 3
                    && args[0].get_type() == DixType::String
                    && args[1].get_type() == DixType::String
                    && args[2].get_type() == DixType::String
            },
        )),
    );

    // String.split(separator) - Split string into array
    methods.insert(
        "split".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "split".to_string(),
            2,
            DixType::Array,
            |args| {
                let s = args[0].as_string();
                let separator = args[1].as_string();

                let parts: Vec<DixValue> = s
                    .split(&separator as &str)
                    .map(|part| DixValue::from_string(part.to_string()))
                    .collect();

                Ok(DixValue::from_array(parts))
            },
            "Splits the string into an array using the specified separator".to_string(),
            |args| {
                args.len() == 2
                    && args[0].get_type() == DixType::String
                    && args[1].get_type() == DixType::String
            },
        )),
    );

    // String.indexOf(substring) - Find index of substring
    methods.insert(
        "indexOf".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "indexOf".to_string(),
            2,
            DixType::Int,
            |args| {
                let s = args[0].as_string();
                let substring = args[1].as_string();

                match s.find(&substring as &str) {
                    Some(idx) => Ok(DixValue::from_int(idx as i32)),
                    None => Ok(DixValue::from_int(-1)),
                }
            },
            "Returns the index of the first occurrence of the substring, or -1 if not found".to_string(),
            |args| {
                args.len() == 2
                    && args[0].get_type() == DixType::String
                    && args[1].get_type() == DixType::String
            },
        )),
    );

    // String.lastIndexOf(substring) - Find last index of substring
    methods.insert(
        "lastIndexOf".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "lastIndexOf".to_string(),
            2,
            DixType::Int,
            |args| {
                let s = args[0].as_string();
                let substring = args[1].as_string();

                match s.rfind(&substring as &str) {
                    Some(idx) => Ok(DixValue::from_int(idx as i32)),
                    None => Ok(DixValue::from_int(-1)),
                }
            },
            "Returns the index of the last occurrence of the substring, or -1 if not found".to_string(),
            |args| {
                args.len() == 2
                    && args[0].get_type() == DixType::String
                    && args[1].get_type() == DixType::String
            },
        )),
    );

    // String.charAt(index) - Get character at index
    methods.insert(
        "charAt".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "charAt".to_string(),
            2,
            DixType::String,
            |args| {
                let s = args[0].as_string();
                let index = args[1].as_int();

                if index < 0 || index >= s.len() as i32 {
                    return Err("Index is out of range".to_string());
                }

                let ch = s.chars().nth(index as usize)
                    .ok_or("Index is out of range")?;

                Ok(DixValue::from_string(ch.to_string()))
            },
            "Returns the character at the specified index".to_string(),
            |args| {
                args.len() == 2
                    && args[0].get_type() == DixType::String
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
                    && validation_helpers::valid_string_index(&args[0], &args[1])
            },
        )),
    );

    // String.isEmpty() - Check if string is empty
    methods.insert(
        "isEmpty".to_string(),
        Box::new(BuiltinMethod::new(
            "isEmpty".to_string(),
            1,
            DixType::Bool,
            |args| {
                let s = args[0].as_string();
                Ok(DixValue::from_bool(s.is_empty()))
            },
            "Checks if the string is null or empty".to_string(),
        )),
    );

    // String.isBlank() - Check if string is blank (empty or whitespace)
    methods.insert(
        "isBlank".to_string(),
        Box::new(BuiltinMethod::new(
            "isBlank".to_string(),
            1,
            DixType::Bool,
            |args| {
                let s = args[0].as_string();
                Ok(DixValue::from_bool(s.trim().is_empty()))
            },
            "Checks if the string is null, empty, or contains only whitespace".to_string(),
        )),
    );

    // String.padLeft(totalWidth, paddingChar) - Pad string on left
    methods.insert(
        "padLeft".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "padLeft".to_string(),
            3,
            DixType::String,
            |args| {
                let s = args[0].as_string();
                let width = args[1].as_int() as usize;
                let pad_char_str = args[2].as_string();

                if pad_char_str.is_empty() {
                    return Err("Padding character cannot be empty".to_string());
                }

                let pad_char = pad_char_str.chars().next().unwrap();

                if s.len() >= width {
                    Ok(DixValue::from_string(s))
                } else {
                    let padding = pad_char.to_string().repeat(width - s.len());
                    Ok(DixValue::from_string(format!("{}{}", padding, s)))
                }
            },
            "Pads the string on the left with the specified character to reach the total width".to_string(),
            |args| {
                args.len() == 3
                    && args[0].get_type() == DixType::String
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
                    && args[2].get_type() == DixType::String
            },
        )),
    );

    // String.padRight(totalWidth, paddingChar) - Pad string on right
    methods.insert(
        "padRight".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "padRight".to_string(),
            3,
            DixType::String,
            |args| {
                let s = args[0].as_string();
                let width = args[1].as_int() as usize;
                let pad_char_str = args[2].as_string();

                if pad_char_str.is_empty() {
                    return Err("Padding character cannot be empty".to_string());
                }

                let pad_char = pad_char_str.chars().next().unwrap();

                if s.len() >= width {
                    Ok(DixValue::from_string(s))
                } else {
                    let padding = pad_char.to_string().repeat(width - s.len());
                    Ok(DixValue::from_string(format!("{}{}", s, padding)))
                }
            },
            "Pads the string on the right with the specified character to reach the total width".to_string(),
            |args| {
                args.len() == 3
                    && args[0].get_type() == DixType::String
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
                    && args[2].get_type() == DixType::String
            },
        )),
    );

    methods
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_upper() {
        let methods = get_methods();
        let to_upper = methods.get("toUpper").unwrap();

        let args = vec![DixValue::from_string("hello".to_string())];
        let result = to_upper.call(&args).unwrap();

        assert_eq!(result.as_string(), "HELLO");
    }

    #[test]
    fn test_substring() {
        let methods = get_methods();
        let substring = methods.get("substring").unwrap();

        let args = vec![
            DixValue::from_string("hello world".to_string()),
            DixValue::from_int(0),
            DixValue::from_int(5),
        ];
        let result = substring.call(&args).unwrap();

        assert_eq!(result.as_string(), "hello");
    }

    #[test]
    fn test_split() {
        let methods = get_methods();
        let split = methods.get("split").unwrap();

        let args = vec![
            DixValue::from_string("a,b,c".to_string()),
            DixValue::from_string(",".to_string()),
        ];
        let result = split.call(&args).unwrap();

        assert_eq!(result.as_array().len(), 3);
        assert_eq!(result.as_array()[0].as_string(), "a");
        assert_eq!(result.as_array()[1].as_string(), "b");
        assert_eq!(result.as_array()[2].as_string(), "c");
    }
}