// src/Builtins/Static/array_object.rs
//! Array static object implementation for DixScript
//! Provides array creation and manipulation functions

use crate::Builtins::Core::{BuiltinMethod, DixType, DixValue, IBuiltinMethod, validation_helpers};
use crate::Builtins::Static::{IStaticObject, StaticObjectBase};

/// Array static object implementation
pub struct ArrayObject {
    base: StaticObjectBase,
}

impl ArrayObject {
    pub fn new() -> Self {
        let mut base = StaticObjectBase::new("Array".to_string());
        Self::initialize_methods(&mut base);
        ArrayObject { base }
    }

    fn initialize_methods(base: &mut StaticObjectBase) {
        // Array.range(start, end) - Create array with range of numbers
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "range".to_string(),
            2,
            DixType::Array,
            |args| {
                let start = args[0].as_int();
                let end = args[1].as_int();

                if start > end {
                    return Err("Start value cannot be greater than end value".to_string());
                }

                let mut result = Vec::new();
                for i in start..=end {
                    result.push(DixValue::from_int(i));
                }

                Ok(DixValue::from_array(result))
            },
            "Creates an array with numbers from start to end (inclusive)".to_string(),
            validation_helpers::all_numeric,
        )));

        // Array.fill(value, count) - Create array filled with value
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "fill".to_string(),
            2,
            DixType::Array,
            |args| {
                let value = &args[0];
                let count = args[1].as_int();

                if count < 0 {
                    return Err("Count cannot be negative".to_string());
                }

                if count > 10000 {
                    return Err("Count cannot exceed 10000".to_string());
                }

                let mut result = Vec::new();
                for _ in 0..count {
                    result.push(value.deep_clone());
                }

                Ok(DixValue::from_array(result))
            },
            "Creates an array filled with the specified value".to_string(),
            |args| {
                validation_helpers::argument_not_null(0, args)
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
            },
        )));

        // Array.empty() - Create empty array
        base.register_method(Box::new(BuiltinMethod::new(
            "empty".to_string(),
            0,
            DixType::Array,
            |_args| Ok(DixValue::from_array(Vec::new())),
            "Creates an empty array".to_string(),
        )));

        // Array.of(values...) - Create array from multiple values
        base.register_method(Box::new(BuiltinMethod::new_variadic(
            "of".to_string(),
            0,
            DixType::Array,
            |args| Ok(DixValue::from_array(args.to_vec())),
            "Creates an array from the provided values".to_string(),
        )));

        // Array.repeat(array, times) - Repeat array content
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "repeat".to_string(),
            2,
            DixType::Array,
            |args| {
                let array = args[0].as_array();
                let times = args[1].as_int();

                if times < 0 {
                    return Err("Times cannot be negative".to_string());
                }

                if times > 1000 {
                    return Err("Times cannot exceed 1000".to_string());
                }

                let mut result = Vec::new();
                for _ in 0..times {
                    for item in array {
                        result.push(item.deep_clone());
                    }
                }

                Ok(DixValue::from_array(result))
            },
            "Repeats the array content the specified number of times".to_string(),
            |args| {
                validation_helpers::first_is_array(args)
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
            },
        )));

        // Array.concat(array1, array2, ...) - Concatenate arrays
        base.register_method(Box::new(BuiltinMethod::new_variadic(
            "concat".to_string(),
            2,
            DixType::Array,
            |args| {
                if args.len() < 2 {
                    return Err("concat requires at least 2 arrays".to_string());
                }

                let mut result = Vec::new();
                for arg in args {
                    if arg.get_type() != DixType::Array {
                        return Err("All arguments must be arrays".to_string());
                    }
                    result.extend(arg.as_array().iter().cloned());
                }

                Ok(DixValue::from_array(result))
            },
            "Concatenates multiple arrays into one".to_string(),
        )));

        // Array.fromString(text, separator) - Split string into array
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "fromString".to_string(),
            2,
            DixType::Array,
            |args| {
                let text = args[0].as_string();
                let separator = args[1].as_string();

                if separator.is_empty() {
                    // Split into characters
                    let chars: Vec<DixValue> = text
                        .chars()
                        .map(|c| DixValue::from_string(c.to_string()))
                        .collect();
                    return Ok(DixValue::from_array(chars));
                }

                let parts: Vec<DixValue> = text
                    .split(&separator)
                    .map(|s| DixValue::from_string(s.to_string()))
                    .collect();

                Ok(DixValue::from_array(parts))
            },
            "Creates an array by splitting a string".to_string(),
            |args| {
                validation_helpers::argument_has_type(0, DixType::String, args)
                    && validation_helpers::argument_has_type(1, DixType::String, args)
            },
        )));

        // Array.reverse(array) - Create reversed copy of array
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "reverse".to_string(),
            1,
            DixType::Array,
            |args| {
                let array = args[0].as_array();
                let reversed: Vec<DixValue> = array.iter().rev().cloned().collect();
                Ok(DixValue::from_array(reversed))
            },
            "Creates a reversed copy of the array".to_string(),
            validation_helpers::first_is_array,
        )));

        // Array.sort(array) - Create sorted copy of array
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "sort".to_string(),
            1,
            DixType::Array,
            |args| {
                let array = args[0].as_array();
                let mut sorted: Vec<DixValue> = array.to_vec();
                sorted.sort_by_key(|a| a.as_string());
                Ok(DixValue::from_array(sorted))
            },
            "Creates a sorted copy of the array".to_string(),
            validation_helpers::first_is_array,
        )));

        // Array.unique(array) - Remove duplicates
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "unique".to_string(),
            1,
            DixType::Array,
            |args| {
                let array = args[0].as_array();
                let mut unique = Vec::new();

                for item in array {
                    if !unique.iter().any(|u: &DixValue| u.equal_to(item)) {  // ADD TYPE ANNOTATION
                        unique.push(item.clone());
                    }
                }

                Ok(DixValue::from_array(unique))
            },
            "Removes duplicate values from array".to_string(),
            validation_helpers::first_is_array,
        )));

        // Array.slice(array, start, end) - Extract portion of array
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "slice".to_string(),
            3,
            DixType::Array,
            |args| {
                let array = args[0].as_array();
                let mut start = args[1].as_int();
                let mut end = args[2].as_int();

                let len = array.len() as i32;

                if start < 0 {
                    start = (len + start).max(0);
                }
                if end < 0 {
                    end = (len + end).max(0);
                }

                start = start.max(0).min(len);
                end = end.max(start).min(len);

                let result: Vec<DixValue> = array
                    .iter()
                    .skip(start as usize)
                    .take((end - start) as usize)
                    .cloned()
                    .collect();

                Ok(DixValue::from_array(result))
            },
            "Extracts a portion of the array".to_string(),
            |args| {
                validation_helpers::first_is_array(args)
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
                    && validation_helpers::argument_has_type(2, DixType::Int, args)
            },
        )));

        // Array.filter(array, filterValue) - Filter array by value
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "filter".to_string(),
            2,
            DixType::Array,
            |args| {
                let array = args[0].as_array();
                let filter_value = &args[1];

                let filtered: Vec<DixValue> = array
                    .iter()
                    .filter(|item| item.equal_to(filter_value))
                    .cloned()
                    .collect();

                Ok(DixValue::from_array(filtered))
            },
            "Filters array to include only matching values".to_string(),
            validation_helpers::first_is_array,
        )));

        // Array.contains(array, value) - Check if array contains value
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "contains".to_string(),
            2,
            DixType::Bool,
            |args| {
                let array = args[0].as_array();
                let search_value = &args[1];

                let contains = array.iter().any(|item| item.equal_to(search_value));
                Ok(DixValue::from_bool(contains))
            },
            "Checks if array contains the specified value".to_string(),
            validation_helpers::first_is_array,
        )));

        // Array.indexOf(array, value) - Find index of value
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "indexOf".to_string(),
            2,
            DixType::Int,
            |args| {
                let array = args[0].as_array();
                let search_value = &args[1];

                for (i, item) in array.iter().enumerate() {
                    if item.equal_to(search_value) {
                        return Ok(DixValue::from_int(i as i32));
                    }
                }

                Ok(DixValue::from_int(-1))
            },
            "Finds the index of the first occurrence of a value".to_string(),
            validation_helpers::first_is_array,
        )));

        // Array.lastIndexOf(array, value) - Find last index of value
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "lastIndexOf".to_string(),
            2,
            DixType::Int,
            |args| {
                let array = args[0].as_array();
                let search_value = &args[1];

                for (i, item) in array.iter().enumerate().rev() {
                    if item.equal_to(search_value) {
                        return Ok(DixValue::from_int(i as i32));
                    }
                }

                Ok(DixValue::from_int(-1))
            },
            "Finds the index of the last occurrence of a value".to_string(),
            validation_helpers::first_is_array,
        )));

        // Array.flatten(array) - Flatten nested arrays
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "flatten".to_string(),
            1,
            DixType::Array,
            |args| {
                let array = args[0].as_array();
                let mut result = Vec::new();

                flatten_array(array, &mut result);

                Ok(DixValue::from_array(result))
            },
            "Flattens nested arrays into a single array".to_string(),
            validation_helpers::first_is_array,
        )));

        // Array.sum(array) - Sum numeric array
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "sum".to_string(),
            1,
            DixType::Double,
            |args| {
                let array = args[0].as_array();
                let mut sum = 0.0;

                for item in array {
                    if !item.is_numeric() {
                        return Err("Array contains non-numeric values".to_string());
                    }
                    sum += item.as_double();
                }

                Ok(DixValue::from_double(sum))
            },
            "Calculates the sum of numeric array elements".to_string(),
            validation_helpers::first_is_array,
        )));

        // Array.average(array) - Average of numeric array
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "average".to_string(),
            1,
            DixType::Double,
            |args| {
                let array = args[0].as_array();

                if array.is_empty() {
                    return Err("Cannot calculate average of empty array".to_string());
                }

                let mut sum = 0.0;
                for item in array {
                    if !item.is_numeric() {
                        return Err("Array contains non-numeric values".to_string());
                    }
                    sum += item.as_double();
                }

                Ok(DixValue::from_double(sum / array.len() as f64))
            },
            "Calculates the average of numeric array elements".to_string(),
            validation_helpers::first_is_array,
        )));

        // Array.min(array) - Find minimum value
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "min".to_string(),
            1,
            DixType::Double,
            |args| {
                let array = args[0].as_array();

                if array.is_empty() {
                    return Err("Cannot find minimum of empty array".to_string());
                }

                let mut min = f64::MAX;
                for item in array {
                    if !item.is_numeric() {
                        return Err("Array contains non-numeric values".to_string());
                    }
                    min = min.min(item.as_double());
                }

                Ok(DixValue::from_double(min))
            },
            "Finds the minimum value in a numeric array".to_string(),
            validation_helpers::first_is_array,
        )));

        // Array.max(array) - Find maximum value
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "max".to_string(),
            1,
            DixType::Double,
            |args| {
                let array = args[0].as_array();

                if array.is_empty() {
                    return Err("Cannot find maximum of empty array".to_string());
                }

                let mut max = f64::MIN;
                for item in array {
                    if !item.is_numeric() {
                        return Err("Array contains non-numeric values".to_string());
                    }
                    max = max.max(item.as_double());
                }

                Ok(DixValue::from_double(max))
            },
            "Finds the maximum value in a numeric array".to_string(),
            validation_helpers::first_is_array,
        )));
    }
}

