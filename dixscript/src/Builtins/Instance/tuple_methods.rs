// dixscript/src/Builtins/Instance/tuple_methods.rs
//! Tuple instance methods for DixScript
//! Tuples have max 6 elements in DixScript

use crate::Builtins::Core::{DixType, DixValue, IBuiltinMethod, BuiltinMethod};
use std::collections::HashMap;

pub fn get_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    methods.insert("length".to_string(), Box::new(BuiltinMethod::new(
        "length".to_string(), 1, DixType::Int, length,
        "Returns the number of elements in the tuple".to_string(),
    )));

    methods.insert("get".to_string(), Box::new(BuiltinMethod::new(
        "get".to_string(), 2,
        DixType::Any,
        get,
        "Returns the element at the specified index (0-5)".to_string(),
    )));

    methods.insert("first".to_string(), Box::new(BuiltinMethod::new(
        "first".to_string(), 1,
        DixType::Any,
        first,
        "Returns the first element (index 0)".to_string(),
    )));

    methods.insert("second".to_string(), Box::new(BuiltinMethod::new(
        "second".to_string(), 1,
        DixType::Any,
        second,
        "Returns the second element (index 1)".to_string(),
    )));

    methods.insert("third".to_string(), Box::new(BuiltinMethod::new(
        "third".to_string(), 1,
        DixType::Any,
        third,
        "Returns the third element (index 2)".to_string(),
    )));

    methods.insert("fourth".to_string(), Box::new(BuiltinMethod::new(
        "fourth".to_string(), 1,
        DixType::Any,
        fourth,
        "Returns the fourth element (index 3)".to_string(),
    )));

    methods.insert("fifth".to_string(), Box::new(BuiltinMethod::new(
        "fifth".to_string(), 1,
        DixType::Any,
        fifth,
        "Returns the fifth element (index 4)".to_string(),
    )));

    methods.insert("sixth".to_string(), Box::new(BuiltinMethod::new(
        "sixth".to_string(), 1,
        DixType::Any,
        sixth,
        "Returns the sixth element (index 5)".to_string(),
    )));

    methods.insert("contains".to_string(), Box::new(BuiltinMethod::new(
        "contains".to_string(), 2, DixType::Bool, contains,
        "Checks if the tuple contains the specified value".to_string(),
    )));

    // Renamed from any() — 'any' clashes with DataType::Any keyword
    methods.insert("containsAny".to_string(), Box::new(BuiltinMethod::new(
        "containsAny".to_string(), 2, DixType::Bool, contains_any,
        "Checks if the tuple contains any element equal to the specified value (alias for contains with intent clarity)".to_string(),
    )));

    methods.insert("toArray".to_string(), Box::new(BuiltinMethod::new(
        "toArray".to_string(), 1, DixType::Array, to_array,
        "Converts the tuple to an array".to_string(),
    )));

    methods.insert("reverse".to_string(), Box::new(BuiltinMethod::new(
        "reverse".to_string(), 1, DixType::Tuple, reverse,
        "Returns a new tuple with elements in reverse order".to_string(),
    )));

    methods.insert("swap".to_string(), Box::new(BuiltinMethod::new(
        "swap".to_string(), 3, DixType::Tuple, swap,
        "Returns a new tuple with two elements swapped".to_string(),
    )));

    methods
}

// ==================== METHOD IMPLEMENTATIONS ====================

fn require_tuple(args: &[DixValue], method: &str) -> Result<Vec<DixValue>, String> {
    if args[0].get_type() != DixType::Tuple {
        return Err(format!("Cannot call {}() on {:?}", method, args[0].get_type()));
    }
    Ok(args[0].as_array().clone())
}

fn length(args: &[DixValue]) -> Result<DixValue, String> {
    let elements = require_tuple(args, "length")?;
    Ok(DixValue::from_int(elements.len() as i32))
}

fn get(args: &[DixValue]) -> Result<DixValue, String> {
    let elements = require_tuple(args, "get")?;
    if !args[1].is_numeric() { return Err("Index must be numeric".to_string()); }
    let index = args[1].as_int();
    if index < 0 || index >= elements.len() as i32 {
        return Err(format!("Tuple index {} out of range [0, {}]", index, elements.len() - 1));
    }
    Ok(elements[index as usize].clone())
}

fn first(args: &[DixValue]) -> Result<DixValue, String> {
    let elements = require_tuple(args, "first")?;
    if elements.is_empty() { return Err("Tuple is empty".to_string()); }
    Ok(elements[0].clone())
}

fn second(args: &[DixValue]) -> Result<DixValue, String> {
    let elements = require_tuple(args, "second")?;
    if elements.len() < 2 { return Err("Tuple does not have a second element".to_string()); }
    Ok(elements[1].clone())
}

fn third(args: &[DixValue]) -> Result<DixValue, String> {
    let elements = require_tuple(args, "third")?;
    if elements.len() < 3 { return Err("Tuple does not have a third element".to_string()); }
    Ok(elements[2].clone())
}

fn fourth(args: &[DixValue]) -> Result<DixValue, String> {
    let elements = require_tuple(args, "fourth")?;
    if elements.len() < 4 { return Err("Tuple does not have a fourth element".to_string()); }
    Ok(elements[3].clone())
}

fn fifth(args: &[DixValue]) -> Result<DixValue, String> {
    let elements = require_tuple(args, "fifth")?;
    if elements.len() < 5 { return Err("Tuple does not have a fifth element".to_string()); }
    Ok(elements[4].clone())
}

