//! Backs `mdix audit` by wrapping `dixscript::Compiler::DLM::Auditor::AuditFileManager`.
//!
//! Read-only from the CLI's side — `.mdix.au` files are written exclusively
//! by DiyAuditor/EnhancedAuditor during `compile_with_dlm`, locked read-only
//! at the OS level between writes (see Compiler/Utilities/file_permissions.rs).
//! There's no `mdix audit generate`/`edit` for the same reason there's no
//! `mdix key encrypt` — this is a compiler-managed artifact you inspect, not
//! one you hand-author.

use std::path::{Path, PathBuf};

use dixscript::Compiler::DLM::Auditor::{AuditEntryRecord, AuditFileData, AuditFileManager};

use crate::commands::CliError;

pub struct AuditSummary {
    pub path: String,
    pub source_file: String,
    pub format: String,
    pub max_entries: usize,
    pub created: String,
    pub entry_count: usize,
    pub latest_status: Option<String>,
    pub latest_timestamp: Option<String>,
}

/// `.mdix.au` files rotate to `<path>.archive_<timestamp>` once `max_entries`
/// is hit (see AuditFileManager::rotate_if_needed) — surfacing these means
/// `mdix audit info` doesn't quietly hide older history that's still on disk.
pub fn find_archives(audit_file_path: &str) -> Vec<String> {
    let path = Path::new(audit_file_path);
    let (Some(dir), Some(file_name)) = (path.parent(), path.file_name().and_then(|f| f.to_str()))
    else {
        return Vec::new();
    };

    let prefix = format!("{file_name}.archive_");
    let Ok(read_dir) = std::fs::read_dir(if dir.as_os_str().is_empty() { Path::new(".") } else { dir })
    else {
        return Vec::new();
    };

    let mut archives: Vec<String> = read_dir
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) {
                Some(dir.join(&name).to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();
    archives.sort();
    archives
}

fn load(audit_file_path: &str) -> Result<AuditFileData, CliError> {
    let path = PathBuf::from(audit_file_path);
    if !path.exists() {
        return Err(CliError::FileNotFound(path));
    }

    let manager = AuditFileManager::new(audit_file_path.to_string(), usize::MAX);

    // `load()` returns None for "doesn't exist" AND "couldn't parse" alike —
    // we've already ruled out the first, so a None here specifically means
    // the file exists but isn't a well-formed audit file.
    manager.load().ok_or_else(|| {
        CliError::ParseError(format!(
            "'{}' exists but isn't a valid .mdix.au audit file",
            audit_file_path
        ))
    })
}

pub fn get_summary(audit_file_path: &str) -> Result<AuditSummary, CliError> {
    let data = load(audit_file_path)?;
    let latest = data.entries.last();

    Ok(AuditSummary {
        path: audit_file_path.to_string(),
        source_file: data.config.source_file,
        format: data.config.format,
        max_entries: data.config.max_entries,
        created: data.config.created.to_rfc3339(),
        entry_count: data.entries.len(),
        latest_status: latest.map(|e| e.status.clone()),
        latest_timestamp: latest.map(|e| e.timestamp.to_rfc3339()),
    })
}

/// Full entry list, optionally limited to the most recent `tail` entries.
pub fn get_entries(audit_file_path: &str, tail: Option<usize>) -> Result<Vec<AuditEntryRecord>, CliError> {
    let data = load(audit_file_path)?;
    match tail {
        Some(n) if n < data.entries.len() => {
            Ok(data.entries[data.entries.len() - n..].to_vec())
        }
        _ => Ok(data.entries),
    }
}
