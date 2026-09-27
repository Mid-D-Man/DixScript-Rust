// src/Builtins/Static/schema_object.rs
//! Schema static object - builds @SCHEMA field descriptors
//!
//! One method per field type (Schema.string, Schema.int, Schema.bool, ...)
//! rather than a single generic Schema.field(type, ...) -- each method's
//! first argument is always `required`, followed by any type-specific
//! constraints (min/max for numeric types, minLength/maxLength for string,
//! minItems/maxItems for array, the enum name for Schema.enum).
//!
//! Every method returns a canonical descriptor: a plain Object with a
//! "type" key (matching the field's DataType, as a string) and a
//! "required" key (bool), plus whichever constraint keys were actually
//! supplied -- a constraint argument that wasn't passed is simply absent
//! from the returned object, not present with a null/sentinel value, so
//! `@SCHEMA`'s own validation pass can tell "no constraint" apart from
//! "constrained to this value" with a plain `.get(key)`.
//!
//! This is compile-time descriptor construction only: nothing here
//! inspects or validates real @DATA values against real data at runtime.
//! `@SCHEMA`'s semantic analyzer is what reads these descriptors back
//! (by key) and checks them against the resolved @DATA AST.
//!
//! QuickFuncs may also be used to build a descriptor (for conditional
//! fields, shared helpers, etc.), but whatever they return must match this
//! same shape -- the safest way to guarantee that is to have the QuickFunc
//! compose these very methods rather than hand-build the HashMap.

use crate::Builtins::Core::{BuiltinMethod, DixType, DixValue, IBuiltinMethod};
use crate::Builtins::Static::{IStaticObject, StaticObjectBase};
use std::collections::HashMap;

/// Schema static object implementation
pub struct SchemaObject {
    base: StaticObjectBase,
}

impl SchemaObject {
    pub fn new() -> Self {
        let mut base = StaticObjectBase::new("Schema".to_string());
        Self::initialize_methods(&mut base);
        SchemaObject { base }
    }

