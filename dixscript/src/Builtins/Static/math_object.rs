// src/Builtins/Static/math_object.rs
//! Math static object - Mathematical functions
//! Provides max, min, abs, sqrt, trigonometric functions, etc.

use crate::Builtins::Core::{BuiltinMethod, DixType, DixValue, IBuiltinMethod, validation_helpers};
use crate::Builtins::Static::{IStaticObject, StaticObjectBase};
use std::f64::consts::{E, PI};

/// Math static object implementation
pub struct MathObject {
    base: StaticObjectBase,
}

impl MathObject {
    pub fn new() -> Self {
        let mut base = StaticObjectBase::new("Math".to_string());
        Self::initialize_methods(&mut base);
        MathObject { base }
    }

    fn initialize_methods(base: &mut StaticObjectBase) {
        // Math.max(a, b) - Returns maximum of two numbers
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "max".to_string(),
            2,
            DixType::Double,
            |args| {
                let a = args[0].as_double();
                let b = args[1].as_double();
                Ok(DixValue::from_double(a.max(b)))
            },
            "Returns the maximum of two numbers".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.min(a, b) - Returns minimum of two numbers
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "min".to_string(),
            2,
            DixType::Double,
            |args| {
                let a = args[0].as_double();
                let b = args[1].as_double();
                Ok(DixValue::from_double(a.min(b)))
            },
            "Returns the minimum of two numbers".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.abs(x) - Returns absolute value
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "abs".to_string(),
            1,
            DixType::Double,
            |args| {
                let x = args[0].as_double();
                Ok(DixValue::from_double(x.abs()))
            },
            "Returns the absolute value of a number".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.sqrt(x) - Returns square root
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "sqrt".to_string(),
            1,
            DixType::Double,
            |args| {
                let x = args[0].as_double();
                if x < 0.0 {
                    return Err("Cannot calculate square root of negative number".to_string());
                }
                Ok(DixValue::from_double(x.sqrt()))
            },
            "Returns the square root of a number".to_string(),
            |args| validation_helpers::all_numeric(args) && args[0].as_double() >= 0.0,
        )));

        // Math.pow(base, exponent) - Returns base raised to power
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "pow".to_string(),
            2,
            DixType::Double,
            |args| {
                let base_num = args[0].as_double();
                let exponent = args[1].as_double();
                Ok(DixValue::from_double(base_num.powf(exponent)))
            },
            "Returns base raised to the power of exponent".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.floor(x) - Returns largest integer less than or equal to x
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "floor".to_string(),
            1,
            DixType::Int,
            |args| {
                let x = args[0].as_double();
                Ok(DixValue::from_int(x.floor() as i32))
            },
            "Returns the largest integer less than or equal to a number".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.ceil(x) - Returns smallest integer greater than or equal to x
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "ceil".to_string(),
            1,
            DixType::Int,
            |args| {
                let x = args[0].as_double();
                Ok(DixValue::from_int(x.ceil() as i32))
            },
            "Returns the smallest integer greater than or equal to a number".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.round(x) - Returns x rounded to nearest integer
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "round".to_string(),
            1,
            DixType::Int,
            |args| {
                let x = args[0].as_double();
                Ok(DixValue::from_int(x.round() as i32))
            },
            "Returns a number rounded to the nearest integer".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.sin(x) - Returns sine of x (in radians)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "sin".to_string(),
            1,
            DixType::Double,
            |args| {
                let x = args[0].as_double();
                Ok(DixValue::from_double(x.sin()))
            },
            "Returns the sine of an angle in radians".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.cos(x) - Returns cosine of x (in radians)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "cos".to_string(),
            1,
            DixType::Double,
            |args| {
                let x = args[0].as_double();
                Ok(DixValue::from_double(x.cos()))
            },
            "Returns the cosine of an angle in radians".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.tan(x) - Returns tangent of x (in radians)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "tan".to_string(),
            1,
            DixType::Double,
            |args| {
                let x = args[0].as_double();
                Ok(DixValue::from_double(x.tan()))
            },
            "Returns the tangent of an angle in radians".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.log(x) - Returns natural logarithm of x
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "log".to_string(),
            1,
            DixType::Double,
            |args| {
                let x = args[0].as_double();
                if x <= 0.0 {
                    return Err("Cannot calculate logarithm of non-positive number".to_string());
                }
                Ok(DixValue::from_double(x.ln()))
            },
            "Returns the natural logarithm of a number".to_string(),
            |args| validation_helpers::all_numeric(args) && args[0].as_double() > 0.0,
        )));

        // Math.log10(x) - Returns base-10 logarithm of x
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "log10".to_string(),
            1,
            DixType::Double,
            |args| {
                let x = args[0].as_double();
                if x <= 0.0 {
                    return Err("Cannot calculate logarithm of non-positive number".to_string());
                }
                Ok(DixValue::from_double(x.log10()))
            },
            "Returns the base-10 logarithm of a number".to_string(),
            |args| validation_helpers::all_numeric(args) && args[0].as_double() > 0.0,
        )));

        // Math.exp(x) - Returns e raised to power x
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "exp".to_string(),
            1,
            DixType::Double,
            |args| {
                let x = args[0].as_double();
                Ok(DixValue::from_double(x.exp()))
            },
            "Returns e raised to the power of x".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.sign(x) - Returns sign of x (-1, 0, or 1)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "sign".to_string(),
            1,
            DixType::Int,
            |args| {
                let x = args[0].as_double();
                let sign = if x > 0.0 {
                    1
                } else if x < 0.0 {
                    -1
                } else {
                    0
                };
                Ok(DixValue::from_int(sign))
            },
            "Returns the sign of a number (-1, 0, or 1)".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.clamp(value, min, max) - Clamps value between min and max
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "clamp".to_string(),
            3,
            DixType::Double,
            |args| {
                let value = args[0].as_double();
                let min = args[1].as_double();
                let max = args[2].as_double();

                if min > max {
                    return Err("Min value cannot be greater than max value".to_string());
                }

                Ok(DixValue::from_double(value.clamp(min, max)))
            },
            "Clamps a value between minimum and maximum bounds".to_string(),
            |args| {
                validation_helpers::all_numeric(args) && args[1].as_double() <= args[2].as_double()
            },
        )));

        // Math.pi() - Returns PI constant
        base.register_method(Box::new(BuiltinMethod::new(
            "pi".to_string(),
            0,
            DixType::Double,
            |_args| Ok(DixValue::from_double(PI)),
            "Returns the value of PI (3.14159...)".to_string(),
        )));

        // Math.e() - Returns E constant
        base.register_method(Box::new(BuiltinMethod::new(
            "e".to_string(),
            0,
            DixType::Double,
            |_args| Ok(DixValue::from_double(E)),
            "Returns the value of E (2.71828...)".to_string(),
        )));

        // Math.radians(degrees) - Convert degrees to radians
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "radians".to_string(),
            1,
            DixType::Double,
            |args| {
                let degrees = args[0].as_double();
                Ok(DixValue::from_double(degrees.to_radians()))
            },
            "Converts degrees to radians".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.degrees(radians) - Convert radians to degrees
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "degrees".to_string(),
            1,
            DixType::Double,
            |args| {
                let radians = args[0].as_double();
                Ok(DixValue::from_double(radians.to_degrees()))
            },
            "Converts radians to degrees".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.truncate(x) - Returns integer part of x
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "truncate".to_string(),
            1,
            DixType::Int,
            |args| {
                let x = args[0].as_double();
                Ok(DixValue::from_int(x.trunc() as i32))
            },
            "Returns the integer part of a number".to_string(),
            validation_helpers::all_numeric,
        )));

        // Math.remainder(dividend, divisor) - Returns remainder after division
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "remainder".to_string(),
            2,
            DixType::Double,
            |args| {
                let dividend = args[0].as_double();
                let divisor = args[1].as_double();

                if divisor == 0.0 {
                    return Err("Division by zero".to_string());
                }

                Ok(DixValue::from_double(dividend % divisor))
            },
            "Returns the remainder after division".to_string(),
            |args| validation_helpers::all_numeric(args) && args[1].as_double() != 0.0,
        )));
    }
}

