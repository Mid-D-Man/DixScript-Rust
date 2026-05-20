
//! Number instance methods for DixScript (Int, Long, Float, Double)

use crate::Builtins::Core::{
    DixType, DixValue, IBuiltinMethod, BuiltinMethod, validation_helpers,
};
use std::collections::HashMap;

/// Get all instance methods for Int type
pub fn get_int_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // Int.abs()
    methods.insert(
        "abs".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "abs".to_string(),
            1,
            DixType::Int,
            |args| {
                Ok(DixValue::from_int(args[0].as_int().abs()))
            },
            "Returns the absolute value of the integer".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.toString()
    methods.insert(
        "toString".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toString".to_string(),
            1,
            DixType::String,
            |args| {
                Ok(DixValue::from_string(args[0].as_int().to_string()))
            },
            "Converts the integer to a string representation".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.toFloat()
    methods.insert(
        "toFloat".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toFloat".to_string(),
            1,
            DixType::Float,
            |args| {
                Ok(DixValue::from_float(args[0].as_int() as f32))
            },
            "Converts the integer to a float".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.toDouble()
    methods.insert(
        "toDouble".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toDouble".to_string(),
            1,
            DixType::Double,
            |args| {
                Ok(DixValue::from_double(args[0].as_int() as f64))
            },
            "Converts the integer to a double".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.toLong()
    methods.insert(
        "toLong".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toLong".to_string(),
            1,
            DixType::Long,
            |args| {
                Ok(DixValue::from_long(args[0].as_int() as i64))
            },
            "Converts the integer to a long (64-bit)".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.sign()
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

    // Int.isEven()
    methods.insert(
        "isEven".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isEven".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_int() % 2 == 0))
            },
            "Checks if the integer is even".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.isOdd()
    methods.insert(
        "isOdd".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isOdd".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_int() % 2 != 0))
            },
            "Checks if the integer is odd".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.isPositive()
    methods.insert(
        "isPositive".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isPositive".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_int() > 0))
            },
            "Checks if the integer is positive".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    // Int.isNegative()
    methods.insert(
        "isNegative".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isNegative".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_int() < 0))
            },
            "Checks if the integer is negative".to_string(),
            |args| args[0].get_type() == DixType::Int,
        )),
    );

    methods
}

/// Get all instance methods for Long (i64) type
pub fn get_long_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // Long.abs()
    methods.insert(
        "abs".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "abs".to_string(),
            1,
            DixType::Long,
            |args| {
                Ok(DixValue::from_long(args[0].as_long().abs()))
            },
            "Returns the absolute value of the long".to_string(),
            |args| args[0].get_type() == DixType::Long,
        )),
    );

    // Long.toString()
    methods.insert(
        "toString".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toString".to_string(),
            1,
            DixType::String,
            |args| {
                Ok(DixValue::from_string(args[0].as_long().to_string()))
            },
            "Converts the long to a string representation".to_string(),
            |args| args[0].get_type() == DixType::Long,
        )),
    );

    // Long.toInt()  — truncates; use when you know the value fits i32
    methods.insert(
        "toInt".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toInt".to_string(),
            1,
            DixType::Int,
            |args| {
                Ok(DixValue::from_int(args[0].as_long() as i32))
            },
            "Converts the long to an integer (truncating cast to i32)".to_string(),
            |args| args[0].get_type() == DixType::Long,
        )),
    );

    // Long.toFloat()
    methods.insert(
        "toFloat".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toFloat".to_string(),
            1,
            DixType::Float,
            |args| {
                Ok(DixValue::from_float(args[0].as_long() as f32))
            },
            "Converts the long to a float (possible precision loss)".to_string(),
            |args| args[0].get_type() == DixType::Long,
        )),
    );

    // Long.toDouble()
    methods.insert(
        "toDouble".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toDouble".to_string(),
            1,
            DixType::Double,
            |args| {
                Ok(DixValue::from_double(args[0].as_long() as f64))
            },
            "Converts the long to a double (possible precision loss for very large values)".to_string(),
            |args| args[0].get_type() == DixType::Long,
        )),
    );

    // Long.sign()
    methods.insert(
        "sign".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "sign".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = args[0].as_long();
                let sign = if value > 0 { 1 } else if value < 0 { -1 } else { 0 };
                Ok(DixValue::from_int(sign))
            },
            "Returns the sign of the long (-1, 0, or 1)".to_string(),
            |args| args[0].get_type() == DixType::Long,
        )),
    );

    // Long.isEven()
    methods.insert(
        "isEven".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isEven".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_long() % 2 == 0))
            },
            "Checks if the long is even".to_string(),
            |args| args[0].get_type() == DixType::Long,
        )),
    );

    // Long.isOdd()
    methods.insert(
        "isOdd".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isOdd".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_long() % 2 != 0))
            },
            "Checks if the long is odd".to_string(),
            |args| args[0].get_type() == DixType::Long,
        )),
    );

    // Long.isPositive()
    methods.insert(
        "isPositive".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isPositive".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_long() > 0))
            },
            "Checks if the long is positive".to_string(),
            |args| args[0].get_type() == DixType::Long,
        )),
    );

    // Long.isNegative()
    methods.insert(
        "isNegative".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isNegative".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_long() < 0))
            },
            "Checks if the long is negative".to_string(),
            |args| args[0].get_type() == DixType::Long,
        )),
    );

    // Long.fitsInInt() — convenience: true when value is in i32 range
    methods.insert(
        "fitsInInt".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "fitsInInt".to_string(),
            1,
            DixType::Bool,
            |args| {
                let v = args[0].as_long();
                Ok(DixValue::from_bool(v >= i32::MIN as i64 && v <= i32::MAX as i64))
            },
            "Returns true if the long value fits in an i32 without truncation".to_string(),
            |args| args[0].get_type() == DixType::Long,
        )),
    );

    methods
}

