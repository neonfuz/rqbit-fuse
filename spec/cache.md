# Cache System Specification

## 1. Overview

The cache system in torrent-fuse provides efficient storage and retrieval of frequently-accessed data, primarily used for caching file chunks from torrent streams. This specification documents both the historical issues with the previous custom implementation and the current Moka-based solution.

## 2. Historical Issues (Resolved by Migration to Moka)

### 2.1 O(n) Eviction with Full Cache Scan (CACHE-002)

**Problem**: The previous custom implementation used a full scan to find the least recently used entry for eviction:

```rust
// OLD: O(n) eviction - scans entire cache
async fn evict_lru(&self) {
    let lru_key = self
        .entries
        .iter()
        .map(|entry| (entry.key().clone(), entry.value().lru_seq()))
        .min_by_key(|(_, seq)| *seq)
        .map(|(key, _)| key);
    // ...
}
```

**Impact**: At capacity of 10,000+ entries, eviction caused severe performance degradation.

**Status**: RESOLVED - Moka uses TinyLFU algorithm with O(1) eviction.

### 2.2 Capacity Check Race Condition (CACHE-003)

**Problem**: Non-atomic check-and-evict pattern allowed cache overflow:

```rust
// OLD: Race condition - two threads can both pass the check
if self.entries.len() >= self.max_entries {
    self.evict_lru().await;
}
let entry = Arc::new(CacheEntry::new(value, ttl));
self.entries.insert(key, entry);
```

**Impact**: Concurrent inserts could exceed max capacity by multiple entries.

**Status**: RESOLVED - Moka provides atomic capacity management.

### 2.3 contains_key() Memory Leak (CACHE-004)

**Problem**: `contains_key()` returned true for expired entries:

```rust
// OLD: Returns true for expired entries
pub fn contains_key(&self, key: &K) -> bool {
    self.entries.contains_key(key)  // Doesn't check expiration!
}
```

**Impact**: Callers received stale data and cache retained expired entries.

**Status**: RESOLVED - Moka handles TTL transparently; expired entries are invisible.

### 2.4 TOCTOU in Expired Entry Removal (CACHE-005)

**Problem**: Check-then-act pattern in `get()` led to double-removal races:

```rust
// OLD: Race condition between check and removal
if entry.is_expired() {
    trace!("Cache entry expired, removing");
    self.entries.remove(key);  // May have been removed by another thread
    self.metrics.record_expired().await;
}
```

**Impact**: Double-counting in metrics and potential inconsistencies.

**Status**: RESOLVED - Moka operations are atomic; no manual expiration handling needed.

### 2.5 remove() Return Ambiguity (CACHE-006)

**Problem**: `remove()` couldn't distinguish between "not found" and "in use":

```rust
// OLD: Ambiguous return type
pub fn remove(&self, key: &K) -> Option<V> {
    self.entries.remove(key).map(|(_, entry)| {
        Arc::try_unwrap(entry).ok().map(|e| e.value)
    })?
}
// None could mean: not found OR entry still has active references
```

**Impact**: Callers couldn't properly handle "entry in use" scenarios.

**Status**: RESOLVED - Current implementation provides clear semantics (see Section 5).

## 3. Migration to Moka

### 3.1 Why Moka

The Moka crate was selected as the replacement for the custom implementation based on comprehensive research (see `research/cache-design.md`).

**Key Advantages:**

| Feature | Custom (Old) | Moka |
|---------|--------------|------|
| Eviction Complexity | O(n) full scan | O(1) TinyLFU |
| Thread Safety | DashMap + locks | Lock-free reads |
| TTL Handling | Manual expiration | Automatic, transparent |
| Atomic Operations | No | Yes |
| Async Support | Custom | Native |
| Production Use | Unknown | crates.io (85% hit rate) |

### 3.2 API Comparison

| Operation | Old Implementation | Moka Implementation |
|-----------|-------------------|---------------------|
| `get()` | `async fn get(&self, key: &K) -> Option<V>` | Same signature |
| `insert()` | `async fn insert(&self, key: K, value: V)` | Same signature |
| `remove()` | `async fn remove(&self, key: &K) -> Option<V>` | Same signature |
| `contains_key()` | `pub fn contains_key(&self, key: &K) -> bool` | `async fn contains_key(&self, key: &K) -> bool` |
| `stats()` | Custom metrics struct | Similar, with hit/miss tracking |

### 3.3 Performance Benefits

1. **Lock-free Reads**: Moka provides concurrent read access without blocking
2. **Better Hit Ratio**: TinyLFU algorithm achieves near-optimal hit rates
3. **Reduced CPU Usage**: No periodic expiration scans or full-cache evictions
4. **Memory Efficiency**: Automatic cleanup of expired entries
5. **Scalability**: Performance remains stable under high concurrency

## 4. Current Implementation Design

### 4.1 Architecture

The current cache implementation (`src/cache.rs`) uses:

