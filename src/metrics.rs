use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, trace, warn};

/// Metrics for FUSE filesystem operations
#[derive(Debug, Default)]
pub struct FuseMetrics {
    /// Total number of getattr operations
    pub getattr_count: AtomicU64,
    /// Total number of setattr operations
    pub setattr_count: AtomicU64,
    /// Total number of lookup operations
    pub lookup_count: AtomicU64,
    /// Total number of readdir operations
    pub readdir_count: AtomicU64,
    /// Total number of open operations
    pub open_count: AtomicU64,
    /// Total number of read operations
    pub read_count: AtomicU64,
    /// Total number of release operations
    pub release_count: AtomicU64,
    /// Total bytes read
    pub bytes_read: AtomicU64,
    /// Total number of errors
    pub error_count: AtomicU64,
    /// Total time spent in read operations (nanoseconds)
    pub read_latency_ns: AtomicU64,
}

impl FuseMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_getattr(&self) {
        self.getattr_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "getattr");
    }

    pub fn record_setattr(&self) {
        self.setattr_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "setattr");
    }

    pub fn record_lookup(&self) {
        self.lookup_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "lookup");
    }

    pub fn record_readdir(&self) {
        self.readdir_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "readdir");
    }

    pub fn record_open(&self) {
        self.open_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "open");
    }

    pub fn record_read(&self, bytes: u64, latency: Duration) {
        self.read_count.fetch_add(1, Ordering::Relaxed);
        self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
        self.read_latency_ns
            .fetch_add(latency.as_nanos() as u64, Ordering::Relaxed);
        trace!(
            fuse_op = "read",
            bytes_read = bytes,
            latency_ns = latency.as_nanos() as u64
        );
    }

    pub fn record_release(&self) {
        self.release_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "release");
    }

    pub fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the average read latency in milliseconds
    pub fn avg_read_latency_ms(&self) -> f64 {
        let count = self.read_count.load(Ordering::Relaxed);
        if count == 0 {
            0.0
        } else {
            let total_ns = self.read_latency_ns.load(Ordering::Relaxed);
            (total_ns as f64 / count as f64) / 1_000_000.0
        }
    }

    /// Get read throughput in MB/s
    pub fn read_throughput_mbps(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs <= 0.0 {
            return 0.0;
        }
        let bytes = self.bytes_read.load(Ordering::Relaxed);
        (bytes as f64 / 1_048_576.0) / elapsed_secs
    }

    /// Log a summary of metrics
    pub fn log_summary(&self, elapsed_secs: f64) {
        let reads = self.read_count.load(Ordering::Relaxed);
        let bytes = self.bytes_read.load(Ordering::Relaxed);
        let errors = self.error_count.load(Ordering::Relaxed);
        let avg_latency = self.avg_read_latency_ms();
        let throughput = self.read_throughput_mbps(elapsed_secs);

        info!(
            operation = "fuse_metrics_summary",
            reads = reads,
            bytes_read = bytes,
            avg_read_latency_ms = avg_latency,
            throughput_mbps = throughput,
            errors = errors,
            duration_secs = elapsed_secs,
        );
    }
}

/// Metrics for API operations
#[derive(Debug, Default)]
pub struct ApiMetrics {
    /// Total number of API requests
    pub request_count: AtomicU64,
    /// Total number of successful responses
    pub success_count: AtomicU64,
    /// Total number of failed requests
    pub failure_count: AtomicU64,
    /// Total number of retries
    pub retry_count: AtomicU64,
    /// Total time spent in API calls (nanoseconds)
    pub total_latency_ns: AtomicU64,
    /// Circuit breaker state changes
    pub circuit_breaker_opens: AtomicU64,
    pub circuit_breaker_closes: AtomicU64,
}

