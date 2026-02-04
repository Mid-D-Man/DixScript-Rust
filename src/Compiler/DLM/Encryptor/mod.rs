//! Encryptor - Data encryption modules

mod encryptor_trait;
mod xor_encryptor;
mod aes128_encryptor;
mod aes256_encryptor;
mod chacha20_encryptor;

pub use encryptor_trait::{IEncryptor, EncryptorResult};
pub use xor_encryptor::XorEncryptor;
pub use aes128_encryptor::Aes128Encryptor;
pub use aes256_encryptor::Aes256Encryptor;
pub use chacha20_encryptor::Chacha20Encryptor;
