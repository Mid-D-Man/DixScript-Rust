// src/Builtins/Instance/array_methods.rs
//! Array instance methods for DixScript
//! All methods take the array as the first argument (instance parameter)

use crate::Builtins::Core::{DixValue, DixType, IBuiltinMethod, BuiltinMethod, validation_helpers};
use std::collections::HashMap;

/// Get all array instance methods
pub fn get_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // Array.length() - Get array length
    methods.insert(
        "length".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "length".to_string(),
            1,
            DixType::Int,
            array_length,
            "Returns the number of elements in the array".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.contains(element) - Check if contains element
    methods.insert(
        "contains".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "contains".to_string(),
            2,
            DixType::Bool,
            array_contains,
            "Checks if the array contains the specified element".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.indexOf(element) - Find index of element
    methods.insert(
        "indexOf".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "indexOf".to_string(),
            2,
            DixType::Int,
            array_index_of,
            "Returns the index of the first occurrence of the element, or -1 if not found".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.lastIndexOf(element) - Find last index of element
    methods.insert(
        "lastIndexOf".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "lastIndexOf".to_string(),
            2,
            DixType::Int,
            array_last_index_of,
            "Returns the index of the last occurrence of the element, or -1 if not found".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.get(index) - Get element at index
    methods.insert(
        "get".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "get".to_string(),
            2,
            DixType::String, // Return type varies
            array_get,
            "Returns the element at the specified index".to_string(),
            |args| {
                validation_helpers::first_is_array(args)
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
                    && validation_helpers::valid_array_index(&args[0], &args[1])
            },
        )),
    );

    // Array.set(index, value) - Set element at index
    methods.insert(
        "set".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "set".to_string(),
            3,
            DixType::Array,
            array_set,
            "Returns a new array with the element at the specified index set to the new value".to_string(),
            |args| {
                validation_helpers::first_is_array(args)
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
                    && validation_helpers::valid_array_index(&args[0], &args[1])
            },
        )),
    );

    // Array.push(element) - Add element to end
    methods.insert(
        "push".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "push".to_string(),
            2,
            DixType::Array,
            array_push,
            "Returns a new array with the element added to the end".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.pop() - Remove last element
    methods.insert(
        "pop".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "pop".to_string(),
            1,
            DixType::Array,
            array_pop,
            "Returns a new array with the last element removed".to_string(),
            |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
        )),
    );

    // Array.shift() - Remove first element
    methods.insert(
        "shift".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "shift".to_string(),
            1,
            DixType::Array,
            array_shift,
            "Returns a new array with the first element removed".to_string(),
            |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
        )),
    );

    // Array.unshift(element) - Add element to beginning
    methods.insert(
        "unshift".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "unshift".to_string(),
            2,
            DixType::Array,
            array_unshift,
            "Returns a new array with the element added to the beginning".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.slice(start, end) - Extract portion of array
    methods.insert(
        "slice".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "slice".to_string(),
            3,
            DixType::Array,
            array_slice,
            "Returns a new array containing elements from start index to end index (exclusive)".to_string(),
            |args| {
                validation_helpers::first_is_array(args)
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
                    && validation_helpers::argument_has_type(2, DixType::Int, args)
            },
        )),
    );

    // Array.join(separator) - Join elements with separator
    methods.insert(
        "join".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "join".to_string(),
            2,
            DixType::String,
            array_join,
            "Joins all array elements into a string using the specified separator".to_string(),
            |args| {
                validation_helpers::first_is_array(args)
                    && args[1].get_type() == DixType::String
            },
        )),
    );

    // Array.reverse() - Reverse array order
    methods.insert(
        "reverse".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "reverse".to_string(),
            1,
            DixType::Array,
            array_reverse,
            "Returns a new array with elements in reverse order".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.sort() - Sort array
    methods.insert(
        "sort".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "sort".to_string(),
            1,
            DixType::Array,
            array_sort,
            "Returns a new array with elements sorted in ascending order".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.concat(otherArray) - Concatenate arrays
    methods.insert(
        "concat".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "concat".to_string(),
            2,
            DixType::Array,
            array_concat,
            "Returns a new array that is the concatenation of this array and another array".to_string(),
            |args| {
                validation_helpers::first_is_array(args)
                    && validation_helpers::argument_has_type(1, DixType::Array, args)
            },
        )),
    );

    // Array.filter(filterValue) - Filter array elements
    methods.insert(
        "filter".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "filter".to_string(),
            2,
            DixType::Array,
            array_filter,
            "Returns a new array with elements NOT equal to the filter value (removes matching elements)".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.flatten() - Flatten nested arrays
    methods.insert(
        "flatten".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "flatten".to_string(),
            1,
            DixType::Array,
            array_flatten,
            "Flattens nested arrays by one level (e.g., [[1,2],[3,4]] becomes [1,2,3,4])".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.isEmpty() - Check if array is empty
    methods.insert(
        "isEmpty".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isEmpty".to_string(),
            1,
            DixType::Bool,
            array_is_empty,
            "Checks if the array is empty".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.first() - Get first element
    methods.insert(
        "first".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "first".to_string(),
            1,
            DixType::String, // Return type varies
            array_first,
            "Returns the first element of the array".to_string(),
            |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
        )),
    );

    // Array.last() - Get last element
    methods.insert(
        "last".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "last".to_string(),
            1,
            DixType::String, // Return type varies
            array_last,
            "Returns the last element of the array".to_string(),
            |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
        )),
    );

    // Array.distinct() - Remove duplicate elements
    methods.insert(
        "distinct".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "distinct".to_string(),
            1,
            DixType::Array,
            array_distinct,
            "Returns a new array with duplicate elements removed".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.count(element) - Count occurrences of element
    methods.insert(
        "count".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "count".to_string(),
            2,
            DixType::Int,
            array_count,
            "Returns the number of occurrences of the specified element".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.max() - Get maximum value
    methods.insert(
        "max".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "max".to_string(),
            1,
            DixType::Double,
            array_max,
            "Returns the maximum numeric value in the array".to_string(),
            |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
        )),
    );

    // Array.min() - Get minimum value
    methods.insert(
        "min".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "min".to_string(),
            1,
            DixType::Double,
            array_min,
            "Returns the minimum numeric value in the array".to_string(),
            |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
        )),
    );

    // Array.sum() - Sum all numeric values
    methods.insert(
        "sum".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "sum".to_string(),
            1,
            DixType::Double,
            array_sum,
            "Returns the sum of all numeric values in the array".to_string(),
            validation_helpers::first_is_array,
        )),
    );

    // Array.average() - Calculate average
    methods.insert(
        "average".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "average".to_string(),
            1,
            DixType::Double,
            array_average,
            "Returns the average of all numeric values in the array".to_string(),
            |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
        )),
    );

    methods
}