fn sixth(args: &[DixValue]) -> Result<DixValue, String> {
    let elements = require_tuple(args, "sixth")?;
    if elements.len() < 6 { return Err("Tuple does not have a sixth element".to_string()); }
    Ok(elements[5].clone())
}

fn contains(args: &[DixValue]) -> Result<DixValue, String> {
    let elements = require_tuple(args, "contains")?;
    let search = &args[1];
    Ok(DixValue::from_bool(elements.iter().any(|e| e.equal_to(search))))
}

fn contains_any(args: &[DixValue]) -> Result<DixValue, String> {
    // Same logic as contains — exists as a distinct named method for intent clarity
    // and to replace the old any() which clashed with DataType::Any
    let elements = require_tuple(args, "containsAny")?;
    let search = &args[1];
    Ok(DixValue::from_bool(elements.iter().any(|e| e.equal_to(search))))
}

fn to_array(args: &[DixValue]) -> Result<DixValue, String> {
    let elements = require_tuple(args, "toArray")?;
    Ok(DixValue::from_array(elements))
}

fn reverse(args: &[DixValue]) -> Result<DixValue, String> {
    let elements = require_tuple(args, "reverse")?;
    let reversed: Vec<DixValue> = elements.into_iter().rev().collect();
    Ok(DixValue::from_tuple(reversed))
}

fn swap(args: &[DixValue]) -> Result<DixValue, String> {
    let mut elements = require_tuple(args, "swap")?;
    if !args[1].is_numeric() || !args[2].is_numeric() {
        return Err("Indices must be numeric".to_string());
    }
    let i1 = args[1].as_int();
    let i2 = args[2].as_int();
    for idx in [i1, i2] {
        if idx < 0 || idx >= elements.len() as i32 {
            return Err(format!("Index {} out of range [0, {}]", idx, elements.len() - 1));
        }
    }
    elements.swap(i1 as usize, i2 as usize);
    Ok(DixValue::from_tuple(elements))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_positional_accessors_return_type_is_any() {
        let methods = get_methods();
        for name in ["first", "second", "third", "fourth", "fifth", "sixth", "get"] {
            assert_eq!(
                methods[name].return_type(), DixType::Any,
                "{} should return Any, not {:?}", name, methods[name].return_type()
            );
        }
    }

    #[test]
    fn test_tuple_length() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(1), DixValue::from_int(2), DixValue::from_int(3),
        ]);
        assert_eq!(length(&[tuple]).unwrap().as_int(), 3);
    }

    #[test]
    fn test_tuple_accessors() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(10), DixValue::from_int(20), DixValue::from_int(30),
            DixValue::from_int(40), DixValue::from_int(50), DixValue::from_int(60),
        ]);
        assert_eq!(first(&[tuple.clone()]).unwrap().as_int(),  10);
        assert_eq!(second(&[tuple.clone()]).unwrap().as_int(), 20);
        assert_eq!(third(&[tuple.clone()]).unwrap().as_int(),  30);
        assert_eq!(fourth(&[tuple.clone()]).unwrap().as_int(), 40);
        assert_eq!(fifth(&[tuple.clone()]).unwrap().as_int(),  50);
        assert_eq!(sixth(&[tuple.clone()]).unwrap().as_int(),  60);
    }

    #[test]
    fn test_tuple_get() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(10), DixValue::from_int(20),
        ]);
        assert_eq!(get(&[tuple, DixValue::from_int(1)]).unwrap().as_int(), 20);
    }

    #[test]
    fn test_tuple_contains() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(10), DixValue::from_int(20),
        ]);
        assert!(contains(&[tuple.clone(), DixValue::from_int(20)]).unwrap().as_bool());
        assert!(!contains(&[tuple, DixValue::from_int(99)]).unwrap().as_bool());
    }

    #[test]
    fn test_tuple_contains_any() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(10), DixValue::from_int(20),
        ]);
        assert!(contains_any(&[tuple.clone(), DixValue::from_int(10)]).unwrap().as_bool());
        assert!(!contains_any(&[tuple, DixValue::from_int(99)]).unwrap().as_bool());
    }

    #[test]
    fn test_no_any_method_exists() {
        let methods = get_methods();
        assert!(!methods.contains_key("any"), "'any' clashes with DataType::Any and must not exist");
        assert!(methods.contains_key("containsAny"), "containsAny must exist as replacement");
    }

    #[test]
    fn test_tuple_reverse() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(1), DixValue::from_int(2), DixValue::from_int(3),
        ]);
        let result = reverse(&[tuple]).unwrap();
        let elems  = result.as_array();
        assert_eq!(elems[0].as_int(), 3);
        assert_eq!(elems[1].as_int(), 2);
        assert_eq!(elems[2].as_int(), 1);
    }

    #[test]
    fn test_tuple_swap() {
        let tuple = DixValue::from_tuple(vec![
            DixValue::from_int(10), DixValue::from_int(20), DixValue::from_int(30),
        ]);
        let result = swap(&[tuple, DixValue::from_int(0), DixValue::from_int(2)]).unwrap();
        let elems  = result.as_array();
        assert_eq!(elems[0].as_int(), 30);
        assert_eq!(elems[1].as_int(), 20);
        assert_eq!(elems[2].as_int(), 10);
    }

    #[test]
    fn test_require_tuple_rejects_non_tuple() {
        let not_a_tuple = DixValue::from_array(vec![DixValue::from_int(1)]);
        assert!(first(&[not_a_tuple]).is_err());
    }
        }
