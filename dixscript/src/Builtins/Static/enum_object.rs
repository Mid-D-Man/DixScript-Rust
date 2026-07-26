// src/Builtins/Static/enum_object.rs
//! Enum static object implementation for DixScript
//! Provides enum manipulation and utility functions

use crate::Builtins::Core::{BuiltinMethod, DixType, DixValue, IBuiltinMethod};
use crate::Builtins::Static::{IStaticObject, StaticObjectBase};
use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

/// Global registry for DixScript enums defined in @ENUMS section
static DIXSCRIPT_ENUMS: RwLock<Option<HashMap<String, HashMap<String, i32>>>> = RwLock::new(None);

/// Serializes the "register this compile's enums, then later consult them"
/// window that spans `GeneralSemanticAnalyzer::analyze()` (which populates
/// `DIXSCRIPT_ENUMS` via `register_enums_with_builtin_system`) through
/// `ValueResolver::resolve()` (which is what actually calls `Enum.*`
/// builtin methods that read it back) — see `Runtime/loader.rs::compile_source`,
/// which is the only place this gets locked.
///
/// `DIXSCRIPT_ENUMS` itself is process-wide, not per-compile — `clear_enums()`
/// at the start of every top-level (non-nested-import) `analyze()` call is
/// what keeps a *previous, already-finished* compile's enums from leaking
/// into a new one, but it does nothing to stop *two compiles running at the
/// same time on different threads* from interleaving: thread A registers,
/// thread B's own registration clears the registry out from under thread A
/// before thread A's Stage 7 gets to consult it, and thread A sees "Enum
/// 'X' not found" for an enum that is very much declared, just not there
/// anymore by the time it's needed. This lock closes that window — it's a
/// pure coordination primitive (guards `()`, not the data itself, which is
/// still the `RwLock` above), so a panic while holding it doesn't leave
/// anything in a torn state worth failing subsequent compiles over; callers
/// recover from poisoning rather than propagating it.
pub static ENUM_REGISTRY_LOCK: Mutex<()> = Mutex::new(());

/// Enum static object implementation
pub struct EnumObject {
    base: StaticObjectBase,
}

impl EnumObject {
    pub fn new() -> Self {
        let mut base = StaticObjectBase::new("Enum".to_string());
        Self::initialize_methods(&mut base);
        EnumObject { base }
    }

