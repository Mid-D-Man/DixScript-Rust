
// tests/ast_tests.rs

use dixscript::Compiler::AST::*;
use std::time::Instant;

// ==================== HELPER FUNCTIONS ====================

fn measure_time<F, R>(name: &str, f: F) -> (R, std::time::Duration)
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    let duration = start.elapsed();
    println!("{}: {:?}", name, duration);
    (result, duration)
}

fn print_ast_stats(name: &str, node_count: usize, duration: std::time::Duration) {
    let nodes_per_sec = node_count as f64 / duration.as_secs_f64();
    println!("\n=== {} ===", name);
    println!("  Nodes: {}", node_count);
    println!("  Time: {:?}", duration);
    println!("  Speed: {:.0} nodes/sec", nodes_per_sec);
    println!("  Per node: {:.2} µs", duration.as_micros() as f64 / node_count as f64);
}

// ==================== BASIC NODE CREATION ====================

#[test]
fn test_position_creation() {
    let pos = Position::new(10, 5);
    assert_eq!(pos.line, 10);
    assert_eq!(pos.column, 5);
    assert!(pos.is_valid());
    assert!(!pos.is_unknown());
}

#[test]
fn test_position_unknown() {
    let pos = Position::UNKNOWN;
    assert!(pos.is_unknown());
    assert!(!pos.is_valid());
    assert_eq!(pos.to_short_string(), "??:??");
}

#[test]
fn test_position_start() {
    let pos = Position::START;
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);
    assert!(pos.is_valid());
}

#[test]
fn test_position_display() {
    let pos = Position::new(42, 10);
    assert_eq!(pos.to_string(), "Line 42, Column 10");
    assert_eq!(pos.to_short_string(), "42:10");
}

// ==================== DATA TYPE TESTS ====================

#[test]
fn test_data_types() {
    assert_eq!(DataType::Int.to_string(), "int");
    assert_eq!(DataType::Float.to_string(), "float");
    assert_eq!(DataType::String.to_string(), "string");
    assert_eq!(DataType::Any.to_string(), "any");
}

#[test]
fn test_error_handling_strategy() {
    assert_eq!(ErrorHandlingStrategy::Halt.to_string(), "halt");
    assert_eq!(ErrorHandlingStrategy::Continue.to_string(), "continue");
    assert_eq!(ErrorHandlingStrategy::Recover.to_string(), "recover");
}

// ==================== VALUE CREATION ====================

#[test]
fn test_create_integer_value() {
    let pos = Position::new(1, 1);
    let val = create_int(42, pos);
    
    match val {
        Value::Integer { value, position } => {
            assert_eq!(value, 42);
            assert_eq!(position, pos);
        }
        _ => panic!("Expected Integer value"),
    }
    
    assert_eq!(val.to_string(), "42");
}

#[test]
fn test_create_float_value() {
    let val = create_float(3.14, Position::START);
    assert_eq!(val.to_string(), "3.14");
}

#[test]
fn test_create_string_value() {
    let val = create_string("Hello, World!".to_string(), Position::START);
    assert_eq!(val.to_string(), "\"Hello, World!\"");
}

#[test]
fn test_create_bool_value() {
    let val_true = create_bool(true, Position::START);
    let val_false = create_bool(false, Position::START);
    
    assert_eq!(val_true.to_string(), "true");
    assert_eq!(val_false.to_string(), "false");
}

#[test]
fn test_create_null_value() {
    let val = create_null(Position::START);
    assert_eq!(val.to_string(), "null");
}

#[test]
fn test_create_array_value() {
    let pos = Position::START;
    let values = vec![
        create_int(1, pos),
        create_int(2, pos),
        create_int(3, pos),
    ];
    
    let arr = create_array(values, pos);
    assert_eq!(arr.to_string(), "[1, 2, 3]");
}

