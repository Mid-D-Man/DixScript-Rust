// src/Compiler/ImportsResolution/http_cloud_provider.rs

use crate::ErrorManager::ErrorManager;
use super::cloud_storage_provider::{CloudStorageProvider, CloudStorageError};
use reqwest;
use std::time::Duration;

/// HTTP/HTTPS cloud storage provider with Dropbox support
///
/// FEATURES:
/// - Proper redirect handling (up to 10 redirects)
/// - Dropbox URL processing (dl=1 parameter)
/// - Retry logic with exponential backoff
/// - HTML detection (prevents downloading preview pages)
/// - Timeout handling
/// - Firewall/proxy error detection
///
/// v1.0.1 - Enhanced Dropbox compatibility and network awareness
pub struct HttpCloudProvider {
    client: reqwest::Client,
    error_manager: ErrorManager,
}

// Configuration constants
const TIMEOUT_SECONDS: u64 = 60; // Increased for cloud services with redirects
const MAX_RETRIES: usize = 3;
const INITIAL_RETRY_DELAY_MS: u64 = 1000;
const BACKOFF_MULTIPLIER: u64 = 2;
const MAX_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024; // 10MB

impl HttpCloudProvider {
    /// Create new HttpCloudProvider
    pub fn new(error_manager: ErrorManager) -> Self {
        // Configure HTTP client with proper redirect handling
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECONDS))
            .redirect(reqwest::redirect::Policy::limited(10)) // Allow up to 10 redirects
            .user_agent("DixScript-Compiler/1.0.0")
            .build()
            .expect("Failed to create HTTP client");

        HttpCloudProvider {
            client,
            error_manager,
        }
    }

    /// Process Dropbox URLs to ensure direct download
    ///
    /// Ensures dl=1 parameter is present and dl=0 is replaced
    fn process_dropbox_url(url: &str) -> String {
        if !url.to_lowercase().contains("dropbox.com") {
            return url.to_string();
        }

        // Replace dl=0 with dl=1
        if url.contains("dl=0") {
            return url.replace("dl=0", "dl=1");
        }

        // Add dl=1 if not present
        if !url.contains("dl=1") && !url.contains("dl=") {
            let separator = if url.contains('?') { '&' } else { '?' };
            return format!("{}{}dl=1", url, separator);
        }

        url.to_string()
    }

    /// Check if content is HTML (indicating we got a preview page)
    fn is_html_content(content: &str) -> bool {
        if content.is_empty() {
            return false;
        }

        let trimmed = content.trim_start();

        trimmed.starts_with("<!DOCTYPE") ||
            trimmed.starts_with("<html") ||
            trimmed.starts_with("<HTML")
    }

    /// Warn about insecure HTTP usage
    fn warn_insecure_http(&self, url: &str) {
        // Don't warn for localhost/127.0.0.1 (dev servers)
        if url.to_lowercase().contains("localhost") || url.contains("127.0.0.1") {
            return;
        }

        self.error_manager.log_Warning(&format!(
            "⚠️  SECURITY WARNING: Using insecure HTTP for cloud import '{}'. \
             Use HTTPS for production to prevent man-in-the-middle attacks.",
            url
        ));
    }

    /// Detect if error is likely due to firewall blocking
    fn is_likely_firewall_error(error: &reqwest::Error) -> bool {
        // Connection refused, timeout, or DNS failure often indicates firewall
        error.is_timeout() ||
            error.is_connect() ||
            error.to_string().to_lowercase().contains("connection refused") ||
            error.to_string().to_lowercase().contains("network unreachable")
    }

    /// Detect if error is likely due to proxy configuration
    fn is_likely_proxy_error(error: &reqwest::Error) -> bool {
        // Proxy-related errors
        error.to_string().to_lowercase().contains("proxy") ||
            error.status() == Some(reqwest::StatusCode::PROXY_AUTHENTICATION_REQUIRED)
    }
}

