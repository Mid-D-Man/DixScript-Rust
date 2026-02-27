// src/Compiler/ImportsResolution/mod.rs
//! File and cloud import resolution pipeline.

pub mod hash_verifier;
pub mod imports_resolver;

#[cfg(feature = "cloud_imports")]
pub mod cloud_storage_provider;
#[cfg(feature = "cloud_imports")]
pub mod cloud_file_cache;
#[cfg(feature = "cloud_imports")]
pub mod cloud_provider_factory;
#[cfg(feature = "cloud_imports")]
pub mod http_cloud_provider;

pub use hash_verifier::{HashVerifier, HashVerificationError};
pub use imports_resolver::{ImportsResolver, ImportResolutionStats};

#[cfg(feature = "cloud_imports")]
pub use cloud_storage_provider::{CloudStorageProvider, CloudStorageError};
#[cfg(feature = "cloud_imports")]
pub use cloud_file_cache::{CloudFileCache, CacheStatistics};
#[cfg(feature = "cloud_imports")]
pub use cloud_provider_factory::CloudProviderFactory;
#[cfg(feature = "cloud_imports")]
pub use http_cloud_provider::HttpCloudProvider;