#[test]
fn test_create_object_value() {
    let pos = Position::START;
    let properties = vec![
        ObjectProperty::new("id".to_string(), create_int(1, pos), pos),
        ObjectProperty::new("name".to_string(), create_string("John".to_string(), pos), pos),
        ObjectProperty::new("active".to_string(), create_bool(true, pos), pos),
    ];
    
    let obj = create_object(properties, pos);
    let obj_str = obj.to_string();
    
    assert!(obj_str.contains("id = 1"));
    assert!(obj_str.contains("name = \"John\""));
    assert!(obj_str.contains("active = true"));
}

// ==================== EXPRESSION CREATION ====================

#[test]
fn test_create_identifier_expression() {
    let expr = create_identifier("myVar".to_string(), Position::START);
    assert_eq!(expr.to_string(), "myVar");
}

#[test]
fn test_create_arithmetic_expression() {
    let left = create_identifier("x".to_string(), Position::START);
    let right = create_identifier("y".to_string(), Position::START);
    
    let expr = create_arithmetic(left, "+".to_string(), right, Position::START);
    assert_eq!(expr.to_string(), "(x + y)");
}

#[test]
fn test_create_complex_arithmetic_expression() {
    let pos = Position::START;
    
    // (a + b) * c
    let a = create_identifier("a".to_string(), pos);
    let b = create_identifier("b".to_string(), pos);
    let c = create_identifier("c".to_string(), pos);
    
    let add = create_arithmetic(a, "+".to_string(), b, pos);
    let mul = create_arithmetic(add, "*".to_string(), c, pos);
    
    assert_eq!(mul.to_string(), "((a + b) * c)");
}

// ==================== STATEMENT CREATION ====================

#[test]
fn test_create_assignment_statement() {
    let pos = Position::START;
    let value = create_identifier("42".to_string(), pos);
    let stmt = create_assignment("x".to_string(), value, pos);
    
    assert_eq!(stmt.to_string(), "x = 42");
}

#[test]
fn test_create_return_statement() {
    let pos = Position::START;
    let value = create_identifier("result".to_string(), pos);
    let stmt = create_return(value, pos);
    
    assert_eq!(stmt.to_string(), "return result");
}

#[test]
fn test_create_if_statement() {
    let pos = Position::START;
    let condition = create_identifier("condition".to_string(), pos);
    let then_branch = vec![
        create_assignment("x".to_string(), create_identifier("1".to_string(), pos), pos),
    ];
    
    let stmt = create_if(condition, then_branch, None, pos);
    let stmt_str = stmt.to_string();
    
    assert!(stmt_str.contains("if: condition"));
    assert!(stmt_str.contains("x = 1"));
}

// ==================== SECTION CREATION ====================

#[test]
fn test_create_config_section() {
    let pos = Position::START;
    let entries = vec![
        create_config_entry("version".to_string(), create_config_string("1.0.0".to_string()), pos),
        create_config_entry("author".to_string(), create_config_string("Test".to_string()), pos),
    ];
    
    let section = ConfigSection::new(entries, pos);
    let section_str = section.to_string();
    
    assert!(section_str.contains("@CONFIG("));
    assert!(section_str.contains("version -> \"1.0.0\""));
    assert!(section_str.contains("author -> \"Test\""));
}

#[test]
fn test_create_enum_section() {
    let pos = Position::START;
    let fields = vec![
        create_enum_field("FIRST".to_string(), Some(1), pos),
        create_enum_field("SECOND".to_string(), Some(2), pos),
        create_enum_field("THIRD".to_string(), Some(3), pos),
    ];
    
    let enum_decl = create_enum("Status".to_string(), fields, pos);
    let section = EnumsSection::new(vec![enum_decl], pos);
    let section_str = section.to_string();
    
    assert!(section_str.contains("@ENUMS("));
    assert!(section_str.contains("Status {"));
    assert!(section_str.contains("FIRST = 1"));
}

