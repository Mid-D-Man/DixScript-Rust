// src/Builtins/Static/dix_object.rs
//! Dix static object - Core DixScript utilities
//! Provides logging, assertions, and runtime utilities

use crate::Builtins::Core::{BuiltinMethod, DixType, DixValue, IBuiltinMethod};
use crate::Builtins::Static::{IStaticObject, StaticObjectBase};
use crate::ErrorManager::ErrorManager;

/// Dix static object implementation
pub struct DixObject {
    base: StaticObjectBase,
}

impl DixObject {
    pub fn new() -> Self {
        let mut base = StaticObjectBase::new("Dix".to_string());
        Self::initialize_methods(&mut base);
        DixObject { base }
    }

    fn initialize_methods(base: &mut StaticObjectBase) {
        // Dix.Log(message) - Basic logging (maps to Info level)
        base.register_method(Box::new(BuiltinMethod::new(
            "Log".to_string(),
            1,
            DixType::Void,
            |args| {
                let message = convert_to_log_message(&args[0]);
                ErrorManager::get_shared_instance().log_info(&message);
                Ok(DixValue::null())
            },
            "Logs an informational message".to_string(),
        )));

        // Dix.LogInfo(message) - Info level logging
        base.register_method(Box::new(BuiltinMethod::new(
            "LogInfo".to_string(),
            1,
            DixType::Void,
            |args| {
                let message = convert_to_log_message(&args[0]);
                ErrorManager::get_shared_instance().log_info(&message);
                Ok(DixValue::null())
            },
            "Logs an informational message".to_string(),
        )));

        // Dix.LogWarning(message) - Warning level logging
        base.register_method(Box::new(BuiltinMethod::new(
            "LogWarning".to_string(),
            1,
            DixType::Void,
            |args| {
                let message = convert_to_log_message(&args[0]);
                ErrorManager::get_shared_instance().log_Warning(&message);
                Ok(DixValue::null())
            },
            "Logs a warning message".to_string(),
        )));

        // Dix.LogError(message) - Error level logging
        base.register_method(Box::new(BuiltinMethod::new(
            "LogError".to_string(),
            1,
            DixType::Void,
            |args| {
                let message = convert_to_log_message(&args[0]);
                ErrorManager::get_shared_instance().log_error(&message);
                Ok(DixValue::null())
            },
            "Logs an error message".to_string(),
        )));

        // Dix.LogDebug(message) - Debug level logging
        base.register_method(Box::new(BuiltinMethod::new(
            "LogDebug".to_string(),
            1,
            DixType::Void,
            |args| {
                let message = convert_to_log_message(&args[0]);
                ErrorManager::get_shared_instance().log_debug(&message);
                Ok(DixValue::null())
            },
            "Logs a debug message (only shown when debug mode is enabled)".to_string(),
        )));

        // Dix.LogVerbose(message) - Verbose level logging
        base.register_method(Box::new(BuiltinMethod::new(
            "LogVerbose".to_string(),
            1,
            DixType::Void,
            |args| {
                let message = convert_to_log_message(&args[0]);
                ErrorManager::get_shared_instance().log_debug(&format!("[VERBOSE] {}", message));
                Ok(DixValue::null())
            },
            "Logs a verbose debug message (only shown when verbose debug mode is enabled)"
                .to_string(),
        )));

        // Dix.Assert(condition, message) - Runtime assertion
        base.register_method(Box::new(BuiltinMethod::new(
            "Assert".to_string(),
            2,
            DixType::Void,
            |args| {
                let condition = args[0].as_bool();
                let message = convert_to_log_message(&args[1]);

                if !condition {
                    let assert_message = format!("Assertion failed: {}", message);
                    ErrorManager::get_shared_instance().log_error(&assert_message);
                    return Err(assert_message);
                }

                Ok(DixValue::null())
            },
            "Asserts that a condition is true, throws error if false".to_string(),
        )));

        // Dix.Trace(message, context) - Trace logging with optional context
        base.register_method(Box::new(BuiltinMethod::new(
            "Trace".to_string(),
            2,
            DixType::Void,
            |args| {
                let message = convert_to_log_message(&args[0]);
                let context = convert_to_log_message(&args[1]);

                let trace_message = if context.is_empty() {
                    format!("[TRACE] {}", message)
                } else {
                    format!("[TRACE:{}] {}", context, message)
                };

                ErrorManager::get_shared_instance().log_debug(&trace_message);
                Ok(DixValue::null())
            },
            "Logs a trace message with optional context identifier".to_string(),
        )));

        // Dix.Print(message) - Console output
        base.register_method(Box::new(BuiltinMethod::new(
            "Print".to_string(),
            1,
            DixType::Void,
            |args| {
                let message = convert_to_log_message(&args[0]);
                println!("{}", message);
                Ok(DixValue::null())
            },
            "Prints a message directly to console output".to_string(),
        )));

        // Dix.PrintLine(message) - Same as Print
        base.register_method(Box::new(BuiltinMethod::new(
            "PrintLine".to_string(),
            1,
            DixType::Void,
            |args| {
                let message = convert_to_log_message(&args[0]);
                println!("{}", message);
                Ok(DixValue::null())
            },
            "Prints a message to console with newline".to_string(),
        )));

        // Dix.Format(format, ...args) - String formatting
        base.register_method(Box::new(BuiltinMethod::new_variadic(
            "Format".to_string(),
            1,
            DixType::String,
            |args| {
                if args.is_empty() {
                    return Ok(DixValue::from_string(String::new()));
                }

                let format_str = args[0].as_string();

                if args.len() == 1 {
                    return Ok(DixValue::from_string(format_str));
                }

                let format_args: Vec<String> = args[1..]
                    .iter()
                    .map(convert_to_format_arg)
                    .collect();

                let mut result = format_str;
                for (i, arg) in format_args.iter().enumerate() {
                    let placeholder = format!("{{{}}}", i);
                    result = result.replace(&placeholder, arg);
                }

                Ok(DixValue::from_string(result))
            },
            "Formats a string using placeholders (e.g., 'Value: {0}', value)".to_string(),
        )));

        // Dix.Join(separator, ...values) - Join values
        base.register_method(Box::new(BuiltinMethod::new_variadic(
            "Join".to_string(),
            1,
            DixType::String,
            |args| {
                if args.is_empty() {
                    return Ok(DixValue::from_string(String::new()));
                }

                let separator = args[0].as_string();

                if args.len() == 1 {
                    return Ok(DixValue::from_string(String::new()));
                }

                let values: Vec<String> = args[1..]
                    .iter()
                    .map(convert_to_log_message)
                    .collect();

                Ok(DixValue::from_string(values.join(&separator)))
            },
            "Joins multiple values into a string with the specified separator".to_string(),
        )));
    }
}

