// src/Compiler/ImportsResolution/http_cloud_provider.rs
//! HTTP/HTTPS cloud storage provider with retry logic and Dropbox direct-download support.
//!
//! This file is only compiled when the `cloud_imports` feature is enabled.

use std::time::Duration;
use crate::ErrorManager::ErrorManager;
use super::cloud_storage_provider::{CloudStorageProvider, CloudStorageError};

const TIMEOUT_SECONDS: u64 = 60;
const MAX_RETRIES: usize = 3;
const INITIAL_RETRY_DELAY_MS: u64 = 1000;
const BACKOFF_MULTIPLIER: u32 = 2;
const MAX_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024;

pub struct HttpCloudProvider {
    client: reqwest::Client,
    error_manager: ErrorManager,
}

impl HttpCloudProvider {
    pub fn new(error_manager: ErrorManager) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECONDS))
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent("DixScript-Compiler/1.0.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        HttpCloudProvider { client, error_manager }
    }

    fn process_dropbox_url(url: &str) -> String {
        if !url.to_lowercase().contains("dropbox.com") {
            return url.to_string();
        }
        if url.contains("dl=0") {
            return url.replace("dl=0", "dl=1");
        }
        if !url.contains("dl=1") && !url.contains("dl=") {
            let sep = if url.contains('?') { '&' } else { '?' };
            return format!("{}{}dl=1", url, sep);
        }
        url.to_string()
    }

    fn is_html_content(content: &str) -> bool {
        if content.is_empty() {
            return false;
        }
        let trimmed = content.trim_start();
        trimmed.starts_with("<!DOCTYPE")
            || trimmed.starts_with("<html")
            || trimmed.starts_with("<HTML")
    }

    fn warn_insecure_http(&self, url: &str) {
        if url.to_lowercase().contains("localhost") || url.contains("127.0.0.1") {
            return;
        }
        self.error_manager.log_warning(&format!(
            "SECURITY WARNING: Using insecure HTTP for cloud import '{}'. \
             Use HTTPS for production to prevent man-in-the-middle attacks.",
            url
        ));
    }

    fn is_likely_firewall_error(error: &reqwest::Error) -> bool {
        error.is_timeout()
            || error.is_connect()
            || error.to_string().to_lowercase().contains("connection refused")
            || error.to_string().to_lowercase().contains("network unreachable")
    }

    fn is_likely_proxy_error(error: &reqwest::Error) -> bool {
        error.to_string().to_lowercase().contains("proxy")
            || error.status() == Some(reqwest::StatusCode::PROXY_AUTHENTICATION_REQUIRED)
    }
}

#[async_trait::async_trait]
impl CloudStorageProvider for HttpCloudProvider {
    async fn download_file_async(&self, cloud_url: &str) -> Result<String, CloudStorageError> {
        if !cloud_url.starts_with("https://") && !cloud_url.starts_with("http://") {
            return Err(CloudStorageError::InvalidUrl {
                url: cloud_url.to_string(),
                message: "URL must start with http:// or https://".to_string(),
            });
        }

        if cloud_url.starts_with("http://") {
            self.warn_insecure_http(cloud_url);
        }

        let processed_url = Self::process_dropbox_url(cloud_url);
        if processed_url != cloud_url {
            self.error_manager.log_debug(&format!(
                "Processed Dropbox URL: {} -> {}",
                cloud_url, processed_url
            ));
        }

        let mut retry_count = 0usize;
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
                    let status = response.status();
                    if !status.is_success() {
                        return Err(CloudStorageError::HttpError {
                            url: processed_url,
                            status_code: status.as_u16(),
                            message: status
                                .canonical_reason()
                                .unwrap_or("Unknown error")
                                .to_string(),
                        });
                    }

                    if let Some(content_length) = response.content_length() {
                        if content_length > MAX_FILE_SIZE_BYTES {
                            return Err(CloudStorageError::FileTooLarge {
                                url: processed_url,
                                size_bytes: content_length,
                                max_size_bytes: MAX_FILE_SIZE_BYTES,
                            });
                        }
                    }

                    let content = response.text().await.map_err(|e| CloudStorageError::Other {
                        url: processed_url.clone(),
                        message: format!("Failed to read response body: {}", e),
                    })?;

                    let actual_size = content.len() as u64;
                    if actual_size > MAX_FILE_SIZE_BYTES {
                        return Err(CloudStorageError::FileTooLarge {
                            url: processed_url,
                            size_bytes: actual_size,
                            max_size_bytes: MAX_FILE_SIZE_BYTES,
                        });
                    }

                    if Self::is_html_content(&content) {
                        return Err(CloudStorageError::ReceivedHtml {
                            url: processed_url,
                            message: "Received HTML instead of file content. \
                                      This may be a preview page. \
                                      For Dropbox, ensure the URL has the 'dl=1' parameter."
                                .to_string(),
                        });
                    }

                    self.error_manager.log_debug(&format!(
                        "Downloaded {} bytes from {}",
                        actual_size, processed_url
                    ));

                    return Ok(content);
                }

                Err(e) => {
                    if retry_count >= MAX_RETRIES {
                        if e.is_timeout() {
                            return Err(CloudStorageError::Timeout {
                                url: processed_url,
                                duration_secs: TIMEOUT_SECONDS,
                            });
                        }

                        return Err(CloudStorageError::NetworkError {
                            url: processed_url,
                            message: e.to_string(),
                            is_likely_firewall: Self::is_likely_firewall_error(&e),
                            is_likely_proxy: Self::is_likely_proxy_error(&e),
                        });
                    }

                    self.error_manager.log_warning(&format!(
                        "Download failed (attempt {}), retrying in {:?}: {}",
                        retry_count + 1,
                        retry_delay,
                        e
                    ));

                    tokio::time::sleep(retry_delay).await;
                    retry_count += 1;
                    retry_delay *= BACKOFF_MULTIPLIER;
                }
            }
        }
    }

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