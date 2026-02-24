# Cache System

## Overview

The cache system provides minimal caching for API responses to reduce redundant network requests. Currently, only the `list_torrents` API response is cached.

## Configuration

The `CacheConfig` struct is defined but **not currently used** by the cache implementation:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub metadata_ttl: u64,      // seconds - NOT USED
    pub max_entries: usize,     // NOT USED
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

**Note:** These configuration values exist for future use but are not currently connected to any caching logic.

## Implementation

The cache is implemented in `RqbitClient` using a simple `RwLock`-protected optional value:

```rust
list_torrents_cache: Arc<RwLock<Option<(Instant, ListTorrentsResult)>>>,
list_torrents_cache_ttl: Duration,  // Hardcoded to 30 seconds
```

**Characteristics:**
- **Type**: Single-entry cache with timestamp
- **TTL**: Fixed at 30 seconds (not configurable)
- **Thread Safety**: Uses `tokio::sync::RwLock` for concurrent access
- **No eviction**: Simple replacement on each insert
- **No capacity limits**: Only one entry is ever cached

### Cache Operations

```rust
// Check cache (read lock)
async fn get_cached_list(&self) -> Option<ListTorrentsResult>

// Insert into cache (write lock)
async fn insert_list_cache(&self, result: ListTorrentsResult)

// Invalidate cache (write lock)
async fn invalidate_list_torrents_cache(&self)

// Clear cache - for testing only
async fn __test_clear_cache(&self)
```

## Cache Usage

### Cached Endpoints

| Endpoint | Cached | TTL | Invalidation |
|----------|--------|-----|--------------|
| `list_torrents` | Yes | 30s | On add/forget/delete torrent |
| `get_torrent` | No | - | - |
| `get_torrent_stats` | No | - | - |
| `get_piece_bitfield` | No | - | - |
| `read_file` | No | - | - |

### Cache Invalidation

The `list_torrents` cache is automatically invalidated when:
- A torrent is added (magnet or URL)
- A torrent is forgotten
- A torrent is deleted

## Metrics

Cache performance is tracked via the `Metrics` struct:

```rust
pub struct Metrics {
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
}
```

Methods:
- `record_cache_hit()` - Increment hit counter
- `record_cache_miss()` - Increment miss counter

The hit rate is logged on shutdown:
```
cache_hits = N, cache_misses = M, cache_hit_rate_pct = X.XX
```

## Environment Variables

- `TORRENT_FUSE_METADATA_TTL`: Configurable value (not currently used by cache)
- `TORRENT_FUSE_MAX_ENTRIES`: Configurable value (not currently used by cache)

**Note:** These environment variables update the `CacheConfig` struct but do not affect the actual cache behavior since the cache uses hardcoded values.

## Future Improvements

Potential enhancements for the cache system:

1. **Connect CacheConfig**: Use `metadata_ttl` from configuration instead of hardcoded 30s
2. **Add Moka dependency**: Replace simple RwLock cache with `moka::future::Cache` for:
   - Configurable TTL per entry type
   - Capacity-based eviction
   - Better concurrency (lock-free reads)
3. **Extend caching to**:
   - Individual torrent metadata
   - File attributes (stat results)
   - Piece bitfields (with shorter TTL)
4. **Add cache statistics endpoint**: Expose metrics via API

## Files

- `src/api/client.rs` - Cache implementation in `RqbitClient`
- `src/config/mod.rs` - `CacheConfig` struct definition
- `src/metrics.rs` - Cache hit/miss tracking

---

*Last Updated: February 2026*
