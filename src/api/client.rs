use crate::api::circuit_breaker::{CircuitBreaker, CircuitState};
use crate::api::streaming::PersistentStreamManager;
use crate::api::types::*;
use crate::metrics::ApiMetrics;
use anyhow::{Context, Result};
use base64::Engine;
use bytes::Bytes;
use reqwest::{Client, StatusCode};

use futures::stream::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, info, instrument, trace, warn};

/// Combined torrent status and piece bitfield information
#[derive(Debug, Clone)]
pub struct TorrentStatusWithBitfield {
    pub stats: TorrentStats,
    pub bitfield: PieceBitfield,
}

/// HTTP client for interacting with rqbit server
pub struct RqbitClient {
    client: Client,
    /// Base URL for the rqbit API server (validated at construction)
    base_url: String,
    max_retries: u32,
    retry_delay: Duration,
    /// Circuit breaker for resilience
    circuit_breaker: CircuitBreaker,
    /// Metrics collection
    metrics: Arc<ApiMetrics>,
    /// Persistent stream manager for efficient sequential reads
    stream_manager: PersistentStreamManager,
    /// Optional authentication credentials for HTTP Basic Auth
    auth_credentials: Option<(String, String)>,
    /// Cache for list_torrents results to avoid N+1 queries
    list_torrents_cache: Arc<RwLock<Option<(Instant, ListTorrentsResult)>>>,
    /// TTL for list_torrents cache
    list_torrents_cache_ttl: Duration,
    /// Cache for torrent status with bitfield (torrent_id -> (cached_at, result))
    status_bitfield_cache: Arc<RwLock<HashMap<u64, (Instant, TorrentStatusWithBitfield)>>>,
    /// TTL for status bitfield cache
    status_bitfield_cache_ttl: Duration,
}

impl RqbitClient {
    /// Create a new RqbitClient with default configuration
    pub fn new(base_url: String, metrics: Arc<ApiMetrics>) -> Result<Self> {
        Self::with_config(base_url, 3, Duration::from_millis(500), None, metrics)
    }

    /// Create a new RqbitClient with authentication
    pub fn with_auth(
        base_url: String,
        username: String,
        password: String,
        metrics: Arc<ApiMetrics>,
    ) -> Result<Self> {
        Self::with_config(
            base_url,
            3,
            Duration::from_millis(500),
            Some((username, password)),
            metrics,
        )
    }

