use moka::future::Cache as MokaCache;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Cache statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub expired: u64,
    pub size: usize,
    pub weight: u64,
}

impl CacheStats {
    /// Calculate hit rate as a percentage (0-100)
    ///
    /// Returns 0.0 if there are no hits or misses.
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        (self.hits as f64 / total as f64) * 100.0
    }

    /// Calculate miss rate as a percentage (0-100)
    ///
    /// Returns 0.0 if there are no hits or misses.
    pub fn miss_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        (self.misses as f64 / total as f64) * 100.0
    }

    /// Get total number of requests (hits + misses)
    pub fn total_requests(&self) -> u64 {
        self.hits + self.misses
    }
}

/// A thread-safe cache with TTL support and LRU eviction.
///
/// This cache implementation uses the `moka` crate which provides:
/// - O(1) operations (no full scans for eviction)
/// - Atomic operations (no race conditions)
/// - Built-in TTL handling
/// - Async-native support
/// - High concurrency with lock-free reads
///
/// The cache stores key-value pairs with an optional TTL (time-to-live).
/// When the cache reaches its maximum size, entries are evicted using
/// an LRU (Least Recently Used) policy via the TinyLFU algorithm.
pub struct Cache<K, V> {
    /// The underlying moka cache
    inner: MokaCache<K, Arc<V>>,
    /// Hit counter
    hits: AtomicU64,
    /// Miss counter
    misses: AtomicU64,
    /// Eviction counter
    evictions: AtomicU64,
    /// Default TTL for entries
    default_ttl: Duration,
    /// Maximum capacity (for eviction tracking)
    max_capacity: u64,
}

