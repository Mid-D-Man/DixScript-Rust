///Runtime key resolver
use crate::Compiler::DLM::KeyManagement::{KeyFileManager, KeyFileData, EncryptionKeyData};
use crate::ErrorManager::{ErrorManager, DlmErrorType, ErrorSeverity};
use crate::Runtime::load_options::DixLoadOptions;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use argon2::{Argon2, Algorithm, Version, Params};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum KeyFileSource {
    FilePath,
    AutoDetected,
    DirectContent,
    Url,
}

#[derive(Debug, Clone)]
pub struct KeyFileResolution {
    pub source:              KeyFileSource,
    pub source_description:  String,
    pub content:             String,
    pub file_path:           Option<PathBuf>,
}

/// Locates and reads the `.mdix.key` file for a given encrypted file.
pub struct KeyFileResolver {
    error_manager: ErrorManager,
}

impl KeyFileResolver {
    pub fn new() -> Self {
        KeyFileResolver {
            error_manager: ErrorManager::new_isolated(),
        }
    }

    /// Locate and read the key file based on the provided load options.
    ///
    /// Priority: direct content → explicit path → URL → auto-detect.
    pub fn resolve_key_file(
        &self,
        enc_path: &str,
        options: &DixLoadOptions,
    ) -> Result<KeyFileResolution, String> {
        if let Some(ref content) = options.key_file_content {
            if !options.allow_direct_key_content {
                return Err(
                    "Direct key content loading is disabled. \
                     Set allow_direct_key_content = true.".to_string()
                );
            }
            return Ok(KeyFileResolution {
                source:             KeyFileSource::DirectContent,
                source_description: "Direct content provided by caller".to_string(),
                content:            content.clone(),
                file_path:          None,
            });
        }

        if let Some(ref key_path) = options.key_file_path {
            let path = Path::new(key_path);
            if !path.exists() {
                return Err(format!("Explicit key file not found: {}", key_path));
            }
            let content = std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read key file '{}': {}", key_path, e))?;
            return Ok(KeyFileResolution {
                source:             KeyFileSource::FilePath,
                source_description: format!("Explicit path: {}", key_path),
                content,
                file_path:          Some(path.to_path_buf()),
            });
        }

        if let Some(ref url) = options.key_file_url {
            if !options.allow_url_key_loading {
                return Err(
                    "URL key loading is disabled. Set allow_url_key_loading = true.".to_string()
                );
            }
            if !url.starts_with("https://") {
                return Err("Key file URL must use HTTPS protocol.".to_string());
            }
            return Err(
                "URL-based key loading requires an async runtime. \
                 Load the key file content manually and use \
                 DixLoadOptions::with_key_content() instead.".to_string()
            );
        }

        self.auto_detect_key_file(enc_path, options)
    }