impl ApiMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_request(&self, endpoint: &str) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        trace!(api_op = "request", endpoint = endpoint);
    }

    pub fn record_success(&self, endpoint: &str, latency: Duration) {
        self.success_count.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ns
            .fetch_add(latency.as_nanos() as u64, Ordering::Relaxed);
        trace!(
            api_op = "success",
            endpoint = endpoint,
            latency_ms = latency.as_millis() as u64
        );
    }

    pub fn record_failure(&self, endpoint: &str, error: &str) {
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        trace!(api_op = "failure", endpoint = endpoint, error = error);
    }

    pub fn record_retry(&self, endpoint: &str, attempt: u32) {
        self.retry_count.fetch_add(1, Ordering::Relaxed);
        debug!(api_op = "retry", endpoint = endpoint, attempt = attempt);
    }

    pub fn record_circuit_breaker_open(&self) {
        self.circuit_breaker_opens.fetch_add(1, Ordering::Relaxed);
        warn!(api_op = "circuit_breaker", state = "opened");
    }

    pub fn record_circuit_breaker_close(&self) {
        self.circuit_breaker_closes.fetch_add(1, Ordering::Relaxed);
        info!(api_op = "circuit_breaker", state = "closed");
    }

    /// Get average API latency in milliseconds
    pub fn avg_latency_ms(&self) -> f64 {
        let count = self.success_count.load(Ordering::Relaxed);
        if count == 0 {
            0.0
        } else {
            let total_ns = self.total_latency_ns.load(Ordering::Relaxed);
            (total_ns as f64 / count as f64) / 1_000_000.0
        }
    }

    /// Get success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        let total = self.request_count.load(Ordering::Relaxed);
        if total == 0 {
            100.0
        } else {
            let success = self.success_count.load(Ordering::Relaxed);
            (success as f64 / total as f64) * 100.0
        }
    }

    /// Log a summary of API metrics
    pub fn log_summary(&self) {
        let total = self.request_count.load(Ordering::Relaxed);
        let success = self.success_count.load(Ordering::Relaxed);
        let failures = self.failure_count.load(Ordering::Relaxed);
        let retries = self.retry_count.load(Ordering::Relaxed);
        let avg_latency = self.avg_latency_ms();
        let success_rate = self.success_rate();

        info!(
            operation = "api_metrics_summary",
            total_requests = total,
            successful = success,
            failed = failures,
            retries = retries,
            success_rate_pct = success_rate,
            avg_latency_ms = avg_latency,
        );
    }
}

/// Combined metrics for the entire system
pub struct Metrics {
    pub fuse: Arc<FuseMetrics>,
    pub api: Arc<ApiMetrics>,
    pub start_time: Instant,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            fuse: Arc::new(FuseMetrics::new()),
            api: Arc::new(ApiMetrics::new()),
            start_time: Instant::now(),
        }
    }

    /// Log a complete metrics summary
    pub fn log_full_summary(&self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        info!("=== torrent-fuse Metrics Summary ===");
        self.fuse.log_summary(elapsed);
        self.api.log_summary();
        info!("====================================");
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuse_metrics() {
        let metrics = FuseMetrics::new();

        metrics.record_read(1024, Duration::from_millis(10));
        metrics.record_read(2048, Duration::from_millis(20));

        assert_eq!(metrics.read_count.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.bytes_read.load(Ordering::Relaxed), 3072);

        let avg_latency = metrics.avg_read_latency_ms();
        assert!(avg_latency > 14.0 && avg_latency < 16.0);
    }

    #[test]
    fn test_api_metrics() {
        let metrics = ApiMetrics::new();

        // First request - succeeds
        metrics.record_request("/torrents");
        metrics.record_success("/torrents", Duration::from_millis(50));

        // Second request - fails
        metrics.record_request("/torrents/1");
        metrics.record_failure("/torrents/1", "not found");

        assert_eq!(metrics.request_count.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.success_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.failure_count.load(Ordering::Relaxed), 1);

        let success_rate = metrics.success_rate();
        assert!((success_rate - 50.0).abs() < 0.01);
    }
}