#[test]
fn test_create_data_section() {
    let pos = Position::START;
    let entries = vec![
        create_simple_property("x".to_string(), create_int(42, pos), Some(DataType::Int), pos),
        create_simple_property("name".to_string(), create_string("Test".to_string(), pos), Some(DataType::String), pos),
    ];
    
    let section = DataSection::new(entries, pos);
    let section_str = section.to_string();
    
    assert!(section_str.contains("@DATA("));
    assert!(section_str.contains("x<int> = 42"));
    assert!(section_str.contains("name<string> = \"Test\""));
}

// ==================== FULL AST CREATION ====================

#[test]
fn test_create_simple_dixscript() {
    let pos = Position::START;
    
    let config = Some(ConfigSection::new(
        vec![create_config_entry("version".to_string(), create_config_string("1.0.0".to_string()), pos)],
        pos,
    ));
    
    let data = Some(DataSection::new(
        vec![create_simple_property("x".to_string(), create_int(42, pos), None, pos)],
        pos,
    ));
    
    let ast = DixScript::with_sections(config, None, None, None, None, data, None);
    
    let ast_str = ast.to_string();
    assert!(ast_str.contains("@CONFIG("));
    assert!(ast_str.contains("@DATA("));
    assert!(ast_str.contains("x = 42"));
}

#[test]
fn test_create_empty_dixscript() {
    let ast = DixScript::new();
    
    assert!(ast.config.is_none());
    assert!(ast.imports.is_none());
    assert!(ast.dlm.is_none());
    assert!(ast.enums.is_none());
    assert!(ast.quick_functions.is_none());
    assert!(ast.data.is_none());
    assert!(ast.security.is_none());
}

// ==================== PERFORMANCE TESTS ====================

#[test]
fn test_ast_construction_performance_small() {
    let pos = Position::START;
    let iterations = 1000;
    
    let (_, duration) = measure_time("Small AST construction", || {
        for _ in 0..iterations {
            let _val = create_int(42, pos);
            let _expr = create_identifier("x".to_string(), pos);
            let _stmt = create_assignment("x".to_string(), create_identifier("42".to_string(), pos), pos);
        }
    });
    
    print_ast_stats("Small nodes", iterations * 3, duration);
    
    // Should create at least 100k nodes/sec
    let nodes_per_sec = (iterations * 3) as f64 / duration.as_secs_f64();
    assert!(nodes_per_sec > 100_000.0, "Too slow: {:.0} nodes/sec", nodes_per_sec);
}

#[test]
fn test_ast_construction_performance_medium() {
    let pos = Position::START;
    let iterations = 100;
    
    let (_, duration) = measure_time("Medium AST construction", || {
        for i in 0..iterations {
            // Create a data section with 100 properties
            let mut entries = Vec::with_capacity(100);
            for j in 0..100 {
                let name = format!("var_{}", i * 100 + j);
                let value = create_int(j as i32, pos);
                entries.push(create_simple_property(name, value, Some(DataType::Int), pos));
            }
            let _section = DataSection::new(entries, pos);
        }
    });
    
    let total_nodes = iterations * 100;
    print_ast_stats("Medium AST", total_nodes, duration);
    
    // Should create at least 50k nodes/sec
    let nodes_per_sec = total_nodes as f64 / duration.as_secs_f64();
    assert!(nodes_per_sec > 50_000.0, "Too slow: {:.0} nodes/sec", nodes_per_sec);
}

#[test]
fn test_ast_construction_performance_large() {
    let pos = Position::START;
    
    let (node_count, duration) = measure_time("Large AST construction", || {
        let mut entries = Vec::with_capacity(10000);
        
        for i in 0..10000 {
            let name = format!("variable_{}", i);
            let value = create_int(i as i32, pos);
            entries.push(create_simple_property(name, value, Some(DataType::Int), pos));
        }
        
        let _section = DataSection::new(entries, pos);
        10000
    });
    
    print_ast_stats("Large AST", node_count, duration);
    
    // Should create at least 50k nodes/sec even for large ASTs
    let nodes_per_sec = node_count as f64 / duration.as_secs_f64();
    assert!(nodes_per_sec > 50_000.0, "Too slow: {:.0} nodes/sec", nodes_per_sec);
}