    fn initialize_methods(base: &mut StaticObjectBase) {
        // ---- scalar types with a numeric range (min/max) --------------------
        // Schema.int(required, min?, max?)
        base.register_method(Box::new(BuiltinMethod::new_variadic(
            "int".to_string(),
            1,
            DixType::Object,
            |args| build_numeric_descriptor("int", args),
            "Describes an int field: Schema.int(required, min?, max?)".to_string(),
        )));

        // Schema.long(required, min?, max?)
        base.register_method(Box::new(BuiltinMethod::new_variadic(
            "long".to_string(),
            1,
            DixType::Object,
            |args| build_numeric_descriptor("long", args),
            "Describes a long field: Schema.long(required, min?, max?)".to_string(),
        )));

        // Schema.float(required, min?, max?)
        base.register_method(Box::new(BuiltinMethod::new_variadic(
            "float".to_string(),
            1,
            DixType::Object,
            |args| build_numeric_descriptor("float", args),
            "Describes a float field: Schema.float(required, min?, max?)".to_string(),
        )));

        // Schema.double(required, min?, max?)
        base.register_method(Box::new(BuiltinMethod::new_variadic(
            "double".to_string(),
            1,
            DixType::Object,
            |args| build_numeric_descriptor("double", args),
            "Describes a double field: Schema.double(required, min?, max?)".to_string(),
        )));

        // ---- string, with optional length bounds -----------------------------
        // Schema.string(required, minLength?, maxLength?)
        base.register_method(Box::new(BuiltinMethod::new_variadic(
            "string".to_string(),
            1,
            DixType::Object,
            |args| {
                let required = extract_required(args)?;
                let mut obj = base_descriptor("string", required);
                insert_int_bound(&mut obj, args, 1, "minLength")?;
                insert_int_bound(&mut obj, args, 2, "maxLength")?;
                Ok(DixValue::from_object(obj))
            },
            "Describes a string field: Schema.string(required, minLength?, maxLength?)".to_string(),
        )));

        // ---- bool: no constraints beyond required ----------------------------
        // Schema.bool(required)
        base.register_method(Box::new(BuiltinMethod::new(
            "bool".to_string(),
            1,
            DixType::Object,
            |args| {
                let required = extract_required(args)?;
                Ok(DixValue::from_object(base_descriptor("bool", required)))
            },
            "Describes a bool field: Schema.bool(required)".to_string(),
        )));

        // ---- array, with optional item-count bounds --------------------------
        // Schema.array(required, minItems?, maxItems?)
        base.register_method(Box::new(BuiltinMethod::new_variadic(
            "array".to_string(),
            1,
            DixType::Object,
            |args| {
                let required = extract_required(args)?;
                let mut obj = base_descriptor("array", required);
                insert_int_bound(&mut obj, args, 1, "minItems")?;
                insert_int_bound(&mut obj, args, 2, "maxItems")?;
                Ok(DixValue::from_object(obj))
            },
            "Describes an array field: Schema.array(required, minItems?, maxItems?)".to_string(),
        )));

        // ---- enum: the enum name is mandatory, not optional ------------------
        // Schema.enum(required, enumName)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "enum".to_string(),
            2,
            DixType::Object,
            |args| {
                let required = extract_required(args)?;
                let enum_name = args[1].as_string();
                if enum_name.is_empty() {
                    return Err("Schema.enum: enumName must not be empty".to_string());
                }
                let mut obj = base_descriptor("enum", required);
                obj.insert("enumName".to_string(), DixValue::from_string(enum_name));
                Ok(DixValue::from_object(obj))
            },
            "Describes an enum field: Schema.enum(required, enumName)".to_string(),
            |args| args.len() == 2 && args[0].get_type() == DixType::Bool && args[1].get_type() == DixType::String,
        )));

        // ---- structural / presence-only types ---------------------------------
        // Every one of these is `required` and nothing else -- no per-type
        // constraint makes sense yet (validating a nested object's own shape,
        // for instance, is a real follow-up, not attempted here). Written out
        // explicitly rather than looped: BuiltinMethodImpl is a plain `fn`
        // pointer, not a closure type, so nothing here can capture a loop
        // variable -- each closure below is capture-free on purpose (the type
        // name is a string literal baked into that one closure, not a
        // variable from an enclosing scope).
        base.register_method(Box::new(BuiltinMethod::new(
            "object".to_string(),
            1,
            DixType::Object,
            |args| {
                let required = extract_required(args)?;
                Ok(DixValue::from_object(base_descriptor("object", required)))
            },
            "Describes an object field: Schema.object(required)".to_string(),
        )));

        base.register_method(Box::new(BuiltinMethod::new(
            "tuple".to_string(),
            1,
            DixType::Object,
            |args| {
                let required = extract_required(args)?;
                Ok(DixValue::from_object(base_descriptor("tuple", required)))
            },
            "Describes a tuple field: Schema.tuple(required)".to_string(),
        )));

        base.register_method(Box::new(BuiltinMethod::new(
            "date".to_string(),
            1,
            DixType::Object,
            |args| {
                let required = extract_required(args)?;
                Ok(DixValue::from_object(base_descriptor("date", required)))
            },
            "Describes a date field: Schema.date(required)".to_string(),
        )));

        base.register_method(Box::new(BuiltinMethod::new(
            "timestamp".to_string(),
            1,
            DixType::Object,
            |args| {
                let required = extract_required(args)?;
                Ok(DixValue::from_object(base_descriptor("timestamp", required)))
            },
            "Describes a timestamp field: Schema.timestamp(required)".to_string(),
        )));

        base.register_method(Box::new(BuiltinMethod::new(
            "hex".to_string(),
            1,
            DixType::Object,
            |args| {
                let required = extract_required(args)?;
                Ok(DixValue::from_object(base_descriptor("hex", required)))
            },
            "Describes a hex field: Schema.hex(required)".to_string(),
        )));

        base.register_method(Box::new(BuiltinMethod::new(
            "blob".to_string(),
            1,
            DixType::Object,
            |args| {
                let required = extract_required(args)?;
                Ok(DixValue::from_object(base_descriptor("blob", required)))
            },
            "Describes a blob field: Schema.blob(required)".to_string(),
        )));

        base.register_method(Box::new(BuiltinMethod::new(
            "regex".to_string(),
            1,
            DixType::Object,
            |args| {
                let required = extract_required(args)?;
                Ok(DixValue::from_object(base_descriptor("regex", required)))
            },
            "Describes a regex field: Schema.regex(required)".to_string(),
        )));
    }
}

/// The two keys every descriptor always has.
fn base_descriptor(type_name: &str, required: bool) -> HashMap<String, DixValue> {
    let mut obj = HashMap::new();
    obj.insert("type".to_string(), DixValue::from_string(type_name.to_string()));
    obj.insert("required".to_string(), DixValue::from_bool(required));
    obj
}

/// Every method's first argument is `required` -- pulled out and
/// type-checked in one place so each method's own closure just says
/// `extract_required(args)?` instead of repeating the same check.
fn extract_required(args: &[DixValue]) -> Result<bool, String> {
    if args.is_empty() || args[0].get_type() != DixType::Bool {
        return Err("Schema field descriptor: first argument (required) must be a bool".to_string());
    }
    Ok(args[0].as_bool())
}

