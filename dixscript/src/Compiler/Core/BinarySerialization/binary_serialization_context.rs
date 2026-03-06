//! Maintains state during binary serialization/deserialization

use std::collections::HashMap;
use crate::ErrorManager::{ErrorManager, DebugConfig};
use crate::ErrorManager::ErrorTypes::BinarySerializationErrorType;
use super::binary_format::{
    MAX_NESTING_DEPTH, MAX_STRING_LENGTH, MAX_ARRAY_LENGTH,
    MAX_OBJECT_PROPERTIES, ValueTypeTag, SectionId,
};

/// Tracks state during binary serialization/deserialization.
pub struct BinarySerializationContext {
    pub error_manager: ErrorManager,
    /// Cached at construction from the shared ErrorManager's debug mode.
    /// All log-gate decisions flow through this — never read debug_mode directly.
    pub debug_config: DebugConfig,
    scope_stack: Vec<String>,
    current_nesting_depth: usize,
    pub statistics: BinarySerializationStatistics,
}

impl BinarySerializationContext {
    pub fn new() -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let debug_mode = error_manager.get_debug_mode();
        let debug_config = DebugConfig::from_debug_mode(debug_mode);

        BinarySerializationContext {
            error_manager,
            debug_config,
            scope_stack: Vec::new(),
            current_nesting_depth: 0,
            statistics: BinarySerializationStatistics::new(),
        }
    }

    /// Merge statistics collected by a parallel section task into this context.
    pub fn merge_statistics(&mut self, other: BinarySerializationStatistics) {
        for (k, v) in other.value_counts {
            *self.statistics.value_counts.entry(k).or_insert(0) += v;
        }
        for (k, v) in other.section_sizes {
            self.statistics.section_sizes.insert(k, v);
        }
        self.statistics.total_values += other.total_values;
    }

    pub fn enter_nested(&mut self, structure_type: &str) -> Result<(), String> {
        self.current_nesting_depth += 1;
        self.scope_stack.push(structure_type.to_string());
        if self.current_nesting_depth > MAX_NESTING_DEPTH {
            return Err(format!(
                "Nesting depth {} exceeds maximum {} at {}",
                self.current_nesting_depth,
                MAX_NESTING_DEPTH,
                self.get_current_scope()
            ));
        }
        Ok(())
    }

    pub fn exit_nested(&mut self) -> Result<(), String> {
        if self.current_nesting_depth == 0 {
            return Err("Cannot exit - not in nested structure".to_string());
        }
        self.scope_stack.pop();
        self.current_nesting_depth -= 1;
        Ok(())
    }

    pub fn can_enter_nested(&self) -> bool {
        self.current_nesting_depth < MAX_NESTING_DEPTH
    }

    pub fn nesting_depth(&self) -> usize {
        self.current_nesting_depth
    }

    pub fn validate_string_length(&self, length: usize) -> Result<(), String> {
        if length > MAX_STRING_LENGTH {
            return Err(format!(
                "String length {} exceeds maximum {} at {}",
                length,
                MAX_STRING_LENGTH,
                self.get_current_scope()
            ));
        }
        Ok(())
    }

    pub fn validate_array_length(&self, count: usize) -> Result<(), String> {
        if count > MAX_ARRAY_LENGTH {
            return Err(format!(
                "Array count {} exceeds maximum {} at {}",
                count,
                MAX_ARRAY_LENGTH,
                self.get_current_scope()
            ));
        }
        Ok(())
    }

    pub fn validate_object_property_count(&self, count: usize) -> Result<(), String> {
        if count > MAX_OBJECT_PROPERTIES {
            return Err(format!(
                "Object property count {} exceeds maximum {} at {}",
                count,
                MAX_OBJECT_PROPERTIES,
                self.get_current_scope()
            ));
        }
        Ok(())
    }

    pub fn get_current_scope(&self) -> String {
        if self.scope_stack.is_empty() {
            "Root".to_string()
        } else {
            self.scope_stack.join(" > ")
        }
    }

    /// Report a non-fatal error to the shared ErrorManager.
    pub fn add_error(&self, error_type: BinarySerializationErrorType, message: String) {
        self.error_manager.add_binary_serialization_error(
            error_type,
            message,
            None,
            None,
            None,
            None,
        );
    }

    /// Log only when debug is enabled. Caller is responsible for gating
    /// any format!() calls in hot paths before calling this.
    pub fn log_debug(&self, message: &str) {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(message);
        }
    }

    pub fn log_info(&self, message: &str) {
        if self.debug_config.is_enabled {
            self.error_manager
                .log_info(&format!("[BinarySerialization] {}", message));
        }
    }

    pub fn log_verbose(&self, message: &str) {
        if self.debug_config.is_verbose {
            self.error_manager
                .log_debug(&format!("[BinarySerialization] {}", message));
        }
    }
}

impl Default for BinarySerializationContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics collected during a serialization run.
#[derive(Debug, Clone)]
pub struct BinarySerializationStatistics {
    pub total_sections: usize,
    pub total_values: usize,
    pub total_bytes: usize,
    pub value_counts: HashMap<String, usize>,
    pub section_sizes: HashMap<String, usize>,
}

impl BinarySerializationStatistics {
    pub fn new() -> Self {
        BinarySerializationStatistics {
            total_sections: 0,
            total_values: 0,
            total_bytes: 0,
            value_counts: HashMap::new(),
            section_sizes: HashMap::new(),
        }
    }

    pub fn increment_value_count(&mut self, type_tag: ValueTypeTag) {
        let type_name = type_tag.name().to_string();
        *self.value_counts.entry(type_name).or_insert(0) += 1;
        self.total_values += 1;
    }

    pub fn record_section_size(&mut self, section_id: SectionId, size: usize) {
        self.section_sizes.insert(section_id.name().to_string(), size);
    }
}

impl Default for BinarySerializationStatistics {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BinarySerializationStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Statistics: Sections={}, Values={}, TotalBytes={}",
            self.total_sections, self.total_values, self.total_bytes
        )
    }
}

/// Statistics collected during a deserialization run.
#[derive(Debug, Clone)]
pub struct BinaryDeserializationStatistics {
    pub total_sections: usize,
    pub total_values: usize,
    pub total_bytes: usize,
    pub value_counts: HashMap<String, usize>,
}

impl BinaryDeserializationStatistics {
    pub fn new() -> Self {
        BinaryDeserializationStatistics {
            total_sections: 0,
            total_values: 0,
            total_bytes: 0,
            value_counts: HashMap::new(),
        }
    }

    pub fn increment_value_count(&mut self, type_tag: ValueTypeTag) {
        let type_name = type_tag.name().to_string();
        *self.value_counts.entry(type_name).or_insert(0) += 1;
        self.total_values += 1;
    }
}

impl Default for BinaryDeserializationStatistics {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BinaryDeserializationStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Statistics: Sections={}, Values={}, TotalBytes={}",
            self.total_sections, self.total_values, self.total_bytes
        )
    }
            }