impl Default for DixObject {
    fn default() -> Self {
        Self::new()
    }
}

impl IStaticObject for DixObject {
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

// ==================== HELPER FUNCTIONS ====================

fn convert_to_log_message(value: &DixValue) -> String {
    match value.get_type() {
        DixType::String => value.as_string(),
        DixType::Null => "null".to_string(),
        DixType::Bool => value.as_bool().to_string().to_lowercase(),
        DixType::Int => value.as_int().to_string(),
        DixType::Float => value.as_float().to_string(),
        DixType::Double => value.as_double().to_string(),
        DixType::Date => {
            let dt = value.as_datetime();
            dt.format("%Y-%m-%d").to_string()
        }
        DixType::Timestamp => {
            let dt = value.as_datetime();
            dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
        }
        DixType::Hex => value.as_string(),
        DixType::Enum => value.as_string(),
        DixType::Array => format_array(value),
        DixType::Tuple => format_tuple(value),
        DixType::Object => format_object(value),
        DixType::Regex => format!("r:({})", value.as_string()),
        DixType::Blob => {
            if let Ok(base64) = value.as_blob_base64() {
                format!("b:(<{} bytes>)", base64.len())
            } else {
                "b:(<invalid>)".to_string()
            }
        }
        _ => value.as_string(),
    }
}

fn convert_to_format_arg(value: &DixValue) -> String {
    match value.get_type() {
        DixType::Int => value.as_int().to_string(),
        DixType::Float => value.as_float().to_string(),
        DixType::Double => value.as_double().to_string(),
        DixType::Bool => value.as_bool().to_string().to_lowercase(),
        DixType::Date => {
            let dt = value.as_datetime();
            dt.format("%Y-%m-%d").to_string()
        }
        DixType::Timestamp => {
            let dt = value.as_datetime();
            dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
        }
        _ => convert_to_log_message(value),
    }
}

fn format_array(array: &DixValue) -> String {
    let items = array.as_array();
    if items.is_empty() {
        return "[]".to_string();
    }

    let mut result = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            result.push_str(", ");
        }
        if i >= 10 {
            result.push_str(&format!("... ({} more)", items.len() - 10));
            break;
        }
        result.push_str(&convert_to_log_message(item));
    }
    result.push(']');
    result
}

fn format_tuple(tuple: &DixValue) -> String {
    let items = tuple.as_array();
    if items.is_empty() {
        return "t:()".to_string();
    }

    let mut result = String::from("t:(");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            result.push_str(", ");
        }
        result.push_str(&convert_to_log_message(item));
    }
    result.push(')');
    result
}

fn format_object(obj: &DixValue) -> String {
    let props = obj.as_object();
    if props.is_empty() {
        return "{}".to_string();
    }

    let mut result = String::from("{ ");
    let mut index = 0;
    for (key, value) in props.iter() {
        if index > 0 {
            result.push_str(", ");
        }
        if index >= 10 {
            result.push_str(&format!("... ({} more)", props.len() - 10));
            break;
        }
        result.push_str(key);
        result.push_str(": ");
        result.push_str(&convert_to_log_message(value));
        index += 1;
    }
    result.push_str(" }");
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dix_object_creation() {
        let dix = DixObject::new();
        assert_eq!(dix.name(), "Dix");
        assert!(!dix.get_method_names().is_empty());
    }

    #[test]
    fn test_has_methods() {
        let dix = DixObject::new();
        assert!(dix.has_method("Log"));
        assert!(dix.has_method("Format"));
        assert!(dix.has_method("Join"));
    }
}