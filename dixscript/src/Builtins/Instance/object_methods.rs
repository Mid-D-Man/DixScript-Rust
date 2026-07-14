// dixscript/src/Builtins/Instance/object_methods.rs
//! Instance methods for the Object (HashMap) type in DixScript.

use crate::Builtins::Core::{
    DixType, DixValue, BuiltinMethod, IBuiltinMethod,
};
use std::collections::HashMap;

/// Get all instance methods for the Object type.
pub fn get_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // ── Mutation (returns new object) ──────────────────────────────────────

    // Object.add(key, value) → Object
    methods.insert(
        "add".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "add".to_string(),
            3,
            DixType::Object,
            |args| {
                let mut new_obj = args[0].as_object().clone();
                let key   = args[1].as_string();
                let value = args[2].deep_clone();
                new_obj.insert(key, value);
                Ok(DixValue::from_object(new_obj))
            },
            "Inserts a key-value pair and returns the resulting new object".to_string(),
            |args| {
                args[0].get_type() == DixType::Object
                    && args.len() >= 3
            },
        )),
    );

    // Object.set(key, value) → Object  (alias for add)
    methods.insert(
        "set".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "set".to_string(),
            3,
            DixType::Object,
            |args| {
                let mut new_obj = args[0].as_object().clone();
                let key   = args[1].as_string();
                let value = args[2].deep_clone();
                new_obj.insert(key, value);
                Ok(DixValue::from_object(new_obj))
            },
            "Sets a key-value pair and returns the resulting new object".to_string(),
            |args| {
                args[0].get_type() == DixType::Object
                    && args.len() >= 3
            },
        )),
    );

    // Object.remove(key) → Object
    methods.insert(
        "remove".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "remove".to_string(),
            2,
            DixType::Object,
            |args| {
                let mut new_obj = args[0].as_object().clone();
                let key = args[1].as_string();
                new_obj.remove(&key);
                Ok(DixValue::from_object(new_obj))
            },
            "Removes a key and returns the resulting new object".to_string(),
            |args| args[0].get_type() == DixType::Object && args.len() >= 2,
        )),
    );

    // Object.merge(other) → Object  (other's keys override self on clash)
    methods.insert(
        "merge".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "merge".to_string(),
            2,
            DixType::Object,
            |args| {
                let mut new_obj = args[0].as_object().clone();
                for (k, v) in args[1].as_object() {
                    new_obj.insert(k.clone(), v.deep_clone());
                }
                Ok(DixValue::from_object(new_obj))
            },
            "Merges another object into this one; returns the merged object".to_string(),
            |args| {
                args[0].get_type() == DixType::Object
                    && args.len() >= 2
                    && args[1].get_type() == DixType::Object
            },
        )),
    );

    // ── Read-only queries ──────────────────────────────────────────────────

    // Object.get(key) → Any | null
    methods.insert(
        "get".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "get".to_string(),
            2,
            DixType::Any,
            |args| {
                let key    = args[1].as_string();
                let result = args[0].as_object().get(&key).cloned()
                    .unwrap_or_else(DixValue::null);
                Ok(result)
            },
            "Returns the value for the given key, or null if the key is absent".to_string(),
            |args| args[0].get_type() == DixType::Object && args.len() >= 2,
        )),
    );

    // Object.has(key) → bool
    methods.insert(
        "has".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "has".to_string(),
            2,
            DixType::Bool,
            |args| {
                let key = args[1].as_string();
                Ok(DixValue::from_bool(
                    args[0].as_object().contains_key(&key),
                ))
            },
            "Returns true if the object contains the specified key".to_string(),
            |args| args[0].get_type() == DixType::Object && args.len() >= 2,
        )),
    );

    // Object.count() → Int
    methods.insert(
        "count".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "count".to_string(),
            1,
            DixType::Int,
            |args| {
                Ok(DixValue::from_int(args[0].as_object().len() as i32))
            },
            "Returns the number of key-value pairs in the object".to_string(),
            |args| args[0].get_type() == DixType::Object,
        )),
    );

    // ── Conversion / iteration ─────────────────────────────────────────────

    // Object.keys() → Array<String>
    methods.insert(
        "keys".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "keys".to_string(),
            1,
            DixType::Array,
            |args| {
                let keys: Vec<DixValue> = args[0].as_object()
                    .keys()
                    .map(|k| DixValue::from_string(k.clone()))
                    .collect();
                Ok(DixValue::from_array(keys))
            },
            "Returns an array containing all keys of the object".to_string(),
            |args| args[0].get_type() == DixType::Object,
        )),
    );

    // Object.values() → Array
    methods.insert(
        "values".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "values".to_string(),
            1,
            DixType::Array,
            |args| {
                let vals: Vec<DixValue> = args[0].as_object()
                    .values()
                    .map(|v| v.deep_clone())
                    .collect();
                Ok(DixValue::from_array(vals))
            },
            "Returns an array containing all values of the object".to_string(),
            |args| args[0].get_type() == DixType::Object,
        )),
    );

    // Object.entries() → Array of t:(key, value) tuples
    methods.insert(
        "entries".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "entries".to_string(),
            1,
            DixType::Array,
            |args| {
                let pairs: Vec<DixValue> = args[0].as_object()
                    .iter()
                    .map(|(k, v)| {
                        DixValue::from_tuple(vec![
                            DixValue::from_string(k.clone()),
                            v.deep_clone(),
                        ])
                    })
                    .collect();
                Ok(DixValue::from_array(pairs))
            },
            "Returns an array of [key, value] tuple pairs".to_string(),
            |args| args[0].get_type() == DixType::Object,
        )),
    );

    // Object.toArray() → alias for entries()
    methods.insert(
        "toArray".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "toArray".to_string(),
            1,
            DixType::Array,
            |args| {
                let pairs: Vec<DixValue> = args[0].as_object()
                    .iter()
                    .map(|(k, v)| {
                        DixValue::from_tuple(vec![
                            DixValue::from_string(k.clone()),
                            v.deep_clone(),
                        ])
                    })
                    .collect();
                Ok(DixValue::from_array(pairs))
            },
            "Returns an array of [key, value] tuple pairs (alias for entries)".to_string(),
            |args| args[0].get_type() == DixType::Object,
        )),
    );

    // Object.containsValue(value) → bool
    methods.insert(
        "containsValue".to_string(),
        Box::new(BuiltinMethod::new_with_validator(
            "containsValue".to_string(),
            2,
            DixType::Bool,
            |args| {
                let target = &args[1];
                let found  = args[0].as_object().values().any(|v| v.equal_to(target));
                Ok(DixValue::from_bool(found))
            },
            "Returns true if the object contains the specified value".to_string(),
            |args| args[0].get_type() == DixType::Object && args.len() >= 2,
        )),
    );

    methods
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_count() {
        let methods = get_methods();
        let empty   = DixValue::from_object(HashMap::new());
        let added   = methods["add"].call(&[
            empty,
            DixValue::from_string("x".to_string()),
            DixValue::from_int(42),
        ]).unwrap();
        assert_eq!(added.get_type(), DixType::Object);
        let count = methods["count"].call(&[added]).unwrap();
        assert_eq!(count.as_int(), 1);
    }

    #[test]
    fn test_has_and_get() {
        let methods = get_methods();
        let mut map = HashMap::new();
        map.insert("name".to_string(), DixValue::from_string("Alice".to_string()));
        let obj = DixValue::from_object(map);

        let has_name  = methods["has"].call(&[obj.clone(), DixValue::from_string("name".to_string())]).unwrap();
        assert!(has_name.as_bool());

        let has_other = methods["has"].call(&[obj.clone(), DixValue::from_string("age".to_string())]).unwrap();
        assert!(!has_other.as_bool());

        let name_val  = methods["get"].call(&[obj.clone(), DixValue::from_string("name".to_string())]).unwrap();
        assert_eq!(name_val.as_string(), "Alice");

        let null_val  = methods["get"].call(&[obj.clone(), DixValue::from_string("missing".to_string())]).unwrap();
        assert!(null_val.is_null());
    }

    #[test]
    fn test_remove() {
        let methods = get_methods();
        let mut map = HashMap::new();
        map.insert("a".to_string(), DixValue::from_int(1));
        map.insert("b".to_string(), DixValue::from_int(2));
        let obj     = DixValue::from_object(map);
        let removed = methods["remove"].call(&[obj, DixValue::from_string("a".to_string())]).unwrap();
        let count   = methods["count"].call(&[removed.clone()]).unwrap();
        assert_eq!(count.as_int(), 1);
        let has_a   = methods["has"].call(&[removed, DixValue::from_string("a".to_string())]).unwrap();
        assert!(!has_a.as_bool());
    }

    #[test]
    fn test_merge() {
        let methods = get_methods();
        let mut m1 = HashMap::new();
        m1.insert("a".to_string(), DixValue::from_int(1));
        let mut m2 = HashMap::new();
        m2.insert("b".to_string(), DixValue::from_int(2));
        m2.insert("a".to_string(), DixValue::from_int(99)); // overrides
        let merged = methods["merge"].call(&[
            DixValue::from_object(m1),
            DixValue::from_object(m2),
        ]).unwrap();
        let count = methods["count"].call(&[merged.clone()]).unwrap();
        assert_eq!(count.as_int(), 2);
        let a_val = methods["get"].call(&[merged, DixValue::from_string("a".to_string())]).unwrap();
        assert_eq!(a_val.as_int(), 99); // other overrides self
    }

    #[test]
    fn test_keys_and_values() {
        let methods = get_methods();
        let mut map = HashMap::new();
        map.insert("x".to_string(), DixValue::from_int(10));
        map.insert("y".to_string(), DixValue::from_int(20));
        let obj    = DixValue::from_object(map);
        let keys   = methods["keys"].call(&[obj.clone()]).unwrap();
        assert_eq!(keys.as_array().len(), 2);
        let values = methods["values"].call(&[obj]).unwrap();
        assert_eq!(values.as_array().len(), 2);
    }

    #[test]
    fn test_entries() {
        let methods = get_methods();
        let mut map = HashMap::new();
        map.insert("k".to_string(), DixValue::from_int(7));
        let obj     = DixValue::from_object(map);
        let entries = methods["entries"].call(&[obj]).unwrap();
        let arr     = entries.as_array();
        assert_eq!(arr.len(), 1);
        // Each entry is a 2-tuple
        let tuple = arr[0].as_array();
        assert_eq!(tuple[0].as_string(), "k");
        assert_eq!(tuple[1].as_int(), 7);
    }
}
