# Config Fields Usage Analysis

**Date:** 2026-02-23
**Analysis of:** src/config/mod.rs
**Total Fields:** 27 across 7 config sections

## Summary

This analysis categorizes config fields as either **essential** (commonly modified) or **removable** (rarely changed, can use defaults). The goal is to reduce the configuration surface area from 27 fields to approximately 10-12 essential fields.

---

## Field-by-Field Analysis

### ApiConfig (3 fields)

| Field | Usage | Recommendation |
|-------|-------|----------------|
| `url` | Used everywhere - API endpoint | **KEEP** - Essential |
| `username` | Auth credentials | **KEEP** - Essential if using auth |
| `password` | Auth credentials | **KEEP** - Essential if using auth |

**Verdict:** All 3 fields are essential for API connectivity.

---

### CacheConfig (4 fields)

| Field | Usage | Recommendation |
|-------|-------|----------------|
| `metadata_ttl` | Cache TTL for file metadata | **KEEP** - Commonly tuned |
| `torrent_list_ttl` | Cache TTL for torrent list | **REMOVE** - Use metadata_ttl |
| `piece_ttl` | Cache TTL for pieces | **REMOVE** - Use metadata_ttl |
| `max_entries` | Max cache entries | **KEEP** - Commonly tuned |

**Analysis:**
- `torrent_list_ttl` and `piece_ttl` are used in `merge_from_env()` but provide minimal value
- Having separate TTLs adds complexity without clear benefit
- Can consolidate to single `metadata_ttl` for all cache types

**Verdict:** Reduce from 4 to 2 fields.

---

### MountConfig (5 fields)

| Field | Usage | Recommendation |
|-------|-------|----------------|
| `mount_point` | Where to mount filesystem | **KEEP** - Essential |
| `allow_other` | Allow other users access | **REMOVE** - Always use default (false) |
| `auto_unmount` | Auto unmount on exit | **REMOVE** - Always use default (true) |
| `uid` | File ownership user ID | **REMOVE** - Use current user |
| `gid` | File ownership group ID | **REMOVE** - Use current user |

**Analysis:**
- `allow_other`: Used in src/fs/filesystem.rs:791 for FUSE options, but rarely needed
- `auto_unmount`: Used in src/fs/filesystem.rs:789, but default (true) is sensible
- `uid`/`gid`: Used in src/fs/filesystem.rs:795-796 for file attrs, but current user is fine

**Verdict:** Reduce from 5 to 1 field (just `mount_point`).

---

### PerformanceConfig (5 fields)

| Field | Usage | Recommendation |
|-------|-------|----------------|
| `read_timeout` | Timeout for read operations | **KEEP** - Commonly tuned |
| `max_concurrent_reads` | Max concurrent reads | **KEEP** - Commonly tuned |
| `readahead_size` | Read-ahead buffer size | **KEEP** - Commonly tuned |
| `prefetch_enabled` | Enable prefetching | **REMOVE** - Feature doesn't work well |
| `check_pieces_before_read` | Check piece availability | **REMOVE** - Always check |

**Analysis:**
- `prefetch_enabled`: Used in src/fs/filesystem.rs:785 to conditionally prefetch, but TODO notes "feature doesn't work well"
- `check_pieces_before_read`: Used in tests to bypass checks, but should always check in production

**Verdict:** Reduce from 5 to 3 fields.

---

### MonitoringConfig (2 fields)

| Field | Usage | Recommendation |
|-------|-------|----------------|
| `status_poll_interval` | Status polling interval | **REMOVE** - Status monitoring removed in Task 1.1 |
| `stalled_timeout` | Stall detection timeout | **REMOVE** - Status monitoring removed in Task 1.1 |

**Analysis:**
- Both fields only used in `merge_from_env()` 
- Status monitoring background task was removed in Task 1.1
- These fields are now orphaned

**Verdict:** Remove entire MonitoringConfig struct.

---

### LoggingConfig (5 fields)