impl Default for MathObject {
    fn default() -> Self {
        Self::new()
    }
}

impl IStaticObject for MathObject {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_object_creation() {
        let math = MathObject::new();
        assert_eq!(math.name(), "Math");
        assert!(!math.get_method_names().is_empty());
    }

    #[test]
    fn test_math_max() {
        let math = MathObject::new();
        let result = math
            .call_method(
                "max",
                &[DixValue::from_int(10), DixValue::from_int(20)],
            )
            .unwrap();
        assert_eq!(result.as_double(), 20.0);
    }

    #[test]
    fn test_math_min() {
        let math = MathObject::new();
        let result = math
            .call_method(
                "min",
                &[DixValue::from_int(10), DixValue::from_int(20)],
            )
            .unwrap();
        assert_eq!(result.as_double(), 10.0);
    }

    #[test]
    fn test_math_sqrt() {
        let math = MathObject::new();
        let result = math
            .call_method("sqrt", &[DixValue::from_int(16)])
            .unwrap();
        assert_eq!(result.as_double(), 4.0);
    }

    #[test]
    fn test_math_pow() {
        let math = MathObject::new();
        let result = math
            .call_method(
                "pow",
                &[DixValue::from_int(2), DixValue::from_int(3)],
            )
            .unwrap();
        assert_eq!(result.as_double(), 8.0);
    }

    #[test]
    fn test_math_constants() {
        let math = MathObject::new();

        let pi = math.call_method("pi", &[]).unwrap();
        assert!((pi.as_double() - std::f64::consts::PI).abs() < 0.0001);

        let e = math.call_method("e", &[]).unwrap();
        assert!((e.as_double() - std::f64::consts::E).abs() < 0.0001);
    }

    #[test]
    fn test_math_trigonometry() {
        let math = MathObject::new();

        let sin = math
            .call_method("sin", &[DixValue::from_double(std::f64::consts::PI / 2.0)])
            .unwrap();
        assert!((sin.as_double() - 1.0).abs() < 0.0001);

        let cos = math
            .call_method("cos", &[DixValue::from_double(0.0)])
            .unwrap();
        assert!((cos.as_double() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_math_clamp() {
        let math = MathObject::new();

        let result = math
            .call_method(
                "clamp",
                &[
                    DixValue::from_int(5),
                    DixValue::from_int(1),
                    DixValue::from_int(10),
                ],
            )
            .unwrap();
        assert_eq!(result.as_double(), 5.0);

        let result = math
            .call_method(
                "clamp",
                &[
                    DixValue::from_int(15),
                    DixValue::from_int(1),
                    DixValue::from_int(10),
                ],
            )
            .unwrap();
        assert_eq!(result.as_double(), 10.0);
    }
}