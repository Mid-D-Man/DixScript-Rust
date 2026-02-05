// src/Runtime/key_resolver.rs

use std::path::{Path, PathBuf};
use std::fs;
use crate::ErrorManager::{ErrorManager, RuntimeError, RuntimeErrorType, ErrorSeverity};
use super::load_options::DixLoadOptions;

/// Source of the resolved key file
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyFileSource {
    /// Key file path provided explicitly
    FilePath,
    /// Key content provided directly (from vault)
    DirectContent,
    /// Key loaded from URL (HTTPS only)
    Url,
    /// Found via search paths
    SearchPath,
    /// Found in common location
    CommonLocation,
}

/// Result of key file resolution
#[derive(Debug, Clone)]
pub struct KeyFileResolution {
    /// The actual key file content
    pub content: String,
    
    /// Where the key came from
    pub source: KeyFileSource,
    
    /// Human-readable description
    pub source_description: String,
    
    /// File path (if loaded from file)
    pub file_path: Option<PathBuf>,
}

impl KeyFileResolution {
    /// Create from file path
    pub fn from_file(path: PathBuf, content: String) -> Self {
        KeyFileResolution {
            content,
            source: KeyFileSource::FilePath,
            source_description: path.display().to_string(),
            file_path: Some(path),
        }
    }
    
    /// Create from direct content
    pub fn from_content(content: String) -> Self {
        KeyFileResolution {
            content,
            source: KeyFileSource::DirectContent,
            source_description: "Direct content (secure vault)".to_string(),
            file_path: None,
        }
    }
    
    /// Create from URL
    pub fn from_url(url: String, content: String) -> Self {
        KeyFileResolution {
            content,
            source: KeyFileSource::Url,
            source_description: url,
            file_path: None,
        }
    }
}

/// Resolves key files from various sources with security and error handling
pub struct KeyFileResolver {
    error_manager: &'static ErrorManager,
}

impl KeyFileResolver {
    /// Create new key file resolver
    pub fn new() -> Self {
        KeyFileResolver {
            error_manager: ErrorManager::get_shared_instance(),
        }
    }
    
    /// Resolve key file content from various sources
    /// 
    /// Priority order:
    /// 1. Direct content (if provided and allowed)
    /// 2. URL (if provided and allowed) - NOTE: HTTP client not in core, wrappers handle this
    /// 3. Explicit file path
    /// 4. Default key path (same directory as encrypted file)
    /// 5. Search paths (if provided)
    /// 6. Common locations
    pub fn resolve_key_file(
        &self,
        encrypted_file_path: &str,
        options: &DixLoadOptions,
    ) -> Result<KeyFileResolution, String> {
        // Validate options first
        options.validate()?;
        
        // 1. Direct content (highest priority if allowed)
        if let Some(ref content) = options.key_file_content {
            return self.resolve_from_content(content);
        }
        
        // 2. URL (handled by language wrapper - core Rust doesn't do HTTP)
        if let Some(ref url) = options.key_file_url {
            return Err(
                "URL key loading must be handled by language wrapper. \
                 Core Rust package does not include HTTP client. \
                 Use wrapper's LoadEncWithKeyUrl() method instead.".to_string()
            );
        }
        
        // 3. Explicit file path
        if let Some(ref key_path) = options.key_file_path {
            return self.resolve_from_path(key_path);
        }
        
        // 4. Default key path (same directory as .mdix.enc)
        let default_path = Self::get_default_key_path(encrypted_file_path);
        if default_path.exists() {
            self.error_manager.log_info(&format!(
                "Using default key file: {}",
                default_path.display()
            ));
            return self.resolve_from_path(&default_path.to_string_lossy());
        }
        
        // 5. Search in provided paths
        if let Some(ref search_paths) = options.key_file_search_paths {
            if let Ok(resolution) = self.search_in_paths(encrypted_file_path, search_paths) {
                return Ok(resolution);
            }
        }
        
        // 6. Search in common locations
        if let Ok(resolution) = self.search_in_common_locations(encrypted_file_path) {
            return Ok(resolution);
        }
        
        // All strategies failed
        let error_msg = format!(
            "Key file not found for '{}'. Tried:\n\
             1. Default location: {}\n\
             2. Common locations (./keys/, ./secrets/, etc.)\n\
             Provide key file via:\n\
             - DixLoadOptions::with_key_file(path)\n\
             - DixLoadOptions::with_key_content(content) [secure vault only]\n\
             - DixLoadOptions::with_key_url(url) [HTTPS only, via wrapper]",
            encrypted_file_path,
            default_path.display()
        );
        
        self.error_manager.add_runtime_error(
            RuntimeErrorType::ResourceNotFound,
            error_msg.clone(),
            None,
            0,
            0,
            vec![],
            Some("Ensure key file exists or provide it explicitly".to_string()),
            ErrorSeverity::Error,
        );
        
        Err(error_msg)
    }
    
