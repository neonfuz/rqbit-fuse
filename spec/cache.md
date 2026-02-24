# Cache System

## Overview

The cache system uses Moka for efficient metadata caching with TTL support.

## Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub metadata_ttl: u64,      // seconds
    pub max_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            metadata_ttl: 60,
            max_entries: 1000,
        }
    }
}
```

## Implementation

Uses `moka::future::Cache<K, Arc<V>>` with:
- **TTL**: Cache-wide time-to-live for all entries
- **Capacity**: Maximum entry count with automatic eviction
- **Thread Safety**: Lock-free concurrent access

## Interface

```rust
impl<K: Hash + Eq + Send + Sync + 'static, V: Send + Sync + 'static> Cache<K, V> {
    pub fn new(max_entries: usize, default_ttl: Duration) -> Self;
    pub async fn get(&self, key: &K) -> Option<V>;
    pub async fn insert(&self, key: K, value: V);
    pub async fn remove(&self, key: &K) -> Option<V>;
    pub async fn contains_key(&self, key: &K) -> bool;
    pub async fn clear(&self);
    pub async fn is_empty(&self) -> bool;
}
```

## FUSE Integration

The cache is used for:
- Metadata caching (inode entries, file attributes)
- No bitfield caching (synchronous checking)

## Environment Variables

- `METADATA_TTL`: Override metadata TTL (seconds)
- `MAX_ENTRIES`: Override max cache entries

---

*Last Updated: February 2026*