/// Get all instance methods for Float type
pub fn get_float_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // Float.abs()
    methods.insert(
        "abs".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "abs".to_string(),
            1,
            DixType::Float,
            |args| {
                Ok(DixValue::from_float(args[0].as_float().abs()))
            },
            "Returns the absolute value of the float".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.toString()
    methods.insert(
        "toString".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toString".to_string(),
            1,
            DixType::String,
            |args| {
                Ok(DixValue::from_string(args[0].as_float().to_string()))
            },
            "Converts the float to a string representation".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.toInt()
    methods.insert(
        "toInt".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toInt".to_string(),
            1,
            DixType::Int,
            |args| {
                Ok(DixValue::from_int(args[0].as_float() as i32))
            },
            "Converts the float to an integer (truncated)".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.toLong()
    methods.insert(
        "toLong".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toLong".to_string(),
            1,
            DixType::Long,
            |args| {
                Ok(DixValue::from_long(args[0].as_float() as i64))
            },
            "Converts the float to a long (truncated)".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.toDouble()
    methods.insert(
        "toDouble".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toDouble".to_string(),
            1,
            DixType::Double,
            |args| {
                Ok(DixValue::from_double(args[0].as_float() as f64))
            },
            "Converts the float to a double".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.round(decimalPlaces)
    methods.insert(
        "round".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "round".to_string(),
            2,
            DixType::Float,
            |args| {
                let value   = args[0].as_float();
                let decimals = args[1].as_int();
                if decimals < 0 {
                    return Err("Decimal places cannot be negative".to_string());
                }
                let multiplier = 10_f32.powi(decimals);
                Ok(DixValue::from_float((value * multiplier).round() / multiplier))
            },
            "Rounds the float to the specified number of decimal places".to_string(),
            |args| {
                args[0].get_type() == DixType::Float
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
            },
        )),
    );

    // Float.floor()
    methods.insert(
        "floor".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "floor".to_string(),
            1,
            DixType::Int,
            |args| {
                Ok(DixValue::from_int(args[0].as_float().floor() as i32))
            },
            "Returns the largest integer less than or equal to the float".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.ceil()
    methods.insert(
        "ceil".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "ceil".to_string(),
            1,
            DixType::Int,
            |args| {
                Ok(DixValue::from_int(args[0].as_float().ceil() as i32))
            },
            "Returns the smallest integer greater than or equal to the float".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.sign()
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

    // Float.isNaN()
    methods.insert(
        "isNaN".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isNaN".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_float().is_nan()))
            },
            "Checks if the float is NaN (Not a Number)".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.isInfinity()
    methods.insert(
        "isInfinity".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isInfinity".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_float().is_infinite()))
            },
            "Checks if the float is infinite".to_string(),
            |args| args[0].get_type() == DixType::Float,
        )),
    );

    // Float.isFinite()
    methods.insert(
        "isFinite".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isFinite".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_float().is_finite()))
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

    // Double.abs()
    methods.insert(
        "abs".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "abs".to_string(),
            1,
            DixType::Double,
            |args| {
                Ok(DixValue::from_double(args[0].as_double().abs()))
            },
            "Returns the absolute value of the double".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.toString()
    methods.insert(
        "toString".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toString".to_string(),
            1,
            DixType::String,
            |args| {
                Ok(DixValue::from_string(args[0].as_double().to_string()))
            },
            "Converts the double to a string representation".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.toInt()
    methods.insert(
        "toInt".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toInt".to_string(),
            1,
            DixType::Int,
            |args| {
                Ok(DixValue::from_int(args[0].as_double() as i32))
            },
            "Converts the double to an integer (truncated)".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.toLong()
    methods.insert(
        "toLong".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toLong".to_string(),
            1,
            DixType::Long,
            |args| {
                Ok(DixValue::from_long(args[0].as_double() as i64))
            },
            "Converts the double to a long (truncated)".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.toFloat()
    methods.insert(
        "toFloat".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toFloat".to_string(),
            1,
            DixType::Float,
            |args| {
                Ok(DixValue::from_float(args[0].as_double() as f32))
            },
            "Converts the double to a float".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.toDouble() — identity
    methods.insert(
        "toDouble".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toDouble".to_string(),
            1,
            DixType::Double,
            |args| {
                Ok(DixValue::from_double(args[0].as_double()))
            },
            "Returns the double value (identity conversion)".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.round(decimalPlaces)
    methods.insert(
        "round".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "round".to_string(),
            2,
            DixType::Double,
            |args| {
                let value    = args[0].as_double();
                let decimals = args[1].as_int();
                if decimals < 0 {
                    return Err("Decimal places cannot be negative".to_string());
                }
                let multiplier = 10_f64.powi(decimals);
                Ok(DixValue::from_double((value * multiplier).round() / multiplier))
            },
            "Rounds the double to the specified number of decimal places".to_string(),
            |args| {
                args[0].get_type() == DixType::Double
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
            },
        )),
    );

    // Double.floor()
    methods.insert(
        "floor".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "floor".to_string(),
            1,
            DixType::Int,
            |args| {
                Ok(DixValue::from_int(args[0].as_double().floor() as i32))
            },
            "Returns the largest integer less than or equal to the double".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.ceil()
    methods.insert(
        "ceil".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "ceil".to_string(),
            1,
            DixType::Int,
            |args| {
                Ok(DixValue::from_int(args[0].as_double().ceil() as i32))
            },
            "Returns the smallest integer greater than or equal to the double".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.sign()
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

    // Double.isNaN()
    methods.insert(
        "isNaN".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isNaN".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_double().is_nan()))
            },
            "Checks if the double is NaN (Not a Number)".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.isInfinity()
    methods.insert(
        "isInfinity".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isInfinity".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_double().is_infinite()))
            },
            "Checks if the double is infinite".to_string(),
            |args| args[0].get_type() == DixType::Double,
        )),
    );

    // Double.isFinite()
    methods.insert(
        "isFinite".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "isFinite".to_string(),
            1,
            DixType::Bool,
            |args| {
                Ok(DixValue::from_bool(args[0].as_double().is_finite()))
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
        let result = methods.get("abs").unwrap()
            .call(&[DixValue::from_int(-42)]).unwrap();
        assert_eq!(result.as_int(), 42);
    }

    #[test]
    fn test_int_to_long() {
        let methods = get_int_methods();
        let result = methods.get("toLong").unwrap()
            .call(&[DixValue::from_int(100)]).unwrap();
        assert_eq!(result.get_type(), DixType::Long);
        assert_eq!(result.as_long(), 100_i64);
    }

    #[test]
    fn test_long_abs() {
        let methods = get_long_methods();
        let result = methods.get("abs").unwrap()
            .call(&[DixValue::from_long(-9_000_000_000_i64)]).unwrap();
        assert_eq!(result.as_long(), 9_000_000_000_i64);
    }

    #[test]
    fn test_long_sign() {
        let methods = get_long_methods();
        let pos = methods.get("sign").unwrap()
            .call(&[DixValue::from_long(42_i64)]).unwrap();
        assert_eq!(pos.as_int(), 1);
        let neg = methods.get("sign").unwrap()
            .call(&[DixValue::from_long(-1_i64)]).unwrap();
        assert_eq!(neg.as_int(), -1);
        let zero = methods.get("sign").unwrap()
            .call(&[DixValue::from_long(0_i64)]).unwrap();
        assert_eq!(zero.as_int(), 0);
    }

    #[test]
    fn test_long_to_int_truncates() {
        let methods = get_long_methods();
        let result = methods.get("toInt").unwrap()
            .call(&[DixValue::from_long(i64::MAX)]).unwrap();
        // Just verify it doesn't panic — truncation is expected
        let _ = result.as_int();
    }

    #[test]
    fn test_long_fits_in_int() {
        let methods = get_long_methods();
        let fits = methods.get("fitsInInt").unwrap()
            .call(&[DixValue::from_long(42_i64)]).unwrap();
        assert!(fits.as_bool());

        let doesnt = methods.get("fitsInInt").unwrap()
            .call(&[DixValue::from_long(i64::MAX)]).unwrap();
        assert!(!doesnt.as_bool());
    }

    #[test]
    fn test_long_is_even() {
        let methods = get_long_methods();
        let even = methods.get("isEven").unwrap()
            .call(&[DixValue::from_long(9_000_000_000_i64)]).unwrap();
        assert!(even.as_bool());
        let odd = methods.get("isEven").unwrap()
            .call(&[DixValue::from_long(9_000_000_001_i64)]).unwrap();
        assert!(!odd.as_bool());
    }

    #[test]
    fn test_double_to_long() {
        let methods = get_double_methods();
        let result = methods.get("toLong").unwrap()
            .call(&[DixValue::from_double(3.99)]).unwrap();
        assert_eq!(result.get_type(), DixType::Long);
        assert_eq!(result.as_long(), 3_i64);
    }

    #[test]
    fn test_float_round() {
        let methods = get_float_methods();
        let result = methods.get("round").unwrap()
            .call(&[DixValue::from_float(3.14159), DixValue::from_int(2)])
            .unwrap();
        assert!((result.as_float() - 3.14).abs() < 0.01);
    }

    #[test]
    fn test_double_is_nan() {
        let methods = get_double_methods();
        let result = methods.get("isNaN").unwrap()
            .call(&[DixValue::from_double(f64::NAN)]).unwrap();
        assert!(result.as_bool());
    }
}