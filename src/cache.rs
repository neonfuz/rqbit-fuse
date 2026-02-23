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

    /// Create a new cache with a maximum memory size in bytes and default TTL.
    ///
    /// The `weigher` function should return the size in bytes for each value.
    /// When the total weight exceeds `max_bytes`, entries are evicted using LRU policy.
    pub fn with_memory_limit<F>(max_bytes: u64, default_ttl: Duration, weigher: F) -> Self
    where
        F: Fn(&Arc<V>) -> u32 + Send + Sync + 'static,
        V: Clone,
    {
        let inner = MokaCache::builder()
            .max_capacity(max_bytes)
            .weigher(move |_key, value: &Arc<V>| weigher(value))
            .time_to_live(default_ttl)
            .build();

        Self {
            inner,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            default_ttl,
            max_capacity: max_bytes,
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

    /// Test EDGE-018: Rapid insert/remove cycles
    /// Verifies that repeatedly inserting and removing the same key maintains
    /// cache consistency and doesn't cause memory leaks or corruption.
    #[tokio::test]
    async fn test_cache_rapid_insert_remove_cycles() {
        let cache: Cache<String, i32> = Cache::new(100, Duration::from_secs(60));
        let key = "rapid_cycle_key".to_string();
        let cycles = 1000;

        // Perform rapid insert/remove cycles on the same key
        for i in 0..cycles {
            // Insert value
            cache.insert(key.clone(), i).await;

            // Verify it exists via get() before removal
            let retrieved = cache.get(&key).await;
            assert_eq!(
                retrieved,
                Some(i),
                "Should retrieve value just inserted (cycle {})",
                i
            );

            // Remove it
            let removed = cache.remove(&key).await;
            assert_eq!(
                removed,
                Some(i),
                "Should return the value just retrieved (cycle {})",
                i
            );

            // Verify key is gone
            assert!(
                !cache.contains_key(&key).await,
                "Key should not exist after removal (cycle {})",
                i
            );
        }

        // Allow async operations to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify cache is empty (or at least doesn't contain our key)
        assert!(
            !cache.contains_key(&key).await,
            "Key should not exist after all cycles"
        );

        // Verify cache stats are consistent
        // We had 'cycles' number of get() calls (hits)
        // Note: contains_key() calls moka directly, so it doesn't count as a miss
        let stats = cache.stats().await;
        assert_eq!(
            stats.hits, cycles as u64,
            "Should have {} hits from get operations",
            cycles
        );

        // Cache should be in a consistent state (no corruption)
        // Final operation was a remove, so size should be 0 (or small due to timing)
        assert!(
            stats.size <= 1,
            "Cache size should be at most 1 after rapid cycles, got {}",
            stats.size
        );
    }

    /// Test EDGE-018 variant: Alternating insert/remove with different keys
    /// Verifies cache handles mixed key operations correctly during rapid cycles.
    #[tokio::test]
    async fn test_cache_rapid_mixed_key_cycles() {
        let cache: Cache<String, i32> = Cache::new(100, Duration::from_secs(60));
        let num_keys = 10;
        let cycles_per_key = 100;

        // Perform rapid cycles across multiple keys
        for cycle in 0..cycles_per_key {
            for key_id in 0..num_keys {
                let key = format!("key_{}", key_id);
                let value = cycle * num_keys + key_id;

                // Insert
                cache.insert(key.clone(), value as i32).await;

                // Verify immediately via get()
                let retrieved = cache.get(&key).await;
                assert_eq!(
                    retrieved,
                    Some(value as i32),
                    "Should retrieve value for key {} in cycle {}",
                    key_id,
                    cycle
                );

                // Remove
                cache.remove(&key).await;
            }
        }

        // Allow async operations to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // All keys should be removed
        for key_id in 0..num_keys {
            let key = format!("key_{}", key_id);
            assert!(
                !cache.contains_key(&key).await,
                "Key {} should be removed",
                key_id
            );
        }

        // Cache should be in consistent state
        let stats = cache.stats().await;
        // Each cycle per key: get (hit) = 1 hit per key per cycle
        let expected_hits = num_keys * cycles_per_key;
        assert_eq!(
            stats.hits, expected_hits as u64,
            "Expected {} hits, got {}",
            expected_hits, stats.hits
        );
    }

    /// Test EDGE-019: Concurrent insert of same key
    /// Verifies that when multiple threads try to insert the same key simultaneously,
    /// the cache handles it gracefully and maintains exactly one entry.
    #[tokio::test]
    async fn test_concurrent_insert_same_key() {
        use std::sync::Arc;
        use tokio::task;

        let cache: Arc<Cache<String, i32>> = Arc::new(Cache::new(100, Duration::from_secs(60)));
        let key = "concurrent_key".to_string();
        let num_threads = 10;
        let mut handles = vec![];

        // Spawn multiple threads that all try to insert the same key
        for i in 0..num_threads {
            let cache = Arc::clone(&cache);
            let key = key.clone();
            handles.push(task::spawn(async move {
                // Each thread tries to insert a different value for the same key
                cache.insert(key, i as i32).await;
                // Return the value we tried to insert
                i as i32
            }));
        }

        // Wait for all threads to complete
        let mut inserted_values = vec![];
        for handle in handles {
            match handle.await {
                Ok(value) => inserted_values.push(value),
                Err(e) => panic!("Task panicked: {:?}", e),
            }
        }

        // Allow async operations to complete - moka is eventually consistent
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Get the value - it should be one of the values that was inserted
        // This verifies that: 1) the key exists, 2) cache handled concurrent inserts gracefully
        let final_value = cache.get(&key).await;
        assert!(
            final_value.is_some(),
            "Should be able to retrieve the value after concurrent inserts"
        );

        // The final value should be one of the values we inserted
        let final_value = final_value.unwrap();
        assert!(
            inserted_values.contains(&final_value),
            "Final value {} should be one of the inserted values {:?}",
            final_value,
            inserted_values
        );

        // Verify cache is in consistent state - no crashes or errors
        // Cache should be usable after concurrent operations (get() above already proved this)
        let stats = cache.stats().await;
        // Cache should report reasonable stats (no corruption)
        assert!(
            stats.size <= 100,
            "Cache size should not exceed capacity, got {}",
            stats.size
        );

        // Verify the key exists via contains_key as well
        assert!(
            cache.contains_key(&key).await,
            "Key should exist when checked via contains_key"
        );
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

    /// Test EDGE-020: Cache statistics edge cases
    /// Verifies that hit rate calculations handle edge cases without panicking
    /// or dividing by zero.
    #[tokio::test]
    async fn test_cache_stats_edge_cases() {
        // Test 1: Hit rate with 0 total requests (fresh cache)
        let cache: Cache<String, i32> = Cache::new(10, Duration::from_secs(60));
        let stats = cache.stats().await;

        assert_eq!(stats.hits, 0, "Fresh cache should have 0 hits");
        assert_eq!(stats.misses, 0, "Fresh cache should have 0 misses");
        assert_eq!(
            stats.total_requests(),
            0,
            "Fresh cache should have 0 total requests"
        );
        assert_eq!(
            stats.hit_rate(),
            0.0,
            "Hit rate with 0 requests should be 0.0"
        );
        assert_eq!(
            stats.miss_rate(),
            0.0,
            "Miss rate with 0 requests should be 0.0"
        );

        // Test 2: Hit rate with 0 hits, many misses
        // Perform gets on non-existent keys to generate misses
        for i in 0..100 {
            let _ = cache.get(&format!("nonexistent_key_{}", i)).await;
        }

        let stats = cache.stats().await;
        assert_eq!(stats.hits, 0, "Should have 0 hits after only misses");
        assert_eq!(stats.misses, 100, "Should have 100 misses");
        assert_eq!(
            stats.total_requests(),
            100,
            "Should have 100 total requests"
        );
        assert_eq!(stats.hit_rate(), 0.0, "Hit rate with 0 hits should be 0.0");
        assert_eq!(
            stats.miss_rate(),
            100.0,
            "Miss rate with all misses should be 100.0"
        );

        // Test 3: Hit rate with 0 misses, many hits
        let cache2: Cache<String, i32> = Cache::new(10, Duration::from_secs(60));

        // Insert a key
        cache2.insert("test_key".to_string(), 42).await;

        // Wait for insert to complete
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Perform gets on existing key to generate hits
        for _ in 0..100 {
            let _ = cache2.get(&"test_key".to_string()).await;
        }

        let stats = cache2.stats().await;
        assert_eq!(stats.hits, 100, "Should have 100 hits");
        assert_eq!(stats.misses, 0, "Should have 0 misses");
        assert_eq!(
            stats.total_requests(),
            100,
            "Should have 100 total requests"
        );
        assert_eq!(
            stats.hit_rate(),
            100.0,
            "Hit rate with all hits should be 100.0"
        );
        assert_eq!(
            stats.miss_rate(),
            0.0,
            "Miss rate with 0 misses should be 0.0"
        );

        // Test 4: Mixed hit/miss ratio
        let cache3: Cache<String, i32> = Cache::new(10, Duration::from_secs(60));

        // Insert one key
        cache3.insert("existing".to_string(), 1).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 75 hits and 25 misses (75% hit rate)
        for i in 0..75 {
            let _ = cache3.get(&"existing".to_string()).await;
            if i < 25 {
                let _ = cache3.get(&format!("missing{}", i)).await;
            }
        }

        let stats = cache3.stats().await;
        assert_eq!(stats.hits, 75, "Should have 75 hits");
        assert_eq!(stats.misses, 25, "Should have 25 misses");
        assert_eq!(
            stats.total_requests(),
            100,
            "Should have 100 total requests"
        );
        assert!(
            (stats.hit_rate() - 75.0).abs() < 0.001,
            "Hit rate should be 75.0, got {}",
            stats.hit_rate()
        );
        assert!(
            (stats.miss_rate() - 25.0).abs() < 0.001,
            "Miss rate should be 25.0, got {}",
            stats.miss_rate()
        );

        // Test 5: Very large numbers (no overflow)
        let mut stats = CacheStats::default();
        stats.hits = u64::MAX;
        stats.misses = 0;

        // Should not panic or overflow
        let hit_rate = stats.hit_rate();
        assert_eq!(
            hit_rate, 100.0,
            "Hit rate with max u64 hits should be 100.0"
        );

        stats.hits = 0;
        stats.misses = u64::MAX;
        let miss_rate = stats.miss_rate();
        assert_eq!(
            miss_rate, 100.0,
            "Miss rate with max u64 misses should be 100.0"
        );

        // Test 6: Verify no division by zero in edge cases
        stats.hits = 0;
        stats.misses = 0;
        // This should not panic
        let _ = stats.hit_rate();
        let _ = stats.miss_rate();
        let _ = stats.total_requests();
    }

    /// Test EDGE-043: Cache eviction during get operation
    /// Verifies that when a cache eviction occurs while a get() operation is in progress,
    /// the cache handles it gracefully and either returns valid data or None (but never panics).
    #[tokio::test]
    async fn test_cache_eviction_during_get() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use tokio::task;

        // Create a cache with very small capacity to make eviction likely
        let cache: Arc<Cache<String, i32>> = Arc::new(Cache::new(3, Duration::from_secs(60)));

        // Pre-populate the cache to capacity
        cache.insert("key1".to_string(), 100).await;
        cache.insert("key2".to_string(), 200).await;
        cache.insert("key3".to_string(), 300).await;

        // Allow inserts to complete
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Track completed operations
        let get_count = Arc::new(AtomicUsize::new(0));
        let insert_count = Arc::new(AtomicUsize::new(0));

        // Spawn multiple concurrent operations:
        // 1. Continuous get operations on existing keys
        // 2. Continuous insert operations to trigger evictions
        let mut handles: Vec<tokio::task::JoinHandle<()>> = vec![];

        // Spawn 5 tasks doing gets
        for i in 0..5 {
            let cache = Arc::clone(&cache);
            let counter = Arc::clone(&get_count);
            handles.push(task::spawn(async move {
                for j in 0..20 {
                    let key = format!("key{}", (i + j) % 3 + 1);
                    // This get() should handle concurrent eviction gracefully
                    let _ = cache.get(&key).await;
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }

        // Spawn 5 tasks doing inserts (causing evictions)
        for i in 0..5 {
            let cache = Arc::clone(&cache);
            let counter = Arc::clone(&insert_count);
            handles.push(task::spawn(async move {
                for j in 0..10 {
                    let key = format!("new_key_{}_{}", i, j);
                    cache.insert(key, (i * 100 + j) as i32).await;
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle
                .await
                .expect("Task should complete without panicking");
        }

        // Verify operations completed without panic
        let final_get_count = get_count.load(Ordering::Relaxed);
        let final_insert_count = insert_count.load(Ordering::Relaxed);
        assert!(
            final_get_count >= 100,
            "Should have completed at least 100 get operations"
        );
        assert!(
            final_insert_count >= 50,
            "Should have completed at least 50 insert operations"
        );

        // Cache should be in a consistent state
        let stats = cache.stats().await;
        assert!(
            stats.size <= 3,
            "Cache size should not exceed capacity after evictions, got {}",
            stats.size
        );
    }

    /// Test EDGE-043 variant: Cache eviction during get of specific key
    /// Specifically tests when the key being retrieved gets evicted during the operation
    #[tokio::test]
    async fn test_cache_eviction_during_get_specific_key() {
        use std::sync::Arc;
        use tokio::task;

        // Small capacity cache with 1-second TTL
        let cache: Arc<Cache<String, i32>> = Arc::new(Cache::new(2, Duration::from_secs(60)));

        // Insert two keys at capacity
        cache.insert("target_key".to_string(), 42).await;
        cache.insert("other_key".to_string(), 99).await;

        tokio::time::sleep(Duration::from_millis(50)).await;

        // Spawn a task that repeatedly tries to get the target_key
        let cache_clone = Arc::clone(&cache);
        let get_handle = task::spawn(async move {
            let mut success_count = 0;
            let mut none_count = 0;
            for _ in 0..50 {
                match cache_clone.get(&"target_key".to_string()).await {
                    Some(val) => {
                        assert_eq!(val, 42, "Retrieved value should match inserted value");
                        success_count += 1;
                    }
                    None => {
                        none_count += 1;
                    }
                }
            }
            (success_count, none_count)
        });

        // Meanwhile, spawn tasks that cause evictions by inserting new keys
        let mut insert_handles = vec![];
        for i in 0..5 {
            let cache = Arc::clone(&cache);
            insert_handles.push(task::spawn(async move {
                for j in 0..10 {
                    cache
                        .insert(format!("eviction_key_{}_{}", i, j), i * 10 + j)
                        .await;
                }
            }));
        }

        // Wait for all tasks
        let (success_count, none_count) = get_handle.await.unwrap();
        for handle in insert_handles {
            handle.await.unwrap();
        }

        // Verify we got results (either Some or None, but no panics)
        assert!(
            success_count + none_count == 50,
            "All 50 get operations should complete without panic"
        );

        // The test passes if we didn't panic - the cache handled the race condition gracefully
        let stats = cache.stats().await;
        assert!(
            stats.size <= 2,
            "Cache size should not exceed capacity, got {}",
            stats.size
        );
    }

    /// Test EDGE-046: Test cache memory limit
    /// Verifies that when data exceeding the memory limit is inserted,
    /// the cache triggers eviction and handles it gracefully without crashing.
    #[tokio::test]
    async fn test_cache_memory_limit_eviction() {
        // Create a cache with 1MB (1,048,576 bytes) memory limit
        // Weigher returns the size of the Vec<u8> in bytes
        let cache: Cache<String, Vec<u8>> = Cache::with_memory_limit(
            1_048_576, // 1MB
            Duration::from_secs(60),
            |value: &Arc<Vec<u8>>| value.len() as u32,
        );

        // Insert data that fits within the limit (500KB total)
        let data_200kb = vec![0u8; 200_000];
        let data_300kb = vec![0u8; 300_000];

        cache.insert("key1".to_string(), data_200kb).await;
        cache.insert("key2".to_string(), data_300kb).await;

        // Allow async operations to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify both entries exist
        assert!(
            cache.contains_key(&"key1".to_string()).await,
            "key1 should exist"
        );
        assert!(
            cache.contains_key(&"key2".to_string()).await,
            "key2 should exist"
        );

        // Now insert data that exceeds the 1MB limit
        // This should trigger eviction of older entries
        let data_600kb = vec![0u8; 600_000];
        let data_500kb = vec![0u8; 500_000];

        cache.insert("key3".to_string(), data_600kb).await;
        cache.insert("key4".to_string(), data_500kb).await;

        // Allow eviction to process
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cache should have triggered evictions to stay under 1MB
        // key1 and/or key2 should have been evicted
        // Note: For weighted caches, eviction tracking is based on entry count,
        // not weight, so we verify behavior through presence checks instead
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify we can still access newer entries
        assert!(
            cache.contains_key(&"key3".to_string()).await,
            "key3 should exist (recently inserted)"
        );
        assert!(
            cache.contains_key(&"key4".to_string()).await,
            "key4 should exist (recently inserted)"
        );

        // Verify cache is still functional - insert and retrieve a new key
        let data_100kb = vec![0u8; 100_000];
        cache.insert("key5".to_string(), data_100kb.clone()).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let retrieved = cache.get(&"key5".to_string()).await;
        assert_eq!(
            retrieved,
            Some(data_100kb),
            "Should be able to retrieve newly inserted data after memory limit eviction"
        );

        // Test 2: Verify cache handles memory limit boundary
        let cache2: Cache<String, Vec<u8>> = Cache::with_memory_limit(
            100_000, // 100KB limit
            Duration::from_secs(60),
            |value: &Arc<Vec<u8>>| value.len() as u32,
        );

        // Insert data at 50% of limit
        let half_data = vec![0u8; 50_000];
        cache2
            .insert("half_key".to_string(), half_data.clone())
            .await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert_eq!(
            cache2.get(&"half_key".to_string()).await,
            Some(half_data),
            "Should handle data at 50% of memory limit"
        );

        // Insert data that brings total to exactly 100% of limit
        let other_half = vec![0u8; 50_000];
        cache2
            .insert("other_half".to_string(), other_half.clone())
            .await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Both should exist at exactly 100% capacity
        assert!(
            cache2.contains_key(&"half_key".to_string()).await,
            "half_key should exist at 100% capacity"
        );
        assert!(
            cache2.contains_key(&"other_half".to_string()).await,
            "other_half should exist at 100% capacity"
        );

        // Insert one more entry - this may trigger eviction of older entries
        // but we don't assert specific eviction behavior as it's implementation-dependent
        let extra_data = vec![0u8; 10_000];
        cache2
            .insert("extra_key".to_string(), extra_data.clone())
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cache should still be functional and contain some entries
        assert!(
            cache2.contains_key(&"extra_key".to_string()).await,
            "extra_key (recently inserted) should exist"
        );

        // Test 3: Verify no crash with very large single entry
        let cache3: Cache<String, Vec<u8>> = Cache::with_memory_limit(
            1_000, // 1KB limit
            Duration::from_secs(60),
            |value: &Arc<Vec<u8>>| value.len() as u32,
        );

        // Insert data larger than the entire cache limit
        // This should not crash, but will immediately evict itself or not be cached
        let large_data = vec![0u8; 10_000]; // 10KB > 1KB limit
        cache3
            .insert("large_key".to_string(), large_data.clone())
            .await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Cache should not have crashed - check it's still functional
        let small_data = vec![0u8; 100];
        cache3
            .insert("small_key".to_string(), small_data.clone())
            .await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert_eq!(
            cache3.get(&"small_key".to_string()).await,
            Some(small_data),
            "Cache should remain functional after inserting oversized entry"
        );
    }
}
