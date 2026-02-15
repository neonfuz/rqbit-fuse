use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, trace, warn};

/// Macro to generate simple operation recording methods
///
/// Generates methods that:
/// - Increment a counter field
/// - Emit a trace log with the operation name
macro_rules! record_op {
    // Variant with trace logging
    ($method:ident, $field:ident, $op_name:expr) => {
        pub fn $method(&self) {
            self.$field.fetch_add(1, Ordering::Relaxed);
            trace!(fuse_op = $op_name);
        }
    };
    // Variant without trace logging
    ($method:ident, $field:ident) => {
        pub fn $method(&self) {
            self.$field.fetch_add(1, Ordering::Relaxed);
        }
    };
}

/// Trait for metrics that track latency
///
/// Implementors must provide:
/// - count(): The number of operations
/// - total_latency_ns(): Total latency in nanoseconds
pub trait LatencyMetrics {
    /// Get the count of operations
    fn count(&self) -> u64;
    /// Get total latency in nanoseconds
    fn total_latency_ns(&self) -> u64;

    /// Calculate average latency in milliseconds
    ///
    /// Uses atomic snapshot pattern to ensure consistent read of count and total.
    /// Under high contention, may retry to get a consistent pair of values.
    fn avg_latency_ms(&self) -> f64 {
        loop {
            let count = self.count();
            if count == 0 {
                return 0.0;
            }
            let total_ns = self.total_latency_ns();
            let new_count = self.count();
            if new_count == count {
                return (total_ns as f64 / count as f64) / 1_000_000.0;
            }
        }
    }
}

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

    // Generate simple recording methods using macro
    record_op!(record_getattr, getattr_count, "getattr");
    record_op!(record_setattr, setattr_count, "setattr");
    record_op!(record_lookup, lookup_count, "lookup");
    record_op!(record_readdir, readdir_count, "readdir");
    record_op!(record_open, open_count, "open");
    record_op!(record_release, release_count, "release");
    record_op!(record_error, error_count);

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
        loop {
            let reads = self.read_count.load(Ordering::Relaxed);
            let bytes = self.bytes_read.load(Ordering::Relaxed);
            let errors = self.error_count.load(Ordering::Relaxed);
            let new_reads = self.read_count.load(Ordering::Relaxed);
            if new_reads == reads {
                let avg_latency = self.avg_latency_ms();
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
                return;
            }
        }
    }
}

impl LatencyMetrics for FuseMetrics {
    fn count(&self) -> u64 {
        self.read_count.load(Ordering::Relaxed)
    }

    fn total_latency_ns(&self) -> u64 {
        self.read_latency_ns.load(Ordering::Relaxed)
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

    /// Get success rate as a percentage
    ///
    /// Uses atomic snapshot pattern to ensure consistent read of request and success counts.
    pub fn success_rate(&self) -> f64 {
        loop {
            let total = self.request_count.load(Ordering::Relaxed);
            if total == 0 {
                return 100.0;
            }
            let success = self.success_count.load(Ordering::Relaxed);
            let new_total = self.request_count.load(Ordering::Relaxed);
            if new_total == total {
                return (success as f64 / total as f64) * 100.0;
            }
        }
    }

    /// Log a summary of API metrics
    pub fn log_summary(&self) {
        loop {
            let total = self.request_count.load(Ordering::Relaxed);
            let success = self.success_count.load(Ordering::Relaxed);
            let failures = self.failure_count.load(Ordering::Relaxed);
            let retries = self.retry_count.load(Ordering::Relaxed);
            let new_total = self.request_count.load(Ordering::Relaxed);
            if new_total == total {
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
                return;
            }
        }
    }
}

impl LatencyMetrics for ApiMetrics {
    fn count(&self) -> u64 {
        self.success_count.load(Ordering::Relaxed)
    }

