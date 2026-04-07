
//! In-memory data model for `.mdix.au` audit files.
//! Mirrors the role of KeyFileData for .mdix.key files.

use chrono::{DateTime, Utc};

/// Complete audit file — canonical in-memory representation.
#[derive(Debug, Clone)]
pub struct AuditFileData {
    pub config:  AuditFileConfig,
    pub entries: Vec<AuditEntryRecord>,
}

impl AuditFileData {
    pub fn new(config: AuditFileConfig) -> Self {
        AuditFileData { config, entries: Vec::new() }
    }
}

impl Default for AuditFileData {
    fn default() -> Self {
        Self::new(AuditFileConfig::default())
    }
}

/// Configuration header written once at the top of each `.mdix.au` file.
#[derive(Debug, Clone)]
pub struct AuditFileConfig {
    pub source_file: String,
    pub max_entries: usize,
    pub format:      String,
    pub created:     DateTime<Utc>,
}

impl AuditFileConfig {
    pub fn new(source_file: String, max_entries: usize) -> Self {
        AuditFileConfig {
            source_file,
            max_entries,
            format:  "structured".to_string(),
            created: Utc::now(),
        }
    }
}

impl Default for AuditFileConfig {
    fn default() -> Self {
        Self::new(String::new(), 100)
    }
}

/// A single compilation record appended to the audit file.
#[derive(Debug, Clone)]
pub struct AuditEntryRecord {
    /// 1-based index within this audit file (assigned by AuditFileManager).
    pub index:             usize,
    pub compilation_id:    String,
    pub timestamp:         DateTime<Utc>,
    pub source_checksum:   String,
    /// "SUCCESS" | "FAILED" | "WARNING"
    pub status:            String,
    pub modules_executed:  Vec<String>,
    pub execution_time_ms: f64,
    pub changes_summary:   Option<String>,
}

impl AuditEntryRecord {
    pub fn new() -> Self {
        AuditEntryRecord {
            index:             0,
            compilation_id:    String::new(),
            timestamp:         Utc::now(),
            source_checksum:   String::new(),
            status:            "SUCCESS".to_string(),
            modules_executed:  Vec::new(),
            execution_time_ms: 0.0,
            changes_summary:   None,
        }
    }
}

impl Default for AuditEntryRecord {
    fn default() -> Self { Self::new() }
      }
