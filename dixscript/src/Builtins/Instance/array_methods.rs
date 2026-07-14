// dixscript/src/Builtins/Instance/array_methods.rs
// src/Builtins/Instance/array_methods.rs
//! Array instance methods for DixScript
//! All methods take the array as the first argument (instance parameter)

use crate::Builtins::Core::{DixValue, DixType, IBuiltinMethod, BuiltinMethod, validation_helpers};
use std::collections::HashMap;

/// Get all array instance methods
pub fn get_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    methods.insert("length".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "length".to_string(), 1, DixType::Int, array_length,
        "Returns the number of elements in the array".to_string(),
        validation_helpers::first_is_array,
    )));

    methods.insert("contains".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "contains".to_string(), 2, DixType::Bool, array_contains,
        "Checks if the array contains the specified element".to_string(),
        validation_helpers::first_is_array,
    )));

    methods.insert("indexOf".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "indexOf".to_string(), 2, DixType::Int, array_index_of,
        "Returns the index of the first occurrence of the element, or -1 if not found".to_string(),
        validation_helpers::first_is_array,
    )));

    methods.insert("lastIndexOf".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "lastIndexOf".to_string(), 2, DixType::Int, array_last_index_of,
        "Returns the index of the last occurrence of the element, or -1 if not found".to_string(),
        validation_helpers::first_is_array,
    )));

    // Return type is Any — element type is unknown without generics
    methods.insert("get".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "get".to_string(), 2,
        DixType::Any,   // ← was String (wrong); element type varies
        array_get,
        "Returns the element at the specified index".to_string(),
        |args| {
            validation_helpers::first_is_array(args)
                && validation_helpers::argument_has_type(1, DixType::Int, args)
                && validation_helpers::valid_array_index(&args[0], &args[1])
        },
    )));

    methods.insert("set".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "set".to_string(), 3, DixType::Array, array_set,
        "Returns a new array with the element at the specified index set to the new value".to_string(),
        |args| {
            validation_helpers::first_is_array(args)
                && validation_helpers::argument_has_type(1, DixType::Int, args)
                && validation_helpers::valid_array_index(&args[0], &args[1])
        },
    )));

    methods.insert("push".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "push".to_string(), 2, DixType::Array, array_push,
        "Returns a new array with the element added to the end".to_string(),
        validation_helpers::first_is_array,
    )));

    methods.insert("pop".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "pop".to_string(), 1, DixType::Array, array_pop,
        "Returns a new array with the last element removed".to_string(),
        |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
    )));

    methods.insert("shift".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "shift".to_string(), 1, DixType::Array, array_shift,
        "Returns a new array with the first element removed".to_string(),
        |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
    )));

    methods.insert("unshift".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "unshift".to_string(), 2, DixType::Array, array_unshift,
        "Returns a new array with the element added to the beginning".to_string(),
        validation_helpers::first_is_array,
    )));

    methods.insert("slice".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "slice".to_string(), 3, DixType::Array, array_slice,
        "Returns a new array containing elements from start index to end index (exclusive)".to_string(),
        |args| {
            validation_helpers::first_is_array(args)
                && validation_helpers::argument_has_type(1, DixType::Int, args)
                && validation_helpers::argument_has_type(2, DixType::Int, args)
        },
    )));

    methods.insert("join".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "join".to_string(), 2, DixType::String, array_join,
        "Joins all array elements into a string using the specified separator".to_string(),
        |args| {
            validation_helpers::first_is_array(args)
                && args[1].get_type() == DixType::String
        },
    )));

    methods.insert("reverse".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "reverse".to_string(), 1, DixType::Array, array_reverse,
        "Returns a new array with elements in reverse order".to_string(),
        validation_helpers::first_is_array,
    )));

    methods.insert("sort".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "sort".to_string(), 1, DixType::Array, array_sort,
        "Returns a new array with elements sorted in ascending order".to_string(),
        validation_helpers::first_is_array,
    )));

    methods.insert("concat".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "concat".to_string(), 2, DixType::Array, array_concat,
        "Returns a new array that is the concatenation of this array and another array".to_string(),
        |args| {
            validation_helpers::first_is_array(args)
                && validation_helpers::argument_has_type(1, DixType::Array, args)
        },
    )));

    methods.insert("filter".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "filter".to_string(), 2, DixType::Array, array_filter,
        "Returns a new array with elements NOT equal to the filter value".to_string(),
        validation_helpers::first_is_array,
    )));

    methods.insert("flatten".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "flatten".to_string(), 1, DixType::Array, array_flatten,
        "Flattens nested arrays by one level".to_string(),
        validation_helpers::first_is_array,
    )));

    methods.insert("isEmpty".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "isEmpty".to_string(), 1, DixType::Bool, array_is_empty,
        "Checks if the array is empty".to_string(),
        validation_helpers::first_is_array,
    )));

    // Return type Any — element type is not statically known
    methods.insert("first".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "first".to_string(), 1,
        DixType::Any,   // ← was String (wrong)
        array_first,
        "Returns the first element of the array".to_string(),
        |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
    )));

    // Return type Any — element type is not statically known
    methods.insert("last".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "last".to_string(), 1,
        DixType::Any,   // ← was String (wrong)
        array_last,
        "Returns the last element of the array".to_string(),
        |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
    )));

    methods.insert("distinct".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "distinct".to_string(), 1, DixType::Array, array_distinct,
        "Returns a new array with duplicate elements removed".to_string(),
        validation_helpers::first_is_array,
    )));

    methods.insert("count".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "count".to_string(), 2, DixType::Int, array_count,
        "Returns the number of occurrences of the specified element".to_string(),
        validation_helpers::first_is_array,
    )));

    methods.insert("max".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "max".to_string(), 1, DixType::Double, array_max,
        "Returns the maximum numeric value in the array".to_string(),
        |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
    )));

    methods.insert("min".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "min".to_string(), 1, DixType::Double, array_min,
        "Returns the minimum numeric value in the array".to_string(),
        |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
    )));

    methods.insert("sum".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "sum".to_string(), 1, DixType::Double, array_sum,
        "Returns the sum of all numeric values in the array".to_string(),
        validation_helpers::first_is_array,
    )));

    methods.insert("average".to_string(), Box::new(BuiltinMethod::new_with_validator(
        "average".to_string(), 1, DixType::Double, array_average,
        "Returns the average of all numeric values in the array".to_string(),
        |args| validation_helpers::first_is_array(args) && !args[0].as_array().is_empty(),
    )));

    methods
}