#[async_trait::async_trait]
impl CloudStorageProvider for HttpCloudProvider {
    /// Download file from HTTP/HTTPS URL with retry logic
    async fn download_file_async(&self, cloud_url: &str) -> Result<String, CloudStorageError> {
        // Validate URL format
        if !cloud_url.starts_with("https://") && !cloud_url.starts_with("http://") {
            return Err(CloudStorageError::InvalidUrl {
                url: cloud_url.to_string(),
                message: "URL must start with http:// or https://".to_string(),
            });
        }

        // Security warning for HTTP
        if cloud_url.starts_with("http://") {
            self.warn_insecure_http(cloud_url);
        }

        // Process Dropbox URLs
        let processed_url = Self::process_dropbox_url(cloud_url);

        if processed_url != cloud_url {
            self.error_manager.log_debug(&format!(
                "Processed Dropbox URL: {} → {}",
                cloud_url, processed_url
            ));
        }

        // Retry logic with exponential backoff
        let mut retry_count = 0;
        let mut retry_delay = Duration::from_millis(INITIAL_RETRY_DELAY_MS);

        loop {
            self.error_manager.log_debug(&format!(
                "Downloading from {} (attempt {}/{})",
                processed_url,
                retry_count + 1,
                MAX_RETRIES + 1
            ));

            match self.client.get(&processed_url).send().await {
                Ok(response) => {
                    // Check HTTP status
                    let status = response.status();
                    if !status.is_success() {
                        return Err(CloudStorageError::HttpError {
                            url: processed_url,
                            status_code: status.as_u16(),
                            message: status.canonical_reason()
                                .unwrap_or("Unknown error")
                                .to_string(),
                        });
                    }

                    // Check content length if available
                    if let Some(content_length) = response.content_length() {
                        if content_length > MAX_FILE_SIZE_BYTES {
                            return Err(CloudStorageError::FileTooLarge {
                                url: processed_url,
                                size_bytes: content_length,
                                max_size_bytes: MAX_FILE_SIZE_BYTES,
                            });
                        }

                        self.error_manager.log_debug(&format!(
                            "Content-Length: {} bytes",
                            content_length
                        ));
                    }

                    // Read content
                    let content = response.text().await.map_err(|e| {
                        CloudStorageError::Other {
                            url: processed_url.clone(),
                            message: format!("Failed to read response body: {}", e),
                        }
                    })?;

                    // Double-check actual size
                    let actual_size = content.len() as u64;
                    if actual_size > MAX_FILE_SIZE_BYTES {
                        return Err(CloudStorageError::FileTooLarge {
                            url: processed_url,
                            size_bytes: actual_size,
                            max_size_bytes: MAX_FILE_SIZE_BYTES,
                        });
                    }

                    // Validate that we got actual file content, not HTML
                    if Self::is_html_content(&content) {
                        return Err(CloudStorageError::ReceivedHtml {
                            url: processed_url,
                            message: "Received HTML instead of file content. \
                                      This may be a Dropbox preview page. \
                                      Ensure the URL has 'dl=1' parameter for direct download."
                                .to_string(),
                        });
                    }

                    self.error_manager.log_debug(&format!(
                        "Successfully downloaded {} bytes from {}",
                        actual_size, processed_url
                    ));

                    return Ok(content);
                }

                Err(e) => {
                    // Check if we should retry
                    if retry_count >= MAX_RETRIES {
                        // Classify error type
                        if e.is_timeout() {
                            return Err(CloudStorageError::Timeout {
                                url: processed_url,
                                duration_secs: TIMEOUT_SECONDS,
                            });
                        }

                        let is_firewall = Self::is_likely_firewall_error(&e);
                        let is_proxy = Self::is_likely_proxy_error(&e);

                        return Err(CloudStorageError::NetworkError {
                            url: processed_url,
                            message: e.to_string(),
                            is_likely_firewall: is_firewall,
                            is_likely_proxy: is_proxy,
                        });
                    }

                    // Log retry
                    self.error_manager.log_Warning(&format!(
                        "Download failed (attempt {}), retrying in {:?}: {}",
                        retry_count + 1,
                        retry_delay,
                        e
                    ));

                    // Wait before retry
                    tokio::time::sleep(retry_delay).await;

                    // Exponential backoff
                    retry_count += 1;
                    retry_delay *= BACKOFF_MULTIPLIER as u32;
                }
            }
        }
    }

    /// Check if file exists at HTTP/HTTPS URL (HEAD request)
    async fn file_exists_async(&self, cloud_url: &str) -> Result<bool, CloudStorageError> {
        let processed_url = Self::process_dropbox_url(cloud_url);

        match self.client.head(&processed_url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(e) => {
                self.error_manager.log_debug(&format!(
                    "File existence check failed for {}: {}",
                    cloud_url, e
                ));
                Ok(false)
            }
        }
    }
}