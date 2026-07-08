//! Key file manager — creates and reads `.mdix.key` files.
//!
//! Key files are locked read-only immediately after creation.
//! Recompilation unlocks, overwrites, then re-locks.

use super::key_file_data::*;
use super::key_file_format::{MdixKeyWriter, MdixKeyParser};
use crate::Compiler::DLM::dlm_module_base::DLMModuleBase;
use crate::Compiler::Utilities::file_permissions;
use crate::ErrorManager::{DlmErrorType, ErrorSeverity};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct KeyFileManager {
    base:             DLMModuleBase,
    source_file_path: String,
    output_directory: String,
}

impl KeyFileManager {
    pub fn new(source_file_path: String, output_directory: String) -> Self {
        KeyFileManager {
            base: DLMModuleBase::new("KeyFileManager", 0),
            source_file_path,
            output_directory,
        }
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Build the `.mdix.key` file's content and its intended path, without
    /// touching the filesystem at all. `create_key_file` below is now a
    /// thin wrapper: call this, then write the result to disk. Split out
    /// specifically so wasm32 callers (no real filesystem) can get the key
    /// content in memory the same way `result.processed_data` already
    /// carries the encrypted bytes in memory regardless of whether the
    /// `.mdix.enc` disk write succeeds.
    pub fn build_key_file_content(
        &self,
        compiled_file_path:   &str,
        compression_metadata: Option<HashMap<String, String>>,
        encryption_metadata:  Option<HashMap<String, String>>,
        audit_metadata:       Option<HashMap<String, String>>,
        sizes:                (usize, usize, usize),
    ) -> Result<(String, String), String> {
        if self.base.is_debug_enabled() {
            self.base.log_debug("Building .mdix.key file content");
        }

        let (original_size, compressed_size, encrypted_size) = sizes;
        let key_file_path = self.key_file_path(compiled_file_path);

        let mut data = KeyFileData::new();

        data.config.source_file        = Some(self.source_file_path.clone());
        data.file_info.source_file     = Some(self.source_file_path.clone());
        data.file_info.output_file     = Some(compiled_file_path.to_string());
        data.file_info.original_size   = original_size;
        data.file_info.compressed_size = compressed_size;
        data.file_info.encrypted_size  = encrypted_size;

        let is_password_mode = encryption_metadata
            .as_ref()
            .map(|m| m.contains_key("kdf_algorithm"))
            .unwrap_or(false);

        data.config.key_type = if is_password_mode {
            "password".to_string()
        } else {
            "keyfile".to_string()
        };

        let mut modules: Vec<String> = Vec::with_capacity(3);
        if let Some(ref am) = audit_metadata {
            if let Some(name) = am.get("module_name") { modules.push(name.clone()); }
        }
        if let Some(ref cm) = compression_metadata {
            if let Some(name) = cm.get("module_name") { modules.push(name.clone()); }
        }
        if let Some(ref em) = encryption_metadata {
            if let Some(name) = em.get("module_name") { modules.push(name.clone()); }
        }

        data.pipeline.modules_used   = modules.clone();
        data.pipeline.reversal_order = modules.into_iter().rev().collect();

        if let Some(ref em) = encryption_metadata {
            let algorithm      = em.get("algorithm").cloned().unwrap_or_default();
            let iv             = em.get("iv").cloned().unwrap_or_default();
            let security_level = em.get("security_level").cloned()
                .unwrap_or_else(|| "HIGH".to_string());
            let key_length     = em.get("key_length")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(32);

            let kdf = if em.contains_key("kdf_algorithm") {
                Some(KDFParameters {
                    algorithm:   em.get("kdf_algorithm").cloned()
                        .unwrap_or_else(|| "argon2id".to_string()),
                    kdf_version: em.get("kdf_version").cloned()
                        .unwrap_or_else(|| "1.3".to_string()),
                    memory:      em.get("kdf_memory").and_then(|v| v.parse().ok()).unwrap_or(65536),
                    iterations:  em.get("kdf_iterations").and_then(|v| v.parse().ok()).unwrap_or(3),
                    parallelism: em.get("kdf_parallelism").and_then(|v| v.parse().ok()).unwrap_or(4),
                    salt:        em.get("salt").cloned().unwrap_or_default(),
                    salt_length: em.get("salt_length").and_then(|v| v.parse().ok()).unwrap_or(32),
                })
            } else {
                None
            };

            data.key_data.encryption = Some(EncryptionKeyData {
                algorithm,
                key_length,
                security_level,
                key_data: em.get("key_data").cloned(),
                iv,
                kdf,
            });
        }

        if let Some(ref cm) = compression_metadata {
            data.key_data.compression = Some(CompressionKeyData {
                algorithm:         cm.get("algorithm").cloned().unwrap_or_default(),
                compression_level: cm.get("compression_level").cloned(),
                original_size,
                compressed_size,
            });
        }

        let content = MdixKeyWriter::write(&data);

        Ok((key_file_path, content))
    }

    /// Build and write the `.mdix.key` file from module metadata.
    ///
    /// If a key file already exists it is unlocked, overwritten, then re-locked.
    /// `sizes` is `(original_bytes, compressed_bytes, encrypted_bytes)`.
    pub fn create_key_file(
        &self,
        compiled_file_path:   &str,
        compression_metadata: Option<HashMap<String, String>>,
        encryption_metadata:  Option<HashMap<String, String>>,
        audit_metadata:       Option<HashMap<String, String>>,
        sizes:                (usize, usize, usize),
    ) -> Result<String, String> {
        let (key_file_path, content) = self.build_key_file_content(
            compiled_file_path,
            compression_metadata,
            encryption_metadata,
            audit_metadata,
            sizes,
        )?;

        if let Some(parent) = Path::new(&key_file_path).parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create key file directory: {}", e))?;
        }

        self.write_locked(&key_file_path, content.as_bytes())?;

        self.base.log_info(&format!("Key file written: {}", key_file_path));
        Ok(key_file_path)
    }

