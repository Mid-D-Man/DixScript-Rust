
use std::time::Duration;
use crate::Compiler::VersionControl::CompatibilityMode;

/// Configuration options for loading DixScript files
/// 
/// Supports multiple key loading strategies:
/// - Password-based encryption (password mode)
/// - Key file path (explicit or default location)
/// - Direct key content (from secure vault - requires acknowledgment)
/// - Key URL (HTTPS only - requires acknowledgment)
/// - Key search paths
#[derive(Debug, Clone)]
pub struct DixLoadOptions {
    /// Password for decryption (password mode)
    pub password: Option<String>,
    
    /// Explicit key file path (if not in same directory as .mdix.enc)
    pub key_file_path: Option<String>,
    
    /// Direct key file content (e.g., from secure vault)
    /// WARNING: Use only when loading from trusted sources like HashiCorp Vault or AWS Secrets Manager
    pub key_file_content: Option<String>,
    
    /// Load key file from URL
    /// WARNING: Must be HTTPS only. Use only for trusted internal services.
    pub key_file_url: Option<String>,
    
    /// Allow loading key files from URLs (default: false for security)
    pub allow_url_key_loading: bool,
    
    /// Allow loading key files from content string (default: false for security)
    pub allow_direct_key_content: bool,
    
    /// Output directory for generated files (.mdix.enc, .mdix.key, .mdix.au)
    /// If None, uses same directory as source file
    pub output_directory: Option<String>,
    
    /// Validate checksums during load (default: true)
    pub validate_checksums: bool,
    
    /// Throw exception if expected section is missing (default: false)
    pub throw_on_missing_sections: bool,
    
    /// Cache loaded data for hot-reload scenarios (default: false)
    pub enable_caching: bool,
    
    /// How to handle version mismatches (default: Strict)
    pub compatibility_mode: CompatibilityMode,
    
    /// Timeout for URL key file downloads (default: 10 seconds)
    pub url_load_timeout: Duration,
    
    /// Search paths for key files if not found in default location
    pub key_file_search_paths: Option<Vec<String>>,
}

impl DixLoadOptions {
    /// Create default load options
    pub fn new() -> Self {
        DixLoadOptions {
            password: None,
            key_file_path: None,
            key_file_content: None,
            key_file_url: None,
            allow_url_key_loading: false,
            allow_direct_key_content: false,
            output_directory: None,
            validate_checksums: true,
            throw_on_missing_sections: false,
            enable_caching: false,
            compatibility_mode: CompatibilityMode::Strict,
            url_load_timeout: Duration::from_secs(10),
            key_file_search_paths: None,
        }
    }
    
    /// Create options with password
    pub fn with_password(password: impl Into<String>) -> Self {
        DixLoadOptions {
            password: Some(password.into()),
            ..Default::default()
        }
    }
    
    /// Create options with key file path
    pub fn with_key_file(key_file_path: impl Into<String>) -> Self {
        DixLoadOptions {
            key_file_path: Some(key_file_path.into()),
            ..Default::default()
        }
    }
    
    /// Create options with direct key content
    /// 
    /// WARNING: Use only when loading from secure vault (HashiCorp Vault, AWS Secrets Manager, etc.)
    /// Requires explicit security acknowledgment
    pub fn with_key_content(
        key_content: impl Into<String>,
        acknowledge_security_risk: bool,
    ) -> Result<Self, String> {
        if !acknowledge_security_risk {
            return Err(
                "Direct key content loading requires explicit security acknowledgment. \
                 Set acknowledge_security_risk = true if you understand the risks. \
                 This should ONLY be used when loading from trusted secure vaults.".to_string()
            );
        }
        
        Ok(DixLoadOptions {
            key_file_content: Some(key_content.into()),
            allow_direct_key_content: true,
            ..Default::default()
        })
    }
    
