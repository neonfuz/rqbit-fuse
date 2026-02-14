use crate::api::types::*;
use anyhow::{Context, Result};
use bytes::Bytes;
use reqwest::{Client, StatusCode};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, trace, warn};

/// HTTP client for interacting with rqbit server
pub struct RqbitClient {
    client: Client,
    base_url: String,
    max_retries: u32,
    retry_delay: Duration,
}

impl RqbitClient {
    /// Create a new RqbitClient with default configuration
    pub fn new(base_url: String) -> Self {
        Self::with_config(base_url, 3, Duration::from_millis(500))
    }

    /// Create a new RqbitClient with custom retry configuration
    pub fn with_config(base_url: String, max_retries: u32, retry_delay: Duration) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .pool_max_idle_per_host(10)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            base_url,
            max_retries,
            retry_delay,
        }
    }

    /// Helper method to execute a request with retry logic
    async fn execute_with_retry<F, Fut>(&self, operation: F) -> Result<reqwest::Response>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = reqwest::Result<reqwest::Response>>,
    {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            match operation().await {
                Ok(response) => {
                    // Check if we got a server error that might be transient
                    let status = response.status();
                    if status.is_server_error() && attempt < self.max_retries {
                        warn!(
                            "Server error {} on attempt {}/{}, retrying...",
                            status,
                            attempt + 1,
                            self.max_retries + 1
                        );
                        sleep(self.retry_delay * (attempt + 1)).await;
                        continue;
                    }
                    return Ok(response);
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.max_retries {
                        warn!(
                            "Request failed on attempt {}/{}, retrying: {}",
                            attempt + 1,
                            self.max_retries + 1,
                            last_error.as_ref().unwrap()
                        );
                        sleep(self.retry_delay * (attempt + 1)).await;
                    }
                }
            }
        }

        Err(last_error
            .map(ApiError::HttpError)
            .unwrap_or(ApiError::RetryLimitExceeded)
            .into())
    }

    /// Helper to check response status and convert errors
    async fn check_response(&self, response: reqwest::Response) -> Result<reqwest::Response> {
        let status = response.status();

        if status.is_success() || status == StatusCode::PARTIAL_CONTENT {
            Ok(response)
        } else {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(ApiError::ApiError {
                status: status.as_u16(),
                message,
            }
            .into())
        }
    }

    // =========================================================================
    // Torrent Management
    // =========================================================================

    /// List all torrents in the session
    pub async fn list_torrents(&self) -> Result<Vec<TorrentInfo>> {
        let url = format!("{}/torrents", self.base_url);

        trace!("Listing torrents from {}", url);

        let response = self
            .execute_with_retry(|| self.client.get(&url).send())
            .await?;

        let response = self.check_response(response).await?;
        let data: TorrentListResponse = response.json().await?;

        debug!("Listed {} torrents", data.torrents.len());
        Ok(data.torrents)
    }

    /// Get detailed information about a specific torrent
    pub async fn get_torrent(&self, id: u64) -> Result<TorrentInfo> {
        let url = format!("{}/torrents/{}", self.base_url, id);

        trace!("Getting torrent {} from {}", id, url);

        let response = self
            .execute_with_retry(|| self.client.get(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                let response = self.check_response(response).await?;
                let torrent: TorrentInfo = response.json().await?;
                debug!("Got torrent {}: {}", id, torrent.name);
                Ok(torrent)
            }
        }
    }

    /// Add a torrent from a magnet link
    pub async fn add_torrent_magnet(&self, magnet_link: &str) -> Result<AddTorrentResponse> {
        let url = format!("{}/torrents", self.base_url);
        let request = AddMagnetRequest {
            magnet_link: magnet_link.to_string(),
        };

        trace!("Adding torrent from magnet link");

        let response = self
            .execute_with_retry(|| self.client.post(&url).json(&request).send())
            .await?;

        let response = self.check_response(response).await?;
        let result: AddTorrentResponse = response.json().await?;

        debug!("Added torrent {} with hash {}", result.id, result.info_hash);
        Ok(result)
    }

    /// Add a torrent from a torrent file URL
    pub async fn add_torrent_url(&self, torrent_url: &str) -> Result<AddTorrentResponse> {
        let url = format!("{}/torrents", self.base_url);
        let request = AddTorrentUrlRequest {
            torrent_link: torrent_url.to_string(),
        };

        trace!("Adding torrent from URL: {}", torrent_url);

        let response = self
            .execute_with_retry(|| self.client.post(&url).json(&request).send())
            .await?;

        let response = self.check_response(response).await?;
        let result: AddTorrentResponse = response.json().await?;

        debug!("Added torrent {} with hash {}", result.id, result.info_hash);
        Ok(result)
    }

    /// Get statistics for a torrent
    pub async fn get_torrent_stats(&self, id: u64) -> Result<TorrentStats> {
        let url = format!("{}/torrents/{}/stats/v1", self.base_url, id);

        trace!("Getting stats for torrent {} from {}", id, url);

        let response = self
            .execute_with_retry(|| self.client.get(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                let response = self.check_response(response).await?;
                let stats: TorrentStats = response.json().await?;
                trace!("Torrent {} progress: {:.2}%", id, stats.progress_pct);
                Ok(stats)
            }
        }
    }

    /// Get piece availability bitfield for a torrent
    pub async fn get_piece_bitfield(&self, id: u64) -> Result<PieceBitfield> {
        let url = format!("{}/torrents/{}/haves", self.base_url, id);

        trace!("Getting piece bitfield for torrent {} from {}", id, url);

        let response = self
            .execute_with_retry(|| {
                self.client
                    .get(&url)
                    .header("Accept", "application/octet-stream")
                    .send()
            })
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                let response = self.check_response(response).await?;

                // Get the number of pieces from the header
                let num_pieces = response
                    .headers()
                    .get("x-bitfield-len")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .context("Missing or invalid x-bitfield-len header")?;

                let bits = response.bytes().await?.to_vec();

                trace!(
                    "Got piece bitfield for torrent {}: {} bytes, {} pieces",
                    id,
                    bits.len(),
                    num_pieces
                );

                Ok(PieceBitfield { bits, num_pieces })
            }
        }
    }

    // =========================================================================
    // File Operations
    // =========================================================================

    /// Read file data from a torrent
    ///
    /// If `range` is None, reads the entire file.
    /// If `range` is Some((start, end)), reads bytes from start to end (inclusive).
    pub async fn read_file(
        &self,
        torrent_id: u64,
        file_idx: usize,
        range: Option<(u64, u64)>,
    ) -> Result<Bytes> {
        let url = format!(
            "{}/torrents/{}/stream/{}",
            self.base_url, torrent_id, file_idx
        );

        let mut request = self.client.get(&url);

        // Add Range header if specified
        if let Some((start, end)) = range {
            if start > end {
                return Err(ApiError::InvalidRange(format!(
                    "Invalid range: start ({}) > end ({})",
                    start, end
                ))
                .into());
            }
            let range_header = format!("bytes={}-{}", start, end);
            trace!(
                "Reading file {} from torrent {} with range: {}",
                file_idx,
                torrent_id,
                range_header
            );
            request = request.header("Range", range_header);
        } else {
            trace!(
                "Reading entire file {} from torrent {}",
                file_idx,
                torrent_id
            );
        }

        let response = self
            .execute_with_retry(|| request.try_clone().unwrap().send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::FileNotFound {
                torrent_id,
                file_idx,
            }
            .into()),
            StatusCode::RANGE_NOT_SATISFIABLE => {
                let message = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Invalid range".to_string());
                Err(ApiError::InvalidRange(message).into())
            }
            _ => {
                let response = self.check_response(response).await?;
                let bytes = response.bytes().await?;

                trace!(
                    "Read {} bytes from file {} in torrent {}",
                    bytes.len(),
                    file_idx,
                    torrent_id
                );

                Ok(bytes)
            }
        }
    }

    // =========================================================================
    // Torrent Control
    // =========================================================================

    /// Pause a torrent
    pub async fn pause_torrent(&self, id: u64) -> Result<()> {
        let url = format!("{}/torrents/{}/pause", self.base_url, id);

        trace!("Pausing torrent {}", id);

        let response = self
            .execute_with_retry(|| self.client.post(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                self.check_response(response).await?;
                debug!("Paused torrent {}", id);
                Ok(())
            }
        }
    }

    /// Resume/start a torrent
    pub async fn start_torrent(&self, id: u64) -> Result<()> {
        let url = format!("{}/torrents/{}/start", self.base_url, id);

        trace!("Starting torrent {}", id);

        let response = self
            .execute_with_retry(|| self.client.post(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                self.check_response(response).await?;
                debug!("Started torrent {}", id);
                Ok(())
            }
        }
    }

    /// Remove torrent from session (keep files)
    pub async fn forget_torrent(&self, id: u64) -> Result<()> {
        let url = format!("{}/torrents/{}/forget", self.base_url, id);

        trace!("Forgetting torrent {}", id);

        let response = self
            .execute_with_retry(|| self.client.post(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                self.check_response(response).await?;
                debug!("Forgot torrent {}", id);
                Ok(())
            }
        }
    }

    /// Remove torrent from session and delete files
    pub async fn delete_torrent(&self, id: u64) -> Result<()> {
        let url = format!("{}/torrents/{}/delete", self.base_url, id);

        trace!("Deleting torrent {}", id);

        let response = self
            .execute_with_retry(|| self.client.post(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                self.check_response(response).await?;
                debug!("Deleted torrent {}", id);
                Ok(())
            }
        }
    }

    /// Check if the rqbit server is healthy
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/torrents", self.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_piece_bitfield() {
        // Create bitfield with pieces 0, 1, 3 downloaded (binary: 1011)
        // Byte: 0b00001011
        let bitfield = PieceBitfield {
            bits: vec![0b00001011],
            num_pieces: 4,
        };

        assert!(bitfield.has_piece(0));
        assert!(bitfield.has_piece(1));
        assert!(!bitfield.has_piece(2));
        assert!(bitfield.has_piece(3));

        assert_eq!(bitfield.downloaded_count(), 3);
        assert!(!bitfield.is_complete());
    }

    #[test]
    fn test_piece_bitfield_complete() {
        // All 8 pieces downloaded
        let bitfield = PieceBitfield {
            bits: vec![0b11111111],
            num_pieces: 8,
        };

        assert!(bitfield.is_complete());
        assert_eq!(bitfield.downloaded_count(), 8);
    }

    #[tokio::test]
    async fn test_client_creation() {
        let client = RqbitClient::new("http://localhost:3030".to_string());
        assert_eq!(client.base_url, "http://localhost:3030");
        assert_eq!(client.max_retries, 3);
    }

    #[test]
    fn test_api_error_mapping() {
        use libc;

        assert_eq!(ApiError::TorrentNotFound(1).to_fuse_error(), libc::ENOENT);

        assert_eq!(
            ApiError::FileNotFound {
                torrent_id: 1,
                file_idx: 0
            }
            .to_fuse_error(),
            libc::ENOENT
        );

        assert_eq!(
            ApiError::InvalidRange("test".to_string()).to_fuse_error(),
            libc::EINVAL
        );
    }
}
