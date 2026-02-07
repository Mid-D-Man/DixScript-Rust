// src/Compiler/ImportsResolution/mod.rs

//! # ImportsResolution - Cloud imports and file resolution
//!
//! This module handles:
//! - Cloud file downloads (HTTP/HTTPS, future: S3, Azure, GCP)
//! - Local file caching
//! - Hash verification for security
//! - Network error handling (firewalls, proxies, timeouts)
//!
//! ## Ported Components (v1.0.0):
//! -  CloudStorageProvider trait (cloud_storage_provider.rs)
//! -  CloudFileCache (cloud_file_cache.rs)
//! -  CloudProviderFactory (cloud_provider_factory.rs)
//! -  HashVerifier (hash_verifier.rs)
//! -  HttpCloudProvider (http_cloud_provider.rs)
//! -  ImportsResolver (imports_resolver.rs)

pub mod cloud_storage_provider;
pub mod cloud_file_cache;
pub mod cloud_provider_factory;
pub mod hash_verifier;
pub mod http_cloud_provider;
pub mod imports_resolver;

// Re-exports - FIX: Use HashVerificationError not HashVerificationException
pub use cloud_storage_provider::{CloudStorageProvider, CloudStorageError};
pub use cloud_file_cache::{CloudFileCache, CacheStatistics};
pub use cloud_provider_factory::CloudProviderFactory;
pub use hash_verifier::{HashVerifier, HashVerificationError};
pub use http_cloud_provider::HttpCloudProvider;
pub use imports_resolver::{ImportsResolver, ImportResolutionStats};