    fn initialize_methods(base: &mut StaticObjectBase) {
        // Enum.getValues(enumName) - Get all values of a DixScript enum
        base.register_method(Box::new(BuiltinMethod::new(
            "getValues".to_string(),
            1,
            DixType::Array,
            |args| {
                let enum_name = args[0].as_string();

                if enum_name.is_empty() {
                    return Err("Enum name cannot be empty".to_string());
                }

                let registry = DIXSCRIPT_ENUMS.read().unwrap();
                let enums = registry.as_ref().ok_or("Enum registry not initialized")?;

                let enum_values = enums
                    .get(&enum_name)
                    .ok_or_else(|| format!("Enum '{}' not found", enum_name))?;

                let values: Vec<DixValue> = enum_values
                    .keys()
                    .map(|k| DixValue::from_string(k.clone()))
                    .collect();

                Ok(DixValue::from_array(values))
            },
            "Returns all value names of a DixScript enum".to_string(),
        )));

        // Enum.getName(enumName, value) - Get name of enum value
        base.register_method(Box::new(BuiltinMethod::new(
            "getName".to_string(),
            2,
            DixType::String,
            |args| {
                let enum_name = args[0].as_string();
                let value = args[1].as_int();

                if enum_name.is_empty() {
                    return Err("Enum name cannot be empty".to_string());
                }

                let registry = DIXSCRIPT_ENUMS.read().unwrap();
                let enums = registry.as_ref().ok_or("Enum registry not initialized")?;

                let enum_values = enums
                    .get(&enum_name)
                    .ok_or_else(|| format!("Enum '{}' not found", enum_name))?;

                for (name, val) in enum_values {
                    if *val == value {
                        return Ok(DixValue::from_string(name.clone()));
                    }
                }

                Err(format!("Value {} not found in enum '{}'", value, enum_name))
            },
            "Returns the name of an enum value by its numeric value".to_string(),
        )));

        // Enum.getValue(enumName, name) - Get numeric value of enum name
        base.register_method(Box::new(BuiltinMethod::new(
            "getValue".to_string(),
            2,
            DixType::Int,
            |args| {
                let enum_name = args[0].as_string();
                let name = args[1].as_string();

                if enum_name.is_empty() {
                    return Err("Enum name cannot be empty".to_string());
                }

                if name.is_empty() {
                    return Err("Enum value name cannot be empty".to_string());
                }

                let registry = DIXSCRIPT_ENUMS.read().unwrap();
                let enums = registry.as_ref().ok_or("Enum registry not initialized")?;

                let enum_values = enums
                    .get(&enum_name)
                    .ok_or_else(|| format!("Enum '{}' not found", enum_name))?;

                let value = enum_values
                    .get(&name)
                    .ok_or_else(|| format!("Value '{}' not found in enum '{}'", name, enum_name))?;

                Ok(DixValue::from_int(*value))
            },
            "Returns the numeric value of an enum name".to_string(),
        )));

        // Enum.hasValue(enumName, name) - Check if enum has value
        base.register_method(Box::new(BuiltinMethod::new(
            "hasValue".to_string(),
            2,
            DixType::Bool,
            |args| {
                let enum_name = args[0].as_string();
                let name = args[1].as_string();

                if enum_name.is_empty() || name.is_empty() {
                    return Ok(DixValue::from_bool(false));
                }

                let registry = DIXSCRIPT_ENUMS.read().unwrap();
                if let Some(enums) = registry.as_ref() {
                    if let Some(enum_values) = enums.get(&enum_name) {
                        return Ok(DixValue::from_bool(enum_values.contains_key(&name)));
                    }
                }

                Ok(DixValue::from_bool(false))
            },
            "Checks if an enum contains a specific value name".to_string(),
        )));

        // Enum.count(enumName) - Get count of enum values
        base.register_method(Box::new(BuiltinMethod::new(
            "count".to_string(),
            1,
            DixType::Int,
            |args| {
                let enum_name = args[0].as_string();

                if enum_name.is_empty() {
                    return Err("Enum name cannot be empty".to_string());
                }

                let registry = DIXSCRIPT_ENUMS.read().unwrap();
                let enums = registry.as_ref().ok_or("Enum registry not initialized")?;

                let enum_values = enums
                    .get(&enum_name)
                    .ok_or_else(|| format!("Enum '{}' not found", enum_name))?;

                Ok(DixValue::from_int(enum_values.len() as i32))
            },
            "Returns the number of values in an enum".to_string(),
        )));

        // Enum.exists(enumName) - Check if enum exists
        base.register_method(Box::new(BuiltinMethod::new(
            "exists".to_string(),
            1,
            DixType::Bool,
            |args| {
                let enum_name = args[0].as_string();

                if enum_name.is_empty() {
                    return Ok(DixValue::from_bool(false));
                }

                let registry = DIXSCRIPT_ENUMS.read().unwrap();
                if let Some(enums) = registry.as_ref() {
                    return Ok(DixValue::from_bool(enums.contains_key(&enum_name)));
                }

                Ok(DixValue::from_bool(false))
            },
            "Checks if an enum with the given name exists".to_string(),
        )));

        // Enum.list() - List all registered enum names
        base.register_method(Box::new(BuiltinMethod::new(
            "list".to_string(),
            0,
            DixType::Array,
            |_args| {
                let registry = DIXSCRIPT_ENUMS.read().unwrap();
                if let Some(enums) = registry.as_ref() {
                    let enum_names: Vec<DixValue> = enums
                        .keys()
                        .map(|k| DixValue::from_string(k.clone()))
                        .collect();
                    Ok(DixValue::from_array(enum_names))
                } else {
                    Ok(DixValue::from_array(Vec::new()))
                }
            },
            "Returns an array of all registered enum names".to_string(),
        )));

        // Enum.min(enumName) - Get minimum value in enum
        base.register_method(Box::new(BuiltinMethod::new(
            "min".to_string(),
            1,
            DixType::Int,
            |args| {
                let enum_name = args[0].as_string();

                if enum_name.is_empty() {
                    return Err("Enum name cannot be empty".to_string());
                }

                let registry = DIXSCRIPT_ENUMS.read().unwrap();
                let enums = registry.as_ref().ok_or("Enum registry not initialized")?;

                let enum_values = enums
                    .get(&enum_name)
                    .ok_or_else(|| format!("Enum '{}' not found", enum_name))?;

                if enum_values.is_empty() {
                    return Err(format!("Enum '{}' has no values", enum_name));
                }

                let min = enum_values.values().min().unwrap();
                Ok(DixValue::from_int(*min))
            },
            "Returns the minimum numeric value in an enum".to_string(),
        )));

        // Enum.max(enumName) - Get maximum value in enum
        base.register_method(Box::new(BuiltinMethod::new(
            "max".to_string(),
            1,
            DixType::Int,
            |args| {
                let enum_name = args[0].as_string();

                if enum_name.is_empty() {
                    return Err("Enum name cannot be empty".to_string());
                }

                let registry = DIXSCRIPT_ENUMS.read().unwrap();
                let enums = registry.as_ref().ok_or("Enum registry not initialized")?;

                let enum_values = enums
                    .get(&enum_name)
                    .ok_or_else(|| format!("Enum '{}' not found", enum_name))?;

                if enum_values.is_empty() {
                    return Err(format!("Enum '{}' has no values", enum_name));
                }

                let max = enum_values.values().max().unwrap();
                Ok(DixValue::from_int(*max))
            },
            "Returns the maximum numeric value in an enum".to_string(),
        )));

        // Enum.random(enumName) - Get random enum value name
        base.register_method(Box::new(BuiltinMethod::new(
            "random".to_string(),
            1,
            DixType::String,
            |args| {
                let enum_name = args[0].as_string();

                if enum_name.is_empty() {
                    return Err("Enum name cannot be empty".to_string());
                }

                let registry = DIXSCRIPT_ENUMS.read().unwrap();
                let enums = registry.as_ref().ok_or("Enum registry not initialized")?;

                let enum_values = enums
                    .get(&enum_name)
                    .ok_or_else(|| format!("Enum '{}' not found", enum_name))?;

                if enum_values.is_empty() {
                    return Err(format!("Enum '{}' has no values", enum_name));
                }

                let keys: Vec<&String> = enum_values.keys().collect();
                let random_index = rand::random::<usize>() % keys.len();
                let random_key = keys[random_index];

                Ok(DixValue::from_string(random_key.clone()))
            },
            "Returns a random value name from an enum".to_string(),
        )));

        // Enum.contains(enumName, value) - Check if enum contains numeric value
        base.register_method(Box::new(BuiltinMethod::new(
            "contains".to_string(),
            2,
            DixType::Bool,
            |args| {
                let enum_name = args[0].as_string();
                let value = args[1].as_int();

                if enum_name.is_empty() {
                    return Ok(DixValue::from_bool(false));
                }

                let registry = DIXSCRIPT_ENUMS.read().unwrap();
                if let Some(enums) = registry.as_ref() {
                    if let Some(enum_values) = enums.get(&enum_name) {
                        return Ok(DixValue::from_bool(
                            enum_values.values().any(|v| *v == value),
                        ));
                    }
                }

                Ok(DixValue::from_bool(false))
            },
            "Checks if an enum contains a specific numeric value".to_string(),
        )));

        // Enum.toArray(enumName) - Convert enum to array of objects
        base.register_method(Box::new(BuiltinMethod::new(
            "toArray".to_string(),
            1,
            DixType::Array,
            |args| {
                let enum_name = args[0].as_string();

                if enum_name.is_empty() {
                    return Err("Enum name cannot be empty".to_string());
                }

                let registry = DIXSCRIPT_ENUMS.read().unwrap();
                let enums = registry.as_ref().ok_or("Enum registry not initialized")?;

                let enum_values = enums
                    .get(&enum_name)
                    .ok_or_else(|| format!("Enum '{}' not found", enum_name))?;

                let mut pairs: Vec<(&String, &i32)> = enum_values.iter().collect();
                pairs.sort_by_key(|(_, v)| *v);

                let result: Vec<DixValue> = pairs
                    .iter()
                    .map(|(name, value)| {
                        let mut obj = HashMap::new();
                        obj.insert("name".to_string(), DixValue::from_string((*name).clone()));
                        obj.insert("value".to_string(), DixValue::from_int(**value));
                        DixValue::from_object(obj)
                    })
                    .collect();

                Ok(DixValue::from_array(result))
            },
            "Converts an enum to an array of name-value objects".to_string(),
        )));
    }
}

