// src/Builtins/Instance/tuple_methods.rs
//! Tuple instance methods for DixScript
//! Tuples have max 6 elements in DixScript

use crate::Builtins::Core::{DixType, DixValue, IBuiltinMethod, BuiltinMethod};
use std::collections::HashMap;

/// Get all tuple instance methods
pub fn get_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // Tuple.length() - Get number of elements in tuple
    methods.insert(
        "length".to_string(),
        Box::new(BuiltinMethod::new(
            "length".to_string(),
            1,
            DixType::Int,
            length,
            "Returns the number of elements in the tuple".to_string(),
        )),
    );

    // Tuple.get(index) - Get element at index (0-5)
    methods.insert(
        "get".to_string(),
        Box::new(BuiltinMethod::new(
            "get".to_string(),
            2,
            DixType::Null, // Can return any type
            get,
            "Returns the element at the specified index (0-5)".to_string(),
        )),
    );

    // Tuple.first() - Get first element
    methods.insert(
        "first".to_string(),
        Box::new(BuiltinMethod::new(
            "first".to_string(),
            1,
            DixType::Null,
            first,
            "Returns the first element (index 0)".to_string(),
        )),
    );

    // Tuple.second() - Get second element
    methods.insert(
        "second".to_string(),
        Box::new(BuiltinMethod::new(
            "second".to_string(),
            1,
            DixType::Null,
            second,
            "Returns the second element (index 1)".to_string(),
        )),
    );

    // Tuple.third() - Get third element
    methods.insert(
        "third".to_string(),
        Box::new(BuiltinMethod::new(
            "third".to_string(),
            1,
            DixType::Null,
            third,
            "Returns the third element (index 2)".to_string(),
        )),
    );

    // Tuple.fourth() - Get fourth element
    methods.insert(
        "fourth".to_string(),
        Box::new(BuiltinMethod::new(
            "fourth".to_string(),
            1,
            DixType::Null,
            fourth,
            "Returns the fourth element (index 3)".to_string(),
        )),
    );

    // Tuple.fifth() - Get fifth element
    methods.insert(
        "fifth".to_string(),
        Box::new(BuiltinMethod::new(
            "fifth".to_string(),
            1,
            DixType::Null,
            fifth,
            "Returns the fifth element (index 4)".to_string(),
        )),
    );

    // Tuple.sixth() - Get sixth element
    methods.insert(
        "sixth".to_string(),
        Box::new(BuiltinMethod::new(
            "sixth".to_string(),
            1,
            DixType::Null,
            sixth,
            "Returns the sixth element (index 5)".to_string(),
        )),
    );

    // Tuple.contains(value) - Check if tuple contains value
    methods.insert(
        "contains".to_string(),
        Box::new(BuiltinMethod::new(
            "contains".to_string(),
            2,
            DixType::Bool,
            contains,
            "Checks if the tuple contains the specified value".to_string(),
        )),
    );

    // Tuple.toArray() - Convert tuple to array
    methods.insert(
        "toArray".to_string(),
        Box::new(BuiltinMethod::new(
            "toArray".to_string(),
            1,
            DixType::Array,
            to_array,
            "Converts the tuple to an array".to_string(),
        )),
    );

    // Tuple.reverse() - Reverse tuple elements
    methods.insert(
        "reverse".to_string(),
        Box::new(BuiltinMethod::new(
            "reverse".to_string(),
            1,
            DixType::Tuple,
            reverse,
            "Returns a new tuple with elements in reverse order".to_string(),
        )),
    );

    // Tuple.swap(index1, index2) - Swap two elements by index
    methods.insert(
        "swap".to_string(),
        Box::new(BuiltinMethod::new(
            "swap".to_string(),
            3,
            DixType::Tuple,
            swap,
            "Returns a new tuple with two elements swapped".to_string(),
        )),
    );

    methods
}

// ==================== METHOD IMPLEMENTATIONS ====================

/// Get tuple length
fn length(args: &[DixValue]) -> Result<DixValue, String> {
    let tuple = &args[0];

    if tuple.get_type() != DixType::Tuple {
        return Err(format!("Cannot call length() on {:?}", tuple.get_type()));
    }

    let elements = tuple.as_array();
    Ok(DixValue::from_int(elements.len() as i32))
}