impl Default for ArrayObject {
    fn default() -> Self {
        Self::new()
    }
}

impl IStaticObject for ArrayObject {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn call_method(&self, method_name: &str, args: &[DixValue]) -> Result<DixValue, String> {
        self.base.call_method(method_name, args)
    }

    fn has_method(&self, method_name: &str) -> bool {
        self.base.has_method(method_name)
    }

    fn get_method_names(&self) -> Vec<String> {
        self.base.get_method_names()
    }

    fn get_method(&self, method_name: &str) -> Option<&dyn IBuiltinMethod> {
        self.base.get_method(method_name)
    }
}

// ==================== HELPER FUNCTIONS ====================

/// Helper method to recursively flatten nested arrays
fn flatten_array(source: &[DixValue], target: &mut Vec<DixValue>) {
    for item in source {
        if item.get_type() == DixType::Array {
            flatten_array(item.as_array(), target);
        } else {
            target.push(item.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_range() {
        let array_obj = ArrayObject::new();
        let result = array_obj
            .call_method("range", &[DixValue::from_int(1), DixValue::from_int(5)])
            .unwrap();

        assert_eq!(result.as_array().len(), 5);
        assert_eq!(result.as_array()[0].as_int(), 1);
        assert_eq!(result.as_array()[4].as_int(), 5);
    }

    #[test]
    fn test_array_fill() {
        let array_obj = ArrayObject::new();
        let result = array_obj
            .call_method(
                "fill",
                &[DixValue::from_string("x".to_string()), DixValue::from_int(3)],
            )
            .unwrap();

        assert_eq!(result.as_array().len(), 3);
        assert_eq!(result.as_array()[0].as_string(), "x");
    }
}