    /// Resolve from direct content
    fn resolve_from_content(&self, content: &str) -> Result<KeyFileResolution, String> {
        self.error_manager.log_warning(
            "WARNING: Loading key from direct content. \
             This should ONLY be used with trusted secure vaults."
        );
        
        // Validate content looks like a DixScript key file
        if !content.contains("@CONFIG") && !content.contains("@KEY_DATA") {
            let error_msg = "Key content does not appear to be a valid DixScript key file. \
                            Expected @CONFIG or @KEY_DATA sections.";
            
            self.error_manager.add_runtime_error(
                RuntimeErrorType::InvalidOperation,
                error_msg.to_string(),
                Some("KeyFileResolver.resolve_from_content".to_string()),
                0,
                0,
                vec![],
                Some("Check key content format".to_string()),
                ErrorSeverity::Error,
            );
            
            return Err(error_msg.to_string());
        }
        
        self.error_manager.log_info("Key file loaded from direct content");
        
        Ok(KeyFileResolution::from_content(content.to_string()))
    }
    
    /// Resolve from file path
    fn resolve_from_path(&self, key_path: &str) -> Result<KeyFileResolution, String> {
        let path = Path::new(key_path);
        
        if !path.exists() {
            let error_msg = format!("Key file not found: {}", key_path);
            
            self.error_manager.add_runtime_error(
                RuntimeErrorType::ResourceNotFound,
                error_msg.clone(),
                Some("KeyFileResolver.resolve_from_path".to_string()),
                0,
                0,
                vec![],
                Some("Check file path".to_string()),
                ErrorSeverity::Error,
            );
            
            return Err(error_msg);
        }
        
        self.error_manager.log_info(&format!("Reading key file: {}", key_path));
        
        let content = fs::read_to_string(path).map_err(|e| {
            let error_msg = format!("Failed to read key file {}: {}", key_path, e);
            
            self.error_manager.add_runtime_error(
                RuntimeErrorType::InvalidOperation,
                error_msg.clone(),
                Some("KeyFileResolver.resolve_from_path".to_string()),
                0,
                0,
                vec![],
                Some("Check file permissions".to_string()),
                ErrorSeverity::Error,
            );
            
            error_msg
        })?;
        
        if content.trim().is_empty() {
            let error_msg = format!("Key file is empty: {}", key_path);
            
            self.error_manager.add_runtime_error(
                RuntimeErrorType::InvalidOperation,
                error_msg.clone(),
                Some("KeyFileResolver.resolve_from_path".to_string()),
                0,
                0,
                vec![],
                None,
                ErrorSeverity::Error,
            );
            
            return Err(error_msg);
        }
        
        self.error_manager.log_info(&format!(
            "Key file loaded: {} ({} bytes)",
            key_path,
            content.len()
        ));
        
        Ok(KeyFileResolution::from_file(path.to_path_buf(), content))
    }
    
    /// Search for key file in provided paths
    fn search_in_paths(
        &self,
        encrypted_file_path: &str,
        search_paths: &[String],
    ) -> Result<KeyFileResolution, String> {
        let base_name = Self::extract_base_name(encrypted_file_path);
        let key_file_name = format!("{}.mdix.key", base_name);
        
        self.error_manager.log_info(&format!("Searching for key file: {}", key_file_name));
        
        for search_path in search_paths {
            let full_path = Path::new(search_path).join(&key_file_name);
            
            if full_path.exists() {
                self.error_manager.log_info(&format!(
                    "Found key file in search path: {}",
                    full_path.display()
                ));
                return self.resolve_from_path(&full_path.to_string_lossy());
            }
        }
        
        Err("Key file not found in search paths".to_string())
    }
    