/// Get element at index (0-5)
fn get(args: &[DixValue]) -> Result<DixValue, String> {
    let tuple = &args[0];
    let index_value = &args[1];

    if tuple.get_type() != DixType::Tuple {
        return Err(format!("Cannot call get() on {:?}", tuple.get_type()));
    }

    if !index_value.is_numeric() {
        return Err("Index must be numeric".to_string());
    }

    let elements = tuple.as_array();
    let index = index_value.as_int();

    if index < 0 || index >= elements.len() as i32 {
        return Err(format!(
            "Tuple index {} out of range [0, {}]",
            index,
            elements.len() - 1
        ));
    }

    Ok(elements[index as usize].clone())
}

/// Get first element
fn first(args: &[DixValue]) -> Result<DixValue, String> {
    let tuple = &args[0];

    if tuple.get_type() != DixType::Tuple {
        return Err(format!("Cannot call first() on {:?}", tuple.get_type()));
    }

    let elements = tuple.as_array();

    if elements.is_empty() {
        return Err("Tuple is empty".to_string());
    }

    Ok(elements[0].clone())
}

/// Get second element
fn second(args: &[DixValue]) -> Result<DixValue, String> {
    let tuple = &args[0];

    if tuple.get_type() != DixType::Tuple {
        return Err(format!("Cannot call second() on {:?}", tuple.get_type()));
    }

    let elements = tuple.as_array();

    if elements.len() < 2 {
        return Err("Tuple does not have a second element".to_string());
    }

    Ok(elements[1].clone())
}

/// Get third element
fn third(args: &[DixValue]) -> Result<DixValue, String> {
    let tuple = &args[0];

    if tuple.get_type() != DixType::Tuple {
        return Err(format!("Cannot call third() on {:?}", tuple.get_type()));
    }

    let elements = tuple.as_array();

    if elements.len() < 3 {
        return Err("Tuple does not have a third element".to_string());
    }

    Ok(elements[2].clone())
}

/// Get fourth element
fn fourth(args: &[DixValue]) -> Result<DixValue, String> {
    let tuple = &args[0];

    if tuple.get_type() != DixType::Tuple {
        return Err(format!("Cannot call fourth() on {:?}", tuple.get_type()));
    }

    let elements = tuple.as_array();

    if elements.len() < 4 {
        return Err("Tuple does not have a fourth element".to_string());
    }

    Ok(elements[3].clone())
}

/// Get fifth element
fn fifth(args: &[DixValue]) -> Result<DixValue, String> {
    let tuple = &args[0];

    if tuple.get_type() != DixType::Tuple {
        return Err(format!("Cannot call fifth() on {:?}", tuple.get_type()));
    }

    let elements = tuple.as_array();

    if elements.len() < 5 {
        return Err("Tuple does not have a fifth element".to_string());
    }

    Ok(elements[4].clone())
}

/// Get sixth element
fn sixth(args: &[DixValue]) -> Result<DixValue, String> {
    let tuple = &args[0];

    if tuple.get_type() != DixType::Tuple {
        return Err(format!("Cannot call sixth() on {:?}", tuple.get_type()));
    }

    let elements = tuple.as_array();

    if elements.len() < 6 {
        return Err("Tuple does not have a sixth element".to_string());
    }

    Ok(elements[5].clone())
}

/// Check if tuple contains value
fn contains(args: &[DixValue]) -> Result<DixValue, String> {
    let tuple = &args[0];
    let search_value = &args[1];

    if tuple.get_type() != DixType::Tuple {
        return Err(format!("Cannot call contains() on {:?}", tuple.get_type()));
    }

    let elements = tuple.as_array();

    for element in elements {
        if element.equal_to(search_value) {
            return Ok(DixValue::from_bool(true));
        }
    }

    Ok(DixValue::from_bool(false))
}

/// Convert tuple to array
fn to_array(args: &[DixValue]) -> Result<DixValue, String> {
    let tuple = &args[0];

    if tuple.get_type() != DixType::Tuple {
        return Err(format!("Cannot call toArray() on {:?}", tuple.get_type()));
    }

    let elements = tuple.as_array().clone();
    Ok(DixValue::from_array(elements))
}