// ==================== METHOD IMPLEMENTATIONS ====================

fn array_length(args: &[DixValue]) -> Result<DixValue, String> {
    Ok(DixValue::from_int(args[0].as_array().len() as i32))
}

fn array_contains(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let element = &args[1];
    Ok(DixValue::from_bool(array.iter().any(|item| item.equal_to(element))))
}

fn array_index_of(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let element = &args[1];
    for (i, item) in array.iter().enumerate() {
        if item.equal_to(element) { return Ok(DixValue::from_int(i as i32)); }
    }
    Ok(DixValue::from_int(-1))
}

fn array_last_index_of(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let element = &args[1];
    for (i, item) in array.iter().enumerate().rev() {
        if item.equal_to(element) { return Ok(DixValue::from_int(i as i32)); }
    }
    Ok(DixValue::from_int(-1))
}

fn array_get(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let index = args[1].as_int() as usize;
    if index >= array.len() { return Err("Array index out of bounds".to_string()); }
    Ok(array[index].clone())
}

fn array_set(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let index = args[1].as_int() as usize;
    if index >= array.len() { return Err("Array index out of bounds".to_string()); }
    let mut new_array = array.clone();
    new_array[index] = args[2].deep_clone();
    Ok(DixValue::from_array(new_array))
}

fn array_push(args: &[DixValue]) -> Result<DixValue, String> {
    let mut new_array = args[0].as_array().clone();
    new_array.push(args[1].deep_clone());
    Ok(DixValue::from_array(new_array))
}

fn array_pop(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    if array.is_empty() { return Err("Cannot pop from empty array".to_string()); }
    let mut new_array = array.clone();
    new_array.pop();
    Ok(DixValue::from_array(new_array))
}

fn array_shift(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    if array.is_empty() { return Err("Cannot shift from empty array".to_string()); }
    Ok(DixValue::from_array(array[1..].to_vec()))
}

fn array_unshift(args: &[DixValue]) -> Result<DixValue, String> {
    let mut new_array = vec![args[1].deep_clone()];
    new_array.extend(args[0].as_array().iter().cloned());
    Ok(DixValue::from_array(new_array))
}

fn array_slice(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    let len = array.len() as i32;
    let mut start = args[1].as_int().max(0);
    let end   = args[2].as_int().min(len);
    if start > end { start = end; }
    Ok(DixValue::from_array(array[start as usize..end as usize].to_vec()))
}

fn array_join(args: &[DixValue]) -> Result<DixValue, String> {
    let separator = args[1].as_string();
    let strings: Vec<String> = args[0].as_array().iter().map(|item| item.as_string()).collect();
    Ok(DixValue::from_string(strings.join(&separator)))
}

fn array_reverse(args: &[DixValue]) -> Result<DixValue, String> {
    let mut reversed = args[0].as_array().clone();
    reversed.reverse();
    Ok(DixValue::from_array(reversed))
}

fn array_sort(args: &[DixValue]) -> Result<DixValue, String> {
    let mut sorted = args[0].as_array().clone();
    sorted.sort_by_key(|a| a.as_string());
    Ok(DixValue::from_array(sorted))
}

fn array_concat(args: &[DixValue]) -> Result<DixValue, String> {
    let mut combined = args[0].as_array().clone();
    combined.extend(args[1].as_array().iter().cloned());
    Ok(DixValue::from_array(combined))
}

