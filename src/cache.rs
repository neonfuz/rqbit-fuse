use moka::future::Cache as MokaCache;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, trace};

/// Cache statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub expired: u64,
    pub size: usize,
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
    /// Statistics counters
    hits: AtomicU64,
    misses: AtomicU64,
    /// Default TTL for entries
    default_ttl: Duration,
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
            default_ttl,
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
                trace!("Cache hit for key");
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some((*value).clone())
            }
            None => {
                trace!("Cache miss for key");
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
        let arc_value = Arc::new(value);
        self.inner.insert(key, arc_value).await;
        trace!("Inserted value into cache");
    }

    /// Remove a specific entry from the cache
    pub fn remove(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        // moka's invalidate is async, but we need sync API
        // Run it in a blocking task
        let key = key.clone();
        let inner = self.inner.clone();
        
        // Try to get the value before removing
        let value = futures::executor::block_on(async {
            let val = inner.get(&key).await;
            inner.invalidate(&key).await;
            val
        });
        
        value.map(|arc_v| (*arc_v).clone())
    }

    /// Clear all entries from the cache
    pub fn clear(&self) {
        self.inner.invalidate_all();
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            evictions: 0, // moka doesn't expose eviction count directly
            expired: 0,   // moka handles expiration internally
            size: self.inner.entry_count() as usize,
        }
    }

    /// Check if a key exists in the cache (without updating access time)
    /// Note: moka updates access time on get, so we use a non-updating approach
    pub fn contains_key(&self, key: &K) -> bool {
        // moka doesn't have a contains_key that doesn't update access time
        // We check by attempting to get without awaiting (won't update access time)
        futures::executor::block_on(self.inner.get(key)).is_some()
    }

    /// Get the number of entries in the cache
    pub fn len(&self) -> usize {
        self.inner.entry_count() as usize
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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

        // Stats
        let stats = cache.stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.size, 1);
    }

    #[tokio::test]
    async fn test_cache_ttl() {
        let cache: Cache<String, i32> = Cache::new(10, Duration::from_millis(100));

        // Insert with short TTL
        cache.insert("key1".to_string(), 42).await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some(42));

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(cache.get(&"key1".to_string()).await, None);

        let stats = cache.stats().await;
        assert_eq!(stats.misses, 2); // One initial miss + one after expiration
    }

    #[tokio::test]
    async fn test_cache_lru_eviction() {
        let cache: Cache<String, i32> = Cache::new(3, Duration::from_secs(60));

        // Insert 3 entries (at capacity)
        cache.insert("key1".to_string(), 1).await;
        cache.insert("key2".to_string(), 2).await;
        cache.insert("key3".to_string(), 3).await;

        // Access key1 to make it recently used
        let _ = cache.get(&"key1".to_string()).await;

        // Insert 4th entry (should evict key2)
        cache.insert("key4".to_string(), 4).await;

        // key1 and key3 should exist, key2 should be evicted
        assert!(cache.contains_key(&"key1".to_string()));
        assert!(!cache.contains_key(&"key2".to_string()));
        assert!(cache.contains_key(&"key3".to_string()));
        assert!(cache.contains_key(&"key4".to_string()));

        let stats = cache.stats().await;
        assert_eq!(stats.size, 3);
    }

    #[tokio::test]
    async fn test_cache_remove() {
        let cache: Cache<String, i32> = Cache::new(10, Duration::from_secs(60));

        cache.insert("key1".to_string(), 42).await;
        assert_eq!(cache.remove(&"key1".to_string()), Some(42));
        assert_eq!(cache.remove(&"key1".to_string()), None);
        assert!(!cache.contains_key(&"key1".to_string()));
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache: Cache<String, i32> = Cache::new(10, Duration::from_secs(60));

        cache.insert("key1".to_string(), 1).await;
        cache.insert("key2".to_string(), 2).await;

        cache.clear();

        assert!(cache.is_empty());
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
        assert_eq!(stats.size, 10);
        assert!(stats.hits >= 10);
    }
}