    /// Create a new RqbitClient with custom retry configuration
    pub fn with_config(
        base_url: String,
        max_retries: u32,
        retry_delay: Duration,
        auth_credentials: Option<(String, String)>,
        metrics: Arc<ApiMetrics>,
    ) -> Result<Self> {
        // Validate URL at construction time (fail fast on invalid URL)
        let _ = reqwest::Url::parse(&base_url)
            .map_err(|e| ApiError::ClientInitializationError(format!("Invalid URL: {}", e)))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .pool_max_idle_per_host(10)
            .build()
            .map_err(|e| ApiError::ClientInitializationError(e.to_string()))?;

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
            circuit_breaker: CircuitBreaker::new(5, Duration::from_secs(30)),
            metrics,
            stream_manager,
            auth_credentials,
            list_torrents_cache: Arc::new(RwLock::new(None)),
            list_torrents_cache_ttl: Duration::from_secs(30),
            status_bitfield_cache: Arc::new(RwLock::new(HashMap::new())),
            status_bitfield_cache_ttl: Duration::from_secs(5),
        })
    }

    /// Create a new RqbitClient with custom retry and circuit breaker configuration
    pub fn with_circuit_breaker(
        base_url: String,
        max_retries: u32,
        retry_delay: Duration,
        failure_threshold: u32,
        circuit_timeout: Duration,
        auth_credentials: Option<(String, String)>,
        metrics: Arc<ApiMetrics>,
    ) -> Result<Self> {
        // Validate URL at construction time (fail fast on invalid URL)
        let _ = reqwest::Url::parse(&base_url)
            .map_err(|e| ApiError::ClientInitializationError(format!("Invalid URL: {}", e)))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .pool_max_idle_per_host(10)
            .build()
            .map_err(|e| ApiError::ClientInitializationError(e.to_string()))?;

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
            circuit_breaker: CircuitBreaker::new(failure_threshold, circuit_timeout),
            metrics,
            stream_manager,
            auth_credentials,
            list_torrents_cache: Arc::new(RwLock::new(None)),
            list_torrents_cache_ttl: Duration::from_secs(30),
            status_bitfield_cache: Arc::new(RwLock::new(HashMap::new())),
            status_bitfield_cache_ttl: Duration::from_secs(5),
        })
    }

    /// Get the current circuit breaker state
    pub async fn circuit_state(&self) -> CircuitState {
        self.circuit_breaker.state().await
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

    /// Helper method to execute a request with retry logic and circuit breaker
    async fn execute_with_retry<F, Fut>(
        &self,
        endpoint: &str,
        operation: F,
    ) -> Result<reqwest::Response>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = reqwest::Result<reqwest::Response>>,
    {
        let start_time = Instant::now();
        self.metrics.record_request(endpoint);

        // Check circuit breaker first
        if !self.circuit_breaker.can_execute().await {
            self.metrics
                .record_failure(endpoint, "circuit_breaker_open");
            return Err(ApiError::CircuitBreakerOpen.into());
        }

        let mut last_error = None;
        let mut final_result = None;

        for attempt in 0..=self.max_retries {
            match operation().await {
                Ok(response) => {
                    // Check if we got a server error that might be transient
                    let status = response.status();
                    if status.is_server_error() && attempt < self.max_retries {
                        self.metrics.record_retry(endpoint, attempt + 1);
                        warn!(
                            endpoint = endpoint,
                            status = status.as_u16(),
                            attempt = attempt + 1,
                            max_attempts = self.max_retries + 1,
                            "Server error, retrying..."
                        );
                        sleep(self.retry_delay * (attempt + 1)).await;
                        continue;
                    }
                    final_result = Some(Ok(response));
                    break;
                }
                Err(e) => {
                    let api_error: ApiError = e.into();
                    last_error = Some(api_error.clone());

                    // Check if error is transient and we should retry
                    if api_error.is_transient() && attempt < self.max_retries {
                        self.metrics.record_retry(endpoint, attempt + 1);
                        warn!(
                            endpoint = endpoint,
                            attempt = attempt + 1,
                            max_attempts = self.max_retries + 1,
                            error = %api_error,
                            "Transient error, retrying"
                        );
                        sleep(self.retry_delay * (attempt + 1)).await;
                    } else {
                        // Non-transient error or retries exhausted
                        final_result = Some(Err(api_error));
                        break;
                    }
                }
            }
        }

        // Record result in circuit breaker and metrics
        match final_result {
            Some(Ok(response)) => {
                self.circuit_breaker.record_success().await;
                self.metrics.record_success(endpoint, start_time.elapsed());
                Ok(response)
            }
            Some(Err(api_error)) => {
                if api_error.is_transient() {
                    self.circuit_breaker.record_failure().await;
                }
                self.metrics
                    .record_failure(endpoint, &api_error.to_string());
                Err(api_error.into())
            }
            None => {
                // All retries exhausted with transient errors
                self.circuit_breaker.record_failure().await;
                let error = last_error.unwrap_or(ApiError::RetryLimitExceeded);
                self.metrics.record_failure(endpoint, &error.to_string());
                Err(error.into())
            }
        }
    }

    /// Helper to check response status and convert errors
    async fn check_response(&self, response: reqwest::Response) -> Result<reqwest::Response> {
        let status = response.status();

        if status.is_success() || status == StatusCode::PARTIAL_CONTENT {
            Ok(response)
        } else if status == StatusCode::UNAUTHORIZED {
            let message = response.text().await.unwrap_or_default();
            Err(ApiError::AuthenticationError(format!(
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
                    return Err(ApiError::NetworkError(format!(
                        "Failed to read error response body: {}",
                        e
                    ))
                    .into());
                }
            };
            Err(ApiError::ApiError {
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

    /// List all torrents in the session
    ///
    /// This fetches full details for each torrent since the /torrents endpoint
    /// returns a simplified structure without the files field.
    ///
    /// # Returns
    /// Returns a `ListTorrentsResult` containing both successfully loaded torrents
    /// and any errors that occurred. Callers should check `is_partial()` to detect
    /// if some torrents failed to load.
    ///
    /// Uses caching to avoid N+1 queries on repeated calls within the TTL window.
    ///
    /// # Errors
    /// Returns an error only if the initial list request fails. Individual torrent
    /// fetch failures are collected in the result's `errors` field.
    #[instrument(skip(self), fields(api_op = "list_torrents"))]
    pub async fn list_torrents(&self) -> Result<ListTorrentsResult> {
        // Check cache first
        {
            let cache = self.list_torrents_cache.read().await;
            if let Some((cached_at, cached_result)) = cache.as_ref() {
                if cached_at.elapsed() < self.list_torrents_cache_ttl {
                    debug!("list_torrents: cache hit");
                    return Ok(cached_result.clone());
                }
            }
        }

        // Cache miss or expired - fetch fresh data
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
                    // Convert anyhow::Error to ApiError for storage
                    let api_err = if let Some(api_err) = e.downcast_ref::<ApiError>() {
                        api_err.clone()
                    } else {
                        ApiError::HttpError(e.to_string())
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
                if let Some(api_err) = e.downcast_ref::<ApiError>() {
                    if matches!(api_err, ApiError::ApiError { status: 404, .. }) {
                        return Err(ApiError::TorrentNotFound(id).into());
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
                if let Some(api_err) = e.downcast_ref::<ApiError>() {
                    if matches!(api_err, ApiError::ApiError { status: 404, .. }) {
                        return Err(ApiError::TorrentNotFound(id).into());
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

                Ok(PieceBitfield { bits, num_pieces })
            }
        }
    }

    /// Get both torrent stats and piece bitfield in a single call with caching
    ///
    /// This method fetches both the torrent statistics and piece bitfield in parallel,
    /// caches the result for 5 seconds, and returns them together. This is useful
    /// for checking piece availability before attempting to read from a torrent.
    ///
    /// # Arguments
    /// * `id` - The torrent ID
    ///
    /// # Returns
    /// A `TorrentStatusWithBitfield` struct containing both stats and bitfield
    #[instrument(skip(self), fields(api_op = "get_torrent_status_with_bitfield", id))]
    pub async fn get_torrent_status_with_bitfield(
        &self,
        id: u64,
    ) -> Result<TorrentStatusWithBitfield> {
        // Check cache first
        {
            let cache = self.status_bitfield_cache.read().await;
            if let Some((cached_at, cached_result)) = cache.get(&id) {
                if cached_at.elapsed() < self.status_bitfield_cache_ttl {
                    debug!(
                        api_op = "get_torrent_status_with_bitfield",
                        id = id,
                        "cache hit"
                    );
                    return Ok(cached_result.clone());
                }
            }
        }

        // Cache miss or expired - fetch fresh data in parallel
        debug!(
            api_op = "get_torrent_status_with_bitfield",
            id = id,
            "cache miss, fetching fresh data"
        );

        let stats_future = self.get_torrent_stats(id);
        let bitfield_future = self.get_piece_bitfield(id);

        let (stats, bitfield) = tokio::try_join!(stats_future, bitfield_future)?;

        let result = TorrentStatusWithBitfield { stats, bitfield };

        // Cache the result
        {
            let mut cache = self.status_bitfield_cache.write().await;
            cache.insert(id, (Instant::now(), result.clone()));
        }

        Ok(result)
    }

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
    /// * `Err(ApiError)` - Failed to fetch torrent status
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
            return Err(ApiError::InvalidRange("piece_length cannot be zero".to_string()).into());
        }

        // Get cached status with bitfield
        let status = self.get_torrent_status_with_bitfield(torrent_id).await?;

        // Use the bitfield to check piece availability
        Ok(status.bitfield.has_piece_range(offset, size, piece_length))
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
                return Err(ApiError::InvalidRange(format!(
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
            .ok_or_else(|| ApiError::RequestCloneError("Request body not cloneable".to_string()))?;

        let response = self
            .execute_with_retry(&endpoint, move || {
                // This unwrap is safe because we validated the request can be cloned above.
                // GET requests with no body can always be cloned.
                request.try_clone().unwrap().send()
            })
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::FileNotFound {
                torrent_id,
                file_idx,
            }
            .into()),
            StatusCode::RANGE_NOT_SATISFIABLE => {
                let message = match response.text().await {
                    Ok(text) => text,
                    Err(e) => {
                        return Err(ApiError::NetworkError(format!(
                            "Failed to read range error response body: {}",
                            e
                        ))
                        .into());
                    }
                };
                Err(ApiError::InvalidRange(message).into())
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
    ///
    /// This method maintains open HTTP connections and reuses them for sequential reads,
    /// significantly improving performance when rqbit ignores Range headers and returns
    /// full file responses.
    ///
    /// # Arguments
    /// * `torrent_id` - ID of the torrent
    /// * `file_idx` - Index of the file within the torrent
    /// * `offset` - Byte offset to start reading from
    /// * `size` - Number of bytes to read
    ///
    /// # Returns
    /// * `Result<Bytes>` - The requested data
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
    ///
    /// # Arguments
    /// * `torrent_id` - ID of the torrent
    /// * `file_idx` - Index of the file within the torrent
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
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
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
                    self.circuit_breaker.record_success().await;
                    Ok(true)
                } else {
                    warn!("Health check returned status: {}", response.status());
                    self.circuit_breaker.record_failure().await;
                    Ok(false)
                }
            }
            Err(e) => {
                let api_error: ApiError = e.into();
                if api_error.is_transient() {
                    self.circuit_breaker.record_failure().await;
                }
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

        Err(ApiError::ServerDisconnected.into())
    }
}

/// Helper function to create an RqbitClient with optional authentication
///
/// This function checks if username and password are configured, and creates
/// the client with authentication if they are present.
pub fn create_api_client(
    api_config: &crate::config::ApiConfig,
    metrics: Arc<crate::metrics::ApiMetrics>,
) -> Result<RqbitClient> {
    match (&api_config.username, &api_config.password) {
        (Some(username), Some(password)) => RqbitClient::with_auth(
            api_config.url.clone(),
            username.clone(),
            password.clone(),
            metrics,
        ),
        _ => RqbitClient::new(api_config.url.clone(), metrics),
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
        use crate::metrics::ApiMetrics;
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new("http://localhost:3030".to_string(), metrics).unwrap();
        // URL is stored as-is (validated but not modified)
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

        // Test new error mappings
        assert_eq!(ApiError::ConnectionTimeout.to_fuse_error(), libc::EAGAIN);
        assert_eq!(ApiError::ReadTimeout.to_fuse_error(), libc::EAGAIN);
        assert_eq!(ApiError::ServerDisconnected.to_fuse_error(), libc::ENOTCONN);
        assert_eq!(
            ApiError::NetworkError("test".to_string()).to_fuse_error(),
            libc::ENETUNREACH
        );
        assert_eq!(ApiError::CircuitBreakerOpen.to_fuse_error(), libc::EAGAIN);
        assert_eq!(ApiError::RetryLimitExceeded.to_fuse_error(), libc::EAGAIN);

        // Test HTTP status code mappings
        assert_eq!(
            ApiError::ApiError {
                status: 404,
                message: "not found".to_string()
            }
            .to_fuse_error(),
            libc::ENOENT
        );
        assert_eq!(
            ApiError::ApiError {
                status: 403,
                message: "forbidden".to_string()
            }
            .to_fuse_error(),
            libc::EACCES
        );
        assert_eq!(
            ApiError::ApiError {
                status: 503,
                message: "unavailable".to_string()
            }
            .to_fuse_error(),
            libc::EAGAIN
        );
    }

    #[test]
    fn test_api_error_is_transient() {
        assert!(ApiError::ConnectionTimeout.is_transient());
        assert!(ApiError::ReadTimeout.is_transient());
        assert!(ApiError::ServerDisconnected.is_transient());
        assert!(ApiError::NetworkError("test".to_string()).is_transient());
        assert!(ApiError::CircuitBreakerOpen.is_transient());
        assert!(ApiError::RetryLimitExceeded.is_transient());
        assert!(ApiError::ApiError {
            status: 503,
            message: "unavailable".to_string()
        }
        .is_transient());

        assert!(!ApiError::TorrentNotFound(1).is_transient());
        assert!(!ApiError::InvalidRange("test".to_string()).is_transient());
        assert!(!ApiError::ApiError {
            status: 404,
            message: "not found".to_string()
        }
        .is_transient());
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let cb = CircuitBreaker::new(3, Duration::from_millis(100));

        // Initially closed
        assert!(cb.can_execute().await);
        assert_eq!(cb.state().await, CircuitState::Closed);

        // Record failures
        cb.record_failure().await;
        cb.record_failure().await;
        assert!(cb.can_execute().await); // Still closed

        cb.record_failure().await; // Third failure opens circuit
        assert!(!cb.can_execute().await); // Circuit is open
        assert_eq!(cb.state().await, CircuitState::Open);

        // Wait for timeout
        sleep(Duration::from_millis(150)).await;
        assert!(cb.can_execute().await); // Now half-open
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Record success closes circuit
        cb.record_success().await;
        assert!(cb.can_execute().await);
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    // =========================================================================
    // Mocked HTTP Response Tests
    // =========================================================================

    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_list_torrents_success() {
        let mock_server = MockServer::start().await;
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

        Mock::given(method("GET"))
            .and(path("/torrents/999"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = client.get_torrent(999).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<ApiError>().unwrap();
        assert!(matches!(err, ApiError::TorrentNotFound(999)));
    }

    #[tokio::test]
    async fn test_add_torrent_magnet_success() {
        let mock_server = MockServer::start().await;
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

        Mock::given(method("GET"))
            .and(path("/torrents/1/stream/99"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = client.read_file(1, 99, None).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<ApiError>().unwrap();
        assert!(matches!(
            err,
            ApiError::FileNotFound {
                torrent_id: 1,
                file_idx: 99
            }
        ));
    }

    #[tokio::test]
    async fn test_read_file_invalid_range() {
        let mock_server = MockServer::start().await;
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

        Mock::given(method("GET"))
            .and(path("/torrents/1/stream/0"))
            .and(header("range", "bytes=100-200"))
            .respond_with(ResponseTemplate::new(416).set_body_string("Range not satisfiable"))
            .mount(&mock_server)
            .await;

        let result = client.read_file(1, 0, Some((100, 200))).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<ApiError>().unwrap();
        assert!(matches!(err, ApiError::InvalidRange(_)));
    }

    #[tokio::test]
    async fn test_pause_torrent_success() {
        let mock_server = MockServer::start().await;
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

        Mock::given(method("POST"))
            .and(path("/torrents/999/pause"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = client.pause_torrent(999).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<ApiError>().unwrap();
        assert!(matches!(err, ApiError::TorrentNotFound(999)));
    }

    #[tokio::test]
    async fn test_start_torrent_success() {
        let mock_server = MockServer::start().await;
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

        Mock::given(method("POST"))
            .and(path("/torrents/999/start"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = client.start_torrent(999).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<ApiError>().unwrap();
        assert!(matches!(err, ApiError::TorrentNotFound(999)));
    }

    #[tokio::test]
    async fn test_forget_torrent_success() {
        let mock_server = MockServer::start().await;
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

        Mock::given(method("POST"))
            .and(path("/torrents/999/forget"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = client.forget_torrent(999).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<ApiError>().unwrap();
        assert!(matches!(err, ApiError::TorrentNotFound(999)));
    }

    #[tokio::test]
    async fn test_delete_torrent_success() {
        let mock_server = MockServer::start().await;
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

        Mock::given(method("POST"))
            .and(path("/torrents/999/delete"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = client.delete_torrent(999).await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<ApiError>().unwrap();
        assert!(matches!(err, ApiError::TorrentNotFound(999)));
    }

    #[tokio::test]
    async fn test_health_check_success() {
        let mock_server = MockServer::start().await;
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

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
        let metrics = Arc::new(ApiMetrics::new());
        // Use client with 1 retry for faster test
        let client = RqbitClient::with_config(
            mock_server.uri(),
            1,
            Duration::from_millis(10),
            None,
            metrics,
        )
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
        let metrics = Arc::new(ApiMetrics::new());
        let client = RqbitClient::new(mock_server.uri(), metrics).unwrap();

        Mock::given(method("GET"))
            .and(path("/torrents"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal server error"))
            .mount(&mock_server)
            .await;

        let result = client.list_torrents().await;
        assert!(result.is_err());
        let err = result.unwrap_err().downcast::<ApiError>().unwrap();
        assert!(matches!(err, ApiError::ApiError { status: 500, .. }));
    }
}