#[test]
fn test_ast_construction_complex_structures() {
    let pos = Position::START;
    let iterations = 1000;
    
    let (_, duration) = measure_time("Complex structure construction", || {
        for i in 0..iterations {
            // Create nested object with arrays
            let inner_obj = create_object(vec![
                ObjectProperty::new("id".to_string(), create_int(i as i32, pos), pos),
                ObjectProperty::new("name".to_string(), create_string(format!("Item {}", i), pos), pos),
            ], pos);
            
            let array = create_array(vec![
                create_int(1, pos),
                create_int(2, pos),
                create_int(3, pos),
            ], pos);
            
            let _outer_obj = create_object(vec![
                ObjectProperty::new("data".to_string(), inner_obj, pos),
                ObjectProperty::new("values".to_string(), array, pos),
            ], pos);
        }
    });
    
    // Each iteration creates ~10 nodes
    print_ast_stats("Complex structures", iterations * 10, duration);
}

#[test]
fn test_ast_display_performance() {
    let pos = Position::START;
    
    // Create a moderately sized AST
    let mut entries = Vec::with_capacity(1000);
    for i in 0..1000 {
        let name = format!("var_{}", i);
        let value = create_int(i as i32, pos);
        entries.push(create_simple_property(name, value, Some(DataType::Int), pos));
    }
    
    let section = DataSection::new(entries, pos);
    
    // Measure Display/ToString performance
    let (result, duration) = measure_time("AST Display", || {
        section.to_string()
    });
    
    println!("Display performance:");
    println!("  Output size: {} bytes", result.len());
    println!("  Time: {:?}", duration);
    println!("  Throughput: {:.2} MB/sec", result.len() as f64 / duration.as_secs_f64() / 1_000_000.0);
    
    // Should format at least 1 MB/sec
    let mb_per_sec = result.len() as f64 / duration.as_secs_f64() / 1_000_000.0;
    assert!(mb_per_sec > 1.0, "Too slow: {:.2} MB/sec", mb_per_sec);
}

#[test]
fn test_ast_clone_performance() {
    let pos = Position::START;
    
    // Create AST to clone
    let mut entries = Vec::with_capacity(1000);
    for i in 0..1000 {
        entries.push(create_simple_property(
            format!("var_{}", i),
            create_int(i as i32, pos),
            Some(DataType::Int),
            pos,
        ));
    }
    
    let section = DataSection::new(entries, pos);
    
    // Measure clone performance
    let (_, duration) = measure_time("AST Clone", || {
        let _cloned = section.clone();
    });
    
    println!("Clone performance:");
    println!("  Nodes: 1000");
    println!("  Time: {:?}", duration);
    
    // Cloning should be fast (< 1ms for 1000 nodes)
    assert!(duration.as_millis() < 10, "Clone too slow: {:?}", duration);
}

// ==================== MEMORY TESTS ====================

#[test]
fn test_position_size() {
    use std::mem::size_of;
    
    let size = size_of::<Position>();
    println!("Position size: {} bytes", size);
    
    // Position should be small (2 usizes = 16 bytes on 64-bit)
    assert_eq!(size, 16);
}

#[test]
fn test_datatype_size() {
    use std::mem::size_of;
    
    let size = size_of::<DataType>();
    println!("DataType size: {} bytes", size);
    
    // Enum should be 1 byte + padding
    assert!(size <= 8, "DataType too large: {} bytes", size);
}

#[test]
fn test_value_size() {
    use std::mem::size_of;
    
    let size = size_of::<Value>();
    println!("Value size: {} bytes", size);
    
    // Value is a large enum - size depends on largest variant
    // Should be reasonable (< 256 bytes)
    assert!(size < 256, "Value too large: {} bytes", size);
}

#[test]
fn test_expression_size() {
    use std::mem::size_of;
    
    let size = size_of::<Expression>();
    println!("Expression size: {} bytes", size);
    
    // Expression should be reasonable
    assert!(size < 256, "Expression too large: {} bytes", size);
}

// ==================== EQUALITY TESTS ====================

