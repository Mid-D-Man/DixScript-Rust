// dixscript/src/Builtins/Static/random_object.rs
//! Random static object implementation for DixScript
//! Provides random number generation and selection functions

use crate::Builtins::Core::{BuiltinMethod, DixType, DixValue, IBuiltinMethod, validation_helpers};
use crate::Builtins::Static::{IStaticObject, StaticObjectBase};
use rand::Rng;

/// Random static object implementation.
/// ThreadRng is created on-demand per method — no stored state — so this is
/// both Send + Sync without any locking overhead.
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
        // Random.range(min, max) — random i32 in [min, max] inclusive
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
                Ok(DixValue::from_int(rand::thread_rng().gen_range(min..=max)))
            },
            "Returns a random integer between min and max (inclusive)".to_string(),
            validation_helpers::all_numeric,
        )));

        // Random.longRange(min, max) — random i64 in [min, max] inclusive
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "longRange".to_string(),
            2,
            DixType::Long,
            |args| {
                let min = args[0].as_long();
                let max = args[1].as_long();
                if min > max {
                    return Err("Min value cannot be greater than max value".to_string());
                }
                Ok(DixValue::from_long(rand::thread_rng().gen_range(min..=max)))
            },
            "Returns a random long (i64) between min and max (inclusive)".to_string(),
            validation_helpers::all_numeric,
        )));

        // Random.nextFloat() — random f32 in [0.0, 1.0)
        // Renamed from float() to avoid clash with DataType::Float keyword
        base.register_method(Box::new(BuiltinMethod::new(
            "nextFloat".to_string(),
            0,
            DixType::Float,
            |_| Ok(DixValue::from_float(rand::thread_rng().gen::<f32>())),
            "Returns a random float between 0.0 and 1.0".to_string(),
        )));

        // Random.nextDouble() — random f64 in [0.0, 1.0)
        // Renamed from double() to avoid clash with DataType::Double keyword
        base.register_method(Box::new(BuiltinMethod::new(
            "nextDouble".to_string(),
            0,
            DixType::Double,
            |_| Ok(DixValue::from_double(rand::thread_rng().gen::<f64>())),
            "Returns a random double between 0.0 and 1.0".to_string(),
        )));

        // Random.nextBool()
        // Renamed from boolean() for consistency with nextFloat/nextDouble naming
        base.register_method(Box::new(BuiltinMethod::new(
            "nextBool".to_string(),
            0,
            DixType::Bool,
            |_| Ok(DixValue::from_bool(rand::thread_rng().gen::<bool>())),
            "Returns a random boolean value".to_string(),
        )));

        // Random.floatRange(min, max)
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
                Ok(DixValue::from_float(rand::thread_rng().gen_range(min..=max)))
            },
            "Returns a random float between min and max".to_string(),
            validation_helpers::all_numeric,
        )));

        // Random.doubleRange(min, max)
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
                Ok(DixValue::from_double(rand::thread_rng().gen_range(min..=max)))
            },
            "Returns a random double between min and max".to_string(),
            validation_helpers::all_numeric,
        )));

        // Random.choice(array)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "choice".to_string(),
            1,
            DixType::Any,
            |args| {
                let array = args[0].as_array();
                if array.is_empty() {
                    return Err("Cannot choose from empty array".to_string());
                }
                let index = rand::thread_rng().gen_range(0..array.len());
                Ok(array[index].clone())
            },
            "Returns a random element from an array".to_string(),
            validation_helpers::first_is_array,
        )));

        // Random.choices(array, count) — with replacement
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
                if count > 10_000 {
                    return Err("Count cannot exceed 10000".to_string());
                }
                let mut rng    = rand::thread_rng();
                let mut result = Vec::with_capacity(count as usize);
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

        // Random.sample(array, count) — without replacement
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
                if count as usize > array.len() {
                    return Err(format!(
                        "Cannot sample {} items from array of {} elements",
                        count, array.len()
                    ));
                }
                let mut rng     = rand::thread_rng();
                let mut indices: Vec<usize> = (0..array.len()).collect();
                let mut result  = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    let ri         = rng.gen_range(0..indices.len());
                    let array_idx  = indices.remove(ri);
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

        // Random.shuffle(array)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "shuffle".to_string(),
            1,
            DixType::Array,
            |args| {
                let mut shuffled = args[0].as_array().to_vec();
                let mut rng = rand::thread_rng();
                // Fisher-Yates
                for i in (1..shuffled.len()).rev() {
                    let j = rng.gen_range(0..=i);
                    shuffled.swap(i, j);
                }
                Ok(DixValue::from_array(shuffled))
            },
            "Returns a randomly shuffled copy of the array".to_string(),
            validation_helpers::first_is_array,
        )));

        // Random.bytes(count)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "bytes".to_string(),
            1,
            DixType::Array,
            |args| {
                let count = args[0].as_int();
                if count < 0 {
                    return Err("Count cannot be negative".to_string());
                }
                if count > 10_000 {
                    return Err("Count cannot exceed 10000".to_string());
                }
                let mut rng   = rand::thread_rng();
                let mut bytes = vec![0u8; count as usize];
                rng.fill(&mut bytes[..]);
                let result: Vec<DixValue> = bytes.iter()
                    .map(|&b| DixValue::from_int(b as i32))
                    .collect();
                Ok(DixValue::from_array(result))
            },
            "Returns an array of random bytes".to_string(),
            validation_helpers::all_numeric,
        )));

        // Random.randomString(length, charset)
        // Renamed from string() to avoid clash with DataType::String keyword
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "randomString".to_string(),
            2,
            DixType::String,
            |args| {
                let length  = args[0].as_int();
                let charset = args[1].as_string();
                if length < 0 {
                    return Err("Length cannot be negative".to_string());
                }
                if length > 10_000 {
                    return Err("Length cannot exceed 10000".to_string());
                }
                if charset.is_empty() {
                    return Err("Character set cannot be empty".to_string());
                }
                let mut rng            = rand::thread_rng();
                let charset_chars: Vec<char> = charset.chars().collect();
                let mut result         = String::with_capacity(length as usize);
                for _ in 0..length {
                    result.push(charset_chars[rng.gen_range(0..charset_chars.len())]);
                }
                Ok(DixValue::from_string(result))
            },
            "Generates a random string of specified length using given character set".to_string(),
            |args| {
                validation_helpers::argument_has_type(0, DixType::Int, args)
                    && validation_helpers::argument_has_type(1, DixType::String, args)
            },
        )));

        // Random.alphanumeric(length)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "alphanumeric".to_string(),
            1,
            DixType::String,
            |args| {
                let length = args[0].as_int();
                if length < 0 {
                    return Err("Length cannot be negative".to_string());
                }
                if length > 10_000 {
                    return Err("Length cannot exceed 10000".to_string());
                }
                const CHARSET: &[u8] =
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
                let mut rng    = rand::thread_rng();
                let mut result = String::with_capacity(length as usize);
                for _ in 0..length {
                    result.push(CHARSET[rng.gen_range(0..CHARSET.len())] as char);
                }
                Ok(DixValue::from_string(result))
            },
            "Generates a random alphanumeric string of specified length".to_string(),
            validation_helpers::all_numeric,
        )));

        // Random.weighted(values, weights)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "weighted".to_string(),
            2,
            DixType::Any,
            |args| {
                let values  = args[0].as_array();
                let weights = args[1].as_array();
                if values.is_empty() {
                    return Err("Values array cannot be empty".to_string());
                }
                if values.len() != weights.len() {
                    return Err("Values and weights arrays must have the same length".to_string());
                }
                let mut weight_values = Vec::with_capacity(weights.len());
                let mut total_weight  = 0.0_f64;
                for w in weights {
                    if !w.is_numeric() {
                        return Err("All weights must be numeric".to_string());
                    }
                    let wf = w.as_double();
                    if wf < 0.0 {
                        return Err("Weights cannot be negative".to_string());
                    }
                    weight_values.push(wf);
                    total_weight += wf;
                }
                if total_weight == 0.0 {
                    return Err("Total weight cannot be zero".to_string());
                }
                let mut rng            = rand::thread_rng();
                let random_val: f64    = rng.gen_range(0.0..total_weight);
                let mut cumulative     = 0.0_f64;
                for (i, w) in weight_values.iter().enumerate() {
                    cumulative += w;
                    if random_val <= cumulative {
                        return Ok(values[i].clone());
                    }
                }
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
    fn default() -> Self { Self::new() }
}

