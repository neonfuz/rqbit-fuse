# Cache Design Research

## Executive Summary

This document compares cache design approaches for the rqbit-fuse project. After analyzing the current implementation and available options, **Moka** is the recommended replacement for the custom cache implementation.

## Current Implementation Analysis

### File: `src/cache.rs`

The current implementation uses:
- **DashMap** for concurrent hash map storage
- **Custom LRU tracking** via global atomic counter
- **TTL support** via `Instant` timestamps
- **Async statistics** with `tokio::sync::RwLock`

### Current Issues Identified

#### 1. O(n) LRU Eviction (CACHE-002)
```rust
// Line 224-238: Full scan to find LRU entry
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
**Problem**: Scans entire cache to find oldest entry. At capacity=10,000, this is expensive.

#### 2. Race Condition on Capacity Check (CACHE-003)
```rust
// Lines 137-143: Non-atomic check-and-evict
if self.entries.len() >= self.max_entries {
    self.evict_lru().await;
}
let entry = Arc::new(CacheEntry::new(value, ttl));
self.entries.insert(key, entry);
```
**Problem**: Two concurrent inserts can both see `len() < max_entries`, exceeding capacity.

#### 3. Expired Entry in contains_key() (CACHE-004)
```rust
// Line 170-172
pub fn contains_key(&self, key: &K) -> bool {
    self.entries.contains_key(key)  // Doesn't check expiration!
}
```
**Problem**: Returns true for expired entries, causing stale data usage.

#### 4. TOCTOU in Expired Entry Removal (CACHE-005)
```rust
// Lines 111-118: Check-then-act race
if entry.is_expired() {
    trace!("Cache entry expired, removing");
    self.entries.remove(key);  // May have been removed by another thread
    // ...
}
```
**Problem**: Entry could be removed between check and removal, causing double-counting.

#### 5. Ambiguous remove() Return (CACHE-006)
```rust
// Line 150-155
pub fn remove(&self, key: &K) -> Option<V> {
    self.entries.remove(key).map(|(_, entry)| {
        Arc::try_unwrap(entry).ok().map(|e| e.value)
    })?
}
```
**Problem**: Can't distinguish "not found" from "entry in use by another thread".

## Alternative Approaches

### Option 1: `lru` Crate

**Repository**: https://github.com/jeromefroe/lru-rs
**Crates.io**: https://crates.io/crates/lru

#### Features
- Pure LRU implementation with O(1) operations
- Simple API: `put`, `get`, `get_mut`, `pop`
- Non-concurrent (requires external synchronization)
- MSRV: 1.70.0

#### Pros
- Simple, focused implementation
- True O(1) LRU eviction via linked list
- Well-tested, stable API
- Small dependency footprint

#### Cons
- **Not thread-safe** - requires wrapping in Mutex/RwLock
- No built-in TTL support
- No async support
- Would need significant wrapper code

#### Usage Example
```rust
use lru::LruCache;
use std::num::NonZeroUsize;

let mut cache = LruCache::new(NonZeroUsize::new(100).unwrap());
cache.put("key", value);
let val = cache.get(&"key");
```

#### Verdict
❌ **Not recommended** - Requires too much wrapper code for concurrency and TTL.

---

### Option 2: `cached` Crate

**Repository**: https://github.com/jaemk/cached
**Crates.io**: https://crates.io/crates/cached

#### Features
- Memoization-focused with procedural macros
- Multiple cache stores: SizedCache, TimedCache, TimedSizedCache
- Redis and disk cache support
- Async support via features

#### Pros
- Excellent for function memoization
- Built-in TTL support via TimedCache
- Macro-based caching (`#[cached]`, `#[once]`)
- Flexible cache backends

#### Cons
- **Designed for memoization**, not general-purpose caching
- Async support is macro-based, not direct API
- Less control over cache behavior
- Overhead from macro expansion

#### Usage Example
```rust
use cached::proc_macro::cached;

#[cached(size = 100, time = 300)]
async fn fetch_data(key: &str) -> Vec<u8> {
    // async function whose results are cached
}
```

#### Verdict
❌ **Not recommended** - Wrong abstraction; designed for memoization, not the general caching needed for file chunks.

---

### Option 3: `moka` Crate ⭐ RECOMMENDED

**Repository**: https://github.com/moka-rs/moka
**Crates.io**: https://crates.io/crates/moka

#### Features
- **Concurrent** - Thread-safe, lock-free retrievals
- **Async support** - Native async/await API
- **TTL and TTI** - Time-to-live and time-to-idle expiration
- **Size-aware eviction** - Evict by entry weight, not just count
- **TinyLFU** - Near-optimal hit ratio algorithm
- **Production proven** - Used by crates.io (85% hit rate)