    /// Search in common locations
    fn search_in_common_locations(
        &self,
        encrypted_file_path: &str,
    ) -> Result<KeyFileResolution, String> {
        let encrypted_path = Path::new(encrypted_file_path);
        let directory = encrypted_path
            .parent()
            .unwrap_or_else(|| Path::new("."));
        
        let base_name = Self::extract_base_name(encrypted_file_path);
        let key_file_name = format!("{}.mdix.key", base_name);
        
        // Common search locations relative to encrypted file
        let common_locations = vec![
            directory.to_path_buf(),
            directory.join("keys"),
            directory.join("secrets"),
            directory.join(".keys"),
            directory.join(".secrets"),
            directory.join("config").join("keys"),
            directory.join("..").join("keys"),
            PathBuf::from(".keys"),
            PathBuf::from("keys"),
        ];
        
        self.error_manager.log_info(&format!(
            "Searching common locations for: {}",
            key_file_name
        ));
        
        for location in common_locations {
            if !location.exists() {
                continue;
            }
            
            let full_path = location.join(&key_file_name);
            
            if full_path.exists() {
                self.error_manager.log_info(&format!(
                    "Found key file in common location: {}",
                    full_path.display()
                ));
                return self.resolve_from_path(&full_path.to_string_lossy());
            }
        }
        
        Err("Key file not found in common locations".to_string())
    }
    
    /// Get default key path for an encrypted file
    /// 
    /// Rules:
    /// - file.mdix.enc → file.mdix.key
    /// - file.enc → file.key
    pub fn get_default_key_path(encrypted_file_path: &str) -> PathBuf {
        let path = Path::new(encrypted_file_path);
        let directory = path.parent().unwrap_or_else(|| Path::new("."));
        
        let mut file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        
        // Strip .enc extensions
        while file_name.to_lowercase().ends_with(".enc") {
            file_name = file_name[..file_name.len() - 4].to_string();
        }
        
        let key_file_name = format!("{}.key", file_name);
        
        directory.join(key_file_name)
    }
    
    /// Extract base name from encrypted file path
    /// 
    /// Examples:
    /// - /path/to/file.mdix.enc → file
    /// - file.enc → file
    fn extract_base_name(encrypted_file_path: &str) -> String {
        let path = Path::new(encrypted_file_path);
        
        let mut name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        
        // Strip all .enc and .mdix extensions
        while name.to_lowercase().ends_with(".enc") || name.to_lowercase().ends_with(".mdix") {
            if name.to_lowercase().ends_with(".enc") {
                name = name[..name.len() - 4].to_string();
            } else if name.to_lowercase().ends_with(".mdix") {
                name = name[..name.len() - 5].to_string();
            }
        }
        
        name
    }
    
    /// Validate if a key file is accessible and valid
    pub fn validate_key_file(key_file_path: &str) -> Result<bool, String> {
        let path = Path::new(key_file_path);
        
        if !path.exists() {
            return Err(format!("Key file not found: {}", key_file_path));
        }
        
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Access denied to key file {}: {}", key_file_path, e))?;
        
        if content.trim().is_empty() {
            return Err("Key file is empty".to_string());
        }
        
        if !content.contains("@CONFIG") && !content.contains("@KEY_DATA") {
            return Err("Key file does not appear to be valid DixScript format".to_string());
        }
        
        Ok(true)
    }
}

impl Default for KeyFileResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    
    #[test]
    fn test_get_default_key_path() {
        let path = KeyFileResolver::get_default_key_path("file.mdix.enc");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "file.mdix.key");
        
        let path = KeyFileResolver::get_default_key_path("/path/to/data.enc");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "data.key");
    }
    
    #[test]
    fn test_extract_base_name() {
        assert_eq!(KeyFileResolver::extract_base_name("file.mdix.enc"), "file");
        assert_eq!(KeyFileResolver::extract_base_name("data.enc"), "data");
        assert_eq!(KeyFileResolver::extract_base_name("/path/to/config.mdix.enc"), "config");
    }
    
    #[test]
    fn test_resolve_from_content() {
        let resolver = KeyFileResolver::new();
        let content = "@CONFIG(version -> \"1.0.0\")";
        
        let result = resolver.resolve_from_content(content).unwrap();
        assert_eq!(result.source, KeyFileSource::DirectContent);
        assert_eq!(result.content, content);
    }
    
    #[test]
    fn test_resolve_from_content_invalid() {
        let resolver = KeyFileResolver::new();
        let content = "invalid content";
        
        let result = resolver.resolve_from_content(content);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_validate_key_file() {
        // Create temp key file
        let temp_dir = std::env::temp_dir();
        let key_path = temp_dir.join("test.key");
        
        let mut file = fs::File::create(&key_path).unwrap();
        file.write_all(b"@CONFIG(version -> \"1.0.0\")").unwrap();
        
        let result = KeyFileResolver::validate_key_file(key_path.to_str().unwrap());
        assert!(result.is_ok());
        
        // Cleanup
        fs::remove_file(&key_path).unwrap();
    }
          }
