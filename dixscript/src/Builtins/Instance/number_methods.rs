// src/Builtins/Instance/number_methods.rs
//! Number instance methods for DixScript (Int, Float, Double)

use crate::Builtins::Core::{
    DixType, DixValue, IBuiltinMethod, BuiltinMethod, validation_helpers,
};
use std::collections::HashMap;

/// Get all instance methods for Int type
pub fn get_int_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // Int.abs() - Absolute value
    methods.insert(
        "abs".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "abs".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = args[0].as_int();
                Ok(DixValue::from_int(value.abs()))
            },
            "Returns the absolute value of the integer".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.toString() - Convert to string
    methods.insert(
        "toString".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toString".to_string(),
            1,
            DixType::String,
            |args| {
                let value = args[0].as_int();
                Ok(DixValue::from_string(value.to_string()))
            },
            "Converts the integer to a string representation".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.toFloat() - Convert to float
    methods.insert(
        "toFloat".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toFloat".to_string(),
            1,
            DixType::Float,
            |args| {
                let value = args[0].as_int();
                Ok(DixValue::from_float(value as f32))
            },
            "Converts the integer to a float".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.toDouble() - Convert to double
    methods.insert(
        "toDouble".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toDouble".to_string(),
            1,
            DixType::Double,
            |args| {
                let value = args[0].as_int();
                Ok(DixValue::from_double(value as f64))
            },
            "Converts the integer to a double".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.sign() - Get sign (-1, 0, or 1)
    methods.insert(
        "sign".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "sign".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = args[0].as_int();
                let sign = if value > 0 { 1 } else if value < 0 { -1 } else { 0 };
                Ok(DixValue::from_int(sign))
            },
            "Returns the sign of the integer (-1, 0, or 1)".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.isEven() - Check if even
    methods.insert(
        "isEven".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isEven".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = args[0].as_int();
                Ok(DixValue::from_bool(value % 2 == 0))
            },
            "Checks if the integer is even".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.isOdd() - Check if odd
    methods.insert(
        "isOdd".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isOdd".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = args[0].as_int();
                Ok(DixValue::from_bool(value % 2 != 0))
            },
            "Checks if the integer is odd".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.isPositive() - Check if positive
    methods.insert(
        "isPositive".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isPositive".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = args[0].as_int();
                Ok(DixValue::from_bool(value > 0))
            },
            "Checks if the integer is positive".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.isNegative() - Check if negative
    methods.insert(
        "isNegative".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isNegative".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = args[0].as_int();
                Ok(DixValue::from_bool(value < 0))
            },
            "Checks if the integer is negative".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    methods
}

/// Get all instance methods for Float type
pub fn get_float_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // Float.abs() - Absolute value
    methods.insert(
        "abs".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "abs".to_string(),
            1,
            DixType::Float,
            |args| {
                let value = args[0].as_float();
                Ok(DixValue::from_float(value.abs()))
            },
            "Returns the absolute value of the float".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.toString() - Convert to string
    methods.insert(
        "toString".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toString".to_string(),
            1,
            DixType::String,
            |args| {
                let value = args[0].as_float();
                Ok(DixValue::from_string(value.to_string()))
            },
            "Converts the float to a string representation".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.toInt() - Convert to integer (truncated)
    methods.insert(
        "toInt".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toInt".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = args[0].as_float();
                Ok(DixValue::from_int(value as i32))
            },
            "Converts the float to an integer (truncated)".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.toDouble() - Convert to double
    methods.insert(
        "toDouble".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toDouble".to_string(),
            1,
            DixType::Double,
            |args| {
                let value = args[0].as_float();
                Ok(DixValue::from_double(value as f64))
            },
            "Converts the float to a double".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.round(decimalPlaces) - Round to decimal places
    methods.insert(
        "round".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "round".to_string(),
            2,
            DixType::Float,
            |args| {
                let value = args[0].as_float();
                let decimals = args[1].as_int();

                if decimals < 0 {
                    return Err("Decimal places cannot be negative".to_string());
                }

                let multiplier = 10_f32.powi(decimals);
                let rounded = (value * multiplier).round() / multiplier;
                Ok(DixValue::from_float(rounded))
            },
            "Rounds the float to the specified number of decimal places".to_string(),
            |args| {
                args[0].get_type() == DixType::Float
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
            },
        )),
    );

    // Float.floor() - Floor to integer
    methods.insert(
        "floor".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "floor".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = args[0].as_float();
                Ok(DixValue::from_int(value.floor() as i32))
            },
            "Returns the largest integer less than or equal to the float".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.ceil() - Ceiling to integer
    methods.insert(
        "ceil".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "ceil".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = args[0].as_float();
                Ok(DixValue::from_int(value.ceil() as i32))
            },
            "Returns the smallest integer greater than or equal to the float".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.sign() - Get sign (-1, 0, or 1)
    methods.insert(
        "sign".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "sign".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = args[0].as_float();
                let sign = if value > 0.0 { 1 } else if value < 0.0 { -1 } else { 0 };
                Ok(DixValue::from_int(sign))
            },
            "Returns the sign of the float (-1, 0, or 1)".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.isNaN() - Check if Not a Number
    methods.insert(
        "isNaN".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isNaN".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = args[0].as_float();
                Ok(DixValue::from_bool(value.is_nan()))
            },
            "Checks if the float is NaN (Not a Number)".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.isInfinity() - Check if infinite
    methods.insert(
        "isInfinity".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isInfinity".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = args[0].as_float();
                Ok(DixValue::from_bool(value.is_infinite()))
            },
            "Checks if the float is infinite".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.isFinite() - Check if finite
    methods.insert(
        "isFinite".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isFinite".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = args[0].as_float();
                Ok(DixValue::from_bool(value.is_finite()))
            },
            "Checks if the float is finite (not NaN or infinite)".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    methods
}

/// Get all instance methods for Double type
pub fn get_double_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // Double.toDouble() - Identity conversion
    methods.insert(
        "toDouble".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toDouble".to_string(),
            1,
            DixType::Double,
            |args| {
                let value = args[0].as_double();
                Ok(DixValue::from_double(value))
            },
            "Returns the double value (identity conversion - already double)".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.abs() - Absolute value
    methods.insert(
        "abs".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "abs".to_string(),
            1,
            DixType::Double,
            |args| {
                let value = args[0].as_double();
                Ok(DixValue::from_double(value.abs()))
            },
            "Returns the absolute value of the double".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.toString() - Convert to string
    methods.insert(
        "toString".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toString".to_string(),
            1,
            DixType::String,
            |args| {
                let value = args[0].as_double();
                Ok(DixValue::from_string(value.to_string()))
            },
            "Converts the double to a string representation".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.toInt() - Convert to integer (truncated)
    methods.insert(
        "toInt".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toInt".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = args[0].as_double();
                Ok(DixValue::from_int(value as i32))
            },
            "Converts the double to an integer (truncated)".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.toFloat() - Convert to float
    methods.insert(
        "toFloat".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toFloat".to_string(),
            1,
            DixType::Float,
            |args| {
                let value = args[0].as_double();
                Ok(DixValue::from_float(value as f32))
            },
            "Converts the double to a float".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.round(decimalPlaces) - Round to decimal places
    methods.insert(
        "round".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "round".to_string(),
            2,
            DixType::Double,
            |args| {
                let value = args[0].as_double();
                let decimals = args[1].as_int();

                if decimals < 0 {
                    return Err("Decimal places cannot be negative".to_string());
                }

                let multiplier = 10_f64.powi(decimals);
                let rounded = (value * multiplier).round() / multiplier;
                Ok(DixValue::from_double(rounded))
            },
            "Rounds the double to the specified number of decimal places".to_string(),
            |args| {
                args[0].get_type() == DixType::Double
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
            },
        )),
    );

    // Double.floor() - Floor to integer
    methods.insert(
        "floor".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "floor".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = args[0].as_double();
                Ok(DixValue::from_int(value.floor() as i32))
            },
            "Returns the largest integer less than or equal to the double".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.ceil() - Ceiling to integer
    methods.insert(
        "ceil".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "ceil".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = args[0].as_double();
                Ok(DixValue::from_int(value.ceil() as i32))
            },
            "Returns the smallest integer greater than or equal to the double".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.sign() - Get sign (-1, 0, or 1)
    methods.insert(
        "sign".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "sign".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = args[0].as_double();
                let sign = if value > 0.0 { 1 } else if value < 0.0 { -1 } else { 0 };
                Ok(DixValue::from_int(sign))
            },
            "Returns the sign of the double (-1, 0, or 1)".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.isNaN() - Check if Not a Number
    methods.insert(
        "isNaN".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isNaN".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = args[0].as_double();
                Ok(DixValue::from_bool(value.is_nan()))
            },
            "Checks if the double is NaN (Not a Number)".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.isInfinity() - Check if infinite
    methods.insert(
        "isInfinity".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isInfinity".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = args[0].as_double();
                Ok(DixValue::from_bool(value.is_infinite()))
            },
            "Checks if the double is infinite".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.isFinite() - Check if finite
    methods.insert(
        "isFinite".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isFinite".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = args[0].as_double();
                Ok(DixValue::from_bool(value.is_finite()))
            },
            "Checks if the double is finite (not NaN or infinite)".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    methods
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_int_abs() {
        let methods = get_int_methods();
        let abs_method = methods.get("abs").unwrap();

        let result = abs_method.call(&[DixValue::from_int(-42)]).unwrap();
        assert_eq!(result.as_int(), 42);
    }

    #[test]
    fn test_int_is_even() {
        let methods = get_int_methods();
        let is_even = methods.get("isEven").unwrap();

        assert!(is_even.call(&[DixValue::from_int(4)]).unwrap().as_bool());
        assert!(!is_even.call(&[DixValue::from_int(5)]).unwrap().as_bool());
    }

    #[test]
    fn test_float_round() {
        let methods = get_float_methods();
        let round_method = methods.get("round").unwrap();

        let result = round_method
            .call(&[DixValue::from_float(3.14159), DixValue::from_int(2)])
            .unwrap();
        assert!((result.as_float() - 3.14).abs() < 0.01);
    }

    #[test]
    fn test_double_is_nan() {
        let methods = get_double_methods();
        let is_nan = methods.get("isNaN").unwrap();

        assert!(is_nan.call(&[DixValue::from_double(f64::NAN)]).unwrap().as_bool());
        assert!(!is_nan.call(&[DixValue::from_double(42.0)]).unwrap().as_bool());
    }
}