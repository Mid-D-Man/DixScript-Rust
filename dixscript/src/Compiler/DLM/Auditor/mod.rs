
//! Auditor — compilation audit trail modules.

mod auditor_trait;
#[cfg(feature = "dlm-auditor")]
mod audit_file_data;
#[cfg(feature = "dlm-auditor")]
mod audit_file_format;
#[cfg(feature = "dlm-auditor")]
mod audit_file_manager;
#[cfg(feature = "dlm-auditor")]
mod auditor_utilities;
#[cfg(feature = "dlm-auditor")]
mod diy_auditor;
#[cfg(feature = "dlm-auditor")]
mod enhanced_auditor;

#[cfg(feature = "dlm-auditor")]
pub use audit_file_data::{AuditEntryRecord, AuditFileConfig, AuditFileData};
#[cfg(feature = "dlm-auditor")]
pub use audit_file_format::{AuditFileParser, AuditFileWriter};
#[cfg(feature = "dlm-auditor")]
pub use audit_file_manager::AuditFileManager;
pub use auditor_trait::{
    AuditChange, AuditEntry, AuditResult, AuditStep, AuditorResult, DecryptionAttempt, IAuditor,
};
#[cfg(feature = "dlm-auditor")]
pub use auditor_utilities::AuditorPathUtils;
#[cfg(feature = "dlm-auditor")]
pub use diy_auditor::DiyAuditor;
#[cfg(feature = "dlm-auditor")]
pub use enhanced_auditor::EnhancedAuditor;