/// Reverse tuple elements
fn reverse(args: &[DixValue]) -> Result<DixValue, String> {
    let tuple = &args[0];

    if tuple.get_type() != DixType::Tuple {
        return Err(format!("Cannot call reverse() on {:?}", tuple.get_type()));
    }

    let elements = tuple.as_array();
    let mut reversed: Vec<DixValue> = elements.iter().rev().cloned().collect();

    Ok(DixValue::from_tuple(reversed))
}

/// Swap two elements by index
fn swap(args: &[DixValue]) -> Result<DixValue, String> {
    let tuple = &args[0];
    let index1_value = &args[1];
    let index2_value = &args[2];

    if tuple.get_type() != DixType::Tuple {
        return Err(format!("Cannot call swap() on {:?}", tuple.get_type()));
    }

    if !index1_value.is_numeric() || !index2_value.is_numeric() {
        return Err("Indices must be numeric".to_string());
    }

    let mut elements = tuple.as_array().clone();
    let index1 = index1_value.as_int();
    let index2 = index2_value.as_int();

    if index1 < 0 || index1 >= elements.len() as i32 {
        return Err(format!(
            "Index {} out of range [0, {}]",
            index1,
            elements.len() - 1
        ));
    }

    if index2 < 0 || index2 >= elements.len() as i32 {
        return Err(format!(
            "Index {} out of range [0, {}]",
            index2,
            elements.len() - 1
        ));
    }

    // Swap
    elements.swap(index1 as usize, index2 as usize);

    Ok(DixValue::from_tuple(elements))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tuple_length() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(1),
            DixValue::from_int(2),
            DixValue::from_int(3),
        ]);

        let result = length(&[tuple]).unwrap();
        assert_eq!(result.as_int(), 3);
    }

    #[test]
    fn test_tuple_accessors() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(10),
            DixValue::from_int(20),
            DixValue::from_int(30),
            DixValue::from_int(40),
            DixValue::from_int(50),
            DixValue::from_int(60),
        ]);

        assert_eq!(first(&[tuple.clone()]).unwrap().as_int(), 10);
        assert_eq!(second(&[tuple.clone()]).unwrap().as_int(), 20);
        assert_eq!(third(&[tuple.clone()]).unwrap().as_int(), 30);
        assert_eq!(fourth(&[tuple.clone()]).unwrap().as_int(), 40);
        assert_eq!(fifth(&[tuple.clone()]).unwrap().as_int(), 50);
        assert_eq!(sixth(&[tuple.clone()]).unwrap().as_int(), 60);
    }

    #[test]
    fn test_tuple_get() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(10),
            DixValue::from_int(20),
        ]);

        let result = get(&[tuple.clone(), DixValue::from_int(1)]).unwrap();
        assert_eq!(result.as_int(), 20);
    }

    #[test]
    fn test_tuple_contains() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(10),
            DixValue::from_int(20),
        ]);

        let result = contains(&[tuple.clone(), DixValue::from_int(20)]).unwrap();
        assert_eq!(result.as_bool(), true);

        let result = contains(&[tuple, DixValue::from_int(99)]).unwrap();
        assert_eq!(result.as_bool(), false);
    }

    #[test]
    fn test_tuple_reverse() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(1),
            DixValue::from_int(2),
            DixValue::from_int(3),
        ]);

        let result = reverse(&[tuple]).unwrap();
        let elements = result.as_array();

        assert_eq!(elements[0].as_int(), 3);
        assert_eq!(elements[1].as_int(), 2);
        assert_eq!(elements[2].as_int(), 1);
    }

    #[test]
    fn test_tuple_swap() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(10),
            DixValue::from_int(20),
            DixValue::from_int(30),
        ]);

        let result = swap(&[
            tuple,
            DixValue::from_int(0),
            DixValue::from_int(2),
        ])
            .unwrap();

        let elements = result.as_array();
        assert_eq!(elements[0].as_int(), 30);
        assert_eq!(elements[1].as_int(), 20);
        assert_eq!(elements[2].as_int(), 10);
    }
}