| Field | Usage | Recommendation |
|-------|-------|----------------|
| `level` | Log level (error/warn/info/debug/trace) | **KEEP** - Essential |
| `log_fuse_operations` | Log all FUSE ops | **REMOVE** - Always log at debug level |
| `log_api_calls` | Log all API calls | **REMOVE** - Always log at debug level |
| `metrics_enabled` | Enable metrics | **REMOVE** - Removing metrics system |
| `metrics_interval_secs` | Metrics log interval | **REMOVE** - Removing metrics system |

**Analysis:**
- `log_fuse_operations`: Used in src/fs/macros.rs (fuse_log!, fuse_error!, fuse_ok!) and src/fs/filesystem.rs:1077
- `log_api_calls`: Not actually used anywhere in current codebase
- `metrics_enabled`/`metrics_interval_secs`: Metrics system being removed in Phase 4

**Verdict:** Reduce from 5 to 1 field (just `level`).

---

### ResourceLimitsConfig (3 fields)

| Field | Usage | Recommendation |
|-------|-------|----------------|
| `max_cache_bytes` | Max cache size | **REMOVE** - Use hardcoded default |
| `max_open_streams` | Max HTTP streams | **REMOVE** - Use hardcoded default |
| `max_inodes` | Max inodes | **REMOVE** - Use hardcoded default |

**Analysis:**
- `max_inodes`: Used in src/fs/filesystem.rs:109 for InodeManager
- Other fields not actually enforced anywhere
- These are "safety limits" that add complexity without clear value

**Verdict:** Remove entire ResourceLimitsConfig struct, use hardcoded reasonable defaults.

---

## Recommended Configuration Reduction

### Current State (27 fields)
- ApiConfig: 3
- CacheConfig: 4
- MountConfig: 5
- PerformanceConfig: 5
- MonitoringConfig: 2
- LoggingConfig: 5
- ResourceLimitsConfig: 3

### Proposed State (11 fields)
- ApiConfig: 3 (url, username, password)
- CacheConfig: 2 (metadata_ttl, max_entries)
- MountConfig: 1 (mount_point)
- PerformanceConfig: 3 (read_timeout, max_concurrent_reads, readahead_size)
- MonitoringConfig: **REMOVED**
- LoggingConfig: 1 (level)
- ResourceLimitsConfig: **REMOVED**

**Reduction:** 27 fields → 11 fields (59% reduction)

---

## Environment Variables to Keep

Based on the analysis, only these env vars should be supported:

```bash
# Essential
TORRENT_FUSE_API_URL
TORRENT_FUSE_MOUNT_POINT
TORRENT_FUSE_AUTH_USERPASS (or individual TORRENT_FUSE_AUTH_USERNAME/PASSWORD)

# Commonly tuned
TORRENT_FUSE_METADATA_TTL
TORRENT_FUSE_MAX_ENTRIES
TORRENT_FUSE_READ_TIMEOUT
TORRENT_FUSE_MAX_CONCURRENT_READS
TORRENT_FUSE_READAHEAD_SIZE
TORRENT_FUSE_LOG_LEVEL
```

**Reduction:** 30+ env vars → 9 env vars (70% reduction)

---

## Implementation Notes

1. **MountConfig**: Remove `allow_other`, `auto_unmount`, `uid`, `gid` - always use sensible defaults
2. **CacheConfig**: Remove `torrent_list_ttl`, `piece_ttl` - use `metadata_ttl` for all
3. **PerformanceConfig**: Remove `prefetch_enabled`, `check_pieces_before_read` - always use sensible behavior
4. **MonitoringConfig**: Remove entire struct - status monitoring already removed
5. **LoggingConfig**: Remove `log_fuse_operations`, `log_api_calls`, `metrics_enabled`, `metrics_interval_secs`
6. **ResourceLimitsConfig**: Remove entire struct - use hardcoded defaults

---

## References

- See TODO.md Phase 2 for implementation tasks
- See src/config/mod.rs for current implementation
- See src/fs/filesystem.rs for field usage patterns

---

*Analysis completed as part of Task 2.2.1*
