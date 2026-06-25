//! Post-resolution numeric array homogenization.
//!
//! QuickFunc value resolution can produce array literals whose elements
//! retain whatever literal type their *source expression* had — so
//! `[12.3, someIntExpr(), 4.9]` can end up as
//! `[Double(12.3), Integer(4), Double(4.9)]` even though the array is
//! semantically "an array of doubles".
//!
//! This pass walks every array / nested-array / group-array in the `@DATA`
//! section and, when **every** element is a numeric literal but they don't
//! all share the same rank, promotes the lower-rank elements up to the
//! highest rank present:
//!
//! ```text
//! Integer < Long < Float < (Double | ScientificNotation)
//! ```
//!
//! Arrays containing any non-numeric element (strings, objects, enums, ...)
//! are left completely untouched — this only fires when the array is
//! homogeneously numeric except for the int/float distinction.
//!
//! Tuples (`t:(...)`) are intentionally skipped at the sibling level — they
//! are heterogeneous by design — but their *individual* elements are still
//! recursed into in case one of them is itself a numeric array.

use crate::Compiler::AST::{DixScript, DataEntry, Value, Position};

/// Entry point — call once on a fully-resolved AST, before handing it to
/// the binary serializer / converters / "Create Resolved" output.
pub fn homogenize_data_section(ast: &mut DixScript) {
    let data = match ast.data.as_mut() {
        Some(d) => d,
        None => return,
    };

    for entry in data.entries.iter_mut() {
        match entry {
            DataEntry::SimpleProperty { value, .. } => homogenize_value(value),
            DataEntry::TableProperty { properties, .. } => {
                for prop in properties.iter_mut() {
                    homogenize_value(&mut prop.value);
                }
            }
            DataEntry::GroupArray { items, .. } => {
                for item in items.iter_mut() {
                    homogenize_value(item);
                }
                homogenize_numeric_siblings(items);
            }
            DataEntry::ObjectProperty { object, .. } => homogenize_value(object),
        }
    }
}

fn homogenize_value(value: &mut Value) {
    match value {
        Value::Array { values, .. } | Value::NestedArray { values, .. } => {
            for v in values.iter_mut() {
                homogenize_value(v);
            }
            homogenize_numeric_siblings(values);
        }
        Value::Object { properties, .. } => {
            for prop in properties.iter_mut() {
                homogenize_value(&mut prop.value);
            }
        }
        Value::PrefixedConstructor { arguments, .. } => {
            // Tuples are heterogeneous by design — recurse into each
            // element independently, but never homogenize across siblings.
            for arg in arguments.iter_mut() {
                homogenize_value(arg);
            }
        }
        Value::Range { start, end, .. } => {
            homogenize_value(start);
            homogenize_value(end);
        }
        _ => {}
    }
}

/// Numeric "rank" used to pick the common promoted type.
/// `None` means "not a numeric literal" — if ANY element in an array is
/// non-numeric, the whole array is left alone.
#[inline]
fn numeric_rank(value: &Value) -> Option<u8> {
    match value {
        Value::Integer { .. }            => Some(0),
        Value::Long { .. }               => Some(1),
        Value::Float { .. }              => Some(2),
        Value::Double { .. }             => Some(3),
        Value::ScientificNotation { .. } => Some(3),
        _ => None,
    }
}

fn promote(value: Value, target_rank: u8) -> Value {
    match (target_rank, value) {
        (3, Value::Integer { value: v, position }) => Value::Double { value: v as f64, position },
        (3, Value::Long    { value: v, position }) => Value::Double { value: v as f64, position },
        (3, Value::Float   { value: v, position }) => Value::Double { value: v as f64, position },
        (2, Value::Integer { value: v, position }) => Value::Float { value: v as f32, position },
        (2, Value::Long    { value: v, position }) => Value::Float { value: v as f32, position },
        (1, Value::Integer { value: v, position }) => Value::Long { value: v as i64, position },
        (_, other) => other,
    }
}