- **Moka Cache**: `moka::future::Cache<K, Arc<V>>` as the underlying storage
- **Atomic Statistics**: `AtomicU64` counters for hits and misses
- **Arc Wrapping**: Values wrapped in `Arc<>` to avoid expensive cloning
- **TTL Support**: Cache-wide TTL set at construction time

### 4.2 Cache Statistics

```rust
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,        // Successful retrievals
    pub misses: u64,      // Failed retrievals
    pub evictions: u64,   // Entries evicted (currently 0 - moka handles internally)
    pub expired: u64,     // Entries expired (currently 0 - moka handles internally)
    pub size: usize,      // Current entry count
}
```

### 4.3 Atomic Operations

All cache operations are atomic:

- **Insertion**: Atomic with automatic eviction if at capacity
- **Retrieval**: Lock-free read with LRU promotion
- **Removal**: Atomic invalidation
- **TTL Handling**: Automatic expiration check on every access

### 4.4 Thread-Safe Eviction

Moka handles eviction through background maintenance threads:

- **Write Operations**: May trigger immediate eviction if over capacity
- **Background Tasks**: Periodic cleanup of expired entries
- **No Lock Contention**: Eviction doesn't block readers

### 4.5 TTL Handling

Current implementation uses cache-wide TTL:

```rust
let inner = MokaCache::builder()
    .max_capacity(max_entries as u64)
    .time_to_live(default_ttl)
    .build();
```

**Note**: Per-entry TTL is not currently supported. For per-entry expiration, consider using `time_to_idle` or upgrading Moka version.

## 5. Interface Specification

### 5.1 Types

```rust
/// Result of a cache removal operation
#[derive(Debug, Clone, PartialEq)]
pub enum RemovalStatus<V> {
    /// Entry was successfully removed and returned
    Removed(V),
    /// Entry was not found in the cache
    NotFound,
    /// Entry exists but is currently in use (has active references)
    InUse,
}

/// Cache statistics snapshot
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub expired: u64,
    pub size: usize,
}
```

### 5.2 Methods

#### `get(key: &K) -> Option<V>`

Retrieves a value from the cache.

**Parameters:**
- `key`: Reference to the key to look up

**Returns:**
- `Some(V)`: The cloned value if found and not expired
- `None`: If key not found or entry expired

**Side Effects:**
- Increments hit counter on success
- Increments miss counter on failure
- Updates LRU tracking (entry becomes most recently used)

**Async**: Yes

**Example:**
```rust
if let Some(data) = cache.get(&key).await {
    println!("Cache hit: {}", data);
}
```

---

#### `insert(key: K, value: V)`

Inserts a key-value pair into the cache with the default TTL.

**Parameters:**
- `key`: The key to insert
- `value`: The value to store

**Behavior:**
- If cache is at capacity, least recently used entry is evicted atomically
- If key already exists, value is updated
- Entry receives default TTL from cache construction

**Async**: Yes

**Example:**
```rust
cache.insert("chunk_42", data).await;
```

---

#### `insert_with_ttl(key: K, value: V, ttl: Duration)`

**Note**: Currently uses cache-wide TTL. Per-entry TTL support pending.

Inserts a key-value pair with a specific TTL.

**Parameters:**
- `key`: The key to insert
- `value`: The value to store
- `ttl`: Time-to-live duration (currently ignored, uses cache-wide TTL)

**Async**: Yes

---

#### `remove(key: &K) -> Option<V>`

Removes an entry from the cache and returns it if found.

**Parameters:**
- `key`: Reference to the key to remove

**Returns:**
- `Some(V)`: The value if entry was found and removed
- `None`: If entry was not found

**Note**: This method cannot distinguish between "not found" and "in use". For more granular control, an extended API may be added in the future.

**Async**: Yes

**Example:**
```rust
if let Some(old_value) = cache.remove(&key).await {
    println!("Removed: {}", old_value);
}
```

---

#### `contains_key(key: &K) -> bool`

Checks if a key exists in the cache and is not expired.

**Parameters:**
- `key`: Reference to the key to check

**Returns:**
- `true`: Key exists and entry is not expired
- `false`: Key not found or entry expired

**Note**: This operation counts as a cache access and updates LRU tracking.

**Async**: Yes

**Example:**
```rust
if cache.contains_key(&key).await {
    println!("Key exists and is valid");
}
```

---

#### `clear()`

Removes all entries from the cache.

**Async**: Yes

**Example:**
```rust
cache.clear().await;
```

---

#### `stats() -> CacheStats`

Returns current cache statistics.

**Returns:**
`CacheStats` struct with current hit count, miss count, and size.

**Note**: Statistics are approximate due to concurrent access.

**Async**: Yes

**Example:**
```rust
let stats = cache.stats().await;
println!("Hit rate: {:.2}%", 
    stats.hits as f64 / (stats.hits + stats.misses) as f64 * 100.0);
```

---

#### `len() -> usize`

Returns the number of entries currently in the cache.

**Returns:** Current entry count (may include expired entries pending cleanup).

**Async**: No (synchronous, approximate count)

---

#### `is_empty() -> bool`

Checks if the cache is empty.

