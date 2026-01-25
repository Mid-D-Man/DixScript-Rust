// src/Builtins/Static/random_object.rs
//! Random static object implementation for DixScript
//! Provides random number generation and selection functions

use crate::Builtins::Core::{BuiltinMethod, DixType, DixValue, IBuiltinMethod, validation_helpers};
use crate::Builtins::Static::{IStaticObject, StaticObjectBase};
use rand::Rng;

/// Random static object implementation
/// Note: Does not store ThreadRng to maintain thread safety (Send + Sync)
/// ThreadRng is created on-demand in each method
pub struct RandomObject {
    base: StaticObjectBase,
}

impl RandomObject {
    pub fn new() -> Self {
        let mut base = StaticObjectBase::new("Random".to_string());
        Self::initialize_methods(&mut base);
        RandomObject { base }
    }

    fn initialize_methods(base: &mut StaticObjectBase) {
        // Random.range(min, max) - Random integer between min and max (inclusive)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "range".to_string(),
            2,
            DixType::Int,
            |args| {
                let min = args[0].as_int();
                let max = args[1].as_int();

                if min > max {
                    return Err("Min value cannot be greater than max value".to_string());
                }

                let mut rng = rand::thread_rng();
                let value = rng.gen_range(min..=max);

                Ok(DixValue::from_int(value))
            },
            "Returns a random integer between min and max (inclusive)".to_string(),
            validation_helpers::all_numeric,
        )));

        // Random.float() - Random float between 0.0 and 1.0
        base.register_method(Box::new(BuiltinMethod::new(
            "float".to_string(),
            0,
            DixType::Float,
            |_args| {
                let mut rng = rand::thread_rng();
                let value: f32 = rng.gen();
                Ok(DixValue::from_float(value))
            },
            "Returns a random float between 0.0 and 1.0".to_string(),
        )));

        // Random.double() - Random double between 0.0 and 1.0
        base.register_method(Box::new(BuiltinMethod::new(
            "double".to_string(),
            0,
            DixType::Double,
            |_args| {
                let mut rng = rand::thread_rng();
                let value: f64 = rng.gen();
                Ok(DixValue::from_double(value))
            },
            "Returns a random double between 0.0 and 1.0".to_string(),
        )));

        // Random.boolean() - Random true or false
        base.register_method(Box::new(BuiltinMethod::new(
            "boolean".to_string(),
            0,
            DixType::Bool,
            |_args| {
                let mut rng = rand::thread_rng();
                let value: bool = rng.gen();
                Ok(DixValue::from_bool(value))
            },
            "Returns a random boolean value".to_string(),
        )));

        // Random.floatRange(min, max) - Random float between min and max
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "floatRange".to_string(),
            2,
            DixType::Float,
            |args| {
                let min = args[0].as_float();
                let max = args[1].as_float();

                if min > max {
                    return Err("Min value cannot be greater than max value".to_string());
                }

                let mut rng = rand::thread_rng();
                let value = rng.gen_range(min..=max);

                Ok(DixValue::from_float(value))
            },
            "Returns a random float between min and max".to_string(),
            validation_helpers::all_numeric,
        )));

        // Random.doubleRange(min, max) - Random double between min and max
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "doubleRange".to_string(),
            2,
            DixType::Double,
            |args| {
                let min = args[0].as_double();
                let max = args[1].as_double();

                if min > max {
                    return Err("Min value cannot be greater than max value".to_string());
                }

                let mut rng = rand::thread_rng();
                let value = rng.gen_range(min..=max);

                Ok(DixValue::from_double(value))
            },
            "Returns a random double between min and max".to_string(),
            validation_helpers::all_numeric,
        )));

        // Random.choice(array) - Random element from array
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "choice".to_string(),
            1,
            DixType::String,
            |args| {
                let array = args[0].as_array();

                if array.is_empty() {
                    return Err("Cannot choose from empty array".to_string());
                }

                let mut rng = rand::thread_rng();
                let index = rng.gen_range(0..array.len());

                Ok(array[index].clone())
            },
            "Returns a random element from an array".to_string(),
            validation_helpers::first_is_array,
        )));

        // Random.choices(array, count) - Multiple random elements (with replacement)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "choices".to_string(),
            2,
            DixType::Array,
            |args| {
                let array = args[0].as_array();
                let count = args[1].as_int();

                if array.is_empty() {
                    return Err("Cannot choose from empty array".to_string());
                }

                if count < 0 {
                    return Err("Count cannot be negative".to_string());
                }

                if count > 10000 {
                    return Err("Count cannot exceed 10000".to_string());
                }

                let mut rng = rand::thread_rng();
                let mut result = Vec::new();

                for _ in 0..count {
                    let index = rng.gen_range(0..array.len());
                    result.push(array[index].deep_clone());
                }

                Ok(DixValue::from_array(result))
            },
            "Returns multiple random elements from an array (with replacement)".to_string(),
            |args| {
                validation_helpers::first_is_array(args)
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
            },
        )));

        // Random.sample(array, count) - Multiple random elements (without replacement)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "sample".to_string(),
            2,
            DixType::Array,
            |args| {
                let array = args[0].as_array();
                let count = args[1].as_int();

                if array.is_empty() {
                    return Err("Cannot sample from empty array".to_string());
                }

                if count < 0 {
                    return Err("Count cannot be negative".to_string());
                }

                if count > array.len() as i32 {
                    return Err(format!(
                        "Cannot sample {} items from array of {} elements",
                        count,
                        array.len()
                    ));
                }

                let mut rng = rand::thread_rng();
                let mut indices: Vec<usize> = (0..array.len()).collect();
                let mut result = Vec::new();

                for _ in 0..count {
                    let random_idx = rng.gen_range(0..indices.len());
                    let array_idx = indices.remove(random_idx);
                    result.push(array[array_idx].deep_clone());
                }

                Ok(DixValue::from_array(result))
            },
            "Returns multiple random elements from an array (without replacement)".to_string(),
            |args| {
                validation_helpers::first_is_array(args)
                    && validation_helpers::argument_has_type(1, DixType::Int, args)
            },
        )));

        // Random.shuffle(array) - Randomly shuffle array
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "shuffle".to_string(),
            1,
            DixType::Array,
            |args| {
                let array = args[0].as_array();
                let mut shuffled = array.to_vec();

                let mut rng = rand::thread_rng();

                // Fisher-Yates shuffle
                for i in (1..shuffled.len()).rev() {
                    let j = rng.gen_range(0..=i);
                    shuffled.swap(i, j);
                }

                Ok(DixValue::from_array(shuffled))
            },
            "Returns a randomly shuffled copy of the array".to_string(),
            validation_helpers::first_is_array,
        )));

        // Random.bytes(count) - Generate random bytes
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "bytes".to_string(),
            1,
            DixType::Array,
            |args| {
                let count = args[0].as_int();

                if count < 0 {
                    return Err("Count cannot be negative".to_string());
                }

                if count > 10000 {
                    return Err("Count cannot exceed 10000".to_string());
                }

                let mut rng = rand::thread_rng();
                let mut bytes = vec![0u8; count as usize];
                rng.fill(&mut bytes[..]);

                let result: Vec<DixValue> = bytes
                    .iter()
                    .map(|&b| DixValue::from_int(b as i32))
                    .collect();

                Ok(DixValue::from_array(result))
            },
            "Returns an array of random bytes".to_string(),
            validation_helpers::all_numeric,
        )));

        // Random.string(length, charset) - Generate random string
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "string".to_string(),
            2,
            DixType::String,
            |args| {
                let length = args[0].as_int();
                let charset = args[1].as_string();

                if length < 0 {
                    return Err("Length cannot be negative".to_string());
                }

                if length > 10000 {
                    return Err("Length cannot exceed 10000".to_string());
                }

                if charset.is_empty() {
                    return Err("Character set cannot be empty".to_string());
                }

                let mut rng = rand::thread_rng();
                let charset_chars: Vec<char> = charset.chars().collect();
                let mut result = String::with_capacity(length as usize);

                for _ in 0..length {
                    let idx = rng.gen_range(0..charset_chars.len());
                    result.push(charset_chars[idx]);
                }

                Ok(DixValue::from_string(result))
            },
            "Generates a random string of specified length using given character set".to_string(),
            |args| {
                validation_helpers::argument_has_type(0, DixType::Int, args)
                    && validation_helpers::argument_has_type(1, DixType::String, args)
            },
        )));

        // Random.alphanumeric(length) - Generate random alphanumeric string
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "alphanumeric".to_string(),
            1,
            DixType::String,
            |args| {
                let length = args[0].as_int();

                if length < 0 {
                    return Err("Length cannot be negative".to_string());
                }

                if length > 10000 {
                    return Err("Length cannot exceed 10000".to_string());
                }

                const CHARSET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
                let mut rng = rand::thread_rng();
                let charset_chars: Vec<char> = CHARSET.chars().collect();
                let mut result = String::with_capacity(length as usize);

                for _ in 0..length {
                    let idx = rng.gen_range(0..charset_chars.len());
                    result.push(charset_chars[idx]);
                }

                Ok(DixValue::from_string(result))
            },
            "Generates a random alphanumeric string of specified length".to_string(),
            validation_helpers::all_numeric,
        )));

        // Random.weighted(values, weights) - Weighted random choice
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "weighted".to_string(),
            2,
            DixType::String,
            |args| {
                let values = args[0].as_array();
                let weights = args[1].as_array();

                if values.is_empty() {
                    return Err("Values array cannot be empty".to_string());
                }

                if values.len() != weights.len() {
                    return Err("Values and weights arrays must have the same length".to_string());
                }

                // Convert weights to doubles and validate
                let mut weight_values = Vec::with_capacity(weights.len());
                let mut total_weight = 0.0;

                for weight in weights {
                    if !weight.is_numeric() {
                        return Err("All weights must be numeric".to_string());
                    }

                    let w = weight.as_double();
                    if w < 0.0 {
                        return Err("Weights cannot be negative".to_string());
                    }

                    weight_values.push(w);
                    total_weight += w;
                }

                if total_weight == 0.0 {
                    return Err("Total weight cannot be zero".to_string());
                }

                let mut rng = rand::thread_rng();
                let random_value: f64 = rng.gen_range(0.0..total_weight);
                let mut cumulative_weight = 0.0;

                for (i, weight) in weight_values.iter().enumerate() {
                    cumulative_weight += weight;
                    if random_value <= cumulative_weight {
                        return Ok(values[i].clone());
                    }
                }

                // Fallback (should not reach here with proper weights)
                Ok(values[values.len() - 1].clone())
            },
            "Returns a weighted random choice from values based on weights".to_string(),
            |args| {
                validation_helpers::argument_has_type(0, DixType::Array, args)
                    && validation_helpers::argument_has_type(1, DixType::Array, args)
            },
        )));
    }
}

impl Default for RandomObject {
    fn default() -> Self {
        Self::new()
    }
}

impl IStaticObject for RandomObject {
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
    fn test_random_range() {
        let random_obj = RandomObject::new();
        let result = random_obj
            .call_method("range", &[DixValue::from_int(1), DixValue::from_int(10)])
            .unwrap();

        let value = result.as_int();
        assert!(value >= 1 && value <= 10);
    }

    #[test]
    fn test_random_boolean() {
        let random_obj = RandomObject::new();
        let result = random_obj.call_method("boolean", &[]).unwrap();
        assert!(result.get_type() == DixType::Bool);
    }
}