/// If every element of `values` is numeric and they don't all share the
/// same rank, promote the lower-rank elements up to the highest rank
/// present. No-op for arrays with fewer than 2 elements, all-Integer
/// arrays, or arrays containing any non-numeric element.
fn homogenize_numeric_siblings(values: &mut [Value]) {
    if values.len() < 2 {
        return;
    }

    let mut max_rank: u8 = 0;
    for v in values.iter() {
        match numeric_rank(v) {
            Some(r) => max_rank = max_rank.max(r),
            None    => return,
        }
    }

    if max_rank == 0 {
        return; // all plain Integer — nothing to promote
    }

    for v in values.iter_mut() {
        let current = numeric_rank(v).unwrap();
        if current < max_rank {
            let owned = std::mem::replace(v, Value::Null { position: Position::UNKNOWN });
            *v = promote(owned, max_rank);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler::AST::{DataSection, TablePath, PropertyAssignment, ObjectProperty};

    fn int(v: i32) -> Value { Value::Integer { value: v, position: Position::UNKNOWN } }
    fn dbl(v: f64) -> Value { Value::Double { value: v, position: Position::UNKNOWN } }
    fn long(v: i64) -> Value { Value::Long { value: v, position: Position::UNKNOWN } }
    fn s(v: &str) -> Value { Value::String { value: v.into(), position: Position::UNKNOWN } }

    fn ast_with(entries: Vec<DataEntry>) -> DixScript {
        DixScript {
            data: Some(DataSection { entries, position: Position::UNKNOWN }),
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None,
        }
    }

    #[test]
    fn promotes_int_to_double_when_sibling_is_double() {
        let mut ast = ast_with(vec![DataEntry::SimpleProperty {
            name: "values".into(), data_type: None,
            value: Value::Array {
                values: vec![dbl(12.3), int(4), dbl(4.9)],
                position: Position::UNKNOWN,
            },
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::SimpleProperty { value: Value::Array { values, .. }, .. }) =
            ast.data.as_ref().unwrap().entries.first()
        {
            assert!(matches!(values[0], Value::Double { value, .. } if (value - 12.3).abs() < 1e-9));
            assert!(matches!(values[1], Value::Double { value, .. } if value == 4.0));
            assert!(matches!(values[2], Value::Double { value, .. } if (value - 4.9).abs() < 1e-9));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn leaves_all_int_array_untouched() {
        let mut ast = ast_with(vec![DataEntry::SimpleProperty {
            name: "ids".into(), data_type: None,
            value: Value::Array { values: vec![int(1), int(2), int(3)], position: Position::UNKNOWN },
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::SimpleProperty { value: Value::Array { values, .. }, .. }) =
            ast.data.as_ref().unwrap().entries.first()
        {
            assert!(values.iter().all(|v| matches!(v, Value::Integer { .. })));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn leaves_mixed_type_array_untouched() {
        let mut ast = ast_with(vec![DataEntry::SimpleProperty {
            name: "mixed".into(), data_type: None,
            value: Value::Array { values: vec![int(1), s("two"), dbl(3.0)], position: Position::UNKNOWN },
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::SimpleProperty { value: Value::Array { values, .. }, .. }) =
            ast.data.as_ref().unwrap().entries.first()
        {
            assert!(matches!(values[0], Value::Integer { .. }));
            assert!(matches!(values[1], Value::String { .. }));
            assert!(matches!(values[2], Value::Double { .. }));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn promotes_long_over_int_without_any_float() {
        let mut ast = ast_with(vec![DataEntry::SimpleProperty {
            name: "ids".into(), data_type: None,
            value: Value::Array { values: vec![int(1), long(9_000_000_000), int(3)], position: Position::UNKNOWN },
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::SimpleProperty { value: Value::Array { values, .. }, .. }) =
            ast.data.as_ref().unwrap().entries.first()
        {
            assert!(matches!(values[0], Value::Long { value, .. } if value == 1));
            assert!(matches!(values[1], Value::Long { value, .. } if value == 9_000_000_000));
            assert!(matches!(values[2], Value::Long { value, .. } if value == 3));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn recurses_into_nested_arrays_independently() {
        let inner_a = Value::Array { values: vec![int(1), int(2)], position: Position::UNKNOWN };
        let inner_b = Value::Array { values: vec![dbl(1.5), int(2)], position: Position::UNKNOWN };

        let mut ast = ast_with(vec![DataEntry::SimpleProperty {
            name: "matrix".into(), data_type: None,
            value: Value::NestedArray { values: vec![inner_a, inner_b], level: 2, position: Position::UNKNOWN },
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::SimpleProperty { value: Value::NestedArray { values, .. }, .. }) =
            ast.data.as_ref().unwrap().entries.first()
        {
            if let Value::Array { values: a, .. } = &values[0] {
                assert!(a.iter().all(|v| matches!(v, Value::Integer { .. })));
            } else { panic!("expected array"); }

            if let Value::Array { values: b, .. } = &values[1] {
                assert!(b.iter().all(|v| matches!(v, Value::Double { .. })));
            } else { panic!("expected array"); }
        } else {
            panic!("expected nested array");
        }
    }

    #[test]
    fn homogenizes_group_array_entry() {
        let mut ast = ast_with(vec![DataEntry::GroupArray {
            path: TablePath::new(vec!["scores".into()]),
            items: vec![int(10), dbl(9.5), int(8)],
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::GroupArray { items, .. }) = ast.data.as_ref().unwrap().entries.first() {
            assert!(items.iter().all(|v| matches!(v, Value::Double { .. })));
        } else {
            panic!("expected group array");
        }
    }

    #[test]
    fn homogenizes_table_property_values() {
        let mut ast = ast_with(vec![DataEntry::TableProperty {
            path: TablePath::new(vec!["stats".into()]),
            properties: vec![PropertyAssignment::new(
                "weights".into(), None,
                Value::Array { values: vec![int(1), dbl(2.5)], position: Position::UNKNOWN },
                Position::UNKNOWN,
            )],
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::TableProperty { properties, .. }) = ast.data.as_ref().unwrap().entries.first() {
            if let Value::Array { values, .. } = &properties[0].value {
                assert!(values.iter().all(|v| matches!(v, Value::Double { .. })));
            } else { panic!("expected array"); }
        } else {
            panic!("expected table property");
        }
    }

    #[test]
    fn homogenizes_object_property_field_arrays() {
        let mut ast = ast_with(vec![DataEntry::ObjectProperty {
            name: "cfg".into(), data_type: None,
            object: Box::new(Value::Object {
                properties: vec![ObjectProperty::new(
                    "ratios".into(),
                    Value::Array { values: vec![int(1), dbl(0.5)], position: Position::UNKNOWN },
                    Position::UNKNOWN,
                )],
                position: Position::UNKNOWN,
            }),
            position: Position::UNKNOWN,
        }]);

        homogenize_data_section(&mut ast);

        if let Some(DataEntry::ObjectProperty { object, .. }) = ast.data.as_ref().unwrap().entries.first() {
            if let Value::Object { properties, .. } = object.as_ref() {
                if let Value::Array { values, .. } = &properties[0].value {
                    assert!(values.iter().all(|v| matches!(v, Value::Double { .. })));
                } else { panic!("expected array"); }
            } else { panic!("expected object"); }
        } else {
            panic!("expected object property");
        }
    }
          }
