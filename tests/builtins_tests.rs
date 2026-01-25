// tests/builtins_tests.rs
//! Comprehensive tests for DixScript builtins system
//!
//! Tests cover:
//! - DixType and DixValue core functionality
//! - Type conversions and coercions
//! - Arithmetic and comparison operations
//! - Instance methods (String, Array, Number, etc.)
//! - Static objects (Math, DateTime, Array, etc.)
//! - Builtin call resolver and validator
//! - Error handling and edge cases

use dixscript::Builtins::Core::{DixType, DixValue};
use dixscript::Builtins::Resolver::{
    initialize, is_initialized,
    resolve_static_call, resolve_instance_call,
    validate_static_call, validate_instance_call,
    has_static_object, has_static_method, has_instance_method,
    get_static_objects, get_static_methods, get_instance_methods,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

// ==================== SETUP/TEARDOWN ====================

fn setup_builtins() {
    // Initialize builtin resolver
    if !is_initialized() {
        initialize();
    }
}

fn teardown_builtins() {
    // Any cleanup needed
}

// ==================== DIX_TYPE TESTS ====================

#[test]
fn test_dix_type_basic_properties() {
    assert!(DixType::Int.is_numeric());
    assert!(DixType::Float.is_numeric());
    assert!(DixType::Double.is_numeric());
    assert!(!DixType::String.is_numeric());

    assert!(DixType::Array.is_collection());
    assert!(DixType::Tuple.is_collection());
    assert!(DixType::Object.is_collection());
    assert!(!DixType::Int.is_collection());

    assert!(DixType::Array.is_indexable());
    assert!(DixType::Object.is_indexable());
    assert!(DixType::String.is_indexable());
    assert!(!DixType::Int.is_indexable());
}

#[test]
fn test_dix_type_names() {
    assert_eq!(DixType::Int.get_type_name(), "int");
    assert_eq!(DixType::Float.get_type_name(), "float");
    assert_eq!(DixType::Double.get_type_name(), "double");
    assert_eq!(DixType::String.get_type_name(), "string");
    assert_eq!(DixType::Bool.get_type_name(), "bool");
    assert_eq!(DixType::Array.get_type_name(), "array");
    assert_eq!(DixType::Tuple.get_type_name(), "tuple");
    assert_eq!(DixType::Object.get_type_name(), "object");
}

#[test]
fn test_dix_type_conversions() {
    assert!(DixType::Int.can_convert_to(DixType::Float));
    assert!(DixType::Float.can_convert_to(DixType::Double));
    assert!(DixType::Int.can_convert_to(DixType::String));
    assert!(DixType::Null.can_convert_to(DixType::String));
    assert!(DixType::Date.can_convert_to(DixType::Timestamp));

    assert!(!DixType::String.can_convert_to(DixType::Int));
}

#[test]
fn test_dix_type_common_types() {
    assert_eq!(DixType::Int.get_common_type(DixType::Int), DixType::Int);
    assert_eq!(DixType::Int.get_common_type(DixType::Float), DixType::Float);
    assert_eq!(DixType::Int.get_common_type(DixType::Double), DixType::Double);
    assert_eq!(DixType::Float.get_common_type(DixType::Double), DixType::Double);
    assert_eq!(DixType::String.get_common_type(DixType::Int), DixType::String);
}

// ==================== DIX_VALUE BASIC TESTS ====================

#[test]
fn test_dix_value_constructors() {
    let int_val = DixValue::from_int(42);
    assert_eq!(int_val.get_type(), DixType::Int);
    assert_eq!(int_val.as_int(), 42);

    let float_val = DixValue::from_float(3.14);
    assert_eq!(float_val.get_type(), DixType::Float);
    assert!((float_val.as_float() - 3.14).abs() < 0.001);

    let string_val = DixValue::from_string("hello".to_string());
    assert_eq!(string_val.get_type(), DixType::String);
    assert_eq!(string_val.as_string(), "hello");

    let bool_val = DixValue::from_bool(true);
    assert_eq!(bool_val.get_type(), DixType::Bool);
    assert!(bool_val.as_bool());

    let null_val = DixValue::null();
    assert_eq!(null_val.get_type(), DixType::Null);
    assert!(null_val.is_null());
}

#[test]
fn test_dix_value_type_queries() {
    let int_val = DixValue::from_int(42);
    assert!(int_val.is_numeric());
    assert!(!int_val.is_string());
    assert!(!int_val.is_array());
    assert!(!int_val.is_object());

    let string_val = DixValue::from_string("test".to_string());
    assert!(!string_val.is_numeric());
    assert!(string_val.is_string());

    let array_val = DixValue::from_array(vec![DixValue::from_int(1)]);
    assert!(array_val.is_array());
    assert!(!array_val.is_numeric());
}

// ==================== TYPE CONVERSION TESTS ====================

#[test]
fn test_numeric_conversions() {
    let int_val = DixValue::from_int(42);
    assert_eq!(int_val.as_float(), 42.0);
    assert_eq!(int_val.as_double(), 42.0);
    assert_eq!(int_val.as_string(), "42");
    assert!(int_val.as_bool()); // Non-zero is true

    let float_val = DixValue::from_float(3.14);
    assert_eq!(float_val.as_int(), 3);
    assert!((float_val.as_double() - 3.14).abs() < 0.001);

    let zero_val = DixValue::from_int(0);
    assert!(!zero_val.as_bool()); // Zero is false
}

#[test]
fn test_string_conversions() {
    let string_val = DixValue::from_string("42".to_string());
    assert_eq!(string_val.as_int(), 42);
    assert!((string_val.as_float() - 42.0).abs() < 0.001);

    let bool_string = DixValue::from_string("true".to_string());
    assert!(bool_string.as_bool()); // Non-empty string is true

    let empty_string = DixValue::from_string("".to_string());
    assert!(!empty_string.as_bool()); // Empty string is false
}

#[test]
fn test_array_conversions() {
    let arr = DixValue::from_array(vec![
        DixValue::from_int(1),
        DixValue::from_int(2),
        DixValue::from_int(3),
    ]);

    assert_eq!(arr.as_array().len(), 3);
    assert!(arr.as_bool()); // Non-empty array is true
    assert_eq!(arr.as_string(), "[...]");

    let empty_arr = DixValue::from_array(vec![]);
    assert!(!empty_arr.as_bool()); // Empty array is false
}

// ==================== ARITHMETIC OPERATIONS TESTS ====================

#[test]
fn test_addition() {
    let a = DixValue::from_int(10);
    let b = DixValue::from_int(5);
    let result = a.add(&b).unwrap();
    assert_eq!(result.as_int(), 15);

    let c = DixValue::from_float(3.5);
    let d = DixValue::from_float(2.5);
    let result = c.add(&d).unwrap();
    assert!((result.as_float() - 6.0).abs() < 0.001);

    // String concatenation
    let s1 = DixValue::from_string("Hello ".to_string());
    let s2 = DixValue::from_string("World".to_string());
    let result = s1.add(&s2).unwrap();
    assert_eq!(result.as_string(), "Hello World");
}

#[test]
fn test_subtraction() {
    let a = DixValue::from_int(10);
    let b = DixValue::from_int(3);
    let result = a.subtract(&b).unwrap();
    assert_eq!(result.as_int(), 7);

    let c = DixValue::from_double(10.5);
    let d = DixValue::from_double(3.2);
    let result = c.subtract(&d).unwrap();
    assert!((result.as_double() - 7.3).abs() < 0.001);
}

#[test]
fn test_multiplication() {
    let a = DixValue::from_int(6);
    let b = DixValue::from_int(7);
    let result = a.multiply(&b).unwrap();
    assert_eq!(result.as_int(), 42);

    let c = DixValue::from_float(2.5);
    let d = DixValue::from_float(4.0);
    let result = c.multiply(&d).unwrap();
    assert!((result.as_float() - 10.0).abs() < 0.001);
}

#[test]
fn test_division() {
    let a = DixValue::from_int(10);
    let b = DixValue::from_int(2);
    let result = a.divide(&b).unwrap();
    assert_eq!(result.as_int(), 5);

    let c = DixValue::from_double(10.0);
    let d = DixValue::from_double(4.0);
    let result = c.divide(&d).unwrap();
    assert!((result.as_double() - 2.5).abs() < 0.001);
}

#[test]
fn test_division_by_zero() {
    let a = DixValue::from_int(10);
    let b = DixValue::from_int(0);
    let result = a.divide(&b);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Division by zero"));
}

// ==================== COMPARISON TESTS ====================

#[test]
fn test_equality() {
    let a = DixValue::from_int(42);
    let b = DixValue::from_int(42);
    assert!(a.equal_to(&b));

    let c = DixValue::from_int(10);
    assert!(!a.equal_to(&c));

    // Numeric type coercion
    let int_val = DixValue::from_int(10);
    let float_val = DixValue::from_float(10.0);
    assert!(int_val.equal_to(&float_val));
}

#[test]
fn test_greater_than() {
    let a = DixValue::from_int(10);
    let b = DixValue::from_int(5);
    assert!(a.greater_than(&b).unwrap());
    assert!(!b.greater_than(&a).unwrap());

    let c = DixValue::from_double(10.5);
    let d = DixValue::from_double(5.2);
    assert!(c.greater_than(&d).unwrap());
}

#[test]
fn test_less_than() {
    let a = DixValue::from_int(5);
    let b = DixValue::from_int(10);
    assert!(a.less_than(&b).unwrap());
    assert!(!b.less_than(&a).unwrap());
}

#[test]
fn test_string_comparison() {
    let a = DixValue::from_string("apple".to_string());
    let b = DixValue::from_string("banana".to_string());
    assert!(a.less_than(&b).unwrap());
    assert!(b.greater_than(&a).unwrap());

    let c = DixValue::from_string("apple".to_string());
    assert!(a.equal_to(&c));
}

// ==================== COLLECTION OPERATIONS TESTS ====================

#[test]
fn test_array_operations() {
    let mut arr = DixValue::from_array(vec![
        DixValue::from_int(1),
        DixValue::from_int(2),
        DixValue::from_int(3),
    ]);

    assert_eq!(arr.as_array().len(), 3);
    assert_eq!(arr.as_array()[0].as_int(), 1);
    assert_eq!(arr.as_array()[2].as_int(), 3);

    // Mutate array
    arr.as_array_mut().push(DixValue::from_int(4));
    assert_eq!(arr.as_array().len(), 4);
}

#[test]
fn test_object_operations() {
    let mut obj = HashMap::new();
    obj.insert("name".to_string(), DixValue::from_string("Alice".to_string()));
    obj.insert("age".to_string(), DixValue::from_int(30));

    let obj_val = DixValue::from_object(obj);

    assert_eq!(obj_val.as_object().len(), 2);
    assert_eq!(obj_val.as_object()["name"].as_string(), "Alice");
    assert_eq!(obj_val.as_object()["age"].as_int(), 30);
}

#[test]
fn test_tuple_operations() {
    let tuple = DixValue::from_tuple(vec![
        DixValue::from_int(42),
        DixValue::from_string("test".to_string()),
        DixValue::from_bool(true),
    ]);

    assert_eq!(tuple.as_array().len(), 3);
    assert_eq!(tuple.as_array()[0].as_int(), 42);
    assert_eq!(tuple.as_array()[1].as_string(), "test");
    assert!(tuple.as_array()[2].as_bool());
}

// ==================== BLOB OPERATIONS TESTS ====================

#[test]
fn test_blob_basic() {
    let base64_data = "SGVsbG8gV29ybGQ=".to_string(); // "Hello World" in base64
    let blob = DixValue::from_blob(base64_data.clone()).unwrap();

    assert_eq!(blob.get_type(), DixType::Blob);
    assert_eq!(blob.as_blob_base64().unwrap(), base64_data);
}

#[test]
fn test_blob_bytes() {
    let base64_data = "SGVsbG8gV29ybGQ=".to_string();
    let blob = DixValue::from_blob(base64_data).unwrap();

    let bytes = blob.as_blob_bytes().unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(text, "Hello World");
}

#[test]
fn test_blob_metadata() {
    // PNG magic number in base64
    let png_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    let blob = DixValue::from_blob(png_data.to_string()).unwrap();

    let (mime_type, size, _dimensions) = blob.get_blob_metadata().unwrap();
    assert_eq!(mime_type, "image/png");
    assert!(size > 0);
}

// ==================== REGEX OPERATIONS TESTS ====================

#[test]
fn test_regex_creation() {
    let pattern = "^[a-z]+$".to_string();
    let regex = DixValue::from_regex(pattern.clone()).unwrap();

    assert_eq!(regex.get_type(), DixType::Regex);
    assert_eq!(regex.as_string(), pattern);
}

#[test]
fn test_regex_invalid_pattern() {
    let invalid_pattern = "[invalid(".to_string();
    let result = DixValue::from_regex(invalid_pattern);

    assert!(result.is_err());
}

// ==================== STATIC OBJECT TESTS ====================

#[test]
fn test_static_objects_initialized() {
    setup_builtins();

    assert!(has_static_object("Math"));
    assert!(has_static_object("DateTime"));
    assert!(has_static_object("Array"));
    assert!(has_static_object("Dix"));
    assert!(has_static_object("Random"));
    assert!(!has_static_object("NonExistent"));

    teardown_builtins();
}

#[test]
fn test_static_objects_list() {
    setup_builtins();

    let objects = get_static_objects();
    assert!(!objects.is_empty());
    assert!(objects.contains(&"Math".to_string()));
    assert!(objects.contains(&"DateTime".to_string()));

    println!("Static objects: {:#?}", objects);

    teardown_builtins();
}

#[test]
fn test_math_static_methods() {
    setup_builtins();

    assert!(has_static_method("Math", "max"));
    assert!(has_static_method("Math", "min"));
    assert!(has_static_method("Math", "abs"));
    assert!(has_static_method("Math", "sqrt"));
    assert!(!has_static_method("Math", "nonexistent"));

    let methods = get_static_methods("Math");
    println!("Math methods: {:#?}", methods);
    assert!(!methods.is_empty());

    teardown_builtins();
}

#[test]
fn test_math_max() {
    setup_builtins();

    let a = DixValue::from_int(10);
    let b = DixValue::from_int(5);

    let result = resolve_static_call("Math", "max", &[a, b]).unwrap();
    assert_eq!(result.as_int(), 10);

    teardown_builtins();
}

#[test]
fn test_math_min() {
    setup_builtins();

    let a = DixValue::from_double(3.14);
    let b = DixValue::from_double(2.71);

    let result = resolve_static_call("Math", "min", &[a, b]).unwrap();
    assert!((result.as_double() - 2.71).abs() < 0.001);

    teardown_builtins();
}

#[test]
fn test_datetime_now() {
    setup_builtins();

    let result = resolve_static_call("DateTime", "now", &[]).unwrap();
    assert_eq!(result.get_type(), DixType::Timestamp);

    let now = Utc::now();
    let result_time = result.as_datetime();

    // Should be within 1 second
    let diff = (now.timestamp() - result_time.timestamp()).abs();
    assert!(diff < 2);

    teardown_builtins();
}

// ==================== INSTANCE METHOD TESTS ====================

#[test]
fn test_string_instance_methods() {
    setup_builtins();

    assert!(has_instance_method(DixType::String, "toUpper"));
    assert!(has_instance_method(DixType::String, "toLower"));
    assert!(has_instance_method(DixType::String, "length"));
    assert!(has_instance_method(DixType::String, "substring"));

    let methods = get_instance_methods(DixType::String);
    println!("String instance methods: {:#?}", methods);
    assert!(!methods.is_empty());

    teardown_builtins();
}

#[test]
fn test_string_to_upper() {
    setup_builtins();

    let string_val = DixValue::from_string("hello world".to_string());
    let result = resolve_instance_call(&string_val, "toUpper", &[]).unwrap();

    assert_eq!(result.as_string(), "HELLO WORLD");

    teardown_builtins();
}

#[test]
fn test_string_to_lower() {
    setup_builtins();

    let string_val = DixValue::from_string("HELLO WORLD".to_string());
    let result = resolve_instance_call(&string_val, "toLower", &[]).unwrap();

    assert_eq!(result.as_string(), "hello world");

    teardown_builtins();
}

#[test]
fn test_string_length() {
    setup_builtins();

    let string_val = DixValue::from_string("hello".to_string());
    let result = resolve_instance_call(&string_val, "length", &[]).unwrap();

    assert_eq!(result.as_int(), 5);

    teardown_builtins();
}

#[test]
fn test_array_instance_methods() {
    setup_builtins();

    assert!(has_instance_method(DixType::Array, "length"));
    assert!(has_instance_method(DixType::Array, "first"));
    assert!(has_instance_method(DixType::Array, "last"));

    teardown_builtins();
}

#[test]
fn test_array_length() {
    setup_builtins();

    let array = DixValue::from_array(vec![
        DixValue::from_int(1),
        DixValue::from_int(2),
        DixValue::from_int(3),
    ]);

    let result = resolve_instance_call(&array, "length", &[]).unwrap();
    assert_eq!(result.as_int(), 3);

    teardown_builtins();
}

#[test]
fn test_number_instance_methods() {
    setup_builtins();

    assert!(has_instance_method(DixType::Int, "abs"));
    assert!(has_instance_method(DixType::Float, "abs"));
    assert!(has_instance_method(DixType::Double, "abs"));

    teardown_builtins();
}

#[test]
fn test_number_abs() {
    setup_builtins();

    let negative = DixValue::from_int(-42);
    let result = resolve_instance_call(&negative, "abs", &[]).unwrap();

    assert_eq!(result.as_int(), 42);

    teardown_builtins();
}

// ==================== UNIVERSAL METHOD TESTS ====================

#[test]
fn test_universal_method_to_string() {
    setup_builtins();

    // Test on various types
    let int_val = DixValue::from_int(42);
    let result = resolve_instance_call(&int_val, "toString", &[]).unwrap();
    assert_eq!(result.as_string(), "42");

    let bool_val = DixValue::from_bool(true);
    let result = resolve_instance_call(&bool_val, "toString", &[]).unwrap();
    assert_eq!(result.as_string(), "true");

    teardown_builtins();
}

#[test]
fn test_universal_method_type() {
    setup_builtins();

    let int_val = DixValue::from_int(42);
    let result = resolve_instance_call(&int_val, "type", &[]).unwrap();
    assert_eq!(result.as_string(), "int");

    let string_val = DixValue::from_string("hello".to_string());
    let result = resolve_instance_call(&string_val, "type", &[]).unwrap();
    assert_eq!(result.as_string(), "string");

    teardown_builtins();
}

// ==================== VALIDATION TESTS ====================

#[test]
fn test_validate_static_call_success() {
    setup_builtins();

    let result = validate_static_call("Math", "max", 2);
    assert!(result.is_valid);
    assert!(result.error_message.is_none());

    teardown_builtins();
}

#[test]
fn test_validate_static_call_wrong_arg_count() {
    setup_builtins();

    let result = validate_static_call("Math", "max", 3);
    assert!(!result.is_valid);
    assert!(result.error_message.is_some());

    println!("Validation error: {}", result.error_message.unwrap());

    teardown_builtins();
}

#[test]
fn test_validate_static_call_unknown_object() {
    setup_builtins();

    let result = validate_static_call("UnknownObject", "method", 0);
    assert!(!result.is_valid);
    assert!(result.error_message.is_some());

    teardown_builtins();
}

#[test]
fn test_validate_instance_call_success() {
    setup_builtins();

    let result = validate_instance_call(DixType::String, "toUpper", 0);
    assert!(result.is_valid);

    teardown_builtins();
}

#[test]
fn test_validate_instance_call_unknown_method() {
    setup_builtins();

    let result = validate_instance_call(DixType::String, "unknownMethod", 0);
    assert!(!result.is_valid);
    assert!(result.error_message.is_some());

    teardown_builtins();
}

// ==================== ERROR HANDLING TESTS ====================

#[test]
fn test_static_call_unknown_object() {
    setup_builtins();

    let result = resolve_static_call("UnknownObject", "method", &[]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown"));

    teardown_builtins();
}

#[test]
fn test_static_call_unknown_method() {
    setup_builtins();

    let result = resolve_static_call("Math", "unknownMethod", &[]);
    assert!(result.is_err());

    teardown_builtins();
}

#[test]
fn test_instance_call_type_mismatch() {
    setup_builtins();

    let int_val = DixValue::from_int(42);
    let result = resolve_instance_call(&int_val, "toUpper", &[]);

    // Int doesn't have toUpper method
    assert!(result.is_err());

    teardown_builtins();
}

// ==================== PERFORMANCE TESTS ====================

#[test]
fn test_bulk_value_operations() {
    let start = std::time::Instant::now();

    for i in 0..1000 {
        let a = DixValue::from_int(i);
        let b = DixValue::from_int(i + 1);
        let _sum = a.add(&b).unwrap();
    }

    let duration = start.elapsed();
    println!("1000 additions took: {:?}", duration);
    assert!(duration.as_millis() < 100);
}

#[test]
fn test_bulk_string_operations() {
    setup_builtins();

    let start = std::time::Instant::now();

    for _i in 0..1000 {
        let s = DixValue::from_string("test string".to_string());
        let _result = resolve_instance_call(&s, "toUpper", &[]).unwrap();
    }

    let duration = start.elapsed();
    println!("1000 toUpper calls took: {:?}", duration);

    teardown_builtins();
}

// ==================== INTEGRATION TESTS ====================

#[test]
fn test_full_builtin_workflow() {
    setup_builtins();

    println!("\n=== BUILTIN INTEGRATION TEST ===");

    // Test static objects
    let objects = get_static_objects();
    println!("Static objects: {}", objects.len());
    for obj in &objects {
        let methods = get_static_methods(obj);
        println!("  {}: {} methods", obj, methods.len());
    }

    // Test Math operations
    let max_result = resolve_static_call(
        "Math",
        "max",
        &[DixValue::from_int(10), DixValue::from_int(20)],
    ).unwrap();
    println!("Math.max(10, 20) = {}", max_result.as_int());
    assert_eq!(max_result.as_int(), 20);

    // Test String operations
    let string_val = DixValue::from_string("hello world".to_string());
    let upper_result = resolve_instance_call(&string_val, "toUpper", &[]).unwrap();
    println!("'hello world'.toUpper() = {}", upper_result.as_string());
    assert_eq!(upper_result.as_string(), "HELLO WORLD");

    // Test Array operations
    let array = DixValue::from_array(vec![
        DixValue::from_int(1),
        DixValue::from_int(2),
        DixValue::from_int(3),
    ]);
    let length = resolve_instance_call(&array, "length", &[]).unwrap();
    println!("[1,2,3].length() = {}", length.as_int());
    assert_eq!(length.as_int(), 3);

    // Test arithmetic
    let a = DixValue::from_double(10.5);
    let b = DixValue::from_double(5.5);
    let sum = a.add(&b).unwrap();
    println!("10.5 + 5.5 = {}", sum.as_double());
    assert!((sum.as_double() - 16.0).abs() < 0.001);

    // Test validation
    let validation = validate_static_call("Math", "max", 2);
    println!("Validation Math.max(2 args): {}", validation.is_valid);
    assert!(validation.is_valid);

    println!("================================\n");

    teardown_builtins();
}

#[test]
fn test_comprehensive_type_coverage() {
    setup_builtins();

    println!("\n=== TYPE METHOD COVERAGE TEST ===");

    let types = vec![
        DixType::Int,
        DixType::Float,
        DixType::Double,
        DixType::String,
        DixType::Bool,
        DixType::Array,
        DixType::Tuple,
        DixType::Object,
    ];

    for dix_type in types {
        let methods = get_instance_methods(dix_type);
        println!("{}: {} instance methods", dix_type.get_type_name(), methods.len());

        // Verify at least universal methods exist
        assert!(!methods.is_empty(), "{} should have instance methods", dix_type.get_type_name());
    }

    println!("=================================\n");

    teardown_builtins();
}