fn array_filter(args: &[DixValue]) -> Result<DixValue, String> {
    let filter_value = &args[1];
    let filtered = args[0].as_array()
        .iter()
        .filter(|item| !item.equal_to(filter_value))
        .cloned()
        .collect();
    Ok(DixValue::from_array(filtered))
}

fn array_flatten(args: &[DixValue]) -> Result<DixValue, String> {
    let mut flattened = Vec::new();
    for item in args[0].as_array().iter() {
        if item.get_type() == DixType::Array {
            flattened.extend(item.as_array().iter().cloned());
        } else {
            flattened.push(item.clone());
        }
    }
    Ok(DixValue::from_array(flattened))
}

fn array_is_empty(args: &[DixValue]) -> Result<DixValue, String> {
    Ok(DixValue::from_bool(args[0].as_array().is_empty()))
}

fn array_first(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    if array.is_empty() { return Err("Cannot get first element of empty array".to_string()); }
    Ok(array[0].clone())
}

fn array_last(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    if array.is_empty() { return Err("Cannot get last element of empty array".to_string()); }
    Ok(array[array.len() - 1].clone())
}

fn array_distinct(args: &[DixValue]) -> Result<DixValue, String> {
    let mut distinct: Vec<DixValue> = Vec::new();
    for item in args[0].as_array().iter() {
        if !distinct.iter().any(|d: &DixValue| d.equal_to(item)) {
            distinct.push(item.clone());
        }
    }
    Ok(DixValue::from_array(distinct))
}

fn array_count(args: &[DixValue]) -> Result<DixValue, String> {
    let element = &args[1];
    let count = args[0].as_array().iter().filter(|item| item.equal_to(element)).count();
    Ok(DixValue::from_int(count as i32))
}

fn array_max(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    if array.is_empty() { return Err("Cannot find max of empty array".to_string()); }
    let max = array.iter().map(|i| i.as_double()).fold(f64::NEG_INFINITY, f64::max);
    Ok(DixValue::from_double(max))
}

fn array_min(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    if array.is_empty() { return Err("Cannot find min of empty array".to_string()); }
    let min = array.iter().map(|i| i.as_double()).fold(f64::INFINITY, f64::min);
    Ok(DixValue::from_double(min))
}

fn array_sum(args: &[DixValue]) -> Result<DixValue, String> {
    let sum: f64 = args[0].as_array().iter().map(|i| i.as_double()).sum();
    Ok(DixValue::from_double(sum))
}

fn array_average(args: &[DixValue]) -> Result<DixValue, String> {
    let array = args[0].as_array();
    if array.is_empty() { return Err("Cannot calculate average of empty array".to_string()); }
    let sum: f64 = array.iter().map(|i| i.as_double()).sum();
    Ok(DixValue::from_double(sum / array.len() as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_length() {
        let arr = DixValue::from_array(vec![
            DixValue::from_int(1), DixValue::from_int(2), DixValue::from_int(3),
        ]);
        assert_eq!(array_length(&[arr]).unwrap().as_int(), 3);
    }

    #[test]
    fn test_array_first_returns_correct_element() {
        let arr = DixValue::from_array(vec![
            DixValue::from_int(42), DixValue::from_int(99),
        ]);
        assert_eq!(array_first(&[arr]).unwrap().as_int(), 42);
    }

    #[test]
    fn test_array_last_returns_correct_element() {
        let arr = DixValue::from_array(vec![
            DixValue::from_int(1), DixValue::from_int(7),
        ]);
        assert_eq!(array_last(&[arr]).unwrap().as_int(), 7);
    }

    #[test]
    fn test_first_last_return_type_is_any() {
        let methods = get_methods();
        assert_eq!(methods["first"].return_type(), DixType::Any);
        assert_eq!(methods["last"].return_type(),  DixType::Any);
        assert_eq!(methods["get"].return_type(),   DixType::Any);
    }

    #[test]
    fn test_array_contains() {
        let arr = DixValue::from_array(vec![
            DixValue::from_int(1), DixValue::from_int(2), DixValue::from_int(3),
        ]);
        assert!(array_contains(&[arr.clone(), DixValue::from_int(2)]).unwrap().as_bool());
        assert!(!array_contains(&[arr, DixValue::from_int(5)]).unwrap().as_bool());
    }

    #[test]
    fn test_array_sum_return_type() {
        let methods = get_methods();
        assert_eq!(methods["sum"].return_type(), DixType::Double);
    }

    #[test]
    fn test_array_flatten() {
        let inner1 = DixValue::from_array(vec![DixValue::from_int(1), DixValue::from_int(2)]);
        let inner2 = DixValue::from_array(vec![DixValue::from_int(3), DixValue::from_int(4)]);
        let arr = DixValue::from_array(vec![inner1, inner2]);
        let result = array_flatten(&[arr]).unwrap();
        assert_eq!(result.as_array().len(), 4);
    }
}