impl IStaticObject for RandomObject {
    fn name(&self) -> &str { self.base.name() }

    fn call_method(&self, method_name: &str, args: &[DixValue]) -> Result<DixValue, String> {
        self.base.call_method(method_name, args)
    }

    fn has_method(&self, method_name: &str) -> bool { self.base.has_method(method_name) }

    fn get_method_names(&self) -> Vec<String> { self.base.get_method_names() }

    fn get_method(&self, method_name: &str) -> Option<&dyn IBuiltinMethod> {
        self.base.get_method(method_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_range_within_bounds() {
        let obj = RandomObject::new();
        let result = obj.call_method("range", &[
            DixValue::from_int(1),
            DixValue::from_int(10),
        ]).unwrap();
        let v = result.as_int();
        assert!(v >= 1 && v <= 10);
    }

    #[test]
    fn test_random_long_range_within_bounds() {
        let obj = RandomObject::new();
        let min = 1_000_000_000_i64;
        let max = 9_000_000_000_i64;
        let result = obj.call_method("longRange", &[
            DixValue::from_long(min),
            DixValue::from_long(max),
        ]).unwrap();
        assert_eq!(result.get_type(), DixType::Long);
        let v = result.as_long();
        assert!(v >= min && v <= max);
    }

    #[test]
    fn test_random_long_range_min_greater_than_max_fails() {
        let obj = RandomObject::new();
        let result = obj.call_method("longRange", &[
            DixValue::from_long(100_i64),
            DixValue::from_long(1_i64),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_random_long_range_with_zero() {
        let obj = RandomObject::new();
        let result = obj.call_method("longRange", &[
            DixValue::from_long(0_i64),
            DixValue::from_long(0_i64),
        ]).unwrap();
        assert_eq!(result.as_long(), 0_i64);
    }

    #[test]
    fn test_random_next_float_in_range() {
        let obj = RandomObject::new();
        let result = obj.call_method("nextFloat", &[]).unwrap();
        assert_eq!(result.get_type(), DixType::Float);
        let v = result.as_float();
        assert!(v >= 0.0 && v < 1.0);
    }

    #[test]
    fn test_random_next_double_in_range() {
        let obj = RandomObject::new();
        let result = obj.call_method("nextDouble", &[]).unwrap();
        assert_eq!(result.get_type(), DixType::Double);
        let v = result.as_double();
        assert!(v >= 0.0 && v < 1.0);
    }

    #[test]
    fn test_random_next_bool() {
        let obj = RandomObject::new();
        let result = obj.call_method("nextBool", &[]).unwrap();
        assert_eq!(result.get_type(), DixType::Bool);
    }

    #[test]
    fn test_random_shuffle_length_preserved() {
        let obj    = RandomObject::new();
        let input  = DixValue::from_array(vec![
            DixValue::from_int(1),
            DixValue::from_int(2),
            DixValue::from_int(3),
        ]);
        let result = obj.call_method("shuffle", &[input]).unwrap();
        assert_eq!(result.as_array().len(), 3);
    }

    #[test]
    fn test_random_alphanumeric_length() {
        let obj    = RandomObject::new();
        let result = obj.call_method("alphanumeric", &[DixValue::from_int(20)]).unwrap();
        assert_eq!(result.as_string().len(), 20);
    }

    #[test]
    fn test_random_string_with_charset() {
        let obj = RandomObject::new();
        let result = obj.call_method("randomString", &[
            DixValue::from_int(10),
            DixValue::from_string("abc".to_string()),
        ]).unwrap();
        let s = result.as_string();
        assert_eq!(s.len(), 10);
        assert!(s.chars().all(|c| "abc".contains(c)));
    }

    #[test]
    fn test_old_names_no_longer_exist() {
        let obj = RandomObject::new();
        // Verify old clashing names are gone
        assert!(!obj.has_method("float"));
        assert!(!obj.has_method("double"));
        assert!(!obj.has_method("boolean"));
        assert!(!obj.has_method("string"));
        // Verify new names exist
        assert!(obj.has_method("nextFloat"));
        assert!(obj.has_method("nextDouble"));
        assert!(obj.has_method("nextBool"));
        assert!(obj.has_method("randomString"));
    }
            }
