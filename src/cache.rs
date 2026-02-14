use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, trace};

/// Global LRU counter for cache entries
static LRU_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A cache entry with TTL and LRU tracking
struct CacheEntry<T> {
    /// The cached value
    value: T,
    /// When this entry was added
    created_at: Instant,
    /// Time-to-live duration
    ttl: Duration,
    /// Access count for statistics
    access_count: AtomicU64,
    /// LRU sequence number (lower = older)
    lru_seq: AtomicU64,
}

impl<T> CacheEntry<T> {
    /// Create a new cache entry
    fn new(value: T, ttl: Duration) -> Self {
        Self {
            value,
            created_at: Instant::now(),
            ttl,
            access_count: AtomicU64::new(0),
            lru_seq: AtomicU64::new(LRU_COUNTER.fetch_add(1, Ordering::SeqCst)),
        }
    }

    /// Check if the entry has expired
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }

    /// Record an access to this entry
    fn record_access(&self) {
        self.access_count.fetch_add(1, Ordering::Relaxed);
        self.lru_seq
            .store(LRU_COUNTER.fetch_add(1, Ordering::SeqCst), Ordering::SeqCst);
    }

    /// Get the LRU sequence number
    fn lru_seq(&self) -> u64 {
        self.lru_seq.load(Ordering::SeqCst)
    }
}

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
/// The cache stores key-value pairs with an optional TTL (time-to-live).
/// When the cache reaches its maximum size, entries are evicted using
/// an LRU (Least Recently Used) policy.
pub struct Cache<K, V> {
    /// Storage for cache entries
    entries: DashMap<K, Arc<CacheEntry<V>>>,
    /// Maximum number of entries allowed
    max_entries: usize,
    /// Default TTL for entries
    default_ttl: Duration,
    /// Statistics
    stats: RwLock<CacheStats>,
}

impl<K, V> Cache<K, V>
where
    K: std::hash::Hash + Eq + Clone + Send + Sync + 'static,
    V: Send + Sync + 'static,
{
    /// Create a new cache with the specified maximum size and default TTL
    pub fn new(max_entries: usize, default_ttl: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            max_entries,
            default_ttl,
            stats: RwLock::new(CacheStats::default()),
        }
    }

    /// Get a value from the cache.
    /// Returns None if the key is not found or the entry has expired.
    pub async fn get(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        // Check if entry exists
        let entry = match self.entries.get(key) {
            Some(e) => e.clone(),
            None => {
                self.record_miss().await;
                return None;
            }
        };

        // Check if expired
        if entry.is_expired() {
            trace!("Cache entry expired, removing");
            self.entries.remove(key);
            self.record_expired().await;
            self.record_miss().await;
            return None;
        }

        // Record access and return value
        entry.record_access();
        self.record_hit().await;
        Some(entry.value.clone())
    }

    /// Insert a value into the cache.
    /// If the cache is at capacity, the least recently used entry is evicted.
    pub async fn insert(&self, key: K, value: V) {
        self.insert_with_ttl(key, value, self.default_ttl).await;
    }

    /// Insert a value with a custom TTL.
    pub async fn insert_with_ttl(&self, key: K, value: V, ttl: Duration) {
        // Evict expired entries first
        self.evict_expired().await;

        // If at capacity, evict LRU entry
        if self.entries.len() >= self.max_entries {
            self.evict_lru().await;
        }

        let entry = Arc::new(CacheEntry::new(value, ttl));
        self.entries.insert(key, entry);

        let mut stats = self.stats.write().await;
        stats.size = self.entries.len();
    }

    /// Remove a specific entry from the cache
    pub fn remove(&self, key: &K) -> Option<V> {
        self.entries.remove(key).map(|(_, entry)| {
            // This is safe because we own the Arc at this point
            Arc::try_unwrap(entry).ok().map(|e| e.value)
        })?
    }

    /// Clear all entries from the cache
    pub fn clear(&self) {
        self.entries.clear();
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let mut stats = self.stats.read().await.clone();
        stats.size = self.entries.len();
        stats
    }

    /// Check if a key exists in the cache (without updating access time)
    pub fn contains_key(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    /// Get the number of entries in the cache
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Record a cache hit
    async fn record_hit(&self) {
        let mut stats = self.stats.write().await;
        stats.hits += 1;
    }

    /// Record a cache miss
    async fn record_miss(&self) {
        let mut stats = self.stats.write().await;
        stats.misses += 1;
    }

    /// Record an eviction
    async fn record_eviction(&self) {
        let mut stats = self.stats.write().await;
        stats.evictions += 1;
    }

    /// Record an expired entry removal
    async fn record_expired(&self) {
        let mut stats = self.stats.write().await;
        stats.expired += 1;
    }

    /// Evict all expired entries
    async fn evict_expired(&self) {
        let expired_keys: Vec<K> = self
            .entries
            .iter()
            .filter(|entry| entry.value().is_expired())
            .map(|entry| entry.key().clone())
            .collect();

        for key in expired_keys {
            self.entries.remove(&key);
            self.record_expired().await;
        }
    }

    /// Evict the least recently used entry
    async fn evict_lru(&self) {
        // Find the entry with the lowest LRU sequence number (oldest)
        let lru_key = self
            .entries
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().lru_seq()))
            .min_by_key(|(_, seq)| *seq)
            .map(|(key, _)| key);

        if let Some(key) = lru_key {
            self.entries.remove(&key);
            self.record_eviction().await;
            debug!("Evicted LRU entry");
        }
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
        assert_eq!(stats.expired, 1);
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
        assert_eq!(stats.evictions, 1);
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

        // Insert with custom short TTL
        cache
            .insert_with_ttl("key1".to_string(), 42, Duration::from_millis(50))
            .await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some(42));

        // Wait for custom TTL expiration
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(cache.get(&"key1".to_string()).await, None);
    }
}
