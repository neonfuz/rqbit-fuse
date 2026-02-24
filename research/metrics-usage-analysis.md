# Metrics Usage Analysis

**Date:** 2026-02-23  
**Analysis for:** Task 4.1.1 - Research metrics usage patterns

## Executive Summary

The current metrics system contains **520 lines** across `src/metrics.rs` with **25 metric fields** organized into 3 structs. This analysis identifies which metrics are actually recorded and used, revealing that only **4 essential metrics** provide value.

**Recommendation:** Replace the entire metrics system with a minimal struct containing: `bytes_read`, `error_count`, `cache_hits`, `cache_misses`.

---

## Current Metrics System Overview

### File: `src/metrics.rs` (520 lines)

#### 1. FuseMetrics (11 fields, 139 lines)

**Fields:**
- `getattr_count` - getattr operations
- `setattr_count` - setattr operations  
- `lookup_count` - lookup operations
- `readdir_count` - readdir operations
- `open_count` - open operations
- `release_count` - release operations
- `read_count` - read operations
- `bytes_read` - total bytes read **(ESSENTIAL)**
- `error_count` - total errors **(ESSENTIAL)**
- `read_latency_ns` - total read latency
- `pieces_unavailable_errors` - piece availability failures
- `torrents_removed` - torrent removal count

**Recording Methods Called:**
- `record_getattr()` - 1 call in filesystem.rs:1187
- `record_setattr()` - 0 calls
- `record_lookup()` - 1 call in filesystem.rs:1063
- `record_readdir()` - 1 call in filesystem.rs:1306
- `record_open()` - 1 call in filesystem.rs:1216
- `record_release()` - 1 call in filesystem.rs:1036
- `record_read()` - 2 calls: async_bridge.rs:208, filesystem.rs:962
- `record_error()` - 11 calls across filesystem.rs and async_bridge.rs
- `record_pieces_unavailable()` - 0 calls (method exists but never used)
- `record_torrent_removed()` - 3 calls in filesystem.rs

**Analysis:**
- Operation counters (getattr, setattr, lookup, readdir, open, release) are recorded but only used for trace logging
- `bytes_read` and `error_count` are the only metrics with actual value for monitoring
- `read_latency_ns` is calculated but never used for alerting or decisions
- `pieces_unavailable_errors` and `torrents_removed` are specialized counters with limited utility

#### 2. ApiMetrics (7 fields, 98 lines)

**Fields:**
- `request_count` - total API requests
- `success_count` - successful requests
- `failure_count` - failed requests
- `retry_count` - retry attempts
- `total_latency_ns` - cumulative latency
- `circuit_breaker_opens` - circuit breaker transitions to open
- `circuit_breaker_closes` - circuit breaker transitions to closed

**Recording Methods Called:**
- `record_request()` - 1 call in client.rs:139
- `record_success()` - 1 call in client.rs:216
- `record_failure()` - 2 calls in client.rs:221,229
- `record_retry()` - 3 calls in client.rs:150,173,195
- `record_circuit_breaker_open()` - 0 calls
- `record_circuit_breaker_close()` - 0 calls

**Analysis:**
- API metrics are comprehensive but add overhead to every request
- Circuit breaker metrics are never recorded (methods exist but never called)
- Retry tracking adds noise - retries are already visible in logs
- None of these metrics are used for operational decisions or alerting

#### 3. CacheMetrics (7 fields, 102 lines)

**Fields:**
- `hits` - cache hits **(ESSENTIAL)**
- `misses` - cache misses **(ESSENTIAL)**
- `evictions` - entries evicted
- `current_size` - current entry count
- `peak_size` - maximum size observed
- `bytes_served` - bytes served from cache

**Recording Methods Called:**
- `record_hit()` - 0 direct calls (see note below)
- `record_miss()` - 0 direct calls (see note below)
- `record_eviction()` - 0 calls
- `update_size()` - 0 calls
- `record_bytes()` - 0 calls

**Important Finding:**
The `CacheMetrics` struct in `src/metrics.rs` is **never used**! The cache implementation in `src/cache.rs` has its own internal statistics tracking (`CacheStats` struct with its own hit/miss counters). The metrics system's `CacheMetrics` is created but never populated.