#[test]
fn test_position_equality() {
    let pos1 = Position::new(10, 5);
    let pos2 = Position::new(10, 5);
    let pos3 = Position::new(10, 6);
    
    assert_eq!(pos1, pos2);
    assert_ne!(pos1, pos3);
}

#[test]
fn test_value_equality() {
    let pos = Position::START;
    
    let val1 = create_int(42, pos);
    let val2 = create_int(42, pos);
    let val3 = create_int(43, pos);
    
    assert_eq!(val1, val2);
    assert_ne!(val1, val3);
}

#[test]
fn test_dixscript_equality() {
    let ast1 = DixScript::new();
    let ast2 = DixScript::new();
    
    assert_eq!(ast1, ast2);
}

// ==================== EDGE CASES ====================

#[test]
fn test_large_string_value() {
    let large_string = "x".repeat(10000);
    let val = create_string(large_string.clone(), Position::START);
    
    match val {
        Value::String { value, .. } => {
            assert_eq!(value.len(), 10000);
        }
        _ => panic!("Expected String value"),
    }
}

#[test]
fn test_deep_nesting() {
    let pos = Position::START;
    
    // Create deeply nested expressions: ((((x + 1) + 1) + 1) + 1)
    let mut expr = create_identifier("x".to_string(), pos);
    
    for _ in 0..100 {
        let one = create_identifier("1".to_string(), pos);
        expr = create_arithmetic(expr, "+".to_string(), one, pos);
    }
    
    // Should handle deep nesting without stack overflow
    let _result = expr.to_string();
}

#[test]
fn test_many_children() {
    let pos = Position::START;
    
    // Create array with many elements
    let mut values = Vec::with_capacity(1000);
    for i in 0..1000 {
        values.push(create_int(i, pos));
    }
    
    let arr = create_array(values, pos);
    let arr_str = arr.to_string();
    
    // Should handle many children efficiently
    assert!(arr_str.contains("["));
    assert!(arr_str.contains("]"));
}

#[test]
fn test_unicode_in_strings() {
    let val = create_string("Hello 世界 🦀".to_string(), Position::START);
    let val_str = val.to_string();
    
    assert!(val_str.contains("世界"));
    assert!(val_str.contains("🦀"));
}

// ==================== COMPREHENSIVE TEST ====================

#[test]
fn test_build_complete_ast() {
    let pos = Position::START;
    
    let (_, duration) = measure_time("Complete AST construction", || {
        // Config section
        let config = ConfigSection::new(vec![
            create_config_entry("version".to_string(), create_config_string("1.0.0".to_string()), pos),
            create_config_entry("features".to_string(), create_config_string("advanced".to_string()), pos),
        ], pos);
        
        // Enums section
        let enum_section = EnumsSection::new(vec![
            create_enum("Status".to_string(), vec![
                create_enum_field("ACTIVE".to_string(), Some(1), pos),
                create_enum_field("INACTIVE".to_string(), Some(2), pos),
            ], pos),
        ], pos);
        
        // Data section with various types
        let data = DataSection::new(vec![
            create_simple_property("int_val".to_string(), create_int(42, pos), Some(DataType::Int), pos),
            create_simple_property("float_val".to_string(), create_float(3.14, pos), Some(DataType::Float), pos),
            create_simple_property("str_val".to_string(), create_string("test".to_string(), pos), Some(DataType::String), pos),
            create_simple_property("bool_val".to_string(), create_bool(true, pos), Some(DataType::Bool), pos),
            create_simple_property("arr_val".to_string(), create_array(vec![
                create_int(1, pos),
                create_int(2, pos),
                create_int(3, pos),
            ], pos), None, pos),
        ], pos);
        
        let _ast = DixScript::with_sections(
            Some(config),
            None,
            None,
            Some(enum_section),
            None,
            Some(data),
            None,
        );
    });
    
    println!("\nComplete AST built in {:?}", duration);
    
    // Should build complete AST quickly
    assert!(duration.as_millis() < 10);
}