impl Default for EnumObject {
    fn default() -> Self {
        Self::new()
    }
}

impl IStaticObject for EnumObject {
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

// ==================== STATIC METHODS FOR ENUM MANAGEMENT ====================

/// Initialize the enum registry
fn ensure_registry_initialized() {
    let mut registry = DIXSCRIPT_ENUMS.write().unwrap();
    if registry.is_none() {
        *registry = Some(HashMap::new());
    }
}

/// Register a DixScript enum from the @ENUMS section
pub fn register_enum(enum_name: String, values: HashMap<String, i32>) {
    ensure_registry_initialized();
    let mut registry = DIXSCRIPT_ENUMS.write().unwrap();
    if let Some(enums) = registry.as_mut() {
        enums.insert(enum_name, values);
    }
}

/// Unregister a DixScript enum
pub fn unregister_enum(enum_name: &str) -> bool {
    let mut registry = DIXSCRIPT_ENUMS.write().unwrap();
    if let Some(enums) = registry.as_mut() {
        enums.remove(enum_name).is_some()
    } else {
        false
    }
}

/// Clear all registered enums
pub fn clear_enums() {
    let mut registry = DIXSCRIPT_ENUMS.write().unwrap();
    if let Some(enums) = registry.as_mut() {
        enums.clear();
    }
}

/// Get all registered enum names
pub fn get_registered_enums() -> Vec<String> {
    let registry = DIXSCRIPT_ENUMS.read().unwrap();
    if let Some(enums) = registry.as_ref() {
        enums.keys().cloned().collect()
    } else {
        Vec::new()
    }
}

/// Check if an enum is registered
pub fn is_enum_registered(enum_name: &str) -> bool {
    let registry = DIXSCRIPT_ENUMS.read().unwrap();
    if let Some(enums) = registry.as_ref() {
        enums.contains_key(enum_name)
    } else {
        false
    }
}

/// Get enum values for a registered enum
pub fn get_enum_values(enum_name: &str) -> Option<HashMap<String, i32>> {
    let registry = DIXSCRIPT_ENUMS.read().unwrap();
    if let Some(enums) = registry.as_ref() {
        enums.get(enum_name).cloned()
    } else {
        None
    }
}

/// Validate enum access at compile time
pub fn validate_enum_access(enum_name: &str, value_name: &str) -> bool {
    if enum_name.is_empty() || value_name.is_empty() {
        return false;
    }

    let registry = DIXSCRIPT_ENUMS.read().unwrap();
    if let Some(enums) = registry.as_ref() {
        if let Some(values) = enums.get(enum_name) {
            return values.contains_key(value_name);
        }
    }
    false
}

/// Get enum value by name (for compiler use)
pub fn get_enum_value_by_name(enum_name: &str, value_name: &str) -> Option<i32> {
    if enum_name.is_empty() || value_name.is_empty() {
        return None;
    }

    let registry = DIXSCRIPT_ENUMS.read().unwrap();
    if let Some(enums) = registry.as_ref() {
        if let Some(values) = enums.get(enum_name) {
            return values.get(value_name).copied();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enum_registration() {
        let mut values = HashMap::new();
        values.insert("Red".to_string(), 0);
        values.insert("Green".to_string(), 1);
        values.insert("Blue".to_string(), 2);

        register_enum("Color".to_string(), values);

        assert!(is_enum_registered("Color"));
        assert_eq!(get_enum_value_by_name("Color", "Red"), Some(0));
        assert_eq!(get_enum_value_by_name("Color", "Blue"), Some(2));
    }

    #[test]
    fn test_enum_object_methods() {
        let mut values = HashMap::new();
        values.insert("Small".to_string(), 1);
        values.insert("Medium".to_string(), 2);
        values.insert("Large".to_string(), 3);

        register_enum("Size".to_string(), values);

        let enum_obj = EnumObject::new();

        // Test exists
        let result = enum_obj
            .call_method("exists", &[DixValue::from_string("Size".to_string())])
            .unwrap();
        assert!(result.as_bool());

        // Test count
        let result = enum_obj
            .call_method("count", &[DixValue::from_string("Size".to_string())])
            .unwrap();
        assert_eq!(result.as_int(), 3);
    }
                   }