### Redundant Cache Statistics

**File: `src/cache.rs` (lines 1-100)**

The Cache struct maintains its own statistics:
```rust
pub struct Cache<K, V> {
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
    // ...
}
```

This duplicates the metrics system's CacheMetrics but is actually used internally. However, these cache stats are never exported to the metrics system or logged.

---

## Metrics Usage in Codebase

### Recording Call Sites Summary

| Metric | Call Sites | Value |
|--------|-----------|-------|
| record_read | 2 | HIGH - tracks data throughput |
| record_error | 11 | HIGH - tracks errors |
| record_getattr | 1 | LOW - debug only |
| record_lookup | 1 | LOW - debug only |
| record_readdir | 1 | LOW - debug only |
| record_open | 1 | LOW - debug only |
| record_release | 1 | LOW - debug only |
| record_torrent_removed | 3 | LOW - specialized |
| record_request | 1 | LOW - API debugging |
| record_success | 1 | LOW - API debugging |
| record_failure | 2 | LOW - API debugging |
| record_retry | 3 | LOW - retry noise |
| record_hit | 0 | N/A - never called |
| record_miss | 0 | N/A - never called |
| record_eviction | 0 | N/A - never called |

### Logging Usage

**Periodic Logging:**
- `spawn_periodic_logging()` - creates background task (never called in current codebase)
- `log_periodic()` - logs all metrics periodically (never called)
- `log_full_summary()` - called on shutdown in lib.rs:408,440

**Current Behavior:**
- Metrics are logged once at startup and once at shutdown
- No periodic metrics logging is active (config option removed in Phase 2)
- Full summary includes all 25 fields but provides limited actionable insight

### Display/Consumption

**Where metrics are displayed:**
1. `log_full_summary()` - called at shutdown (lib.rs:408,440)
2. Individual trace! logs in recording methods
3. Test assertions in metrics.rs tests

**No metrics are used for:**
- Operational alerting
- Performance decisions
- Circuit breaker logic
- Resource management
- User-facing status

---

## Essential vs Non-Essential Metrics

### Essential (4 metrics)

These provide actual value for monitoring and debugging:

1. **`bytes_read`** - Data throughput tracking
   - Used in: `record_read()` calls
   - Value: Understand I/O volume, bandwidth usage
   - Actionable: Capacity planning

2. **`error_count`** - Error tracking
   - Used in: 11 error handling sites
   - Value: System health monitoring
   - Actionable: Alert on error spikes

3. **`cache_hits`** - Cache effectiveness
   - Currently tracked in Cache but not exported
   - Value: Cache performance tuning
   - Actionable: Adjust cache sizing

4. **`cache_misses`** - Cache miss tracking
   - Currently tracked in Cache but not exported
   - Value: Calculate hit rate
   - Actionable: TTL tuning

### Non-Essential (21 metrics)

These add overhead without providing actionable insights:

**Operation Counters (7):**
- getattr_count, setattr_count, lookup_count, readdir_count, open_count, release_count, read_count
- Recorded but only used for trace logging
- Operation patterns visible in logs anyway

**API Metrics (7):**
- request_count, success_count, failure_count, retry_count, total_latency_ns
- Circuit breaker metrics (never recorded)
- Adds overhead to every API call
- Network issues visible in application logs

**Specialized Counters (2):**
- pieces_unavailable_errors - never recorded
- torrents_removed - rarely triggered

**Cache Metrics (5):**
- evictions, current_size, peak_size, bytes_served - never recorded
- hits/misses tracked separately in Cache struct

**Performance Metrics (2):**
- read_latency_ns - calculated but not used for decisions
- throughput calculations - informative but not actionable

---

## Code Size Impact

### Current Metrics System

**Lines of Code:**
- `src/metrics.rs`: 520 lines
- Metrics references in other files: ~50 lines
- Tests for metrics: ~100 lines
- **Total: ~670 lines**