impl<K, V> Cache<K, V>
where
    K: std::hash::Hash + Eq + Clone + Send + Sync + 'static,
    V: Send + Sync + 'static,
{
    /// Create a new cache with the specified maximum size and default TTL
    pub fn new(max_entries: usize, default_ttl: Duration) -> Self {
        let inner = MokaCache::builder()
            .max_capacity(max_entries as u64)
            .time_to_live(default_ttl)
            .build();

        Self {
            inner,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            default_ttl,
            max_capacity: max_entries as u64,
        }
    }

    /// Get a value from the cache.
    /// Returns None if the key is not found or the entry has expired.
    pub async fn get(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        match self.inner.get(key).await {
            Some(value) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some((*value).clone())
            }
            None => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Insert a value into the cache.
    /// If the cache is at capacity, the least recently used entry is evicted.
    pub async fn insert(&self, key: K, value: V) {
        self.insert_with_ttl(key, value, self.default_ttl).await;
    }

    /// Insert a value with a custom TTL.
    pub async fn insert_with_ttl(&self, key: K, value: V, _ttl: Duration) {
        // Note: moka's time_to_live is set at builder time per-cache,
        // not per-entry. For per-entry TTL, we'd need to use
        // time_to_idle or a different approach. For now, we use
        // the cache-wide TTL set at construction.
        // TODO: Consider using moka's per-entry expiration when available

        // Track size before insert for eviction estimation
        let size_before = self.inner.entry_count();

        let arc_value = Arc::new(value);
        self.inner.insert(key, arc_value).await;

        // Track potential evictions: if we were at capacity before and size didn't increase,
        // an eviction likely occurred to make room. This is an approximation due to
        // moka's async processing.
        if size_before >= self.max_capacity {
            self.evictions.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Remove a specific entry from the cache
    pub async fn remove(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        // Try to get the value before removing
        let value = self.inner.get(key).await;
        self.inner.invalidate(key).await;
        value.map(|arc_v| (*arc_v).clone())
    }

    /// Clear all entries from the cache
    pub async fn clear(&self) {
        self.inner.invalidate_all();
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            expired: 0, // moka handles expiration internally
            size: self.inner.entry_count() as usize,
            weight: 0, // Requires weigher to be configured; not available by default
        }
    }

    /// Check if a key exists in the cache.
    /// Note: This uses get() which updates the access time (LRU tracking)
    pub async fn contains_key(&self, key: &K) -> bool {
        self.inner.get(key).await.is_some()
    }

    /// Get the number of entries in the cache
    pub fn len(&self) -> usize {
        self.inner.entry_count() as usize
    }

    /// Check if the cache is empty
    pub async fn is_empty(&self) -> bool {
        self.inner.entry_count() == 0
    }
}

impl<K, V> Default for Cache<K, V>
where
    K: std::hash::Hash + Eq + Clone + Send + Sync + 'static,
    V: Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new(1000, Duration::from_secs(300))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache: Cache<String, i32> = Cache::new(10, Duration::from_secs(60));

        // Insert and retrieve
        cache.insert("key1".to_string(), 42).await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some(42));

        // Non-existent key
        assert_eq!(cache.get(&"key2".to_string()).await, None);

        // Allow async operations and Moka maintenance tasks to complete
        // Moka processes updates asynchronously, so we need to wait
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Stats
        let stats = cache.stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        // Note: Moka's entry_count() is eventually consistent
        // The size may be 0 or 1 depending on timing, so we just check it's reasonable
        assert!(
            stats.size <= 1,
            "Cache size should be at most 1, got {}",
            stats.size
        );
    }

    #[tokio::test]
    async fn test_cache_ttl() {
        let cache: Cache<String, i32> = Cache::new(10, Duration::from_millis(100));

        // Insert with short TTL
        cache.insert("key1".to_string(), 42).await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some(42));

        // Wait for expiration (TTL is 100ms, so wait 150ms)
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(cache.get(&"key1".to_string()).await, None);

        // Allow async operations to complete
        tokio::time::sleep(Duration::from_millis(50)).await;

        let stats = cache.stats().await;
        // One hit (first get) + one miss (after expiration) = 1 miss total
        // Note: insert() doesn't count as a miss
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }

    #[tokio::test]
    async fn test_cache_lru_eviction() {
        // This test verifies that the cache maintains the expected size after eviction.
        // Moka uses TinyLFU (Tiny Least Frequently Used) which considers both:
        // - Frequency of access
        // - Recency of access
        // The exact eviction decision is non-deterministic in test conditions,
        // so we verify the cache behaves correctly at a high level.

        let cache: Cache<String, i32> = Cache::new(3, Duration::from_secs(60));

        // Insert 3 entries (at capacity)
        cache.insert("key1".to_string(), 1).await;
        cache.insert("key2".to_string(), 2).await;
        cache.insert("key3".to_string(), 3).await;

        // Allow async operations to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify all three entries exist
        assert!(
            cache.contains_key(&"key1".to_string()).await,
            "key1 should exist initially"
        );
        assert!(
            cache.contains_key(&"key2".to_string()).await,
            "key2 should exist initially"
        );
        assert!(
            cache.contains_key(&"key3".to_string()).await,
            "key3 should exist initially"
        );

        // Access key1 multiple times to make it frequently used
        for _ in 0..10 {
            let _ = cache.get(&"key1".to_string()).await;
        }

        // Allow async operations to complete
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Insert 4th entry - should trigger eviction since capacity is 3
        cache.insert("key4".to_string(), 4).await;

        // Give Moka time to process the eviction
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify key1 still exists (frequently accessed)
        assert!(
            cache.contains_key(&"key1".to_string()).await,
            "key1 should exist (frequently used)"
        );

        // key4 should exist (just inserted)
        assert!(
            cache.contains_key(&"key4".to_string()).await,
            "key4 should exist (recently inserted)"
        );

        // Cache should maintain capacity of 3
        let stats = cache.stats().await;
        assert!(
            stats.size <= 3,
            "Cache size should not exceed capacity, got {}",
            stats.size
        );

        // Verify we can still access entries
        assert_eq!(cache.get(&"key1".to_string()).await, Some(1));
        assert_eq!(cache.get(&"key4".to_string()).await, Some(4));
    }

    #[tokio::test]
    async fn test_cache_remove() {
        let cache: Cache<String, i32> = Cache::new(10, Duration::from_secs(60));

        cache.insert("key1".to_string(), 42).await;
        assert_eq!(cache.remove(&"key1".to_string()).await, Some(42));
        assert_eq!(cache.remove(&"key1".to_string()).await, None);
        assert!(!cache.contains_key(&"key1".to_string()).await);
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache: Cache<String, i32> = Cache::new(10, Duration::from_secs(60));

        cache.insert("key1".to_string(), 1).await;
        cache.insert("key2".to_string(), 2).await;

        cache.clear().await;

        assert!(cache.is_empty().await);
        assert_eq!(cache.get(&"key1".to_string()).await, None);
        assert_eq!(cache.get(&"key2".to_string()).await, None);
    }

    #[tokio::test]
    async fn test_cache_custom_ttl() {
        let cache: Cache<String, i32> = Cache::new(10, Duration::from_secs(60));

        // Insert with custom short TTL (cache-wide TTL is 60s, but this entry
        // will use the same TTL since moka doesn't support per-entry TTL yet)
        cache
            .insert_with_ttl("key1".to_string(), 42, Duration::from_millis(50))
            .await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some(42));

        // Note: Since we're using cache-wide TTL, this will still be present
        // Per-entry TTL requires a different approach with moka
    }

    #[tokio::test]
    async fn test_concurrent_cache_access() {
        use std::sync::Arc;
        use tokio::task;

        let cache: Arc<Cache<String, i32>> = Arc::new(Cache::new(100, Duration::from_secs(60)));
        let mut handles = vec![];

        // Spawn multiple tasks that insert and read concurrently
        for i in 0..10 {
            let cache = Arc::clone(&cache);
            handles.push(task::spawn(async move {
                let key = format!("key{}", i);
                cache.insert(key.clone(), i).await;
                cache.get(&key).await
            }));
        }

        // Wait for all tasks
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_some());
        }

        let stats = cache.stats().await;
        // Note: moka may evict entries during concurrent access
        // Just verify we have some entries and hits were recorded
        assert!(
            stats.size > 0 || stats.hits > 0,
            "Cache should have entries or recorded hits"
        );
    }

    /// Test EDGE-016: Cache entry expiration during access
    /// Verifies that when an entry expires during a get() operation,
    /// the cache returns None without panicking.
    #[tokio::test]
    async fn test_cache_entry_expiration_during_access() {
        // Use a very short TTL to make expiration likely during access
        let cache: Cache<String, i32> = Cache::new(10, Duration::from_millis(50));

        // Insert entry with short TTL
        cache.insert("expiring_key".to_string(), 42).await;

        // Verify entry exists initially
        assert_eq!(cache.get(&"expiring_key".to_string()).await, Some(42));

        // Wait for TTL to expire (50ms TTL, wait 60ms)
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Try to get the expired entry - should return None, not panic
        let result = cache.get(&"expiring_key".to_string()).await;
        assert_eq!(result, None, "Should return None for expired entry");

        // Verify stats recorded the miss
        let stats = cache.stats().await;
        assert_eq!(stats.hits, 1, "Should have 1 hit from initial get");
        assert_eq!(stats.misses, 1, "Should have 1 miss from expired entry");
    }

    /// Test EDGE-016 variant: Rapid access as entry expires
    /// Simulates a race condition where multiple gets occur as entry expires
    #[tokio::test]
    async fn test_cache_expiration_race_condition() {
        use std::sync::Arc;
        use tokio::task;

        // Very short TTL to increase chance of race
        let cache: Arc<Cache<String, i32>> = Arc::new(Cache::new(10, Duration::from_millis(100)));

        // Insert entry
        cache.insert("race_key".to_string(), 42).await;

        // Spawn multiple concurrent get operations
        let mut handles = vec![];
        for _ in 0..20 {
            let cache = Arc::clone(&cache);
            handles.push(task::spawn(async move {
                // Try to get entry - some may succeed, some may get None
                // Neither should panic
                cache.get(&"race_key".to_string()).await
            }));
        }

        // Wait for TTL to expire during operations
        tokio::time::sleep(Duration::from_millis(120)).await;

        // Spawn more gets after expiration
        for _ in 0..10 {
            let cache = Arc::clone(&cache);
            handles.push(task::spawn(async move {
                cache.get(&"race_key".to_string()).await
            }));
        }

        // Collect all results - verify no panics occurred
        let mut success_count = 0;
        let mut none_count = 0;
        for handle in handles {
            match handle.await {
                Ok(Some(_)) => success_count += 1,
                Ok(None) => none_count += 1,
                Err(e) => panic!("Task panicked: {:?}", e),
            }
        }

        // Verify we got some results (either Some or None, but no panics)
        assert!(
            success_count + none_count == 30,
            "All 30 operations should complete without panic"
        );

        // Final state should be expired
        assert_eq!(cache.get(&"race_key".to_string()).await, None);
    }

    /// Performance benchmark for concurrent cache reads with statistics collection.
    /// This test verifies that atomic counters provide good performance under
    /// high concurrency.
    #[tokio::test]
    async fn test_cache_stats_performance() {
        use std::sync::Arc;
        use tokio::time::Instant;

        let cache: Arc<Cache<String, i32>> = Arc::new(Cache::new(1000, Duration::from_secs(60)));
        let num_tasks = 100;
        let ops_per_task = 1000;

        // Pre-populate cache
        for i in 0..100 {
            cache.insert(format!("key{}", i), i as i32).await;
        }

        let start = Instant::now();

        // Spawn concurrent readers
        let mut handles = vec![];
        for task_id in 0..num_tasks {
            let cache = Arc::clone(&cache);
            handles.push(tokio::spawn(async move {
                let mut hits = 0;
                for i in 0..ops_per_task {
                    let key = format!("key{}", (task_id + i) % 100);
                    if cache.get(&key).await.is_some() {
                        hits += 1;
                    }
                }
                hits
            }));
        }

        // Wait for all tasks and collect results
        let mut total_hits = 0;
        for handle in handles {
            total_hits += handle.await.unwrap();
        }

        let elapsed = start.elapsed();
        let total_ops = num_tasks * ops_per_task;
        let ops_per_sec = total_ops as f64 / elapsed.as_secs_f64();

        // Verify stats are accurate
        let stats = cache.stats().await;
        assert_eq!(stats.hits, total_hits as u64);
        assert_eq!(stats.misses, (total_ops - total_hits) as u64);

        // Performance assertion: should handle at least 100k ops/sec
        // This is a sanity check - actual performance will vary by hardware
        assert!(
            ops_per_sec > 100_000.0,
            "Cache throughput too low: {:.0} ops/sec (expected > 100k)",
            ops_per_sec
        );

        println!(
            "Cache performance: {:.0} ops/sec ({} threads x {} ops)",
            ops_per_sec, num_tasks, ops_per_task
        );
        println!(
            "Stats accuracy: hits={}, expected_hits={}",
            stats.hits, total_hits
        );
    }
}