#### Pros
- ✅ **True O(1) operations** - No full scans
- ✅ **Atomic operations** - No race conditions
- ✅ **Built-in TTL** - No custom expiration logic needed
- ✅ **Async-native** - First-class async support
- ✅ **High concurrency** - Lock-free reads
- ✅ **Statistics** - Built-in hit/miss metrics (coming in future version)
- ✅ **Battle-tested** - Used in production at scale
- ✅ **Size-aware** - Can evict by byte size, not just entry count

#### Cons
- Larger dependency tree than `lru`
- More complex API (but well-documented)
- MSRV: 1.71.1

#### Usage Example
```rust
use moka::future::Cache;
use std::time::Duration;

let cache = Cache::builder()
    .max_capacity(10_000)
    .time_to_live(Duration::from_secs(300))
    .build();

// Insert
cache.insert(key, value).await;

// Get
let value = cache.get(&key).await;

// Atomic get-or-insert
let value = cache.get_with(key, async {
    // compute value if not present
    compute().await
}).await;
```

#### Verdict
✅ **RECOMMENDED** - Solves all current cache issues with minimal code.

---

## Comparison Matrix

| Feature | Current | `lru` | `cached` | `moka` |
|---------|---------|-------|----------|--------|
| O(1) eviction | ❌ | ✅ | ✅ | ✅ |
| Thread-safe | ✅ | ❌ | ✅ | ✅ |
| Async support | ✅ | ❌ | ⚠️ | ✅ |
| Built-in TTL | ✅ | ❌ | ✅ | ✅ |
| Atomic operations | ❌ | N/A | ✅ | ✅ |
| Hit/miss stats | ✅ | ❌ | ❌ | ✅ |
| Size-aware eviction | ❌ | ❌ | ❌ | ✅ |
| Production proven | ❓ | ✅ | ✅ | ✅ |

## Recommended Solution: Moka

### Migration Plan

#### Phase 1: Add Dependency
```toml
[dependencies]
moka = { version = "0.12", features = ["future"] }
```

#### Phase 2: Replace Cache Implementation
```rust
use moka::future::Cache;
use std::time::Duration;

pub struct ChunkCache {
    inner: Cache<String, Arc<Vec<u8>>>,
}

impl ChunkCache {
    pub fn new(max_entries: u64, ttl: Duration) -> Self {
        Self {
            inner: Cache::builder()
                .max_capacity(max_entries)
                .time_to_live(ttl)
                .build(),
        }
    }
    
    pub async fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        self.inner.get(key).await
    }
    
    pub async fn insert(&self, key: String, value: Arc<Vec<u8>>) {
        self.inner.insert(key, value).await;
    }
}
```

#### Phase 3: Update Callers
- Wrap values in `Arc` to avoid expensive cloning
- Update all cache interactions to use `.await`
- Remove custom expiration logic

#### Phase 4: Add Size-Aware Eviction (Optional)
```rust
let cache = Cache::builder()
    .weigher(|_key, value: &Arc<Vec<u8>>| -> u32 {
        value.len().try_into().unwrap_or(u32::MAX)
    })
    .max_capacity(100 * 1024 * 1024) // 100MB
    .time_to_live(Duration::from_secs(300))
    .build();
```

### Benefits of Migration

1. **Fixes CACHE-002**: O(1) eviction via TinyLFU algorithm
2. **Fixes CACHE-003**: Atomic capacity management
3. **Fixes CACHE-004**: Transparent TTL handling
4. **Fixes CACHE-005**: No TOCTOU - operations are atomic
5. **Fixes CACHE-006**: Clear API semantics
6. **Enables CACHE-007**: Built-in statistics (hit rate, etc.)
7. **Better Performance**: Lock-free reads, better hit ratio

### Risks and Mitigation

| Risk | Mitigation |
|------|------------|
| Dependency size | Moka is larger but production-proven; use `mini-moka` if size is concern |
| API changes | Well-documented migration path; similar API shape |
| Performance regression | Benchmark before/after; moka is typically faster |
| MSRV increase | Current: 1.70; Moka requires 1.71.1 - minimal impact |

### Alternative: Mini Moka

If dependency size is a concern, consider `mini-moka`:
- Smaller footprint
- TTL support
- No background threads (v0.12+)
- Less feature-rich but sufficient for basic caching

## Conclusion

**Moka is the clear winner** for rqbit-fuse's caching needs:

1. Solves all identified issues with current implementation
2. Provides advanced features (size-aware eviction, excellent hit ratio)
3. Production-proven and actively maintained
4. Native async support fits the project's architecture
5. Minimal migration effort required

**Next Steps**:
1. Add `moka` to Cargo.toml
2. Create new cache module using moka
3. Migrate existing code
4. Benchmark and verify
5. Remove old cache implementation

## References

- [Moka Documentation](https://docs.rs/moka/)
- [Moka Migration Guide](https://github.com/moka-rs/moka/blob/main/MIGRATION-GUIDE.md)
- [TinyLFU Algorithm](https://github.com/moka-rs/moka/wiki#admission-and-eviction-policies)
- [Crates.io Cache Case Study](https://github.com/moka-rs/moka/discussions/51)
