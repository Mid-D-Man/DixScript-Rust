//! File and cloud import resolution pipeline.

pub mod hash_verifier;
pub mod imports_resolver;
pub mod cloud_storage_provider;
pub mod cloud_file_cache;

// ── Cloud-only modules — compiled only when the `cloud-import` feature is on ──
// Without this gate, cloud_provider_factory pulls in http_cloud_provider which
// pulls in reqwest — breaking any build with default-features = false.
#[cfg(feature = "cloud-import")]
pub mod cloud_provider_factory;
#[cfg(feature = "cloud-import")]
pub mod http_cloud_provider;

pub use hash_verifier::{HashVerifier, HashVerificationError};
pub use imports_resolver::{ImportsResolver, ImportResolutionStats};
pub use cloud_storage_provider::{CloudStorageProvider, CloudStorageError};
pub use cloud_file_cache::{CloudFileCache, CacheStatistics};

#[cfg(feature = "cloud-import")]
pub use cloud_provider_factory::CloudProviderFactory;
#[cfg(feature = "cloud-import")]
pub use http_cloud_provider::HttpCloudProvider;
