use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

/// Minimal metrics for essential monitoring
#[derive(Debug, Default)]
pub struct Metrics {
    /// Total bytes read
    pub bytes_read: AtomicU64,
    /// Total number of errors
    pub error_count: AtomicU64,
    /// Total number of cache hits
    pub cache_hits: AtomicU64,
    /// Total number of cache misses
    pub cache_misses: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record bytes read
    pub fn record_read(&self, bytes: u64) {
        self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record an error
    pub fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache hit
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache miss
    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Log summary on shutdown
    pub fn log_summary(&self) {
        let bytes = self.bytes_read.load(Ordering::Relaxed);
        let errors = self.error_count.load(Ordering::Relaxed);
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = if total > 0 {
            (hits as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        info!(
            operation = "metrics_summary",
            bytes_read = bytes,
            errors = errors,
            cache_hits = hits,
            cache_misses = misses,
            cache_hit_rate_pct = hit_rate,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics() {
        let metrics = Metrics::new();

        metrics.record_read(1024);
        metrics.record_read(2048);
        metrics.record_error();
        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_miss();

        assert_eq!(metrics.bytes_read.load(Ordering::Relaxed), 3072);
        assert_eq!(metrics.error_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.cache_hits.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.cache_misses.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_cache_hit_rate() {
        let metrics = Metrics::new();

        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_miss();

        let hits = metrics.cache_hits.load(Ordering::Relaxed);
        let misses = metrics.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = (hits as f64 / total as f64) * 100.0;

        assert!((hit_rate - 66.67).abs() < 0.01);
    }
}
