//! Encryptor trait definition

use std::collections::HashMap;

/// Result type for encryptor operations
pub type EncryptorResult<T> = Result<T, String>;

/// Trait for encryption modules
pub trait IEncryptor {
    /// Get module name
    fn module_name(&self) -> &str;

    /// Get encryption algorithm name
    fn algorithm(&self) -> &str;

    /// Initialize encryptor with configuration
    fn initialize(&mut self, config: HashMap<String, String>);

    /// Set password for password-based encryption
    fn set_password(&mut self, password: &str) -> EncryptorResult<()>;

    /// Encrypt binary data
    fn encrypt(&self, data: &[u8]) -> EncryptorResult<Vec<u8>>;

    /// Decrypt binary data
    fn decrypt(&self, encrypted_data: &[u8]) -> EncryptorResult<Vec<u8>>;

    /// Validate encryptor can execute
    fn validate(&self) -> Result<(), String>;

    /// Get metadata for .mdix.key file
    fn get_metadata(&self) -> HashMap<String, String>;

    /// Get priority (lower = earlier execution)
    fn priority(&self) -> i32;
}
