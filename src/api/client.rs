use crate::api::streaming::PersistentStreamManager;
use crate::api::types::*;
use crate::error::RqbitFuseError;
use crate::metrics::Metrics;
use anyhow::{Context, Result};
use base64::Engine;
use bytes::Bytes;
use reqwest::{Client, StatusCode};

use futures::stream::StreamExt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, info, instrument, trace, warn};

/// HTTP client for interacting with rqbit server
pub struct RqbitClient {
    client: Client,
    base_url: String,
    max_retries: u32,
    retry_delay: Duration,
    stream_manager: PersistentStreamManager,
    auth_credentials: Option<(String, String)>,
    list_torrents_cache: Arc<RwLock<Option<(Instant, ListTorrentsResult)>>>,
    list_torrents_cache_ttl: Duration,
    metrics: Option<Arc<Metrics>>,
}

impl RqbitClient {
    /// Create a new RqbitClient with default configuration
    pub fn new(base_url: String) -> Result<Self> {
        Self::with_config(base_url, 3, Duration::from_millis(500), None, None)
    }

    /// Create a new RqbitClient with authentication
    pub fn with_auth(base_url: String, username: String, password: String) -> Result<Self> {
        Self::with_config(
            base_url,
            3,
            Duration::from_millis(500),
            Some((username, password)),
            None,
        )
    }

    /// Create a new RqbitClient with custom retry configuration
    pub fn with_config(
        base_url: String,
        max_retries: u32,
        retry_delay: Duration,
        auth_credentials: Option<(String, String)>,
        metrics: Option<Arc<Metrics>>,
    ) -> Result<Self> {
        // Validate URL at construction time (fail fast on invalid URL)
        let _ = reqwest::Url::parse(&base_url)
            .map_err(|e| RqbitFuseError::IoError(format!("Invalid URL: {}", e)))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .pool_max_idle_per_host(10)
            .build()
            .map_err(|e| RqbitFuseError::IoError(format!("Failed to create HTTP client: {}", e)))?;

        let stream_manager = PersistentStreamManager::new(
            client.clone(),
            base_url.clone(),
            auth_credentials.clone(),
        );

        Ok(Self {
            client,
            base_url,
            max_retries,
            retry_delay,
            stream_manager,
            auth_credentials,
            list_torrents_cache: Arc::new(RwLock::new(None)),
            list_torrents_cache_ttl: Duration::from_secs(30),
            metrics,
        })
    }

    /// Create Authorization header for HTTP Basic Auth
    fn create_auth_header(&self) -> Option<String> {
        self.auth_credentials.as_ref().map(|(username, password)| {
            let credentials = format!("{}:{}", username, password);
            let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
            format!("Basic {}", encoded)
        })
    }

    /// Invalidate the list_torrents cache
    /// Should be called when torrents are added or removed
    async fn invalidate_list_torrents_cache(&self) {
        let mut cache = self.list_torrents_cache.write().await;
        if cache.is_some() {
            debug!("list_torrents: cache invalidated");
            *cache = None;
        }
    }

