
//! Factory that selects the appropriate cloud storage provider by URL scheme.
//!
//! v1.0.0 supports HTTP and HTTPS only; S3, Azure, and GCP are planned.

use std::sync::Arc;
use crate::ErrorManager::ErrorManager;
use super::cloud_storage_provider::CloudStorageProvider;
use super::http_cloud_provider::HttpCloudProvider;

pub struct CloudProviderFactory;

impl CloudProviderFactory {
    /// Return a provider for the given URL scheme.
    ///
    /// Returns `Err` with an actionable message for unsupported or future schemes.
    pub fn get_provider(
        cloud_url: &str,
        error_manager: &ErrorManager,
    ) -> Result<Arc<dyn CloudStorageProvider + Send + Sync>, String> {
        let lower = cloud_url.to_lowercase();

        if lower.starts_with("https://") || lower.starts_with("http://") {
            let provider = HttpCloudProvider::new(error_manager.clone());
            return Ok(Arc::new(provider));
        }

        if lower.starts_with("s3://") {
            return Err(format!(
                "S3 cloud imports are not yet supported in v1.0.0. \
                 Use a direct HTTPS URL instead: {}",
                cloud_url
            ));
        }

        if lower.starts_with("azure://") {
            return Err(format!(
                "Azure Blob Storage imports are not yet supported in v1.0.0. \
                 Use a direct HTTPS URL instead: {}",
                cloud_url
            ));
        }

        if lower.starts_with("gs://") {
            return Err(format!(
                "Google Cloud Storage imports are not yet supported in v1.0.0. \
                 Use a direct HTTPS URL instead: {}",
                cloud_url
            ));
        }

        Err(format!(
            "Unsupported cloud URL scheme. Supported in v1.0.0: http://, https://. URL: {}",
            cloud_url
        ))
    }

    pub fn is_supported_scheme(cloud_url: &str) -> bool {
        let lower = cloud_url.to_lowercase();
        lower.starts_with("https://") || lower.starts_with("http://")
    }

    pub fn get_supported_schemes() -> &'static [&'static str] {
        &["http://", "https://"]
    }

    pub fn get_planned_schemes() -> &'static [&'static str] {
        &["s3://", "azure://", "gs://"]
    }
}