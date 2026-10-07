//! Key management — encryption key derivation, key file I/O.

#[cfg(feature = "encryption-support")]
mod argon2_kdf;
mod key_file_data;
mod key_file_format;
mod key_file_manager;

#[cfg(feature = "encryption-support")]
pub use argon2_kdf::Argon2KDF;
pub use key_file_data::*;
pub use key_file_format::{MdixKeyWriter, MdixKeyParser};
pub use key_file_manager::KeyFileManager;