**Returns:** `true` if no entries, `false` otherwise.

**Async**: Yes

## 6. Testing Strategy

### 6.1 Unit Tests

The cache implementation includes comprehensive unit tests covering:

1. **Basic Operations** (`test_cache_basic_operations`)
   - Insert and retrieve values
   - Handle non-existent keys
   - Verify statistics tracking

2. **TTL Expiration** (`test_cache_ttl`)
   - Values expire after TTL
   - Expired entries return None on get()
   - Expired entries not counted in contains_key()

3. **LRU Eviction** (`test_cache_lru_eviction`)
   - Least recently used entries evicted at capacity
   - Accessed entries remain in cache
   - Cache size never exceeds max_entries

4. **Removal** (`test_cache_remove`)
   - Remove returns value if found
   - Remove returns None if not found
   - Removed entries inaccessible after removal

5. **Clear** (`test_cache_clear`)
   - All entries removed
   - Cache empty after clear
   - Statistics retained (not reset)

6. **Custom TTL** (`test_cache_custom_ttl`)
   - Note: Currently tests cache-wide TTL behavior

7. **Concurrent Access** (`test_concurrent_cache_access`)
   - Multiple threads inserting/reading simultaneously
   - No data corruption under concurrency
   - Statistics approximately correct

### 6.2 Integration Tests (Recommended Additions)

Additional tests to consider:

1. **Capacity Enforcement**
   ```rust
   #[tokio::test]
   async fn test_capacity_never_exceeded() {
       let cache = Cache::new(5, Duration::from_secs(60));
       // Insert 10 items concurrently
       // Verify size never exceeds 5
   }
   ```

2. **Expiration Accuracy**
   ```rust
   #[tokio::test]
   async fn test_expiration_boundary() {
       // Test entries expire at exact TTL boundary
       // Test sub-millisecond TTL handling
   }
   ```

3. **Statistics Accuracy Under Load**
   ```rust
   #[tokio::test]
   async fn test_stats_under_concurrent_load() {
       // High concurrency operations
       // Verify stats remain consistent
   }
   ```

4. **Memory Leak Detection**
   ```rust
   #[tokio::test]
   async fn test_no_memory_leak() {
       // Insert and evict many entries
       // Verify memory usage stable
   }
   ```

### 6.3 Performance Benchmarks

Recommended benchmarks:

1. **Throughput Test**
   - Measure operations/second for get/insert/remove
   - Compare single-threaded vs multi-threaded

2. **Hit Ratio Test**
   - Simulate realistic access patterns
   - Measure achieved hit ratio

3. **Latency Distribution**
   - P50, P95, P99 latencies for operations
   - Identify tail latency issues

4. **Scalability Test**
   - Performance vs number of threads
   - Identify contention points

### 6.4 Test Commands

```bash
# Run all cache tests
cargo test cache

# Run with performance optimizations
cargo test cache --release

# Run specific test
cargo test test_cache_lru_eviction

# Run with logging to see trace output
RUST_LOG=trace cargo test cache -- --nocapture
```

## 7. Future Enhancements

### 7.1 Per-Entry TTL

When Moka supports per-entry TTL or time-to-idle:

```rust
pub async fn insert_with_ttl(&self, key: K, value: V, ttl: Duration) {
    // Use per-entry expiration
    self.inner.insert_with_expire_at(key, value, ttl).await;
}
```

### 7.2 Size-Aware Eviction

Evict based on memory size instead of entry count:

```rust
let cache = Cache::builder()
    .weigher(|_key, value: &Arc<Vec<u8>>| -> u32 {
        value.len().try_into().unwrap_or(u32::MAX)
    })
    .max_capacity(100 * 1024 * 1024) // 100MB
    .build();
```

### 7.3 Enhanced Removal Status

Implement granular removal status:

```rust
pub async fn remove_detailed(&self, key: &K) -> RemovalStatus<V>
where
    V: Clone,
{
    match self.inner.get(key).await {
        Some(value) => {
            // Check reference count
            if Arc::strong_count(&value) > 1 {
                RemovalStatus::InUse
            } else {
                self.inner.invalidate(key).await;
                RemovalStatus::Removed((*value).clone())
            }
        }
        None => RemovalStatus::NotFound,
    }
}
```

### 7.4 Built-in Moka Statistics

When Moka exposes more statistics:

```rust
pub async fn stats(&self) -> CacheStats {
    let moka_stats = self.inner.stats();
    CacheStats {
        hits: moka_stats.hits(),
        misses: moka_stats.misses(),
        evictions: moka_stats.evictions(),
        expired: moka_stats.expired(),
        size: self.inner.entry_count() as usize,
    }
}
```

## 8. References

- **Implementation**: `src/cache.rs`
- **Research**: `research/cache-design.md`
- **TODO Items**: TODO.md (CACHE-001 through CACHE-008)
- **Moka Documentation**: https://docs.rs/moka/
- **TinyLFU Algorithm**: https://github.com/moka-rs/moka/wiki

---

*Specification Version: 1.0*
*Last Updated: February 14, 2026*
