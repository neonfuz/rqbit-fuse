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
    /// Total number of piece availability checks that failed
    pub pieces_unavailable_errors: AtomicU64,
    /// Total number of torrents removed from filesystem
    pub torrents_removed: AtomicU64,
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

    pub fn record_release(&self) {
        self.release_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "release");
    }

    pub fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pieces_unavailable(&self) {
        self.pieces_unavailable_errors
            .fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "pieces_unavailable");
    }

    pub fn record_torrent_removed(&self) {
        self.torrents_removed.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "torrent_removed");
    }

    /// Record a read operation with bytes and latency
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

    /// Calculate average read latency in milliseconds
    pub fn avg_latency_ms(&self) -> f64 {
        let count = self.read_count.load(Ordering::Relaxed);
        if count == 0 {
            return 0.0;
        }
        let total_ns = self.read_latency_ns.load(Ordering::Relaxed);
        (total_ns as f64 / count as f64) / 1_000_000.0
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
        let pieces_unavailable = self.pieces_unavailable_errors.load(Ordering::Relaxed);
        let torrents_removed = self.torrents_removed.load(Ordering::Relaxed);
        let avg_latency = self.avg_latency_ms();
        let throughput = self.read_throughput_mbps(elapsed_secs);

        info!(
            operation = "fuse_metrics_summary",
            reads = reads,
            bytes_read = bytes,
            avg_read_latency_ms = avg_latency,
            throughput_mbps = throughput,
            errors = errors,
            pieces_unavailable_errors = pieces_unavailable,
            torrents_removed = torrents_removed,
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

    /// Calculate average latency in milliseconds
    pub fn avg_latency_ms(&self) -> f64 {
        let count = self.success_count.load(Ordering::Relaxed);
        if count == 0 {
            return 0.0;
        }
        let total_ns = self.total_latency_ns.load(Ordering::Relaxed);
        (total_ns as f64 / count as f64) / 1_000_000.0
    }

    /// Get success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        let total = self.request_count.load(Ordering::Relaxed);
        if total == 0 {
            return 100.0;
        }
        let success = self.success_count.load(Ordering::Relaxed);
        (success as f64 / total as f64) * 100.0
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

/// Metrics for cache operations
#[derive(Debug, Default)]
pub struct CacheMetrics {
    /// Total number of cache hits
    pub hits: AtomicU64,
    /// Total number of cache misses
    pub misses: AtomicU64,
    /// Total number of cache evictions
    pub evictions: AtomicU64,
    /// Current cache size (entries)
    pub current_size: AtomicU64,
    /// Peak cache size observed
    pub peak_size: AtomicU64,
    /// Total bytes served from cache
    pub bytes_served: AtomicU64,
}

impl CacheMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a cache hit
    pub fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache miss
    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache eviction
    pub fn record_eviction(&self) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }

    /// Update current cache size and track peak
    pub fn update_size(&self, size: usize) {
        let size = size as u64;
        self.current_size.store(size, Ordering::Relaxed);

        // Update peak if current size exceeds it
        let current_peak = self.peak_size.load(Ordering::Relaxed);
        if size > current_peak {
            let _ = self.peak_size.compare_exchange(
                current_peak,
                size,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
    }

    /// Record bytes served from cache
    pub fn record_bytes(&self, bytes: u64) {
        self.bytes_served.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Get hit rate as a percentage
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            return 0.0;
        }
        (hits as f64 / total as f64) * 100.0
    }

    /// Get current cache size
    pub fn current_size(&self) -> usize {
        self.current_size.load(Ordering::Relaxed) as usize
    }

    /// Get peak cache size
    pub fn peak_size(&self) -> usize {
        self.peak_size.load(Ordering::Relaxed) as usize
    }

    /// Log a summary of cache metrics
    pub fn log_summary(&self) {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let evictions = self.evictions.load(Ordering::Relaxed);
        let current_size = self.current_size.load(Ordering::Relaxed);
        let peak_size = self.peak_size.load(Ordering::Relaxed);
        let bytes_served = self.bytes_served.load(Ordering::Relaxed);

        let hit_rate = self.hit_rate();

        info!(
            operation = "cache_metrics_summary",
            hits = hits,
            misses = misses,
            hit_rate_pct = hit_rate,
            evictions = evictions,
            current_size = current_size,
            peak_size = peak_size,
            bytes_served = bytes_served,
        );
    }
}

/// Combined metrics for the entire system
pub struct Metrics {
    pub fuse: Arc<FuseMetrics>,
    pub api: Arc<ApiMetrics>,
    pub cache: Arc<CacheMetrics>,
    pub start_time: Instant,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            fuse: Arc::new(FuseMetrics::new()),
            api: Arc::new(ApiMetrics::new()),
            cache: Arc::new(CacheMetrics::new()),
            start_time: Instant::now(),
        }
    }

    /// Log a complete metrics summary
    pub fn log_full_summary(&self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        info!("=== rqbit-fuse Metrics Summary ===");
        self.fuse.log_summary(elapsed);
        self.api.log_summary();
        self.cache.log_summary();
        info!("====================================");
    }

    /// Log periodic metrics summary (for background task)
    pub fn log_periodic(&self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        info!("--- rqbit-fuse Metrics (periodic) ---");
        self.fuse.log_summary(elapsed);
        self.api.log_summary();
        self.cache.log_summary();
        info!("---------------------------------------");
    }

    /// Get elapsed time since metrics creation
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Create a periodic logging background task
    pub fn spawn_periodic_logging(
        self: &Arc<Self>,
        interval_secs: u64,
        stop: Arc<std::sync::atomic::AtomicBool>,
    ) -> tokio::task::JoinHandle<()> {
        let metrics = Arc::clone(self);
        tokio::spawn(async move {
            use tokio::time::{interval, Duration};

            let mut ticker = interval(Duration::from_secs(interval_secs));

            loop {
                ticker.tick().await;

                if stop.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }

                metrics.log_periodic();
            }
        })
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

        let avg_latency = metrics.avg_latency_ms();
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

    #[test]
    fn test_cache_metrics() {
        let metrics = CacheMetrics::new();

        metrics.record_hit();
        metrics.record_hit();
        metrics.record_miss();

        assert_eq!(metrics.hits.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.misses.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.hit_rate(), 66.66666666666667);
    }

    #[test]
    fn test_explicit_methods() {
        let metrics = FuseMetrics::new();

        // Test all explicit recording methods
        metrics.record_getattr();
        metrics.record_setattr();
        metrics.record_lookup();
        metrics.record_readdir();
        metrics.record_open();
        metrics.record_release();
        metrics.record_error();
        metrics.record_pieces_unavailable();
        metrics.record_torrent_removed();

        assert_eq!(metrics.getattr_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.setattr_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.lookup_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.readdir_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.open_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.release_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.error_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.pieces_unavailable_errors.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.torrents_removed.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_api_avg_latency() {
        let metrics = ApiMetrics::new();

        // Test zero operations returns 0.0
        assert_eq!(metrics.avg_latency_ms(), 0.0);

        // Record some successes
        metrics.record_request("/test");
        metrics.record_success("/test", Duration::from_millis(100));
        metrics.record_request("/test");
        metrics.record_success("/test", Duration::from_millis(300));

        // Average should be 200ms
        let avg = metrics.avg_latency_ms();
        assert!(avg > 199.0 && avg < 201.0);
    }
}