// ==================== METHOD IMPLEMENTATIONS ====================

fn array_length(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    Ok(DixValue::from_int(array.len() as i32))
}

fn array_contains(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let element = &args[1];

    let contains = array.iter().any(|item| item.equal_to(element));
    Ok(DixValue::from_bool(contains))
}

fn array_index_of(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let element = &args[1];

    for (i, item) in array.iter().enumerate() {
        if item.equal_to(element) {
            return Ok(DixValue::from_int(i as i32));
        }
    }

    Ok(DixValue::from_int(-1))
}

fn array_last_index_of(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let element = &args[1];

    for (i, item) in array.iter().enumerate().rev() {
        if item.equal_to(element) {
            return Ok(DixValue::from_int(i as i32));
        }
    }

    Ok(DixValue::from_int(-1))
}

fn array_get(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let index = args[1].as_int() as usize;

    if index >= array.len() {
        return Err("Array index out of bounds".to_string());
    }

    Ok(array[index].clone())
}

fn array_set(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let index = args[1].as_int() as usize;
    let value = &args[2];

    if index >= array.len() {
        return Err("Array index out of bounds".to_string());
    }

    let mut new_array = array.clone();
    new_array[index] = value.deep_clone();

    Ok(DixValue::from_array(new_array))
}

fn array_push(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let element = &args[1];

    let mut new_array = array.clone();
    new_array.push(element.deep_clone());

    Ok(DixValue::from_array(new_array))
}

fn array_pop(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();

    if array.is_empty() {
        return Err("Cannot pop from empty array".to_string());
    }

    let mut new_array = array.clone();
    new_array.pop();

    Ok(DixValue::from_array(new_array))
}

fn array_shift(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();

    if array.is_empty() {
        return Err("Cannot shift from empty array".to_string());
    }

    let new_array = array[1..].to_vec();
    Ok(DixValue::from_array(new_array))
}

fn array_unshift(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let element = &args[1];

    let mut new_array = vec![element.deep_clone()];
    new_array.extend(array.iter().cloned());

    Ok(DixValue::from_array(new_array))
}

fn array_slice(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let mut start = args[1].as_int();
    let mut end = args[2].as_int();

    let len = array.len() as i32;

    // Clamp values
    if start < 0 {
        start = 0;
    }
    if end > len {
        end = len;
    }
    if start > end {
        start = end;
    }

    let sliced = array[start as usize..end as usize].to_vec();
    Ok(DixValue::from_array(sliced))
}

fn array_join(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let separator = args[1].as_string();

    let strings: Vec<String> = array.iter().map(|item| item.as_string()).collect();
    Ok(DixValue::from_string(strings.join(&separator)))
}