    fn total_latency_ns(&self) -> u64 {
        self.total_latency_ns.load(Ordering::Relaxed)
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

    /// Log periodic metrics summary (for background task)
    pub fn log_periodic(&self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        info!("--- torrent-fuse Metrics (periodic) ---");
        self.fuse.log_summary(elapsed);
        self.api.log_summary();
        info!("---------------------------------------");
    }

    /// Get elapsed time since metrics creation
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Create a periodic logging background task
    ///
    /// Returns a future that logs metrics at the specified interval
    /// until the shared stop flag is set.
    ///
    /// # Arguments
    /// * `interval_secs` - How often to log metrics
    /// * `stop` - Arc flag to signal task stop
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
    fn test_latency_metrics_trait_fuse() {
        let metrics = FuseMetrics::new();

        // Test zero operations returns 0.0
        assert_eq!(metrics.avg_latency_ms(), 0.0);

        // Record some reads
        metrics.record_read(1024, Duration::from_millis(10));
        metrics.record_read(1024, Duration::from_millis(30));

        // Average should be 20ms
        let avg = metrics.avg_latency_ms();
        assert!(avg > 19.0 && avg < 21.0);
    }

    #[test]
    fn test_latency_metrics_trait_api() {
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

    #[test]
    fn test_macro_generated_methods() {
        let metrics = FuseMetrics::new();

        // Test all macro-generated methods
        metrics.record_getattr();
        metrics.record_setattr();
        metrics.record_lookup();
        metrics.record_readdir();
        metrics.record_open();
        metrics.record_release();
        metrics.record_error();

        assert_eq!(metrics.getattr_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.setattr_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.lookup_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.readdir_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.open_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.release_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.error_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_concurrent_avg_latency_consistency() {
        use std::sync::Arc;
        use std::thread;

        let metrics = Arc::new(FuseMetrics::new());
        let metrics_clone = Arc::clone(&metrics);

        // Spawn threads that continuously record reads
        let writers: Vec<_> = (0..4)
            .map(|_| {
                let m = Arc::clone(&metrics_clone);
                thread::spawn(move || {
                    for i in 0..1000 {
                        m.record_read(1024, Duration::from_nanos(1000 + i as u64));
                    }
                })
            })
            .collect();

        // Spawn threads that continuously read the average
        let readers: Vec<_> = (0..4)
            .map(|_| {
                let m = Arc::clone(&metrics);
                thread::spawn(move || {
                    for _ in 0..1000 {
                        let avg = m.avg_latency_ms();
                        // Average should be non-negative and reasonable
                        assert!(avg >= 0.0);
                        assert!(avg < 1000.0); // Should be way less than 1 second
                    }
                })
            })
            .collect();

        for w in writers {
            w.join().unwrap();
        }
        for r in readers {
            r.join().unwrap();
        }

        // Verify final count is correct
        assert_eq!(metrics.read_count.load(Ordering::Relaxed), 4000);
    }

    #[test]
    fn test_concurrent_success_rate_consistency() {
        use std::sync::Arc;
        use std::thread;

        let metrics = Arc::new(ApiMetrics::new());
        let metrics_clone = Arc::clone(&metrics);

        // Spawn threads that continuously record requests
        let writers: Vec<_> = (0..4)
            .map(|_| {
                let m = Arc::clone(&metrics_clone);
                thread::spawn(move || {
                    for _ in 0..500 {
                        m.record_request("/test");
                        m.record_success("/test", Duration::from_millis(10));
                    }
                    for _ in 0..500 {
                        m.record_request("/test");
                        m.record_failure("/test", "error");
                    }
                })
            })
            .collect();

        // Spawn threads that continuously read success rate
        let readers: Vec<_> = (0..4)
            .map(|_| {
                let m = Arc::clone(&metrics);
                thread::spawn(move || {
                    for _ in 0..1000 {
                        let rate = m.success_rate();
                        // Success rate should be between 0 and 100
                        assert!(rate >= 0.0);
                        assert!(rate <= 100.0);
                    }
                })
            })
            .collect();

        for w in writers {
            w.join().unwrap();
        }
        for r in readers {
            r.join().unwrap();
        }

        // Verify final counts
        assert_eq!(metrics.request_count.load(Ordering::Relaxed), 4000);
        assert_eq!(metrics.success_count.load(Ordering::Relaxed), 2000);
    }
}