/// Shared body for Schema.int/long/float/double -- same shape (required,
/// min?, max?), differing only in the "type" tag stored in the descriptor.
fn build_numeric_descriptor(type_name: &str, args: &[DixValue]) -> Result<DixValue, String> {
    let required = extract_required(args)?;
    let mut obj = base_descriptor(type_name, required);
    insert_numeric_bound(&mut obj, args, 1, "min")?;
    insert_numeric_bound(&mut obj, args, 2, "max")?;
    Ok(DixValue::from_object(obj))
}

/// Inserts `key` from `args[index]` if that argument was actually passed;
/// leaves the descriptor without `key` at all otherwise (not a null entry)
/// so `.get(key)` on the resolved descriptor means "unconstrained", not
/// "constrained to null". Accepts any numeric DixType, since a `double`
/// field's own min/max may be fractional.
fn insert_numeric_bound(obj: &mut HashMap<String, DixValue>, args: &[DixValue], index: usize, key: &str) -> Result<(), String> {
    if let Some(arg) = args.get(index) {
        if !arg.is_numeric() {
            return Err(format!("Schema field descriptor: '{key}' must be numeric"));
        }
        obj.insert(key.to_string(), arg.clone());
    }
    Ok(())
}

/// Same as `insert_numeric_bound`, but for count-style constraints
/// (string length, array item count) that only make sense as whole
/// numbers -- accepts Int or Long specifically, not Float/Double.
fn insert_int_bound(obj: &mut HashMap<String, DixValue>, args: &[DixValue], index: usize, key: &str) -> Result<(), String> {
    if let Some(arg) = args.get(index) {
        if !matches!(arg.get_type(), DixType::Int | DixType::Long) {
            return Err(format!("Schema field descriptor: '{key}' must be an int or long"));
        }
        obj.insert(key.to_string(), arg.clone());
    }
    Ok(())
}

impl Default for SchemaObject {
    fn default() -> Self {
        Self::new()
    }
}

impl IStaticObject for SchemaObject {
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
    fn test_schema_object_creation() {
        let schema = SchemaObject::new();
        assert_eq!(schema.name(), "Schema");
        assert!(schema.has_method("int"));
        assert!(schema.has_method("string"));
        assert!(schema.has_method("bool"));
        assert!(schema.has_method("enum"));
    }

    #[test]
    fn test_schema_int_no_bounds() {
        let schema = SchemaObject::new();
        let result = schema.call_method("int", &[DixValue::from_bool(true)]).unwrap();
        let obj = result.as_object();
        assert_eq!(obj.get("type").unwrap().as_string(), "int");
        assert!(obj.get("required").unwrap().as_bool());
        assert!(obj.get("min").is_none());
        assert!(obj.get("max").is_none());
    }

    #[test]
    fn test_schema_int_with_bounds() {
        let schema = SchemaObject::new();
        let result = schema
            .call_method(
                "int",
                &[DixValue::from_bool(true), DixValue::from_int(1025), DixValue::from_int(65535)],
            )
            .unwrap();
        let obj = result.as_object();
        assert_eq!(obj.get("min").unwrap().as_int(), 1025);
        assert_eq!(obj.get("max").unwrap().as_int(), 65535);
    }

    #[test]
    fn test_schema_string_length_bounds() {
        let schema = SchemaObject::new();
        let result = schema
            .call_method(
                "string",
                &[DixValue::from_bool(false), DixValue::from_int(1), DixValue::from_int(50)],
            )
            .unwrap();
        let obj = result.as_object();
        assert_eq!(obj.get("type").unwrap().as_string(), "string");
        assert!(!obj.get("required").unwrap().as_bool());
        assert_eq!(obj.get("minLength").unwrap().as_int(), 1);
        assert_eq!(obj.get("maxLength").unwrap().as_int(), 50);
    }

    #[test]
    fn test_schema_bool_rejects_non_bool_required() {
        let schema = SchemaObject::new();
        let result = schema.call_method("bool", &[DixValue::from_int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_enum_requires_name() {
        let schema = SchemaObject::new();
        let result = schema
            .call_method("enum", &[DixValue::from_bool(true), DixValue::from_string("ElementGroup".to_string())])
            .unwrap();
        let obj = result.as_object();
        assert_eq!(obj.get("type").unwrap().as_string(), "enum");
        assert_eq!(obj.get("enumName").unwrap().as_string(), "ElementGroup");
    }

    #[test]
    fn test_schema_presence_only_types() {
        let schema = SchemaObject::new();
        for name in ["object", "tuple", "date", "timestamp", "hex", "blob", "regex"] {
            let result = schema.call_method(name, &[DixValue::from_bool(true)]).unwrap();
            let obj = result.as_object();
            assert_eq!(obj.get("type").unwrap().as_string(), name);
            assert!(obj.get("required").unwrap().as_bool());
        }
    }
}