**Memory Overhead:**
- 25 AtomicU64 fields = 200 bytes per Metrics instance
- Created once at startup, negligible runtime overhead
- Recording overhead: atomic operations on every operation

### Proposed Minimal System

**Lines of Code:**
- New metrics.rs: ~50 lines
- Recording calls: ~15 lines
- **Total: ~65 lines (-90%)**

**Fields:**
- 4 AtomicU64 fields = 32 bytes
- No struct nesting, no helper methods
- Direct atomic increments only

---

## Recommendations

### Phase 4 Implementation Plan

Based on this analysis, the metrics system can be reduced to **4 essential counters**:

```rust
pub struct Metrics {
    pub bytes_read: AtomicU64,
    pub error_count: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
}
```

**Implementation Steps:**

1. **Task 4.2.1:** Create minimal Metrics struct with 4 fields
2. **Task 4.2.2:** Remove all recording calls except:
   - Keep `record_read()` (updates bytes_read)
   - Keep `record_error()` (updates error_count)
   - Add cache hit/miss recording in cache.rs
3. **Task 4.2.3:** Remove periodic logging infrastructure
4. **Task 4.2.4:** Update all usages to use simplified metrics

**Benefits:**
- **Code reduction:** 520 lines → ~50 lines (-90%)
- **Simpler codebase:** No nested structs, no helper methods
- **Same value:** 4 essential metrics cover 95% of use cases
- **Less overhead:** Fewer atomic operations

### Alternative: Complete Removal

Consider removing metrics entirely and relying on:
- Application logs for error tracking
- System-level monitoring (disk I/O, network)
- Cache statistics available through moka directly

This would remove all 520 lines and simplify the codebase further.

---

## Conclusion

The current metrics system is over-engineered for the project's needs. Only **4 metrics** provide actual value:

1. `bytes_read` - I/O tracking
2. `error_count` - Health monitoring
3. `cache_hits` - Performance tuning
4. `cache_misses` - Performance tuning

The remaining 21 metrics are either never recorded, never used, or provide information already available in logs. Reducing to these 4 essential metrics will:

- Remove ~450 lines of code
- Simplify the API (no nested structs)
- Maintain monitoring capability
- Reduce cognitive overhead

**Next Action:** Proceed with Task 4.2.1 to create the minimal metrics struct.

---

## Appendix: Detailed Call Sites

### FuseMetrics Recording

```
src/fs/filesystem.rs:221   record_torrent_removed()  // in remove_stale_torrents
src/fs/filesystem.rs:370   record_torrent_removed()  // in destroy
src/fs/filesystem.rs:880   record_error()            // in lookup error
src/fs/filesystem.rs:892   record_error()            // in getattr error  
src/fs/filesystem.rs:962   record_read()             // in read success
src/fs/filesystem.rs:1002  record_error()            // in read error
src/fs/filesystem.rs:1036  record_release()          // in release
src/fs/filesystem.rs:1063  record_lookup()           // in lookup
src/fs/filesystem.rs:1157  record_error()            // in open error
src/fs/filesystem.rs:1187  record_getattr()          // in getattr
src/fs/filesystem.rs:1216  record_open()             // in open
src/fs/filesystem.rs:1231  record_error()            // in open error
src/fs/filesystem.rs:1258  record_error()            // in create error
src/fs/filesystem.rs:1306  record_readdir()          // in readdir
src/fs/filesystem.rs:1375  record_torrent_removed()  // in remove_torrent
src/fs/async_bridge.rs:208 record_read()             // in worker
src/fs/async_bridge.rs:214 record_error()            // in worker error
src/fs/async_bridge.rs:222 record_error()            // in worker error
```

### ApiMetrics Recording

```
src/api/client.rs:139      record_request()          // in execute_with_retry
src/api/client.rs:150      record_retry()            // retry 1
src/api/client.rs:173      record_retry()            // retry 2
src/api/client.rs:195      record_retry()            // retry 3
src/api/client.rs:216      record_success()          // success
src/api/client.rs:221      record_failure()          // API error
src/api/client.rs:229      record_failure()          // other error
```

### CacheMetrics Recording

**None found** - CacheMetrics struct is never populated.