    /// Create options with key file URL
    /// 
    /// WARNING: Must be HTTPS only. Use only for trusted internal services.
    /// Requires explicit security acknowledgment
    pub fn with_key_url(
        key_file_url: impl Into<String>,
        acknowledge_security_risk: bool,
    ) -> Result<Self, String> {
        let url = key_file_url.into();
        
        if !acknowledge_security_risk {
            return Err(
                "URL key loading requires explicit security acknowledgment. \
                 Set acknowledge_security_risk = true if you understand the risks. \
                 This should ONLY be used for HTTPS URLs from trusted internal services.".to_string()
            );
        }
        
        if !url.starts_with("https://") {
            return Err(
                "Key file URL must use HTTPS protocol for security. \
                 HTTP is not allowed for key file loading.".to_string()
            );
        }
        
        Ok(DixLoadOptions {
            key_file_url: Some(url),
            allow_url_key_loading: true,
            ..Default::default()
        })
    }
    
    /// Create options with output directory
    pub fn with_output_directory(output_directory: impl Into<String>) -> Self {
        DixLoadOptions {
            output_directory: Some(output_directory.into()),
            ..Default::default()
        }
    }
    
    /// Create options with key search paths
    pub fn with_key_search_paths(search_paths: Vec<String>) -> Self {
        DixLoadOptions {
            key_file_search_paths: Some(search_paths),
            ..Default::default()
        }
    }
    
    /// Validate options for security and consistency
    pub fn validate(&self) -> Result<(), String> {
        // Check for multiple key loading methods
        let mut key_options_count = 0;
        if self.key_file_path.is_some() {
            key_options_count += 1;
        }
        if self.key_file_content.is_some() {
            key_options_count += 1;
        }
        if self.key_file_url.is_some() {
            key_options_count += 1;
        }
        
        if key_options_count > 1 {
            return Err(
                "Cannot specify multiple key loading methods. \
                 Use only ONE of: key_file_path, key_file_content, or key_file_url.".to_string()
            );
        }
        
        // Validate URL key loading
        if let Some(ref url) = self.key_file_url {
            if !self.allow_url_key_loading {
                return Err(
                    "URL key loading is disabled for security. \
                     Set allow_url_key_loading = true if you trust the source.".to_string()
                );
            }
            
            if !url.starts_with("https://") {
                return Err(
                    "Key file URL must use HTTPS protocol. HTTP is not allowed.".to_string()
                );
            }
        }
        
        // Validate direct key content
        if let Some(ref content) = self.key_file_content {
            if !self.allow_direct_key_content {
                return Err(
                    "Direct key content loading is disabled for security. \
                     Set allow_direct_key_content = true if you trust the source.".to_string()
                );
            }
            
            if content.len() < 50 {
                return Err(
                    "Key file content appears too short to be valid. \
                     Ensure you're providing the complete key file content.".to_string()
                );
            }
        }
        
        Ok(())
    }
}

impl Default for DixLoadOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_options() {
        let opts = DixLoadOptions::new();
        assert!(opts.password.is_none());
        assert!(opts.validate_checksums);
        assert!(!opts.allow_url_key_loading);
        assert!(!opts.allow_direct_key_content);
    }
    
    #[test]
    fn test_with_password() {
        let opts = DixLoadOptions::with_password("test123");
        assert_eq!(opts.password.as_deref(), Some("test123"));
    }
    
    #[test]
    fn test_with_key_file() {
        let opts = DixLoadOptions::with_key_file("/path/to/key.key");
        assert_eq!(opts.key_file_path.as_deref(), Some("/path/to/key.key"));
    }
    
    #[test]
    fn test_with_key_content_requires_ack() {
        let result = DixLoadOptions::with_key_content("key_content", false);
        assert!(result.is_err());
        
        let result = DixLoadOptions::with_key_content("key_content", true);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_with_key_url_requires_https() {
        let result = DixLoadOptions::with_key_url("http://example.com/key", true);
        assert!(result.is_err());
        
        let result = DixLoadOptions::with_key_url("https://example.com/key", true);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_validate_multiple_key_methods() {
        let opts = DixLoadOptions {
            key_file_path: Some("path".to_string()),
            key_file_content: Some("content".to_string()),
            allow_direct_key_content: true,
            ..Default::default()
        };
        
        assert!(opts.validate().is_err());
    }
    
    #[test]
    fn test_validate_url_security() {
        let opts = DixLoadOptions {
            key_file_url: Some("https://example.com/key".to_string()),
            allow_url_key_loading: false,
            ..Default::default()
        };
        
        assert!(opts.validate().is_err());
    }
  }
