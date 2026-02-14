use crate::api::types::*;
use crate::metrics::ApiMetrics;
use anyhow::{Context, Result};
use bytes::Bytes;
use reqwest::{Client, StatusCode};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, trace, warn};

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation, requests allowed
    Closed,
    /// Failure threshold reached, requests blocked
    Open,
    /// Testing if service recovered
    HalfOpen,
}

/// Circuit breaker for handling cascading failures
pub struct CircuitBreaker {
    /// Current state of the circuit
    state: Arc<RwLock<CircuitState>>,
    /// Number of consecutive failures
    failure_count: AtomicU32,
    /// Threshold before opening circuit
    failure_threshold: u32,
    /// Duration to wait before attempting recovery
    timeout: Duration,
    /// Time when circuit was opened
    opened_at: Arc<RwLock<Option<Instant>>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with default settings
    pub fn new(failure_threshold: u32, timeout: Duration) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: AtomicU32::new(0),
            failure_threshold,
            timeout,
            opened_at: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if request is allowed
    pub async fn can_execute(&self) -> bool {
        let state = *self.state.read().await;
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                let opened_at = *self.opened_at.read().await;
                if let Some(time) = opened_at {
                    if time.elapsed() >= self.timeout {
                        // Transition to half-open
                        *self.state.write().await = CircuitState::HalfOpen;
                        debug!("Circuit breaker transitioning to half-open");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful request
    pub async fn record_success(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
        let mut state = self.state.write().await;
        if *state != CircuitState::Closed {
            debug!("Circuit breaker closing");
            *state = CircuitState::Closed;
            *self.opened_at.write().await = None;
        }
    }

    /// Record a failed request
    pub async fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        if count >= self.failure_threshold {
            let mut state = self.state.write().await;
            if *state == CircuitState::Closed || *state == CircuitState::HalfOpen {
                warn!(
                    "Circuit breaker opened after {} consecutive failures",
                    count
                );
                *state = CircuitState::Open;
                *self.opened_at.write().await = Some(Instant::now());
            }
        }
    }

    /// Get current state
    pub async fn state(&self) -> CircuitState {
        *self.state.read().await
    }
}

/// HTTP client for interacting with rqbit server
pub struct RqbitClient {
    client: Client,
    base_url: String,
    max_retries: u32,
    retry_delay: Duration,
    /// Circuit breaker for resilience
    circuit_breaker: CircuitBreaker,
    /// Metrics collection
    metrics: Arc<ApiMetrics>,
}

impl RqbitClient {
    /// Create a new RqbitClient with default configuration
    pub fn new(base_url: String, metrics: Arc<ApiMetrics>) -> Self {
        Self::with_config(base_url, 3, Duration::from_millis(500), metrics)
    }

    /// Create a new RqbitClient with custom retry configuration
    pub fn with_config(base_url: String, max_retries: u32, retry_delay: Duration, metrics: Arc<ApiMetrics>) -> Self {
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
            circuit_breaker: CircuitBreaker::new(5, Duration::from_secs(30)),
            metrics,
        }
    }

    /// Create a new RqbitClient with custom retry and circuit breaker configuration
    pub fn with_circuit_breaker(
        base_url: String,
        max_retries: u32,
        retry_delay: Duration,
        failure_threshold: u32,
        circuit_timeout: Duration,
        metrics: Arc<ApiMetrics>,
    ) -> Self {
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
            circuit_breaker: CircuitBreaker::new(failure_threshold, circuit_timeout),
            metrics,
        }
    }

    /// Get the current circuit breaker state
    pub async fn circuit_state(&self) -> CircuitState {
        self.circuit_breaker.state().await
    }