    fn auto_detect_key_file(
        &self,
        enc_path: &str,
        options: &DixLoadOptions,
    ) -> Result<KeyFileResolution, String> {
        let enc_path_buf = Path::new(enc_path);
        let dir          = enc_path_buf.parent().unwrap_or_else(|| Path::new("."));

        let file_name = enc_path_buf
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("output");

        let base_stem = file_name
            .strip_suffix(".enc").unwrap_or(file_name)
            .strip_suffix(".mdix").unwrap_or(file_name);

        let mut search_dirs: Vec<PathBuf> = vec![dir.to_path_buf()];
        if let Some(ref paths) = options.key_file_search_paths {
            for p in paths {
                search_dirs.push(PathBuf::from(p));
            }
        }

        for search_dir in &search_dirs {
            let candidate = search_dir.join(format!("{}.mdix.key", base_stem));
            if candidate.exists() {
                let content = std::fs::read_to_string(&candidate)
                    .map_err(|e| format!(
                        "Failed to read key file '{}': {}", candidate.display(), e
                    ))?;
                return Ok(KeyFileResolution {
                    source:             KeyFileSource::AutoDetected,
                    source_description: format!("Auto-detected: {}", candidate.display()),
                    content,
                    file_path:          Some(candidate),
                });
            }
        }

        Err(format!(
            "Key file '{}.mdix.key' not found. Searched in: {}",
            base_stem,
            search_dirs.iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

impl Default for KeyFileResolver {
    fn default() -> Self {
        Self::new()
    }
}

// ── KeyResolver ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ResolvedKey {
    pub key_bytes:  Vec<u8>,
    pub iv_bytes:   Vec<u8>,
    pub algorithm:  String,
    pub key_length: u32,
}

#[derive(Debug, Clone)]
pub enum KeySource {
    KeyFile(String),
    Password {
        key_file_path: String,
        password:      String,
    },
    RawBytes {
        key_bytes:  Vec<u8>,
        iv_bytes:   Vec<u8>,
        algorithm:  String,
    },
}

/// Derives or extracts the actual encryption key bytes from a key source.
pub struct KeyResolver {
    error_manager:    ErrorManager,
    source_file_path: String,
}

impl KeyResolver {
    pub fn new(source_file_path: String) -> Self {
        KeyResolver {
            error_manager:    ErrorManager::new_isolated(),
            source_file_path,
        }
    }

    pub fn resolve(&self, source: &KeySource) -> Result<ResolvedKey, String> {
        match source {
            KeySource::KeyFile(key_file_path) =>
                self.resolve_from_key_file(key_file_path),
            KeySource::Password { key_file_path, password } =>
                self.resolve_from_password(key_file_path, password),
            KeySource::RawBytes { key_bytes, iv_bytes, algorithm } =>
                self.resolve_from_raw_bytes(key_bytes, iv_bytes, algorithm),
        }
    }

    fn resolve_from_key_file(&self, key_file_path: &str) -> Result<ResolvedKey, String> {
        self.log_debug(&format!("Resolving key from key file: {}", key_file_path));

        let manager  = self.make_key_file_manager(key_file_path);
        let data     = manager.read_key_file(key_file_path)?;
        data.validate().map_err(|errs| errs.join(", "))?;

        let enc      = self.require_enc_meta(&data)?;
        let key_data = enc.key_data.as_ref().ok_or_else(|| {
            let msg = format!(
                "Key file '{}' has no key_data. Was it created in password mode? \
                 Use KeySource::Password instead.",
                key_file_path
            );
            self.report_error(&msg);
            msg
        })?;

        let key_bytes = BASE64.decode(key_data).map_err(|e| {
            let msg = format!("Failed to base64-decode key_data: {}", e);
            self.report_error(&msg);
            msg
        })?;

        let iv_bytes = self.decode_iv(enc, key_file_path)?;

        Ok(ResolvedKey {
            key_bytes,
            iv_bytes,
            algorithm:  enc.algorithm.clone(),
            key_length: enc.key_length as u32,
        })
    }

    fn resolve_from_password(
        &self,
        key_file_path: &str,
        password: &str,
    ) -> Result<ResolvedKey, String> {
        if password.is_empty() {
            let msg = "Password cannot be empty".to_string();
            self.report_error(&msg);
            return Err(msg);
        }

        self.log_debug(&format!(
            "Deriving key from password using key file: {}", key_file_path
        ));

        let manager = self.make_key_file_manager(key_file_path);
        let data    = manager.read_key_file(key_file_path)?;
        data.validate().map_err(|errs| errs.join(", "))?;

        if !manager.is_password_protected(&data) {
            let msg = format!(
                "Key file '{}' is not password-protected. Use KeySource::KeyFile instead.",
                key_file_path
            );
            self.report_error(&msg);
            return Err(msg);
        }

        let enc = self.require_enc_meta(&data)?;
        let kdf = enc.kdf.as_ref().ok_or_else(|| {
            let msg = "Password-protected key file is missing KDF parameters".to_string();
            self.report_error(&msg);
            msg
        })?;

        let salt = BASE64.decode(&kdf.salt).map_err(|e| {
            let msg = format!("Failed to base64-decode salt: {}", e);
            self.report_error(&msg);
            msg
        })?;

        let key_length  = enc.key_length;
        let t_cost      = kdf.iterations;
        let m_cost      = kdf.memory;
        let p_cost      = kdf.parallelism;

        self.log_debug(&format!(
            "Argon2id KDF: memory={}KB, iterations={}, parallelism={}, key_len={}",
            m_cost, t_cost, p_cost, key_length
        ));

        let params = Params::new(m_cost, t_cost, p_cost, Some(key_length)).map_err(|e| {
            let msg = format!("Invalid Argon2 parameters: {}", e);
            self.report_error(&msg);
            msg
        })?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut key_bytes = vec![0u8; key_length];
        argon2.hash_password_into(password.as_bytes(), &salt, &mut key_bytes)
            .map_err(|e| {
                let msg = format!("Key derivation failed: {}", e);
                self.report_error(&msg);
                msg
            })?;

        let iv_bytes = self.decode_iv(enc, key_file_path)?;

        Ok(ResolvedKey {
            key_bytes,
            iv_bytes,
            algorithm:  enc.algorithm.clone(),
            key_length: enc.key_length as u32,
        })
    }

    fn resolve_from_raw_bytes(
        &self,
        key_bytes: &[u8],
        iv_bytes: &[u8],
        algorithm: &str,
    ) -> Result<ResolvedKey, String> {
        if key_bytes.is_empty() {
            let msg = "Raw key bytes cannot be empty".to_string();
            self.report_error(&msg);
            return Err(msg);
        }
        if iv_bytes.is_empty() {
            let msg = "Raw IV bytes cannot be empty".to_string();
            self.report_error(&msg);
            return Err(msg);
        }

        let key_length = key_bytes.len() as u32;
        self.validate_key_length(key_length, algorithm)?;

        Ok(ResolvedKey {
            key_bytes:  key_bytes.to_vec(),
            iv_bytes:   iv_bytes.to_vec(),
            algorithm:  algorithm.to_string(),
            key_length,
        })
    }

    fn make_key_file_manager(&self, key_file_path: &str) -> KeyFileManager {
        let output_dir = Path::new(key_file_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".")
            .to_string();
        KeyFileManager::new(self.source_file_path.clone(), output_dir)
    }

    fn require_enc_meta<'a>(
        &self,
        data: &'a KeyFileData,
    ) -> Result<&'a EncryptionKeyData, String> {
        data.key_data.encryption.as_ref().ok_or_else(|| {
            let msg = format!(
                "Key file for '{}' contains no encryption metadata.",
                self.source_file_path
            );
            self.report_error(&msg);
            msg
        })
    }

    fn decode_iv(&self, enc: &EncryptionKeyData, key_file_path: &str) -> Result<Vec<u8>, String> {
        if enc.iv.is_empty() {
            let msg = format!("Key file '{}' has an empty IV field", key_file_path);
            self.report_error(&msg);
            return Err(msg);
        }
        BASE64.decode(&enc.iv).map_err(|e| {
            let msg = format!("Failed to base64-decode IV: {}", e);
            self.report_error(&msg);
            msg
        })
    }

    fn validate_key_length(&self, key_length: u32, algorithm: &str) -> Result<(), String> {
        let expected: Option<u32> = match algorithm.to_lowercase().as_str() {
            "aes128" | "aes-128-gcm" | "aes128-gcm"             => Some(16),
            "aes256" | "aes-256-gcm" | "aes256-gcm"             => Some(32),
            "chacha20" | "chacha20poly1305" | "chacha20-poly1305" => Some(32),
            _                                                     => None,
        };

        if let Some(exp) = expected {
            if key_length != exp {
                let msg = format!(
                    "Key length {} bytes does not match algorithm '{}' (expected {} bytes)",
                    key_length, algorithm, exp
                );
                self.report_error(&msg);
                return Err(msg);
            }
        }

        Ok(())
    }

    #[inline]
    fn report_error(&self, message: &str) {
        self.error_manager.add_dlm_error(
            DlmErrorType::InvocationFailed,
            message.to_string(),
            Some("KeyResolver".to_string()),
            None,
            None,
            ErrorSeverity::Error,
        );
    }

    #[inline]
    fn log_debug(&self, message: &str) {
        self.error_manager.log_debug(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_bytes_empty_key_fails() {
        let resolver = KeyResolver::new("test.mdix".to_string());
        let result   = resolver.resolve(&KeySource::RawBytes {
            key_bytes: vec![],
            iv_bytes:  vec![0u8; 12],
            algorithm: "aes256".to_string(),
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_raw_bytes_wrong_key_length_for_aes128() {
        let resolver = KeyResolver::new("test.mdix".to_string());
        let result   = resolver.resolve(&KeySource::RawBytes {
            key_bytes: vec![0u8; 32],
            iv_bytes:  vec![0u8; 12],
            algorithm: "aes128".to_string(),
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected 16 bytes"));
    }

    #[test]
    fn test_raw_bytes_correct_aes256() {
        let resolver = KeyResolver::new("test.mdix".to_string());
        let result   = resolver.resolve(&KeySource::RawBytes {
            key_bytes: vec![0u8; 32],
            iv_bytes:  vec![0u8; 12],
            algorithm: "aes256".to_string(),
        });
        assert!(result.is_ok());
        let key = result.unwrap();
        assert_eq!(key.key_length, 32);
        assert_eq!(key.algorithm, "aes256");
    }

    #[test]
    fn test_empty_password_fails() {
        let resolver = KeyResolver::new("test.mdix".to_string());
        let result   = resolver.resolve(&KeySource::Password {
            key_file_path: "nonexistent.mdix.key".to_string(),
            password:      "".to_string(),
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_each_resolver_has_isolated_error_state() {
        let resolver_a = KeyResolver::new("a.mdix".to_string());
        let resolver_b = KeyResolver::new("b.mdix".to_string());

        let _ = resolver_a.resolve(&KeySource::RawBytes {
            key_bytes: vec![],
            iv_bytes:  vec![0u8; 12],
            algorithm: "aes256".to_string(),
        });

        // resolver_b should have no errors even though resolver_a does.
        assert!(!resolver_b.error_manager.has_errors());
        assert!(resolver_a.error_manager.has_errors());
    }
}