fn array_reverse(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();

    let mut reversed = array.clone();
    reversed.reverse();

    Ok(DixValue::from_array(reversed))
}

fn array_sort(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();

    let mut sorted = array.clone();
    sorted.sort_by(|a, b| a.as_string().cmp(&b.as_string()));

    Ok(DixValue::from_array(sorted))
}

fn array_concat(args: &[DixValue]) -> Result<DixValue, String> {
    let array1 = args[0].as_array();
    let array2 = args[1].as_array();

    let mut combined = array1.clone();
    combined.extend(array2.iter().cloned());

    Ok(DixValue::from_array(combined))
}

fn array_filter(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let filter_value = &args[1];

    // Filter OUT elements matching filterValue (remove matching elements)
    let filtered: Vec<DixValue> = array
        .iter()
        .filter(|item| !item.equal_to(filter_value))
        .cloned()
        .collect();

    Ok(DixValue::from_array(filtered))
}

fn array_flatten(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let mut flattened = Vec::new();

    for item in array.iter() {
        if item.get_type() == DixType::Array {
            // Flatten one level only
            let nested = item.as_array();
            flattened.extend(nested.iter().cloned());
        } else {
            flattened.push(item.clone());
        }
    }

    Ok(DixValue::from_array(flattened))
}

fn array_is_empty(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    Ok(DixValue::from_bool(array.is_empty()))
}

fn array_first(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();

    if array.is_empty() {
        return Err("Cannot get first element of empty array".to_string());
    }

    Ok(array[0].clone())
}

fn array_last(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();

    if array.is_empty() {
        return Err("Cannot get last element of empty array".to_string());
    }

    Ok(array[array.len() - 1].clone())
}

fn array_distinct(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let mut distinct = Vec::new();

    for item in array.iter() {
        if !distinct.iter().any(|d: &DixValue| d.equal_to(item)) {  // ADD TYPE ANNOTATION
            distinct.push(item.clone());
        }
    }

    Ok(DixValue::from_array(distinct))
}

fn array_count(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let element = &args[1];

    let count = array.iter().filter(|item| item.equal_to(element)).count();
    Ok(DixValue::from_int(count as i32))
}

fn array_max(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();

    if array.is_empty() {
        return Err("Cannot find max of empty array".to_string());
    }

    let max = array
        .iter()
        .map(|item| item.as_double())
        .fold(f64::NEG_INFINITY, f64::max);

    Ok(DixValue::from_double(max))
}

fn array_min(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();

    if array.is_empty() {
        return Err("Cannot find min of empty array".to_string());
    }

    let min = array
        .iter()
        .map(|item| item.as_double())
        .fold(f64::INFINITY, f64::min);

    Ok(DixValue::from_double(min))
}

fn array_sum(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();

    let sum: f64 = array.iter().map(|item| item.as_double()).sum();
    Ok(DixValue::from_double(sum))
}

fn array_average(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();

    if array.is_empty() {
        return Err("Cannot calculate average of empty array".to_string());
    }

    let sum: f64 = array.iter().map(|item| item.as_double()).sum();
    let avg = sum / array.len() as f64;

    Ok(DixValue::from_double(avg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_length() {
        let arr = DixValue::from_array(vec![
            DixValue::from_int(1),
            DixValue::from_int(2),
            DixValue::from_int(3),
        ]);

        let result = array_length(&[arr]).unwrap();
        assert_eq!(result.as_int(), 3);
    }

    #[test]
    fn test_array_contains() {
        let arr = DixValue::from_array(vec![
            DixValue::from_int(1),
            DixValue::from_int(2),
            DixValue::from_int(3),
        ]);

        let result = array_contains(&[arr.clone(), DixValue::from_int(2)]).unwrap();
        assert!(result.as_bool());

        let result = array_contains(&[arr, DixValue::from_int(5)]).unwrap();
        assert!(!result.as_bool());
    }

    #[test]
    fn test_array_push() {
        let arr = DixValue::from_array(vec![
            DixValue::from_int(1),
            DixValue::from_int(2),
        ]);

        let result = array_push(&[arr, DixValue::from_int(3)]).unwrap();
        assert_eq!(result.as_array().len(), 3);
        assert_eq!(result.as_array()[2].as_int(), 3);
    }

    #[test]
    fn test_array_flatten() {
        let inner1 = DixValue::from_array(vec![DixValue::from_int(1), DixValue::from_int(2)]);
        let inner2 = DixValue::from_array(vec![DixValue::from_int(3), DixValue::from_int(4)]);
        let arr = DixValue::from_array(vec![inner1, inner2]);

        let result = array_flatten(&[arr]).unwrap();
        assert_eq!(result.as_array().len(), 4);
        assert_eq!(result.as_array()[0].as_int(), 1);
        assert_eq!(result.as_array()[3].as_int(), 4);
    }
}