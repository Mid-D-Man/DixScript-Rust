//! Result structures for DLM pipeline execution

use std::collections::HashMap;
use std::time::Duration;

/// Result of DLM forward pipeline execution (compilation)
#[derive(Debug, Clone)]
pub struct DLMPipelineResult {
    pub is_success: bool,
    pub processed_data: Vec<u8>,
    pub metadata: HashMap<String, HashMap<String, String>>,
    pub executed_modules: Vec<String>,
    pub total_duration: Duration,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    
    // File paths generated
    pub encrypted_file_path: Option<String>,
    pub key_file_path: Option<String>,
    pub audit_file_path: Option<String>,
    
    // Statistics
    pub original_size: usize,
    pub processed_size: usize,
    pub compression_ratio: f64,
}

impl DLMPipelineResult {
    pub fn new(original_size: usize) -> Self {
        DLMPipelineResult {
            is_success: false,
            processed_data: Vec::new(),
            metadata: HashMap::new(),
            executed_modules: Vec::new(),
            total_duration: Duration::ZERO,
            errors: Vec::new(),
            warnings: Vec::new(),
            encrypted_file_path: None,
            key_file_path: None,
            audit_file_path: None,
            original_size,
            processed_size: 0,
            compression_ratio: 0.0,
        }
    }
}

impl std::fmt::Display for DLMPipelineResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.is_success { "SUCCESS" } else { "FAILED" };
        write!(
            f,
            "DLM Pipeline {}: Executed {} modules, Original: {} bytes, Processed: {} bytes, Duration: {:.2}ms",
            status,
            self.executed_modules.len(),
            self.original_size,
            self.processed_size,
            self.total_duration.as_millis()
        )
    }
}

/// Result of DLM reverse pipeline execution (decryption/decompression)
#[derive(Debug, Clone)]
pub struct DLMReverseResult {
    pub is_success: bool,
    pub restored_data: Vec<u8>,
    pub metadata: HashMap<String, HashMap<String, String>>,
    pub executed_modules: Vec<String>,
    pub total_duration: Duration,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    
    pub encrypted_size: usize,
    pub restored_size: usize,
}

impl DLMReverseResult {
    pub fn new(encrypted_size: usize) -> Self {
        DLMReverseResult {
            is_success: false,
            restored_data: Vec::new(),
            metadata: HashMap::new(),
            executed_modules: Vec::new(),
            total_duration: Duration::ZERO,
            errors: Vec::new(),
            warnings: Vec::new(),
            encrypted_size,
            restored_size: 0,
        }
    }
}

impl std::fmt::Display for DLMReverseResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.is_success { "SUCCESS" } else { "FAILED" };
        write!(
            f,
            "DLM Reverse Pipeline {}: Executed {} modules, Encrypted: {} bytes, Restored: {} bytes, Duration: {:.2}ms",
            status,
            self.executed_modules.len(),
            self.encrypted_size,
            self.restored_size,
            self.total_duration.as_millis()
        )
    }
          }