    /// Helper method to execute a request with retry logic and circuit breaker
    async fn execute_with_retry<F, Fut>(&self, endpoint: &str, operation: F) -> Result<reqwest::Response>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = reqwest::Result<reqwest::Response>>,
    {
        let start_time = Instant::now();
        self.metrics.record_request(endpoint);

        // Check circuit breaker first
        if !self.circuit_breaker.can_execute().await {
            self.metrics.record_failure(endpoint, "circuit_breaker_open");
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
                self.metrics.record_failure(endpoint, &api_error.to_string());
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

        trace!(api_op = "list_torrents", url = %url);

        let response = self
            .execute_with_retry("/torrents", || self.client.get(&url).send())
            .await?;

        let response = self.check_response(response).await?;
        let data: TorrentListResponse = response.json().await?;

        debug!(api_op = "list_torrents", count = data.torrents.len(), "Listed torrents");
        Ok(data.torrents)
    }

    /// Get detailed information about a specific torrent
    pub async fn get_torrent(&self, id: u64) -> Result<TorrentInfo> {
        let url = format!("{}/torrents/{}", self.base_url, id);
        let endpoint = format!("/torrents/{}", id);

        trace!(api_op = "get_torrent", id = id);

        let response = self
            .execute_with_retry(&endpoint, || self.client.get(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                let response = self.check_response(response).await?;
                let torrent: TorrentInfo = response.json().await?;
                debug!(api_op = "get_torrent", id = id, name = %torrent.name);
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

        trace!(api_op = "add_torrent_magnet");

        let response = self
            .execute_with_retry("/torrents", || self.client.post(&url).json(&request).send())
            .await?;

        let response = self.check_response(response).await?;
        let result: AddTorrentResponse = response.json().await?;

        debug!(api_op = "add_torrent_magnet", id = result.id, info_hash = %result.info_hash);
        Ok(result)
    }

    /// Add a torrent from a torrent file URL
    pub async fn add_torrent_url(&self, torrent_url: &str) -> Result<AddTorrentResponse> {
        let url = format!("{}/torrents", self.base_url);
        let request = AddTorrentUrlRequest {
            torrent_link: torrent_url.to_string(),
        };

        trace!(api_op = "add_torrent_url", url = %torrent_url);

        let response = self
            .execute_with_retry("/torrents", || self.client.post(&url).json(&request).send())
            .await?;

        let response = self.check_response(response).await?;
        let result: AddTorrentResponse = response.json().await?;

        debug!(api_op = "add_torrent_url", id = result.id, info_hash = %result.info_hash);
        Ok(result)
    }

    /// Get statistics for a torrent
    pub async fn get_torrent_stats(&self, id: u64) -> Result<TorrentStats> {
        let url = format!("{}/torrents/{}/stats/v1", self.base_url, id);
        let endpoint = format!("/torrents/{}/stats", id);

        trace!(api_op = "get_torrent_stats", id = id);

        let response = self
            .execute_with_retry(&endpoint, || self.client.get(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                let response = self.check_response(response).await?;
                let stats: TorrentStats = response.json().await?;
                trace!(api_op = "get_torrent_stats", id = id, progress_pct = stats.progress_pct);
                Ok(stats)
            }
        }
    }

    /// Get piece availability bitfield for a torrent
    pub async fn get_piece_bitfield(&self, id: u64) -> Result<PieceBitfield> {
        let url = format!("{}/torrents/{}/haves", self.base_url, id);
        let endpoint = format!("/torrents/{}/haves", id);

        trace!(api_op = "get_piece_bitfield", id = id);

        let response = self
            .execute_with_retry(&endpoint, || {
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
                    api_op = "get_piece_bitfield",
                    id = id,
                    bytes = bits.len(),
                    num_pieces = num_pieces
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
        let endpoint = format!("/torrents/{}/stream/{}", torrent_id, file_idx);

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
                api_op = "read_file",
                torrent_id = torrent_id,
                file_idx = file_idx,
                range = %range_header
            );
            request = request.header("Range", range_header);
        } else {
            trace!(
                api_op = "read_file",
                torrent_id = torrent_id,
                file_idx = file_idx
            );
        }

        let response = self
            .execute_with_retry(&endpoint, || request.try_clone().unwrap().send())
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
                    api_op = "read_file",
                    torrent_id = torrent_id,
                    file_idx = file_idx,
                    bytes_read = bytes.len()
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
        let endpoint = format!("/torrents/{}/pause", id);

        trace!(api_op = "pause_torrent", id = id);

        let response = self
            .execute_with_retry(&endpoint, || self.client.post(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                self.check_response(response).await?;
                debug!(api_op = "pause_torrent", id = id, "Paused torrent");
                Ok(())
            }
        }
    }

    /// Resume/start a torrent
    pub async fn start_torrent(&self, id: u64) -> Result<()> {
        let url = format!("{}/torrents/{}/start", self.base_url, id);
        let endpoint = format!("/torrents/{}/start", id);

        trace!(api_op = "start_torrent", id = id);

        let response = self
            .execute_with_retry(&endpoint, || self.client.post(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                self.check_response(response).await?;
                debug!(api_op = "start_torrent", id = id, "Started torrent");
                Ok(())
            }
        }
    }

    /// Remove torrent from session (keep files)
    pub async fn forget_torrent(&self, id: u64) -> Result<()> {
        let url = format!("{}/torrents/{}/forget", self.base_url, id);
        let endpoint = format!("/torrents/{}/forget", id);

        trace!(api_op = "forget_torrent", id = id);

        let response = self
            .execute_with_retry(&endpoint, || self.client.post(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                self.check_response(response).await?;
                debug!(api_op = "forget_torrent", id = id, "Forgot torrent");
                Ok(())
            }
        }
    }

    /// Remove torrent from session and delete files
    pub async fn delete_torrent(&self, id: u64) -> Result<()> {
        let url = format!("{}/torrents/{}/delete", self.base_url, id);
        let endpoint = format!("/torrents/{}/delete", id);

        trace!(api_op = "delete_torrent", id = id);

        let response = self
            .execute_with_retry(&endpoint, || self.client.post(&url).send())
            .await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
            _ => {
                self.check_response(response).await?;
                debug!(api_op = "delete_torrent", id = id, "Deleted torrent");
                Ok(())
            }
        }
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
        let client = RqbitClient::new("http://localhost:3030".to_string(), metrics);
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
        assert_eq!(
            ApiError::CircuitBreakerOpen.to_fuse_error(),
            libc::EAGAIN
        );
        assert_eq!(
            ApiError::RetryLimitExceeded.to_fuse_error(),
            libc::EAGAIN
        );

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
        assert!(
            ApiError::ApiError {
                status: 503,
                message: "unavailable".to_string()
            }
            .is_transient()
        );

        assert!(!ApiError::TorrentNotFound(1).is_transient());
        assert!(!ApiError::InvalidRange("test".to_string()).is_transient());
        assert!(
            !ApiError::ApiError {
                status: 404,
                message: "not found".to_string()
            }
            .is_transient()
        );
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
}
