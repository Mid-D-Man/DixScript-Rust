//! Encryptor - Data encryption modules

mod encryptor_trait;
#[cfg(feature = "dlm-encryptor")]
mod xor_encryptor;
#[cfg(feature = "aes128-support")]
mod aes128_encryptor;
#[cfg(feature = "aes256-support")]
mod aes256_encryptor;
#[cfg(feature = "chacha20-support")]
mod chacha20_encryptor;

pub use encryptor_trait::{IEncryptor, EncryptorResult};
#[cfg(feature = "dlm-encryptor")]
pub use xor_encryptor::XorEncryptor;
#[cfg(feature = "aes128-support")]
pub use aes128_encryptor::Aes128Encryptor;
#[cfg(feature = "aes256-support")]
pub use aes256_encryptor::Aes256Encryptor;
#[cfg(feature = "chacha20-support")]
pub use chacha20_encryptor::Chacha20Encryptor;
