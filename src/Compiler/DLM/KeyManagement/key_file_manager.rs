//! Manages .mdix.key file generation and reading

use crate::Compiler::AST::DixScript;
use crate::Compiler::DLM::dlm_module_base::DebugConfig;
use crate::ErrorManager::{ErrorManager, DlmErrorType};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use chrono::Utc;
use regex::Regex;
use crate::ErrorSeverity;
use super::key_file_data::*;

/// Manages .mdix.key file operations
pub struct KeyFileManager {
    error_manager: ErrorManager,
    debug_config: DebugConfig,
}

impl KeyFileManager {
    /// Create new KeyFileManager
    pub fn new(debug_mode: crate::Compiler::Core::Config::DebugMode) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let debug_config = DebugConfig::from_debug_mode(debug_mode);
        
        KeyFileManager {
            error_manager,
            debug_config,
        }
    }
    
    /// Generate .mdix.key file from pipeline metadata
    pub fn generate_key_file(
        &self,
        key_file_path: &Path,
        pipeline_metadata: &HashMap<String, HashMap<String, String>>,
        ast: &DixScript,
    ) -> Result<(), String> {
        self.error_manager.log_info(&format!(
            "[KeyFileManager] Generating key file: {}",
            key_file_path.display()
        ));
        
        let key_file_content = self.build_key_file_content(pipeline_metadata, ast)?;
        
        // Write key file
        let mut file = File::create(key_file_path)
            .map_err(|e| format!("Failed to create key file: {}", e))?;
        
        file.write_all(key_file_content.as_bytes())
            .map_err(|e| format!("Failed to write key file: {}", e))?;
        
        self.error_manager.log_info(&format!(
            "[KeyFileManager] ✅ Key file generated: {}",
            key_file_path.display()
        ));
        
        // Handle backups (if configured)
        if let Some(ref security) = ast.security {
            self.handle_backups(key_file_path, security)?;
        }
        
        Ok(())
    }
    
    /// Build .mdix.key file content in DixScript format
    fn build_key_file_content(
        &self,
        metadata: &HashMap<String, HashMap<String, String>>,
        ast: &DixScript,
    ) -> Result<String, String> {
        let mut content = String::new();
        
        // Header
        content.push_str("// DixScript Key File - v1.0.0\n");
        content.push_str(&format!("// Generated: {}\n", Utc::now().format("%Y-%m-%dT%H:%M:%SZ")));
        content.push_str("// ⚠️ KEEP THIS FILE SECRET - Contains decryption keys!\n");
        content.push_str("\n");
        
        // @CONFIG section
        content.push_str("@CONFIG(\n");
        content.push_str("  version -> \"1.0.0\",\n");
        content.push_str("  type -> \"keyfile\",\n");
        content.push_str(&format!("  generated -> {}\n", Utc::now().format("%Y-%m-%dT%H:%M:%SZ")));
        content.push_str(")\n");
        content.push_str("\n");
        
        // @DLM_PIPELINE section
        content.push_str("@DLM_PIPELINE(\n");
        
        // Modules used
        let mut modules = Vec::new();
        if metadata.contains_key("compressor") {
            if let Some(comp_meta) = metadata.get("compressor") {
                if let Some(module_name) = comp_meta.get("module_name") {
                    modules.push(module_name.clone());
                }
            }
        }
        if metadata.contains_key("encryptor") {
            if let Some(enc_meta) = metadata.get("encryptor") {
                if let Some(module_name) = enc_meta.get("module_name") {
                    modules.push(module_name.clone());
                }
            }
        }
        if metadata.contains_key("auditor") {
            if let Some(aud_meta) = metadata.get("auditor") {
                if let Some(module_name) = aud_meta.get("module_name") {
                    modules.push(module_name.clone());
                }
            }
        }
        
        content.push_str(&format!("  modules_used:: \"{}\"\n", modules.join("\", \"")));
        content.push_str(")\n");
        content.push_str("\n");
        
        // @KEY_DATA section
        content.push_str("@KEY_DATA(\n");
        
        // Encryption metadata
        if let Some(enc_meta) = metadata.get("encryptor") {
            content.push_str("  // Encryption configuration\n");
            self.append_metadata(&mut content, enc_meta, "  ");
            content.push_str("\n");
        }
        
        // Compression metadata
        if let Some(comp_meta) = metadata.get("compressor") {
            content.push_str("  // Compression configuration\n");
            self.append_metadata(&mut content, comp_meta, "  ");
        }
        
        content.push_str(")\n");
        content.push_str("\n");
        
        // @FILE_INFO section
        content.push_str("@FILE_INFO(\n");
        content.push_str(&format!("  created -> {}", Utc::now().format("%Y-%m-%dT%H:%M:%SZ")));
        content.push_str("\n)\n");
        
        Ok(content)
    }
    
    #[inline]
    fn append_metadata(&self, content: &mut String, meta: &HashMap<String, String>, indent: &str) {
        for (key, value) in meta {
            // Try to parse as number
            if let Ok(_) = value.parse::<i64>() {
                content.push_str(&format!("{}{} = {},\n", indent, key, value));
            } else if let Ok(_) = value.parse::<f64>() {
                content.push_str(&format!("{}{} = {},\n", indent, key, value));
            } else if value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false") {
                content.push_str(&format!("{}{} = {},\n", indent, key, value.to_lowercase()));
            } else {
                content.push_str(&format!("{}{} = \"{}\",\n", indent, key, value));
            }
        }
    }
    
    /// Handle backup creation (if enabled in @SECURITY)
    fn handle_backups(
        &self,
        key_file_path: &Path,
        security: &crate::Compiler::AST::SecuritySection,
    ) -> Result<(), String> {
        // Find keystore block
        let keystore_block = security.entries.iter()
            .find(|e| e.block_key.eq_ignore_ascii_case("keystore"));
        
        if keystore_block.is_none() {
            return Ok(());
        }
        
        let block = keystore_block.unwrap();
        
        // Check backup_count field
        let backup_count_field = block.fields.iter()
            .find(|f| f.key.eq_ignore_ascii_case("backup_count"));
        
        if backup_count_field.is_none() {
            return Ok(());
        }
        
        let backup_count = if let crate::Compiler::AST::Value::Integer { value, .. } = &backup_count_field.unwrap().value {
            *value
        } else {
            return Ok(());
        };
        
        if backup_count <= 0 {
            return Ok(());
        }
        
        self.error_manager.log_info(&format!(
            "[KeyFileManager] Creating {} backup(s) of key file...",
            backup_count
        ));
        
        // Create backups with timestamp
        let directory = key_file_path.parent().unwrap_or_else(|| Path::new("."));
        let file_stem = key_file_path.file_stem().unwrap().to_str().unwrap();
        
        // Get existing backups
        let mut existing_backups = Vec::new();
        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with(file_stem) && name.contains(".backup_") {
                        existing_backups.push(path);
                    }
                }
            }
        }
        
        // Sort by creation time (newest first)
        existing_backups.sort_by_key(|p| {
            std::fs::metadata(p)
                .and_then(|m| m.created())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });
        existing_backups.reverse();
        
        // Delete oldest backups if we exceed limit
        while existing_backups.len() >= backup_count as usize {
            if let Some(oldest) = existing_backups.pop() {
                std::fs::remove_file(&oldest).ok();
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug(&format!(
                        "[KeyFileManager] Deleted old backup: {}",
                        oldest.file_name().unwrap().to_str().unwrap()
                    ));
                }
            }
        }
        
        // Create new backup
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_path = directory.join(format!("{}.backup_{}", file_stem, timestamp));
        
        std::fs::copy(key_file_path, &backup_path)
            .map_err(|e| format!("Failed to create backup: {}", e))?;
        
        self.error_manager.log_info(&format!(
            "[KeyFileManager] 🔑 Backup created: {}",
            backup_path.file_name().unwrap().to_str().unwrap()
        ));
        
        Ok(())
    }
    
    /// Read .mdix.key file and extract metadata
    pub fn read_key_file(&self, key_file_path: &Path) -> Result<HashMap<String, HashMap<String, String>>, String> {
        self.error_manager.log_info(&format!(
            "[KeyFileManager] Reading key file: {}",
            key_file_path.display()
        ));
        
        if !key_file_path.exists() {
            self.error_manager.add_dlm_error(
                DlmErrorType::KeyFileMissing,
                format!("Key file not found: {}", key_file_path.display()),
                Some("KeyFileManager".to_string()),
                Some(key_file_path.to_str().unwrap().to_string()),
                None,
                ErrorSeverity::Fatal,
            );
            return Err(format!("Key file not found: {}", key_file_path.display()));
        }
        
        let mut file = File::open(key_file_path)
            .map_err(|e| format!("Failed to open key file: {}", e))?;
        
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| format!("Failed to read key file: {}", e))?;
        
        // Parse key file (simple regex-based parsing for now)
        let mut metadata = HashMap::new();
        
        // Extract KEY_DATA section
        let re = Regex::new(r"@KEY_DATA\((.*?)\)").unwrap();
        if let Some(captures) = re.captures(&content) {
            if let Some(key_data_content) = captures.get(1) {
                self.parse_key_data(key_data_content.as_str(), &mut metadata);
            }
        }
        
        self.error_manager.log_info("[KeyFileManager] ✅ Key file parsed successfully");
        
        Ok(metadata)
    }
    
    /// Simple parser for KEY_DATA section
    /// TODO: Replace with full DixScript parser later
    fn parse_key_data(&self, key_data_content: &str, metadata: &mut HashMap<String, HashMap<String, String>>) {
        let lines: Vec<&str> = key_data_content.lines().collect();
        
        let mut encryptor_meta = HashMap::new();
        let mut compressor_meta = HashMap::new();
        
        let mut in_encryption = false;
        let mut in_compression = false;
        
        for line in lines {
            let trimmed = line.trim();
            
            if trimmed.starts_with("//") {
                if trimmed.contains("Encryption") {
                    in_encryption = true;
                    in_compression = false;
                } else if trimmed.contains("Compression") {
                    in_encryption = false;
                    in_compression = true;
                }
                continue;
            }
            
            if !trimmed.contains('=') {
                continue;
            }
            
            let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
            if parts.len() != 2 {
                continue;
            }
            
            let key = parts[0].trim();
            let value = parts[1].trim().trim_end_matches(',').trim_matches('"');
            
            if in_encryption {
                encryptor_meta.insert(key.to_string(), value.to_string());
            } else if in_compression {
                compressor_meta.insert(key.to_string(), value.to_string());
            }
        }
        
        if !encryptor_meta.is_empty() {
            metadata.insert("encryptor".to_string(), encryptor_meta);
        }
        
        if !compressor_meta.is_empty() {
            metadata.insert("compressor".to_string(), compressor_meta);
        }
    }
          }
