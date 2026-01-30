// src/Compiler/ImportsResolution/cloud_provider_factory.rs

use crate::ErrorManager::ErrorManager;
use super::http_cloud_provider::HttpCloudProvider;
use super::cloud_storage_provider::CloudStorageProvider;
use std::sync::Arc;

/// Factory for selecting appropriate cloud storage provider based on URL scheme
///
/// v1.0.0 - HTTP/HTTPS only (Phase 1)
/// Future: S3, Azure Blob, GCP Storage (Phases 2-4)
pub struct CloudProviderFactory;

impl CloudProviderFactory {
    /// Get appropriate cloud storage provider for URL
    ///
    /// # Errors
    /// - Returns error if URL scheme is not supported
    pub fn get_provider(
        cloud_url: &str,
        error_manager: &ErrorManager,
    ) -> Result<Arc<dyn CloudStorageProvider + Send + Sync>, String> {
        let lower_url = cloud_url.to_lowercase();

        // Phase 1: HTTP/HTTPS support
        if lower_url.starts_with("https://") || lower_url.starts_with("http://") {
            error_manager.log_debug(&format!("Using HttpCloudProvider for {}", cloud_url));

            let provider = HttpCloudProvider::new(error_manager.clone());
            return Ok(Arc::new(provider));
        }

        // Phase 2: S3 support (not yet implemented)
        if lower_url.starts_with("s3://") {
            return Err(format!(
                "S3 cloud imports are not yet supported in v1.0.0. \
                 S3 support will be added in a future version. URL: {}",
                cloud_url
            ));
        }

        // Phase 3: Azure Blob Storage support (not yet implemented)
        if lower_url.starts_with("azure://") {
            return Err(format!(
                "Azure Blob Storage imports are not yet supported in v1.0.0. \
                 Azure support will be added in a future version. URL: {}",
                cloud_url
            ));
        }

        // Phase 4: GCP Storage support (not yet implemented)
        if lower_url.starts_with("gs://") {
            return Err(format!(
                "Google Cloud Storage imports are not yet supported in v1.0.0. \
                 GCP support will be added in a future version. URL: {}",
                cloud_url
            ));
        }

        // Unknown scheme
        Err(format!(
            "Unsupported cloud URL scheme. Supported in v1.0.0: http://, https://. \
             URL: {}",
            cloud_url
        ))
    }

    /// Check if URL scheme is supported in current version
    pub fn is_supported_scheme(cloud_url: &str) -> bool {
        let lower_url = cloud_url.to_lowercase();
        lower_url.starts_with("https://") || lower_url.starts_with("http://")
    }

    /// Get list of supported schemes in current version
    pub fn get_supported_schemes() -> &'static [&'static str] {
        &["http://", "https://"]
    }

    /// Get list of planned schemes for future versions
    pub fn get_planned_schemes() -> &'static [&'static str] {
        &["s3://", "azure://", "gs://"]
    }
}