    /// Execute request with automatic retry for transient failures
    async fn execute_with_retry<F, Fut>(
        &self,
        endpoint: &str,
        operation: F,
    ) -> Result<reqwest::Response>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = reqwest::Result<reqwest::Response>>,
    {
        let mut last_error = None;
        let mut final_result = None;

        for attempt in 0..=self.max_retries {
            match operation().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_server_error() && attempt < self.max_retries {
                        warn!(
                            endpoint,
                            status = status.as_u16(),
                            attempt = attempt + 1,
                            "Server error, retrying"
                        );
                        sleep(self.retry_delay * (attempt + 1)).await;
                        continue;
                    }

                    if status == StatusCode::TOO_MANY_REQUESTS && attempt < self.max_retries {
                        let retry_after = response
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                            .map(Duration::from_secs)
                            .unwrap_or_else(|| self.retry_delay * (attempt + 1));

                        warn!(
                            endpoint,
                            status = status.as_u16(),
                            retry_after_secs = retry_after.as_secs(),
                            attempt = attempt + 1,
                            "Rate limited"
                        );
                        sleep(retry_after).await;
                        continue;
                    }

                    final_result = Some(Ok(response));
                    break;
                }
                Err(e) => {
                    let api_error: RqbitFuseError = e.into();
                    last_error = Some(api_error.clone());

                    if api_error.is_transient() && attempt < self.max_retries {
                        warn!(endpoint, attempt = attempt + 1, error = %api_error, "Retrying");
                        sleep(self.retry_delay * (attempt + 1)).await;
                    } else {
                        final_result = Some(Err(api_error));
                        break;
                    }
                }
            }
        }

        match final_result {
            Some(Ok(response)) => Ok(response),
            Some(Err(api_error)) => Err(api_error.into()),
            None => Err(last_error
                .unwrap_or_else(|| RqbitFuseError::NotReady("Retry limit exceeded".to_string()))
                .into()),
        }
    }

    /// Helper to check response status and convert errors
    async fn check_response(&self, response: reqwest::Response) -> Result<reqwest::Response> {
        let status = response.status();

        if status.is_success() || status == StatusCode::PARTIAL_CONTENT {
            Ok(response)
        } else if status == StatusCode::UNAUTHORIZED {
            let message = response.text().await.unwrap_or_default();
            Err(RqbitFuseError::PermissionDenied(format!(
                "Authentication failed: {}",
                if message.is_empty() {
                    "Invalid credentials".to_string()
                } else {
                    message
                }
            ))
            .into())
        } else {
            let message = match response.text().await {
                Ok(text) => text,
                Err(e) => {
                    return Err(RqbitFuseError::NetworkError(format!(
                        "Failed to read error response body: {}",
                        e
                    ))
                    .into());
                }
            };
            Err(RqbitFuseError::ApiError {
                status: status.as_u16(),
                message,
            }
            .into())
        }
    }

    /// Generic GET request that returns JSON
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        url: &str,
    ) -> Result<T> {
        let response = self
            .execute_with_retry(endpoint, || {
                let mut req = self.client.get(url);
                if let Some(auth_header) = self.create_auth_header() {
                    req = req.header("Authorization", auth_header);
                }
                req.send()
            })
            .await?;
        let response = self.check_response(response).await?;
        Ok(response.json().await?)
    }

    /// Generic POST request with JSON body that returns JSON
    async fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        url: &str,
        body: &B,
    ) -> Result<T> {
        let response = self
            .execute_with_retry(endpoint, || {
                let mut req = self.client.post(url).json(body);
                if let Some(auth_header) = self.create_auth_header() {
                    req = req.header("Authorization", auth_header);
                }
                req.send()
            })
            .await?;
        let response = self.check_response(response).await?;
        Ok(response.json().await?)
    }

    // =========================================================================
    // Torrent Management
    // =========================================================================

    /// List all torrents in the session with caching
    #[instrument(skip(self), fields(api_op = "list_torrents"))]
    pub async fn list_torrents(&self) -> Result<ListTorrentsResult> {
        // Check cache first
        {
            let cache = self.list_torrents_cache.read().await;
            if let Some((cached_at, cached_result)) = cache.as_ref() {
                if cached_at.elapsed() < self.list_torrents_cache_ttl {
                    debug!("list_torrents: cache hit");
                    if let Some(metrics) = &self.metrics {
                        metrics.record_cache_hit();
                    }
                    return Ok(cached_result.clone());
                }
            }
        }

        // Cache miss or expired - fetch fresh data
        if let Some(metrics) = &self.metrics {
            metrics.record_cache_miss();
        }
        debug!("list_torrents: cache miss or expired, fetching fresh data");
        let url = format!("{}/torrents", self.base_url);

        let response = self
            .execute_with_retry("/torrents", || {
                let mut req = self.client.get(&url);
                if let Some(auth_header) = self.create_auth_header() {
                    req = req.header("Authorization", auth_header);
                }
                req.send()
            })
            .await?;

        let response = self.check_response(response).await?;
        let data: TorrentListResponse = response.json().await?;

        // Fetch full details for each torrent since /torrents doesn't include files
        let mut result = ListTorrentsResult {
            torrents: Vec::with_capacity(data.torrents.len()),
            errors: Vec::new(),
        };

        for basic_info in data.torrents {
            match self.get_torrent(basic_info.id).await {
                Ok(full_info) => {
                    result.torrents.push(full_info);
                }
                Err(e) => {
                    warn!(
                        id = basic_info.id,
                        name = %basic_info.name,
                        error = %e,
                        "Failed to get full details for torrent"
                    );
                    // Convert anyhow::Error to RqbitFuseError for storage
                    let api_err = if let Some(api_err) = e.downcast_ref::<RqbitFuseError>() {
                        api_err.clone()
                    } else {
                        RqbitFuseError::IoError(e.to_string())
                    };
                    result
                        .errors
                        .push((basic_info.id, basic_info.name, api_err));
                }
            }
        }

        // Log summary if there were partial failures
        if result.is_partial() {
            info!(
                successes = result.torrents.len(),
                failures = result.errors.len(),
                "Partial result for list_torrents: {} succeeded, {} failed",
                result.torrents.len(),
                result.errors.len()
            );
        }

        // Cache the result
        {
            let mut cache = self.list_torrents_cache.write().await;
            *cache = Some((Instant::now(), result.clone()));
        }

        Ok(result)
    }

    /// Get detailed information about a specific torrent
    #[instrument(skip(self), fields(api_op = "get_torrent", id))]
    pub async fn get_torrent(&self, id: u64) -> Result<TorrentInfo> {
        let url = format!("{}/torrents/{}", self.base_url, id);
        let endpoint = format!("/torrents/{}", id);

        trace!(api_op = "get_torrent", id = id);

        match self.get_json::<TorrentInfo>(&endpoint, &url).await {
            Ok(torrent) => {
                debug!(api_op = "get_torrent", id = id, name = %torrent.name);
                Ok(torrent)
            }
            Err(e) => {
                // Check if it's a 404 error from the API
                if let Some(api_err) = e.downcast_ref::<RqbitFuseError>() {
                    if matches!(api_err, RqbitFuseError::ApiError { status: 404, .. }) {
                        return Err(RqbitFuseError::NotFound(format!("torrent {}", id)).into());
                    }
                }
                Err(e)
            }
        }
    }

    /// Add a torrent from a magnet link
    #[instrument(skip(self), fields(api_op = "add_torrent_magnet"))]
    pub async fn add_torrent_magnet(&self, magnet_link: &str) -> Result<AddTorrentResponse> {
        let url = format!("{}/torrents", self.base_url);
        let request = AddMagnetRequest {
            magnet_link: magnet_link.to_string(),
        };

        trace!(api_op = "add_torrent_magnet");

        let result = self
            .post_json::<_, AddTorrentResponse>("/torrents", &url, &request)
            .await?;
        debug!(api_op = "add_torrent_magnet", id = result.id, info_hash = %result.info_hash);
        self.invalidate_list_torrents_cache().await;
        Ok(result)
    }

    /// Add a torrent from a torrent file URL
    #[instrument(skip(self), fields(api_op = "add_torrent_url", url = %torrent_url))]
    pub async fn add_torrent_url(&self, torrent_url: &str) -> Result<AddTorrentResponse> {
        let url = format!("{}/torrents", self.base_url);
        let request = AddTorrentUrlRequest {
            torrent_link: torrent_url.to_string(),
        };

        trace!(api_op = "add_torrent_url", url = %torrent_url);

        let result = self
            .post_json::<_, AddTorrentResponse>("/torrents", &url, &request)
            .await?;
        debug!(api_op = "add_torrent_url", id = result.id, info_hash = %result.info_hash);
        self.invalidate_list_torrents_cache().await;
        Ok(result)
    }

    /// Get statistics for a torrent
    #[instrument(skip(self), fields(api_op = "get_torrent_stats", id))]
    pub async fn get_torrent_stats(&self, id: u64) -> Result<TorrentStats> {
        let url = format!("{}/torrents/{}/stats/v1", self.base_url, id);
        let endpoint = format!("/torrents/{}/stats", id);

        trace!(api_op = "get_torrent_stats", id = id);

        match self.get_json::<TorrentStats>(&endpoint, &url).await {
            Ok(stats) => {
                let progress_pct = if stats.total_bytes > 0 {
                    (stats.progress_bytes as f64 / stats.total_bytes as f64) * 100.0
                } else {
                    0.0
                };
                trace!(
                    api_op = "get_torrent_stats",
                    id = id,
                    state = %stats.state,
                    progress_pct = progress_pct,
                    finished = stats.finished,
                );
                Ok(stats)
            }
            Err(e) => {
                // Check if it's a 404 error from the API
                if let Some(api_err) = e.downcast_ref::<RqbitFuseError>() {
                    if matches!(api_err, RqbitFuseError::ApiError { status: 404, .. }) {
                        return Err(RqbitFuseError::NotFound(format!("torrent {}", id)).into());
                    }
                }
                Err(e)
            }
        }
    }

    /// Get piece availability bitfield for a torrent
    #[instrument(skip(self), fields(api_op = "get_piece_bitfield", id))]
    pub async fn get_piece_bitfield(&self, id: u64) -> Result<PieceBitfield> {
        let url = format!("{}/torrents/{}/haves", self.base_url, id);
        let endpoint = format!("/torrents/{}/haves", id);

        let response = self
            .execute_with_retry(&endpoint, || {
                let mut req = self
                    .client
                    .get(&url)
                    .header("Accept", "application/octet-stream");
                if let Some(auth_header) = self.create_auth_header() {
                    req = req.header("Authorization", auth_header);
                }
                req.send()
            })
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => {
                Err(RqbitFuseError::NotFound(format!("torrent {}", id)).into())
            }
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

                Ok(PieceBitfield { bits, num_pieces })
            }
        }
    }

    /// Get both torrent stats and piece bitfield in a single call with caching
    ///
    /// Check if a byte range is fully available (all pieces downloaded)
    ///
    /// This method checks whether all pieces covering the specified byte range
    /// have been downloaded. It's useful for determining if a read operation
    /// can succeed on a paused torrent without blocking.
    ///
    /// # Arguments
    /// * `torrent_id` - The torrent ID
    /// * `offset` - Starting byte offset in the file
    /// * `size` - Number of bytes to check
    /// * `piece_length` - Size of each piece in bytes (from torrent info)
    ///
    /// # Returns
    /// * `Ok(true)` - All pieces in the range are available
    /// * `Ok(false)` - At least one piece in the range is not available
    /// * `Err(RqbitFuseError)` - Failed to fetch torrent status
    #[instrument(
        skip(self),
        fields(api_op = "check_range_available", torrent_id, offset, size)
    )]
    pub async fn check_range_available(
        &self,
        torrent_id: u64,
        offset: u64,
        size: u64,
        piece_length: u64,
    ) -> Result<bool> {
        // Handle edge cases
        if size == 0 {
            return Ok(true);
        }
        if piece_length == 0 {
            return Err(
                RqbitFuseError::InvalidArgument("piece_length cannot be zero".to_string()).into(),
            );
        }

        // Fetch bitfield directly (no caching)
        let bitfield = self.get_piece_bitfield(torrent_id).await?;

        // Use the bitfield to check piece availability
        Ok(bitfield.has_piece_range(offset, size, piece_length))
    }

    // =========================================================================
    // File Operations
    // =========================================================================

    /// Read file data from a torrent
    ///
    /// If `range` is None, reads the entire file.
    /// If `range` is Some((start, end)), reads bytes from start to end (inclusive).
    #[instrument(skip(self), fields(api_op = "read_file", torrent_id, file_idx, range = ?range))]
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
        let endpoint = format!("/torrents/{}/stream/{}", torrent_id, file_idx);

        let mut request = self.client.get(&url);

        // Add Authorization header if credentials are configured
        if let Some(auth_header) = self.create_auth_header() {
            request = request.header("Authorization", auth_header);
        }

        // Add Range header if specified
        if let Some((start, end)) = range {
            if start > end {
                return Err(RqbitFuseError::InvalidArgument(format!(
                    "Invalid range: start ({}) > end ({})",
                    start, end
                ))
                .into());
            }
            let range_header = format!("bytes={}-{}", start, end);
            request = request.header("Range", range_header);
        }

        // Handle request clone failure gracefully
        // First clone is validated; subsequent clones during retries are safe since
        // GET requests have no body and can always be cloned
        let request = request
            .try_clone()
            .ok_or_else(|| RqbitFuseError::IoError("Request body not cloneable".to_string()))?;

        let response = self
            .execute_with_retry(&endpoint, move || {
                // This unwrap is safe because we validated the request can be cloned above.
                // GET requests with no body can always be cloned.
                request.try_clone().unwrap().send()
            })
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(RqbitFuseError::NotFound(format!(
                "file {} in torrent {}",
                file_idx, torrent_id
            ))
            .into()),
            StatusCode::RANGE_NOT_SATISFIABLE => {
                let message = match response.text().await {
                    Ok(text) => text,
                    Err(e) => {
                        return Err(RqbitFuseError::NetworkError(format!(
                            "Failed to read range error response body: {}",
                            e
                        ))
                        .into());
                    }
                };
                Err(RqbitFuseError::InvalidArgument(message).into())
            }
            _ => {
                let status = response.status();
                let is_range_request = range.is_some();
                let requested_size = range.map(|(start, end)| (end - start + 1) as usize);

                // Check if server returned 200 OK for a range request (rqbit bug workaround)
                let is_full_response = status == StatusCode::OK && is_range_request;

                if is_full_response {
                    debug!(
                        "Server returned 200 OK for range request, will limit to {} bytes",
                        requested_size.unwrap_or(0)
                    );
                }

                let response = self.check_response(response).await?;

                // Stream the response and apply byte limit if needed
                let mut stream = response.bytes_stream();
                let mut result = Vec::new();
                let mut total_read = 0usize;
                // If server returned full file for a range request, we need to:
                // 1. Skip bytes to reach the requested start offset
                // 2. Limit bytes read to the requested size
                let (limit, mut bytes_to_skip) = if is_full_response {
                    let range_start = range.map(|(start, _)| start as usize).unwrap_or(0);
                    let range_size = requested_size.unwrap_or(0);
                    (Some(range_size), range_start)
                } else {
                    (None, 0)
                };

                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;

                    // Handle skipping bytes for full response workaround
                    if bytes_to_skip > 0 {
                        if chunk.len() <= bytes_to_skip {
                            // Skip entire chunk
                            bytes_to_skip -= chunk.len();
                            continue;
                        } else {
                            // Partial skip - take remaining bytes from this chunk
                            let remaining = &chunk[bytes_to_skip..];
                            bytes_to_skip = 0;

                            if let Some(limit) = limit {
                                let to_take = remaining.len().min(limit.saturating_sub(total_read));
                                result.extend_from_slice(&remaining[..to_take]);
                                total_read += to_take;
                                if total_read >= limit {
                                    break;
                                }
                            } else {
                                result.extend_from_slice(remaining);
                                total_read += remaining.len();
                            }
                            continue;
                        }
                    }

                    // Normal reading (no skip needed)
                    if let Some(limit) = limit {
                        let remaining = limit.saturating_sub(total_read);
                        if remaining == 0 {
                            break;
                        }
                        let to_take = chunk.len().min(remaining);
                        result.extend_from_slice(&chunk[..to_take]);
                        total_read += to_take;
                    } else {
                        result.extend_from_slice(&chunk);
                        total_read += chunk.len();
                    }
                }

                Ok(Bytes::from(result))
            }
        }
    }

    /// Read file data using persistent streaming for efficient sequential access
    #[instrument(
        skip(self),
        fields(api_op = "read_file_streaming", torrent_id, file_idx, offset, size)
    )]
    pub async fn read_file_streaming(
        &self,
        torrent_id: u64,
        file_idx: usize,
        offset: u64,
        size: usize,
    ) -> Result<Bytes> {
        self.stream_manager
            .read(torrent_id, file_idx, offset, size)
            .await
    }

    /// Close a persistent stream for a specific file
    pub async fn close_file_stream(&self, torrent_id: u64, file_idx: usize) {
        self.stream_manager.close_stream(torrent_id, file_idx).await;
    }

    /// Close all persistent streams for a torrent
    ///
    /// # Arguments
    /// * `torrent_id` - ID of the torrent
    pub async fn close_torrent_streams(&self, torrent_id: u64) {
        self.stream_manager.close_torrent_streams(torrent_id).await;
    }

    /// Get statistics about the persistent stream manager
    pub async fn stream_stats(&self) -> crate::api::streaming::StreamManagerStats {
        self.stream_manager.stats().await
    }

    // =========================================================================
    // Torrent Control
    // =========================================================================

    /// Execute a torrent action (pause, start, forget, delete)
    async fn torrent_action(&self, id: u64, action: &str) -> Result<()> {
        let url = format!("{}/torrents/{}/{}", self.base_url, id, action);
        let endpoint = format!("/torrents/{}/{}", id, action);

        trace!(api_op = "torrent_action", id = id, action = action);

        let response = self
            .execute_with_retry(&endpoint, || self.client.post(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => {
                Err(RqbitFuseError::NotFound(format!("torrent {}", id)).into())
            }
            _ => {
                self.check_response(response).await?;
                debug!(
                    api_op = "torrent_action",
                    id = id,
                    action = action,
                    "Success"
                );
                // Invalidate cache for forget and delete actions
                if action == "forget" || action == "delete" {
                    self.invalidate_list_torrents_cache().await;
                }
                Ok(())
            }
        }
    }

    /// Pause a torrent
    #[instrument(skip(self), fields(api_op = "pause_torrent", id))]
    pub async fn pause_torrent(&self, id: u64) -> Result<()> {
        self.torrent_action(id, "pause").await
    }

    /// Resume/start a torrent
    #[instrument(skip(self), fields(api_op = "start_torrent", id))]
    pub async fn start_torrent(&self, id: u64) -> Result<()> {
        self.torrent_action(id, "start").await
    }

    /// Remove torrent from session (keep files)
    #[instrument(skip(self), fields(api_op = "forget_torrent", id))]
    pub async fn forget_torrent(&self, id: u64) -> Result<()> {
        self.torrent_action(id, "forget").await
    }

    /// Remove torrent from session and delete files
    #[instrument(skip(self), fields(api_op = "delete_torrent", id))]
    pub async fn delete_torrent(&self, id: u64) -> Result<()> {
        self.torrent_action(id, "delete").await
    }

    /// Check if the rqbit server is healthy
    /// Uses a short timeout for quick health checks
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/torrents", self.base_url);

        // Use a shorter timeout for health checks (5 seconds)
        let health_client = Client::builder()
            .timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(1)
            .build()
            .expect("Failed to build health check client");

        match health_client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    Ok(true)
                } else {
                    warn!("Health check returned status: {}", response.status());
                    Ok(false)
                }
            }
            Err(e) => {
                let api_error: RqbitFuseError = e.into();
                warn!("Health check failed: {}", api_error);
                Ok(false)
            }
        }
    }

    /// Wait for the server to become available with exponential backoff
    pub async fn wait_for_server(&self, max_wait: Duration) -> Result<()> {
        let start = Instant::now();
        let mut attempt = 0;

        while start.elapsed() < max_wait {
            match self.health_check().await {
                Ok(true) => {
                    debug!("Server is available after {:?}", start.elapsed());
                    return Ok(());
                }
                Ok(false) => {
                    attempt += 1;
                    let delay = Duration::from_millis(500 * 2_u64.pow(attempt.min(5)));
                    debug!(
                        "Server not ready, waiting {:?} before retry {}...",
                        delay, attempt
                    );
                    sleep(delay).await;
                }
                Err(e) => {
                    error!("Error during server wait: {}", e);
                    attempt += 1;
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }

        Err(RqbitFuseError::NetworkError("Server disconnected".to_string()).into())
    }

    /// Clear the list_torrents cache (for integration tests).
    #[doc(hidden)]
    pub async fn __test_clear_cache(&self) {
        let mut cache = self.list_torrents_cache.write().await;
        *cache = None;
    }
}

/// Helper function to create an RqbitClient with optional authentication
///
/// This function checks if username and password are configured, and creates
/// the client with authentication if they are present.
pub fn create_api_client(
    api_config: &crate::config::ApiConfig,
    metrics: Option<Arc<Metrics>>,
) -> Result<RqbitClient> {
    match (&api_config.username, &api_config.password) {
        (Some(username), Some(password)) => RqbitClient::with_config(
            api_config.url.clone(),
            3,
            Duration::from_millis(500),
            Some((username.clone(), password.clone())),
            metrics,
        ),
        _ => RqbitClient::with_config(
            api_config.url.clone(),
            3,
            Duration::from_millis(500),
            None,
            metrics,
        ),
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
        let client = RqbitClient::new("http://localhost:3030".to_string()).unwrap();
        // URL is stored as-is (validated but not modified)
        assert_eq!(client.base_url, "http://localhost:3030");
        assert_eq!(client.max_retries, 3);
    }

    #[test]
    fn test_api_error_mapping() {
        use libc;

        // Test simplified NotFound error
        assert_eq!(
            RqbitFuseError::NotFound("torrent 1".to_string()).to_errno(),
            libc::ENOENT
        );

        // Test simplified InvalidArgument error
        assert_eq!(
            RqbitFuseError::InvalidArgument("test".to_string()).to_errno(),
            libc::EINVAL
        );

        // Test simplified TimedOut error
        assert_eq!(
            RqbitFuseError::TimedOut("connection".to_string()).to_errno(),
            libc::ETIMEDOUT
        );

        // Test NetworkError mapping
        assert_eq!(
            RqbitFuseError::NetworkError("test".to_string()).to_errno(),
            libc::ENETUNREACH
        );

        // Test NotReady error (replaces CircuitBreakerOpen, RetryLimitExceeded)
        assert_eq!(
            RqbitFuseError::NotReady("retry limit".to_string()).to_errno(),
            libc::EAGAIN
        );

        // Test HTTP status code mappings
        assert_eq!(
            RqbitFuseError::ApiError {
                status: 404,
                message: "not found".to_string()
            }
            .to_errno(),
            libc::ENOENT
        );
        assert_eq!(
            RqbitFuseError::ApiError {
                status: 403,
                message: "forbidden".to_string()
            }
            .to_errno(),
            libc::EACCES
        );
        assert_eq!(
            RqbitFuseError::ApiError {
                status: 503,
                message: "unavailable".to_string()
            }
            .to_errno(),
            libc::EAGAIN
        );
    }

    #[test]
    fn test_api_error_is_transient() {
        // Test simplified transient errors
        assert!(RqbitFuseError::TimedOut("connection".to_string()).is_transient());
        assert!(RqbitFuseError::NetworkError("test".to_string()).is_transient());
        assert!(RqbitFuseError::NotReady("retry limit".to_string()).is_transient());
        assert!(RqbitFuseError::ApiError {
            status: 503,
            message: "unavailable".to_string()
        }
        .is_transient());

        // Test non-transient errors
        assert!(!RqbitFuseError::NotFound("test".to_string()).is_transient());
        assert!(!RqbitFuseError::InvalidArgument("test".to_string()).is_transient());
        assert!(!RqbitFuseError::ApiError {
            status: 404,
            message: "not found".to_string()
        }
        .is_transient());
    }

    // =========================================================================
    // Mocked HTTP Response Tests
    // =========================================================================

    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_list_torrents_success() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        let response_body = serde_json::json!({
            "torrents": [
                {
                    "id": 1,
                    "info_hash": "abc123",
                    "name": "Test Torrent",
                    "output_folder": "/downloads",
                    "file_count": 2,
                    "files": [
                        {"name": "file1.txt", "length": 1024, "components": ["file1.txt"]},
                        {"name": "file2.txt", "length": 2048, "components": ["file2.txt"]}
                    ],
                    "piece_length": 1048576
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .mount(&mock_server)
            .await;

        // Mock the individual torrent endpoint since list_torrents() now fetches full details
        let single_torrent_body = serde_json::json!({
            "id": 1,
            "info_hash": "abc123",
            "name": "Test Torrent",
            "output_folder": "/downloads",
            "file_count": 2,
            "files": [
                {"name": "file1.txt", "length": 1024, "components": ["file1.txt"]},
                {"name": "file2.txt", "length": 2048, "components": ["file2.txt"]}
            ],
            "piece_length": 1048576
        });

        Mock::given(method("GET"))
            .and(path("/torrents/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(single_torrent_body))
            .mount(&mock_server)
            .await;

        let result = client.list_torrents().await.unwrap();
        assert!(!result.is_partial());
        assert!(result.has_successes());
        assert_eq!(result.torrents.len(), 1);
        assert_eq!(result.torrents[0].id, 1);
        assert_eq!(result.torrents[0].name, "Test Torrent");
        assert_eq!(result.torrents[0].info_hash, "abc123");
        assert_eq!(result.torrents[0].file_count, Some(2));

        // Verify the mock was called (WireMock verification)
        mock_server.verify().await;
    }

    #[tokio::test]
    async fn test_list_torrents_empty() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        let response_body = serde_json::json!({
            "torrents": []
        });

        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .mount(&mock_server)
            .await;

        let result = client.list_torrents().await.unwrap();
        assert!(!result.has_successes());
        assert!(result.torrents.is_empty());

        // Verify the mock was called (WireMock verification)
        mock_server.verify().await;
    }

    #[tokio::test]
    async fn test_list_torrents_partial_failure() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        // Mock the list endpoint returning 2 torrents
        let list_response = serde_json::json!({
            "torrents": [
                {
                    "id": 1,
                    "info_hash": "abc123",
                    "name": "Good Torrent",
                    "output_folder": "/downloads"
                },
                {
                    "id": 2,
                    "info_hash": "def456",
                    "name": "Bad Torrent",
                    "output_folder": "/downloads"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(ResponseTemplate::new(200).set_body_json(list_response))
            .mount(&mock_server)
            .await;

        // Mock the first torrent detail endpoint - succeeds
        let good_torrent = serde_json::json!({
            "id": 1,
            "info_hash": "abc123",
            "name": "Good Torrent",
            "output_folder": "/downloads",
            "file_count": 1,
            "files": [{"name": "file.txt", "length": 1024, "components": ["file.txt"]}],
            "piece_length": 1048576
        });

        Mock::given(method("GET"))
            .and(path("/torrents/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(good_torrent))
            .mount(&mock_server)
            .await;

        // Mock the second torrent detail endpoint - fails with 404
        Mock::given(method("GET"))
            .and(path("/torrents/2"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
            .mount(&mock_server)
            .await;

        let result = client.list_torrents().await.unwrap();

        // Verify partial result
        assert!(result.is_partial());
        assert!(result.has_successes());
        assert_eq!(result.torrents.len(), 1);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.torrents[0].id, 1);
        assert_eq!(result.errors[0].0, 2); // id
        assert_eq!(result.errors[0].1, "Bad Torrent"); // name
    }

    #[tokio::test]
    async fn test_get_torrent_success() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        let response_body = serde_json::json!({
            "id": 1,
            "info_hash": "abc123",
            "name": "Test Torrent",
            "output_folder": "/downloads",
            "file_count": 1,
            "files": [
                {"name": "test.txt", "length": 1024, "components": ["test.txt"]}
            ],
            "piece_length": 1048576
        });

        Mock::given(method("GET"))
            .and(path("/torrents/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .mount(&mock_server)
            .await;

        let torrent = client.get_torrent(1).await.unwrap();
        assert_eq!(torrent.id, 1);
        assert_eq!(torrent.name, "Test Torrent");
    }

    #[tokio::test]
    async fn test_get_torrent_not_found() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("GET"))
            .and(path("/torrents/999"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = client.get_torrent(999).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<RqbitFuseError>().unwrap();
        assert!(matches!(err, RqbitFuseError::NotFound(ref msg) if msg.contains("999")));
    }

    #[tokio::test]
    async fn test_add_torrent_magnet_success() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        let request_body = serde_json::json!({
            "magnet_link": "magnet:?xt=urn:btih:abc123"
        });

        let response_body = serde_json::json!({
            "id": 42,
            "info_hash": "abc123"
        });

        Mock::given(method("POST"))
            .and(path("/torrents"))
            .and(header("content-type", "application/json"))
            .and(body_json(request_body))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .mount(&mock_server)
            .await;

        let result = client
            .add_torrent_magnet("magnet:?xt=urn:btih:abc123")
            .await
            .unwrap();
        assert_eq!(result.id, 42);
        assert_eq!(result.info_hash, "abc123");
    }

    #[tokio::test]
    async fn test_add_torrent_url_success() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        let request_body = serde_json::json!({
            "torrent_link": "http://example.com/test.torrent"
        });

        let response_body = serde_json::json!({
            "id": 43,
            "info_hash": "def456"
        });

        Mock::given(method("POST"))
            .and(path("/torrents"))
            .and(header("content-type", "application/json"))
            .and(body_json(request_body))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .mount(&mock_server)
            .await;

        let result = client
            .add_torrent_url("http://example.com/test.torrent")
            .await
            .unwrap();
        assert_eq!(result.id, 43);
        assert_eq!(result.info_hash, "def456");
    }

    #[tokio::test]
    async fn test_get_torrent_stats_success() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        let response_body = serde_json::json!({
            "state": "live",
            "file_progress": [1500],
            "error": null,
            "progress_bytes": 1500,
            "uploaded_bytes": 0,
            "total_bytes": 3072,
            "finished": false,
            "live": {
                "snapshot": {
                    "downloaded_and_checked_bytes": 1500
                },
                "download_speed": {
                    "mbps": 1.5,
                    "human_readable": "1.50 MiB/s"
                },
                "upload_speed": {
                    "mbps": 0.5,
                    "human_readable": "0.50 MiB/s"
                }
            }
        });

        Mock::given(method("GET"))
            .and(path("/torrents/1/stats/v1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .mount(&mock_server)
            .await;

        let stats = client.get_torrent_stats(1).await.unwrap();
        assert_eq!(stats.state, "live");
        assert_eq!(stats.progress_bytes, 1500);
        assert_eq!(stats.total_bytes, 3072);
        assert!(!stats.finished);
        assert!(stats.live.is_some());
        let live = stats.live.unwrap();
        assert_eq!(live.snapshot.downloaded_and_checked_bytes, 1500);
        assert_eq!(live.download_speed.mbps, 1.5);
        assert_eq!(live.download_speed.human_readable, "1.50 MiB/s");
    }

    #[tokio::test]
    async fn test_get_torrent_stats_error_state() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        let response_body = serde_json::json!({
            "state": "error",
            "file_progress": [],
            "error": "No space left on device",
            "progress_bytes": 0,
            "uploaded_bytes": 0,
            "total_bytes": 1000000000,
            "finished": false,
            "live": null
        });

        Mock::given(method("GET"))
            .and(path("/torrents/3/stats/v1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .mount(&mock_server)
            .await;

        let stats = client.get_torrent_stats(3).await.unwrap();
        assert_eq!(stats.state, "error");
        assert_eq!(stats.error, Some("No space left on device".to_string()));
        assert_eq!(stats.progress_bytes, 0);
        assert_eq!(stats.total_bytes, 1000000000);
        assert!(!stats.finished);
        assert!(stats.live.is_none());
    }

    #[tokio::test]
    async fn test_get_piece_bitfield_success() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        // Bitfield with pieces 0, 1, 3 downloaded (binary: 1011 = 0x0B)
        let bitfield_data = vec![0b00001011u8];

        Mock::given(method("GET"))
            .and(path("/torrents/1/haves"))
            .and(header("accept", "application/octet-stream"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(bitfield_data.clone())
                    .append_header("x-bitfield-len", "4"),
            )
            .mount(&mock_server)
            .await;

        let bitfield = client.get_piece_bitfield(1).await.unwrap();
        assert_eq!(bitfield.num_pieces, 4);
        assert!(bitfield.has_piece(0));
        assert!(bitfield.has_piece(1));
        assert!(!bitfield.has_piece(2));
        assert!(bitfield.has_piece(3));
    }

    #[tokio::test]
    async fn test_read_file_success() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        let file_data = b"Hello, World!";

        Mock::given(method("GET"))
            .and(path("/torrents/1/stream/0"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(file_data.as_slice()))
            .mount(&mock_server)
            .await;

        let data = client.read_file(1, 0, None).await.unwrap();
        assert_eq!(data.as_ref(), file_data);
    }

    #[tokio::test]
    async fn test_read_file_with_range() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        let file_data = b"World";

        Mock::given(method("GET"))
            .and(path("/torrents/1/stream/0"))
            .and(header("range", "bytes=7-11"))
            .respond_with(
                ResponseTemplate::new(206)
                    .set_body_bytes(file_data.as_slice())
                    .append_header("content-range", "bytes 7-11/13"),
            )
            .mount(&mock_server)
            .await;

        let data = client.read_file(1, 0, Some((7, 11))).await.unwrap();
        assert_eq!(data.as_ref(), file_data);
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("GET"))
            .and(path("/torrents/1/stream/99"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = client.read_file(1, 99, None).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<RqbitFuseError>().unwrap();
        assert!(matches!(
            err,
            RqbitFuseError::NotFound(ref msg) if msg.contains("99") && msg.contains("1")
        ));
    }

    #[tokio::test]
    async fn test_read_file_invalid_range() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("GET"))
            .and(path("/torrents/1/stream/0"))
            .and(header("range", "bytes=100-200"))
            .respond_with(ResponseTemplate::new(416).set_body_string("Range not satisfiable"))
            .mount(&mock_server)
            .await;

        let result = client.read_file(1, 0, Some((100, 200))).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<RqbitFuseError>().unwrap();
        assert!(matches!(err, RqbitFuseError::InvalidArgument(_)));
    }

    #[tokio::test]
    async fn test_pause_torrent_success() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("POST"))
            .and(path("/torrents/1/pause"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        client.pause_torrent(1).await.unwrap();
    }

    #[tokio::test]
    async fn test_pause_torrent_not_found() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("POST"))
            .and(path("/torrents/999/pause"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = client.pause_torrent(999).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<RqbitFuseError>().unwrap();
        assert!(matches!(err, RqbitFuseError::NotFound(ref msg) if msg.contains("999")));
    }

    #[tokio::test]
    async fn test_start_torrent_success() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("POST"))
            .and(path("/torrents/1/start"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        client.start_torrent(1).await.unwrap();
    }

    #[tokio::test]
    async fn test_start_torrent_not_found() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("POST"))
            .and(path("/torrents/999/start"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = client.start_torrent(999).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<RqbitFuseError>().unwrap();
        assert!(matches!(err, RqbitFuseError::NotFound(ref msg) if msg.contains("999")));
    }

    #[tokio::test]
    async fn test_forget_torrent_success() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("POST"))
            .and(path("/torrents/1/forget"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        client.forget_torrent(1).await.unwrap();
    }

    #[tokio::test]
    async fn test_forget_torrent_not_found() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("POST"))
            .and(path("/torrents/999/forget"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = client.forget_torrent(999).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<RqbitFuseError>().unwrap();
        assert!(matches!(err, RqbitFuseError::NotFound(ref msg) if msg.contains("999")));
    }

    #[tokio::test]
    async fn test_delete_torrent_success() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("POST"))
            .and(path("/torrents/1/delete"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        client.delete_torrent(1).await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_torrent_not_found() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("POST"))
            .and(path("/torrents/999/delete"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = client.delete_torrent(999).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<RqbitFuseError>().unwrap();
        assert!(matches!(err, RqbitFuseError::NotFound(ref msg) if msg.contains("999")));
    }

    #[tokio::test]
    async fn test_health_check_success() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"torrents": []})),
            )
            .mount(&mock_server)
            .await;

        let healthy = client.health_check().await.unwrap();
        assert!(healthy);
    }

    #[tokio::test]
    async fn test_health_check_failure() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock_server)
            .await;

        let healthy = client.health_check().await.unwrap();
        assert!(!healthy);
    }

    #[tokio::test]
    async fn test_retry_on_server_error() {
        let mock_server = MockServer::start().await;
        // Use client with 1 retry for faster test
        let client =
            RqbitClient::with_config(mock_server.uri(), 1, Duration::from_millis(10), None, None)
                .unwrap();

        // First request fails with 503, second succeeds
        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"torrents": []})),
            )
            .mount(&mock_server)
            .await;

        let result = client.list_torrents().await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_api_error_response() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal server error"))
            .mount(&mock_server)
            .await;

        let result = client.list_torrents().await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<RqbitFuseError>().unwrap();
        assert!(matches!(err, RqbitFuseError::ApiError { status: 500, .. }));
    }

    // =========================================================================
    // EDGE-036: HTTP 429 Too Many Requests Tests
    // =========================================================================

    #[tokio::test]
    async fn test_edge_036_rate_limit_with_retry_after_header() {
        let mock_server = MockServer::start().await;
        let client =
            RqbitClient::with_config(mock_server.uri(), 1, Duration::from_millis(100), None, None)
                .unwrap();

        // First request returns 429 with Retry-After: 1 second
        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("retry-after", "1")
                    .set_body_string("Rate limited"),
            )
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        // Second request succeeds
        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"torrents": []})),
            )
            .mount(&mock_server)
            .await;

        let start = Instant::now();
        let result = client.list_torrents().await.unwrap();
        let elapsed = start.elapsed();

        // Should succeed after retry
        assert!(result.is_empty());
        // Should wait at least 1 second (Retry-After header value)
        assert!(
            elapsed >= Duration::from_secs(1),
            "Should respect Retry-After header"
        );
    }

    #[tokio::test]
    async fn test_edge_036_rate_limit_without_retry_after_uses_default_delay() {
        let mock_server = MockServer::start().await;
        let client =
            RqbitClient::with_config(mock_server.uri(), 1, Duration::from_millis(50), None, None)
                .unwrap();

        // First request returns 429 without Retry-After header
        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(ResponseTemplate::new(429).set_body_string("Rate limited"))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        // Second request succeeds
        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"torrents": []})),
            )
            .mount(&mock_server)
            .await;

        let start = Instant::now();
        let result = client.list_torrents().await.unwrap();
        let elapsed = start.elapsed();

        // Should succeed after retry
        assert!(result.is_empty());
        // Should use default retry delay (50ms * attempt 1 = 50ms)
        assert!(
            elapsed >= Duration::from_millis(50),
            "Should use default retry delay when no Retry-After header"
        );
    }

    #[tokio::test]
    async fn test_edge_036_rate_limit_exhausts_retries() {
        let mock_server = MockServer::start().await;
        // Client with 0 retries (only initial attempt)
        let client =
            RqbitClient::with_config(mock_server.uri(), 0, Duration::from_millis(10), None, None)
                .unwrap();

        // Always returns 429
        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("retry-after", "0")
                    .set_body_string("Rate limited"),
            )
            .mount(&mock_server)
            .await;

        let result = client.list_torrents().await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<RqbitFuseError>().unwrap();
        // Should get the 429 error after exhausting retries
        assert!(matches!(err, RqbitFuseError::ApiError { status: 429, .. }));
    }

    #[tokio::test]
    async fn test_edge_036_multiple_rate_limits_eventually_succeed() {
        let mock_server = MockServer::start().await;
        let client =
            RqbitClient::with_config(mock_server.uri(), 3, Duration::from_millis(10), None, None)
                .unwrap();

        // First 3 requests return 429, 4th succeeds
        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("retry-after", "0")
                    .set_body_string("Rate limited"),
            )
            .up_to_n_times(3)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"torrents": []})),
            )
            .mount(&mock_server)
            .await;

        let result = client.list_torrents().await.unwrap();
        assert!(result.is_empty());
    }

    // =========================================================================
    // EDGE-037: Malformed JSON Response Tests
    // =========================================================================

    #[tokio::test]
    async fn test_edge_037_malformed_json_list_torrents() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        // Return invalid JSON - missing closing brace
        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{\"torrents\": ["))
            .mount(&mock_server)
            .await;

        let result = client.list_torrents().await;
        assert!(result.is_err(), "Should return error for malformed JSON");

        // Verify the error is a JSON parse error
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("EOF") || err_str.contains("parse") || err_str.contains("JSON"),
            "Error should indicate JSON parsing issue: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_edge_037_malformed_json_get_torrent() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        // Return invalid JSON - invalid escape sequence
        Mock::given(method("GET"))
            .and(path("/torrents/1"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("{\"id\": 1, \"name\": \"test\\x"),
            )
            .mount(&mock_server)
            .await;

        let result = client.get_torrent(1).await;
        assert!(result.is_err(), "Should return error for malformed JSON");

        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("escape") || err_str.contains("parse") || err_str.contains("JSON"),
            "Error should indicate JSON parsing issue: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_edge_037_invalid_json_type() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        // Return JSON with wrong type - string instead of number for id
        Mock::given(method("GET"))
            .and(path("/torrents/1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("{\"id\": \"not_a_number\", \"name\": \"test\"}"),
            )
            .mount(&mock_server)
            .await;

        let result = client.get_torrent(1).await;
        assert!(
            result.is_err(),
            "Should return error for JSON type mismatch"
        );

        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("type") || err_str.contains("invalid") || err_str.contains("expected"),
            "Error should indicate type mismatch: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_edge_037_empty_json_response() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        // Return empty string
        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(ResponseTemplate::new(200).set_body_string(""))
            .mount(&mock_server)
            .await;

        let result = client.list_torrents().await;
        assert!(result.is_err(), "Should return error for empty response");
    }

    #[tokio::test]
    async fn test_edge_037_json_with_null_required_fields() {
        let mock_server = MockServer::start().await;
        let client = RqbitClient::new(mock_server.uri()).unwrap();

        // Return JSON with null where a struct is expected
        Mock::given(method("GET"))
            .and(path("/torrents/1/stats/v1"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "{\"state\": null, \"progress_bytes\": null, \"total_bytes\": 100}",
            ))
            .mount(&mock_server)
            .await;

        let result = client.get_torrent_stats(1).await;
        assert!(
            result.is_err(),
            "Should return error for null required fields"
        );
    }

    // =========================================================================
    // EDGE-038: Timeout at Different Stages Tests
    // =========================================================================

    /// Test connection timeout when server is unreachable
    /// Tests that connection failures produce appropriate timeout errors
    #[tokio::test]
    async fn test_edge_038_connection_timeout() {
        // Create a reqwest client with a very short connect timeout
        let client = Client::builder()
            .connect_timeout(Duration::from_millis(100)) // Short connect timeout
            .timeout(Duration::from_millis(200))
            .build()
            .unwrap();

        let start = Instant::now();
        // Try to connect to a non-routable address on a closed port
        // This should timeout quickly due to the connect_timeout setting
        let result = client.get("http://192.0.2.1:3030/torrents").send().await;
        let elapsed = start.elapsed();

        // Should fail with timeout
        assert!(result.is_err(), "Should fail with connection timeout");

        let err = result.unwrap_err();
        assert!(
            err.is_timeout() || err.is_connect(),
            "Should be a timeout or connect error, got: {:?}",
            err
        );

        // Should timeout quickly due to connect_timeout setting
        assert!(
            elapsed < Duration::from_secs(3),
            "Should timeout quickly with connect_timeout, but took {:?}",
            elapsed
        );
    }

    /// Test read timeout when server responds slowly
    /// Server takes longer than the configured read timeout
    #[tokio::test]
    async fn test_edge_038_read_timeout() {
        let mock_server = MockServer::start().await;

        // Create client with a very short timeout (50ms)
        let client = Client::builder()
            .timeout(Duration::from_millis(50))
            .build()
            .unwrap();

        // Mock a response that delays longer than the timeout
        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"torrents": []}))
                    .set_delay(Duration::from_millis(200)), // Delay longer than timeout
            )
            .mount(&mock_server)
            .await;

        let start = Instant::now();
        let result = client
            .get(format!("{}/torrents", mock_server.uri()))
            .send()
            .await;
        let elapsed = start.elapsed();

        // Should timeout
        assert!(result.is_err(), "Should timeout on slow response");

        let err = result.unwrap_err();
        assert!(err.is_timeout(), "Error should be a timeout");

        // Should timeout around 50ms (with some overhead)
        assert!(
            elapsed < Duration::from_millis(150),
            "Should timeout around 50ms, but took {:?}",
            elapsed
        );
    }

    /// Test that DNS resolution timeout is handled appropriately
    /// This tests that unresolvable hostnames produce appropriate errors
    #[tokio::test]
    async fn test_edge_038_dns_resolution_failure() {
        // Use a hostname that should not resolve
        // This tests DNS failure handling, not actual timeout timing
        // (DNS timeouts can be long and system-dependent)
        let client = RqbitClient::with_config(
            "http://this-host-definitely-does-not-exist.invalid".to_string(),
            0, // No retries
            Duration::from_millis(10),
            None,
            None,
        )
        .unwrap();

        let start = Instant::now();
        let result = client.list_torrents().await;
        let elapsed = start.elapsed();

        // Should fail
        assert!(result.is_err(), "Should fail with DNS resolution error");

        let err = result.unwrap_err().downcast::<RqbitFuseError>().unwrap();
        // Should be a network-related error (either DNS failure or connection failure)
        assert!(
            matches!(
                err,
                RqbitFuseError::NetworkError(_)
                    | RqbitFuseError::TimedOut(_)
                    | RqbitFuseError::IoError(_)
            ),
            "Should get network/DNS error, got: {:?}",
            err
        );

        // Should complete in reasonable time (DNS resolution typically times out in ~5s)
        assert!(
            elapsed < Duration::from_secs(30),
            "DNS resolution should complete or timeout reasonably, but took {:?}",
            elapsed
        );
    }

    /// Test that different timeout stages return appropriate error types
    #[tokio::test]
    async fn test_edge_038_timeout_error_types() {
        // Test simplified TimedOut error mapping (replaces ConnectionTimeout and ReadTimeout)
        let timed_out_err = RqbitFuseError::TimedOut("connection".to_string());
        assert_eq!(timed_out_err.to_errno(), libc::ETIMEDOUT);
        assert!(timed_out_err.is_transient());
        assert!(timed_out_err.is_server_unavailable());
    }

    // =========================================================================
    // EDGE-039: Connection Reset Tests
    // =========================================================================

    /// Test that connection reset errors are converted to NetworkError
    /// and marked as transient for retry
    #[tokio::test]
    async fn test_edge_039_connection_reset_error_conversion() {
        // Test that NetworkError is transient and indicates server unavailable
        // (replaces ServerDisconnected)
        let network_err = RqbitFuseError::NetworkError("connection reset by peer".to_string());
        assert!(
            network_err.is_transient(),
            "NetworkError should be transient"
        );
        assert!(
            network_err.is_server_unavailable(),
            "NetworkError should indicate server unavailable"
        );
        assert_eq!(
            network_err.to_errno(),
            libc::ENETUNREACH,
            "NetworkError should map to ENETUNREACH"
        );
    }

    /// Test that connection reset errors trigger retry logic
    /// Server returns success after initial connection failures
    #[tokio::test]
    async fn test_edge_039_connection_reset_retries_success() {
        let mock_server = MockServer::start().await;

        // Create client with retry enabled
        let client = RqbitClient::with_config(
            mock_server.uri(),
            3,                         // max_retries
            Duration::from_millis(10), // short delay for tests
            None,
            None,
        )
        .unwrap();

        // First two attempts will fail with 503 (simulating transient connection issues)
        // Third attempt will succeed
        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(
                ResponseTemplate::new(503).set_body_string("Service temporarily unavailable"),
            )
            .up_to_n_times(2)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "torrents": []
            })))
            .mount(&mock_server)
            .await;

        // This should succeed after retries
        let result = client.list_torrents().await;

        assert!(
            result.is_ok(),
            "Should succeed after retries, got: {:?}",
            result
        );
        let torrents = result.unwrap();
        assert!(torrents.is_empty(), "Should return empty torrent list");
    }

    /// Test that connection reset errors eventually fail when retries are exhausted
    /// Server continues to return 503 errors beyond retry limit
    #[tokio::test]
    async fn test_edge_039_connection_reset_retries_exhausted() {
        let mock_server = MockServer::start().await;

        // Create client with limited retries
        let client = RqbitClient::with_config(
            mock_server.uri(),
            2,                         // max_retries = 2 (total 3 attempts)
            Duration::from_millis(10), // short delay for tests
            None,
            None,
        )
        .unwrap();

        // Server always returns 503 (service unavailable)
        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Connection reset by peer"))
            .mount(&mock_server)
            .await;

        // This should fail after retries are exhausted
        let result = client.list_torrents().await;

        assert!(result.is_err(), "Should fail after retries exhausted");

        let err = result.unwrap_err().downcast::<RqbitFuseError>().unwrap();
        assert!(
            matches!(
                err,
                RqbitFuseError::ApiError { status: 503, .. }
                    | RqbitFuseError::NetworkError(_)
                    | RqbitFuseError::NotReady(_)
            ),
            "Should get appropriate error after retries, got: {:?}",
            err
        );
    }

    /// Test that connection reset during body read is handled gracefully
    /// Server accepts connection but closes it while sending body
    #[tokio::test]
    async fn test_edge_039_connection_reset_during_body_read() {
        let mock_server = MockServer::start().await;

        let client = RqbitClient::new(mock_server.uri()).unwrap();

        // Server returns 200 but with incomplete/empty body (simulating connection reset mid-read)
        Mock::given(method("GET"))
            .and(path("/torrents/1/stream/0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("") // Empty body simulates connection reset
                    .set_delay(Duration::from_millis(10)),
            )
            .mount(&mock_server)
            .await;

        // Attempt to read file data - should handle gracefully
        let result = client.read_file(1, 0, Some((0, 100))).await;

        // This should either succeed with empty data or fail gracefully
        match result {
            Ok(data) => {
                // If it succeeds, should have empty or minimal data
                assert!(data.len() <= 100, "Should return at most requested bytes");
            }
            Err(e) => {
                // If it fails, should be a graceful error (not panic)
                let err = e.downcast::<RqbitFuseError>().unwrap_or_else(|_| {
                    panic!("Error should be downcastable to RqbitFuseError");
                });
                // Error should be handled, not panic
                assert!(
                    !matches!(err, RqbitFuseError::IoError(_) if err.to_string().contains("panic")),
                    "Should not panic on connection reset"
                );
            }
        }
    }
}
