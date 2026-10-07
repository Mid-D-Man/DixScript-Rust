//! Encryptor - Data encryption modules

mod encryptor_trait;
#[cfg(feature = "dlm-encryptor")]
mod xor_encryptor;
#[cfg(feature = "encryption-support")]
mod aes128_encryptor;
#[cfg(feature = "encryption-support")]
mod aes256_encryptor;
#[cfg(feature = "encryption-support")]
mod chacha20_encryptor;

pub use encryptor_trait::{IEncryptor, EncryptorResult};
#[cfg(feature = "dlm-encryptor")]
pub use xor_encryptor::XorEncryptor;
#[cfg(feature = "encryption-support")]
pub use aes128_encryptor::Aes128Encryptor;
#[cfg(feature = "encryption-support")]
pub use aes256_encryptor::Aes256Encryptor;
#[cfg(feature = "encryption-support")]
pub use chacha20_encryptor::Chacha20Encryptor;