    /// Read and parse an existing `.mdix.key` file.
    pub fn read_key_file(&self, key_file_path: &str) -> Result<KeyFileData, String> {
        if !Path::new(key_file_path).exists() {
            let msg = format!("Key file not found: {}", key_file_path);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::KeyFileMissing,
                msg.clone(),
                Some(self.base.module_name().to_string()),
                None,
                Some("Ensure the .mdix.key file is in the expected location".to_string()),
                ErrorSeverity::Error,
            );
            return Err(msg);
        }

        let content = fs::read_to_string(key_file_path).map_err(|e| {
            let msg = format!("Failed to read key file: {}", e);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvocationFailed,
                msg.clone(),
                Some(self.base.module_name().to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            msg
        })?;

        let data = MdixKeyParser::parse(&content).map_err(|e| {
            let msg = format!("Failed to parse key file: {}", e);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvocationFailed,
                msg.clone(),
                Some(self.base.module_name().to_string()),
                None,
                Some("Key file may be corrupted or from an incompatible version".to_string()),
                ErrorSeverity::Error,
            );
            msg
        })?;

        if self.base.is_debug_enabled() {
            self.base.log_debug(&format!("Key file loaded: {}", key_file_path));
        }

        Ok(data)
    }

    pub fn extract_encryption_config(
        &self,
        data: &KeyFileData,
    ) -> Option<HashMap<String, String>> {
        let enc = data.key_data.encryption.as_ref()?;
        let mut config = HashMap::with_capacity(12);
        config.insert("algorithm".to_string(),      enc.algorithm.clone());
        config.insert("key_length".to_string(),     enc.key_length.to_string());
        config.insert("iv".to_string(),             enc.iv.clone());
        config.insert("security_level".to_string(), enc.security_level.clone());
        if let Some(ref kd) = enc.key_data {
            config.insert("key_data".to_string(), kd.clone());
        }
        if let Some(ref kdf) = enc.kdf {
            config.insert("kdf_algorithm".to_string(),   kdf.algorithm.clone());
            config.insert("kdf_version".to_string(),     kdf.kdf_version.clone());
            config.insert("kdf_memory".to_string(),      kdf.memory.to_string());
            config.insert("kdf_iterations".to_string(),  kdf.iterations.to_string());
            config.insert("kdf_parallelism".to_string(), kdf.parallelism.to_string());
            config.insert("salt".to_string(),            kdf.salt.clone());
            config.insert("salt_length".to_string(),     kdf.salt_length.to_string());
        }
        Some(config)
    }

    pub fn extract_compression_config(
        &self,
        data: &KeyFileData,
    ) -> Option<HashMap<String, String>> {
        let comp = data.key_data.compression.as_ref()?;
        let mut config = HashMap::with_capacity(4);
        config.insert("algorithm".to_string(), comp.algorithm.clone());
        if let Some(ref level) = comp.compression_level {
            config.insert("compression_level".to_string(), level.clone());
        }
        config.insert("original_size".to_string(),   comp.original_size.to_string());
        config.insert("compressed_size".to_string(), comp.compressed_size.to_string());
        Some(config)
    }

    pub fn is_password_protected(&self, data: &KeyFileData) -> bool {
        data.is_password_mode()
            && data.key_data.encryption
                .as_ref()
                .map(|e| e.kdf.is_some())
                .unwrap_or(false)
    }

    // ── Private ───────────────────────────────────────────────────────────────

    fn write_locked(&self, path: &str, content: &[u8]) -> Result<(), String> {
        let p           = Path::new(path);
        let file_exists = p.exists();

        if file_exists {
            file_permissions::set_writable(p)
                .map_err(|e| format!("Cannot unlock key file for writing: {}", e))?;
        }

        let result = fs::write(p, content).map_err(|e| {
            let msg = format!("Failed to write key file: {}", e);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvocationFailed,
                msg.clone(),
                Some(self.base.module_name().to_string()),
                None,
                Some(format!("Check write permissions for: {}", path)),
                ErrorSeverity::Error,
            );
            msg
        });

        if let Err(e) = file_permissions::set_readonly(p) {
            self.base.log_warning(&format!("Could not lock key file read-only: {}", e));
        }

        result
    }

    /// Derive the `.mdix.key` path from the compiled output path.
    ///
    /// Strips `.mdix.enc` or `.enc` from the end so that
    /// `output/01_gzip.mdix.enc` → `output/01_gzip.mdix.key`
    /// rather than the incorrect `output/01_gzip.mdix.mdix.key`.
    fn key_file_path(&self, compiled_file_path: &str) -> String {
        let name = Path::new(compiled_file_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("output");

        // Strip compound suffix first, then fallback to single-extension strip.
        let base = if name.ends_with(".mdix.enc") {
            &name[..name.len() - ".mdix.enc".len()]
        } else if name.ends_with(".enc") {
            &name[..name.len() - ".enc".len()]
        } else if name.ends_with(".mdix") {
            &name[..name.len() - ".mdix".len()]
        } else {
            name
        };

        Path::new(&self.output_directory)
            .join(format!("{}.mdix.key", base))
            .to_string_lossy()
            .to_string()
    }
    }
